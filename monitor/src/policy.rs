use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{create_dir, read_link, File},
    io::{Read, Write},
    net::Ipv4Addr,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cfg_if::cfg_if;
use itertools::Itertools;
use jane_eyre::eyre::{self, bail, Context, OptionExt};
use mktemp::Temp;
use serde::Serialize;
use settings::{
    profile::{ImageType, Profile},
    units::MemorySize,
    DOTENV, TOML,
};
use tracing::{debug, info, info_span, warn};

use crate::{
    data::{get_profile_configuration_path, get_profile_data_path, get_runner_data_path},
    github::unregister_runner,
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
    current_override: Option<Override>,
}

/// Overrides compromise on some of our usual guarantees:
/// - We may agree to start a runner that we ultimately can’t start or reserve
/// - We may forget our override if the monitor is restarted (for now at least)
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Override {
    pub profile_override_counts: BTreeMap<String, usize>,
    pub profile_target_counts: BTreeMap<String, usize>,
    pub actual_runner_ids_by_profile_key: BTreeMap<String, BTreeSet<usize>>,
}

#[derive(Debug, PartialEq, Default)]
pub struct RunnerChanges {
    pub unregister_and_destroy_runner_ids: Vec<usize>,
    pub create_counts_by_profile_key: BTreeMap<String, usize>,
}
impl RunnerChanges {
    pub fn is_empty(&self) -> bool {
        self.unregister_and_destroy_runner_ids.is_empty()
            && self.create_counts_by_profile_key.values().sum::<usize>() == 0
    }
}

#[derive(Debug, Serialize)]
pub struct RunnerCounts {
    pub target: usize,
    pub healthy: usize,
    pub started_or_crashed: usize,
    pub idle: usize,
    pub reserved: usize,
    pub busy: usize,
    pub excess_healthy: usize,
    pub wanted: usize,
    pub image_age: Option<Duration>,
}

impl Policy {
    pub fn new(profiles: BTreeMap<String, Profile>) -> eyre::Result<Self> {
        let result = Self {
            profiles,
            base_image_snapshots: BTreeMap::default(),
            ipv4_addresses: BTreeMap::default(),
            runners: None,
            current_override: None,
        };

        let profile_target_counts = result
            .profiles()
            .map(|(key, profile)| (key.clone(), profile.target_count))
            .collect();
        result.validate_resource_requirements(&profile_target_counts)?;

        Ok(result)
    }

    pub fn validate_resource_requirements(
        &self,
        profile_target_counts: &BTreeMap<String, usize>,
    ) -> eyre::Result<()> {
        let required_1g_hugepages = self
            .profiles()
            .map(|(key, profile)| {
                let target_count = profile_target_counts.get(&**key).copied().unwrap_or(0);
                target_count * profile.requires_1g_hugepages
            })
            .sum::<usize>();
        if required_1g_hugepages > TOML.available_1g_hugepages {
            bail!("Profile configuration requires too many 1G hugepages");
        }

        let required_normal_memory = self
            .profiles()
            .map(|(key, profile)| {
                let target_count = profile_target_counts.get(&**key).copied().unwrap_or(0);
                target_count * profile.requires_normal_memory
            })
            .sum::<MemorySize>();
        if required_normal_memory > TOML.available_normal_memory {
            bail!("Profile configuration requires too much normal memory");
        }

        Ok(())
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
        self.update_override_internal();
    }

    pub fn compute_runner_changes(&self) -> eyre::Result<RunnerChanges> {
        if self.runners.is_none() {
            bail!("Policy has no Runners!");
        }

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

        // Destroy invalid runners, but don’t count them as healthy.
        for (&id, _runner) in invalid {
            result.unregister_and_destroy_runner_ids.push(id);
        }

        // Destroy other healthy runners that need to be destroyed, keeping counts per profile.
        let mut proposed_healthy_destroy_counts = self
            .profiles()
            .map(|(key, _)| (&**key, 0))
            .collect::<BTreeMap<_, _>>();
        for (&id, runner) in done_or_unregistered
            .chain(started_or_crashed_and_too_old)
            .chain(reserved_for_too_long)
        {
            result.unregister_and_destroy_runner_ids.push(id);
            *proposed_healthy_destroy_counts
                .get_mut(runner.base_vm_name())
                .expect("Guaranteed by initialiser") += 1;
        }

        // Excess healthy runners should be destroyed if they are idle.
        // Compute this in a separate step, so we can take into account how many destroys we’ve already proposed.
        let excess_idle_runners = self.profiles().flat_map(|(key, profile)| {
            self.idle_runners_for_profile(profile).take(
                self.excess_healthy_runner_count(profile) - proposed_healthy_destroy_counts[&**key],
            )
        });
        for (&id, _runner) in excess_idle_runners {
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

        // If there are runners to destroy, do not create any new runners.
        // Destroying runners may fail, so we can’t assume that their resources will necessarily be freed.
        if !result.unregister_and_destroy_runner_ids.is_empty() {
            result.create_counts_by_profile_key.clear();
        }

        Ok(result)
    }

    /// Spawn a thread that registers and creates a runner.
    ///
    /// Returns the libvirt guest name, since that can’t be parallelised without causing errors.
    pub fn register_create_runner(
        &self,
        profile: &Profile,
        id: usize,
    ) -> eyre::Result<JoinHandle<eyre::Result<String>>> {
        if self.base_image_snapshot(&profile.base_vm_name).is_none() {
            bail!(
                "Tried to create runner, but profile has no base image snapshot (profile {})",
                profile.base_vm_name
            );
        };

        let profile = profile.clone();
        let base_vm_name = profile.base_vm_name.clone();
        let vm_name = format!("{base_vm_name}.{id}");

        Ok(thread::spawn(move || {
            let _span = info_span!("create_runner_thread", runner_id = id, base_vm_name).entered();
            info!(runner_id = id, base_vm_name, "Creating runner");
            match profile.image_type {
                ImageType::Rust => {
                    create_dir(get_runner_data_path(id, None)?)?;
                    let mut runner_toml =
                        File::create_new(get_runner_data_path(id, Path::new("runner.toml"))?)?;
                    writeln!(runner_toml, r#"image_type = "Rust""#)?;
                    symlink(
                        get_profile_configuration_path(&profile, Path::new("boot-script"))?,
                        get_runner_data_path(id, Path::new("boot-script"))?,
                    )?;
                    if !DOTENV.dont_register_runners {
                        let github_api_registration = register_runner(&profile, &vm_name)?;
                        let mut github_api_registration_file = File::create_new(
                            get_runner_data_path(id, Path::new("github-api-registration"))?,
                        )?;
                        github_api_registration_file
                            .write_all(github_api_registration.as_bytes())?;
                    }

                    create_runner(&profile, &vm_name, id)
                }
            }
        }))
    }

    /// Spawn a thread that unregisters, stops, and destroys a runner.
    pub fn unregister_stop_destroy_runner(
        &self,
        id: usize,
    ) -> eyre::Result<JoinHandle<eyre::Result<()>>> {
        let runner = self
            .runner(id)
            .expect("Guaranteed by compute_runner_changes()")
            .clone();
        let Some(profile) = self
            .profile(runner.base_vm_name())
            .map(|profile| profile.to_owned())
        else {
            bail!("Profile");
        };
        info!(runner_id = id, profile.base_vm_name, "Destroying runner");

        match profile.image_type {
            ImageType::Rust => {
                let vm_name = format!("{}.{id}", profile.base_vm_name);

                Ok(thread::spawn(move || {
                    let _span =
                        info_span!("destroy_runner_thread", runner_id = id, vm_name).entered();
                    if let Some(registration) = runner.registration() {
                        if let Err(error) = unregister_runner(registration.id) {
                            warn!(?error, "Failed to unregister runner: {error}");
                        }
                    }
                    if let Err(error) = destroy_runner(&profile, &vm_name, id) {
                        warn!(?error, "Failed to destroy runner: {error}");
                    }

                    Ok(())
                }))
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
            excess_healthy: self.excess_healthy_runner_count(profile),
            wanted: self.wanted_runner_count(profile),
            image_age: self.image_age(profile).ok().flatten(),
        }
    }

    pub fn target_runner_count(&self, profile: &Profile) -> usize {
        if DOTENV.dont_create_runners || self.image_needs_rebuild(profile).unwrap_or(true) {
            0
        } else {
            self.target_runner_count_with_override(profile)
        }
    }

    fn target_runner_count_with_override(&self, profile: &Profile) -> usize {
        if let Some(current_override) = self.current_override.as_ref() {
            if let Some(target_count) = current_override
                .profile_target_counts
                .get(&profile.base_vm_name)
            {
                *target_count
            } else {
                0
            }
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

    pub fn excess_healthy_runner_count(&self, profile: &Profile) -> usize {
        // Healthy runners beyond the target count are excess runners.
        if self.healthy_runner_count(profile) > self.target_runner_count(profile) {
            self.healthy_runner_count(profile) - self.target_runner_count(profile)
        } else {
            0
        }
    }

    pub fn wanted_runner_count(&self, profile: &Profile) -> usize {
        // Healthy runners below the target count are wanted runners.
        if self.target_runner_count(profile) > self.healthy_runner_count(profile) {
            self.target_runner_count(profile) - self.healthy_runner_count(profile)
        } else {
            0
        }
    }

    pub fn critical_runner_count(&self, profile: &Profile) -> usize {
        self.busy_runner_count(profile) + self.reserved_runner_count(profile)
    }

    /// Returns whether the image definitely needs to be rebuilt or not, or None
    /// if we don’t know.
    pub fn image_needs_rebuild(&self, profile: &Profile) -> Option<bool> {
        if self.target_runner_count_with_override(profile) == 0 {
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

    pub fn get_override(&self) -> Option<&Override> {
        self.current_override.as_ref()
    }

    pub fn try_override(
        &mut self,
        profile_override_counts: BTreeMap<String, usize>,
    ) -> eyre::Result<&Override> {
        info!(override_counts = ?profile_override_counts, "Override request");

        // TODO: do we need to take this into account?
        let _runner_changes = self
            .compute_runner_changes()
            .wrap_err("Please wait a few seconds")?;

        if self.current_override.is_some() {
            bail!("Unable to set override while another override is active");
        }
        if profile_override_counts.is_empty() {
            bail!("No point in setting an empty override");
        }

        // Compute the extra demand: subtract counts that are already covered by our static configuration,
        // except for any runners that are currently critical.
        let mut profile_extra_counts = profile_override_counts.clone();
        for (profile_key, count) in profile_extra_counts.iter_mut() {
            let Some(profile) = self.profile(&profile_key) else {
                bail!("No profile with key: {profile_key}");
            };
            let delta = if profile.target_count > self.critical_runner_count(profile) {
                profile.target_count - self.critical_runner_count(profile)
            } else {
                0
            };
            *count = if *count > delta { *count - delta } else { 0 };
        }

        // Fail loudly if the requested override is meaningless.
        if profile_extra_counts.values().sum::<usize>() == 0 {
            bail!("Requested override is meaningless");
        }

        // Ideally we can satisfy both the extra demand and our static configuration in full.
        // Compute the counts for this ideal scenario.
        let mut scenario = self
            .profiles()
            .map(|(key, profile)| (key.clone(), profile.target_count))
            .collect::<BTreeMap<_, _>>();
        for (profile_key, count) in scenario.iter_mut() {
            if let Some(extra_count) = profile_extra_counts.get(profile_key) {
                *count += *extra_count;
            }
        }

        // Starting with the ideal scenario, try to adjust the scenario until it validates.
        let mut adjusted_override_counts = profile_override_counts;
        'validate: while self
            .validate_resource_requirements(dbg!(&scenario))
            .is_err()
        {
            // Try profiles in descending count order, to avoid starving niche runners.
            let candidate_profile_keys = scenario
                .clone()
                .into_iter()
                .sorted_by_key(|(_, count)| *count)
                .rev()
                .map(|(profile_key, _)| profile_key)
                .collect::<Vec<_>>();
            // First try decrementing a profile that was not requested.
            for profile_key in candidate_profile_keys.iter() {
                if !adjusted_override_counts.contains_key(profile_key) {
                    let profile = self
                        .profile(profile_key)
                        .expect("Guaranteed by initialiser");
                    let count = scenario
                        .get_mut(profile_key)
                        .expect("Guaranteed by candidate_profile_keys");
                    // Only try decrementing profiles that have non-critical runners to spare.
                    if *count > self.critical_runner_count(profile) {
                        *count -= 1;
                        continue 'validate;
                    }
                }
            }
            // If none of those profiles could be decremented, try decrementing a profile that was requested.
            for profile_key in candidate_profile_keys.iter() {
                if adjusted_override_counts.contains_key(profile_key) {
                    let profile = self
                        .profile(profile_key)
                        .expect("Guaranteed by initialiser");
                    let count = scenario
                        .get_mut(profile_key)
                        .expect("Guaranteed by candidate_profile_keys");
                    // Only try decrementing profiles that have non-critical runners to spare.
                    if *count > self.critical_runner_count(profile) {
                        if let Some(override_count) = adjusted_override_counts.get_mut(profile_key)
                        {
                            let extra_count = profile_extra_counts
                                .get_mut(profile_key)
                                .expect("Guaranteed by initialiser");
                            if *extra_count > 0 {
                                *count -= 1;
                                *override_count -= 1;
                                *extra_count -= 1;
                                continue 'validate;
                            }
                        }
                    }
                }
            }
            // No more adjustments are possible.
            break;
        }

        // Fail if the requested override had to be adjusted so far that it became meaningless.
        if profile_extra_counts.values().sum::<usize>() == 0 {
            bail!("Requested override had to be adjusted so far that it became meaningless");
        }

        info!(adjusted_override_counts = ?adjusted_override_counts, extra_counts = ?profile_extra_counts, ?scenario, "Found solution for override request");

        self.current_override = Some(Override {
            profile_override_counts: adjusted_override_counts,
            profile_target_counts: scenario,
            actual_runner_ids_by_profile_key: BTreeMap::default(),
        });

        Ok(self
            .current_override
            .as_ref()
            .expect("Guaranteed by assignment"))
    }

    pub fn cancel_override(&mut self) -> eyre::Result<Option<Override>> {
        Ok(self.current_override.take())
    }

    fn update_override_internal(&mut self) {
        // If the current override is finished, remove it.
        if self.override_is_finished() {
            self.current_override = None;
            return;
        }
        // Get all currently idle runners.
        let idle_runners = self
            .profiles()
            .map(|(profile_key, profile)| {
                let idle_runners = self
                    .idle_runners_for_profile(profile)
                    .map(|(id, _runner)| *id)
                    .collect::<Vec<_>>();
                (profile_key.clone(), idle_runners)
            })
            .collect::<BTreeMap<_, _>>();
        // Try to take ownership of idle runners that belong to the override.
        if let Some(current_override) = self.current_override.as_mut() {
            for (profile_key, count) in current_override.profile_override_counts.clone() {
                let actual_runner_ids = current_override
                    .actual_runner_ids_by_profile_key
                    .entry(profile_key.clone())
                    .or_default();
                while actual_runner_ids.len() < count {
                    // Find an idle runner that we don’t already own.
                    if let Some(idle_runners) = idle_runners.get(&profile_key) {
                        if let Some(id) = idle_runners
                            .iter()
                            .find(|id| !actual_runner_ids.contains(id))
                        {
                            actual_runner_ids.insert(*id);
                            continue;
                        }
                    }
                    // No more idle runners available right now.
                    break;
                }
            }
        }
    }

    fn override_is_finished(&self) -> bool {
        if let Some(current_override) = self.current_override.as_ref() {
            // If the override has delivered all of its runners...
            let actual_runner_count = current_override
                .actual_runner_ids_by_profile_key
                .values()
                .map(|ids| ids.len())
                .sum::<usize>();
            let sum_override_counts = current_override
                .profile_override_counts
                .values()
                .sum::<usize>();
            if actual_runner_count == sum_override_counts {
                // If all of those runners no longer exist...
                if !current_override
                    .actual_runner_ids_by_profile_key
                    .values()
                    .flat_map(|ids| ids.iter())
                    .any(|id| self.runner(*id).is_some())
                {
                    // The override is finished.
                    return true;
                }
            }
        }

        false
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

pub fn runner_images_path(runner_id: usize) -> PathBuf {
    Path::new("/var/lib/libvirt/images/runner").join(&format!("{runner_id}"))
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

#[cfg(test)]
mod test {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use jane_eyre::eyre;
    use settings::{profile::Profile, DOTENV};

    use crate::{
        github::{ApiRunner, ApiRunnerLabel},
        policy::{set_base_image_mtime_for_test, Override, RunnerChanges},
        runner::{set_runner_created_time_for_test, Runners, Status},
    };

    use super::Policy;

    fn profile(
        key: &'static str,
        target_count: usize,
        requires_1g_hugepages: usize,
        requires_normal_memory: &'static str,
    ) -> Profile {
        Profile {
            configuration_name: key.to_owned(),
            base_vm_name: key.to_owned(),
            github_runner_label: key.to_owned(),
            target_count,
            image_type: settings::profile::ImageType::Rust,
            requires_1g_hugepages,
            requires_normal_memory: requires_normal_memory.parse().expect("Bad value in test"),
        }
    }

    #[derive(Debug, Clone)]
    struct FakeRunner {
        profile_key: &'static str,
        status: Status,
        created_time: SystemTime,
        reserved_since: Option<Duration>,
    }
    impl FakeRunner {
        fn idle(profile_key: &'static str) -> Self {
            Self {
                profile_key,
                status: Status::Idle,
                created_time: system_time_minus_seconds(9001),
                reserved_since: None,
            }
        }
        fn busy(profile_key: &'static str) -> Self {
            Self {
                profile_key,
                status: Status::Busy,
                created_time: system_time_minus_seconds(9001),
                reserved_since: None,
            }
        }
        fn reserved(profile_key: &'static str) -> Self {
            Self {
                profile_key,
                status: Status::Reserved,
                created_time: system_time_minus_seconds(9001),
                reserved_since: Some(epoch_duration_now()),
            }
        }
    }
    fn runners(fake_runners: Vec<FakeRunner>) -> Runners {
        let mut next_runner_id = 0;
        let mut make_runner_id_and_guest_name = |profile_key: &str| -> (usize, String) {
            let id = next_runner_id;
            let name = format!("{}.{}", profile_key, id,);
            next_runner_id += 1;
            (id, name)
        };
        let make_registration = |guest_name: &str| -> ApiRunner {
            ApiRunner {
                id: 0,       // any
                busy: false, // any
                name: format!("{}@{}", guest_name, DOTENV.github_api_suffix),
                status: "".to_owned(), // any
                labels: vec![],        // any
            }
        };

        let mut registrations = vec![];
        let mut guest_names = vec![];
        for fake in fake_runners {
            let (runner_id, guest_name) = make_runner_id_and_guest_name(fake.profile_key);
            set_runner_created_time_for_test(runner_id, fake.created_time);
            match fake.status {
                Status::Invalid => registrations.push(make_registration(&guest_name)),
                Status::DoneOrUnregistered => {
                    guest_names.push(format!("{}-{}", DOTENV.libvirt_prefix, guest_name))
                }
                Status::Busy => {
                    let mut api_runner = make_registration(&guest_name);
                    api_runner.busy = true;
                    registrations.push(api_runner);
                    guest_names.push(format!("{}-{}", DOTENV.libvirt_prefix, guest_name));
                }
                Status::Reserved => {
                    let mut api_runner = make_registration(&guest_name);
                    api_runner.labels.push(ApiRunnerLabel {
                        name: "reserved-for:".to_owned(), // any value
                    });
                    if let Some(reserved_since) = fake.reserved_since {
                        let reserved_since = reserved_since.as_secs();
                        api_runner.labels.push(ApiRunnerLabel {
                            name: format!("reserved-since:{reserved_since}"),
                        });
                    }
                    registrations.push(api_runner);
                    guest_names.push(format!("{}-{}", DOTENV.libvirt_prefix, guest_name));
                }
                Status::Idle => {
                    let mut api_runner = make_registration(&guest_name);
                    api_runner.status = "online".to_owned();
                    registrations.push(api_runner);
                    guest_names.push(format!("{}-{}", DOTENV.libvirt_prefix, guest_name));
                }
                Status::StartedOrCrashed => {
                    registrations.push(make_registration(&guest_name));
                    guest_names.push(format!("{}-{}", DOTENV.libvirt_prefix, guest_name));
                }
            }
        }

        Runners::new(registrations, guest_names)
    }

    fn system_time_minus_seconds(delta: u64) -> SystemTime {
        SystemTime::now()
            .checked_sub(Duration::from_secs(delta))
            .expect("Bad delta")
    }
    fn epoch_duration_minus_seconds(delta: u64) -> Duration {
        let now = epoch_duration_now();

        now - Duration::from_secs(delta)
    }
    fn epoch_duration_now() -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Bad time to run this test")
    }

    #[test]
    fn test_new_policy() -> eyre::Result<()> {
        // No profiles is always ok.
        assert!(Policy::new([].into()).is_ok());

        // Sum of `target_count * requires_1g_hugepages` must not exceed `available_1g_hugepages`.
        assert!(Policy::new(
            [
                ("linux".to_owned(), profile("linux", 2, 24, "0B")),
                ("windows".to_owned(), profile("windows", 1, 24, "0B")),
                ("macos".to_owned(), profile("macos", 1, 24, "0B")),
                ("wpt".to_owned(), profile("wpt", 0, 12, "0B")),
            ]
            .into()
        )
        .is_ok());
        assert!(Policy::new(
            [
                ("linux".to_owned(), profile("linux", 2, 24, "0B")),
                ("windows".to_owned(), profile("windows", 1, 24, "0B")),
                ("macos".to_owned(), profile("macos", 1, 24, "0B")),
                ("wpt".to_owned(), profile("wpt", 1, 12, "0B")),
            ]
            .into()
        )
        .is_err());

        // Sum of `target_count * requires_normal_memory` must not exceed `available_normal_memory`.
        assert!(Policy::new(
            [
                ("linux".to_owned(), profile("linux", 2, 0, "4G")),
                ("windows".to_owned(), profile("windows", 1, 0, "4G")),
                ("macos".to_owned(), profile("macos", 1, 0, "4G")),
                ("wpt".to_owned(), profile("wpt", 0, 0, "4G")),
            ]
            .into()
        )
        .is_ok());
        assert!(Policy::new(
            [
                ("linux".to_owned(), profile("linux", 2, 0, "4G")),
                ("windows".to_owned(), profile("windows", 1, 0, "4G")),
                ("macos".to_owned(), profile("macos", 1, 0, "4G")),
                ("wpt".to_owned(), profile("wpt", 1, 0, "4G")),
            ]
            .into()
        )
        .is_err());

        Ok(())
    }

    #[test]
    fn test_compute_runner_changes() -> eyre::Result<()> {
        let mut policy = Policy::new(
            [
                ("linux".to_owned(), profile("linux", 5, 0, "0B")),
                ("windows".to_owned(), profile("windows", 3, 0, "0B")),
                ("macos".to_owned(), profile("macos", 3, 0, "0B")),
                ("wpt".to_owned(), profile("wpt", 0, 0, "0B")),
            ]
            .into(),
        )?;

        // If the runners are not yet known, return an error.
        assert!(policy.compute_runner_changes().is_err());

        // Images need rebuild, because there is no good image.
        policy.set_runners(runners(vec![]));
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![],
                create_counts_by_profile_key: [
                    ("linux".to_owned(), 0),
                    ("windows".to_owned(), 0),
                    ("macos".to_owned(), 0),
                    ("wpt".to_owned(), 0),
                ]
                .into(),
            },
        );

        // Images need rebuild, because they are too old.
        let too_old = system_time_minus_seconds(86500);
        set_base_image_mtime_for_test("/var/lib/libvirt/images/base/linux/base.img@", too_old);
        set_base_image_mtime_for_test("/var/lib/libvirt/images/base/macos/base.img@", too_old);
        set_base_image_mtime_for_test("/var/lib/libvirt/images/base/windows/base.img@", too_old);
        policy.set_base_image_snapshot("linux", "")?;
        policy.set_base_image_snapshot("macos", "")?;
        policy.set_base_image_snapshot("windows", "")?;
        policy.set_base_image_snapshot("wpt", "")?;
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![],
                create_counts_by_profile_key: [
                    ("linux".to_owned(), 0),
                    ("windows".to_owned(), 0),
                    ("macos".to_owned(), 0),
                    ("wpt".to_owned(), 0),
                ]
                .into(),
            },
        );

        // Empty state.
        let fresh = system_time_minus_seconds(0);
        set_base_image_mtime_for_test("/var/lib/libvirt/images/base/linux/base.img@", fresh);
        set_base_image_mtime_for_test("/var/lib/libvirt/images/base/macos/base.img@", fresh);
        set_base_image_mtime_for_test("/var/lib/libvirt/images/base/windows/base.img@", fresh);
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![],
                create_counts_by_profile_key: [
                    ("linux".to_owned(), 5),
                    ("windows".to_owned(), 3),
                    ("macos".to_owned(), 3),
                    ("wpt".to_owned(), 0),
                ]
                .into(),
            },
        );

        // All of the reasons we might destroy runners.
        let fake_runners = vec![
            // [0] Invalid => unregister and destroy
            FakeRunner {
                profile_key: "linux",
                status: Status::Invalid,
                created_time: SystemTime::now(),
                reserved_since: None,
            },
            // [1] DoneOrUnregistered => unregister and destroy
            FakeRunner {
                profile_key: "linux",
                status: Status::DoneOrUnregistered,
                created_time: SystemTime::now(),
                reserved_since: None,
            },
            // [2] StartedOrCrashed, but not too old => keep (1/5)
            FakeRunner {
                profile_key: "linux",
                status: Status::StartedOrCrashed,
                created_time: SystemTime::now(),
                reserved_since: None,
            },
            // [3] StartedOrCrashed and too old => unregister and destroy
            FakeRunner {
                profile_key: "linux",
                status: Status::StartedOrCrashed,
                created_time: system_time_minus_seconds(130),
                reserved_since: None,
            },
            // [4] Reserved, but not for too long => keep (2/5)
            FakeRunner {
                profile_key: "linux",
                status: Status::Reserved,
                created_time: system_time_minus_seconds(9001),
                reserved_since: Some(epoch_duration_now()),
            },
            // [5] Reserved for too long => unregister and destroy
            FakeRunner {
                profile_key: "linux",
                status: Status::Reserved,
                created_time: system_time_minus_seconds(9001),
                reserved_since: Some(epoch_duration_minus_seconds(210)),
            },
            // [6] [7] [8] [9] [10] [11] [12] Idle or Busy => bleed off excess Idle runners
            // => destroy (1) (2) (3) (4) keep (3/5) (4/5) (5/5)
            FakeRunner::idle("linux"),
            FakeRunner::idle("linux"),
            FakeRunner::idle("linux"),
            FakeRunner::idle("linux"),
            FakeRunner::idle("linux"),
            FakeRunner::idle("linux"),
            FakeRunner::idle("linux"),
        ];
        policy.set_runners(runners(fake_runners.clone()));
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![0, 1, 3, 5, 6, 7, 8, 9],
                create_counts_by_profile_key: [].into(),
            },
        );

        // Destroys failed? Propose those destroys again.
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![0, 1, 3, 5, 6, 7, 8, 9],
                create_counts_by_profile_key: [].into(),
            },
        );

        // All destroys succeeded? Now create runners.
        let fake_runners = fake_runners
            .into_iter()
            .enumerate()
            .filter(|(i, _)| ![0, 1, 3, 5, 6, 7, 8, 9].contains(i))
            .map(|(_, fake)| fake)
            .collect::<Vec<_>>();
        policy.set_runners(runners(fake_runners.clone()));
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![],
                create_counts_by_profile_key: [
                    ("linux".to_owned(), 0),
                    ("windows".to_owned(), 3),
                    ("macos".to_owned(), 3),
                    ("wpt".to_owned(), 0),
                ]
                .into(),
            },
        );

        // Creates failed? Propose those creates again.
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![],
                create_counts_by_profile_key: [
                    ("linux".to_owned(), 0),
                    ("windows".to_owned(), 3),
                    ("macos".to_owned(), 3),
                    ("wpt".to_owned(), 0),
                ]
                .into(),
            },
        );

        // Only idle runners can be considered for destruction.
        let fake_runners = vec![
            // [0] [1] [2] [3] [4] [5] [6] [7] [8] Idle or Busy => bleed off excess Idle runners
            // => keep (1/5) (2/5) (3/5) (4/5) (5/5) (6/5) (7/5) (8/5) (9/5)
            FakeRunner::busy("linux"),
            FakeRunner::busy("linux"),
            FakeRunner::busy("linux"),
            FakeRunner::busy("linux"),
            FakeRunner::busy("linux"),
            FakeRunner::busy("linux"),
            FakeRunner::busy("linux"),
            FakeRunner::busy("linux"),
            FakeRunner::busy("linux"),
        ];
        policy.set_runners(runners(fake_runners.clone()));
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![],
                create_counts_by_profile_key: [
                    ("linux".to_owned(), 0),
                    ("windows".to_owned(), 3),
                    ("macos".to_owned(), 3),
                    ("wpt".to_owned(), 0),
                ]
                .into(),
            },
        );

        Ok(())
    }

    #[test]
    fn test_try_override() -> eyre::Result<()> {
        let mut policy = Policy::new(
            [
                ("linux".to_owned(), profile("linux", 2, 24, "1G")),
                ("windows".to_owned(), profile("windows", 1, 24, "1G")),
                ("macos".to_owned(), profile("macos", 1, 24, "1G")),
                ("wpt".to_owned(), profile("wpt", 0, 12, "1G")),
            ]
            .into(),
        )?;
        set_base_image_mtime_for_test(
            "/var/lib/libvirt/images/base/linux/base.img@",
            SystemTime::now(),
        );
        set_base_image_mtime_for_test(
            "/var/lib/libvirt/images/base/windows/base.img@",
            SystemTime::now(),
        );
        set_base_image_mtime_for_test(
            "/var/lib/libvirt/images/base/macos/base.img@",
            SystemTime::now(),
        );
        policy.set_base_image_snapshot("linux", "")?;
        policy.set_base_image_snapshot("windows", "")?;
        policy.set_base_image_snapshot("macos", "")?;

        // If the runners are not yet known, refuse the request.
        assert!(policy.try_override([].into()).is_err());

        // The runners are now known.
        let fake_runners = vec![
            FakeRunner::idle("linux"),
            FakeRunner::busy("linux"),
            FakeRunner::reserved("windows"),
            FakeRunner::idle("macos"),
        ];
        policy.set_runners(runners(fake_runners.clone()));

        // If the request literally asks for no runners, refuse the request.
        assert!(policy.try_override([].into()).is_err());

        // If the request effectively asks for no extra runners, refuse the request.
        assert!(policy.try_override([("wpt".to_owned(), 0)].into()).is_err());
        assert!(policy
            .try_override([("linux".to_owned(), 0)].into())
            .is_err());
        assert!(policy
            .try_override([("linux".to_owned(), 1)].into())
            .is_err());

        // Accept the request, adjusted for critical runners.
        // Requests that would exceed available resources when taken alone are still acceptable.
        assert_eq!(
            policy.try_override([("wpt".to_owned(), 9)].into())?,
            &Override {
                profile_override_counts: [("wpt".to_owned(), 4)].into(),
                profile_target_counts: [
                    ("linux".to_owned(), 1),
                    ("macos".to_owned(), 0),
                    ("windows".to_owned(), 1),
                    ("wpt".to_owned(), 4),
                ]
                .into(),
                actual_runner_ids_by_profile_key: [].into(),
            },
        );

        // If an override is already active, refuse the request.
        assert!(policy.try_override([("wpt".to_owned(), 8)].into()).is_err());

        // When computing runner changes, take the override into account.
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![0, 3],
                create_counts_by_profile_key: [].into(),
            },
        );

        // All destroys succeeded? Now create runners, taking the override into account.
        let fake_runners = fake_runners
            .into_iter()
            .enumerate()
            .filter(|(i, _)| ![0, 3].contains(i))
            .map(|(_, fake)| fake)
            .collect::<Vec<_>>();
        policy.set_runners(runners(fake_runners.clone()));

        // We can’t create runners yet, because the image needs rebuild.
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![],
                create_counts_by_profile_key: [
                    ("linux".to_owned(), 0),
                    ("windows".to_owned(), 0),
                    ("macos".to_owned(), 0),
                    ("wpt".to_owned(), 0),
                ]
                .into(),
            },
        );

        // The image is ready. Let’s create runners.
        set_base_image_mtime_for_test(
            "/var/lib/libvirt/images/base/wpt/base.img@",
            SystemTime::now(),
        );
        policy.set_base_image_snapshot("wpt", "")?;
        assert_eq!(
            policy.compute_runner_changes()?,
            RunnerChanges {
                unregister_and_destroy_runner_ids: vec![],
                create_counts_by_profile_key: [
                    ("linux".to_owned(), 0),
                    ("windows".to_owned(), 0),
                    ("macos".to_owned(), 0),
                    ("wpt".to_owned(), 4),
                ]
                .into(),
            },
        );

        Ok(())
    }

    #[test]
    fn test_try_override_sweep_one_profile() -> eyre::Result<()> {
        let make_policy = || -> eyre::Result<_> {
            let mut policy = Policy::new(
                [
                    ("linux".to_owned(), profile("linux", 1, 24, "1G")),
                    ("windows".to_owned(), profile("windows", 1, 24, "1G")),
                    ("macos".to_owned(), profile("macos", 1, 24, "1G")),
                    ("wpt".to_owned(), profile("wpt", 2, 12, "1G")),
                ]
                .into(),
            )?;
            policy.set_runners(runners(vec![]));
            Ok(policy)
        };
        for count in 0..=5 {
            let mut policy = make_policy()?;
            let result = policy.try_override([("linux".to_owned(), count)].into());
            if [0, 1].contains(&count) {
                assert!(result.is_err(), "{count}: {result:?}");
            } else {
                assert!(result.is_ok(), "{count}: {result:?}");
            }
        }
        for count in 0..=5 {
            let mut policy = make_policy()?;
            let result = policy.try_override([("windows".to_owned(), count)].into());
            if [0, 1].contains(&count) {
                assert!(result.is_err(), "{count}: {result:?}");
            } else {
                assert!(result.is_ok(), "{count}: {result:?}");
            }
        }
        for count in 0..=5 {
            let mut policy = make_policy()?;
            let result = policy.try_override([("macos".to_owned(), count)].into());
            if [0, 1].contains(&count) {
                assert!(result.is_err(), "{count}: {result:?}");
            } else {
                assert!(result.is_ok(), "{count}: {result:?}");
            }
        }
        for count in 0..=9 {
            let mut policy = make_policy()?;
            let result = policy.try_override([("wpt".to_owned(), count)].into());
            if [0, 1, 2].contains(&count) {
                assert!(result.is_err(), "{count}: {result:?}");
            } else {
                assert!(result.is_ok(), "{count}: {result:?}");
            }
        }

        Ok(())
    }

    #[test]
    fn test_try_override_adjustment_order() -> eyre::Result<()> {
        let make_policy = || -> eyre::Result<_> {
            let mut policy = Policy::new(
                [
                    ("a-niche".to_owned(), profile("a-niche", 2, 6, "0B")),
                    ("b-common".to_owned(), profile("b-common", 6, 6, "0B")),
                    ("override".to_owned(), profile("override", 0, 6, "0B")),
                    ("y-common".to_owned(), profile("y-common", 6, 6, "0B")),
                    ("z-niche".to_owned(), profile("z-niche", 2, 6, "0B")),
                ]
                .into(),
            )?;
            policy.set_runners(runners(vec![]));
            Ok(policy)
        };

        let mut policy = make_policy()?;
        assert_eq!(
            policy.try_override([("override".to_owned(), 2)].into())?,
            &Override {
                profile_override_counts: [("override".to_owned(), 2)].into(),
                profile_target_counts: [
                    ("a-niche".to_owned(), 2),
                    ("b-common".to_owned(), 5),
                    ("override".to_owned(), 2),
                    ("y-common".to_owned(), 5),
                    ("z-niche".to_owned(), 2),
                ]
                .into(),
                actual_runner_ids_by_profile_key: [].into(),
            },
        );

        Ok(())
    }
}
