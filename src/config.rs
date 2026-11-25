use anyhow::{Context, Result, bail};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub access_key: String,
    pub jwt_secret: String,
    pub listen_addr: String,
    pub http_listen_addr: String,
    /// Directory or `*.log` path for daily action audit files.
    pub log_path: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let access_key = env::var("ACCESS_KEY").context("ACCESS_KEY is required")?;
        let jwt_secret = env::var("JWT_SECRET").context("JWT_SECRET is required")?;
        let listen_addr =
            env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9099".to_string());
        let http_listen_addr =
            env::var("HTTP_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9000".to_string());
        let log_path = env::var("LOG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./logs/actions.log"));

        if access_key.trim().is_empty() {
            bail!("ACCESS_KEY must not be empty");
        }
        if jwt_secret.trim().is_empty() {
            bail!("JWT_SECRET must not be empty");
        }
        if log_path.as_os_str().is_empty() {
            bail!("LOG_PATH must not be empty");
        }

        Ok(Self {
            access_key,
            jwt_secret,
            listen_addr,
            http_listen_addr,
            log_path,
        })
    }
}
