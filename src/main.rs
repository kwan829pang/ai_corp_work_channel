mod action_log;
mod auth;
mod client;
mod config;
mod mention;
mod protocol;
mod server;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "agent_channel", about = "TCP NDJSON message channel")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run the socket server
    Serve,
    /// Connect as a CLI chat client
    Client(client::ClientArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Serve => {
            let config = config::Config::from_env()?;
            server::run(config).await
        }
        Commands::Client(args) => client::run(args).await,
    }
}
