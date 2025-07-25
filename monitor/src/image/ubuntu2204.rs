use std::ffi::OsStr;
use std::fs::remove_file;
use std::path::Path;
use std::time::Duration;

use bytesize::ByteSize;
use cmd_lib::run_cmd;
use cmd_lib::spawn_with_output;
use jane_eyre::eyre;
use jane_eyre::eyre::OptionExt;
use settings::profile::Profile;
use tracing::info;
use tracing::warn;

use crate::data::get_profile_configuration_path;
use crate::image::create_base_images_dir;
use crate::image::create_runner_images_dir;
use crate::image::delete_base_image_file;
use crate::image::prune_base_image_files;
use crate::image::undefine_libvirt_guest;
use crate::policy::runner_images_path;
use crate::shell::atomic_symlink;
use crate::shell::log_output_as_info;
use crate::shell::reflink_or_copy_with_warning;
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
    let base_image_path = create_disk_image(
        base_images_path,
        snapshot_name,
        base_image_size,
        Path::new(&os_image_path),
    )?;

    define_base_guest(
        profile,
        &base_image_path,
        &[CdromImage::new("sda", config_iso_path)],
    )?;
    start_libvirt_guest(base_vm_name)?;
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

pub(super) fn redefine_base_guest_with_symlinks(
    base_images_path: impl AsRef<Path>,
    profile: &Profile,
) -> Result<(), eyre::Error> {
    let base_images_path = base_images_path.as_ref();
    let config_iso_symlink_path = base_images_path.join(format!("config.iso"));
    let config_iso_symlink_path = config_iso_symlink_path
        .to_str()
        .ok_or_eyre("Unsupported path")?;
    let base_image_symlink_path = base_images_path.join(format!("base.img"));
    undefine_libvirt_guest(&profile.base_vm_name)?;
    define_base_guest(
        profile,
        &base_image_symlink_path,
        &[CdromImage::new("sda", &config_iso_symlink_path)],
    )?;

    Ok(())
}

fn define_base_guest(
    profile: &Profile,
    base_image_path: &dyn AsRef<OsStr>,
    cdrom_images: &[CdromImage],
) -> eyre::Result<()> {
    let base_vm_name = &profile.base_vm_name;
    let guest_xml_path = get_profile_configuration_path(&profile, Path::new("guest.xml"))?;
    define_libvirt_guest(
        base_vm_name,
        guest_xml_path,
        &[&"-f", &base_image_path],
        cdrom_images,
    )?;

    Ok(())
}

pub(super) fn prune_images(profile: &Profile) -> eyre::Result<()> {
    prune_base_image_files(profile, "config.iso")?;
    prune_base_image_files(profile, "base.img")?;

    Ok(())
}

pub(super) fn delete_image(profile: &Profile, snapshot_name: &str) {
    delete_base_image_file(profile, &format!("config.iso@{snapshot_name}"));
    delete_base_image_file(profile, &format!("base.img@{snapshot_name}"));
}

pub fn register_runner(profile: &Profile, vm_name: &str) -> eyre::Result<String> {
    crate::github::register_runner(vm_name, &profile.github_runner_label, "/a")
}

pub fn create_runner(profile: &Profile, vm_name: &str, runner_id: usize) -> eyre::Result<String> {
    let prefixed_vm_name = format!("{}-{vm_name}", DOTENV.libvirt_prefix);
    let pipe = || |reader| log_output_as_info(reader);
    let base_vm_name = &profile.base_vm_name;

    // Copy images in the monitor, not with `virt-clone --auto-clone --reflink`,
    // because the latter can’t be parallelised without causing errors.
    // TODO copy config.iso?
    let base_images_path = create_base_images_dir(profile)?;
    let base_image_symlink_path = base_images_path.join(format!("base.img"));
    let runner_images_path = create_runner_images_dir(runner_id)?;
    let runner_base_image_path = runner_images_path.join(format!("base.img"));
    reflink_or_copy_with_warning(&base_image_symlink_path, &runner_base_image_path)?;

    spawn_with_output!(virt-clone -o $base_vm_name -n $prefixed_vm_name --preserve-data -f $runner_base_image_path 2>&1)?
        .wait_with_pipe(&mut pipe())?;

    Ok(prefixed_vm_name)
}

pub fn destroy_runner(vm_name: &str, runner_id: usize) -> eyre::Result<()> {
    // TODO delete config.iso?
    let runner_images_path = runner_images_path(runner_id);
    let runner_base_image_path = runner_images_path.join(format!("base.img"));
    if let Err(error) = remove_file(&runner_base_image_path) {
        warn!(?runner_base_image_path, ?error, "Failed to delete file");
    }

    let prefixed_vm_name = format!("{}-{vm_name}", DOTENV.libvirt_prefix);
    let pipe = || |reader| log_output_as_info(reader);
    let _ =
        spawn_with_output!(virsh destroy -- $prefixed_vm_name 2>&1)?.wait_with_pipe(&mut pipe());
    let _ = spawn_with_output!(virsh undefine --nvram --storage vda -- $prefixed_vm_name 2>&1)?
        .wait_with_pipe(&mut pipe());

    Ok(())
}
