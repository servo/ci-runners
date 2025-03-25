use core::str;
use std::{
    fs::{create_dir_all, rename},
    path::Path,
};

use cmd_lib::run_fun;
use jane_eyre::eyre::{self, eyre};

use crate::{shell::SHELL, DOTENV};

pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = libvirt_prefix();
    let result = run_fun!(virsh list --name --all)?;
    let result = result
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
