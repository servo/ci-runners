mod data;
mod id;
mod profile;

use core::str;
use std::{
    collections::HashMap,
    env,
    process::{Command, Stdio},
};

use dotenv::dotenv;
use id::IdGen;
use jane_eyre::eyre::{self, Context};
use log::warn;
use profile::Profile;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ApiRunner {
    id: usize,
    busy: bool,
    name: String,
    os: String,
    status: String,
}

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

fn list_registered_runners() -> eyre::Result<Vec<ApiRunner>> {
    let output = Command::new("../list-registered-runners.sh")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    if output.status.success() {
        return serde_json::from_slice(&output.stdout).wrap_err("Failed to parse JSON");
    } else {
        eyre::bail!("Command exited with status {}", output.status);
    }
}

fn list_registered_runners_for_host() -> eyre::Result<Vec<ApiRunner>> {
    let suffix = format!("@{}", env::var("SERVO_CI_GITHUB_API_SUFFIX")?);
    let result = list_registered_runners()?
        .into_iter()
        .filter(|runner| runner.name.ends_with(&suffix));

    Ok(result.collect())
}

fn list_runner_guests() -> eyre::Result<Vec<String>> {
    let output = Command::new("../list-libvirt-guests.sh")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    if !output.status.success() {
        eyre::bail!("Command exited with status {}", output.status);
    }

    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = format!("{}-", env::var("SERVO_CI_LIBVIRT_PREFIX")?);
    let result = str::from_utf8(&output.stdout)
        .wrap_err("Failed to decode UTF-8")?
        .split_terminator('\n')
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned);

    Ok(result.collect())
}

fn list_runner_volumes() -> eyre::Result<Vec<String>> {
    let output = Command::new("../list-runner-volumes.sh")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    if !output.status.success() {
        eyre::bail!("Command exited with status {}", output.status);
    }

    // Output is already filtered by prefix, but filter again just in case.
    let prefix = format!("{}/", env::var("SERVO_CI_ZFS_PREFIX")?);
    let result = str::from_utf8(&output.stdout)
        .wrap_err("Failed to decode UTF-8")?
        .split_terminator('\n')
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned);

    Ok(result.collect())
}
