mod auth;
mod dashboard;
mod data;
mod github;
mod id;
mod image;
mod libvirt;
mod profile;
mod rocket_eyre;
mod runner;
mod settings;
mod shell;
mod zfs;

use core::str;
use std::{
    collections::BTreeMap,
    env,
    fs::File,
    path::{Path, PathBuf},
    process::exit,
    sync::{LazyLock, RwLock},
    thread::{self},
    time::Duration,
};

use askama::Template;
use crossbeam_channel::{Receiver, Sender};
use dotenv::dotenv;
use jane_eyre::eyre::{self, eyre, Context, OptionExt};
use mktemp::Temp;
use rocket::{
    fs::NamedFile,
    get,
    http::ContentType,
    post,
    response::content::{RawJson, RawText},
};
use serde::Deserialize;
use serde_json::json;
use tokio::try_join;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    auth::{ApiKeyGuard, RemoteAddr},
    dashboard::Dashboard,
    data::{get_profile_data_path, get_runner_data_path, run_migrations},
    github::{list_registered_runners_for_host, Cache},
    id::IdGen,
    image::Rebuilds,
    libvirt::list_runner_guests,
    profile::{Profiles, RunnerCounts},
    rocket_eyre::EyreReport,
    runner::{Runner, Runners, Status},
    settings::{Dotenv, Toml},
    zfs::list_runner_volumes,
};

static LIB_MONITOR_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Some(lib_monitor_dir) = env::var_os("LIB_MONITOR_DIR") {
        PathBuf::from(&lib_monitor_dir)
    } else {
        PathBuf::from("..")
    }
});

static IMAGE_DEPS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let image_deps_dir = env::var_os("IMAGE_DEPS_DIR").expect("IMAGE_DEPS_DIR not set!");
    PathBuf::from(&image_deps_dir)
});

static DOTENV: LazyLock<Dotenv> = LazyLock::new(|| {
    dotenv().expect("Failed to load variables from .env");
    Dotenv::load()
});

static TOML: LazyLock<Toml> =
    LazyLock::new(|| Toml::load_default().expect("Failed to load settings from monitor.toml"));

static DASHBOARD: RwLock<Option<Dashboard>> = RwLock::new(None);

/// Requests that are handled synchronously by the monitor thread.
///
/// The requests that can be handled without the monitor thread are as follows:
/// - GET `/` => templates/index.html
/// - GET `/dashboard.html` => templates/dashboard.html
/// - GET `/dashboard.json` => `{"profile_runner_counts": {}, "runners": []}`
/// - GET `/profile/<profile key>/screenshot.png` => image/png
/// - GET `/runner/<our runner id>/screenshot.png` => image/png
#[derive(Debug)]
enum Request {
    /// POST `/profile/<profile_key>/take?unique_id&qualified_repo=<user>/<repo>&run_id` => `{"id", "runner"}` | `null`
    /// POST `/profile/<profile_key>/take/<count>?unique_id&qualified_repo=<user>/<repo>&run_id` => `[{"id", "runner"}]` | `null`
    TakeRunners {
        response_tx: Sender<String>,
        profile_key: String,
        query: TakeRunnerQuery,
        count: usize,
    },

    /// GET `/runner/<our runner id>/screenshot/now` => image/png
    Screenshot {
        response_tx: Sender<eyre::Result<Temp>>,
        runner_id: usize,
    },

    /// - GET `/github-jitconfig` => application/json
    GithubJitconfig {
        response_tx: Sender<eyre::Result<Option<String>>>,
        remote_addr: RemoteAddr,
    },

    /// - GET `/boot` => text/plain
    BootScript {
        response_tx: Sender<eyre::Result<String>>,
        remote_addr: RemoteAddr,
    },
}
#[derive(Debug, Deserialize)]
struct TakeRunnerQuery {
    unique_id: String,
    qualified_repo: String,
    run_id: String,
}

struct Channel<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}
static REQUEST: LazyLock<Channel<Request>> = LazyLock::new(|| {
    let (sender, receiver) = crossbeam_channel::bounded(0);
    Channel { sender, receiver }
});

#[derive(Clone, Debug, Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub content: String,
}

#[get("/")]
fn index_route() -> rocket_eyre::Result<IndexTemplate> {
    Ok(DASHBOARD
        .read()
        .map_err(|e| eyre!("Failed to acquire RwLock: {e:?}"))
        .map_err(EyreReport::ServiceUnavailable)?
        .as_ref()
        .map(|d| IndexTemplate {
            content: d.html.clone(),
        })
        .ok_or_eyre("Monitor thread is still starting or not responding")
        .map_err(EyreReport::ServiceUnavailable)?)
}

#[get("/dashboard.html")]
fn dashboard_html_route() -> rocket_eyre::Result<String> {
    Ok(DASHBOARD
        .read()
        .map_err(|e| eyre!("Failed to acquire RwLock: {e:?}"))
        .map_err(EyreReport::ServiceUnavailable)?
        .as_ref()
        .map(|d| d.html.clone())
        .ok_or_eyre("Monitor thread is still starting or not responding")
        .map_err(EyreReport::ServiceUnavailable)?)
}

#[get("/dashboard.json")]
fn dashboard_json_route() -> rocket_eyre::Result<RawJson<String>> {
    let result = DASHBOARD
        .read()
        .map_err(|e| eyre!("Failed to acquire RwLock: {e:?}"))
        .map_err(EyreReport::ServiceUnavailable)?
        .as_ref()
        .map(|x| x.json.clone())
        .ok_or_eyre("Monitor thread is still starting or not responding")
        .map_err(EyreReport::ServiceUnavailable)?;

    Ok(RawJson(result))
}

#[post("/profile/<profile_key>/take?<unique_id>&<qualified_repo>&<run_id>")]
fn take_runner_route(
    profile_key: String,
    unique_id: String,
    qualified_repo: String,
    run_id: String,
    _auth: ApiKeyGuard,
) -> rocket_eyre::Result<RawJson<String>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::TakeRunners {
            response_tx,
            profile_key,
            query: TakeRunnerQuery {
                unique_id,
                qualified_repo,
                run_id,
            },
            count: 1,
        },
        DOTENV.monitor_thread_send_timeout,
    )?;
    let result = response_rx.recv_timeout(DOTENV.monitor_thread_recv_timeout)?;

    Ok(RawJson(result))
}

#[post("/profile/<profile_key>/take/<count>?<unique_id>&<qualified_repo>&<run_id>")]
fn take_runners_route(
    profile_key: String,
    count: usize,
    unique_id: String,
    qualified_repo: String,
    run_id: String,
    _auth: ApiKeyGuard,
) -> rocket_eyre::Result<RawJson<String>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::TakeRunners {
            response_tx,
            profile_key,
            query: TakeRunnerQuery {
                unique_id,
                qualified_repo,
                run_id,
            },
            count,
        },
        DOTENV.monitor_thread_send_timeout,
    )?;
    let result = response_rx.recv_timeout(
        // TODO: make this configurable?
        DOTENV.monitor_thread_recv_timeout + Duration::from_secs(count as u64),
    )?;

    Ok(RawJson(result))
}

#[get("/profile/<profile_key>/screenshot.png")]
async fn profile_screenshot_route(profile_key: String) -> rocket_eyre::Result<NamedFile> {
    let path = get_profile_data_path(&profile_key, Path::new("screenshot.png"))
        .wrap_err("Failed to compute path")
        .map_err(EyreReport::InternalServerError)?;

    Ok(NamedFile::open(path).await?)
}

#[get("/runner/<runner_id>/screenshot.png")]
async fn runner_screenshot_route(runner_id: usize) -> rocket_eyre::Result<NamedFile> {
    let path = get_runner_data_path(runner_id, Path::new("screenshot.png"))
        .wrap_err("Failed to compute path")
        .map_err(EyreReport::InternalServerError)?;

    Ok(NamedFile::open(path).await?)
}

#[get("/runner/<runner_id>/screenshot/now")]
fn runner_screenshot_now_route(runner_id: usize) -> rocket_eyre::Result<(ContentType, File)> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::Screenshot {
            response_tx,
            runner_id,
        },
        DOTENV.monitor_thread_send_timeout,
    )?;
    let path = response_rx.recv_timeout(DOTENV.monitor_thread_recv_timeout)??;
    debug!(?path);

    // Moving `path` into File ensures Temp is not dropped until close
    Ok((ContentType::PNG, File::open(path)?))
}

#[get("/github-jitconfig")]
fn github_jitconfig_route(remote_addr: RemoteAddr) -> rocket_eyre::Result<RawJson<String>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::GithubJitconfig {
            response_tx,
            remote_addr,
        },
        DOTENV.monitor_thread_send_timeout,
    )?;
    let result = response_rx
        .recv_timeout(DOTENV.monitor_thread_recv_timeout)?
        .map_err(EyreReport::ServiceUnavailable)?;

    Ok(RawJson(json!(result).to_string()))
}

#[get("/boot")]
fn boot_script_route(remote_addr: RemoteAddr) -> rocket_eyre::Result<RawText<String>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::BootScript {
            response_tx,
            remote_addr,
        },
        DOTENV.monitor_thread_send_timeout,
    )?;
    let result = response_rx
        .recv_timeout(DOTENV.monitor_thread_recv_timeout)?
        .map_err(EyreReport::ServiceUnavailable)?;

    Ok(RawText(result))
}

#[rocket::main]
async fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    if env::var_os("RUST_LOG").is_none() {
        // EnvFilter Builder::with_default_directive doesn’t support multiple directives,
        // so we need to apply defaults ourselves.
        env::set_var("RUST_LOG", "monitor=info,rocket=info,cmd_lib::child=info");
    }
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::builder().from_env_lossy())
        .init();

    dotenv()?;
    info!(LIB_MONITOR_DIR = ?*LIB_MONITOR_DIR);
    info!(IMAGE_DEPS_DIR = ?*IMAGE_DEPS_DIR);
    run_migrations()?;

    tokio::task::spawn(async move {
        let thread = thread::spawn(monitor_thread);
        loop {
            if thread.is_finished() {
                match thread.join() {
                    Ok(Ok(())) => {
                        info!("Monitor thread exited");
                        exit(0);
                    }
                    Ok(Err(report)) => error!(%report, "Monitor thread error"),
                    Err(panic) => error!(?panic, "Monitor thread panic"),
                };
                exit(1);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    let rocket = |listen_addr: &str| {
        rocket::custom(
            rocket::Config::figment()
                .merge(("port", 8000))
                .merge(("address", listen_addr)),
        )
        .mount(
            "/",
            rocket::routes![
                index_route,
                dashboard_html_route,
                dashboard_json_route,
                take_runner_route,
                take_runners_route,
                profile_screenshot_route,
                runner_screenshot_route,
                runner_screenshot_now_route,
                github_jitconfig_route,
                boot_script_route,
            ],
        )
        .launch()
    };

    try_join!(rocket("::1"), rocket("192.168.100.1"))?;

    Ok(())
}

/// The monitor thread is our single source of truth.
///
/// It handles one [`Request`] at a time, polling for updated resources before
/// each request, then sends one response to the API server for each request.
fn monitor_thread() -> eyre::Result<()> {
    let mut id_gen = IdGen::new_load().unwrap_or_else(|error| {
        warn!(?error, "Failed to read last-runner-id: {error}");
        IdGen::new_empty()
    });

    let mut profiles = Profiles::new(TOML.initial_profiles())?;
    let mut registrations_cache = Cache::default();
    let mut image_rebuilds = Rebuilds::default();

    loop {
        let registrations = registrations_cache.get(|| list_registered_runners_for_host())?;
        let guests = list_runner_guests()?;
        let volumes = list_runner_volumes()?;
        trace!(?registrations, ?guests, ?volumes);
        info!(
            "{} registrations, {} guests, {} volumes",
            registrations.len(),
            guests.len(),
            volumes.len()
        );

        let runners = Runners::new(registrations, guests, volumes);
        image_rebuilds.run(&mut profiles, &runners)?;

        let profile_runner_counts: BTreeMap<_, _> = profiles
            .iter()
            .map(|(key, profile)| (key.clone(), profiles.runner_counts(profile, &runners)))
            .collect();
        for (
            key,
            RunnerCounts {
                target,
                healthy,
                started_or_crashed,
                idle,
                reserved,
                busy,
                excess_idle,
                wanted,
                image_age,
            },
        ) in profile_runner_counts.iter()
        {
            let profile = profiles.get(key).ok_or_eyre("Failed to get profile")?;
            let image =
                profiles
                    .base_image_snapshot(key)
                    .map(|snapshot| match profile.image_type {
                        profile::ImageType::BuildImageScript => format!(
                            "{}/{}@{snapshot}",
                            DOTENV.zfs_clone_prefix, profile.base_vm_name
                        ),
                        profile::ImageType::Rust => profile
                            .base_image_path(&**snapshot)
                            .as_os_str()
                            .to_str()
                            .expect("Guaranteed by base_image_path()")
                            .to_owned(),
                    });
            info!("profile {key}: {healthy}/{target} healthy runners ({idle} idle, {reserved} reserved, {busy} busy, {started_or_crashed} started or crashed, {excess_idle} excess idle, {wanted} wanted), image {:?} age {image_age:?}", image);
        }
        for (_id, runner) in runners.iter() {
            runner.log_info();
        }

        runners.update_screenshots();
        profiles.update_ipv4_addresses();
        for (_key, profile) in profiles.iter() {
            profile.update_screenshot();
        }

        let mut unregister_and_destroy = |id, runner: &Runner| {
            if runner.registration().is_some() {
                if let Err(error) = runners.unregister_runner(id) {
                    warn!(?error, "Failed to unregister runner: {error}");
                }
            }
            if let Some(profile) = profiles.get(runner.base_vm_name()) {
                if let Err(error) = profiles.destroy_runner(profile, id) {
                    warn!(?error, "Failed to destroy runner: {error}");
                }
            }
            registrations_cache.invalidate();
        };

        if DOTENV.destroy_all_non_busy_runners {
            let non_busy_runners = runners
                .iter()
                .filter(|(_id, runner)| runner.status() != Status::Busy);
            for (&id, runner) in non_busy_runners {
                unregister_and_destroy(id, runner);
            }
        } else {
            // Invalid => unregister and destroy
            // DoneOrUnregistered => destroy (no need to unregister)
            // StartedOrCrashed and too old => unregister and destroy
            // Reserved for too long => unregister and destroy
            // Idle or Busy => bleed off excess Idle runners
            let invalid = runners
                .iter()
                .filter(|(_id, runner)| runner.status() == Status::Invalid);
            let done_or_unregistered = runners
                .iter()
                .filter(|(_id, runner)| runner.status() == Status::DoneOrUnregistered)
                // Don’t destroy unregistered runners if we aren’t registering them in the first place.
                .filter(|_| !DOTENV.dont_register_runners);
            let started_or_crashed_and_too_old = runners.iter().filter(|(_id, runner)| {
                runner.status() == Status::StartedOrCrashed
                    && runner
                        .age()
                        .map_or(true, |age| age > DOTENV.monitor_start_timeout)
            });
            let reserved_for_too_long = runners.iter().filter(|(_id, runner)| {
                runner.status() == Status::Reserved
                    && runner
                        .reserved_since()
                        .ok()
                        .flatten()
                        .map_or(true, |duration| duration > DOTENV.monitor_reserve_timeout)
            });
            let excess_idle_runners = profiles.iter().flat_map(|(_key, profile)| {
                profile
                    .idle_runners(&runners)
                    .take(profiles.excess_idle_runner_count(profile, &runners))
            });
            for (&id, runner) in invalid
                .chain(done_or_unregistered)
                .chain(started_or_crashed_and_too_old)
                .chain(reserved_for_too_long)
                .chain(excess_idle_runners)
            {
                unregister_and_destroy(id, runner);
            }

            let profile_wanted_counts = profiles
                .iter()
                .map(|(_key, profile)| (profile, profiles.wanted_runner_count(profile, &runners)));
            for (profile, wanted_count) in profile_wanted_counts {
                for _ in 0..wanted_count {
                    if let Err(error) = profiles.create_runner(profile, id_gen.next()) {
                        warn!(?error, "Failed to create runner: {error}");
                    }
                    registrations_cache.invalidate();
                }
            }
        }

        // Update dashboard data, for the API.
        if let Ok(mut dashboard) = DASHBOARD.write() {
            *dashboard = Some(Dashboard::render(
                &profiles,
                &profile_runner_counts,
                &runners,
            )?);
        }

        // Handle one request from the API.
        if let Ok(request) = REQUEST.receiver.recv_timeout(DOTENV.monitor_poll_interval) {
            info!(?request, "Received API request");

            match request {
                Request::TakeRunners {
                    response_tx,
                    profile_key: profile,
                    query:
                        TakeRunnerQuery {
                            unique_id,
                            qualified_repo,
                            run_id,
                        },
                    count,
                } => {
                    let mut result = vec![];
                    let matching_runners = runners
                        .iter()
                        .filter(|(_, runner)| {
                            runner.status() == Status::Idle && runner.base_vm_name() == profile
                        })
                        .take(count)
                        .collect::<Vec<_>>();
                    for (&id, runner) in matching_runners {
                        registrations_cache.invalidate();
                        if runners
                            .reserve_runner(id, &unique_id, &qualified_repo, &run_id)
                            .is_ok()
                        {
                            result.push(json!({
                                "id": id,
                                "runner": runner,
                            }));
                        }
                    }
                    let response = if !result.is_empty() {
                        serde_json::to_string(&result)?
                    } else {
                        // TODO: send error when no runners available
                        // TODO: send error when any reservations fail
                        serde_json::to_string(&Option::<()>::None)?
                    };
                    response_tx
                        .send(response)
                        .expect("Failed to send Response to API thread");
                }
                Request::Screenshot {
                    response_tx,
                    runner_id,
                } => {
                    response_tx
                        .send(runners.screenshot_runner(runner_id))
                        .expect("Failed to send Response to API thread");
                }
                Request::GithubJitconfig {
                    response_tx,
                    remote_addr,
                } => {
                    // The monitor runs a loop like (1) update our lists of resources, including guest IPv4 addresses,
                    // (2) wait for up to 5 seconds for a message, (3) handle one message. If the DHCP lease and the
                    // GET /github-jitconfig request both happen in step (2) without step (1) in between, we won’t know
                    // the IPv4 address, so let’s update the IPv4 addresses before continuing.
                    let mut runners = runners;
                    runners.update_ipv4_addresses();

                    response_tx
                        .send(
                            runners
                                .github_jitconfig(remote_addr)
                                .map(|result| result.map(|ip| ip.to_owned())),
                        )
                        .expect("Failed to send Response to API thread");
                }
                Request::BootScript {
                    response_tx,
                    remote_addr,
                } => {
                    // The monitor runs a loop like (1) update our lists of resources, including guest IPv4 addresses,
                    // (2) wait for up to 5 seconds for a message, (3) handle one message. If the DHCP lease and the
                    // GET /github-jitconfig request both happen in step (2) without step (1) in between, we won’t know
                    // the IPv4 address, so let’s update the IPv4 addresses before continuing.
                    let mut runners = runners;
                    runners.update_ipv4_addresses();
                    profiles.update_ipv4_addresses();

                    let result = profiles.boot_script(remote_addr);
                    response_tx
                        .send(result)
                        .expect("Failed to send Response to API thread");
                }
            }
        } else {
            info!("Did not receive an API request");
        }
    }
}
