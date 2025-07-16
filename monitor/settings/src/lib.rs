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
    pub monitor_api_token_authorization_value: String,
    pub github_api_scope: String,
    pub github_api_suffix: String,
    pub libvirt_prefix: String,
    pub monitor_data_path: Option<String>,
    pub monitor_poll_interval: Duration,
    pub api_cache_timeout: Duration,
    pub monitor_start_timeout: Duration,
    pub monitor_reserve_timeout: Duration,
    pub monitor_thread_send_timeout: Duration,
    pub monitor_thread_recv_timeout: Duration,
    pub destroy_all_non_busy_runners: bool,
    pub dont_register_runners: bool,
    pub dont_create_runners: bool,
    pub main_repo_path: String,
    // SERVO_CI_DOT_CARGO_PATH not used
}

#[derive(Deserialize)]
pub struct Toml {
    pub external_base_url: String,
    base_image_max_age: u64,
    dont_update_cached_servo_repo: Option<bool>,
    pub available_1g_hugepages: usize,
    pub available_normal_memory: MemorySize,
    profiles: BTreeMap<String, Profile>,
}

impl Dotenv {
    pub fn load() -> Self {
        let monitor_api_token = env_string("SERVO_CI_MONITOR_API_TOKEN");
        let result = Self {
            monitor_api_token_authorization_value: Self::monitor_api_token_authorization_value(
                &monitor_api_token,
            ),
            github_api_scope: env_string("SERVO_CI_GITHUB_API_SCOPE"),
            github_api_suffix: env_string("SERVO_CI_GITHUB_API_SUFFIX"),
            libvirt_prefix: env_string("SERVO_CI_LIBVIRT_PREFIX"),
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
            main_repo_path: env_string("SERVO_CI_MAIN_REPO_PATH"),
        };

        result.validate()
    }

    #[cfg(any(test, feature = "test"))]
    fn load_for_tests() -> Self {
        let mut github_api_scope = None;
        let mut github_api_suffix = None;
        let mut libvirt_prefix = None;
        let mut monitor_data_path = None;
        let mut monitor_poll_interval = None;
        let mut api_cache_timeout = None;
        let mut monitor_start_timeout = None;
        let mut monitor_reserve_timeout = None;
        let mut monitor_thread_send_timeout = None;
        let mut monitor_thread_recv_timeout = None;
        let mut destroy_all_non_busy_runners = None;
        let mut dont_register_runners = None;
        let mut dont_create_runners = None;
        let mut main_repo_path = None;

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
                "SERVO_CI_GITHUB_API_SCOPE" => github_api_scope = Some(value),
                "SERVO_CI_GITHUB_API_SUFFIX" => github_api_suffix = Some(value),
                "SERVO_CI_LIBVIRT_PREFIX" => libvirt_prefix = Some(value),
                "SERVO_CI_MONITOR_DATA_PATH" => monitor_data_path = Some(value),
                "SERVO_CI_MONITOR_POLL_INTERVAL" => monitor_poll_interval = Some(value),
                "SERVO_CI_API_CACHE_TIMEOUT" => api_cache_timeout = Some(value),
                "SERVO_CI_MONITOR_START_TIMEOUT" => monitor_start_timeout = Some(value),
                "SERVO_CI_MONITOR_RESERVE_TIMEOUT" => monitor_reserve_timeout = Some(value),
                "SERVO_CI_MONITOR_THREAD_SEND_TIMEOUT" => monitor_thread_send_timeout = Some(value),
                "SERVO_CI_MONITOR_THREAD_RECV_TIMEOUT" => monitor_thread_recv_timeout = Some(value),
                "SERVO_CI_DESTROY_ALL_NON_BUSY_RUNNERS" => {
                    destroy_all_non_busy_runners = Some(value)
                }
                "SERVO_CI_DONT_REGISTER_RUNNERS" => dont_register_runners = Some(value),
                "SERVO_CI_DONT_CREATE_RUNNERS" => dont_create_runners = Some(value),
                "SERVO_CI_MAIN_REPO_PATH" => main_repo_path = Some(value),
                _ => { /* do nothing */ }
            }
        }

        // Totally not `ChangeMe`.
        let monitor_api_token = "ChangedMe";

        let result = Self {
            monitor_api_token_authorization_value: Self::monitor_api_token_authorization_value(
                monitor_api_token,
            ),
            github_api_scope: mandatory_string("SERVO_CI_GITHUB_API_SCOPE", github_api_scope),
            github_api_suffix: mandatory_string("SERVO_CI_GITHUB_API_SUFFIX", github_api_suffix),
            libvirt_prefix: mandatory_string("SERVO_CI_LIBVIRT_PREFIX", libvirt_prefix),
            monitor_data_path,
            monitor_poll_interval: parse_duration_secs(
                "SERVO_CI_MONITOR_POLL_INTERVAL",
                monitor_poll_interval,
            ),
            api_cache_timeout: parse_duration_secs("SERVO_CI_API_CACHE_TIMEOUT", api_cache_timeout),
            monitor_start_timeout: parse_duration_secs(
                "SERVO_CI_MONITOR_START_TIMEOUT",
                monitor_start_timeout,
            ),
            monitor_reserve_timeout: parse_duration_secs(
                "SERVO_CI_MONITOR_RESERVE_TIMEOUT",
                monitor_reserve_timeout,
            ),
            monitor_thread_send_timeout: parse_duration_secs(
                "SERVO_CI_MONITOR_THREAD_SEND_TIMEOUT",
                monitor_thread_send_timeout,
            ),
            monitor_thread_recv_timeout: parse_duration_secs(
                "SERVO_CI_MONITOR_THREAD_RECV_TIMEOUT",
                monitor_thread_recv_timeout,
            ),
            destroy_all_non_busy_runners: parse_bool(
                "SERVO_CI_DESTROY_ALL_NON_BUSY_RUNNERS",
                destroy_all_non_busy_runners,
            ),
            dont_register_runners: parse_bool(
                "SERVO_CI_DONT_REGISTER_RUNNERS",
                dont_register_runners,
            ),
            dont_create_runners: parse_bool("SERVO_CI_DONT_CREATE_RUNNERS", dont_create_runners),
            main_repo_path: mandatory_string("SERVO_CI_MAIN_REPO_PATH", main_repo_path),
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
        let result: Toml = toml::from_str(include_str!("../../monitor.toml.example"))?;

        result.validate()
    }

    fn validate(self) -> eyre::Result<Self> {
        if !self.external_base_url.ends_with("/") {
            bail!("external_base_url setting must end with slash!");
        }

        for (key, profile) in self.profiles.iter() {
            assert_eq!(
                *key, profile.base_vm_name,
                "Runner::base_vm_name relies on Toml.profiles key (profile name) and base_vm_name being equal"
            );
        }

        Ok(self)
    }

    pub fn base_image_max_age(&self) -> Duration {
        Duration::from_secs(self.base_image_max_age)
    }

    pub fn dont_update_cached_servo_repo(&self) -> bool {
        self.dont_update_cached_servo_repo.unwrap_or(false)
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

fn parse_u64(key: &str, value: Option<String>) -> u64 {
    mandatory_string(key, value)
        .parse()
        .expect(&format!("Failed to parse {key}!"))
}

fn env_duration_secs(key: &str) -> Duration {
    parse_duration_secs(key, env_option_string(key))
}

fn parse_duration_secs(key: &str, value: Option<String>) -> Duration {
    Duration::from_secs(parse_u64(key, value))
}

fn env_bool(key: &str) -> bool {
    parse_bool(key, env::var_os(key).map(|_| "".to_owned()))
}

fn parse_bool(_key: &str, value: Option<String>) -> bool {
    value.is_some()
}
