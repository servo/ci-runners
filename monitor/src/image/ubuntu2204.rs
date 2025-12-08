use std::ffi::OsStr;
use std::fs::remove_file;
use std::path::Path;
use std::time::Duration;

use bytesize::ByteSize;
use cmd_lib::run_cmd;
use cmd_lib::spawn_with_output;
use jane_eyre::eyre;
use settings::profile::Profile;
use tracing::info;
use tracing::warn;

use crate::data::get_profile_configuration_path;
use crate::data::get_profile_data_path;
use crate::image::create_runner_images_dir;
use crate::image::delete_template_or_rebuild_image_file;
use crate::image::rename_guest;
use crate::image::undefine_libvirt_guest;
use crate::policy::runner_image_path;
use crate::policy::template_or_rebuild_image_path;
use crate::shell::atomic_symlink;
use crate::shell::log_output_as_info;
use crate::shell::reflink_or_copy_with_warning;
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
    let rebuild_guest_name = &profile.rebuild_guest_name(snapshot_name);
    let profile_configuration_path = get_profile_configuration_path(&profile, None)?;
    let config_iso_filename = format!("config.iso@{snapshot_name}");
    let config_iso_path = base_images_path.join(&config_iso_filename);
    let config_iso_path = config_iso_path.to_str().expect("Unsupported path");
    info!(config_iso_path, "Creating config image file");
    run_cmd!(genisoimage -V CIDATA -R -f -o $config_iso_path $profile_configuration_path/user-data $profile_configuration_path/meta-data)?;

    let os_image_path = IMAGE_DEPS_DIR
        .join("ubuntu2204")
        .join("jammy-server-cloudimg-amd64.raw");
    let base_image_path = create_disk_image(
        base_images_path,
        snapshot_name,
        base_image_size,
        Path::new(&os_image_path),
    )?;

    define_base_guest(
        profile,
        snapshot_name,
        &base_image_path,
        &[CdromImage::new("sda", config_iso_path)],
    )?;
    start_libvirt_guest(rebuild_guest_name)?;
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
    let rebuild_guest_name = &profile.rebuild_guest_name(snapshot_name);
    let guest_xml_path = get_profile_configuration_path(&profile, Path::new("guest.xml"))?;
    define_libvirt_guest(
        &profile.profile_name,
        rebuild_guest_name,
        guest_xml_path,
        &[&"-f", &base_image_path],
        cdrom_images,
    )?;

    Ok(())
}

pub(super) fn delete_template(profile: &Profile, snapshot_name: &str) -> eyre::Result<()> {
    undefine_libvirt_guest(&profile.template_guest_name(snapshot_name))?;
    delete_template_or_rebuild_image_file(profile, &format!("config.iso@{snapshot_name}"));
    delete_template_or_rebuild_image_file(profile, &format!("base.img@{snapshot_name}"));
    Ok(())
}

pub fn register_runner(runner_guest_name: &str, labels: &[String]) -> eyre::Result<String> {
    monitor::github::register_runner(runner_guest_name, "/a", labels)
}

pub fn create_runner(
    profile: &Profile,
    snapshot_name: &str,
    runner_guest_name: &str,
    runner_id: usize,
) -> eyre::Result<String> {
    let pipe = || |reader| log_output_as_info(reader);
    let template_guest_name = &profile.template_guest_name(snapshot_name);

    // Copy images in the monitor, not with `virt-clone --auto-clone --reflink`,
    // because the latter can’t be parallelised without causing errors.
    // TODO copy config.iso?
    let template_base_img = template_or_rebuild_image_path(profile, snapshot_name, "base.img");
    create_runner_images_dir()?;
    let runner_base_img = runner_image_path(runner_id, "base.img");
    reflink_or_copy_with_warning(&template_base_img, &runner_base_img)?;

    spawn_with_output!(virt-clone -o $template_guest_name -n $runner_guest_name --preserve-data -f $runner_base_img 2>&1)?
        .wait_with_pipe(&mut pipe())?;

    Ok(runner_guest_name.to_owned())
}

pub fn destroy_runner(runner_guest_name: &str, runner_id: usize) -> eyre::Result<()> {
    // TODO delete config.iso?
    let runner_base_image_path = runner_image_path(runner_id, "base.img");
    if let Err(error) = remove_file(&runner_base_image_path) {
        warn!(?runner_base_image_path, ?error, "Failed to delete file");
    }

    let pipe = || |reader| log_output_as_info(reader);
    let _ =
        spawn_with_output!(virsh destroy -- $runner_guest_name 2>&1)?.wait_with_pipe(&mut pipe());
    let _ = spawn_with_output!(virsh undefine --nvram --storage vda -- $runner_guest_name 2>&1)?
        .wait_with_pipe(&mut pipe());

    Ok(())
}
