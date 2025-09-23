use core::str;
use std::{
    fs::{create_dir_all, rename},
    net::Ipv4Addr,
    path::Path,
};

use cmd_lib::{run_fun, spawn_with_output};
use jane_eyre::eyre;
use settings::TOML;
use tracing::debug;

use crate::shell::log_output_as_trace;

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
