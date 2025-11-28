pub mod libvirt;
pub mod utm;

#[cfg_attr(target_os = "linux", path = "impl_libvirt.rs")]
#[cfg_attr(target_os = "macos", path = "impl_utm.rs")]
mod platform;

use std::{collections::BTreeSet, net::Ipv4Addr, path::Path, time::Duration};

use jane_eyre::eyre;
use settings::profile::Profile;

pub fn initialise() -> eyre::Result<()> {
    self::platform::initialise()
}

pub fn handle_main_thread_request() -> eyre::Result<()> {
    self::platform::handle_main_thread_request()
}

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

pub fn start_guest(guest_name: &str) -> eyre::Result<()> {
    self::platform::start_guest(guest_name)
}

pub fn wait_for_guest(guest_name: &str, timeout: Duration) -> eyre::Result<()> {
    self::platform::wait_for_guest(guest_name, timeout)
}

pub fn rename_guest(old_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    self::platform::rename_guest(old_guest_name, new_guest_name)
}

pub fn delete_guest(guest_name: &str) -> eyre::Result<()> {
    self::platform::delete_guest(guest_name)
}

pub fn prune_base_image_files(
    profile: &Profile,
    keep_snapshots: BTreeSet<String>,
) -> eyre::Result<()> {
    self::platform::prune_base_image_files(profile, keep_snapshots)
}
