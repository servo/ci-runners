use core::str;
use std::process::{Command, Stdio};

use jane_eyre::eyre::{self, Context};

use crate::SETTINGS;

pub fn list_runner_volumes() -> eyre::Result<Vec<String>> {
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
    let prefix = format!("{}/", SETTINGS.zfs_prefix);
    let result = str::from_utf8(&output.stdout)
        .wrap_err("Failed to decode UTF-8")?
        .split_terminator('\n')
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned);

    Ok(result.collect())
}
