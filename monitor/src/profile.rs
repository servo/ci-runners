use std::{collections::BTreeMap, process::Command};

use jane_eyre::eyre;
use log::info;
use serde::Serialize;

use crate::{
    runner::{Runner, Runners, Status},
    SETTINGS,
};

#[derive(Debug, Default)]
pub struct Profiles {
    inner: BTreeMap<String, Profile>,
}

#[derive(Debug)]
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
}

impl Profiles {
    pub fn insert(&mut self, key: impl AsRef<str>, profile: Profile) {
        assert_eq!(key.as_ref(), profile.base_vm_name, "Runner::base_vm_name relies on Profiles key (profile name) and base_vm_name being equal");
        self.inner.insert(key.as_ref().to_owned(), profile);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Profile)> {
        self.inner.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn get(&self, key: impl AsRef<str>) -> Option<&Profile> {
        self.inner.get(key.as_ref())
    }
}

impl Profile {
    pub fn create_runner(&self, id: usize) -> eyre::Result<()> {
        info!(
            "Creating runner {id} with base vm name {}",
            self.base_vm_name
        );
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
        info!(
            "Destroying runner {id} with base vm name {}",
            self.base_vm_name
        );
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
        }
    }

    pub fn target_runner_count(&self) -> usize {
        if SETTINGS.dont_create_runners {
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
}
