use std::env;

use dotenv::dotenv;
use jane_eyre::eyre;
use settings::{IMAGE_DEPS_DIR, LIB_MONITOR_DIR};
use tracing::info;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init() -> eyre::Result<()> {
    init_logging_only()?;
    dotenv()?;
    info!(LIB_MONITOR_DIR = ?*LIB_MONITOR_DIR);
    info!(IMAGE_DEPS_DIR = ?*IMAGE_DEPS_DIR);

    Ok(())
}

pub fn init_logging_only() -> Result<(), eyre::Error> {
    jane_eyre::install()?;
    if env::var_os("RUST_LOG").is_none() {
        // EnvFilter Builder::with_default_directive doesn’t support multiple directives,
        // so we need to apply defaults ourselves.
        // FIXME: this is unsound, unless called before the process ever becomes multi-threaded!
        unsafe {
            env::set_var(
                "RUST_LOG",
                "monitor=info,chunker=info,queue=info,cli=info,data=info,settings=info,rocket=info,cmd_lib::child=info",
            );
        }
    }
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::builder().from_env_lossy())
        .init();

    Ok(())
}
