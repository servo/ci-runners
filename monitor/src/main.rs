mod data;
mod github;
mod id;
mod libvirt;
mod profile;
mod runner;
mod settings;
mod zfs;

use std::{
    collections::BTreeMap,
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
use log::{error, info, trace, warn};
use serde_json::json;
use warp::{
    filters::reply::header,
    reject::{self, Reject, Rejection},
    reply::{self, Reply},
    Filter,
};

use crate::{
    github::{list_registered_runners_for_host, Cache},
    id::IdGen,
    libvirt::list_runner_guests,
    profile::{Profile, Profiles, RunnerCounts},
    runner::{Runner, Runners, Status},
    settings::Settings,
    zfs::list_runner_volumes,
};

static SETTINGS: LazyLock<Settings> = LazyLock::new(|| {
    dotenv().expect("Failed to load variables from .env");
    Settings::load()
});

/// GET `/` => `{"profile_runner_counts": {}, "runners": []}`
static STATUS: RwLock<Option<String>> = RwLock::new(None);

#[derive(Debug)]
enum Request {
    /// POST `/<profile>/<unique id>/<user>/<repo>/<run id>` => `{"id", "runner"}` | `null`
    TakeRunner {
        profile: String,
        unique_id: String,
        user: String,
        repo: String,
        run_id: String,
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
    let (sender, receiver) = crossbeam_channel::bounded(1);
    Channel { sender, receiver }
});
static RESPONSE: LazyLock<Channel<String>> = LazyLock::new(|| {
    let (sender, receiver) = crossbeam_channel::bounded(1);
    Channel { sender, receiver }
});

#[tokio::main]
async fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    env_logger::init();

    tokio::task::spawn(async move {
        let thread = thread::spawn(monitor_thread);
        loop {
            if thread.is_finished() {
                match thread.join() {
                    Ok(Ok(())) => {
                        info!("Monitor thread exited");
                        exit(0);
                    }
                    Ok(Err(report)) => error!("Monitor thread error: {report}"),
                    Err(panic) => error!("Monitor thread panic: {panic:?}"),
                };
                exit(1);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    let status_route = warp::path!()
        .and(warp::filters::method::get())
        .and_then(|| async {
            STATUS
                .try_read()
                .ok()
                .map(|x| x.clone())
                .flatten()
                .ok_or_else(|| {
                    reject::custom(NotReadyError(eyre!(
                        "Monitor thread is still starting or not responding"
                    )))
                })
        });

    let take_runner_route = warp::path!(String / String / String / String / String)
        .and(warp::filters::method::post())
        .and(warp::filters::header::exact(
            "Authorization",
            &SETTINGS.monitor_api_token_authorization_value,
        ))
        .and_then(|profile, unique_id, user, repo, run_id| async {
            || -> eyre::Result<String> {
                REQUEST.sender.send_timeout(
                    Request::TakeRunner {
                        profile,
                        unique_id,
                        user,
                        repo,
                        run_id,
                    },
                    SETTINGS.monitor_thread_send_timeout,
                )?;
                Ok(RESPONSE
                    .receiver
                    .recv_timeout(SETTINGS.monitor_thread_recv_timeout)?)
            }()
            .map_err(|error| reject::custom(ChannelError(error)))
        });

    // Successful responses are in JSON. Error responses are in plain text.
    let routes = status_route.or(take_runner_route);
    let routes = routes.with(header("Content-Type", "application/json; charset=utf-8"));
    let routes = routes.recover(recover);

    warp::serve(routes)
        .run(("::1".parse::<IpAddr>()?, 8000))
        .await;

    Ok(())
}

async fn recover(error: Rejection) -> Result<impl Reply, std::convert::Infallible> {
    Ok(if let Some(error) = error.find::<NotReadyError>() {
        reply::with_status(format!("{}", error.0), StatusCode::SERVICE_UNAVAILABLE)
    } else if let Some(error) = error.find::<ChannelError>() {
        reply::with_status(
            format!("Channel error: {}", error.0),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    } else {
        reply::with_status(
            format!("Internal error: {error:?}"),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    })
}

/// The monitor thread is our single source of truth.
///
/// It handles one [`Request`] at a time, polling for updated resources before
/// each request, then sends one response to the API server for each request.
fn monitor_thread() -> eyre::Result<()> {
    let mut profiles = Profiles::default();
    profiles.insert(
        "servo-windows10",
        Profile {
            configuration_name: "windows10".to_owned(),
            base_vm_name: "servo-windows10".to_owned(),
            base_image_snapshot: "ready".to_owned(),
            github_runner_label: "self-hosted-image:windows10".to_owned(),
            target_count: 2,
        },
    );
    profiles.insert(
        "servo-windows10.new",
        Profile {
            configuration_name: "windows10".to_owned(),
            base_vm_name: "servo-windows10.new".to_owned(),
            base_image_snapshot: "ready".to_owned(),
            github_runner_label: "self-hosted-image:windows10.new".to_owned(),
            target_count: 0,
        },
    );
    profiles.insert(
        "servo-ubuntu2204",
        Profile {
            configuration_name: "ubuntu2204".to_owned(),
            base_vm_name: "servo-ubuntu2204".to_owned(),
            base_image_snapshot: "ready".to_owned(),
            github_runner_label: "self-hosted-image:ubuntu2204".to_owned(),
            target_count: 2,
        },
    );
    profiles.insert(
        "servo-ubuntu2204.new",
        Profile {
            configuration_name: "ubuntu2204".to_owned(),
            base_vm_name: "servo-ubuntu2204.new".to_owned(),
            base_image_snapshot: "ready".to_owned(),
            github_runner_label: "self-hosted-image:ubuntu2204.new".to_owned(),
            target_count: 0,
        },
    );

    let mut id_gen = IdGen::new_load().unwrap_or_else(|error| {
        warn!("{error}");
        IdGen::new_empty()
    });

    let mut registrations_cache = Cache::default();

    loop {
        let registrations = registrations_cache.get(|| list_registered_runners_for_host())?;
        let guests = list_runner_guests()?;
        let volumes = list_runner_volumes()?;
        trace!("registrations = {:?}", registrations);
        trace!("guests = {:?}", guests);
        trace!("volumes = {:?}", volumes);
        info!(
            "{} registrations, {} guests, {} volumes",
            registrations.len(),
            guests.len(),
            volumes.len()
        );

        let runners = Runners::new(registrations, guests, volumes);
        let profile_runner_counts: BTreeMap<_, _> = profiles
            .iter()
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
                    warn!("Failed to unregister runner: {error}");
                }
            }
            if let Some(profile) = profiles.get(runner.base_vm_name()) {
                if let Err(error) = profile.destroy_runner(id) {
                    warn!("Failed to destroy runner: {error}");
                }
            }
            registrations_cache.invalidate();
        };

        if SETTINGS.destroy_all_non_busy_runners {
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
                .filter(|_| !SETTINGS.dont_register_runners);
            let started_or_crashed_and_too_old = runners.iter().filter(|(_id, runner)| {
                runner.status() == Status::StartedOrCrashed
                    && runner
                        .age()
                        .map_or(true, |age| age > SETTINGS.monitor_start_timeout)
            });
            let reserved_for_too_long = runners.iter().filter(|(_id, runner)| {
                runner.status() == Status::Reserved
                    && runner
                        .reserved_since()
                        .ok()
                        .flatten()
                        .map_or(true, |duration| duration > SETTINGS.monitor_reserve_timeout)
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
                        warn!("Failed to create runner: {error}");
                    }
                    registrations_cache.invalidate();
                }
            }
        }

        // Update status, for the API.
        if let Ok(mut status) = STATUS.write() {
            *status = Some(serde_json::to_string(&json!({
                "profile_runner_counts": &profile_runner_counts,
                "runners": &runners
                    .iter()
                    .map(|(id, runner)| {
                        json!({
                            "id": id,
                            "runner": runner,
                        })
                    })
                    .collect::<Vec<_>>(),
            }))?);
        }

        // Handle one request from the API.
        if let Ok(request) = REQUEST
            .receiver
            .recv_timeout(SETTINGS.monitor_poll_interval)
        {
            info!("Received API request: {request:?}");

            let response = match request {
                Request::TakeRunner {
                    profile,
                    unique_id,
                    user,
                    repo,
                    run_id,
                } => {
                    if let Some((&id, runner)) = runners.iter().find(|(_, runner)| {
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
                    }
                }
            };

            RESPONSE
                .sender
                .send(response)
                .expect("Failed to send Response to API thread");
        } else {
            info!("Did not receive an API request");
        }
    }
}
