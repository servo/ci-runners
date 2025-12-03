pub mod macos13;
pub mod ubuntu2204;
pub mod windows10;

use core::str;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs::{create_dir_all, read_dir, remove_file, set_permissions, File},
    io::{Seek, Write},
    mem::take,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
    time::Duration,
};

use bytesize::ByteSize;
use chrono::{SecondsFormat, Utc};
use cmd_lib::{run_cmd, run_fun, spawn_with_output};
use jane_eyre::eyre::{self, bail, OptionExt};
use settings::{
    profile::{parse_rebuild_guest_name, parse_template_guest_name, Profile},
    TOML,
};
use tracing::{debug, error, info, warn};

use crate::{
    libvirt::{list_rebuild_guests, list_template_guests},
    policy::{runner_images_path, template_or_rebuild_images_path, Policy},
    shell::{log_output_as_info, reflink_or_copy_with_warning},
};

#[derive(Debug, Default)]
pub struct Rebuilds {
    cached_servo_repo_update: Option<JoinHandle<eyre::Result<()>>>,
    rebuilds: BTreeMap<String, Rebuild>,
}

#[derive(Debug)]
struct Rebuild {
    thread: JoinHandle<eyre::Result<()>>,
    snapshot_name: String,
    guest_name: String,
}

impl Rebuilds {
    pub fn run(&mut self, policy: &mut Policy) -> eyre::Result<()> {
        // Clean up any dangling resources from past rebuilds.
        let current_known_rebuild_guest_names = self
            .rebuild_guest_names()
            .into_iter()
            .map(|(_key, guest_name)| guest_name)
            .collect::<BTreeSet<_>>();
        for rebuild_guest_name in list_rebuild_guests()? {
            if !current_known_rebuild_guest_names.contains(&rebuild_guest_name) {
                undefine_libvirt_guest(&rebuild_guest_name)?;
                let (profile_key, snapshot_name) =
                    match parse_rebuild_guest_name(&rebuild_guest_name) {
                        Ok(result) => result,
                        Err(error) => {
                            warn!(?error, "Failed to clean up bad image files");
                            continue;
                        }
                    };
                let Some(profile) = policy.profile(profile_key) else {
                    warn!(
                        ?profile_key,
                        "Failed to clean up bad image files: Unknown profile"
                    );
                    continue;
                };
                delete_template(profile, snapshot_name)?;
            }
        }

        let mut profiles_needing_rebuild = BTreeMap::default();
        let mut cached_servo_repo_was_just_updated = false;

        // Reap the Servo update thread, if needed.
        if let Some(thread) = self.cached_servo_repo_update.take() {
            if thread.is_finished() {
                match thread.join() {
                    Ok(Ok(())) => {
                        info!("Servo update thread exited");
                        cached_servo_repo_was_just_updated = true;
                    }
                    Ok(Err(report)) => error!(%report, "Servo update thread error"),
                    Err(panic) => error!(?panic, "Servo update thread panic"),
                };
            } else {
                self.cached_servo_repo_update = Some(thread);
            }
        }

        // Determine which profiles need their images rebuilt.
        for (key, profile) in policy.profiles() {
            let needs_rebuild = policy.image_needs_rebuild(profile);
            if needs_rebuild.unwrap_or(true) {
                let runner_count = policy.runners_for_profile(profile).count();
                if needs_rebuild.is_none() {
                    info!("profile {key}: image may or may not need rebuild");
                } else if self.cached_servo_repo_update.is_some() {
                    info!( "profile {key}: image needs rebuild; cached Servo repo update still running" );
                } else if self.rebuilds.contains_key(key) {
                    info!("profile {key}: image needs rebuild; image rebuild still running");
                } else if runner_count > 0 {
                    info!(
                        runner_count,
                        "profile {key}: image needs rebuild; waiting for runners"
                    );
                } else {
                    info!("profile {key}: image needs rebuild");
                    profiles_needing_rebuild.insert(key, profile);
                }
            }
        }

        // If we’re kicking off image rebuilds, update our cached Servo repo if there are no
        // rebuilds already running that might read from it.
        if self.rebuilds.is_empty()
            && !profiles_needing_rebuild.is_empty()
            && !cached_servo_repo_was_just_updated
            && !TOML.dont_update_cached_servo_repo()
        {
            assert!(self.cached_servo_repo_update.is_none());

            // Kick off a Servo update thread. Don’t start any image rebuild threads.
            info!("Updating our cached Servo repo, before we start image rebuilds");
            self.cached_servo_repo_update = Some(thread::spawn(servo_update_thread));
            return Ok(());
        }

        // Kick off image rebuild threads for profiles needing it.
        for (key, profile) in profiles_needing_rebuild {
            let snapshot_name = Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true);

            let key_for_thread = key.clone();
            let snapshot_name_for_thread = snapshot_name.clone();
            let thread = match profile.image_type {
                settings::profile::ImageType::Rust => {
                    let profile = profile.clone();
                    thread::spawn(move || {
                        rebuild_with_rust(&key_for_thread, profile, &snapshot_name_for_thread)
                    })
                }
            };

            self.rebuilds.insert(
                key.to_owned(),
                Rebuild {
                    thread,
                    snapshot_name: snapshot_name.clone(),
                    guest_name: profile.rebuild_guest_name(&snapshot_name),
                },
            );
        }

        // Reap image rebuild threads, updating the profile on success.
        let mut remaining_rebuilds = BTreeMap::default();
        for (profile_key, rebuild) in take(&mut self.rebuilds) {
            if rebuild.thread.is_finished() {
                match rebuild.thread.join() {
                    Ok(Ok(())) => {
                        info!(profile_key, "Image rebuild thread exited");
                        policy.set_base_image_snapshot(&profile_key, &rebuild.snapshot_name)?;
                    }
                    Ok(Err(report)) => error!(profile_key, %report, "Image rebuild thread error"),
                    Err(panic) => error!(profile_key, ?panic, "Image rebuild thread panic"),
                };
            } else {
                remaining_rebuilds.insert(profile_key, rebuild);
            }
        }
        self.rebuilds.extend(remaining_rebuilds);

        Ok(())
    }

    pub fn rebuild_guest_names(&self) -> BTreeMap<String, String> {
        self.rebuilds
            .iter()
            .map(|(profile_key, rebuild)| (profile_key.clone(), rebuild.guest_name.clone()))
            .collect()
    }
}

#[tracing::instrument]
fn servo_update_thread() -> eyre::Result<()> {
    info!("Starting repo update");

    let main_repo_path = &TOML.main_repo_path;
    let pipe = || |reader| log_output_as_info(reader);
    spawn_with_output!(git -C $main_repo_path reset --hard 2>&1)?.wait_with_pipe(&mut pipe())?;
    spawn_with_output!(git -C $main_repo_path fetch origin main 2>&1)?
        .wait_with_pipe(&mut pipe())?;
    spawn_with_output!(git -C $main_repo_path switch --detach FETCH_HEAD 2>&1)?
        .wait_with_pipe(&mut pipe())?;
    // Allow git-clone(1) <https://stackoverflow.com/a/19707416>
    spawn_with_output!(git -C $main_repo_path update-server-info 2>&1)?
        .wait_with_pipe(&mut pipe())?;

    Ok(())
}

#[tracing::instrument(skip(profile, snapshot_name))]
fn rebuild_with_rust(
    profile_key: &str,
    profile: Profile,
    snapshot_name: &str,
) -> Result<(), eyre::Error> {
    info!(?snapshot_name, "Starting image rebuild");

    let base_images_path = create_template_or_rebuild_images_dir(&profile)?;

    match match &*profile.profile_name {
        "servo-macos13" => macos13::rebuild(
            &base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(2000),
        ),
        "servo-macos14" => macos13::rebuild(
            &base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(2000),
        ),
        "servo-macos15" => macos13::rebuild(
            &base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(2000),
        ),
        "servo-ubuntu2204" => ubuntu2204::rebuild(
            &base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(2000),
        ),
        "servo-ubuntu2204-bench" => ubuntu2204::rebuild(
            &base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(1000),
        ),
        "base-ubuntu2204" => ubuntu2204::rebuild(
            &base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(20),
            Duration::from_secs(90),
        ),
        "servo-ubuntu2204-wpt" => ubuntu2204::rebuild(
            &base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(2000),
        ),
        "servo-windows10" => windows10::rebuild(
            &base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(3000),
        ),
        other => todo!("Rebuild not yet implemented: {other}"),
    } {
        result @ Ok(()) => {
            prune_templates(&profile)?;
            result
        }
        Err(error) => {
            warn!(?error, "Image rebuild error");
            delete_template(&profile, snapshot_name)?;
            Err(error)
        }
    }
}

pub fn delete_template(profile: &Profile, snapshot_name: &str) -> eyre::Result<()> {
    match &*profile.profile_name {
        "servo-macos13" => macos13::delete_template(profile, snapshot_name),
        "servo-macos14" => macos13::delete_template(profile, snapshot_name),
        "servo-macos15" => macos13::delete_template(profile, snapshot_name),
        "servo-ubuntu2204" => ubuntu2204::delete_template(profile, snapshot_name),
        "servo-ubuntu2204-bench" => ubuntu2204::delete_template(profile, snapshot_name),
        "base-ubuntu2204" => ubuntu2204::delete_template(profile, snapshot_name),
        "servo-ubuntu2204-wpt" => ubuntu2204::delete_template(profile, snapshot_name),
        "servo-windows10" => windows10::delete_template(profile, snapshot_name),
        other => todo!("Image pruning not yet implemented: {other}"),
    }
}

pub fn register_runner(profile: &Profile, runner_guest_name: &str) -> eyre::Result<String> {
    match &*profile.profile_name {
        "servo-macos13" => macos13::register_runner(profile, runner_guest_name),
        "servo-macos14" => macos13::register_runner(profile, runner_guest_name),
        "servo-macos15" => macos13::register_runner(profile, runner_guest_name),
        "servo-ubuntu2204" => ubuntu2204::register_runner(profile, runner_guest_name),
        "servo-ubuntu2204-bench" => ubuntu2204::register_runner(profile, runner_guest_name),
        "base-ubuntu2204" => ubuntu2204::register_runner(profile, runner_guest_name),
        "servo-ubuntu2204-wpt" => ubuntu2204::register_runner(profile, runner_guest_name),
        "servo-windows10" => windows10::register_runner(profile, runner_guest_name),
        other => todo!("Runner registration not yet implemented: {other}"),
    }
}

pub fn create_runner(
    profile: &Profile,
    snapshot_name: &str,
    runner_guest_name: &str,
    runner_id: usize,
) -> eyre::Result<String> {
    match &*profile.profile_name {
        "servo-macos13" => {
            macos13::create_runner(profile, snapshot_name, runner_guest_name, runner_id)
        }
        "servo-macos14" => {
            macos13::create_runner(profile, snapshot_name, runner_guest_name, runner_id)
        }
        "servo-macos15" => {
            macos13::create_runner(profile, snapshot_name, runner_guest_name, runner_id)
        }
        "servo-ubuntu2204" => {
            ubuntu2204::create_runner(profile, snapshot_name, runner_guest_name, runner_id)
        }
        "servo-ubuntu2204-bench" => {
            ubuntu2204::create_runner(profile, snapshot_name, runner_guest_name, runner_id)
        }
        "base-ubuntu2204" => {
            ubuntu2204::create_runner(profile, snapshot_name, runner_guest_name, runner_id)
        }
        "servo-ubuntu2204-wpt" => {
            ubuntu2204::create_runner(profile, snapshot_name, runner_guest_name, runner_id)
        }
        "servo-windows10" => {
            windows10::create_runner(profile, snapshot_name, runner_guest_name, runner_id)
        }
        other => todo!("Runner creation not yet implemented: {other}"),
    }
}

pub fn destroy_runner(
    profile: &Profile,
    runner_guest_name: &str,
    runner_id: usize,
) -> eyre::Result<()> {
    match &*profile.profile_name {
        "servo-macos13" => macos13::destroy_runner(runner_guest_name, runner_id),
        "servo-macos14" => macos13::destroy_runner(runner_guest_name, runner_id),
        "servo-macos15" => macos13::destroy_runner(runner_guest_name, runner_id),
        "servo-ubuntu2204" => ubuntu2204::destroy_runner(runner_guest_name, runner_id),
        "servo-ubuntu2204-bench" => ubuntu2204::destroy_runner(runner_guest_name, runner_id),
        "base-ubuntu2204" => ubuntu2204::destroy_runner(runner_guest_name, runner_id),
        "servo-ubuntu2204-wpt" => ubuntu2204::destroy_runner(runner_guest_name, runner_id),
        "servo-windows10" => windows10::destroy_runner(runner_guest_name, runner_id),
        other => todo!("Runner destruction not yet implemented: {other}"),
    }
}

pub(self) fn create_template_or_rebuild_images_dir(profile: &Profile) -> eyre::Result<PathBuf> {
    let base_images_path = template_or_rebuild_images_path(profile);
    debug!(?base_images_path, "Creating base images subdirectory");
    create_dir_all(&base_images_path)?;

    Ok(base_images_path)
}

pub(self) fn create_runner_images_dir() -> eyre::Result<PathBuf> {
    let runner_images_path = runner_images_path();
    debug!(?runner_images_path, "Creating runner images directory");
    create_dir_all(&runner_images_path)?;

    Ok(runner_images_path)
}

pub(self) fn prune_templates(profile: &Profile) -> eyre::Result<()> {
    // Build a sorted list of template guest names for this profile.
    let mut snapshot_names = vec![];
    for template_guest_name in list_template_guests()? {
        if let Ok((profile_key, snapshot_name)) = parse_template_guest_name(&template_guest_name) {
            if profile_key == profile.profile_name {
                snapshot_names.push(snapshot_name.to_owned());
            }
        } else {
            undefine_libvirt_guest(&template_guest_name)?;
        }
    }
    snapshot_names.sort();

    // Delete all of those templates, except the three most recent.
    // Since the snapshot names are RFC 3339 timestamps, we can use the sorted order (until year 10000).
    let keep_snapshots = snapshot_names.clone().into_iter().rev().take(3);
    let delete_snapshots = snapshot_names.iter().rev().skip(3);
    for snapshot_name in delete_snapshots {
        delete_template(profile, snapshot_name)?;
    }

    // Now delete any files that are not associated with a known snapshot.
    let keep_snapshots = keep_snapshots.collect::<BTreeSet<_>>();
    let base_images_path = template_or_rebuild_images_path(profile);
    info!(?base_images_path, "Pruning base image files");
    create_dir_all(&base_images_path)?;

    for entry in read_dir(&base_images_path)? {
        let filename = entry?.file_name();
        let filename = filename.to_str().ok_or_eyre("Unsupported path")?;
        if let Some((_base, snapshot_name)) = filename.split_once("@") {
            if !keep_snapshots.contains(snapshot_name) {
                delete_template_or_rebuild_image_file(profile, filename);
            }
        } else {
            delete_template_or_rebuild_image_file(profile, filename);
        }
    }

    Ok(())
}

pub(self) fn delete_template_or_rebuild_image_file(profile: &Profile, filename: &str) {
    let base_images_path = template_or_rebuild_images_path(profile);
    let path = base_images_path.join(filename);
    info!(?path, "Deleting");
    if let Err(error) = remove_file(&path) {
        warn!(?path, ?error, "Failed to delete");
    }
}

pub(self) fn create_disk_image<'icp>(
    base_images_path: impl AsRef<Path>,
    snapshot_name: &str,
    size: ByteSize,
    initial_contents_path: impl Into<Option<&'icp Path>>,
) -> eyre::Result<PathBuf> {
    let base_images_path = base_images_path.as_ref();
    let base_image_filename = format!("base.img@{snapshot_name}");
    let base_image_path = base_images_path.join(&base_image_filename);

    info!(?base_image_path, "Creating base image file");
    let mut base_image_file = if let Some(from) = initial_contents_path.into() {
        reflink_or_copy_with_warning(from, &base_image_path)?;

        // Copying out of the nix store yields a file with mode 444 (read only). Make sure the file is writable.
        set_permissions(&base_image_path, PermissionsExt::from_mode(0o644))?;

        File::options().write(true).open(&base_image_path)?
    } else {
        File::create_new(&base_image_path)?
    };

    let delta = size
        .0
        .checked_sub(base_image_file.stream_position()?)
        .ok_or_eyre("`size` is smaller than `initial_contents`")?;

    // If `size` is bigger than `initial_contents`, extend the file quickly by seeking and writing at least one byte.
    // We could write all the zeros, but this is not necessarily helpful since ZFS is a COW file system.
    if let Some(delta) = delta.checked_sub(1) {
        base_image_file.seek_relative(delta.try_into()?)?;
        base_image_file.write_all(&[0])?;
    }

    Ok(base_image_path)
}

pub(self) fn define_libvirt_guest(
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

pub(self) fn libvirt_change_media(
    guest_name: &str,
    cdrom_images: &[CdromImage],
) -> eyre::Result<()> {
    for CdromImage { target_dev, path } in cdrom_images {
        run_cmd!(virsh change-media -- $guest_name $target_dev $path)?;
    }

    Ok(())
}

pub(self) fn undefine_libvirt_guest(guest_name: &str) -> eyre::Result<()> {
    if run_cmd!(virsh domstate -- $guest_name).is_ok() {
        // FIXME make this idempotent in a less noisy way?
        let _ = run_cmd!(virsh destroy -- $guest_name);
        run_cmd!(virsh undefine --nvram -- $guest_name)?;
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
pub fn start_libvirt_guest(guest_name: &str) -> eyre::Result<()> {
    info!(?guest_name, "Starting guest");
    run_cmd!(virsh start -- $guest_name)?;

    Ok(())
}

pub(self) fn wait_for_guest(guest_name: &str, timeout: Duration) -> eyre::Result<()> {
    let timeout = timeout.as_secs();
    info!("Waiting for guest to shut down (max {timeout} seconds)");
    if !run_cmd!(time virsh event --timeout $timeout -- $guest_name lifecycle).is_ok() {
        bail!("`virsh event` failed or timed out!");
    }
    for _ in 0..100 {
        if run_fun!(virsh domstate -- $guest_name)?.trim_ascii() == "shut off" {
            return Ok(());
        }
    }

    bail!("Guest did not shut down as expected")
}

pub(self) fn rename_guest(old_guest_name: &str, new_guest_name: &str) -> eyre::Result<()> {
    run_cmd!(virsh domrename -- $old_guest_name $new_guest_name)?;
    Ok(())
}
