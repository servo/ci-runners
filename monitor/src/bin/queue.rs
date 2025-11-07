use std::{
    collections::BTreeMap,
    fmt::{Display, Write as _},
    process::exit,
    sync::{LazyLock, RwLock},
    thread,
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};
use jane_eyre::eyre::{self, bail, eyre, OptionExt};
use rand::seq::SliceRandom;
use reqwest::Client;
use rocket::{
    get, post,
    response::content::{RawHtml, RawJson, RawText},
    serde::json::Json,
    FromForm,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use settings::{DOTENV, TOML};
use tokio::task::JoinSet;
use tracing::{debug, error, info};
use web::{
    auth::ApiKeyGuard,
    rocket_eyre::{self, EyreReport},
};

static QUICK_LOOKUP: RwLock<BTreeMap<UniqueId, QuickLookupStatus>> = RwLock::new(BTreeMap::new());
static DASHBOARD: RwLock<Option<String>> = RwLock::new(None);

#[derive(Clone, Debug, Deserialize, Eq, FromForm, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
struct UniqueId(String);
impl Display for UniqueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum QuickLookupStatus {
    ReadyToTake,
    NotReadyYet,
}

#[derive(Debug)]
enum Request {
    Enqueue {
        response_tx: Sender<eyre::Result<QueueEntry>>,
        entry: QueueEntry,
    },
    Take {
        response_tx: Sender<eyre::Result<TakeResult>>,
        unique_id: UniqueId,
    },
}
#[derive(Debug)]
enum TakeResult {
    Success(eyre::Result<String>),
    TryAgain(Duration),
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
    unique_id: UniqueId,
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

#[post("/take?<unique_id>")]
async fn take_route(
    unique_id: UniqueId,
    _auth: ApiKeyGuard<'_>,
) -> rocket_eyre::Result<RawJson<String>> {
    match QUICK_LOOKUP.read().expect("Poisoned").get(&unique_id) {
        Some(QuickLookupStatus::ReadyToTake) => {}
        Some(QuickLookupStatus::NotReadyYet) => {
            return Err(EyreReport::TryAgain(Duration::from_secs(5)));
        }
        None => {
            return Err(eyre!("Not found: {unique_id:?}").into());
        }
    };
    let (response_tx, response_rx) = crossbeam_channel::bounded(0);
    REQUEST.sender.send_timeout(
        Request::Take {
            response_tx,
            unique_id,
        },
        TOML.monitor_thread_send_timeout(),
    )?;
    let result = match response_rx.recv_timeout(TOML.monitor_thread_recv_timeout())?? {
        TakeResult::Success(result) => result?,
        TakeResult::TryAgain(duration) => Err(EyreReport::TryAgain(duration))?,
    };
    Ok(RawJson(result))
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
            rocket::routes![
                index_route,
                dashboard_text_route,
                profile_enqueue_route,
                take_route,
            ],
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

#[derive(Debug, Default)]
struct Queue {
    queue: Vec<QueueEntry>,
    servers: BTreeMap<Server, ServerStatus>,
}

#[derive(Clone, Debug)]
struct ServerStatus {
    last_monitor_response: MonitorResponse,
    stale: bool,
}

impl ServerStatus {
    fn fresh_only(&self) -> Option<&MonitorResponse> {
        (!self.stale).then_some(&self.last_monitor_response)
    }
    fn fresh_or_stale(&self) -> &MonitorResponse {
        &self.last_monitor_response
    }
}

impl Queue {
    fn try_enqueue(&mut self, entry: QueueEntry) -> eyre::Result<QueueEntry> {
        if self.queue.iter().find(|e| e.matches(&entry)).is_some() {
            bail!("Already in queue: {:?}", entry.unique_id);
        }
        self.queue.push(entry.clone());
        Ok(entry)
    }

    async fn try_take(&mut self, unique_id: &UniqueId) -> eyre::Result<TakeResult> {
        if let Some(entry) = self.get_entry(unique_id) {
            if let Some(server) = self.pick_server(&entry) {
                self.remove_entry(unique_id);
                let QueueEntry {
                    unique_id,
                    qualified_repo,
                    run_id,
                    profile_key,
                } = entry;
                Ok(TakeResult::Success(
                    (async || {
                        Ok(client(Duration::from_millis(3000))?
                            .post(format!("{server}/profile/{profile_key}/take"))
                            .query(&[
                                ("unique_id", unique_id.to_string()),
                                ("qualified_repo", qualified_repo),
                                ("run_id", run_id),
                            ])
                            .bearer_auth(&*DOTENV.monitor_api_token_raw_value)
                            .send()
                            .await?
                            .text()
                            .await?)
                    })()
                    .await,
                ))
            } else {
                Ok(TakeResult::TryAgain(Duration::from_secs(1)))
            }
        } else {
            bail!("Not found: {unique_id:?}");
        }
    }

    fn get_entry(&self, unique_id: &UniqueId) -> Option<QueueEntry> {
        self.queue.iter().find(|e| e.matches_id(unique_id)).cloned()
    }

    fn remove_entry(&mut self, unique_id: &UniqueId) {
        self.queue.retain(|e| !e.matches_id(unique_id));
    }

    fn quick_lookup_status(&self, entry: &QueueEntry) -> QuickLookupStatus {
        match self.pick_server(entry) {
            Some(_) => QuickLookupStatus::ReadyToTake,
            None => QuickLookupStatus::NotReadyYet,
        }
    }

    fn pick_server(&self, entry: &QueueEntry) -> Option<Server> {
        let mut servers = self.servers.clone().into_iter().collect::<Vec<_>>();
        let mut rng = rand::rng();
        servers.shuffle(&mut rng);
        for (server, status) in servers {
            if let Some(response) = status.fresh_only() {
                if let Some(runner_counts) = response.profile_runner_counts.get(&entry.profile_key)
                {
                    if runner_counts.idle >= 1 {
                        return Some(server);
                    }
                }
            }
        }
        None
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Server(String);
impl Display for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct QueueEntry {
    unique_id: UniqueId,
    qualified_repo: String,
    run_id: String,
    profile_key: String,
}

impl QueueEntry {
    fn matches(&self, other: &Self) -> bool {
        self.matches_id(&other.unique_id)
    }
    fn matches_id(&self, unique_id: &UniqueId) -> bool {
        self.unique_id == *unique_id
    }
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
    let mut queue = Queue::default();

    loop {
        info!("Querying servers for updates");

        for status in queue.servers.values_mut() {
            status.stale = true;
        }

        let mut set = JoinSet::new();
        for server_url in config.servers.iter() {
            let client = client.clone();
            set.spawn(async {
                (
                    Server(server_url.clone()),
                    get_monitor_dashboard_for_server(client, server_url).await,
                )
            });
        }

        for (server, result) in set.join_all().await {
            match result {
                Ok(response) => {
                    debug!(?server, ?response);
                    queue.servers.insert(
                        server,
                        ServerStatus {
                            last_monitor_response: response,
                            stale: false,
                        },
                    );
                }
                Err(error) => {
                    error!(?error);
                }
            }
        }

        let mut queue_text = String::default();
        for entry in queue.queue.iter() {
            writeln!(&mut queue_text, "{entry:?}")?;
        }
        *QUICK_LOOKUP.write().expect("Poisoned") = queue
            .queue
            .iter()
            .map(|entry| (entry.unique_id.clone(), queue.quick_lookup_status(entry)))
            .collect();

        let mut servers_text = String::default();
        for (server, status) in queue.servers.iter() {
            write!(&mut servers_text, "- {server}")?;
            if status.stale {
                writeln!(&mut servers_text, " (stale!)")?;
            } else {
                writeln!(&mut servers_text, "")?;
            }
            for (profile_key, runner_counts) in status.fresh_or_stale().profile_runner_counts.iter()
            {
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
                        .send(queue.try_enqueue(entry))
                        .expect("Failed to send Response to API thread");
                }
                Request::Take {
                    response_tx,
                    unique_id,
                } => {
                    response_tx
                        .send(queue.try_take(&unique_id).await)
                        .expect("Failed to send Response to API thread");
                }
            }
        }
    }
}

fn client(timeout: Duration) -> eyre::Result<Client> {
    Ok(Client::builder().timeout(timeout).build()?)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MonitorResponse {
    profile_runner_counts: BTreeMap<String, ProfileRunnerCounts>,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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
