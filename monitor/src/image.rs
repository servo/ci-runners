pub mod macos13;
pub mod ubuntu2204;
pub mod windows10;

use core::str;
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{create_dir_all, File},
    io::{Read, Seek, Write},
    mem::take,
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
    time::Duration,
};

use bytesize::{ByteSize, MIB};
use chrono::{SecondsFormat, Utc};
use cmd_lib::{run_cmd, spawn_with_output};
use jane_eyre::eyre::{self, bail, OptionExt};
use tracing::{error, info, warn};

use crate::{
    profile::{Profile, Profiles},
    runner::Runners,
    shell::log_output_as_info,
    DOTENV,
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
}

impl Rebuilds {
    pub fn run(&mut self, profiles: &mut Profiles, runners: &Runners) -> eyre::Result<()> {
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
        for (key, profile) in profiles.iter() {
            let needs_rebuild = profiles.image_needs_rebuild(profile);
            if needs_rebuild.unwrap_or(true) {
                let runner_count = profile.runners(&runners).count();
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
                crate::profile::ImageType::Rust => {
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
                        profiles.set_base_image_snapshot(&profile_key, &rebuild.snapshot_name)?;
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
}

#[tracing::instrument]
fn servo_update_thread() -> eyre::Result<()> {
    info!("Starting repo update");

    let main_repo_path = &DOTENV.main_repo_path;
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

    let base_images_path = create_base_images_dir(&profile)?;
    let base_vm_name = &profile.base_vm_name;
    if run_cmd!(virsh domstate -- $base_vm_name).is_ok() {
        // FIXME make this idempotent in a less noisy way?
        let _ = run_cmd!(virsh destroy -- $base_vm_name);
        run_cmd!(virsh undefine --nvram -- $base_vm_name)?;
    }

    match &*profile.configuration_name {
        "macos13" => macos13::rebuild(
            base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(2000),
        ),
        "ubuntu2204" => ubuntu2204::rebuild(
            base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(2000),
        ),
        "ubuntu2204-rust" => ubuntu2204::rebuild(
            base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(20),
            Duration::from_secs(90),
        ),
        "ubuntu2204-wpt" => ubuntu2204::rebuild(
            base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(2000),
        ),
        "windows10" => windows10::rebuild(
            base_images_path,
            &profile,
            snapshot_name,
            ByteSize::gib(90),
            Duration::from_secs(3000),
        ),
        other => todo!("Rebuild not yet implemented: {other}"),
    }
}

pub fn register_runner(profile: &Profile, vm_name: &str) -> eyre::Result<String> {
    match &*profile.configuration_name {
        "macos13" => macos13::register_runner(profile, vm_name),
        "ubuntu2204" => ubuntu2204::register_runner(profile, vm_name),
        "ubuntu2204-rust" => ubuntu2204::register_runner(profile, vm_name),
        "ubuntu2204-wpt" => ubuntu2204::register_runner(profile, vm_name),
        "windows10" => windows10::register_runner(profile, vm_name),
        other => todo!("Runner registration not yet implemented: {other}"),
    }
}

pub fn create_runner(profile: &Profile, vm_name: &str) -> eyre::Result<()> {
    match &*profile.configuration_name {
        "macos13" => macos13::create_runner(profile, vm_name),
        "ubuntu2204" => ubuntu2204::create_runner(profile, vm_name),
        "ubuntu2204-rust" => ubuntu2204::create_runner(profile, vm_name),
        "ubuntu2204-wpt" => ubuntu2204::create_runner(profile, vm_name),
        "windows10" => windows10::create_runner(profile, vm_name),
        other => todo!("Runner creation not yet implemented: {other}"),
    }
}

pub fn destroy_runner(profile: &Profile, vm_name: &str) -> eyre::Result<()> {
    match &*profile.configuration_name {
        "macos13" => macos13::destroy_runner(vm_name),
        "ubuntu2204" => ubuntu2204::destroy_runner(vm_name),
        "ubuntu2204-rust" => ubuntu2204::destroy_runner(vm_name),
        "ubuntu2204-wpt" => ubuntu2204::destroy_runner(vm_name),
        "windows10" => windows10::destroy_runner(vm_name),
        other => todo!("Runner destruction not yet implemented: {other}"),
    }
}

pub(self) fn create_base_images_dir(profile: &Profile) -> eyre::Result<PathBuf> {
    let base_images_path = profile.base_images_path();
    info!(?base_images_path, "Creating libvirt images subdirectory");
    create_dir_all(&base_images_path)?;

    Ok(base_images_path)
}

pub(self) fn create_disk_image(
    base_images_path: impl AsRef<Path>,
    snapshot_name: &str,
    size: ByteSize,
    mut initial_contents: impl Read,
) -> eyre::Result<PathBuf> {
    let base_images_path = base_images_path.as_ref();
    let base_image_filename = format!("base.img@{snapshot_name}");
    let base_image_path = base_images_path.join(&base_image_filename);
    info!(?base_image_path, "Creating base image file");
    let mut base_image_file = File::create_new(&base_image_path)?;
    info!("Writing base image file: {size} left");
    std::io::copy(&mut initial_contents, &mut base_image_file)?;

    let size = size
        .0
        .checked_sub(base_image_file.stream_position()?)
        .ok_or_eyre("`size` is smaller than `initial_contents`")?;
    let mut size = ByteSize(size);

    // If needed, do one write of less than 1 MiB, to align the image to 1 MiB.
    if size.0 % MIB > 0 {
        info!("Writing base image file: {size} left");
        let len = size.0 / MIB * MIB + MIB - size.0;
        let len_usize = len.try_into().expect("Guaranteed by platform");
        base_image_file.write_all(&vec![0u8; len_usize])?;
        size.0 -= len;
    }
    // Continue writing 1 MiB at a time, logging progress every 1 GiB.
    while size.0 >= MIB {
        info!("Writing base image file: {size} left");
        let mut limit = ByteSize::gib(1);
        while size.0 >= MIB && limit.0 > 0 {
            let len_usize = MIB.try_into().expect("Guaranteed by platform");
            base_image_file.write_all(&vec![0u8; len_usize])?;
            size.0 -= MIB;
            limit.0 -= MIB;
        }
    }
    // If needed, do one write of less than 1 MiB, to finish the image.
    if size.0 > 0 {
        info!("Writing base image file: {size} left");
        let len = size.0 / MIB * MIB + MIB - size.0;
        let len_usize = len.try_into().expect("Guaranteed by platform");
        base_image_file.write_all(&vec![0u8; len_usize])?;
        size.0 -= len;
    }

    Ok(base_image_path)
}

pub(self) fn define_libvirt_guest(
    base_vm_name: &str,
    guest_xml_path: impl AsRef<Path>,
    args: &[&dyn AsRef<OsStr>],
) -> eyre::Result<()> {
    // This dance is needed to randomise the MAC address of the guest.
    let guest_xml_path = guest_xml_path.as_ref();
    let args = args.iter().map(|x| x.as_ref()).collect::<Vec<_>>();
    run_cmd!(virsh define -- $guest_xml_path)?;
    run_cmd!(virt-clone --preserve-data --check path_in_use=off -o $base_vm_name.init -n $base_vm_name $[args])?;
    run_cmd!(virsh undefine -- $base_vm_name.init)?;

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
pub fn start_libvirt_guest(guest_name: &str, cdrom_images: &[CdromImage]) -> eyre::Result<()> {
    info!("Starting guest");
    // FIXME: This dance is only needed because `virt-clone -f` ignores cdrom drives.
    run_cmd!(virsh start --paused -- $guest_name)?;
    for CdromImage { target_dev, path } in cdrom_images {
        run_cmd!(virsh change-media -- $guest_name $target_dev $path)?;
    }
    run_cmd!(virsh resume -- $guest_name)?;

    Ok(())
}

pub(self) fn wait_for_guest(base_vm_name: &str, timeout: Duration) -> eyre::Result<()> {
    let timeout = timeout.as_secs();
    info!("Waiting for guest to shut down (max {timeout} seconds)"); // normally ~37 seconds
    if !run_cmd!(time virsh event --timeout $timeout -- $base_vm_name lifecycle).is_ok() {
        bail!("`virsh event` failed or timed out!");
    }

    Ok(())
}
