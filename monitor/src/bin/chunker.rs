use std::{collections::BTreeMap, env, sync::Mutex};

use jane_eyre::eyre::{self, eyre};
use rocket::{get, post, response::content::RawText, serde::json::Json};
use tokio::try_join;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use web::rocket_eyre;

static BUILDS: Mutex<BTreeMap<String, Build>> = Mutex::new(BTreeMap::new());

#[derive(Debug)]
struct Build {
    taken_chunks: usize,
    total_chunks: usize,
}
impl Build {
    fn new(total_chunks: usize) -> Self {
        Self {
            taken_chunks: 0,
            total_chunks,
        }
    }
}

#[get("/")]
fn index_route() -> rocket_eyre::Result<RawText<String>> {
    Ok(RawText(format!("{BUILDS:#?}")))
}

#[post("/take?<unique_id>&<total_chunks>")]
fn take_chunk_route(
    unique_id: String,
    total_chunks: usize,
) -> rocket_eyre::Result<Json<Option<usize>>> {
    let mut builds = BUILDS.lock().map_err(|e| eyre!("{e:?}"))?;
    let build = builds.entry(unique_id).or_insert(Build::new(total_chunks));

    if total_chunks != build.total_chunks {
        Err(eyre!(
            "Wrong number of total chunks (expected {}, actual {total_chunks})",
            build.total_chunks
        ))?;
    }

    if build.taken_chunks < total_chunks {
        let response = Some(build.taken_chunks);
        build.taken_chunks += 1;
        Ok(Json(response))
    } else {
        Ok(Json(None))
    }
}

#[rocket::main]
async fn main() -> eyre::Result<()> {
    jane_eyre::install()?;
    if env::var_os("RUST_LOG").is_none() {
        // EnvFilter Builder::with_default_directive doesn’t support multiple directives,
        // so we need to apply defaults ourselves.
        env::set_var("RUST_LOG", "chunker=info,rocket=info");
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
        .mount("/", rocket::routes![index_route, take_chunk_route])
        .launch()
    };

    try_join!(rocket("::1"), rocket("192.168.100.1"))?;

    Ok(())
}
