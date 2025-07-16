use std::{
    collections::BTreeMap,
    fs::{create_dir, read_link, File},
    io::{Read, Write},
    net::Ipv4Addr,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cfg_if::cfg_if;
use jane_eyre::eyre::{self, bail, Context, OptionExt};
use mktemp::Temp;
use serde::Serialize;
use settings::{
    profile::{ImageType, Profile},
    DOTENV, TOML,
};
use tracing::{debug, info, warn};

use crate::{
    data::{get_profile_configuration_path, get_profile_data_path, get_runner_data_path},
    image::{create_runner, destroy_runner, register_runner},
    libvirt::{get_ipv4_address, update_screenshot},
    runner::{Runner, Runners, Status},
};

#[derive(Debug)]
pub struct Policy {
    profiles: BTreeMap<String, Profile>,
    base_image_snapshots: BTreeMap<String, String>,
    ipv4_addresses: BTreeMap<String, Option<Ipv4Addr>>,
    runners: Option<Runners>,
}

#[derive(Debug, PartialEq, Default)]
pub struct RunnerChanges {
    pub unregister_and_destroy_runner_ids: Vec<usize>,
    pub create_counts_by_profile_key: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
pub struct RunnerCounts {
    pub target: usize,
    pub healthy: usize,
    pub started_or_crashed: usize,
    pub idle: usize,
    pub reserved: usize,
    pub busy: usize,
    pub excess_idle: usize,
    pub wanted: usize,
    pub image_age: Option<Duration>,
}

impl Policy {
    pub fn new(profiles: BTreeMap<String, Profile>) -> Self {
        Self {
            profiles,
            base_image_snapshots: BTreeMap::default(),
            ipv4_addresses: BTreeMap::default(),
            runners: None,
        }
    }

    pub fn read_base_image_snapshots(&mut self) -> eyre::Result<()> {
        for (profile_key, profile) in self.profiles.iter() {
            if let Some(base_image_snapshot) = read_base_image_snapshot(profile)? {
                self.base_image_snapshots
                    .insert(profile_key.clone(), base_image_snapshot);
            }
        }

        Ok(())
    }

    pub fn profiles(&self) -> impl Iterator<Item = (&String, &Profile)> {
        self.profiles.iter()
    }

    pub fn profile(&self, profile_key: &str) -> Option<&Profile> {
        self.profiles.get(profile_key)
    }

    pub fn base_image_snapshot(&self, profile_key: &str) -> Option<&String> {
        self.base_image_snapshots.get(profile_key)
    }

    pub fn set_runners(&mut self, runners: Runners) {
        self.runners = Some(runners);
    }

    pub fn compute_runner_changes(&self) -> RunnerChanges {
        let mut result = RunnerChanges::default();

        // Invalid => unregister and destroy
        // DoneOrUnregistered => destroy (no need to unregister)
        // StartedOrCrashed and too old => unregister and destroy
        // Reserved for too long => unregister and destroy
        // Idle or Busy => bleed off excess Idle runners
        let invalid = self
            .runners()
            .filter(|(_id, runner)| runner.status() == Status::Invalid);
        let done_or_unregistered = self
            .runners()
            .filter(|(_id, runner)| runner.status() == Status::DoneOrUnregistered)
            // Don’t destroy unregistered runners if we aren’t registering them in the first place.
            .filter(|_| !DOTENV.dont_register_runners);
        let started_or_crashed_and_too_old = self.runners().filter(|(_id, runner)| {
            runner.status() == Status::StartedOrCrashed
                && runner
                    .age()
                    .map_or(true, |age| age > DOTENV.monitor_start_timeout)
        });
        let reserved_for_too_long = self.runners().filter(|(_id, runner)| {
            runner.status() == Status::Reserved
                && runner
                    .reserved_since()
                    .ok()
                    .flatten()
                    .map_or(true, |duration| duration > DOTENV.monitor_reserve_timeout)
        });
        let excess_idle_runners = self.profiles().flat_map(|(_key, profile)| {
            self.idle_runners_for_profile(profile)
                .take(self.excess_idle_runner_count(profile))
        });
        for (&id, _runner) in invalid
            .chain(done_or_unregistered)
            .chain(started_or_crashed_and_too_old)
            .chain(reserved_for_too_long)
            .chain(excess_idle_runners)
        {
            result.unregister_and_destroy_runner_ids.push(id);
        }

        let profile_wanted_counts = self
            .profiles()
            .map(|(key, profile)| (key, self.wanted_runner_count(profile)));
        for (profile_key, wanted_count) in profile_wanted_counts {
            result
                .create_counts_by_profile_key
                .insert(profile_key.clone(), wanted_count);
        }

        result
    }

    pub fn create_runner(&self, profile: &Profile, id: usize) -> eyre::Result<()> {
        if self.base_image_snapshot(&profile.base_vm_name).is_none() {
            bail!(
                "Tried to create runner, but profile has no base image snapshot (profile {})",
                profile.base_vm_name
            );
        };
        info!(runner_id = id, profile.base_vm_name, "Creating runner");
        match profile.image_type {
            ImageType::Rust => {
                let base_vm_name = &profile.base_vm_name;
                create_dir(get_runner_data_path(id, None)?)?;
                let mut runner_toml =
                    File::create_new(get_runner_data_path(id, Path::new("runner.toml"))?)?;
                writeln!(runner_toml, r#"image_type = "Rust""#)?;
                symlink(
                    get_profile_configuration_path(profile, Path::new("boot-script"))?,
                    get_runner_data_path(id, Path::new("boot-script"))?,
                )?;
                let vm_name = format!("{base_vm_name}.{id}");
                if !DOTENV.dont_register_runners {
                    let mut github_api_registration = File::create_new(get_runner_data_path(
                        id,
                        Path::new("github-api-registration"),
                    )?)?;
                    github_api_registration
                        .write_all(register_runner(profile, &vm_name)?.as_bytes())?;
                }
                create_runner(profile, &vm_name)?;

                Ok(())
            }
        }
    }

    pub fn destroy_runner(&self, profile: &Profile, id: usize) -> eyre::Result<()> {
        info!(runner_id = id, profile.base_vm_name, "Destroying runner");
        match profile.image_type {
            ImageType::Rust => {
                let vm_name = format!("{}.{id}", profile.base_vm_name);
                destroy_runner(profile, &vm_name)?;
                Ok(())
            }
        }
    }

    pub fn runner_counts(&self, profile: &Profile) -> RunnerCounts {
        RunnerCounts {
            target: self.target_runner_count(profile),
            healthy: self.healthy_runner_count(profile),
            started_or_crashed: self.started_or_crashed_runner_count(profile),
            idle: self.idle_runner_count(profile),
            reserved: self.reserved_runner_count(profile),
            busy: self.busy_runner_count(profile),
            excess_idle: self.excess_idle_runner_count(profile),
            wanted: self.wanted_runner_count(profile),
            image_age: self.image_age(profile).ok().flatten(),
        }
    }

    pub fn target_runner_count(&self, profile: &Profile) -> usize {
        if DOTENV.dont_create_runners || self.image_needs_rebuild(profile).unwrap_or(true) {
            0
        } else {
            profile.target_count
        }
    }

    pub fn healthy_runner_count(&self, profile: &Profile) -> usize {
        self.started_or_crashed_runner_count(profile)
            + self.idle_runner_count(profile)
            + self.reserved_runner_count(profile)
            + self.busy_runner_count(profile)
            + self.done_or_unregistered_runner_count(profile)
    }

    pub fn started_or_crashed_runner_count(&self, profile: &Profile) -> usize {
        self.runners_for_profile(profile)
            .filter(|(_id, runner)| runner.status() == Status::StartedOrCrashed)
            .count()
    }

    pub fn idle_runner_count(&self, profile: &Profile) -> usize {
        self.runners_for_profile(profile)
            .filter(|(_id, runner)| runner.status() == Status::Idle)
            .count()
    }

    pub fn reserved_runner_count(&self, profile: &Profile) -> usize {
        self.runners_for_profile(profile)
            .filter(|(_id, runner)| runner.status() == Status::Reserved)
            .count()
    }

    pub fn busy_runner_count(&self, profile: &Profile) -> usize {
        self.runners_for_profile(profile)
            .filter(|(_id, runner)| runner.status() == Status::Busy)
            .count()
    }

    pub fn done_or_unregistered_runner_count(&self, profile: &Profile) -> usize {
        self.runners_for_profile(profile)
            .filter(|(_id, runner)| runner.status() == Status::DoneOrUnregistered)
            .count()
    }

    pub fn excess_idle_runner_count(&self, profile: &Profile) -> usize {
        // Healthy runners beyond the target count are excess runners.
        let excess = if self.healthy_runner_count(profile) > self.target_runner_count(profile) {
            self.healthy_runner_count(profile) - self.target_runner_count(profile)
        } else {
            0
        };

        // But we can only destroy idle runners, since busy runners have work to do.
        excess.min(self.idle_runner_count(profile))
    }

    pub fn wanted_runner_count(&self, profile: &Profile) -> usize {
        // Healthy runners below the target count are wanted runners.
        if self.target_runner_count(profile) > self.healthy_runner_count(profile) {
            self.target_runner_count(profile) - self.healthy_runner_count(profile)
        } else {
            0
        }
    }

    /// Returns whether the image definitely needs to be rebuilt or not, or None
    /// if we don’t know.
    pub fn image_needs_rebuild(&self, profile: &Profile) -> Option<bool> {
        if profile.target_count == 0 {
            // Profiles with zero target_count may have been set to zero because
            // there is insufficient hugepages space to run them
            return Some(false);
        }

        // If we fail to get the image age, err on the side of caution
        let image_age = match self.image_age(profile) {
            Ok(result) => result,
            Err(error) => {
                warn!(?error, "Failed to get image age");
                return None;
            }
        };

        // If the profile has no image age, we may need to build its image for the first time
        Some(image_age.map_or(true, |age| age > TOML.base_image_max_age()))
    }

    pub fn image_age(&self, profile: &Profile) -> eyre::Result<Option<Duration>> {
        let Some(base_image_snapshot) = self.base_image_snapshot(&profile.base_vm_name) else {
            return Ok(None);
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .wrap_err("Failed to get current time")?;
        let creation_time = match profile.image_type {
            ImageType::Rust => {
                let base_image_path = base_image_path(profile, &**base_image_snapshot);
                let Some(mtime) = base_image_mtime(profile, &base_image_path) else {
                    return Ok(None);
                };
                match mtime.duration_since(UNIX_EPOCH) {
                    Ok(result) => result,
                    Err(error) => {
                        debug!(
                            profile.base_vm_name,
                            ?base_image_path,
                            ?error,
                            "Failed to calculate image age"
                        );
                        return Ok(None);
                    }
                }
            }
        };

        Ok(Some(now - creation_time))
    }

    pub fn set_base_image_snapshot(
        &mut self,
        profile_key: &str,
        base_image_snapshot: &str,
    ) -> eyre::Result<()> {
        self.base_image_snapshots
            .insert(profile_key.to_owned(), base_image_snapshot.to_owned());

        Ok(())
    }

    pub fn update_ipv4_addresses_for_profile_guests(&mut self) {
        for (key, profile) in self.profiles.iter() {
            let ipv4_address = get_ipv4_address(&profile.base_vm_name);
            let entry = self.ipv4_addresses.entry(key.clone()).or_default();
            if ipv4_address != *entry {
                info!(
                    "IPv4 address changed for profile guest {key}: {:?} -> {:?}",
                    *entry, ipv4_address
                );
            }
            *entry = ipv4_address;
        }
    }

    pub fn boot_script_for_profile_guest(
        &self,
        remote_addr: web::auth::RemoteAddr,
    ) -> eyre::Result<Option<String>> {
        for (key, ipv4_address) in self.ipv4_addresses.iter() {
            if let Some(ipv4_address) = ipv4_address {
                if remote_addr == *ipv4_address {
                    let profile = self.profiles.get(key).expect("Guaranteed by Profiles impl");
                    let path = get_profile_configuration_path(profile, Path::new("boot-script"))?;
                    let mut result = String::default();
                    File::open(path)?.read_to_string(&mut result)?;
                    return Ok(Some(result));
                }
            }
        }

        Ok(None)
    }

    pub fn runners(&self) -> impl Iterator<Item = (&usize, &Runner)> {
        self.runners.iter().flat_map(|runners| runners.iter())
    }

    pub fn runner(&self, id: usize) -> Option<&Runner> {
        self.runners.as_ref().and_then(|runners| runners.get(id))
    }

    pub fn runners_for_profile<'s, 'p: 's>(
        &'s self,
        profile: &'p Profile,
    ) -> impl Iterator<Item = (&'s usize, &'s Runner)> {
        self.runners_for_profile_key(&profile.base_vm_name)
    }

    pub fn runners_for_profile_key<'s, 'p: 's>(
        &'s self,
        profile_key: &'p str,
    ) -> impl Iterator<Item = (&'s usize, &'s Runner)> {
        self.runners
            .iter()
            .flat_map(|runners| runners.by_profile(profile_key))
    }

    pub fn idle_runners_for_profile<'s, 'p: 's>(
        &'s self,
        profile: &'p Profile,
    ) -> impl Iterator<Item = (&'s usize, &'s Runner)> {
        self.runners_for_profile(profile)
            .filter(|(_id, runner)| runner.status() == Status::Idle)
    }

    pub fn update_screenshots(&self) {
        if let Some(runners) = self.runners.as_ref() {
            runners.update_screenshots();
        }
        for (_key, profile) in self.profiles() {
            if let Err(error) = self.try_update_screenshot(profile) {
                debug!(
                    profile.base_vm_name,
                    ?error,
                    "Failed to update screenshot for profile guest"
                );
            }
        }
    }

    fn try_update_screenshot(&self, profile: &Profile) -> eyre::Result<()> {
        let output_dir = get_profile_data_path(&profile.base_vm_name, None)?;
        update_screenshot(&profile.base_vm_name, &output_dir)?;

        Ok(())
    }
}

/// Proxies to [Runner].
impl Policy {
    pub fn reserve_runner(
        &self,
        id: usize,
        unique_id: &str,
        qualified_repo: &str,
        run_id: &str,
    ) -> eyre::Result<()> {
        let Some(runners) = self.runners.as_ref() else {
            bail!("Policy has no Runners!");
        };

        runners.reserve_runner(id, unique_id, qualified_repo, run_id)
    }

    pub fn unregister_runner(&self, id: usize) -> eyre::Result<()> {
        let Some(runners) = self.runners.as_ref() else {
            bail!("Policy has no Runners!");
        };

        runners.unregister_runner(id)
    }

    pub fn screenshot_runner(&self, id: usize) -> eyre::Result<Temp> {
        let Some(runners) = self.runners.as_ref() else {
            bail!("Policy has no Runners!");
        };

        runners.screenshot_runner(id)
    }

    pub fn github_jitconfig(
        &self,
        remote_addr: web::auth::RemoteAddr,
    ) -> eyre::Result<Option<&str>> {
        let Some(runners) = self.runners.as_ref() else {
            bail!("Policy has no Runners!");
        };

        runners.github_jitconfig(remote_addr)
    }

    pub fn update_ipv4_addresses_for_runner_guests(&mut self) -> eyre::Result<()> {
        let Some(runners) = self.runners.as_mut() else {
            bail!("Policy has no Runners!");
        };

        runners.update_ipv4_addresses();

        Ok(())
    }

    pub fn boot_script_for_runner_guest(
        &self,
        remote_addr: web::auth::RemoteAddr,
    ) -> eyre::Result<Option<String>> {
        let Some(runners) = self.runners.as_ref() else {
            bail!("Policy has no Runners!");
        };

        runners.boot_script(remote_addr)
    }
}

pub fn base_images_path(profile: &Profile) -> PathBuf {
    Path::new("/var/lib/libvirt/images/base").join(&profile.base_vm_name)
}

pub fn base_image_path<'snap>(
    profile: &Profile,
    snapshot_name: impl Into<Option<&'snap str>>,
) -> PathBuf {
    if let Some(snapshot_name) = snapshot_name.into() {
        base_images_path(profile).join(format!("base.img@{snapshot_name}"))
    } else {
        base_images_path(profile).join("base.img")
    }
}

fn read_base_image_snapshot(profile: &Profile) -> eyre::Result<Option<String>> {
    let path = base_image_path(profile, None);
    if let Ok(path) = read_link(path) {
        let path = path.to_str().ok_or_eyre("Symlink target is unsupported")?;
        let (_, snapshot_name) = path
            .split_once("@")
            .ok_or_eyre("Symlink target has no snapshot name")?;
        return Ok(Some(snapshot_name.to_owned()));
    }

    Ok(None)
}

cfg_if! {
    if #[cfg(not(test))] {
        fn base_image_mtime(profile: &Profile, base_image_path: impl AsRef<Path>) -> Option<SystemTime> {
            let base_image_path = base_image_path.as_ref();
            let metadata = match std::fs::metadata(&base_image_path) {
                Ok(result) => result,
                Err(error) => {
                    debug!(
                        profile.base_vm_name,
                        ?base_image_path,
                        ?error,
                        "Failed to get file metadata"
                    );
                    return None;
                }
            };

            Some(metadata.modified().expect("Guaranteed by platform"))
        }
    } else {
        use std::cell::RefCell;

        thread_local! {
            static BASE_IMAGE_MTIMES: RefCell<BTreeMap<String, SystemTime>> = RefCell::new(BTreeMap::new());
        }

        fn base_image_mtime(_profile: &Profile, base_image_path: impl AsRef<Path>) -> Option<SystemTime> {
            let base_image_path = base_image_path.as_ref().to_str().expect("Unsupported path");

            BASE_IMAGE_MTIMES.with_borrow(|mtimes| mtimes.get(base_image_path).copied())
        }

        fn set_base_image_mtime_for_test(base_image_path: &str, mtime: impl Into<Option<SystemTime>>) {
            BASE_IMAGE_MTIMES.with_borrow_mut(|mtimes| {
                if let Some(mtime) = mtime.into() {
                    mtimes.insert(base_image_path.to_owned(), mtime);
                } else {
                    mtimes.remove(base_image_path);
                }
            });
        }
    }
}
