use crate::protocol::{ClientFrame, ServerFrame};
use anyhow::{Context, Result, bail};
use clap::Args;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{error, info};

#[derive(Debug, Args)]
pub struct ClientArgs {
    /// Server host:port
    #[arg(long, default_value = "127.0.0.1:9099")]
    pub addr: String,

    /// Shared access key (defaults to ACCESS_KEY env)
    #[arg(long)]
    pub access_key: Option<String>,

    /// Unique nickname for this session
    #[arg(long)]
    pub nickname: String,

    /// Channel to join
    #[arg(long)]
    pub channel: String,
}

pub async fn run(args: ClientArgs) -> Result<()> {
    let _ = dotenvy::dotenv();
    let access_key = args
        .access_key
        .or_else(|| std::env::var("ACCESS_KEY").ok())
        .context("ACCESS_KEY required (flag or env)")?;

    let stream = TcpStream::connect(&args.addr)
        .await
        .with_context(|| format!("connect {}", args.addr))?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    send_frame(
        &mut writer,
        &ClientFrame::Auth {
            access_key,
            nickname: args.nickname.clone(),
        },
    )
    .await?;

    let Some(line) = lines.next_line().await? else {
        bail!("server closed during auth");
    };
    let auth_resp: ServerFrame = serde_json::from_str(&line).context("parse auth response")?;
    let token = match auth_resp {
        ServerFrame::AuthOk { token, nickname } => {
            info!(%nickname, "authenticated");
            token
        }
        ServerFrame::Error { code, message } => {
            bail!("auth failed: {code}: {message}");
        }
        other => bail!("unexpected auth response: {other:?}"),
    };

    send_frame(
        &mut writer,
        &ClientFrame::Join {
            token: token.clone(),
            channel: args.channel.clone(),
        },
    )
    .await?;

    let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        let mut stdin_lines = BufReader::new(stdin()).lines();
        while let Ok(Some(line)) = stdin_lines.next_line().await {
            if stdin_tx.send(line).is_err() {
                break;
            }
        }
    });

    let channel = args.channel.clone();
    println!("joined channel `{channel}`. Type messages and press Enter. Ctrl+C to quit.");

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line? {
                    Some(raw) => {
                        match serde_json::from_str::<ServerFrame>(&raw) {
                            Ok(ServerFrame::Message { channel, content, name, submit_at }) => {
                                println!("[{channel}] {name} ({submit_at}): {content}");
                            }
                            Ok(ServerFrame::Dm { from, to, channel, content, submit_at }) => {
                                println!("[DM {from}->{to} via {channel}] ({submit_at}): {content}");
                            }
                            Ok(ServerFrame::MentionResult { delivered, offline }) => {
                                if !delivered.is_empty() {
                                    println!("dm delivered: {}", delivered.join(", "));
                                }
                                if !offline.is_empty() {
                                    println!("dm offline: {}", offline.join(", "));
                                }
                            }
                            Ok(ServerFrame::Joined { channel }) => {
                                info!(%channel, "joined");
                            }
                            Ok(ServerFrame::Error { code, message }) => {
                                error!(%code, %message, "server error");
                                eprintln!("error [{code}]: {message}");
                            }
                            Ok(other) => {
                                println!("<< {other:?}");
                            }
                            Err(err) => {
                                eprintln!("bad frame: {err}: {raw}");
                            }
                        }
                    }
                    None => {
                        bail!("server disconnected");
                    }
                }
            }
            maybe = stdin_rx.recv() => {
                match maybe {
                    Some(content) if !content.trim().is_empty() => {
                        send_frame(
                            &mut writer,
                            &ClientFrame::Message {
                                token: token.clone(),
                                channel: channel.clone(),
                                content,
                            },
                        ).await?;
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }

    Ok(())
}

async fn send_frame<T: serde::Serialize>(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    frame: &T,
) -> Result<()> {
    let mut line = serde_json::to_string(frame)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
