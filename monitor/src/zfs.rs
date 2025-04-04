use core::str;
use std::{
    process::{Command, Stdio},
    time::Duration,
};

use cmd_lib::run_fun;
use jane_eyre::eyre::{self, Context};

use crate::{DOTENV, LIB_MONITOR_DIR};

pub fn list_runner_volumes() -> eyre::Result<Vec<String>> {
    let output = Command::new("./list-runner-volumes.sh")
        .current_dir(&*LIB_MONITOR_DIR)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    if !output.status.success() {
        eyre::bail!("Command exited with status {}", output.status);
    }

    // Output is already filtered by prefix, but filter again just in case.
    let prefix = format!("{}/", DOTENV.zfs_prefix);
    let result = str::from_utf8(&output.stdout)
        .wrap_err("Failed to decode UTF-8")?
        .split_terminator('\n')
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned);

    Ok(result.collect())
}

pub fn snapshot_creation_time_unix(zvol_name: &str, snapshot_name: &str) -> eyre::Result<Duration> {
    let dataset_and_snapshot = format!("{}/{zvol_name}@{snapshot_name}", DOTENV.zfs_clone_prefix);
    let result = run_fun!(zfs get -Hpo value creation $dataset_and_snapshot)?
        .parse::<u64>()
        .wrap_err("Failed to parse as u64")?;

    Ok(Duration::from_secs(result))
}
