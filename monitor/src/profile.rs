use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::Path,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
use jane_eyre::eyre::{self, Context};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{
    data::get_profile_data_path,
    libvirt::update_screenshot,
    runner::{Runner, Runners, Status},
    zfs::snapshot_creation_time_unix,
    DOTENV, TOML,
};

#[derive(Debug)]
pub struct Profiles {
    profiles: BTreeMap<String, Profile>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Profile {
    pub configuration_name: String,
    pub base_vm_name: String,
    pub base_image_snapshot: String,
    pub github_runner_label: String,
    pub target_count: usize,
}

#[derive(Debug, Serialize)]
pub struct RunnerCounts {
    pub target: usize,
    pub healthy: usize,
    pub started_or_crashed: usize,
    pub idle: usize,
    pub reserved: usize,
    pub busy: usize,
    pub excess_idle: usize,
    pub wanted: usize,
    pub image_age: Option<Duration>,
}

impl Profiles {
    pub fn new(mut profiles: BTreeMap<String, Profile>) -> eyre::Result<Self> {
        // When starting the monitor, check for data/profiles/<key>/base-image-snapshot,
        // and use that instead of the base_image_snapshot setting in TOML.
        for (_profile_key, profile) in profiles.iter_mut() {
            profile.read_base_image_snapshot()?;
        }

        Ok(Self { profiles })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Profile)> {
        self.profiles.iter()
    }

    pub fn get(&self, profile_key: &str) -> Option<&Profile> {
        self.profiles.get(profile_key)
    }

    pub fn get_mut(&mut self, profile_key: &str) -> Option<&mut Profile> {
        self.profiles.get_mut(profile_key)
    }
}

impl Profile {
    pub fn create_runner(&self, id: usize) -> eyre::Result<()> {
        info!(runner_id = id, self.base_vm_name, "Creating runner");
        let exit_status = Command::new("../create-runner.sh")
            .args([
                &id.to_string(),
                &self.base_vm_name,
                &self.base_image_snapshot,
                &self.configuration_name,
                &self.github_runner_label,
            ])
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

    pub fn destroy_runner(&self, id: usize) -> eyre::Result<()> {
        info!(runner_id = id, self.base_vm_name, "Destroying runner");
        let exit_status = Command::new("../destroy-runner.sh")
            .args([&self.base_vm_name, &id.to_string()])
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

    pub fn runners<'p, 'r: 'p>(
        &'p self,
        runners: &'r Runners,
    ) -> impl Iterator<Item = (&'r usize, &'r Runner)> + 'p {
        runners
            .iter()
            .filter(|(_id, runner)| runner.base_vm_name() == self.base_vm_name)
    }

    pub fn idle_runners<'p, 'r: 'p>(
        &'p self,
        runners: &'r Runners,
    ) -> impl Iterator<Item = (&'r usize, &'r Runner)> + 'p {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Idle)
    }

    pub fn runner_counts(&self, runners: &Runners) -> RunnerCounts {
        RunnerCounts {
            target: self.target_runner_count(),
            healthy: self.healthy_runner_count(runners),
            started_or_crashed: self.started_or_crashed_runner_count(runners),
            idle: self.idle_runner_count(runners),
            reserved: self.reserved_runner_count(runners),
            busy: self.busy_runner_count(runners),
            excess_idle: self.excess_idle_runner_count(runners),
            wanted: self.wanted_runner_count(runners),
            image_age: self.image_age().ok().flatten(),
        }
    }

    pub fn target_runner_count(&self) -> usize {
        if DOTENV.dont_create_runners || self.image_needs_rebuild().unwrap_or(true) {
            0
        } else {
            self.target_count
        }
    }

    pub fn healthy_runner_count(&self, runners: &Runners) -> usize {
        self.started_or_crashed_runner_count(runners)
            + self.idle_runner_count(runners)
            + self.reserved_runner_count(runners)
            + self.busy_runner_count(runners)
            + self.done_or_unregistered_runner_count(runners)
    }

    pub fn started_or_crashed_runner_count(&self, runners: &Runners) -> usize {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::StartedOrCrashed)
            .count()
    }

    pub fn idle_runner_count(&self, runners: &Runners) -> usize {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Idle)
            .count()
    }

    pub fn reserved_runner_count(&self, runners: &Runners) -> usize {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Reserved)
            .count()
    }

    pub fn busy_runner_count(&self, runners: &Runners) -> usize {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Busy)
            .count()
    }

    pub fn done_or_unregistered_runner_count(&self, runners: &Runners) -> usize {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::DoneOrUnregistered)
            .count()
    }

    pub fn excess_idle_runner_count(&self, runners: &Runners) -> usize {
        // Healthy runners beyond the target count are excess runners.
        let excess = if self.healthy_runner_count(runners) > self.target_runner_count() {
            self.healthy_runner_count(runners) - self.target_runner_count()
        } else {
            0
        };

        // But we can only destroy idle runners, since busy runners have work to do.
        excess.min(self.idle_runner_count(runners))
    }

    pub fn wanted_runner_count(&self, runners: &Runners) -> usize {
        // Healthy runners below the target count are wanted runners.
        if self.target_runner_count() > self.healthy_runner_count(runners) {
            self.target_runner_count() - self.healthy_runner_count(runners)
        } else {
            0
        }
    }

    /// Returns whether the image definitely needs to be rebuilt or not, or None
    /// if we don’t know.
    pub fn image_needs_rebuild(&self) -> Option<bool> {
        if self.target_count == 0 {
            // Profiles with zero target_count may have been set to zero because
            // there is insufficient hugepages space to run them
            return Some(false);
        }

        // If we fail to get the image age, err on the side of caution
        let image_age = match self.image_age() {
            Ok(result) => result,
            Err(error) => {
                warn!(?error, "Failed to get image age");
                return None;
            }
        };

        // If the profile has no image age, we may need to build its image for the first time
        Some(image_age.map_or(true, |age| age > TOML.base_image_max_age()))
    }

    pub fn image_age(&self) -> eyre::Result<Option<Duration>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .wrap_err("Failed to get current time")?;
        let creation_time =
            match snapshot_creation_time_unix(&self.base_vm_name, &self.base_image_snapshot) {
                Ok(result) => result,
                Err(error) => {
                    debug!(
                        self.base_vm_name,
                        ?error,
                        "Failed to get snapshot creation time"
                    );
                    return Ok(None);
                }
            };

        Ok(Some(now - creation_time))
    }

    pub fn update_screenshot(&self) {
        if let Err(error) = self.try_update_screenshot() {
            debug!(
                self.base_vm_name,
                ?error,
                "Failed to update screenshot for profile guest"
            );
        }
    }

    fn try_update_screenshot(&self) -> eyre::Result<()> {
        let output_dir = get_profile_data_path(&self.base_vm_name, None)?;
        update_screenshot(&self.base_vm_name, &output_dir)?;

        Ok(())
    }

    pub fn set_base_image_snapshot(&mut self, base_image_snapshot: &str) -> eyre::Result<()> {
        self.base_image_snapshot = base_image_snapshot.to_owned();

        let path = get_profile_data_path(&self.base_vm_name, Path::new("base-image-snapshot"))?;
        let mut file = AtomicWriteFile::open(&path)?;
        write!(file, "{base_image_snapshot}")?;
        file.commit()?;

        Ok(())
    }

    fn read_base_image_snapshot(&mut self) -> eyre::Result<()> {
        let path = get_profile_data_path(&self.base_vm_name, Path::new("base-image-snapshot"))?;
        if let Ok(mut file) = File::open(&path) {
            let mut base_image_snapshot = String::default();
            file.read_to_string(&mut base_image_snapshot)?;
            self.base_image_snapshot = base_image_snapshot;
        }

        Ok(())
    }
}
