use core::str;
use std::{
    env,
    process::{Command, Stdio},
};

use jane_eyre::eyre::{self, Context};

pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
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
    let prefix = libvirt_prefix();
    let result = str::from_utf8(&output.stdout)
        .wrap_err("Failed to decode UTF-8")?
        .split_terminator('\n')
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned);

    Ok(result.collect())
}

pub fn libvirt_prefix() -> String {
    format!(
        "{}-",
        env::var("SERVO_CI_LIBVIRT_PREFIX").expect("SERVO_CI_LIBVIRT_PREFIX not defined!")
    )
}
