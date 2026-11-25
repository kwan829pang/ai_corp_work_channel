mod connection;
mod http;
mod publish;
mod state;

use crate::action_log::ActionLogger;
use crate::config::Config;
use anyhow::{Context, Result};
use state::SharedState;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tracing::info;

pub async fn run(config: Config) -> Result<()> {
    let action_log = ActionLogger::open(&config.log_path)
        .with_context(|| format!("open action log at {}", config.log_path.display()))?;
    info!(path = %config.log_path.display(), "action log ready");

    let state = SharedState::new();
    let http_config = config.clone();
    let http_state = state.clone();
    let http_log = action_log.clone();

    tokio::spawn(async move {
        if let Err(err) = http::run(http_config, http_state, http_log).await {
            tracing::error!(error = %err, "http server stopped");
        }
    });

    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .with_context(|| format!("bind {}", config.listen_addr))?;
    info!(addr = %config.listen_addr, "tcp listening");

    let next_id = AtomicU64::new(1);

    loop {
        let (stream, peer) = listener.accept().await.context("accept")?;
        let conn_id = next_id.fetch_add(1, Ordering::Relaxed);
        let state = state.clone();
        let config = config.clone();
        let action_log = action_log.clone();
        info!(conn_id, %peer, "accepted");
        tokio::spawn(async move {
            if let Err(err) =
                connection::handle_connection(stream, conn_id, state, config, action_log).await
            {
                tracing::debug!(conn_id, error = %err, "handler finished with error");
            }
        });
    }
}
