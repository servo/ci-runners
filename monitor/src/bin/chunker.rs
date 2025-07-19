use std::{
    collections::BTreeMap,
    env,
    fmt::Debug,
    sync::Mutex,
    time::{Duration, Instant},
};

use jane_eyre::eyre::{self, eyre};
use rand::distr::{Alphanumeric, SampleString};
use rocket::{
    form::{FromFormField, ValueField},
    get, post,
    response::content::RawText,
    serde::json::Json,
};
use serde::Deserialize;
use tokio::try_join;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use web::rocket_eyre;

static BUILDS: Mutex<BTreeMap<String, Build>> = Mutex::new(BTreeMap::new());

#[derive(Debug)]
struct Build {
    unique_id: String,
    qualified_repo: String,
    run_id: String,
    runners: BTreeMap<usize, Runner>,
    total_chunks: usize,
    taken_chunks: usize,
    started_at: Instant,
    secret_token: SecretToken,
}
impl Build {
    fn new(
        unique_id: String,
        qualified_repo: String,
        run_id: String,
        runners: BTreeMap<usize, RunnerEnvironment>,
        total_chunks: usize,
    ) -> Self {
        Self {
            unique_id,
            qualified_repo,
            run_id,
            runners: runners
                .into_iter()
                .map(|(runner, environment)| (runner, Runner::new(environment)))
                .collect(),
            total_chunks,
            taken_chunks: 0,
            started_at: Instant::now(),
            secret_token: SecretToken::default(),
        }
    }
}

#[derive(Debug)]
struct Runner {
    environment: RunnerEnvironment,
    taken_chunks: usize,
    started_at: Vec<Duration>,
}
impl Runner {
    fn new(environment: RunnerEnvironment) -> Self {
        Self {
            environment,
            taken_chunks: 0,
            started_at: vec![],
        }
    }
}

#[derive(Debug, Deserialize, PartialEq)]
enum RunnerEnvironment {
    GithubHosted,
    SelfHosted,
}
impl<'v> FromFormField<'v> for RunnerEnvironment {
    fn from_value(field: ValueField<'v>) -> rocket::form::Result<'v, Self> {
        match field.value {
            "github-hosted" => Ok(Self::GithubHosted),
            "self-hosted" => Ok(Self::SelfHosted),
            _ => Err(rocket::form::Error::validation(
                "Bad runner environment value",
            ))?,
        }
    }
}

struct SecretToken(String);
impl Default for SecretToken {
    fn default() -> Self {
        Self(Alphanumeric.sample_string(&mut rand::rng(), 32))
    }
}
impl Debug for SecretToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SecretToken").finish()
    }
}

#[get("/")]
fn index_route() -> rocket_eyre::Result<RawText<String>> {
    Ok(RawText(format!("{BUILDS:#?}")))
}

/// POST `/start?qualified_repo=<user>/<repo>` => `<secret_token>`
#[post("/start?<unique_id>&<qualified_repo>&<run_id>&<runners>&<total_chunks>")]
fn start_route(
    unique_id: String,
    qualified_repo: String,
    run_id: String,
    runners: BTreeMap<usize, RunnerEnvironment>,
    total_chunks: usize,
) -> rocket_eyre::Result<RawText<String>> {
    let mut builds = BUILDS.lock().map_err(|e| eyre!("{e:?}"))?;
    if builds.contains_key(&unique_id) {
        Err(eyre!("Build already started with unique id: {unique_id}"))?;
    }
    let build = builds
        .entry(unique_id.clone())
        .and_modify(|_| unreachable!())
        .or_insert(Build::new(
            unique_id,
            qualified_repo,
            run_id,
            runners,
            total_chunks,
        ));

    Ok(RawText(build.secret_token.0.clone()))
}

#[post("/take?<unique_id>&<secret_token>&<runner>")]
fn take_chunk_route(
    unique_id: String,
    secret_token: String,
    runner: usize,
) -> rocket_eyre::Result<Json<Option<usize>>> {
    let mut builds = BUILDS.lock().map_err(|e| eyre!("{e:?}"))?;
    let Some(build) = builds.get_mut(&unique_id) else {
        return Err(eyre!("Unknown build with unique id: {unique_id}"))?;
    };
    if secret_token != build.secret_token.0 {
        return Err(eyre!("Incorrect secret token"))?;
    }
    let Some(runner) = build.runners.get_mut(&runner) else {
        return Err(eyre!("Unknown runner {runner} ({unique_id})"))?;
    };

    if build.taken_chunks >= build.total_chunks * 2 / 3
        && runner.environment == RunnerEnvironment::GithubHosted
    {
        // Forbid slow GitHub-hosted runners from taking the last 1/3 of chunks.
        let response = None;
        info!(?unique_id, ?response);
        Ok(Json(response))
    } else if build.taken_chunks < build.total_chunks {
        let response = Some(build.taken_chunks);
        build.taken_chunks += 1;
        runner.taken_chunks += 1;
        runner
            .started_at
            .push(Instant::now().duration_since(build.started_at));
        info!(?unique_id, ?response);
        Ok(Json(response))
    } else {
        let response = None;
        info!(?unique_id, ?response);
        Ok(Json(response))
    }
}

#[rocket::main]
async fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    if env::var_os("RUST_LOG").is_none() {
        // EnvFilter Builder::with_default_directive doesn’t support multiple directives,
        // so we need to apply defaults ourselves.
        env::set_var(
            "RUST_LOG",
            "chunker=info,rocket=info,rocket::server=info,rocket::server::_=off",
        );
    }
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::builder().from_env_lossy())
        .init();

    let rocket = |listen_addr: &str| {
        rocket::custom(
            rocket::Config::figment()
                .merge(("port", 8001))
                .merge(("address", listen_addr)),
        )
        .mount(
            "/",
            rocket::routes![index_route, start_route, take_chunk_route],
        )
        .launch()
    };

    try_join!(rocket("::1"), rocket("192.168.100.1"))?;

    Ok(())
}
