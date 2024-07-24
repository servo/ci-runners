mod data;
mod github;
mod id;
mod libvirt;
mod profile;
mod zfs;

use std::collections::HashMap;

use dotenv::dotenv;
use jane_eyre::eyre;
use log::warn;

use crate::{
    github::list_registered_runners_for_host, id::IdGen, libvirt::list_runner_guests,
    profile::Profile, zfs::list_runner_volumes,
};

fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    env_logger::init();
    dotenv().expect("Failed to load variables from .env");

    let mut profiles = HashMap::new();
    profiles.insert(
        "windows10".to_owned(),
        Profile {
            configuration_name: "windows2019".to_owned(),
            base_vm_name: "servo-windows10".to_owned(),
            base_image_snapshot: "3-ready".to_owned(),
            target_count: 2,
        },
    );

    dbg!(list_registered_runners_for_host()?);
    dbg!(list_runner_guests()?);
    dbg!(list_runner_volumes()?);

    let mut id_gen = IdGen::new_load().unwrap_or_else(|error| {
        warn!("{error}");
        IdGen::new_empty()
    });

    profiles["windows10"].create_runner(dbg!(id_gen.next()));

    Ok(())
}
