mod data;
mod github;
mod id;
mod libvirt;
mod profile;
mod runner;
mod settings;
mod zfs;

use std::{collections::BTreeMap, sync::LazyLock, thread::sleep};

use dotenv::dotenv;
use jane_eyre::eyre;
use log::{info, trace, warn};

use crate::{
    github::{list_registered_runners_for_host, Cache},
    id::IdGen,
    libvirt::list_runner_guests,
    profile::{Profile, Profiles, RunnerCounts},
    runner::{Runners, Status},
    settings::Settings,
    zfs::list_runner_volumes,
};

static SETTINGS: LazyLock<Settings> = LazyLock::new(|| {
    dotenv().expect("Failed to load variables from .env");
    Settings::load()
});

fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    env_logger::init();

    let mut profiles = Profiles::default();
    profiles.insert(
        "servo-windows10",
        Profile {
            configuration_name: "windows10".to_owned(),
            base_vm_name: "servo-windows10".to_owned(),
            base_image_snapshot: "ready".to_owned(),
            github_runner_label: "self-hosted-image:windows10".to_owned(),
            target_count: 2,
        },
    );
    profiles.insert(
        "servo-windows10.new",
        Profile {
            configuration_name: "windows10".to_owned(),
            base_vm_name: "servo-windows10.new".to_owned(),
            base_image_snapshot: "ready".to_owned(),
            github_runner_label: "self-hosted-image:windows10.new".to_owned(),
            target_count: 0,
        },
    );
    profiles.insert(
        "servo-ubuntu2204",
        Profile {
            configuration_name: "ubuntu2204".to_owned(),
            base_vm_name: "servo-ubuntu2204".to_owned(),
            base_image_snapshot: "ready".to_owned(),
            github_runner_label: "self-hosted-image:ubuntu2204".to_owned(),
            target_count: 1,
        },
    );
    profiles.insert(
        "servo-ubuntu2204.new",
        Profile {
            configuration_name: "ubuntu2204".to_owned(),
            base_vm_name: "servo-ubuntu2204.new".to_owned(),
            base_image_snapshot: "ready".to_owned(),
            github_runner_label: "self-hosted-image:ubuntu2204.new".to_owned(),
            target_count: 0,
        },
    );

    let mut id_gen = IdGen::new_load().unwrap_or_else(|error| {
        warn!("{error}");
        IdGen::new_empty()
    });

    let mut registrations_cache = Cache::default();

    loop {
        let registrations = registrations_cache.get(|| list_registered_runners_for_host())?;
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
        // Reserved for too long => unregister and destroy
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
                    .map_or(true, |age| age > SETTINGS.monitor_start_timeout)
        });
        let reserved_for_too_long = runners.iter().filter(|(_id, runner)| {
            runner.status() == Status::Reserved
                && runner
                    .reserved_since()
                    .ok()
                    .flatten()
                    .map_or(true, |duration| duration > SETTINGS.monitor_reserve_timeout)
        });
        let excess_idle_runners = profiles.iter().flat_map(|(_key, profile)| {
            profile
                .idle_runners(&runners)
                .take(profile.excess_idle_runner_count(&runners))
        });
        for (&id, runner) in invalid
            .chain(done_or_unregistered)
            .chain(started_or_crashed_and_too_old)
            .chain(reserved_for_too_long)
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
            registrations_cache.invalidate();
        }

        let profile_wanted_counts = profiles
            .iter()
            .map(|(_key, profile)| (profile, profile.wanted_runner_count(&runners)));
        for (profile, wanted_count) in profile_wanted_counts {
            for _ in 0..wanted_count {
                if let Err(error) = profile.create_runner(id_gen.next()) {
                    warn!("Failed to create runner: {error}");
                }
                registrations_cache.invalidate();
            }
        }

        // TODO: <https://serverfault.com/questions/523350> ?
        sleep(SETTINGS.monitor_poll_interval);
    }
}
