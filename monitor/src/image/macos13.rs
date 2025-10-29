use std::ffi::OsStr;
use std::fs::copy;
use std::fs::remove_file;
use std::path::Path;
use std::time::Duration;

use bytesize::ByteSize;
use cmd_lib::run_cmd;
use cmd_lib::spawn_with_output;
use jane_eyre::eyre;
use jane_eyre::eyre::OptionExt;
use settings::profile::Profile;
use tracing::warn;

use crate::image::create_base_images_dir;
use crate::image::create_runner_images_dir;
use crate::image::delete_base_image_file;
use crate::image::libvirt_change_media;
use crate::image::prune_base_image_files;
use crate::image::undefine_libvirt_guest;
use crate::image::CdromImage;
use crate::policy::runner_image_path;
use crate::shell::atomic_symlink;
use crate::shell::log_output_as_info;
use crate::shell::reflink_or_copy_with_warning;

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
    let profile_guest_name = &profile.profile_guest_name();

    let base_image_symlink_path = base_images_path.join(format!("base.img"));
    let initial_contents_path = format!("/var/lib/libvirt/images/{profile_guest_name}.clean.img");
    let base_image_path = create_disk_image(
        base_images_path,
        snapshot_name,
        base_image_size,
        Path::new(&initial_contents_path),
    )?;

    define_base_guest(profile, &base_image_path, &[])?;
    let ovmf_vars_clean_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{profile_guest_name}.clean.fd");
    let ovmf_vars_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{profile_guest_name}.fd");
    copy(ovmf_vars_clean_path, ovmf_vars_path)?;
    start_libvirt_guest(profile_guest_name)?;
    wait_for_guest(profile_guest_name, wait_duration)?;

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
    undefine_libvirt_guest(&profile.profile_guest_name())?;
    define_base_guest(profile, &base_image_symlink_path, &[])?;

    Ok(())
}

fn define_base_guest(
    profile: &Profile,
    base_image_path: &dyn AsRef<OsStr>,
    cdrom_images: &[CdromImage],
) -> eyre::Result<()> {
    let profile_guest_name = &profile.profile_guest_name();
    let base_image_path = base_image_path
        .as_ref()
        .to_str()
        .ok_or_eyre("Unsupported path")?;
    // Clone the hand-made clean guest, since we can’t yet automate the macOS install
    run_cmd!(virt-clone --preserve-data --check path_in_use=off -o $profile_guest_name.clean -n $profile_guest_name --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.$profile_guest_name.fd --skip-copy sda -f $base_image_path --skip-copy sdc)?;
    libvirt_change_media(profile_guest_name, cdrom_images)?;

    Ok(())
}

pub(super) fn prune_images(profile: &Profile) -> eyre::Result<()> {
    prune_base_image_files(profile, "base.img")?;

    Ok(())
}

pub(super) fn delete_image(profile: &Profile, snapshot_name: &str) {
    delete_base_image_file(profile, &format!("base.img@{snapshot_name}"));
}

pub fn register_runner(profile: &Profile, runner_guest_name: &str) -> eyre::Result<String> {
    crate::github::register_runner(
        runner_guest_name,
        &profile.github_runner_label,
        "/Users/servo/a",
    )
}

pub fn create_runner(
    profile: &Profile,
    runner_guest_name: &str,
    runner_id: usize,
) -> eyre::Result<String> {
    let pipe = || |reader| log_output_as_info(reader);
    let profile_guest_name = &profile.profile_name;

    // Copy images in the monitor, not with `virt-clone --auto-clone --reflink`,
    // because the latter can’t be parallelised without causing errors.
    let base_images_path = create_base_images_dir(profile)?;
    let base_image_symlink_path = base_images_path.join(format!("base.img"));
    create_runner_images_dir()?;
    let runner_base_image_path = runner_image_path(runner_id, "base.img");
    reflink_or_copy_with_warning(&base_image_symlink_path, &runner_base_image_path)?;

    let ovmf_vars_base_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{profile_guest_name}.clean.fd");
    let ovmf_vars_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{runner_guest_name}.fd");
    copy(ovmf_vars_base_path, ovmf_vars_path)?;

    spawn_with_output!(virt-clone -o $profile_guest_name -n $runner_guest_name --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.$runner_guest_name.fd --preserve-data --skip-copy sda -f $runner_base_image_path --skip-copy sdc 2>&1)?.wait_with_pipe(&mut pipe())?;

    Ok(runner_guest_name.to_owned())
}

pub fn destroy_runner(runner_guest_name: &str, runner_id: usize) -> eyre::Result<()> {
    let runner_base_image_path = runner_image_path(runner_id, "base.img");
    if let Err(error) = remove_file(&runner_base_image_path) {
        warn!(?runner_base_image_path, ?error, "Failed to delete file");
    }
    let ovmf_vars_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{runner_guest_name}.fd");
    if let Err(error) = remove_file(&ovmf_vars_path) {
        warn!(?ovmf_vars_path, ?error, "Failed to delete file");
    }

    let pipe = || |reader| log_output_as_info(reader);
    let _ =
        spawn_with_output!(virsh destroy -- $runner_guest_name 2>&1)?.wait_with_pipe(&mut pipe());
    let _ = spawn_with_output!(virsh undefine --nvram --storage sdb -- $runner_guest_name 2>&1)?
        .wait_with_pipe(&mut pipe());

    Ok(())
}
