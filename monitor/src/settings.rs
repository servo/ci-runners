use std::{
    collections::BTreeMap,
    env::{self, VarError},
    fs::File,
    io::Read,
    path::Path,
    time::Duration,
};

use jane_eyre::eyre::{self, bail};
use serde::Deserialize;

use crate::profile::Profile;

pub struct Dotenv {
    // GITHUB_TOKEN not used
    // LIBVIRT_DEFAULT_URI not used
    pub monitor_api_token_authorization_value: String,
    // SERVO_CI_GITHUB_API_SCOPE not used
    pub github_api_suffix: String,
    pub libvirt_prefix: String,
    pub zfs_prefix: String,
    // SERVO_CI_ZFS_CLONE_PREFIX not used
    pub monitor_data_path: Option<String>,
    // SERVO_CI_ZVOL_BLOCK_DEVICE_TIMEOUT not used
    pub monitor_poll_interval: Duration,
    pub api_cache_timeout: Duration,
    pub monitor_start_timeout: Duration,
    pub monitor_reserve_timeout: Duration,
    pub monitor_thread_send_timeout: Duration,
    pub monitor_thread_recv_timeout: Duration,
    pub destroy_all_non_busy_runners: bool,
    pub dont_register_runners: bool,
    pub dont_create_runners: bool,
    // SERVO_CI_MAIN_REPO_PATH not used
    // SERVO_CI_DOT_CARGO_PATH not used
}

#[derive(Deserialize)]
pub struct Toml {
    pub external_base_url: String,
    pub profiles: BTreeMap<String, Profile>,
}

impl Dotenv {
    pub fn load() -> Self {
        let monitor_api_token = env_string("SERVO_CI_MONITOR_API_TOKEN");
        if monitor_api_token == "ChangeMe" {
            panic!("SERVO_CI_MONITOR_API_TOKEN must be changed!");
        }

        Self {
            monitor_api_token_authorization_value: format!("Bearer {monitor_api_token}"),
            github_api_suffix: env_string("SERVO_CI_GITHUB_API_SUFFIX"),
            libvirt_prefix: env_string("SERVO_CI_LIBVIRT_PREFIX"),
            zfs_prefix: env_string("SERVO_CI_ZFS_PREFIX"),
            monitor_data_path: env_option_string("SERVO_CI_MONITOR_DATA_PATH"),
            monitor_poll_interval: env_duration_secs("SERVO_CI_MONITOR_POLL_INTERVAL"),
            api_cache_timeout: env_duration_secs("SERVO_CI_API_CACHE_TIMEOUT"),
            monitor_start_timeout: env_duration_secs("SERVO_CI_MONITOR_START_TIMEOUT"),
            monitor_reserve_timeout: env_duration_secs("SERVO_CI_MONITOR_RESERVE_TIMEOUT"),
            monitor_thread_send_timeout: env_duration_secs("SERVO_CI_MONITOR_THREAD_SEND_TIMEOUT"),
            monitor_thread_recv_timeout: env_duration_secs("SERVO_CI_MONITOR_THREAD_RECV_TIMEOUT"),
            destroy_all_non_busy_runners: env_bool("SERVO_CI_DESTROY_ALL_NON_BUSY_RUNNERS"),
            dont_register_runners: env_bool("SERVO_CI_DONT_REGISTER_RUNNERS"),
            dont_create_runners: env_bool("SERVO_CI_DONT_CREATE_RUNNERS"),
        }
    }
}

impl Toml {
    pub fn load_default() -> eyre::Result<Self> {
        Self::load("monitor.toml")
    }

    pub fn load(path: impl AsRef<Path>) -> eyre::Result<Self> {
        let mut result = String::default();
        File::open(path)?.read_to_string(&mut result)?;
        let result: Toml = toml::from_str(&result)?;

        if !result.external_base_url.ends_with("/") {
            bail!("external_base_url setting must end with slash!");
        }

        for (key, profile) in result.profiles.iter() {
            assert_eq!(*key, profile.base_vm_name, "Runner::base_vm_name relies on Toml.profiles key (profile name) and base_vm_name being equal");
        }

        Ok(result)
    }

    pub fn profiles(&self) -> impl Iterator<Item = (&str, &Profile)> {
        self.profiles.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn profile(&self, key: impl AsRef<str>) -> Option<&Profile> {
        self.profiles.get(key.as_ref())
    }
}

fn env_option_string(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(result) => Some(result),
        Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(_)) => panic!("{key} not Unicode!"),
    }
}

fn env_string(key: &str) -> String {
    env_option_string(key).expect(&format!("{key} not defined!"))
}

fn env_u64(key: &str) -> u64 {
    env_string(key)
        .parse()
        .expect(&format!("Failed to parse {key}!"))
}

fn env_duration_secs(key: &str) -> Duration {
    Duration::from_secs(env_u64(key))
}

fn env_bool(key: &str) -> bool {
    env::var_os(key).is_some()
}
