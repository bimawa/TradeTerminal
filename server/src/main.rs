mod auth;
mod binance;
mod bybit;
mod client_handler;
mod config;
mod db;
mod exchange;
mod server;
pub mod tls;

use anyhow::Result;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::from_env()?;
    let database = Arc::new(db::init_database()?);
    tracing::info!(
        "Starting trade server on {} with exchange: {:?}",
        config.listen_addr,
        config.exchange
    );

    server::run(config, database).await
}
