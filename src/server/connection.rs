use crate::auth::{self, AuthError};
use crate::config::Config;
use crate::mention::extract_mentions;
use crate::protocol::{ClientFrame, ServerFrame};
use crate::server::state::SharedState;
use anyhow::{Context, Result};
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, warn};

pub async fn handle_connection(
    stream: TcpStream,
    conn_id: u64,
    state: SharedState,
    config: Config,
) -> Result<()> {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".into());
    debug!(conn_id, %peer, "connection opened");

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerFrame>();

    let write_task = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            let mut line = serde_json::to_string(&frame).unwrap_or_else(|_| {
                serde_json::to_string(&ServerFrame::error("internal", "serialize failed"))
                    .unwrap_or_else(|_| {
                        r#"{"type":"error","code":"internal","message":"serialize failed"}"#
                            .to_string()
                    })
            });
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if writer.flush().await.is_err() {
                break;
            }
        }
    });

    let result = run_session(conn_id, &mut lines, &out_tx, &state, &config).await;

    state.remove_connection(conn_id).await;
    drop(out_tx);
    let _ = write_task.await;

    if let Err(err) = &result {
        warn!(conn_id, %peer, error = %err, "connection ended with error");
    } else {
        debug!(conn_id, %peer, "connection closed");
    }
    result
}

async fn run_session(
    conn_id: u64,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
    out_tx: &mpsc::UnboundedSender<ServerFrame>,
    state: &SharedState,
    config: &Config,
) -> Result<()> {
let nickname = match authenticate(conn_id, lines, out_tx, state, config).await? {
        Some(n) => n,
        None => return Ok(()),
    };

    let ctx = SessionCtx {
        conn_id,
        nickname: &nickname,
        out_tx,
        state,
        config,
    };

    while let Some(line) = lines.next_line().await.context("read line")? {
        if line.trim().is_empty() {
            continue;
        }

        let frame: ClientFrame = match serde_json::from_str(&line) {
            Ok(f) => f,
            Err(err) => {
                let _ = out_tx.send(ServerFrame::error(
                    "bad_request",
                    format!("invalid json: {err}"),
                ));
                continue;
            }
        };

        match frame {
            ClientFrame::Auth { .. } => {
                let _ = out_tx.send(ServerFrame::error(
                    "already_authenticated",
                    "connection already authenticated",
                ));
            }
            ClientFrame::Join { token, channel } => {
                if let Err(err) = handle_join(&ctx, &token, &channel).await {
                    let _ = out_tx.send(err);
                }
            }
            ClientFrame::Message {
                token,
                channel,
                content,
            } => {
                if let Err(err) = handle_message(&ctx, &token, &channel, &content).await {
                    let _ = out_tx.send(err);
                }
            }
        }
    }

    Ok(())
}

async fn authenticate(
    conn_id: u64,
    lines: &mut tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
    out_tx: &mpsc::UnboundedSender<ServerFrame>,
    state: &SharedState,
    config: &Config,
) -> Result<Option<String>> {
    let Some(line) = lines.next_line().await.context("read auth line")? else {
        return Ok(None);
    };

    let frame: ClientFrame = match serde_json::from_str(&line) {
        Ok(f) => f,
        Err(err) => {
            let _ = out_tx.send(ServerFrame::error(
                "bad_request",
                format!("invalid json: {err}"),
            ));
            return Ok(None);
        }
    };

    let ClientFrame::Auth {
        access_key,
        nickname,
    } = frame
    else {
        let _ = out_tx.send(ServerFrame::error(
            "unauthorized",
            "first frame must be auth",
        ));
        return Ok(None);
    };

    if let Err(err) = auth::validate_access_key(&config.access_key, &access_key) {
        let _ = out_tx.send(map_auth_error(err));
        return Ok(None);
    }

    let nickname = match auth::normalize_nickname(&nickname) {
        Ok(n) => n,
        Err(err) => {
            let _ = out_tx.send(map_auth_error(err));
            return Ok(None);
        }
    };

    if let Err(msg) = state.try_reserve_nickname(conn_id, &nickname).await {
        let _ = out_tx.send(ServerFrame::error("nickname_taken", msg));
        return Ok(None);
    }

    let token = match auth::issue_token(&nickname, &config.jwt_secret) {
        Ok(t) => t,
        Err(err) => {
            state.remove_connection(conn_id).await;
            let _ = out_tx.send(map_auth_error(err));
            return Ok(None);
        }
    };

    state.register_outbound(conn_id, out_tx.clone()).await;

    let _ = out_tx.send(ServerFrame::AuthOk {
        token,
        nickname: nickname.clone(),
    });
    Ok(Some(nickname))
}

struct SessionCtx<'a> {
    conn_id: u64,
    nickname: &'a str,
    out_tx: &'a mpsc::UnboundedSender<ServerFrame>,
    state: &'a SharedState,
    config: &'a Config,
}

async fn handle_join(ctx: &SessionCtx<'_>, token: &str, channel: &str) -> Result<(), ServerFrame> {
    let claims = auth::verify_token(token, &ctx.config.jwt_secret).map_err(map_auth_error)?;
    if claims.sub != ctx.nickname {
        return Err(ServerFrame::error(
            "unauthorized",
            "token subject mismatch",
        ));
    }

    let channel = channel.trim();
    if channel.is_empty() {
        return Err(ServerFrame::error("bad_request", "channel must not be empty"));
    }

    ctx.state
        .join_channel(ctx.conn_id, channel, ctx.out_tx.clone())
        .await
        .map_err(|e| ServerFrame::error("join_failed", e))?;

    let _ = ctx.out_tx.send(ServerFrame::Joined {
        channel: channel.to_string(),
    });
    Ok(())
}

async fn handle_message(
    ctx: &SessionCtx<'_>,
    token: &str,
    channel: &str,
    content: &str,
) -> Result<(), ServerFrame> {
    let claims = auth::verify_token(token, &ctx.config.jwt_secret).map_err(map_auth_error)?;
    if claims.sub != ctx.nickname {
        return Err(ServerFrame::error(
            "unauthorized",
            "token subject mismatch",
        ));
    }

    let channel = channel.trim();
    if channel.is_empty() {
        return Err(ServerFrame::error("bad_request", "channel must not be empty"));
    }

    // Auto-join on first message to a channel.
    ctx.state
        .join_channel(ctx.conn_id, channel, ctx.out_tx.clone())
        .await
        .map_err(|e| ServerFrame::error("join_failed", e))?;

    let submit_at = Utc::now().timestamp();
    let frame = ServerFrame::Message {
        channel: channel.to_string(),
        content: content.to_string(),
        name: ctx.nickname.to_string(),
        submit_at,
    };

    ctx.state.broadcast(channel, frame, None).await;
    route_mention_dms(ctx, channel, content, submit_at).await;
    Ok(())
}

async fn route_mention_dms(ctx: &SessionCtx<'_>, channel: &str, content: &str, submit_at: i64) {
    let mentions = extract_mentions(content);
    if mentions.is_empty() {
        return;
    }

    let sender_key = ctx.nickname.to_ascii_lowercase();
    let mut delivered = Vec::new();
    let mut offline = Vec::new();

    for mention in mentions {
        if mention.to_ascii_lowercase() == sender_key {
            continue;
        }

        let dm = ServerFrame::Dm {
            from: ctx.nickname.to_string(),
            to: mention.clone(),
            channel: channel.to_string(),
            content: content.to_string(),
            submit_at,
        };

        if ctx.state.send_to_nickname(&mention, dm).await {
            delivered.push(mention);
        } else {
            offline.push(mention);
        }
    }

    if !delivered.is_empty() || !offline.is_empty() {
        let _ = ctx.out_tx.send(ServerFrame::MentionResult {
            delivered,
            offline,
        });
    }
}

fn map_auth_error(err: AuthError) -> ServerFrame {
    match err {
        AuthError::InvalidAccessKey => ServerFrame::error("invalid_access_key", err.to_string()),
        AuthError::EmptyNickname => ServerFrame::error("bad_request", err.to_string()),
        AuthError::InvalidToken(msg) => ServerFrame::error("invalid_token", msg),
    }
}
