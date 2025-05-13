use std::fs::File;
use std::path::Path;
use std::time::Duration;

use bytesize::ByteSize;
use cmd_lib::run_cmd;
use cmd_lib::spawn_with_output;
use jane_eyre::eyre;
use jane_eyre::eyre::OptionExt;
use tracing::info;

use crate::data::get_profile_configuration_path;
use crate::profile::Profile;
use crate::shell::atomic_symlink;
use crate::shell::log_output_as_info;
use crate::DOTENV;
use crate::IMAGE_DEPS_DIR;

use super::create_disk_image;
use super::define_libvirt_guest;
use super::start_libvirt_guest;
use super::wait_for_guest;
use super::CdromImage;

pub(super) fn rebuild(
    base_images_path: impl AsRef<Path>,
    profile: &Profile,
    snapshot_name: &str,
    base_image_size: ByteSize,
    wait_duration: Duration,
) -> eyre::Result<()> {
    let base_images_path = base_images_path.as_ref();
    let base_vm_name = &profile.base_vm_name;
    let profile_configuration_path = get_profile_configuration_path(&profile, None)?;
    let config_iso_symlink_path = base_images_path.join(format!("config.iso"));
    let config_iso_filename = format!("config.iso@{snapshot_name}");
    let config_iso_path = base_images_path.join(&config_iso_filename);
    let config_iso_path = config_iso_path.to_str().expect("Unsupported path");
    info!(config_iso_path, "Creating config image file");
    run_cmd!(genisoimage -V CIDATA -R -f -o $config_iso_path $profile_configuration_path/user-data $profile_configuration_path/meta-data)?;

    let base_image_symlink_path = base_images_path.join(format!("base.img"));
    let os_image_path = IMAGE_DEPS_DIR
        .join("ubuntu2204")
        .join("jammy-server-cloudimg-amd64.raw");
    let os_image = File::open(os_image_path)?;
    let base_image_path =
        create_disk_image(base_images_path, snapshot_name, base_image_size, os_image)?;

    let guest_xml_path = get_profile_configuration_path(&profile, Path::new("guest.xml"))?;
    define_libvirt_guest(base_vm_name, guest_xml_path, &[&"-f", &base_image_path])?;
    start_libvirt_guest(base_vm_name, &[CdromImage::new("sda", config_iso_path)])?;
    wait_for_guest(base_vm_name, wait_duration)?;

    let base_image_filename = Path::new(
        base_image_path
            .file_name()
            .expect("Guaranteed by make_disk_image"),
    );
    atomic_symlink(config_iso_filename, config_iso_symlink_path)?;
    atomic_symlink(base_image_filename, base_image_symlink_path)?;

    Ok(())
}

pub fn register_runner(profile: &Profile, vm_name: &str) -> eyre::Result<String> {
    crate::github::register_runner(vm_name, &profile.github_runner_label, "/a")
}

pub fn create_runner(profile: &Profile, vm_name: &str) -> eyre::Result<()> {
    let prefixed_vm_name = format!("{}-{vm_name}", DOTENV.libvirt_prefix);
    let pipe = || |reader| log_output_as_info(reader);
    let base_vm_name = &profile.base_vm_name;
    spawn_with_output!(virt-clone --auto-clone --reflink -o $base_vm_name -n $prefixed_vm_name 2>&1)?
        .wait_with_pipe(&mut pipe())?;

    let config_iso_path = profile.base_images_path().join("config.iso");
    let config_iso_path = config_iso_path.to_str().ok_or_eyre("Unsupported path")?;
    start_libvirt_guest(
        &prefixed_vm_name,
        &[CdromImage::new("sda", config_iso_path)],
    )?;

    Ok(())
}

pub fn destroy_runner(vm_name: &str) -> eyre::Result<()> {
    let prefixed_vm_name = format!("{}-{vm_name}", DOTENV.libvirt_prefix);
    let pipe = || |reader| log_output_as_info(reader);
    let _ = spawn_with_output!(virsh undefine --nvram --storage vda -- $prefixed_vm_name 2>&1)?
        .wait_with_pipe(&mut pipe());

    Ok(())
}
