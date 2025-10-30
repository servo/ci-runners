use core::str;
use std::{
    collections::BTreeSet,
    fs::{create_dir_all, read_dir, rename},
    net::Ipv4Addr,
    path::Path,
    time::Duration,
};

use cmd_lib::{run_cmd, run_fun, spawn_with_output};
use jane_eyre::eyre::{self, OptionExt, bail};
use settings::{TOML, profile::Profile};
use shell::log_output_as_trace;
use tracing::{debug, info};

use crate::libvirt::{delete_template_or_rebuild_image_file, template_or_rebuild_images_path};

pub fn initialise() -> eyre::Result<()> {
    // Do nothing (not applicable to libvirt)
    Ok(())
}

pub fn handle_main_thread_request() -> eyre::Result<()> {
    // Do nothing (not applicable to libvirt)
    Ok(())
}

pub fn list_template_guests() -> eyre::Result<Vec<String>> {
    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = format!("{}-", TOML.libvirt_template_guest_prefix());
    let result = run_fun!(virsh list --name --all)?;
    let result = result
        .split_terminator('\n')
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned);

    Ok(result.collect())
}

pub fn list_rebuild_guests() -> eyre::Result<Vec<String>> {
    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = format!("{}-", TOML.libvirt_rebuild_guest_prefix());
    let result = run_fun!(virsh list --name --all)?;
    let result = result
        .split_terminator('\n')
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned);

    Ok(result.collect())
}

pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
    // Output is not filtered by prefix, so we must filter it ourselves.
    let prefix = format!("{}-", TOML.libvirt_runner_guest_prefix());
    let result = run_fun!(virsh list --name --all)?;
    let result = result
        .split_terminator('\n')
        .filter(|name| name.starts_with(&prefix))
        .map(str::to_owned);

    Ok(result.collect())
}

pub fn update_screenshot(guest_name: &str, output_dir: &Path) -> Result<(), eyre::Error> {
    create_dir_all(output_dir)?;
    let new_path = output_dir.join("screenshot.png.new");
    take_screenshot(guest_name, &new_path)?;
    let path = output_dir.join("screenshot.png");
    rename(new_path, path)?;

    Ok(())
}

pub fn take_screenshot(guest_name: &str, output_path: &Path) -> Result<(), eyre::Error> {
    // Squelch errors due to guests being shut off
    let pipe = || |reader| log_output_as_trace(reader);
    spawn_with_output!(virsh screenshot -- $guest_name $output_path 2>&1)?
        .wait_with_pipe(&mut pipe())?;
    Ok(())
}

pub fn get_ipv4_address(guest_name: &str) -> Option<Ipv4Addr> {
    virsh_domifaddr(guest_name, "lease")
        .or_else(|| virsh_domifaddr(guest_name, "arp"))
        .or_else(|| virsh_domifaddr(guest_name, "agent"))
}

pub fn start_guest(guest_name: &str) -> eyre::Result<()> {
    info!(?guest_name, "Starting guest");
    run_cmd!(virsh start -- $guest_name)?;

    Ok(())
}

pub fn wait_for_guest(guest_name: &str, timeout: Duration) -> eyre::Result<()> {
    let timeout = timeout.as_secs();
    info!("Waiting for guest to shut down (max {timeout} seconds)");
    if !run_cmd!(time virsh event --timeout $timeout -- $guest_name lifecycle).is_ok() {
        bail!("`virsh event` failed or timed out!");
    }
    for _ in 0..100 {
        if run_fun!(virsh domstate -- $guest_name)?.trim_ascii() == "shut off" {
            return Ok(());
        }
    }

    bail!("Guest did not shut down as expected")
}

pub fn rename_guest(old_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    run_cmd!(virsh domrename -- $old_guest_name $new_guest_name)?;
    Ok(())
}

pub fn delete_guest(guest_name: &str) -> eyre::Result<()> {
    if run_cmd!(virsh domstate -- $guest_name).is_ok() {
        // FIXME make this idempotent in a less noisy way?
        let _ = run_cmd!(virsh destroy -- $guest_name);
        run_cmd!(virsh undefine --nvram -- $guest_name)?;
    }

    Ok(())
}

pub fn prune_base_image_files(
    profile: &Profile,
    keep_snapshots: BTreeSet<String>,
) -> eyre::Result<()> {
    let base_images_path = template_or_rebuild_images_path(profile);
    info!(?base_images_path, "Pruning base image files");
    create_dir_all(&base_images_path)?;

    for entry in read_dir(&base_images_path)? {
        let filename = entry?.file_name();
        let filename = filename.to_str().ok_or_eyre("Unsupported path")?;
        if let Some((_base, snapshot_name)) = filename.split_once("@") {
            if !keep_snapshots.contains(snapshot_name) {
                delete_template_or_rebuild_image_file(profile, filename);
            }
        } else {
            delete_template_or_rebuild_image_file(profile, filename);
        }
    }

    Ok(())
}

fn virsh_domifaddr(guest_name: &str, source: &str) -> Option<Ipv4Addr> {
    let output = run_fun!(virsh domifaddr --source $source $guest_name 2> /dev/null);
    match output {
        Ok(output) => parse_virsh_domifaddr_output(&output),
        Err(error) => {
            debug!(?error, "Failed to get IPv4 address of guest");
            None
        }
    }
}

fn parse_virsh_domifaddr_output(output: &str) -> Option<Ipv4Addr> {
    for row in output.lines().skip(2) {
        let address_with_subnet = row.split_ascii_whitespace().nth(3)?;
        let (address, _subnet) = address_with_subnet.split_once('/')?;
        if address.starts_with("192.168.100.") {
            if let Ok(result) = address.parse::<Ipv4Addr>() {
                return Some(result);
            }
        }
    }

    None
}

#[test]
fn test_parse_virsh_domifaddr_output() {
    use std::str::FromStr;
    // `--source lease` case
    assert_eq!(
        parse_virsh_domifaddr_output(
            r" Name       MAC address          Protocol     Address
-------------------------------------------------------------------------------
 vnet6130   52:54:00:1c:1f:5e    ipv4         192.168.100.195/24"
        ),
        Some(Ipv4Addr::from_str("192.168.100.195").expect("Guaranteed by argument"))
    );
    // `--source arp` case
    assert_eq!(
        parse_virsh_domifaddr_output(
            r" Name       MAC address          Protocol     Address
-------------------------------------------------------------------------------
 vnet91     52:54:00:95:5e:68    ipv4         192.168.100.189/0"
        ),
        Some(Ipv4Addr::from_str("192.168.100.189").expect("Guaranteed by argument"))
    );
    // `--source agent` case
    assert_eq!(
        parse_virsh_domifaddr_output(
            r" Name       MAC address          Protocol     Address
-------------------------------------------------------------------------------
 lo0        0:0:0:0:0:0          ipv4         127.0.0.1/8
 -          -                    ipv6         ::1/128
 -          -                    ipv6         fe80::1/64
 en0        52:54:0:9b:ba:6e     ipv6         fe80::143b:6173:696:e384/64
 -          -                    ipv4         192.168.100.133/24
 utun0      0:0:0:0:0:0          ipv6         fe80::6acf:786a:a5db:69d1/64
 utun1      0:0:0:0:0:0          ipv6         fe80::f380:1b3c:4f93:2de0/64
 utun2      0:0:0:0:0:0          ipv6         fe80::ce81:b1c:bd2c:69e/64"
        ),
        Some(Ipv4Addr::from_str("192.168.100.133").expect("Guaranteed by argument"))
    );
}
