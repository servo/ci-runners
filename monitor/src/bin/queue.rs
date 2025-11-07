use std::{
    collections::BTreeMap,
    fmt::Write as _,
    process::exit,
    sync::{LazyLock, RwLock},
    thread,
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use jane_eyre::eyre::{self, bail, OptionExt};
use reqwest::Client;
use rocket::{
    get, post,
    response::content::{RawHtml, RawText},
    serde::json::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use settings::TOML;
use tokio::task::JoinSet;
use tracing::{error, info};
use web::{auth::ApiKeyGuard, rocket_eyre};

static DASHBOARD: RwLock<Option<String>> = RwLock::new(None);

#[derive(Debug)]
enum Request {
    Enqueue {
        response_tx: Sender<eyre::Result<QueueEntry>>,
        entry: QueueEntry,
    },
}

struct Channel<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}
static REQUEST: LazyLock<Channel<Request>> = LazyLock::new(|| {
    let (sender, receiver) = crossbeam_channel::bounded(0);
    Channel { sender, receiver }
});

#[get("/")]
async fn index_route() -> rocket_eyre::Result<RawHtml<&'static str>> {
    Ok(RawHtml(include_str!("queue/index.html")))
}

#[get("/dashboard.txt")]
async fn dashboard_text_route() -> rocket_eyre::Result<RawText<String>> {
    Ok(RawText(
        DASHBOARD
            .read()
            .expect("Poisoned")
            .clone()
            .unwrap_or_default(),
    ))
}

#[post("/profile/<profile_key>/enqueue?<unique_id>&<qualified_repo>&<run_id>")]
async fn profile_enqueue_route(
    unique_id: String,
    qualified_repo: String,
    run_id: String,
    profile_key: String,
    _auth: ApiKeyGuard<'_>,
) -> rocket_eyre::Result<Json<QueueEntry>> {
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::Enqueue {
            response_tx,
            entry: QueueEntry {
                unique_id,
                qualified_repo,
                run_id,
                profile_key,
            },
        },
        TOML.monitor_thread_send_timeout(),
    )?;
    let result = response_rx.recv_timeout(TOML.monitor_thread_recv_timeout())??;

    Ok(Json(result))
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    cli::init()?;

    tokio::task::spawn(async move {
        let thread = thread::spawn(queue_thread);
        loop {
            if thread.is_finished() {
                match thread.join() {
                    Ok(Ok(())) => {
                        info!("Queue thread exited");
                        exit(0);
                    }
                    Ok(Err(report)) => error!(?report, "Queue thread error"),
                    Err(panic) => error!(?panic, "Queue thread panic"),
                };
                exit(1);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    let rocket = |listen_addr: &str| {
        rocket::custom(
            rocket::Config::figment()
                .merge(("port", 8002))
                .merge(("address", listen_addr)),
        )
        .mount(
            "/",
            rocket::routes![index_route, dashboard_text_route, profile_enqueue_route],
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct QueueEntry {
    unique_id: String,
    qualified_repo: String,
    run_id: String,
    profile_key: String,
}

#[tokio::main]
async fn queue_thread() -> eyre::Result<()> {
    let config = TOML
        .queue
        .as_ref()
        .ok_or_eyre("monitor.toml has no [queue]!")?;
    let client = Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()?;
    let mut queue_entries: Vec<QueueEntry> = vec![];
    let mut monitor_responses: BTreeMap<String, MonitorResponse> = BTreeMap::default();

    loop {
        let mut queue_text = String::default();
        for entry in queue_entries.iter() {
            writeln!(&mut queue_text, "{entry:?}")?;
        }

        info!("Querying servers for updates");

        let mut set = JoinSet::new();
        for server_url in config.servers.iter() {
            let client = client.clone();
            set.spawn(async {
                (
                    server_url.clone(),
                    get_monitor_dashboard_for_server(client, server_url).await,
                )
            });
        }

        for (server, result) in set.join_all().await {
            match result {
                Ok(response) => {
                    info!(?server, ?response);
                    monitor_responses.insert(server, response);
                }
                Err(error) => {
                    error!(?error);
                }
            }
        }

        let mut servers_text = String::default();
        for (server, response) in monitor_responses.iter() {
            writeln!(&mut servers_text, "- {server}")?;
            for (profile_key, runner_counts) in response.profile_runner_counts.iter() {
                writeln!(&mut servers_text, "    - {profile_key}")?;
                writeln!(
                    &mut servers_text,
                    "      {} idle, {} healthy, {} target",
                    runner_counts.idle, runner_counts.healthy, runner_counts.target
                )?;
            }
        }

        let mut new_dashboard = String::default();
        writeln!(&mut new_dashboard, ">>> queue\n{queue_text}")?;
        writeln!(&mut new_dashboard, ">>> servers\n{servers_text}")?;
        *DASHBOARD.write().expect("Poisoned") = Some(new_dashboard);

        // Handle one request from the API.
        if let Ok(request) = REQUEST.receiver.recv_timeout(TOML.monitor_poll_interval()) {
            match request {
                Request::Enqueue { response_tx, entry } => {
                    response_tx
                        .send(try_enqueue(&mut queue_entries, entry))
                        .expect("Failed to send Response to API thread");
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct MonitorResponse {
    profile_runner_counts: BTreeMap<String, ProfileRunnerCounts>,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}
#[derive(Debug, Deserialize, Serialize)]
struct ProfileRunnerCounts {
    idle: usize,
    healthy: usize,
    target: usize,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}
async fn get_monitor_dashboard_for_server(
    client: Client,
    server: &str,
) -> eyre::Result<MonitorResponse> {
    let response = client
        .get(format!("{server}/dashboard.json"))
        .send()
        .await?;
    Ok(response.json::<MonitorResponse>().await?)
}

fn try_enqueue(queue_entries: &mut Vec<QueueEntry>, entry: QueueEntry) -> eyre::Result<QueueEntry> {
    if queue_entries
        .iter()
        .find(|e| e.unique_id == entry.unique_id)
        .is_some()
    {
        bail!("Already in queue: {:?}", entry.unique_id);
    }
    queue_entries.push(entry.clone());
    Ok(entry)
}
