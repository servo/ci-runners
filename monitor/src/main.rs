mod dashboard;
mod data;
mod github;
mod id;
mod image;
mod libvirt;
mod policy;
mod runner;
mod shell;
#[cfg_attr(not(target_os = "macos"), path = "utm_dummy.rs")]
mod utm;

use core::str;
use std::{
    collections::BTreeMap,
    env,
    fs::File,
    path::Path,
    process::exit,
    sync::{LazyLock, RwLock},
    thread::{self},
    time::Duration,
};

use askama::Template;
use askama_web::WebTemplate;
use chrono::Utc;
use crossbeam_channel::{Receiver, Sender};
use jane_eyre::eyre::{self, eyre, Context, OptionExt};
use mktemp::Temp;
use rocket::{
    delete,
    fs::{FileServer, NamedFile},
    get,
    http::ContentType,
    post,
    response::content::{RawJson, RawText},
    serde::json::Json,
};
use serde::Deserialize;
use serde_json::json;
use settings::{IMAGE_DEPS_DIR, TOML};
use tokio::task::JoinSet;
use tracing::{debug, error, info, trace, warn};
use web::{
    auth::ApiKeyGuard,
    rocket_eyre::{self, EyreReport},
};

use crate::{
    dashboard::Dashboard,
    data::{get_profile_data_path, get_runner_data_path, run_migrations},
    github::{
        download_artifact_string, list_registered_runners_for_host, list_workflow_run_artifacts,
        Cache,
    },
    id::IdGen,
    image::{start_libvirt_guest, Rebuilds},
    libvirt::list_runner_guests,
    policy::{base_image_path, Override, Policy, RunnerCounts},
    runner::{Runners, Status},
};

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

    /// GET `/policy/override`
    GetOverridePolicy {
        response_tx: Sender<Option<Override>>,
    },

    /// POST `/policy/override?<profile_key...>=<count>` => `{"<profile_key...>": <count...>}`
    OverridePolicy {
        response_tx: Sender<eyre::Result<Override>>,
        profile_override_counts: BTreeMap<String, usize>,
    },

    /// DELETE `/policy/override` => `{"<profile_key...>": <count...>}`
    CancelOverridePolicy {
        response_tx: Sender<eyre::Result<Option<Override>>>,
    },

    /// GET `/runner/<our runner id>/screenshot/now` => image/png
    Screenshot {
        response_tx: Sender<eyre::Result<Temp>>,
        runner_id: usize,
    },

    /// - GET `/github-jitconfig` => application/json
    GithubJitconfig {
        response_tx: Sender<eyre::Result<Option<String>>>,
        remote_addr: web::auth::RemoteAddr,
    },

    /// - GET `/boot` => text/plain
    BootScript {
        response_tx: Sender<eyre::Result<String>>,
        remote_addr: web::auth::RemoteAddr,
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

#[derive(Clone, Debug, Template, WebTemplate)]
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
        TOML.monitor_thread_send_timeout(),
    )?;
    let result = response_rx.recv_timeout(TOML.monitor_thread_recv_timeout())?;

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
        TOML.monitor_thread_send_timeout(),
    )?;
    let result = response_rx.recv_timeout(
        // TODO: make this configurable?
        TOML.monitor_thread_recv_timeout() + Duration::from_secs(count as u64),
    )?;

    Ok(RawJson(result))
}

#[post("/select-runner?<unique_id>&<qualified_repo>&<run_id>")]
fn select_runner_route(
    unique_id: String,
    qualified_repo: String,
    run_id: String,
) -> rocket_eyre::Result<RawJson<String>> {
    if !qualified_repo.starts_with(&TOML.allowed_qualified_repo_prefix) {
        Err(EyreReport::InternalServerError(eyre!(
            "Not allowed on this `qualified_repo`"
        )))?;
    }
    let artifacts = list_workflow_run_artifacts(&qualified_repo, &run_id)?;
    let args_artifact = format!("servo-ci-runners_{unique_id}");
    let Some(args_artifact) = artifacts
        .into_iter()
        .find(|artifact| artifact.name == args_artifact)
    else {
        Err(EyreReport::InternalServerError(eyre!(
            "No args artifact found: {args_artifact}"
        )))?
    };
    let artifact_age = Utc::now().signed_duration_since(args_artifact.created_at);
    if artifact_age > TOML.tokenless_select_artifact_max_age() {
        Err(EyreReport::InternalServerError(eyre!(
            "Args artifact is too old ({}): {}",
            artifact_age,
            args_artifact.name,
        )))?
    }
    let args_artifact = download_artifact_string(&args_artifact.archive_download_url)?;
    let mut args = args_artifact
        .lines()
        .flat_map(|line| line.split_once("="))
        .collect::<BTreeMap<&str, &str>>();
    if args.remove("unique_id") != Some(&*unique_id) {
        Err(EyreReport::InternalServerError(eyre!(
            "Wrong unique_id in artifact"
        )))?;
    }
    if args.remove("qualified_repo") != Some(&*qualified_repo) {
        Err(EyreReport::InternalServerError(eyre!(
            "Wrong qualified_repo in artifact"
        )))?;
    }
    if args.remove("run_id") != Some(&*run_id) {
        Err(EyreReport::InternalServerError(eyre!(
            "Wrong run_id in artifact"
        )))?;
    }
    let Some(profile_key) = args.remove("self_hosted_image_name") else {
        Err(EyreReport::InternalServerError(eyre!(
            "Wrong run_id in artifact"
        )))?
    };

    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::TakeRunners {
            response_tx,
            profile_key: profile_key.to_owned(),
            query: TakeRunnerQuery {
                unique_id,
                qualified_repo,
                run_id,
            },
            count: 1,
        },
        TOML.monitor_thread_send_timeout(),
    )?;
    let result = response_rx.recv_timeout(TOML.monitor_thread_recv_timeout())?;

    Ok(RawJson(result))
}

#[get("/policy/override")]
fn get_override_policy_route() -> rocket_eyre::Result<Json<Option<Override>>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::GetOverridePolicy { response_tx },
        TOML.monitor_thread_send_timeout(),
    )?;

    Ok(Json(
        response_rx.recv_timeout(TOML.monitor_thread_recv_timeout())?,
    ))
}

#[post("/policy/override?<profile_override_counts..>")]
fn override_policy_route(
    profile_override_counts: BTreeMap<String, usize>,
    _auth: ApiKeyGuard,
) -> rocket_eyre::Result<Json<Override>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::OverridePolicy {
            response_tx,
            profile_override_counts,
        },
        TOML.monitor_thread_send_timeout(),
    )?;

    Ok(Json(
        response_rx.recv_timeout(TOML.monitor_thread_recv_timeout())??,
    ))
}

#[delete("/policy/override")]
fn delete_override_policy_route(_auth: ApiKeyGuard) -> rocket_eyre::Result<Json<Option<Override>>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::CancelOverridePolicy { response_tx },
        TOML.monitor_thread_send_timeout(),
    )?;

    Ok(Json(
        response_rx.recv_timeout(TOML.monitor_thread_recv_timeout())??,
    ))
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
        TOML.monitor_thread_send_timeout(),
    )?;
    let path = response_rx.recv_timeout(TOML.monitor_thread_recv_timeout())??;
    debug!(?path);

    // Moving `path` into File ensures Temp is not dropped until close
    Ok((ContentType::PNG, File::open(path)?))
}

#[get("/github-jitconfig")]
fn github_jitconfig_route(
    remote_addr: web::auth::RemoteAddr,
) -> rocket_eyre::Result<RawJson<String>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::GithubJitconfig {
            response_tx,
            remote_addr,
        },
        TOML.monitor_thread_send_timeout(),
    )?;
    let result = response_rx
        .recv_timeout(TOML.monitor_thread_recv_timeout())?
        .map_err(EyreReport::ServiceUnavailable)?;

    Ok(RawJson(json!(result).to_string()))
}

#[get("/boot")]
fn boot_script_route(remote_addr: web::auth::RemoteAddr) -> rocket_eyre::Result<RawText<String>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::BootScript {
            response_tx,
            remote_addr,
        },
        TOML.monitor_thread_send_timeout(),
    )?;
    let result = response_rx
        .recv_timeout(TOML.monitor_thread_recv_timeout())?
        .map_err(EyreReport::ServiceUnavailable)?;

    Ok(RawText(result))
}

#[rocket::main]
async fn main() -> eyre::Result<()> {
    if env::var_os("RUST_LOG").is_none() {
        // EnvFilter Builder::with_default_directive doesn’t support multiple directives,
        // so we need to apply defaults ourselves.
        env::set_var("RUST_LOG", "monitor=info,rocket=info,cmd_lib::child=info");
    }
    cli::init()?;
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
                select_runner_route,
                get_override_policy_route,
                override_policy_route,
                delete_override_policy_route,
                profile_screenshot_route,
                runner_screenshot_route,
                runner_screenshot_now_route,
                github_jitconfig_route,
                boot_script_route,
            ],
        )
        .mount(
            "/image-deps/",
            FileServer::new(&*IMAGE_DEPS_DIR, rocket::fs::Options::NormalizeDirs),
        )
        .mount(
            "/cache/servo/",
            FileServer::new(
                &TOML.main_repo_path,
                rocket::fs::Options::NormalizeDirs | rocket::fs::Options::DotFiles,
            ),
        )
        .launch()
    };

    let mut set = JoinSet::new();
    for address in TOML.listen_on.iter() {
        set.spawn(rocket(&address));
    }
    for result in set.join_all().await {
        result?;
    }

    Ok(())
}

/// The monitor thread is our single source of truth.
///
/// It handles one [`Request`] at a time, polling for updated resources before
/// each request, then sends one response to the API server for each request.
fn monitor_thread() -> eyre::Result<()> {
    #[cfg(target_os = "macos")]
    crate::utm::request_automation_permission()?;

    let mut id_gen = IdGen::new_load().unwrap_or_else(|error| {
        warn!(?error, "Failed to read last-runner-id: {error}");
        IdGen::new_empty()
    });

    let mut policy = Policy::new(TOML.initial_profiles())?;
    let mut registrations_cache = Cache::default();
    let mut image_rebuilds = Rebuilds::default();
    policy.read_base_image_snapshots()?;

    loop {
        let registrations = registrations_cache.get(|| list_registered_runners_for_host())?;
        let guests = list_runner_guests()?;
        trace!(?registrations, ?guests);
        info!(
            "{} registrations, {} guests",
            registrations.len(),
            guests.len(),
        );

        policy.set_runners(Runners::new(registrations, guests));
        image_rebuilds.run(&mut policy)?;

        let profile_runner_counts: BTreeMap<_, _> = policy
            .profiles()
            .map(|(key, profile)| (key.clone(), policy.runner_counts(profile)))
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
                excess_healthy,
                wanted,
                image_age,
            },
        ) in profile_runner_counts.iter()
        {
            let profile = policy.profile(key).ok_or_eyre("Failed to get profile")?;
            let image = policy
                .base_image_snapshot(key)
                .map(|snapshot| match profile.image_type {
                    settings::profile::ImageType::Rust => base_image_path(profile, &**snapshot)
                        .as_os_str()
                        .to_str()
                        .expect("Guaranteed by base_image_path()")
                        .to_owned(),
                });
            info!("profile {key}: {healthy}/{target} healthy runners ({idle} idle, {reserved} reserved, {busy} busy, {started_or_crashed} started or crashed, {excess_healthy} excess healthy, {wanted} wanted), image {:?} age {image_age:?}", image);
        }
        for (_id, runner) in policy.runners() {
            runner.log_info();
        }

        policy.update_screenshots();
        policy.update_ipv4_addresses_for_profile_guests();

        if TOML.destroy_all_non_busy_runners() {
            let non_busy_runners = policy
                .runners()
                .filter(|(_id, runner)| runner.status() != Status::Busy);
            let mut threads = vec![];
            for (&id, _runner) in non_busy_runners {
                threads.push(policy.unregister_stop_destroy_runner(id)?);
                registrations_cache.invalidate();
            }
            for thread in threads {
                thread
                    .join()
                    .map_err(|e| eyre!("Thread panicked: {e:?}"))??;
            }
        } else {
            let changes = policy.compute_runner_changes()?;
            if !changes.is_empty() {
                info!(?changes, "Started executing runner changes");
                let mut threads = vec![];
                for runner_id in changes.unregister_and_destroy_runner_ids {
                    threads.push(policy.unregister_stop_destroy_runner(runner_id)?);
                    registrations_cache.invalidate();
                }
                for thread in threads {
                    thread
                        .join()
                        .map_err(|e| eyre!("Thread panicked: {e:?}"))??;
                }
                let mut threads = vec![];
                for (profile_key, count) in changes.create_counts_by_profile_key {
                    let profile = policy
                        .profile(&profile_key)
                        .expect("Guaranteed by compute_runner_changes()");
                    for _ in 0..count {
                        threads.push(policy.register_create_runner(profile, id_gen.next())?);
                        registrations_cache.invalidate();
                    }
                }
                for thread in threads {
                    if let Err(error) = thread
                        .join()
                        .map_err(|e| eyre!("Thread panicked: {e:?}"))
                        .and_then(|inner_result| inner_result)
                        .and_then(|runner_guest_name| start_libvirt_guest(&runner_guest_name))
                    {
                        warn!(?error, "Failed to create runner: {error}");
                    }
                }
                info!("Finished executing runner changes");
            }
        }

        // Update dashboard data, for the API.
        if let Ok(mut dashboard) = DASHBOARD.write() {
            *dashboard = Some(Dashboard::render(&policy, &profile_runner_counts)?);
        }

        // Handle one request from the API.
        if let Ok(request) = REQUEST.receiver.recv_timeout(TOML.monitor_poll_interval()) {
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
                    let matching_runners = policy
                        .runners()
                        .filter(|(_, runner)| {
                            runner.status() == Status::Idle && runner.profile_name() == profile
                        })
                        .take(count)
                        .collect::<Vec<_>>();
                    for (&id, runner) in matching_runners {
                        registrations_cache.invalidate();
                        if policy
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
                Request::GetOverridePolicy { response_tx } => {
                    response_tx
                        .send(policy.get_override().cloned())
                        .expect("Failed to send Response to API thread");
                }
                Request::OverridePolicy {
                    response_tx,
                    profile_override_counts,
                } => {
                    response_tx
                        .send(policy.try_override(profile_override_counts).cloned())
                        .expect("Failed to send Response to API thread");
                }
                Request::CancelOverridePolicy { response_tx } => {
                    response_tx
                        .send(policy.cancel_override())
                        .expect("Failed to send Response to API thread");
                }
                Request::Screenshot {
                    response_tx,
                    runner_id,
                } => {
                    response_tx
                        .send(policy.screenshot_runner(runner_id))
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
                    policy.update_ipv4_addresses_for_runner_guests()?;

                    let result = policy
                        .github_jitconfig(remote_addr)
                        .map(|result| result.map(|ip| ip.to_owned()));
                    if result.as_ref().map_or(false, |result| result.is_some()) {
                        // TODO make this configurable?
                        registrations_cache.invalidate_in(Duration::from_secs(10));
                    }

                    response_tx
                        .send(result)
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
                    policy.update_ipv4_addresses_for_runner_guests()?;
                    policy.update_ipv4_addresses_for_profile_guests();

                    let result = policy
                        .boot_script_for_runner_guest(remote_addr.clone())
                        .transpose()
                        .or_else(|| {
                            policy
                                .boot_script_for_profile_guest(remote_addr)
                                .transpose()
                        })
                        .transpose()
                        .and_then(|result| result.ok_or_eyre("No guest found with IP address"));
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
