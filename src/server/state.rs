use crate::protocol::ServerFrame;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

pub type OutboundTx = mpsc::UnboundedSender<ServerFrame>;

#[derive(Debug, Default)]
struct Inner {
    nicknames: HashSet<String>,
    /// connection_id -> nickname
    connections: HashMap<u64, String>,
    /// connection_id -> personal outbound (for DMs)
    outbound: HashMap<u64, OutboundTx>,
    /// lowercase nickname -> connection_id
    nick_index: HashMap<String, u64>,
    /// channel -> connection_id -> sender
    channels: HashMap<String, HashMap<u64, OutboundTx>>,
    /// connection_id -> channels joined
    memberships: HashMap<u64, HashSet<String>>,
}

#[derive(Clone, Default)]
pub struct SharedState {
    inner: Arc<RwLock<Inner>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn try_reserve_nickname(
        &self,
        conn_id: u64,
        nickname: &str,
    ) -> Result<(), String> {
        let key = nickname.to_ascii_lowercase();
        let mut guard = self.inner.write().await;
        if guard.nicknames.contains(&key) {
            return Err("nickname already in use".into());
        }
        guard.nicknames.insert(key.clone());
        guard.nick_index.insert(key, conn_id);
        guard.connections.insert(conn_id, nickname.to_string());
        guard.memberships.insert(conn_id, HashSet::new());
        Ok(())
    }

    pub async fn register_outbound(&self, conn_id: u64, tx: OutboundTx) {
        let mut guard = self.inner.write().await;
        guard.outbound.insert(conn_id, tx);
    }

    /// Deliver `frame` to an online nickname (case-insensitive).
    pub async fn send_to_nickname(&self, nickname: &str, frame: ServerFrame) -> bool {
        let key = nickname.to_ascii_lowercase();
        let guard = self.inner.read().await;
        let Some(&conn_id) = guard.nick_index.get(&key) else {
            return false;
        };
        let Some(tx) = guard.outbound.get(&conn_id) else {
            return false;
        };
        tx.send(frame).is_ok()
    }

    pub async fn join_channel(
        &self,
        conn_id: u64,
        channel: &str,
        tx: OutboundTx,
    ) -> Result<(), String> {
        let mut guard = self.inner.write().await;
        if !guard.connections.contains_key(&conn_id) {
            return Err("not authenticated".into());
        }
        guard
            .channels
            .entry(channel.to_string())
            .or_default()
            .insert(conn_id, tx);
        guard
            .memberships
            .entry(conn_id)
            .or_default()
            .insert(channel.to_string());
        Ok(())
    }

    pub async fn broadcast(&self, channel: &str, frame: ServerFrame, except: Option<u64>) {
        let guard = self.inner.read().await;
        let Some(members) = guard.channels.get(channel) else {
            return;
        };
        for (conn_id, tx) in members {
            if except.is_some_and(|id| id == *conn_id) {
                continue;
            }
            let _ = tx.send(frame.clone());
        }
    }

    pub async fn remove_connection(&self, conn_id: u64) {
        let mut guard = self.inner.write().await;
        guard.outbound.remove(&conn_id);
        if let Some(nickname) = guard.connections.remove(&conn_id) {
            let key = nickname.to_ascii_lowercase();
            guard.nicknames.remove(&key);
            guard.nick_index.remove(&key);
        }
        if let Some(channels) = guard.memberships.remove(&conn_id) {
            for channel in channels {
                if let Some(members) = guard.channels.get_mut(&channel) {
                    members.remove(&conn_id);
                    if members.is_empty() {
                        guard.channels.remove(&channel);
                    }
                }
            }
        }
    }
}
