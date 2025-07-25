use serde::Deserialize;
use serde_json::Value;
use std::{
    process::{self, Command, Output},
    string::FromUtf8Error,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    thread,
    time::Duration,
};
use thiserror::Error;

use anyhow::{anyhow, Context};
use clap::Parser;
use log::{debug, error, info, warn};
static RUNNER_ID: AtomicU64 = AtomicU64::new(0);
static EXITING: AtomicU32 = AtomicU32::new(0);
const MAX_SPAWN_RETRIES: u32 = 10;
/// The final builder name will be {BUILDER_NAME}.{RUNNER_SUFFIX_ENV}.{RUNNER_ID}, same for RUNNER
const BUILDER_NAME: &str = "dresden-hos-builder";
const RUNNER_NAME: &str = "dresden-hos-runner";
const RUNNER_SUFFIX_ENV: &str = "RUNNER_SUFFIX";
/// How long the loop will sleep.
const LOOP_SLEEP: u64 = 30;

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
                "{}.{}.{}",
                BUILDER_NAME,
                std::env::var(RUNNER_SUFFIX_ENV).unwrap_or_default(),
                RUNNER_ID.fetch_add(1, Ordering::Relaxed),
            ),
            runner_group_id: 1,
            labels: vec!["self-hosted".into(), OS_TAG.into(), "hos-builder".into()],
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
        let device = devices.first().ok_or(SpawnRunnerError::NoHdcDeviceFound)?;

        Ok(RunnerConfig {
            servo_ci_scope: servo_ci_scope.to_string(),
            name: format!(
                "{}.{}.{}",
                RUNNER_NAME,
                std::env::var(RUNNER_SUFFIX_ENV).unwrap_or_default(),
                RUNNER_ID.fetch_add(1, Ordering::Relaxed)
            ),
            runner_group_id: 1,
            labels: vec!["self-hosted".into(), OS_TAG.into(), "hos-runner".into()],
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
    #[error("Failed to deserialize list runners api")]
    ListRunnersDeserialize(serde_json::Error),
}

/// Function to call the api. Raw just is used spawnrunner.
/// This gives you the _executed_ cmd.
/// Notice that the api_endpoint needs a slash before it. The api is very peculious
/// with slashes and this is the easiest
/// Note: octocrab apparently requires more coarse grained tokens compared
/// to `gh`, so we use `gh`.
fn call_github_runner_api(
    ci_scope: &str,
    method: &str,
    api_endpoint: &str,
    raw: Option<&RunnerConfig>,
) -> Result<Output, SpawnRunnerError> {
    let mut cmd = Command::new("gh");
    let api_endpoint = format!("{ci_scope}/actions/runners{api_endpoint}");
    cmd.args([
        "api",
        "--method",
        method,
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "X-GitHub-Api-Version: 2022-11-28",
        &api_endpoint,
    ]);
    if let Some(config) = raw {
        for label in &config.labels {
            cmd.arg("--raw-field").arg(format!("labels[]={label}"));
        }
        cmd.arg("--raw-field")
            // Todo: perhaps add information if it has a device or not
            .arg(format!("name={}", config.name))
            .arg("--raw-field")
            .arg(format!("work_folder={}", config.work_folder))
            .arg("--field")
            .arg(format!("runner_group_id={}", config.runner_group_id));
    }

    let output = cmd.output().map_err(SpawnRunnerError::SpawnGhError)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(SpawnRunnerError::GhApiError(
            output.status.code().unwrap_or(-1),
            stderr,
        ));
    }
    Ok(output)
}

// todo: add arg for optional device to pass into the runner
fn spawn_runner(config: &RunnerConfig) -> Result<process::Child, SpawnRunnerError> {
    let output = call_github_runner_api(
        &config.servo_ci_scope,
        "POST",
        "/generate-jitconfig",
        Some(config),
    )?;

    let registration_info = String::from_utf8(output.stdout)?;
    let registration_info: Value = serde_json::from_str(&registration_info)?;
    let Some(encoded_jit_config) = &registration_info
        .get("encoded_jit_config")
        .and_then(|v| v.as_str())
    else {
        return Err(SpawnRunnerError::EncodedJitConfigNotFound);
    };
    if let Some(id) = registration_info.get("runner").and_then(|v| v.get("id")) {
        debug!("The GitHub runner id: is {id} ");
    } else {
        warn!("Couldn't find runner.id in the GitHub API answer");
    }

    let mut cmd = std::process::Command::new("docker");
    cmd.arg("run").arg("--rm");

    if let Some(device) = &config.map_device {
        cmd.arg("--device").arg(device);
    }

    // Start the gh runner inside the container
    cmd.arg(&config.docker_image_and_tag)
        .arg("/home/servo_ci/runner/run.sh")
        .arg(" --jitconfig")
        .arg(encoded_jit_config);

    let runner = cmd.spawn().map_err(SpawnRunnerError::SpawnDockerError)?;
    Ok(runner)
}

/// Response of github api runner list
#[allow(unused)]
#[derive(Debug, Deserialize)]
struct ListRunnersResponse {
    total_count: usize,
    runners: Vec<GithubRunner>,
}

/// A github runner from the api
#[allow(unused)]
#[derive(Debug, Deserialize)]
struct GithubRunner {
    id: u64,
    name: String,
    os: String,
    status: String,
    busy: bool,
}

/// Deregisters and kills runners that are offline according to gh api.
/// This does not kill any docker containers.
/// Notice that the api endpoint might need a slash
fn kill_offline_runners(servo_ci_scope: &str) -> Result<(), SpawnRunnerError> {
    let output = call_github_runner_api(servo_ci_scope, "GET", "", None)?;
    let runner_response: ListRunnersResponse =
        serde_json::from_slice(&output.stdout).map_err(SpawnRunnerError::ListRunnersDeserialize)?;

    info!("All runners {runner_response:?}");
    let filtered_response = runner_response
        .runners
        .iter()
        .filter(|runner| runner.status.contains("offline"))
        .filter(|runner| {
            runner.name.contains(&format!(
                "{}.{}",
                RUNNER_NAME,
                std::env::var(RUNNER_SUFFIX_ENV).unwrap_or_default()
            )) || runner.name.contains(&format!(
                "{}.{}",
                BUILDER_NAME,
                std::env::var(RUNNER_SUFFIX_ENV).unwrap_or_default()
            ))
        });

    for i in filtered_response {
        info!(
            "Trying to stop container {} with id {} and status {}",
            i.name, i.id, i.status
        );
        call_github_runner_api(
            servo_ci_scope,
            "DELETE",
            &(String::from("/") + &i.id.to_string()),
            None,
        )?;
    }
    Ok(())
}

// Note: For now we assume linux x64. Compilation will fail on other platforms to remind us of that.
#[cfg(target_os = "linux")]
const OS_TAG: &str = "Linux";

fn check_and_inc_retries(retries: &mut u32) {
    *retries += 1;
    if *retries > MAX_SPAWN_RETRIES {
        println!("We had {retries} many times to spawn a runner/builder. It is not happening.");
        std::process::exit(-1);
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    info!("Starting monitor for selfhosted docker-based github runners!");

    let args = Args::parse();
    println!("{args:?}");

    let servo_ci_scope = std::env::var("SERVO_CI_GITHUB_API_SCOPE")
        .context("SERVO_CI_GITHUB_API_SCOPE must be set.")?;

    // First Ctrl+c: Stop adding new servers, and gracefully exit once all currently running jobs have finished
    // Second Ctrl+c: Try killing currently running child processes and then exiting
    // Third Ctrl+c: Exit immediatly.
    ctrlc::set_handler(|| {
        let prev = EXITING.fetch_add(1, Ordering::Release);
        if prev == 2 {
            std::process::exit(-1);
        }
    })
    .context("Failed to setup signal handler")?;

    let mut running_hos_builders = vec![];
    let mut running_hos_runners = vec![];
    // Todo: implement something to reserve devices for the duration of the docker run child process.
    const MAX_HOS_RUNNERS: usize = 1;
    let mut retries_builder = 0;
    let mut retries_runner = 0;

    loop {
        let exiting = EXITING.load(Ordering::Relaxed);
        if running_hos_builders.len() < args.concurrent_builders.into() && exiting == 0 {
            match spawn_runner(&RunnerConfig::new_hos_builder(&servo_ci_scope)) {
                Ok(child) => {
                    retries_builder = 0;
                    running_hos_builders.push(child)
                }
                Err(SpawnRunnerError::GhApiError(_, message))
                    if message.contains("gh: Already exists") =>
                {
                    // Might happen if containers were not killed properly after a forced exit.
                    info!("Runner name already taken - Will retry with new name later.");
                    check_and_inc_retries(&mut retries_builder);
                }
                Err(e) => {
                    error!("Failed to spawn JIT runner: {e:?}");
                    check_and_inc_retries(&mut retries_builder);
                }
            };
        }
        if running_hos_runners.len() < MAX_HOS_RUNNERS && exiting == 0 {
            match RunnerConfig::new_hos_runner(&servo_ci_scope).and_then(|cfg| spawn_runner(&cfg)) {
                Ok(child) => {
                    retries_runner = 0;
                    running_hos_runners.push(child)
                }
                Err(SpawnRunnerError::GhApiError(_, message))
                    if message.contains("gh: Already exists") =>
                {
                    // Might happen if containers were not killed properly after a forced exit.
                    info!("Runner name already taken - Will retry with new name later.");
                    check_and_inc_retries(&mut retries_runner);
                }
                Err(e) => {
                    error!("Failed to spawn JIT runner with HOS device: {e:?}");
                    check_and_inc_retries(&mut retries_runner);
                }
            };
        }
        let mut still_running = vec![];
        for mut builder in running_hos_builders {
            match builder.try_wait() {
                Ok(Some(exit_status)) => {
                    debug!("hos-builder finished with {exit_status:?}")
                }
                Ok(None) => still_running.push(builder),
                Err(e) => {
                    error!("Failed to check the exit status of hos-builder process: {e:?}");
                    // lets just forget about this builder for now.
                }
            }
        }
        running_hos_builders = still_running;

        let mut still_running = vec![];
        for mut builder in running_hos_runners {
            match builder.try_wait() {
                Ok(Some(exit_status)) => {
                    debug!("hos-runner finished with {exit_status:?}")
                }
                Ok(None) => still_running.push(builder),
                Err(e) => {
                    error!("Failed to check the exit status of hos-builder process: {e:?}");
                    // lets just forget about this builder for now.
                }
            }
        }

        running_hos_runners = still_running;

        if running_hos_builders.is_empty()
            && running_hos_runners.is_empty()
            && EXITING.load(Ordering::Relaxed) > 0
        {
            break;
        } else if EXITING.load(Ordering::Relaxed) >= 2 {
            let mut failed_count = 0;
            for mut builder in running_hos_builders.into_iter().chain(running_hos_runners) {
                if let Err(e) = builder.kill() {
                    warn!("Failed to kill process due to {e:?}");
                    failed_count += 1;
                }
            }
            if failed_count > 0 {
                error!("Failed to kill {failed_count} builders. Check for zombie processes.");
            }
            return Err(anyhow!("Exiting after receiving multiple Ctrl+c"));
        } else if running_hos_builders.len() >= args.concurrent_builders.into() {
            // Limit our spinning if we anyway wouldn't have capacity for a new builder.
            thread::sleep(Duration::from_millis(500));
        }

        thread::sleep(Duration::from_secs(LOOP_SLEEP));
        // Check if some still running images are listed as offline from github api point of view
        if let Err(e) = kill_offline_runners(&servo_ci_scope) {
            error!("Killing offline runners failed with {e:?}");
        }
    }

    info!("Exiting....");
    Ok(())
}
