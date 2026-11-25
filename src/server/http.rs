use crate::action_log::ActionLogger;
use crate::auth::{self, AuthError};
use crate::config::Config;
use crate::server::publish;
use crate::server::state::SharedState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::info;

#[derive(Clone)]
struct AppState {
    shared: SharedState,
    config: Config,
    action_log: ActionLogger,
}

#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    pub access_key: String,
    pub nickname: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub nickname: String,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub channel: String,
    pub content: String,
    /// Display name on the channel `message` frame. Defaults to JWT `sub`.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MentionsAck {
    pub delivered: Vec<String>,
    pub offline: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub ok: bool,
    pub channel: String,
    pub name: String,
    pub content: String,
    pub submit_at: i64,
    pub recipients: usize,
    pub mentions: MentionsAck,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

enum ApiError {
    Unauthorized,
    InvalidAccessKey,
    InvalidToken(String),
    BadRequest(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing or invalid authorization".to_string(),
            ),
            Self::InvalidAccessKey => (
                StatusCode::UNAUTHORIZED,
                "invalid_access_key",
                "invalid access key".to_string(),
            ),
            Self::InvalidToken(message) => (StatusCode::UNAUTHORIZED, "invalid_token", message),
            Self::BadRequest(message) => (StatusCode::UNPROCESSABLE_ENTITY, "bad_request", message),
        };
        (
            status,
            Json(ErrorBody {
                code: code.to_string(),
                message,
            }),
        )
            .into_response()
    }
}

impl From<AuthError> for ApiError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::InvalidAccessKey => Self::InvalidAccessKey,
            AuthError::EmptyNickname => Self::BadRequest(err.to_string()),
            AuthError::InvalidToken(message) => Self::InvalidToken(message),
        }
    }
}

pub async fn run(
    config: Config,
    shared: SharedState,
    action_log: ActionLogger,
) -> anyhow::Result<()> {
    let addr = config.http_listen_addr.clone();
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/auth", post(auth))
        .route("/v1/messages", post(send_message))
        .with_state(AppState {
            shared,
            config,
            action_log,
        });

    let listener = TcpListener::bind(&addr).await?;
    info!(addr = %addr, "http listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn auth(
    State(app): State<AppState>,
    Json(body): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    auth::validate_access_key(&app.config.access_key, &body.access_key)?;
    let nickname = auth::normalize_nickname(&body.nickname)?;
    let token = auth::issue_token(&nickname, &app.config.jwt_secret)?;
    app.action_log.record("http_auth", &nickname, None);
    Ok(Json(AuthResponse { token, nickname }))
}

async fn send_message(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, ApiError> {
    let token = extract_bearer_token(&headers).ok_or(ApiError::Unauthorized)?;
    let claims = auth::verify_token(&token, &app.config.jwt_secret)?;

    let channel = body.channel.trim();
    if channel.is_empty() {
        return Err(ApiError::BadRequest("channel must not be empty".into()));
    }
    if body.content.is_empty() {
        return Err(ApiError::BadRequest("content must not be empty".into()));
    }

    let name = match body.name.as_deref() {
        Some(raw) => auth::normalize_nickname(raw)?,
        None => auth::normalize_nickname(&claims.sub)?,
    };

    let outcome = publish::publish_message(&app.shared, channel, &name, &body.content).await;
    app.action_log.record("http_message", &name, Some(channel));

    Ok(Json(SendMessageResponse {
        ok: true,
        channel: channel.to_string(),
        name,
        content: body.content,
        submit_at: outcome.submit_at,
        recipients: outcome.recipients,
        mentions: MentionsAck {
            delivered: outcome.mentions_delivered,
            offline: outcome.mentions_offline,
        },
    }))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = headers.get(axum::http::header::AUTHORIZATION)?;
    let auth = auth.to_str().ok()?;
    let bearer = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))?;
    Some(bearer.to_string())
}
