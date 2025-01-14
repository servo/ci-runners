mod dashboard;
mod data;
mod github;
mod id;
mod image;
mod libvirt;
mod profile;
mod runner;
mod settings;
mod shell;
mod zfs;

use core::str;
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::Read,
    net::IpAddr,
    path::Path,
    process::exit,
    str::FromStr,
    sync::{LazyLock, RwLock},
    thread::{self},
    time::{Duration, UNIX_EPOCH},
};

use askama::Template;
use crossbeam_channel::{Receiver, Sender};
use dotenv::dotenv;
use http::{StatusCode, Uri};
use jane_eyre::eyre::{self, eyre, Context, OptionExt};
use mktemp::Temp;
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info, trace, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use warp::{
    filters::reply::header,
    redirect::see_other,
    reject::{self, Reject, Rejection},
    reply::{self, with_header, Reply},
    Filter,
};

use crate::{
    dashboard::Dashboard,
    data::{get_profile_data_path, get_runner_data_path, run_migrations},
    github::{list_registered_runners_for_host, Cache},
    id::IdGen,
    image::Rebuilds,
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

static DASHBOARD: RwLock<Option<Dashboard>> = RwLock::new(None);

static HTML: &str = "text/html; charset=utf-8";
static JSON: &str = "application/json; charset=utf-8";
static PNG: &str = "image/png";

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
    /// POST `/<profile_key>/<unique id>/<user>/<repo>/<run id>` => `{"id", "runner"}` | `null`
    /// POST `/profile/<profile_key>/take?unique_id&qualified_repo=<user>/<repo>&run_id` => `{"id", "runner"}` | `null`
    TakeRunner {
        response_tx: Sender<String>,
        profile_key: String,
        query: TakeRunnerQuery,
    },

    /// GET `/runner/<our runner id>/screenshot/now` => image/png
    Screenshot {
        response_tx: Sender<eyre::Result<Temp>>,
        runner_id: usize,
    },
}
#[derive(Debug, Deserialize)]
struct TakeRunnerQuery {
    unique_id: String,
    qualified_repo: String,
    run_id: String,
}

#[derive(Debug)]
struct NotReadyError(eyre::Report);
impl Reject for NotReadyError {}
#[derive(Debug)]
struct ChannelError(eyre::Report);
impl Reject for ChannelError {}
#[derive(Debug)]
struct InternalError(eyre::Report);
impl Reject for InternalError {}

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

    let index_route = warp::path!()
        .and(warp::filters::method::get())
        .and_then(|| async {
            DASHBOARD
                .read()
                .map_err(|e| {
                    reject::custom(NotReadyError(eyre!("Failed to acquire RwLock: {e:?}")))
                })?
                .as_ref()
                .map(|d| IndexTemplate {
                    content: d.html.clone(),
                })
                .ok_or_else(|| {
                    reject::custom(NotReadyError(eyre!(
                        "Monitor thread is still starting or not responding"
                    )))
                })
        })
        .with(header("Content-Type", HTML));

    let dashboard_html_route = warp::path!("dashboard.html")
        .and(warp::filters::method::get())
        .and_then(|| async {
            DASHBOARD
                .read()
                .map_err(|e| {
                    reject::custom(NotReadyError(eyre!("Failed to acquire RwLock: {e:?}")))
                })?
                .as_ref()
                .map(|d| d.html.clone())
                .ok_or_else(|| {
                    reject::custom(NotReadyError(eyre!(
                        "Monitor thread is still starting or not responding"
                    )))
                })
        })
        .with(header("Content-Type", HTML));

    let dashboard_json_route = warp::path!("dashboard.json")
        .and(warp::filters::method::get())
        .and_then(|| async {
            DASHBOARD
                .read()
                .map_err(|e| {
                    reject::custom(NotReadyError(eyre!("Failed to acquire RwLock: {e:?}")))
                })?
                .as_ref()
                .map(|x| x.json.clone())
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
        .and_then(|profile_key, unique_id, user, repo, run_id| async move {
            || -> eyre::Result<String> {
                let (response_tx, response_rx) = crossbeam_channel::bounded(0);
                REQUEST.sender.send_timeout(
                    Request::TakeRunner {
                        response_tx,
                        profile_key,
                        query: TakeRunnerQuery {
                            unique_id,
                            qualified_repo: format!("{user}/{repo}"),
                            run_id,
                        },
                    },
                    DOTENV.monitor_thread_send_timeout,
                )?;
                Ok(response_rx.recv_timeout(DOTENV.monitor_thread_recv_timeout)?)
            }()
            .map_err(|error| reject::custom(ChannelError(error)))
        })
        .with(header("Content-Type", JSON));

    let take_runner_route2 = warp::path!("profile" / String / "take")
        .and(warp::filters::method::post())
        .and(warp::filters::header::exact(
            "Authorization",
            &DOTENV.monitor_api_token_authorization_value,
        ))
        .and(warp::filters::query::query())
        .and_then(|profile_key, query: TakeRunnerQuery| async {
            || -> eyre::Result<String> {
                let (response_tx, response_rx) = crossbeam_channel::bounded(0);
                REQUEST.sender.send_timeout(
                    Request::TakeRunner {
                        response_tx,
                        profile_key,
                        query,
                    },
                    DOTENV.monitor_thread_send_timeout,
                )?;
                Ok(response_rx.recv_timeout(DOTENV.monitor_thread_recv_timeout)?)
            }()
            .map_err(|error| reject::custom(ChannelError(error)))
        })
        .with(header("Content-Type", JSON));

    let profile_screenshot_route =
        warp::path!("profile" / String / "screenshot.png")
            .and(warp::filters::method::get())
            .and(warp::filters::header::optional("If-None-Match"))
            .and(warp::filters::query::query())
            .and_then(
                |profile_key: String,
                 if_none_match: Option<String>,
                 query: HashMap<String, String>| async move {
                    if !query.is_empty() {
                        // If the page cache-busts the <img src> to force the browser to revalidate,
                        // redirect to the bare url, so the browser can send its If-Modified-Since
                        // <https://stackoverflow.com/a/9505557>
                        let url = Uri::from_str(&format!("/profile/{profile_key}/screenshot.png"))
                            .wrap_err("failed to build Uri")
                            .map_err(InternalError)?;
                        return Ok(Box::new(see_other(url)) as Box<dyn Reply>);
                    }
                    let path = get_profile_data_path(&profile_key, Path::new("screenshot.png"))
                        .wrap_err("Failed to compute path")
                        .map_err(InternalError)?;
                    serve_static_file(path, if_none_match)
                },
            )
            .with(header("Content-Type", PNG));

    let runner_screenshot_route = warp::path!("runner" / usize / "screenshot.png")
        .and(warp::filters::method::get())
        .and(warp::filters::header::optional("If-None-Match"))
        .and(warp::filters::query::query())
        .and_then(
            |runner_id, if_none_match: Option<String>, query: HashMap<String, String>| async move {
                if !query.is_empty() {
                    // If the page cache-busts the <img src> to force the browser to revalidate,
                    // redirect to the bare url, so the browser can send its If-Modified-Since
                    // <https://stackoverflow.com/a/9505557>
                    let url = Uri::from_str(&format!("/runner/{runner_id}/screenshot.png"))
                        .wrap_err("failed to build Uri")
                        .map_err(InternalError)?;
                    return Ok(Box::new(see_other(url)) as Box<dyn Reply>);
                }
                let path = get_runner_data_path(runner_id, Path::new("screenshot.png"))
                    .wrap_err("Failed to compute path")
                    .map_err(InternalError)?;
                serve_static_file(path, if_none_match)
            },
        )
        .with(header("Content-Type", PNG));

    let runner_screenshot_now_route = warp::path!("runner" / usize / "screenshot" / "now")
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
    let routes = index_route
        .or(dashboard_html_route)
        .or(dashboard_json_route)
        .or(take_runner_route)
        .or(take_runner_route2)
        .or(profile_screenshot_route)
        .or(runner_screenshot_route)
        .or(runner_screenshot_now_route);
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
    } else if let Some(error) = error.find::<InternalError>() {
        error!(
            ?error,
            "InternalError: responding with HTTP 500 Internal Server Error: {}", error.0
        );
        reply::with_status(
            format!("Internal error: {}", error.0),
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

fn serve_static_file(
    path: impl AsRef<Path>,
    if_none_match: Option<String>,
) -> Result<Box<dyn Reply>, Rejection> {
    let mut file = File::open(path)
        .wrap_err("Failed to open file")
        .map_err(InternalError)?;
    let metadata = file
        .metadata()
        .wrap_err("Failed to get metadata")
        .map_err(InternalError)?;
    let mtime = metadata
        .modified()
        .wrap_err("Failed to get mtime")
        .map_err(InternalError)?
        .duration_since(UNIX_EPOCH)
        .wrap_err("Failed to compute mtime")
        .map_err(InternalError)?
        .as_millis();
    let etag = format!(r#""{mtime}""#);

    Ok::<_, Rejection>(if if_none_match.is_some_and(|inm| inm == etag) {
        Box::new(StatusCode::NOT_MODIFIED) as Box<dyn Reply>
    } else {
        let mut result = vec![];
        file.read_to_end(&mut result)
            .wrap_err("Failed to read file")
            .map_err(InternalError)?;
        Box::new(with_header(result, "ETag", etag)) as Box<dyn Reply>
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

    let mut profiles = TOML.initial_profiles();
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
            .map(|(key, profile)| (key.clone(), profile.runner_counts(&runners)))
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
            info!("profile {key}: {healthy}/{target} healthy runners ({idle} idle, {reserved} reserved, {busy} busy, {started_or_crashed} started or crashed, {excess_idle} excess idle, {wanted} wanted), image {}/{}@{} age {image_age:?}", DOTENV.zfs_clone_prefix, profile.base_vm_name, profile.base_image_snapshot);
        }
        for (_id, runner) in runners.iter() {
            runner.log_info();
        }

        runners.update_screenshots();
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
            let excess_idle_runners = profiles.iter().flat_map(|(_key, profile)| {
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

            let profile_wanted_counts = profiles
                .iter()
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
                Request::TakeRunner {
                    response_tx,
                    profile_key: profile,
                    query:
                        TakeRunnerQuery {
                            unique_id,
                            qualified_repo,
                            run_id,
                        },
                } => {
                    let response = if let Some((&id, runner)) =
                        runners.iter().find(|(_, runner)| {
                            runner.status() == Status::Idle && runner.base_vm_name() == profile
                        }) {
                        registrations_cache.invalidate();
                        if runners
                            .reserve_runner(id, &unique_id, &qualified_repo, &run_id)
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
