use std::{
    env::{self, VarError},
    time::Duration,
};

pub struct Settings {
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
    // SERVO_CI_DONT_REGISTER_RUNNERS not used
    pub dont_create_runners: bool,
    // SERVO_CI_MAIN_REPO_PATH not used
    // SERVO_CI_DOT_CARGO_PATH not used
}

impl Settings {
    pub fn load() -> Self {
        Self {
            github_api_suffix: env_string("SERVO_CI_GITHUB_API_SUFFIX"),
            libvirt_prefix: env_string("SERVO_CI_LIBVIRT_PREFIX"),
            zfs_prefix: env_string("SERVO_CI_ZFS_PREFIX"),
            monitor_data_path: env_option_string("SERVO_CI_MONITOR_DATA_PATH"),
            monitor_poll_interval: env_duration_secs("SERVO_CI_MONITOR_POLL_INTERVAL"),
            api_cache_timeout: env_duration_secs("SERVO_CI_API_CACHE_TIMEOUT"),
            monitor_start_timeout: env_duration_secs("SERVO_CI_MONITOR_START_TIMEOUT"),
            monitor_reserve_timeout: env_duration_secs("SERVO_CI_MONITOR_RESERVE_TIMEOUT"),
            dont_create_runners: env_bool("SERVO_CI_DONT_CREATE_RUNNERS"),
        }
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
