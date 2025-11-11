#[cfg_attr(target_os = "linux", path = "impl_libvirt.rs")]
mod platform;

use std::{net::Ipv4Addr, path::Path};

use jane_eyre::eyre;

pub fn list_template_guests() -> eyre::Result<Vec<String>> {
    self::platform::list_template_guests()
}

pub fn list_rebuild_guests() -> eyre::Result<Vec<String>> {
    self::platform::list_rebuild_guests()
}

pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
    self::platform::list_runner_guests()
}

pub fn update_screenshot(guest_name: &str, output_dir: &Path) -> eyre::Result<()> {
    self::platform::update_screenshot(guest_name, output_dir)
}

pub fn take_screenshot(guest_name: &str, output_path: &Path) -> eyre::Result<()> {
    self::platform::take_screenshot(guest_name, output_path)
}

pub fn get_ipv4_address(guest_name: &str) -> Option<Ipv4Addr> {
    self::platform::get_ipv4_address(guest_name)
}
