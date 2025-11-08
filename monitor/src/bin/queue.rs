use std::{
    collections::BTreeMap,
    fmt::{Display, Write as _},
    process::exit,
    sync::{LazyLock, RwLock},
    thread,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use jane_eyre::eyre::{self, bail, eyre, OptionExt};
use monitor::validate_tokenless_select;
use rand::{
    distr::{Alphanumeric, SampleString},
    rng,
    seq::SliceRandom,
};
use reqwest::Client;
use rocket::{
    get, post,
    response::content::{RawHtml, RawJson, RawText},
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

/// How long to wait for the completion of downstream dashboard requests to the monitor API.
const DASHBOARD_UPDATE_REQUEST_TIMEOUT: Duration = Duration::from_millis(1500);

/// How long to wait for the completion of downstream take requests to the monitor API.
const DOWNSTREAM_TAKE_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// How long to tell the client to wait before retrying a `/take` request (via ‘Retry-After’).
const TAKE_RESPONSE_RETRY_AFTER: Duration = Duration::from_secs(5);

/// Queued jobs are considered abandoned if the last related request was this long ago.
const QUEUED_JOB_EXPIRY_AGE: Duration = Duration::from_secs(30);

/// How long to pause after the queue thread exits, cleanly or otherwise.
const PAUSE_AFTER_QUEUE_THREAD_EXIT: Duration = Duration::from_secs(1);

/// Requests for the queue thread (see [`Request`] for more details).
static REQUEST: LazyLock<Channel<Request>> = LazyLock::new(|| {
    let (sender, receiver) = crossbeam_channel::bounded(0);
    Channel { sender, receiver }
});

/// Cached dashboard contents, to help service requests without cranking the queue thread.
static DASHBOARD: RwLock<Option<String>> = RwLock::new(None);

/// Cached data about each queued job, to help service requests without cranking the queue thread.
static QUEUE_CACHE: RwLock<BTreeMap<UniqueId, CachedEntry>> = RwLock::new(BTreeMap::new());

/// Last time each queued job was mentioned in a request, to help clean up abandoned entries.
static ACCESS_TIMES: RwLock<BTreeMap<UniqueId, Instant>> = RwLock::new(BTreeMap::new());

/// Newtype for the unique id of a queued job, which should be a UUIDv4.
#[derive(Clone, Debug, Deserialize, Eq, FromForm, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
struct UniqueId(String);
impl Display for UniqueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Newtype for the base url of one of our underlying servers.
#[repr(transparent)]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Server(String);
impl Display for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CachedEntry {
    /// Random value generated in `/enqueue` and checked in `/take`, to ensure that the client
    /// sending a `/take` is the same as the client that sent the `/enqueue`.
    token: String,
    ready: ReadyToTake,
}

/// Whether a `/take` request should actually be forwarded to the queue thread.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ReadyToTake {
    /// There may be enough capacity to take the runner, but it may fail under contention.
    Maybe,
    /// There is definitely not enough capacity to take the runner yet.
    No,
}

struct Channel<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}

/// Request for the queue thread.
///
/// These requests are assumed to be authorised, and each request that gets forwarded to the queue
/// thread requires refreshing the status of our resources, so they are relatively expensive.
#[derive(Debug)]
enum Request {
    Enqueue {
        response_tx: Sender<eyre::Result<String>>,
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

/// Enqueue a job, tokenful version.
///
/// Returns a random token that the client needs to send in its `/take` requests.
///
/// There are currently no validation checks, but you need to send the monitor API token as
/// `Authorization: Bearer <token>`.
#[post("/profile/<profile_key>/enqueue?<unique_id>&<qualified_repo>&<run_id>")]
async fn profile_enqueue_route(
    unique_id: UniqueId,
    qualified_repo: String,
    run_id: String,
    profile_key: String,
    _auth: ApiKeyGuard<'_>,
) -> rocket_eyre::Result<RawText<String>> {
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

    Ok(RawText(result))
}

/// Enqueue a job, tokenless version.
///
/// Returns a random token that the client needs to send in its `/take` requests.
///
/// There is no `profile_key`, because that needs to be set in a GitHub Actions artifact, which
/// also serves as proof that an authorised job actually requested it. Since this endpoint is not
/// protected by the monitor API token, there are several validation checks.
#[post("/enqueue?<unique_id>&<qualified_repo>&<run_id>")]
async fn enqueue_route(
    unique_id: UniqueId,
    qualified_repo: String,
    run_id: String,
) -> rocket_eyre::Result<RawText<String>> {
    let profile_key = validate_tokenless_select(&unique_id.to_string(), &qualified_repo, &run_id)?;
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

    Ok(RawText(result))
}

/// Take a runner from one of the underlying servers.
///
/// `<token>` must be the same as in the response to POST `/enqueue`.
///
/// Returns the same response as POST `/profile/<profile_key>/take` does in the monitor API,
/// because this endpoint just forwards the request to a server with available capacity.
#[post("/take/<unique_id>?<token>")]
async fn take_route(unique_id: String, token: String) -> rocket_eyre::Result<RawJson<String>> {
    let unique_id = UniqueId(unique_id);
    let Some(quick_lookup) = QUEUE_CACHE
        .read()
        .expect("Poisoned")
        .get(&unique_id)
        .cloned()
    else {
        return Err(EyreReport::NotFound(eyre!("Not found: {unique_id:?}")));
    };
    ACCESS_TIMES
        .write()
        .expect("Poisoned")
        .insert(unique_id.clone(), Instant::now());
    if token != quick_lookup.token {
        return Err(EyreReport::Forbidden(eyre!("Bad token: {unique_id:?}")));
    }
    if quick_lookup.ready == ReadyToTake::No {
        return Err(EyreReport::TryAgain(TAKE_RESPONSE_RETRY_AFTER));
    }
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
            tokio::time::sleep(PAUSE_AFTER_QUEUE_THREAD_EXIT).await;
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
                enqueue_route,
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

// ----- Queue thread (backend) -----

#[tokio::main]
async fn queue_thread() -> eyre::Result<()> {
    let config = TOML
        .queue
        .as_ref()
        .ok_or_eyre("monitor.toml has no [queue]!")?;
    let mut queue = Queue::default();

    loop {
        info!("Querying servers for updates");
        queue.start_update();

        let mut set = JoinSet::new();
        for server_url in config.servers.iter() {
            set.spawn(async {
                (
                    Server(server_url.clone()),
                    get_monitor_dashboard_for_server(server_url).await,
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
                            fresh: true,
                        },
                    );
                }
                Err(error) => {
                    error!(?error);
                }
            }
        }

        let mut queue_text = String::default();
        for (unique_id, entry) in queue.iter() {
            let access_times = ACCESS_TIMES.read().expect("Poisoned");
            let access_time = access_times.get(unique_id).expect("Guaranteed by Queue");
            writeln!(
                &mut queue_text,
                "- {unique_id} (last request {:?} ago)",
                access_time.elapsed()
            )?;
            writeln!(&mut queue_text, "  {entry:?}")?;
        }
        *QUEUE_CACHE.write().expect("Poisoned") = queue
            .iter()
            .flat_map(|(unique_id, entry)| {
                queue
                    .quick_lookup_info(entry)
                    .map(|info| (unique_id.clone(), info))
            })
            .collect();

        let mut servers_text = String::default();
        for (server, status) in queue.servers.iter() {
            write!(&mut servers_text, "- {server}")?;
            if status.fresh {
                writeln!(&mut servers_text, "")?;
            } else {
                writeln!(&mut servers_text, " (stale!)")?;
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

#[derive(Debug, Default)]
struct Queue {
    order: Vec<UniqueId>,
    entries: BTreeMap<UniqueId, QueueEntry>,
    tokens: BTreeMap<UniqueId, String>,
    servers: BTreeMap<Server, ServerStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct QueueEntry {
    unique_id: UniqueId,
    qualified_repo: String,
    run_id: String,
    profile_key: String,
}

impl QueueEntry {
    fn matches_id(&self, unique_id: &UniqueId) -> bool {
        self.unique_id == *unique_id
    }
}

impl Queue {
    fn try_enqueue(&mut self, entry: QueueEntry) -> eyre::Result<String> {
        if self.order.iter().find(|id| entry.matches_id(id)).is_some() {
            bail!("Already in queue: {:?}", entry.unique_id);
        }
        let unique_id = entry.unique_id.clone();
        self.order.push(unique_id.clone());
        self.entries.insert(unique_id.clone(), entry);
        let token = self
            .tokens
            .entry(unique_id.clone())
            .or_insert(Alphanumeric.sample_string(&mut rng(), 32));
        QUEUE_CACHE.write().expect("Poisoned").insert(
            unique_id.clone(),
            CachedEntry {
                token: token.clone(),
                ready: ReadyToTake::No,
            },
        );
        ACCESS_TIMES
            .write()
            .expect("Poisoned")
            .insert(unique_id.clone(), Instant::now());
        Ok(token.clone())
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
                        Ok(client(DOWNSTREAM_TAKE_REQUEST_TIMEOUT)?
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
                Ok(TakeResult::TryAgain(TAKE_RESPONSE_RETRY_AFTER))
            }
        } else {
            bail!("Not found: {unique_id:?}");
        }
    }

    fn start_update(&mut self) {
        let mut access_times = ACCESS_TIMES.write().expect("Poisoned");
        for (unique_id, access_time) in access_times.clone() {
            if access_time.elapsed() > QUEUED_JOB_EXPIRY_AGE {
                self.remove_entry(&unique_id);
                access_times.remove(&unique_id);
            }
        }
        for status in self.servers.values_mut() {
            status.fresh = false;
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&UniqueId, &QueueEntry)> {
        self.order
            .iter()
            .flat_map(|id| self.entries.get(id).map(|entry| (id, entry)))
    }

    fn get_entry(&self, unique_id: &UniqueId) -> Option<QueueEntry> {
        self.entries.get(unique_id).cloned()
    }

    fn remove_entry(&mut self, unique_id: &UniqueId) {
        self.order.retain(|id| id != unique_id);
        self.entries.remove(unique_id);
        self.tokens.remove(unique_id);
    }

    fn quick_lookup_info(&self, entry: &QueueEntry) -> Option<CachedEntry> {
        let token = self.tokens.get(&entry.unique_id)?.clone();
        let status = match self.pick_server(entry) {
            Some(_) => ReadyToTake::Maybe,
            None => ReadyToTake::No,
        };
        Some(CachedEntry {
            token,
            ready: status,
        })
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

#[derive(Clone, Debug)]
struct ServerStatus {
    last_monitor_response: MonitorResponse,
    /// Whether this data was successfully updated in this iteration of the event loop.
    ///
    /// Only fresh data can be used to forward `/take` requests to a server.
    fresh: bool,
}

impl ServerStatus {
    fn fresh_only(&self) -> Option<&MonitorResponse> {
        self.fresh.then_some(&self.last_monitor_response)
    }
    fn fresh_or_stale(&self) -> &MonitorResponse {
        &self.last_monitor_response
    }
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

async fn get_monitor_dashboard_for_server(server: &str) -> eyre::Result<MonitorResponse> {
    let response = client(DASHBOARD_UPDATE_REQUEST_TIMEOUT)?
        .get(format!("{server}/dashboard.json"))
        .send()
        .await?;
    Ok(response.json::<MonitorResponse>().await?)
}

fn client(timeout: Duration) -> eyre::Result<Client> {
    Ok(Client::builder().timeout(timeout).build()?)
}
