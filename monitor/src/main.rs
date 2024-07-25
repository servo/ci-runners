mod data;
mod github;
mod id;
mod libvirt;
mod profile;
mod runner;
mod zfs;

use std::{collections::HashMap, thread::sleep, time::Duration};

use dotenv::dotenv;
use jane_eyre::eyre;
use log::{info, warn};

use crate::{
    github::list_registered_runners_for_host,
    id::IdGen,
    libvirt::list_runner_guests,
    profile::Profile,
    runner::{Runners, Status},
    zfs::list_runner_volumes,
};

fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    env_logger::init();
    dotenv().expect("Failed to load variables from .env");

    let mut profiles = HashMap::new();
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

    // profiles["servo-windows10"].create_runner(dbg!(id_gen.next()));
    // profiles["servo-windows10"].create_runner(dbg!(id_gen.next()));

    loop {
        let registrations = dbg!(list_registered_runners_for_host()?);
        let guests = dbg!(list_runner_guests()?);
        let volumes = dbg!(list_runner_volumes()?);
        info!(
            "{} registrations, {} guests, {} volumes",
            registrations.len(),
            guests.len(),
            volumes.len()
        );

        let runners = Runners::new(registrations, guests, volumes);
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
                    .map_or(true, |age| age > Duration::from_secs(300))
        });
        for (&id, runner) in invalid
            .chain(done_or_unregistered)
            .chain(started_or_crashed_and_too_old)
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

        // TODO: <https://serverfault.com/questions/523350> ?
        sleep(Duration::from_secs(10));
    }

    Ok(())
}
