pub mod data;

pub mod profile;
pub mod units;

use std::{
    collections::BTreeMap,
    env::{self, VarError},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};

use chrono::TimeDelta;
use jane_eyre::eyre::{self, bail};
use serde::Deserialize;

use crate::{profile::Profile, units::MemorySize};

pub static LIB_MONITOR_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Some(lib_monitor_dir) = env::var_os("LIB_MONITOR_DIR") {
        PathBuf::from(&lib_monitor_dir)
    } else {
        PathBuf::from("..")
    }
});

pub static IMAGE_DEPS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let image_deps_dir = env::var_os("IMAGE_DEPS_DIR").expect("IMAGE_DEPS_DIR not set!");
    PathBuf::from(&image_deps_dir)
});

pub static DOTENV: LazyLock<Dotenv> = LazyLock::new(|| {
    #[cfg(not(any(test, feature = "test")))]
    // FIXME: ensure that this is called before any other threads are started.
    // If we can’t ensure that, remove this and spawn ourselves as a child with a modified env.
    // <https://github.com/dotenv-rs/dotenv/issues/99>
    dotenv::dotenv().expect("Failed to load variables from .env");
    #[cfg(not(any(test, feature = "test")))]
    return Dotenv::load();
    #[cfg(any(test, feature = "test"))]
    return Dotenv::load_for_tests();
});

pub static TOML: LazyLock<Toml> = LazyLock::new(|| {
    #[cfg(not(any(test, feature = "test")))]
    return Toml::load_default().expect("Failed to load settings from monitor.toml");
    #[cfg(any(test, feature = "test"))]
    return Toml::load_for_tests().expect("Guaranteed by monitor.toml.example");
});

#[derive(Default)]
pub struct Dotenv {
    // GITHUB_TOKEN not used
    // LIBVIRT_DEFAULT_URI not used
    pub monitor_api_token_raw_value: String,
    pub monitor_api_token_authorization_value: String,
    pub monitor_data_path: Option<String>,
}

#[derive(Deserialize)]
pub struct Toml {
    pub listen_on: Vec<String>,
    pub external_base_url: String,
    pub github_api_scope: String,
    pub allowed_qualified_repo_prefix: String,
    pub github_api_suffix: String,
    monitor_poll_interval: u64,
    api_cache_timeout: u64,
    tokenless_select_artifact_max_age: u32,
    monitor_start_timeout: u64,
    monitor_reserve_timeout: u64,
    monitor_thread_send_timeout: u64,
    monitor_thread_recv_timeout: u64,
    destroy_all_non_busy_runners: Option<bool>,
    dont_register_runners: Option<bool>,
    dont_create_runners: Option<bool>,
    pub main_repo_path: String,
    base_image_max_age: u64,
    dont_update_cached_servo_repo: Option<bool>,
    libvirt_runner_guest_prefix: Option<String>,
    pub available_1g_hugepages: usize,
    pub available_normal_memory: MemorySize,
    profiles: BTreeMap<String, Profile>,
}

impl Dotenv {
    pub fn load() -> Self {
        let monitor_api_token = env_string("SERVO_CI_MONITOR_API_TOKEN");
        let result = Self {
            monitor_api_token_raw_value: monitor_api_token.clone(),
            monitor_api_token_authorization_value: Self::monitor_api_token_authorization_value(
                &monitor_api_token,
            ),
            monitor_data_path: env_option_string("SERVO_CI_MONITOR_DATA_PATH"),
        };

        result.validate()
    }

    #[cfg(any(test, feature = "test"))]
    fn load_for_tests() -> Self {
        let mut monitor_data_path = None;

        // TODO: find a way to do this without a temporary file
        use std::io::Write;
        let env_path = mktemp::Temp::new_path();
        File::create_new(&env_path)
            .expect("Failed to create temporary env file")
            .write_all(include_bytes!("../../../.env.example"))
            .expect("Failed to write temporary env file");

        // TODO: this is no longer marked deprecated, but a new version of dotenv has not been released.
        // Remove this allow once we’ve updated to that new version of dotenv.
        #[allow(deprecated)]
        for entry in dotenv::from_path_iter(env_path).expect("Failed to load temporary env file") {
            let (key, value) = entry.expect("Failed to load entry");
            match &*key {
                "SERVO_CI_MONITOR_API_TOKEN" => { /* do nothing (see below) */ }
                "SERVO_CI_MONITOR_DATA_PATH" => monitor_data_path = Some(value),
                _ => { /* do nothing */ }
            }
        }

        // Totally not `ChangeMe`.
        let monitor_api_token = "ChangedMe";

        let result = Self {
            monitor_api_token_raw_value: monitor_api_token.to_owned(),
            monitor_api_token_authorization_value: Self::monitor_api_token_authorization_value(
                monitor_api_token,
            ),
            monitor_data_path,
        };

        result.validate()
    }

    fn validate(self) -> Self {
        if self.monitor_api_token_authorization_value == "Bearer ChangeMe" {
            panic!("SERVO_CI_MONITOR_API_TOKEN must be changed!");
        }

        self
    }

    fn monitor_api_token_authorization_value(monitor_api_token: &str) -> String {
        format!("Bearer {monitor_api_token}")
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

        result.validate()
    }

    #[cfg(any(test, feature = "test"))]
    fn load_for_tests() -> eyre::Result<Self> {
        let result: Toml = toml::from_str(include_str!("../../../monitor.toml.example"))?;

        result.validate()
    }

    fn validate(self) -> eyre::Result<Self> {
        if !self.external_base_url.ends_with("/") {
            bail!("external_base_url setting must end with slash!");
        }

        for (key, profile) in self.profiles.iter() {
            assert_eq!(
                *key, profile.profile_name,
                "Runner::profile_name relies on Toml.profiles key (profile name) and profile_name being equal"
            );
        }

        Ok(self)
    }

    pub fn monitor_poll_interval(&self) -> Duration {
        Duration::from_secs(self.monitor_poll_interval)
    }

    pub fn api_cache_timeout(&self) -> Duration {
        Duration::from_secs(self.api_cache_timeout)
    }

    pub fn tokenless_select_artifact_max_age(&self) -> TimeDelta {
        TimeDelta::new(self.tokenless_select_artifact_max_age.into(), 0)
            .expect("`tokenless_select_artifact_max_age` setting is out of range")
    }

    pub fn monitor_start_timeout(&self) -> Duration {
        Duration::from_secs(self.monitor_start_timeout)
    }

    pub fn monitor_reserve_timeout(&self) -> Duration {
        Duration::from_secs(self.monitor_reserve_timeout)
    }

    pub fn monitor_thread_send_timeout(&self) -> Duration {
        Duration::from_secs(self.monitor_thread_send_timeout)
    }

    pub fn monitor_thread_recv_timeout(&self) -> Duration {
        Duration::from_secs(self.monitor_thread_recv_timeout)
    }

    pub fn destroy_all_non_busy_runners(&self) -> bool {
        self.destroy_all_non_busy_runners.unwrap_or(false)
    }

    pub fn dont_register_runners(&self) -> bool {
        self.dont_register_runners.unwrap_or(false)
    }

    pub fn dont_create_runners(&self) -> bool {
        self.dont_create_runners.unwrap_or(false) || self.destroy_all_non_busy_runners()
    }

    pub fn base_image_max_age(&self) -> Duration {
        Duration::from_secs(self.base_image_max_age)
    }

    pub fn dont_update_cached_servo_repo(&self) -> bool {
        self.dont_update_cached_servo_repo.unwrap_or(false)
    }

    pub fn libvirt_runner_guest_prefix(&self) -> &str {
        self.libvirt_runner_guest_prefix
            .as_deref()
            .unwrap_or("ci-runner")
    }

    pub fn initial_profiles(&self) -> BTreeMap<String, Profile> {
        self.profiles.clone()
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
    mandatory_string(key, env_option_string(key))
}

fn mandatory_string(key: &str, value: Option<String>) -> String {
    value.expect(&format!("{key} not defined!"))
}
