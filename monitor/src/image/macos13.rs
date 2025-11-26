use std::ffi::OsStr;
use std::fs::copy;
use std::fs::remove_file;
use std::path::Path;
use std::time::Duration;

use bytesize::ByteSize;
use cmd_lib::run_cmd;
use cmd_lib::spawn_with_output;
use hypervisor::delete_guest;
use hypervisor::libvirt::create_runner_images_dir;
use hypervisor::libvirt::create_template_or_rebuild_images_dir;
use hypervisor::libvirt::delete_template_or_rebuild_image_file;
use hypervisor::libvirt::libvirt_change_media;
use hypervisor::libvirt::CdromImage;
use hypervisor::rename_guest;
use hypervisor::start_guest;
use hypervisor::wait_for_guest;
use jane_eyre::eyre;
use jane_eyre::eyre::OptionExt;
use settings::profile::Profile;
use shell::atomic_symlink;
use shell::log_output_as_info;
use shell::reflink_or_copy_with_warning;
use tracing::warn;

use crate::data::get_profile_data_path;
use crate::image::Image;
use crate::policy::runner_image_path;
use crate::policy::template_or_rebuild_image_path;

use super::create_disk_image;

pub struct Macos13 {
    base_image_size: ByteSize,
    wait_duration: Duration,
}

impl Macos13 {
    pub const fn new(base_image_size: ByteSize, wait_duration: Duration) -> Self {
        Self {
            base_image_size,
            wait_duration,
        }
    }
}

impl Image for Macos13 {
    fn rebuild(&self, profile: &Profile, snapshot_name: &str) -> eyre::Result<()> {
        rebuild(
            profile,
            snapshot_name,
            self.base_image_size,
            self.wait_duration,
        )
    }
    fn delete_template(&self, profile: &Profile, snapshot_name: &str) -> eyre::Result<()> {
        delete_template(profile, snapshot_name)
    }
    fn register_runner(&self, profile: &Profile, runner_guest_name: &str) -> eyre::Result<String> {
        register_runner(profile, runner_guest_name)
    }
    fn create_runner(
        &self,
        profile: &Profile,
        snapshot_name: &str,
        runner_guest_name: &str,
        runner_id: usize,
    ) -> eyre::Result<String> {
        create_runner(profile, snapshot_name, runner_guest_name, runner_id)
    }
    fn destroy_runner(&self, runner_guest_name: &str, runner_id: usize) -> eyre::Result<()> {
        destroy_runner(runner_guest_name, runner_id)
    }
}

pub(super) fn rebuild(
    profile: &Profile,
    snapshot_name: &str,
    base_image_size: ByteSize,
    wait_duration: Duration,
) -> eyre::Result<()> {
    let base_images_path = create_template_or_rebuild_images_dir(&profile)?;
    let profile_name = &profile.profile_name;
    let snapshot_path_slug = &profile.snapshot_path_slug(snapshot_name);
    let rebuild_guest_name = &profile.rebuild_guest_name(snapshot_name);

    let initial_contents_path = format!("/var/lib/libvirt/images/{profile_name}.clean.img");
    let base_image_path = create_disk_image(
        base_images_path,
        snapshot_name,
        base_image_size,
        Path::new(&initial_contents_path),
    )?;

    define_base_guest(profile, snapshot_name, &base_image_path, &[])?;
    let ovmf_vars_clean_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{profile_name}.clean.fd");
    let ovmf_vars_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{snapshot_path_slug}.fd");
    copy(ovmf_vars_clean_path, ovmf_vars_path)?;
    start_guest(rebuild_guest_name)?;
    wait_for_guest(rebuild_guest_name, wait_duration)?;

    let template_guest_name = &profile.template_guest_name(snapshot_name);
    rename_guest(rebuild_guest_name, template_guest_name)?;
    let snapshot_symlink_path =
        get_profile_data_path(&profile.profile_name, Path::new("snapshot"))?;
    atomic_symlink(snapshot_name, snapshot_symlink_path)?;

    Ok(())
}

fn define_base_guest(
    profile: &Profile,
    snapshot_name: &str,
    base_image_path: &dyn AsRef<OsStr>,
    cdrom_images: &[CdromImage],
) -> eyre::Result<()> {
    let clean_guest_name = &format!("{}.clean", profile.profile_name);
    let rebuild_guest_name = &profile.rebuild_guest_name(snapshot_name);
    let base_image_path = base_image_path
        .as_ref()
        .to_str()
        .ok_or_eyre("Unsupported path")?;
    // Clone the hand-made clean guest, since we can’t yet automate the macOS install
    run_cmd!(virt-clone --preserve-data --check path_in_use=off -o $clean_guest_name -n $rebuild_guest_name --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.$clean_guest_name.fd --skip-copy sda -f $base_image_path --skip-copy sdc)?;
    libvirt_change_media(rebuild_guest_name, cdrom_images)?;

    Ok(())
}

pub(super) fn delete_template(profile: &Profile, snapshot_name: &str) -> eyre::Result<()> {
    delete_guest(&profile.template_guest_name(snapshot_name))?;
    delete_template_or_rebuild_image_file(profile, &format!("base.img@{snapshot_name}"));
    Ok(())
}

pub fn register_runner(profile: &Profile, runner_guest_name: &str) -> eyre::Result<String> {
    monitor::github::register_runner(
        runner_guest_name,
        &profile.github_runner_label,
        "/Users/servo/a",
    )
}

pub fn create_runner(
    profile: &Profile,
    snapshot_name: &str,
    runner_guest_name: &str,
    runner_id: usize,
) -> eyre::Result<String> {
    let pipe = || |reader| log_output_as_info(reader);
    let snapshot_path_slug = &profile.snapshot_path_slug(snapshot_name);
    let template_guest_name = &profile.template_guest_name(snapshot_name);

    // Copy images in the monitor, not with `virt-clone --auto-clone --reflink`,
    // because the latter can’t be parallelised without causing errors.
    let template_base_img = template_or_rebuild_image_path(profile, snapshot_name, "base.img");
    create_runner_images_dir()?;
    let runner_base_img = runner_image_path(runner_id, "base.img");
    reflink_or_copy_with_warning(&template_base_img, &runner_base_img)?;

    let ovmf_vars_base_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{snapshot_path_slug}.fd");
    let ovmf_vars_path =
        format!("/var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{runner_guest_name}.fd");
    copy(ovmf_vars_base_path, ovmf_vars_path)?;

    spawn_with_output!(virt-clone -o $template_guest_name -n $runner_guest_name --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.$runner_guest_name.fd --preserve-data --skip-copy sda -f $runner_base_img --skip-copy sdc 2>&1)?.wait_with_pipe(&mut pipe())?;

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
