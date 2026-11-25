mod connection;
mod state;

use crate::config::Config;
use anyhow::{Context, Result};
use state::SharedState;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tracing::info;

pub async fn run(config: Config) -> Result<()> {
    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .with_context(|| format!("bind {}", config.listen_addr))?;
    info!(addr = %config.listen_addr, "listening");

    let state = SharedState::new();
    let next_id = AtomicU64::new(1);

    loop {
        let (stream, peer) = listener.accept().await.context("accept")?;
        let conn_id = next_id.fetch_add(1, Ordering::Relaxed);
        let state = state.clone();
        let config = config.clone();
        info!(conn_id, %peer, "accepted");
        tokio::spawn(async move {
            if let Err(err) = connection::handle_connection(stream, conn_id, state, config).await {
                tracing::debug!(conn_id, error = %err, "handler finished with error");
            }
        });
    }
}
