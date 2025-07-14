use dotenv::dotenv;
use jane_eyre::eyre;
use settings::{IMAGE_DEPS_DIR, LIB_MONITOR_DIR};
use tracing::info;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init() -> eyre::Result<()> {
    jane_eyre::install()?;
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::builder().from_env_lossy())
        .init();

    dotenv()?;
    info!(LIB_MONITOR_DIR = ?*LIB_MONITOR_DIR);
    info!(IMAGE_DEPS_DIR = ?*IMAGE_DEPS_DIR);

    Ok(())
}
