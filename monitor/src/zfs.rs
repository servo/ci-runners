use core::str;
use std::{
    process::{Command, Stdio},
    time::Duration,
};

use jane_eyre::eyre::{self, bail, eyre, Context, OptionExt};

use crate::{shell::SHELL, DOTENV};

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
    let output = SHELL
        .lock()
        .map_err(|e| eyre!("Mutex poisoned: {e:?}"))?
        .run(
            include_str!("get-snapshot-creation.sh"),
            [dataset_and_snapshot],
        )?
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    if !output.status.success() {
        let stdout = str::from_utf8(&output.stdout)
            .to_owned()
            .map_err(|_| output.stdout.clone());
        let stderr = str::from_utf8(&output.stderr)
            .to_owned()
            .map_err(|_| output.stderr.clone());
        bail!(
            "Command exited with status {}: stdout {:?}, stderr {:?}",
            output.status,
            stdout,
            stderr
        );
    }
    let result = str::from_utf8(&output.stdout)
        .wrap_err("Failed to decode UTF-8")?
        .strip_suffix('\n')
        .ok_or_eyre("Failed to strip trailing newline")?
        .parse::<u64>()
        .wrap_err("Failed to parse as u64")?;

    Ok(Duration::from_secs(result))
}
