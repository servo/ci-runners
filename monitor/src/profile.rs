use std::process::Command;

use jane_eyre::eyre;
use log::info;

use crate::runner::{Runner, Runners, Status};

pub struct Profile {
    pub configuration_name: String,
    pub base_vm_name: String,
    pub base_image_snapshot: String,
    pub target_count: usize,
}

pub struct RunnerCounts {
    pub target: usize,
    pub healthy: usize,
    pub idle: usize,
    pub busy: usize,
    pub excess_idle: usize,
}

impl Profile {
    pub fn create_runner(&self, id: usize) {
        info!(
            "Creating runner {id} with base vm name {}",
            self.base_vm_name
        );
        Command::new("../create-runner.sh")
            .args([
                &id.to_string(),
                &self.base_vm_name,
                &self.base_image_snapshot,
                &self.configuration_name,
            ])
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
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

    pub fn runner_counts(&self, runners: &Runners) -> RunnerCounts {
        RunnerCounts {
            target: self.target_runner_count(),
            healthy: self.healthy_runner_count(runners),
            idle: self.idle_runner_count(runners),
            busy: self.busy_runner_count(runners),
            excess_idle: self.excess_idle_runner_count(runners),
        }
    }

    pub fn target_runner_count(&self) -> usize {
        self.target_count
    }

    pub fn healthy_runner_count(&self, runners: &Runners) -> usize {
        self.idle_runner_count(runners) + self.busy_runner_count(runners)
    }

    pub fn idle_runner_count(&self, runners: &Runners) -> usize {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Idle)
            .count()
    }

    pub fn busy_runner_count(&self, runners: &Runners) -> usize {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Busy)
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
}
