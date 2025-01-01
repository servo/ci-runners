mod dashboard;
mod data;
mod github;
mod id;
mod libvirt;
mod profile;
mod runner;
mod settings;
mod shell;
mod zfs;

use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    net::IpAddr,
    process::exit,
    sync::{LazyLock, RwLock},
    thread,
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use dotenv::dotenv;
use http::StatusCode;
use jane_eyre::eyre::{self, eyre};
use mktemp::Temp;
use serde_json::json;
use tracing::{error, info, trace, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use warp::{
    filters::reply::header,
    reject::{self, Reject, Rejection},
    reply::{self, Reply},
    Filter,
};

use crate::{
    dashboard::Dashboard,
    github::{list_registered_runners_for_host, Cache},
    id::IdGen,
    libvirt::list_runner_guests,
    profile::RunnerCounts,
    runner::{Runner, Runners, Status},
    settings::{Dotenv, Toml},
    zfs::list_runner_volumes,
};

static DOTENV: LazyLock<Dotenv> = LazyLock::new(|| {
    dotenv().expect("Failed to load variables from .env");
    Dotenv::load()
});

static TOML: LazyLock<Toml> =
    LazyLock::new(|| Toml::load_default().expect("Failed to load settings from monitor.toml"));

/// GET `/` => `{"profile_runner_counts": {}, "runners": []}`
static DASHBOARD: RwLock<Option<Dashboard>> = RwLock::new(None);

static JSON: &str = "application/json; charset=utf-8";
static PNG: &str = "image/png";

#[derive(Debug)]
enum Request {
    /// POST `/<profile>/<unique id>/<user>/<repo>/<run id>` => `{"id", "runner"}` | `null`
    TakeRunner {
        response_tx: Sender<String>,
        profile: String,
        unique_id: String,
        user: String,
        repo: String,
        run_id: String,
    },

    /// GET `/runner/<our runner id>/screenshot` => image/png
    Screenshot {
        response_tx: Sender<eyre::Result<Temp>>,
        runner_id: usize,
    },
}

#[derive(Debug)]
struct NotReadyError(eyre::Report);
impl Reject for NotReadyError {}
#[derive(Debug)]
struct ChannelError(eyre::Report);
impl Reject for ChannelError {}

struct Channel<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}
static REQUEST: LazyLock<Channel<Request>> = LazyLock::new(|| {
    let (sender, receiver) = crossbeam_channel::bounded(0);
    Channel { sender, receiver }
});

#[tokio::main]
async fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    if std::env::var_os("RUST_LOG").is_none() {
        // EnvFilter Builder::with_default_directive doesn’t support multiple directives,
        // so we need to apply defaults ourselves.
        std::env::set_var("RUST_LOG", "monitor=info,warp::server=info");
    }
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::builder().from_env_lossy())
        .init();

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

    let dashboard_route = warp::path!()
        .and(warp::filters::method::get())
        .and_then(|| async {
            DASHBOARD
                .try_read()
                .ok()
                .and_then(|x| x.clone())
                .map(|x| x.json)
                .ok_or_else(|| {
                    reject::custom(NotReadyError(eyre!(
                        "Monitor thread is still starting or not responding"
                    )))
                })
        })
        .with(header("Content-Type", JSON));

    let take_runner_route = warp::path!(String / String / String / String / String)
        .and(warp::filters::method::post())
        .and(warp::filters::header::exact(
            "Authorization",
            &DOTENV.monitor_api_token_authorization_value,
        ))
        .and_then(|profile, unique_id, user, repo, run_id| async {
            || -> eyre::Result<String> {
                let (response_tx, response_rx) = crossbeam_channel::bounded(0);
                REQUEST.sender.send_timeout(
                    Request::TakeRunner {
                        response_tx,
                        profile,
                        unique_id,
                        user,
                        repo,
                        run_id,
                    },
                    DOTENV.monitor_thread_send_timeout,
                )?;
                Ok(response_rx.recv_timeout(DOTENV.monitor_thread_recv_timeout)?)
            }()
            .map_err(|error| reject::custom(ChannelError(error)))
        })
        .with(header("Content-Type", JSON));

    let screenshot_route = warp::path!("runner" / usize / "screenshot")
        .and(warp::filters::method::get())
        .and_then(|runner_id| async move {
            || -> eyre::Result<Vec<u8>> {
                let (response_tx, response_rx) = crossbeam_channel::bounded(0);
                REQUEST.sender.send_timeout(
                    Request::Screenshot {
                        response_tx,
                        runner_id,
                    },
                    DOTENV.monitor_thread_send_timeout,
                )?;
                let path = response_rx.recv_timeout(DOTENV.monitor_thread_recv_timeout)??;
                let mut file = File::open(&path)?; // borrow to avoid dropping Temp
                let mut result = vec![];
                file.read_to_end(&mut result)?;
                Ok(result)
            }()
            .map_err(|error| reject::custom(ChannelError(error)))
        })
        .with(header("Content-Type", PNG));

    // Successful responses are in their own types. Error responses are in plain text.
    let routes = dashboard_route.or(take_runner_route).or(screenshot_route);
    let routes = routes.recover(recover);

    warp::serve(routes)
        .run(("::1".parse::<IpAddr>()?, 8000))
        .await;

    Ok(())
}

async fn recover(error: Rejection) -> Result<impl Reply, std::convert::Infallible> {
    Ok(if let Some(error) = error.find::<NotReadyError>() {
        error!(
            ?error,
            "NotReadyError: responding with HTTP 503 Service Unavailable: {}", error.0
        );
        reply::with_status(format!("{}", error.0), StatusCode::SERVICE_UNAVAILABLE)
    } else if let Some(error) = error.find::<ChannelError>() {
        error!(
            ?error,
            "ChannelError: responding with HTTP 500 Internal Server Error: {}", error.0
        );
        reply::with_status(
            format!("Channel error: {}", error.0),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    } else {
        error!(
            ?error,
            "Unknown error: responding with HTTP 500 Internal Server Error",
        );
        reply::with_status(
            format!("Unknown error: {error:?}"),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    })
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

    let mut registrations_cache = Cache::default();

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
        let profile_runner_counts: BTreeMap<_, _> = TOML
            .profiles()
            .map(|(key, profile)| (key, profile.runner_counts(&runners)))
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
            },
        ) in profile_runner_counts.iter()
        {
            info!("profile {key}: {healthy}/{target} healthy runners ({idle} idle, {reserved} reserved, {busy} busy, {started_or_crashed} started or crashed, {excess_idle} excess idle, {wanted} wanted)");
        }
        for (_id, runner) in runners.iter() {
            runner.log_info();
        }

        let mut unregister_and_destroy = |id, runner: &Runner| {
            if runner.registration().is_some() {
                if let Err(error) = runners.unregister_runner(id) {
                    warn!(?error, "Failed to unregister runner: {error}");
                }
            }
            if let Some(profile) = TOML.profile(runner.base_vm_name()) {
                if let Err(error) = profile.destroy_runner(id) {
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
            let excess_idle_runners = TOML.profiles().flat_map(|(_key, profile)| {
                profile
                    .idle_runners(&runners)
                    .take(profile.excess_idle_runner_count(&runners))
            });
            for (&id, runner) in invalid
                .chain(done_or_unregistered)
                .chain(started_or_crashed_and_too_old)
                .chain(reserved_for_too_long)
                .chain(excess_idle_runners)
            {
                unregister_and_destroy(id, runner);
            }

            let profile_wanted_counts = TOML
                .profiles()
                .map(|(_key, profile)| (profile, profile.wanted_runner_count(&runners)));
            for (profile, wanted_count) in profile_wanted_counts {
                for _ in 0..wanted_count {
                    if let Err(error) = profile.create_runner(id_gen.next()) {
                        warn!(?error, "Failed to create runner: {error}");
                    }
                    registrations_cache.invalidate();
                }
            }
        }

        // Update status, for the API.
        if let Ok(mut status) = DASHBOARD.write() {
            *status = Some(Dashboard::render(&profile_runner_counts, &runners)?);
        }

        // Handle one request from the API.
        if let Ok(request) = REQUEST.receiver.recv_timeout(DOTENV.monitor_poll_interval) {
            info!(?request, "Received API request");

            match request {
                Request::TakeRunner {
                    response_tx,
                    profile,
                    unique_id,
                    user,
                    repo,
                    run_id,
                } => {
                    let response = if let Some((&id, runner)) =
                        runners.iter().find(|(_, runner)| {
                            runner.status() == Status::Idle && runner.base_vm_name() == profile
                        }) {
                        registrations_cache.invalidate();
                        if runners
                            .reserve_runner(id, &unique_id, &user, &repo, &run_id)
                            .is_ok()
                        {
                            serde_json::to_string(&json!({
                                "id": id,
                                "runner": runner,
                            }))?
                        } else {
                            // TODO: send error when reservation fails
                            serde_json::to_string(&Option::<()>::None)?
                        }
                    } else {
                        // TODO: send error when no runners available
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
            }
        } else {
            info!("Did not receive an API request");
        }
    }
}
