use std::{
    collections::BTreeMap, fmt::Write as _, process::exit, sync::RwLock, thread, time::Duration,
};

use jane_eyre::eyre::{self, OptionExt};
use reqwest::Client;
use rocket::{get, response::content::RawText};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use settings::TOML;
use tokio::{task::JoinSet, time::sleep};
use tracing::{error, info};
use web::rocket_eyre;

static DASHBOARD: RwLock<Option<String>> = RwLock::new(None);

#[get("/")]
async fn index_route() -> rocket_eyre::Result<RawText<String>> {
    Ok(RawText(
        DASHBOARD
            .read()
            .expect("Poisoned")
            .clone()
            .unwrap_or_default(),
    ))
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
        .mount("/", rocket::routes![index_route,])
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

#[tokio::main]
async fn queue_thread() -> eyre::Result<()> {
    let config = TOML
        .queue
        .as_ref()
        .ok_or_eyre("monitor.toml has no [queue]!")?;
    let client = Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()?;
    let mut monitor_responses: BTreeMap<String, MonitorResponse> = BTreeMap::default();

    loop {
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
        writeln!(&mut new_dashboard, ">>> servers\n{servers_text}")?;
        *DASHBOARD.write().expect("Poisoned") = Some(new_dashboard);

        sleep(Duration::from_secs(1)).await;
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
