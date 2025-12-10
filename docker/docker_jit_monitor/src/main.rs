use std::{
    process::{Child, Command},
    string::FromUtf8Error,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    thread::{self},
    time::Duration,
};
use thiserror::Error;

use anyhow::{anyhow, Context};
use clap::Parser;
use log::{error, info, warn};

use crate::github_api::spawn_runner;

mod github_api;

static RUNNER_ID: AtomicU64 = AtomicU64::new(0);
static EXITING: AtomicU32 = AtomicU32::new(0);

/// Returns the hostname or None.
fn gethostname() -> Option<String> {
    Command::new("/usr/bin/uname")
        .arg("-n")
        .output()
        .ok()
        .and_then(|output| {
            std::str::from_utf8(output.stdout.trim_ascii_end())
                .ok()
                .map(|s| s.to_owned())
        })
}

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(
        short,
        long,
        help = "Number of concurrent builder github runners on this machine",
        default_value_t = 1
    )]
    concurrent_builders: u8,
}

struct RunnerConfig {
    /// Base scope of the API, e.g. `/repos/<username>/servo`
    servo_ci_scope: String,
    name: String,
    runner_group_id: u64,
    container_type: ContainerType,
    labels: Vec<String>,
    docker_image_and_tag: String,
    work_folder: String,
    /// Map a device into the docker container
    map_device: Option<String>,
}

impl RunnerConfig {
    fn new_hos_builder(servo_ci_scope: &str) -> Self {
        RunnerConfig {
            servo_ci_scope: servo_ci_scope.to_string(),
            name: format!(
                "dresden-hos-builder.{}-{}",
                gethostname().unwrap_or_default(),
                RUNNER_ID.fetch_add(1, Ordering::Relaxed),
            ),
            runner_group_id: 1,
            labels: vec!["self-hosted".into(), OS_TAG.into(), "hos-builder".into()],
            container_type: ContainerType::Builder,
            docker_image_and_tag: "servo_gha_hos_builder:latest".into(),
            work_folder: "/data".to_string(),
            map_device: None,
        }
    }

    /// Creates a RunnerConfig for a HarmonyOS Test Runner
    fn new_hos_runner(servo_ci_scope: &str) -> Result<Self, SpawnRunnerError> {
        let tree = cyme::lsusb::profiler::get_spusb(false).map_err(|e| {
            error!("cyme get_spusb failed with: {e:?}");
            SpawnRunnerError::LsUsbError
        })?;
        let usb_devices = tree.flatten_devices();
        let hdc_devices: Vec<_> = usb_devices
            .into_iter()
            .filter(|device| device.name.to_ascii_lowercase().contains("hdc device"))
            .collect();
        info!("Found {} hdc devices!", hdc_devices.len());
        if hdc_devices.len() > 1 {
            warn!("We currently only support using the first HDC device. Any further devices will be ignored.")
        }
        let devices: Vec<_> = hdc_devices
            .into_iter()
            // todo: Check if we need to create the device location like this, or if perhaps
            // cyme offers a convenience method for it.
            .map(|device| {
                format!(
                    "/dev/bus/usb/{:03}/{:03}",
                    device.location_id.bus, device.location_id.number
                )
            })
            .collect();
        let device = devices.get(0).ok_or(SpawnRunnerError::NoHdcDeviceFound)?;

        Ok(RunnerConfig {
            servo_ci_scope: servo_ci_scope.to_string(),
            name: format!(
                "dresden-hos-runner.{}-{}",
                gethostname().unwrap_or_default(),
                RUNNER_ID.fetch_add(1, Ordering::Relaxed)
            ),
            runner_group_id: 1,
            labels: vec!["self-hosted".into(), OS_TAG.into(), "hos-runner".into()],
            container_type: ContainerType::Runner,
            docker_image_and_tag: "servo_gha_hos_runner:latest".into(),
            work_folder: "/data".to_string(),
            map_device: Some(device.clone()),
        })
    }
}

#[derive(Error, Debug)]
enum SpawnRunnerError {
    #[error("IoError when invoking `gh api`: `{0:?}`")]
    SpawnGhError(std::io::Error),
    #[error("`gh api` exited with `{0}`. stderr: `{1:?}`")]
    GhApiError(i32, String),
    #[error("Invalid String: {0:?}")]
    InvalidUtf8(#[from] FromUtf8Error),
    #[error("Invalid JSON: {0:?}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("Couldn't find the encoded JIT config in the github api response")]
    EncodedJitConfigNotFound,
    #[error("Failed to spawn docker with IoError: `{0:?}`")]
    SpawnDockerError(std::io::Error),
    #[error("Couldn't find any hdc devices")]
    NoHdcDeviceFound,
    #[error("Failed to list USB devices")]
    LsUsbError,
}

// Note: For now we assume linux x64. Compilation will fail on other platforms to remind us of that.
#[cfg(target_os = "linux")]
const OS_TAG: &str = "Linux";

#[derive(Clone, Debug, PartialEq)]
enum ContainerType {
    Builder,
    Runner,
}

impl ContainerType {
    /// This iterator will go from Builder -> Runner and then stop.
    fn iter() -> ContainerTypeIterator {
        ContainerTypeIterator {
            current: None,
            finished: false,
        }
    }

    /// The number of concurrent instances we allow for this container type
    fn concurrent_number(&self, args: &Args) -> usize {
        match self {
            ContainerType::Builder => args.concurrent_builders as usize,
            ContainerType::Runner => 1,
        }
    }
}

struct ContainerTypeIterator {
    current: Option<ContainerType>,
    finished: bool,
}

impl Iterator for ContainerTypeIterator {
    type Item = ContainerType;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        self.current = match self.current {
            None => Some(ContainerType::Builder),
            Some(ContainerType::Builder) => Some(ContainerType::Runner),
            Some(ContainerType::Runner) => {
                self.finished = true;
                None
            }
        };
        self.current.clone()
    }
}

#[test]
fn iter_test() {
    let mut it = ContainerType::iter();
    assert_eq!(it.next(), Some(ContainerType::Builder));
    assert_eq!(it.next(), Some(ContainerType::Runner));
    assert_eq!(it.next(), None);
}

struct DockerContainer {
    #[allow(unused)]
    name: String,
    process: Child,
    container_type: ContainerType,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    info!("Starting monitor for selfhosted docker-based github runners!");

    let args = Args::parse();

    let servo_ci_scope = std::env::var("SERVO_CI_GITHUB_API_SCOPE")
        .context("SERVO_CI_GITHUB_API_SCOPE must be set.")?;

    // First Ctrl+c: Stop adding new servers, and gracefully exit once all currently running jobs have finished
    // Second Ctrl+c: Try killing currently running child processes and then exiting
    // Third Ctrl+c: Exit immediatly.
    ctrlc::set_handler(|| {
        let prev = EXITING.fetch_add(1, Ordering::Release);
        if prev == 2 {
            std::process::exit(-1);
        } else {
            println!("Will exit when all runners have stopped");
        }
    })
    .context("Failed to setup signal handler")?;

    let mut running_containers: Vec<DockerContainer> = vec![];
    // Todo: implement something to reserve devices for the duration of the docker run child process.

    loop {
        let exiting = EXITING.load(Ordering::Relaxed);
        for container_type in ContainerType::iter() {
            if running_containers
                .iter()
                .filter(|container| container.container_type == container_type)
                .count()
                < container_type.concurrent_number(&args)
                && exiting == 0
            {
                let config = match container_type {
                    ContainerType::Builder => RunnerConfig::new_hos_builder(&servo_ci_scope),
                    ContainerType::Runner => match RunnerConfig::new_hos_runner(&servo_ci_scope) {
                        Ok(config) => config,
                        Err(e) => {
                            error!("Failed to do runner config ({e:?})");
                            break;
                        }
                    },
                };

                match spawn_runner(config) {
                    Ok(container) => running_containers.push(container),
                    Err(SpawnRunnerError::GhApiError(_, message))
                        if message.contains("gh: Already exists") =>
                    {
                        info!("Runner name already taken - Will retry with new name later")
                    }
                    Err(e) => {
                        error!("Failed to spawn JIT runner: {e:?}");
                    }
                }
            }
        }

        let mut still_running = vec![];
        for mut container in running_containers {
            match container.process.try_wait() {
                Ok(Some(_exit_status)) => {}
                Ok(None) => still_running.push(container),
                Err(e) => {
                    error!("Failed to check the exit status of hos-container: {e:?}");
                }
            }
        }

        if still_running.is_empty() && EXITING.load(Ordering::Relaxed) > 0 {
            break;
        } else if EXITING.load(Ordering::Relaxed) >= 2 {
            for mut container in still_running {
                if let Err(e) = container.process.kill() {
                    warn!("Failed to kill process due to {e:?}");
                    error!("Failed to kill some processes. Check for zombie processes.")
                }
            }
            return Err(anyhow!("Exiting after receiving multiple Ctrl+c"));
        }

        running_containers = still_running;
        thread::sleep(Duration::from_millis(500));
    }

    info!("Exiting....");
    Ok(())
}
