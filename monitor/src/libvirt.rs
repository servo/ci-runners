use core::str;
use std::{
    fs::{create_dir_all, rename},
    path::Path,
    process::{Command, Stdio},
};

use jane_eyre::eyre::{self, eyre, Context};

use crate::{shell::SHELL, DOTENV};

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
    format!("{}-", DOTENV.libvirt_prefix)
}

pub fn update_screenshot(guest_name: &str, output_dir: &Path) -> Result<(), eyre::Error> {
    create_dir_all(output_dir)?;
    let new_path = output_dir.join("screenshot.png.new");
    let exit_status = SHELL
        .lock()
        .map_err(|e| eyre!("Mutex poisoned: {e:?}"))?
        .run(
            include_str!("screenshot-guest.sh"),
            [Path::new(guest_name), &new_path],
        )?
        .spawn()?
        .wait()?;
    if !exit_status.success() {
        eyre::bail!("Command exited with status {}", exit_status);
    }
    let path = output_dir.join("screenshot.png");
    rename(new_path, path)?;

    Ok(())
}
