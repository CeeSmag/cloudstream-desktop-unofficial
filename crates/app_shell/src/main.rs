//! Wiring only: builds implementations and starts the app.
#![forbid(unsafe_code)]

use tracing_subscriber::EnvFilter;

fn initialize_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn main() -> anyhow::Result<()> {
    initialize_logging();
    tracing::info!("cloudstream-rs starting");
    Ok(())
}
