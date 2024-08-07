use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    process::Command,
    time::{Duration, SystemTime},
};

use jane_eyre::eyre::{self, bail};
use log::{info, trace, warn};

use crate::{data::get_runner_data_path, github::ApiRunner, libvirt::libvirt_prefix};

#[derive(Debug)]
pub struct Runners {
    runners: BTreeMap<usize, Runner>,
}

/// State of a runner and its live resources.
#[derive(Debug)]
pub struct Runner {
    id: usize,
    created_time: SystemTime,
    registration: Option<ApiRunner>,
    guest_name: Option<String>,
    volume_name: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Status {
    Invalid,
    StartedOrCrashed,
    Idle,
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
        trace!("ids = {ids:?}, registration_ids = {registration_ids:?}, guest_ids = {guest_ids:?}, volume_ids = {volume_ids:?}");

        // Create a tracking object for each runner id.
        let mut runners = BTreeMap::default();
        for id in ids {
            let runner = match Runner::new(id) {
                Ok(runner) => runner,
                Err(error) => {
                    warn!("Failed to create Runner object for runner id {id}: {error}");
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
                runner.guest_name = Some(guest_name);
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

    pub fn unregister_runner(&self, id: usize) -> eyre::Result<()> {
        let Some(registration) = self
            .runners
            .get(&id)
            .and_then(|runner| runner.registration())
        else {
            bail!("Tried to unregister an unregistered runner");
        };
        info!(
            "Unregistering runner {id} with GitHub API runner id {}",
            registration.id
        );
        let exit_status = Command::new("../unregister-runner.sh")
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
}

impl Runner {
    /// Creates an object for tracking the state of a runner.
    ///
    /// For use by [`Runners::new`] only. Does not create a runner.
    fn new(id: usize) -> eyre::Result<Self> {
        let created_time = get_runner_data_path(id, "created-time")?;
        let created_time = fs::metadata(created_time)?.modified()?;
        trace!("[{id}] created_time = {created_time:?}");

        Ok(Self {
            id,
            created_time,
            registration: None,
            guest_name: None,
            volume_name: None,
        })
    }

    pub fn registration(&self) -> Option<&ApiRunner> {
        self.registration.as_ref()
    }

    pub fn log_info(&self) {
        info!(
            "[{}] status {:?}, age {:?}",
            self.id,
            self.status(),
            self.age()
        );
    }

    pub fn age(&self) -> eyre::Result<Duration> {
        Ok(self.created_time.elapsed()?)
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

pub fn start_timeout() -> u64 {
    env::var("SERVO_CI_MONITOR_START_TIMEOUT")
        .expect("SERVO_CI_MONITOR_START_TIMEOUT not defined!")
        .parse()
        .expect("Failed to parse SERVO_CI_MONITOR_START_TIMEOUT")
}
