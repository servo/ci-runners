#[cfg_attr(target_os = "linux", path = "hypervisor_libvirt.rs")]
#[cfg_attr(target_os = "macos", path = "hypervisor_utm.rs")]
mod platform;

use jane_eyre::eyre;

pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
    self::platform::list_runner_guests()
}
