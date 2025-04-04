use std::{
    collections::BTreeMap,
    fs::{create_dir, File},
    io::{Read, Write},
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
use cmd_lib::run_cmd;
use jane_eyre::eyre::{self, bail, Context};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{
    auth::RemoteAddr,
    data::{get_profile_configuration_path, get_profile_data_path, get_runner_data_path},
    github::register_runner,
    libvirt::{get_ipv4_address, update_screenshot},
    runner::{Runner, Runners, Status},
    zfs::snapshot_creation_time_unix,
    DOTENV, LIB_MONITOR_DIR, TOML,
};

#[derive(Debug)]
pub struct Profiles {
    profiles: BTreeMap<String, Profile>,
    base_image_snapshots: BTreeMap<String, String>,
    ipv4_addresses: BTreeMap<String, Option<Ipv4Addr>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Profile {
    pub configuration_name: String,
    pub base_vm_name: String,
    pub github_runner_label: String,
    pub target_count: usize,
    #[serde(default)]
    pub image_type: ImageType,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub enum ImageType {
    #[default]
    BuildImageScript,
    Rust,
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

impl Profiles {
    pub fn new(profiles: BTreeMap<String, Profile>) -> eyre::Result<Self> {
        let mut base_image_snapshots = BTreeMap::default();
        for profile_key in profiles.keys() {
            if let Some(base_image_snapshot) = Self::read_base_image_snapshot(&profile_key)? {
                base_image_snapshots.insert(profile_key.clone(), base_image_snapshot);
            }
        }

        Ok(Self {
            profiles,
            base_image_snapshots,
            ipv4_addresses: BTreeMap::default(),
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Profile)> {
        self.profiles.iter()
    }

    pub fn get(&self, profile_key: &str) -> Option<&Profile> {
        self.profiles.get(profile_key)
    }

    pub fn base_image_snapshot(&self, profile_key: &str) -> Option<&String> {
        self.base_image_snapshots.get(profile_key)
    }

    pub fn create_runner(&self, profile: &Profile, id: usize) -> eyre::Result<()> {
        let Some(base_image_snapshot) = self.base_image_snapshot(&profile.base_vm_name) else {
            bail!(
                "Tried to create runner, but profile has no base image snapshot (profile {})",
                profile.base_vm_name
            );
        };
        info!(runner_id = id, profile.base_vm_name, "Creating runner");
        match profile.image_type {
            ImageType::BuildImageScript => {
                let exit_status = Command::new("./create-runner.sh")
                    .current_dir(&*LIB_MONITOR_DIR)
                    .args([
                        &id.to_string(),
                        &profile.base_vm_name,
                        base_image_snapshot,
                        &profile.configuration_name,
                        &profile.github_runner_label,
                    ])
                    .spawn()
                    .unwrap()
                    .wait()
                    .unwrap();
                if exit_status.success() {
                    Ok(())
                } else {
                    eyre::bail!("Command exited with status {}", exit_status);
                }
            }
            ImageType::Rust => {
                let base_vm_name = &profile.base_vm_name;
                create_dir(get_runner_data_path(id, None)?)?;
                let mut runner_toml =
                    File::create_new(get_runner_data_path(id, Path::new("runner.toml"))?)?;
                writeln!(runner_toml, r#"image_type = "Rust""#)?;
                let vm_name = format!("{base_vm_name}.{id}");
                let prefixed_vm_name = format!("{}-{vm_name}", DOTENV.libvirt_prefix);
                register_runner(&vm_name, &profile.github_runner_label, "../a")?;
                run_cmd!(virt-clone --auto-clone --reflink -o $base_vm_name -n $prefixed_vm_name)?;
                run_cmd!(virsh start -- $prefixed_vm_name)?;
                Ok(())
            }
        }
    }

    pub fn destroy_runner(&self, profile: &Profile, id: usize) -> eyre::Result<()> {
        info!(runner_id = id, profile.base_vm_name, "Destroying runner");
        let exit_status = Command::new("./destroy-runner.sh")
            .current_dir(&*LIB_MONITOR_DIR)
            .args([&profile.base_vm_name, &id.to_string()])
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
        if exit_status.success() {
            return Ok(());
        } else {
            eyre::bail!("Command exited with status {}", exit_status);
        }
    }
}

impl Profile {
    pub fn runners<'p, 'r: 'p>(
        &'p self,
        runners: &'r Runners,
    ) -> impl Iterator<Item = (&'r usize, &'r Runner)> + 'p {
        runners
            .iter()
            .filter(|(_id, runner)| runner.base_vm_name() == self.base_vm_name)
    }

    pub fn idle_runners<'p, 'r: 'p>(
        &'p self,
        runners: &'r Runners,
    ) -> impl Iterator<Item = (&'r usize, &'r Runner)> + 'p {
        self.runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Idle)
    }
}

impl Profiles {
    pub fn runner_counts(&self, profile: &Profile, runners: &Runners) -> RunnerCounts {
        RunnerCounts {
            target: self.target_runner_count(profile),
            healthy: self.healthy_runner_count(profile, runners),
            started_or_crashed: self.started_or_crashed_runner_count(profile, runners),
            idle: self.idle_runner_count(profile, runners),
            reserved: self.reserved_runner_count(profile, runners),
            busy: self.busy_runner_count(profile, runners),
            excess_idle: self.excess_idle_runner_count(profile, runners),
            wanted: self.wanted_runner_count(profile, runners),
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

    pub fn healthy_runner_count(&self, profile: &Profile, runners: &Runners) -> usize {
        self.started_or_crashed_runner_count(profile, runners)
            + self.idle_runner_count(profile, runners)
            + self.reserved_runner_count(profile, runners)
            + self.busy_runner_count(profile, runners)
            + self.done_or_unregistered_runner_count(profile, runners)
    }

    pub fn started_or_crashed_runner_count(&self, profile: &Profile, runners: &Runners) -> usize {
        profile
            .runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::StartedOrCrashed)
            .count()
    }

    pub fn idle_runner_count(&self, profile: &Profile, runners: &Runners) -> usize {
        profile
            .runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Idle)
            .count()
    }

    pub fn reserved_runner_count(&self, profile: &Profile, runners: &Runners) -> usize {
        profile
            .runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Reserved)
            .count()
    }

    pub fn busy_runner_count(&self, profile: &Profile, runners: &Runners) -> usize {
        profile
            .runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::Busy)
            .count()
    }

    pub fn done_or_unregistered_runner_count(&self, profile: &Profile, runners: &Runners) -> usize {
        profile
            .runners(runners)
            .filter(|(_id, runner)| runner.status() == Status::DoneOrUnregistered)
            .count()
    }

    pub fn excess_idle_runner_count(&self, profile: &Profile, runners: &Runners) -> usize {
        // Healthy runners beyond the target count are excess runners.
        let excess =
            if self.healthy_runner_count(profile, runners) > self.target_runner_count(profile) {
                self.healthy_runner_count(profile, runners) - self.target_runner_count(profile)
            } else {
                0
            };

        // But we can only destroy idle runners, since busy runners have work to do.
        excess.min(self.idle_runner_count(profile, runners))
    }

    pub fn wanted_runner_count(&self, profile: &Profile, runners: &Runners) -> usize {
        // Healthy runners below the target count are wanted runners.
        if self.target_runner_count(profile) > self.healthy_runner_count(profile, runners) {
            self.target_runner_count(profile) - self.healthy_runner_count(profile, runners)
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
            ImageType::BuildImageScript => {
                match snapshot_creation_time_unix(&profile.base_vm_name, base_image_snapshot) {
                    Ok(result) => result,
                    Err(error) => {
                        debug!(
                            profile.base_vm_name,
                            ?error,
                            "Failed to get snapshot creation time"
                        );
                        return Ok(None);
                    }
                }
            }
            ImageType::Rust => {
                let base_image_path = profile.base_image_path(&**base_image_snapshot);
                let metadata = match std::fs::metadata(&base_image_path) {
                    Ok(result) => result,
                    Err(error) => {
                        debug!(
                            profile.base_vm_name,
                            ?base_image_path,
                            ?error,
                            "Failed to get file metadata"
                        );
                        return Ok(None);
                    }
                };
                let mtime = metadata.modified().expect("Guaranteed by platform");
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
}

impl Profile {
    pub fn update_screenshot(&self) {
        if let Err(error) = self.try_update_screenshot() {
            debug!(
                self.base_vm_name,
                ?error,
                "Failed to update screenshot for profile guest"
            );
        }
    }

    fn try_update_screenshot(&self) -> eyre::Result<()> {
        let output_dir = get_profile_data_path(&self.base_vm_name, None)?;
        update_screenshot(&self.base_vm_name, &output_dir)?;

        Ok(())
    }

    pub fn base_images_path(&self) -> PathBuf {
        Path::new("/var/lib/libvirt/images/base").join(&self.base_vm_name)
    }

    pub fn base_image_path<'snap>(&self, snapshot_name: impl Into<Option<&'snap str>>) -> PathBuf {
        if let Some(snapshot_name) = snapshot_name.into() {
            self.base_images_path()
                .join(format!("base.img@{snapshot_name}"))
        } else {
            self.base_images_path().join("base.img")
        }
    }
}

impl Profiles {
    pub fn set_base_image_snapshot(
        &mut self,
        profile_key: &str,
        base_image_snapshot: &str,
    ) -> eyre::Result<()> {
        self.base_image_snapshots
            .insert(profile_key.to_owned(), base_image_snapshot.to_owned());

        let path = get_profile_data_path(profile_key, Path::new("base-image-snapshot"))?;
        let mut file = AtomicWriteFile::open(&path)?;
        write!(file, "{base_image_snapshot}")?;
        file.commit()?;

        Ok(())
    }

    fn read_base_image_snapshot(profile_key: &str) -> eyre::Result<Option<String>> {
        let path = get_profile_data_path(profile_key, Path::new("base-image-snapshot"))?;
        if let Ok(mut file) = File::open(&path) {
            let mut base_image_snapshot = String::default();
            file.read_to_string(&mut base_image_snapshot)?;
            return Ok(Some(base_image_snapshot));
        }

        Ok(None)
    }

    pub fn update_ipv4_addresses(&mut self) {
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

    pub fn boot_script(&self, remote_addr: RemoteAddr) -> eyre::Result<String> {
        for (key, ipv4_address) in self.ipv4_addresses.iter() {
            if let Some(ipv4_address) = ipv4_address {
                if remote_addr == *ipv4_address {
                    let profile = self.profiles.get(key).expect("Guaranteed by Profiles impl");
                    let path = get_profile_configuration_path(profile, Path::new("boot-script.sh"));
                    let mut result = String::default();
                    File::open(path)?.read_to_string(&mut result)?;
                    return Ok(result);
                }
            }
        }

        bail!("No profile guest found with IP address: {}", remote_addr)
    }
}
