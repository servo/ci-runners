use std::ffi::OsStr;
use std::fs::copy;
use std::fs::File;
use std::path::Path;
use std::time::Duration;

use bytesize::ByteSize;
use cmd_lib::run_cmd;
use cmd_lib::spawn_with_output;
use jane_eyre::eyre;
use jane_eyre::eyre::OptionExt;

use crate::image::delete_base_image_file;
use crate::image::libvirt_change_media;
use crate::image::prune_base_image_files;
use crate::image::undefine_libvirt_guest;
use crate::image::CdromImage;
use crate::profile::Profile;
use crate::shell::atomic_symlink;
use crate::shell::log_output_as_info;
use crate::DOTENV;

use super::create_disk_image;
use super::start_libvirt_guest;
use super::wait_for_guest;

pub(super) fn rebuild(
    base_images_path: impl AsRef<Path>,
    profile: &Profile,
    snapshot_name: &str,
    base_image_size: ByteSize,
    wait_duration: Duration,
) -> eyre::Result<()> {
    let base_images_path = base_images_path.as_ref();
    let base_vm_name = &profile.base_vm_name;

    let base_image_symlink_path = base_images_path.join(format!("base.img"));
    let os_image_path = Path::new("/dev/zvol")
        .join(&DOTENV.zfs_clone_prefix)
        .join("servo-macos13.clean@automated");
    let os_image = File::open(os_image_path)?;
    let base_image_path =
        create_disk_image(base_images_path, snapshot_name, base_image_size, os_image)?;

    define_base_guest(profile, &base_image_path, &[])?;
    // Clone the hand-made clean guest, since we can’t yet automate the macOS install
    run_cmd!(virt-clone --preserve-data --check path_in_use=off -o $base_vm_name.clean -n $base_vm_name --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.$base_vm_name.fd --skip-copy sda -f $base_image_path --skip-copy sdc)?;
    let ovmf_vars_clean_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{base_vm_name}.clean.fd");
    let ovmf_vars_path = format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{base_vm_name}.fd");
    copy(ovmf_vars_clean_path, ovmf_vars_path)?;
    start_libvirt_guest(base_vm_name)?;
    wait_for_guest(base_vm_name, wait_duration)?;

    let base_image_filename = Path::new(
        base_image_path
            .file_name()
            .expect("Guaranteed by make_disk_image"),
    );
    atomic_symlink(base_image_filename, base_image_symlink_path)?;

    Ok(())
}

pub(super) fn redefine_base_guest_with_symlinks(
    base_images_path: impl AsRef<Path>,
    profile: &Profile,
) -> Result<(), eyre::Error> {
    let base_images_path = base_images_path.as_ref();
    let base_image_symlink_path = base_images_path.join(format!("base.img"));
    undefine_libvirt_guest(&profile.base_vm_name)?;
    define_base_guest(profile, &base_image_symlink_path, &[])?;

    Ok(())
}

fn define_base_guest(
    profile: &Profile,
    base_image_path: &dyn AsRef<OsStr>,
    cdrom_images: &[CdromImage],
) -> eyre::Result<()> {
    let base_vm_name = &profile.base_vm_name;
    let base_image_path = base_image_path
        .as_ref()
        .to_str()
        .ok_or_eyre("Unsupported path")?;
    // Clone the hand-made clean guest, since we can’t yet automate the macOS install
    run_cmd!(virt-clone --preserve-data --check path_in_use=off -o $base_vm_name.clean -n $base_vm_name --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.$base_vm_name.fd --skip-copy sda -f $base_image_path --skip-copy sdc)?;
    libvirt_change_media(base_vm_name, cdrom_images)?;

    Ok(())
}

pub(super) fn prune_images(profile: &Profile) -> eyre::Result<()> {
    prune_base_image_files(profile, "base.img")?;

    Ok(())
}

pub(super) fn delete_image(profile: &Profile, snapshot_name: &str) {
    delete_base_image_file(profile, &format!("base.img@{snapshot_name}"));
}

pub fn register_runner(profile: &Profile, vm_name: &str) -> eyre::Result<String> {
    crate::github::register_runner(vm_name, &profile.github_runner_label, "/Users/servo/a")
}

pub fn create_runner(profile: &Profile, vm_name: &str) -> eyre::Result<()> {
    let prefixed_vm_name = format!("{}-{vm_name}", DOTENV.libvirt_prefix);
    let pipe = || |reader| log_output_as_info(reader);
    let base_vm_name = &profile.base_vm_name;
    spawn_with_output!(virt-clone --auto-clone --reflink -o $base_vm_name -n $prefixed_vm_name --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.$vm_name.fd --skip-copy sda --skip-copy sdc 2>&1)?.wait_with_pipe(&mut pipe())?;
    let ovmf_vars_base_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{base_vm_name}.clean.fd");
    let ovmf_vars_path = format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{vm_name}.fd");
    copy(ovmf_vars_base_path, ovmf_vars_path)?;
    start_libvirt_guest(&prefixed_vm_name)?;

    Ok(())
}

pub fn destroy_runner(vm_name: &str) -> eyre::Result<()> {
    let prefixed_vm_name = format!("{}-{vm_name}", DOTENV.libvirt_prefix);
    let pipe = || |reader| log_output_as_info(reader);
    let _ =
        spawn_with_output!(virsh destroy -- $prefixed_vm_name 2>&1)?.wait_with_pipe(&mut pipe());
    let _ = spawn_with_output!(virsh undefine --nvram --storage sdb -- $prefixed_vm_name 2>&1)?
        .wait_with_pipe(&mut pipe());

    Ok(())
}
