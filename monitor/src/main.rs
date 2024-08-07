mod data;
mod github;
mod id;
mod libvirt;
mod profile;
mod runner;
mod zfs;

use std::{collections::BTreeMap, thread::sleep, time::Duration};

use dotenv::dotenv;
use jane_eyre::eyre;
use log::{info, trace, warn};

use crate::{
    github::list_registered_runners_for_host,
    id::IdGen,
    libvirt::list_runner_guests,
    profile::{Profile, RunnerCounts},
    runner::{start_timeout, Runners, Status},
    zfs::list_runner_volumes,
};

fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    env_logger::init();
    dotenv().expect("Failed to load variables from .env");

    let mut profiles = BTreeMap::new();
    profiles.insert(
        "servo-windows10".to_owned(),
        Profile {
            configuration_name: "windows2019".to_owned(),
            base_vm_name: "servo-windows10".to_owned(),
            base_image_snapshot: "3-ready".to_owned(),
            target_count: 2,
        },
    );

    let mut id_gen = IdGen::new_load().unwrap_or_else(|error| {
        warn!("{error}");
        IdGen::new_empty()
    });

    loop {
        let registrations = list_registered_runners_for_host()?;
        let guests = list_runner_guests()?;
        let volumes = list_runner_volumes()?;
        trace!("registrations = {:?}", registrations);
        trace!("guests = {:?}", guests);
        trace!("volumes = {:?}", volumes);
        info!(
            "{} registrations, {} guests, {} volumes",
            registrations.len(),
            guests.len(),
            volumes.len()
        );

        let runners = Runners::new(registrations, guests, volumes);
        let profile_runner_counts: BTreeMap<_, _> = profiles
            .iter()
            .map(|(key, profile)| (key, profile.runner_counts(&runners)))
            .collect();
        for (
            key,
            RunnerCounts {
                target,
                healthy,
                started_or_crashed,
                idle,
                busy,
                excess_idle,
                wanted,
            },
        ) in profile_runner_counts
        {
            info!("profile {key}: {healthy}/{target} healthy runners ({busy} busy, {idle} idle, {started_or_crashed} started or crashed, {excess_idle} excess idle, {wanted} wanted)");
        }
        for (_id, runner) in runners.iter() {
            runner.log_info();
        }

        // Invalid => unregister and destroy
        // DoneOrUnregistered => destroy (no need to unregister)
        // StartedOrCrashed and too old => unregister and destroy
        // Idle or Busy => bleed off excess Idle runners
        let invalid = runners
            .iter()
            .filter(|(_id, runner)| runner.status() == Status::Invalid);
        let done_or_unregistered = runners
            .iter()
            .filter(|(_id, runner)| runner.status() == Status::DoneOrUnregistered);
        let started_or_crashed_and_too_old = runners.iter().filter(|(_id, runner)| {
            runner.status() == Status::StartedOrCrashed
                && runner
                    .age()
                    .map_or(true, |age| age > Duration::from_secs(start_timeout()))
        });
        let excess_idle_runners = profiles.iter().flat_map(|(_key, profile)| {
            profile
                .idle_runners(&runners)
                .take(profile.excess_idle_runner_count(&runners))
        });
        for (&id, runner) in invalid
            .chain(done_or_unregistered)
            .chain(started_or_crashed_and_too_old)
            .chain(excess_idle_runners)
        {
            if runner.registration().is_some() {
                if let Err(error) = runners.unregister_runner(id) {
                    warn!("Failed to unregister runner: {error}");
                }
            }
            if let Some(profile) = profiles.get(runner.base_vm_name()) {
                if let Err(error) = profile.destroy_runner(id) {
                    warn!("Failed to destroy runner: {error}");
                }
            }
        }

        let profile_wanted_counts = profiles
            .iter()
            .map(|(_key, profile)| (profile, profile.wanted_runner_count(&runners)));
        for (profile, wanted_count) in profile_wanted_counts {
            for _ in 0..wanted_count {
                if let Err(error) = profile.create_runner(id_gen.next()) {
                    warn!("Failed to create runner: {error}");
                }
            }
        }

        // TODO: <https://serverfault.com/questions/523350> ?
        sleep(Duration::from_secs(5));
    }
}
