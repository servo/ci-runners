use std::{
    ffi::OsStr,
    fs::{create_dir_all, remove_file},
    path::{Path, PathBuf},
};

use cmd_lib::run_cmd;
use jane_eyre::eyre;
use settings::profile::Profile;
use tracing::{debug, info, warn};

pub fn template_or_rebuild_images_path(profile: &Profile) -> PathBuf {
    Path::new("/var/lib/libvirt/images/base").join(&profile.profile_name)
}

pub fn runner_images_path() -> PathBuf {
    PathBuf::from("/var/lib/libvirt/images/runner")
}

pub fn delete_template_or_rebuild_image_file(profile: &Profile, filename: &str) {
    let base_images_path = template_or_rebuild_images_path(profile);
    let path = base_images_path.join(filename);
    info!(?path, "Deleting");
    if let Err(error) = remove_file(&path) {
        warn!(?path, ?error, "Failed to delete");
    }
}

pub fn create_template_or_rebuild_images_dir(profile: &Profile) -> eyre::Result<PathBuf> {
    let base_images_path = template_or_rebuild_images_path(profile);
    debug!(?base_images_path, "Creating base images subdirectory");
    create_dir_all(&base_images_path)?;

    Ok(base_images_path)
}

pub fn create_runner_images_dir() -> eyre::Result<PathBuf> {
    let runner_images_path = runner_images_path();
    debug!(?runner_images_path, "Creating runner images directory");
    create_dir_all(&runner_images_path)?;

    Ok(runner_images_path)
}

pub fn define_libvirt_guest(
    profile_name: &str,
    guest_name: &str,
    guest_xml_path: impl AsRef<Path>,
    args: &[&dyn AsRef<OsStr>],
    cdrom_images: &[CdromImage],
) -> eyre::Result<()> {
    // This dance is needed to randomise the MAC address of the guest.
    let guest_xml_path = guest_xml_path.as_ref();
    let args = args.iter().map(|x| x.as_ref()).collect::<Vec<_>>();
    run_cmd!(virsh define -- $guest_xml_path)?;
    run_cmd!(virt-clone --preserve-data --check path_in_use=off -o $profile_name.init -n $guest_name $[args])?;
    libvirt_change_media(guest_name, cdrom_images)?;
    run_cmd!(virsh undefine -- $profile_name.init)?;

    Ok(())
}

pub fn libvirt_change_media(guest_name: &str, cdrom_images: &[CdromImage]) -> eyre::Result<()> {
    for CdromImage { target_dev, path } in cdrom_images {
        run_cmd!(virsh change-media -- $guest_name $target_dev $path)?;
    }

    Ok(())
}

pub struct CdromImage<'path> {
    pub target_dev: &'static str,
    pub path: &'path str,
}
impl<'path> CdromImage<'path> {
    pub fn new(target_dev: &'static str, path: &'path str) -> Self {
        Self { target_dev, path }
    }
}
