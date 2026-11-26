use crate::auth::{self, AuthError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const DEFAULT_TTL: Duration = Duration::from_secs(120);

/// Single-use HTTP auth nonces with short TTL.
#[derive(Clone)]
pub struct NonceStore {
    inner: Arc<Mutex<HashMap<String, Instant>>>,
    ttl: Duration,
}

impl Default for NonceStore {
    fn default() -> Self {
        Self::new(DEFAULT_TTL)
    }
}

impl NonceStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    pub async fn issue(&self) -> String {
        let nonce = auth::generate_nonce();
        let mut guard = self.inner.lock().await;
        Self::purge_expired(&mut guard, self.ttl);
        guard.insert(nonce.clone(), Instant::now());
        nonce
    }

    /// Consume a nonce if present and unexpired.
    pub async fn take(&self, nonce: &str) -> Result<(), AuthError> {
        let mut guard = self.inner.lock().await;
        Self::purge_expired(&mut guard, self.ttl);
        match guard.remove(nonce) {
            Some(issued_at) if issued_at.elapsed() <= self.ttl => Ok(()),
            _ => Err(AuthError::InvalidNonce),
        }
    }

    fn purge_expired(map: &mut HashMap<String, Instant>, ttl: Duration) {
        map.retain(|_, issued_at| issued_at.elapsed() <= ttl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn issue_and_take_once() {
        let store = NonceStore::new(Duration::from_secs(60));
        let nonce = store.issue().await;
        assert!(store.take(&nonce).await.is_ok());
        assert!(matches!(
            store.take(&nonce).await,
            Err(AuthError::InvalidNonce)
        ));
    }

    #[tokio::test]
    async fn expired_nonce_rejected() {
        let store = NonceStore::new(Duration::from_millis(1));
        let nonce = store.issue().await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(matches!(
            store.take(&nonce).await,
            Err(AuthError::InvalidNonce)
        ));
    }
}
