use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Debug, Display},
    fs::{self, File},
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use itertools::Itertools;
use jane_eyre::eyre::{self, bail, eyre};
use mktemp::Temp;
use serde::Serialize;
use tracing::{error, info, trace, warn};

use crate::{
    auth::RemoteAddr,
    data::get_runner_data_path,
    github::{ApiGenerateJitconfigResponse, ApiRunner},
    libvirt::{get_ipv4_address, libvirt_prefix, update_screenshot},
    shell::SHELL,
    LIB_MONITOR_DIR,
};

#[derive(Debug, Serialize)]
pub struct Runners {
    runners: BTreeMap<usize, Runner>,
}

/// State of a runner and its live resources.
#[derive(Debug, Serialize)]
pub struct Runner {
    id: usize,
    created_time: SystemTime,
    registration: Option<ApiRunner>,
    guest_name: Option<String>,
    volume_name: Option<String>,
    ipv4_address: Option<Ipv4Addr>,
    github_jitconfig: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Status {
    Invalid,
    StartedOrCrashed,
    Idle,
    Reserved,
    Busy,
    DoneOrUnregistered,
}

impl Runners {
    pub fn new(
        registrations: Vec<ApiRunner>,
        guest_names: Vec<String>,
        volume_names: Vec<String>,
    ) -> Self {
        // Gather all known runner ids with live resources.
        let registration_ids = registrations
            .iter()
            .flat_map(|registration| registration.name.rsplit_once('@'))
            .flat_map(|(name, _host)| name.rsplit_once('.'))
            .flat_map(|(_, id)| id.parse())
            .collect::<Vec<usize>>();
        let guest_ids = guest_names
            .iter()
            .flat_map(|guest| guest.rsplit_once('.'))
            .flat_map(|(_, id)| id.parse())
            .collect::<Vec<usize>>();
        let volume_ids = volume_names
            .iter()
            .flat_map(|volume| volume.rsplit_once('.'))
            .flat_map(|(_, id)| id.parse())
            .collect::<Vec<usize>>();
        let ids: BTreeSet<usize> = registration_ids
            .iter()
            .copied()
            .chain(guest_ids.iter().copied())
            .chain(volume_ids.iter().copied())
            .collect();
        trace!(?ids, ?registration_ids, ?guest_ids, ?volume_ids);

        // Create a tracking object for each runner id.
        let mut runners = BTreeMap::default();
        for id in ids {
            let runner = match Runner::new(id) {
                Ok(runner) => runner,
                Err(error) => {
                    warn!(
                        runner_id = id,
                        ?error,
                        "Failed to create Runner object: {error}",
                    );
                    continue;
                }
            };
            runners.insert(id, runner);
        }

        // Populate the tracking objects with references to live resources.
        for (id, registration) in registration_ids.iter().zip(registrations) {
            if let Some(runner) = runners.get_mut(id) {
                runner.registration = Some(registration);
            }
        }
        for (id, guest_name) in guest_ids.iter().zip(guest_names) {
            if let Some(runner) = runners.get_mut(id) {
                let ipv4_address = get_ipv4_address(&guest_name);
                runner.guest_name = Some(guest_name);
                runner.ipv4_address = ipv4_address;
            }
        }
        for (id, volume_name) in volume_ids.iter().zip(volume_names) {
            if let Some(runner) = runners.get_mut(id) {
                runner.volume_name = Some(volume_name);
            }
        }

        Self { runners }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&usize, &Runner)> {
        self.runners.iter()
    }

    pub fn by_profile<'s>(&'s self, key: &'s str) -> impl Iterator<Item = (&'s usize, &'s Runner)> {
        self.runners
            .iter()
            .filter(move |(_, runner)| runner.base_vm_name() == key)
    }

    pub fn unregister_runner(&self, id: usize) -> eyre::Result<()> {
        let Some(registration) = self
            .runners
            .get(&id)
            .and_then(|runner| runner.registration())
        else {
            bail!("Tried to unregister an unregistered runner");
        };
        info!(runner_id = id, registration.id, "Unregistering runner");
        let exit_status = Command::new("./unregister-runner.sh")
            .current_dir(&*LIB_MONITOR_DIR)
            .arg(&registration.id.to_string())
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
        if exit_status.success() {
            return Ok(());
        } else {
            eyre::bail!("Command exited with status {}", exit_status);
        }
    }

    pub fn reserve_runner(
        &self,
        id: usize,
        unique_id: &str,
        qualified_repo: &str,
        run_id: &str,
    ) -> eyre::Result<()> {
        let Some(runner) = self.runners.get(&id) else {
            bail!("No runner with id exists: {id}");
        };
        let Some(registration) = runner.registration() else {
            bail!("Tried to reserve an unregistered runner");
        };
        info!(runner_id = id, registration.id, "Reserving runner");
        let exit_status = Command::new("./reserve-runner.sh")
            .current_dir(&*LIB_MONITOR_DIR)
            .arg(&registration.id.to_string())
            .arg(unique_id)
            .arg(format!("{qualified_repo}/actions/runs/{run_id}"))
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
        if exit_status.success() {
            Ok(())
        } else {
            eyre::bail!("Command exited with status {}", exit_status);
        }
    }

    pub fn screenshot_runner(&self, id: usize) -> eyre::Result<Temp> {
        let Some(runner) = self.runners.get(&id) else {
            bail!("No runner with id exists: {id}");
        };
        let Some(guest_name) = runner.guest_name.as_deref() else {
            bail!("Tried to screenshot a runner with no libvirt guest: {id}");
        };
        let result = Temp::new_file()?;
        let exit_status = SHELL
            .lock()
            .map_err(|e| eyre!("Mutex poisoned: {e:?}"))?
            .run(
                include_str!("screenshot-guest.sh"),
                [PathBuf::from(guest_name), result.clone()],
            )?
            .spawn()?
            .wait()?;
        if exit_status.success() {
            Ok(result)
        } else {
            eyre::bail!("Command exited with status {}", exit_status);
        }
    }

    pub fn update_screenshots(&self) {
        for &id in self.runners.keys() {
            if let Err(error) = self.update_screenshot(id) {
                error!(id, ?error, "Failed to update screenshot for runner");
            }
        }
    }

    fn update_screenshot(&self, id: usize) -> eyre::Result<()> {
        let Some(runner) = self.runners.get(&id) else {
            bail!("No runner with id exists: {id}");
        };
        let Some(guest_name) = runner.guest_name.as_deref() else {
            bail!("Tried to screenshot a runner with no libvirt guest: {id}");
        };
        let output_dir = get_runner_data_path(id, None)?;
        update_screenshot(guest_name, &output_dir)?;

        Ok(())
    }

    pub fn github_jitconfig(&self, remote_addr: RemoteAddr) -> eyre::Result<Option<&str>> {
        for (_id, runner) in self.runners.iter() {
            if let Some(ipv4_address) = runner.ipv4_address {
                if remote_addr == ipv4_address {
                    return Ok(runner.github_jitconfig.as_deref());
                }
            }
        }

        bail!("No runner found with IP address: {}", remote_addr)
    }

    pub fn update_ipv4_addresses(&mut self) {
        for (&id, runner) in self.runners.iter_mut() {
            if let Some(guest_name) = runner.guest_name.as_deref() {
                let ipv4_address = get_ipv4_address(guest_name);
                if ipv4_address != runner.ipv4_address {
                    info!(
                        "IPv4 address changed for runner {id}: {:?} -> {:?}",
                        runner.ipv4_address, ipv4_address
                    );
                }
                runner.ipv4_address = ipv4_address;
            }
        }
    }
}

impl Runner {
    /// Creates an object for tracking the state of a runner.
    ///
    /// For use by [`Runners::new`] only. Does not create a runner.
    fn new(id: usize) -> eyre::Result<Self> {
        let created_time = get_runner_data_path(id, Path::new("created-time"))?;
        let created_time = fs::metadata(created_time)?.modified()?;
        trace!(?created_time, "[{id}]");

        let github_jitconfig = || -> eyre::Result<String> {
            let result = get_runner_data_path(id, Path::new("github-api-registration"))?;
            let result: ApiGenerateJitconfigResponse =
                serde_json::from_reader(File::open(result)?)?;
            Ok(result.encoded_jit_config)
        };
        let github_jitconfig = match github_jitconfig() {
            Ok(result) => Some(result),
            Err(error) => {
                warn!(?error, "Failed to get GitHub jitconfig of runner");
                None
            }
        };

        Ok(Self {
            id,
            created_time,
            registration: None,
            guest_name: None,
            volume_name: None,
            ipv4_address: None,
            github_jitconfig: github_jitconfig,
        })
    }

    pub fn registration(&self) -> Option<&ApiRunner> {
        self.registration.as_ref()
    }

    pub fn log_info(&self) {
        fn fmt_option_display<T: Display>(x: Option<T>) -> String {
            x.map_or("None".to_owned(), |x| format!("{}", x))
        }
        fn fmt_option_debug<T: Debug>(x: Option<T>) -> String {
            x.map_or("None".to_owned(), |x| format!("{:?}", x))
        }
        info!(
            "[{}] profile {}, ipv4 {}, status {:?}, age {}, jitconfig {}, reserved for {}",
            self.id,
            self.base_vm_name(),
            fmt_option_display(self.ipv4_address),
            self.status(),
            fmt_option_debug(self.age().ok()),
            self.github_jitconfig.as_ref().map_or("no", |_| "yes"),
            fmt_option_debug(self.reserved_since().ok().flatten()),
        );
        if let Some(registration) = self.registration() {
            if !registration.labels.is_empty() {
                info!(
                    "[{}] - github labels: {}",
                    self.id,
                    registration.labels().join(","),
                );
            }
            if let Some(workflow_run) = registration.label_with_key("reserved-by") {
                info!(
                    "[{}] - workflow run page: https://github.com/{}",
                    self.id, workflow_run
                );
            }
        }
    }

    pub fn age(&self) -> eyre::Result<Duration> {
        Ok(self.created_time.elapsed()?)
    }

    pub fn reserved_since(&self) -> eyre::Result<Option<Duration>> {
        if let Some(registration) = &self.registration {
            if let Some(reserved_since) = registration.label_with_key("reserved-since") {
                let reserved_since = reserved_since.parse::<u64>()?;
                let reserved_since = UNIX_EPOCH + Duration::from_secs(reserved_since);
                return Ok(Some(reserved_since.elapsed()?));
            }
        }

        Ok(None)
    }

    pub fn status(&self) -> Status {
        if self.guest_name.is_none() || self.volume_name.is_none() {
            return Status::Invalid;
        };
        let Some(registration) = &self.registration else {
            return Status::DoneOrUnregistered;
        };
        if registration.busy {
            return Status::Busy;
        }
        if registration.label_with_key("reserved-for").is_some() {
            return Status::Reserved;
        }
        if registration.status == "online" {
            return Status::Idle;
        }
        return Status::StartedOrCrashed;
    }

    pub fn base_vm_name(&self) -> &str {
        self.base_vm_name_from_registration()
            .or_else(|| self.base_vm_name_from_guest_name())
            .or_else(|| self.base_vm_name_from_volume_name())
            .expect("Guaranteed by Runners::new")
    }

    fn base_vm_name_from_registration(&self) -> Option<&str> {
        self.registration
            .iter()
            .flat_map(|registration| registration.name.rsplit_once('@'))
            .flat_map(|(rest, _host)| rest.rsplit_once('.'))
            .map(|(base, _id)| base)
            .next()
    }

    fn base_vm_name_from_guest_name(&self) -> Option<&str> {
        let prefix = libvirt_prefix();
        self.guest_name
            .iter()
            .flat_map(|name| name.strip_prefix(&prefix))
            .flat_map(|name| name.rsplit_once('.'))
            .map(|(base, _id)| base)
            .next()
    }

    fn base_vm_name_from_volume_name(&self) -> Option<&str> {
        self.volume_name
            .iter()
            .flat_map(|path| path.rsplit_once('.'))
            .flat_map(|(rest, _id)| rest.rsplit_once('/'))
            .map(|(_rest, base)| base)
            .next()
    }
}
