//! Redis client for the control plane.
//!
//! [`RedisHandle::new`] only parses the URL; the first call to
//! [`RedisHandle::connection`] opens an auto-reconnecting
//! [`redis::aio::ConnectionManager`], which is then shared (cheap clones).
//! Services therefore start even when Redis is down, and readiness reports it.
//!
//! The URL (which may embed a password) is never logged: `Debug` is redacted
//! and connection errors are reported without the URL.

use std::{fmt, sync::Arc, time::Duration};

use redis::{
    AsyncCommands, Client,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::OnceCell;

pub mod lockout;

pub use redis;

/// Time allowed to establish a TCP connection.
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);
/// Time allowed for a single command's response.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Errors from this crate.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// The URL could not be parsed. The URL itself is never included.
    #[error("invalid redis URL")]
    InvalidUrl,
    /// Connection or command error.
    #[error("redis: {0}")]
    Redis(#[from] redis::RedisError),
    /// `PING` returned something other than `PONG`.
    #[error("unexpected PING reply")]
    UnexpectedReply,
}

/// Shared, lazily connected Redis handle. Clone freely.
#[derive(Clone)]
pub struct RedisHandle {
    client: Client,
    manager: Arc<OnceCell<ConnectionManager>>,
}

impl fmt::Debug for RedisHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedisHandle")
            .field("url", &"<redacted>")
            .field("connected", &self.manager.initialized())
            .finish()
    }
}

impl RedisHandle {
    /// Parses the URL without connecting.
    ///
    /// # Errors
    /// [`CacheError::InvalidUrl`] if the URL is malformed.
    pub fn new(url: &SecretString) -> Result<Self, CacheError> {
        let client = Client::open(url.expose_secret()).map_err(|_| CacheError::InvalidUrl)?;
        Ok(Self {
            client,
            manager: Arc::new(OnceCell::new()),
        })
    }

    /// Returns the shared connection, connecting on first use.
    /// A failed first attempt is retried on the next call.
    ///
    /// # Errors
    /// Connection failures.
    pub async fn connection(&self) -> Result<ConnectionManager, CacheError> {
        let manager = self
            .manager
            .get_or_try_init(|| async {
                let config = ConnectionManagerConfig::new()
                    .set_connection_timeout(Some(CONNECTION_TIMEOUT))
                    .set_response_timeout(Some(RESPONSE_TIMEOUT));
                ConnectionManager::new_with_config(self.client.clone(), config).await
            })
            .await?;
        Ok(manager.clone())
    }

    /// Sends `PING` and expects `PONG`.
    ///
    /// # Errors
    /// Connection failures or an unexpected reply.
    pub async fn ping(&self) -> Result<(), CacheError> {
        let mut conn = self.connection().await?;
        let reply: String = redis::cmd("PING").query_async(&mut conn).await?;
        if reply == "PONG" {
            Ok(())
        } else {
            Err(CacheError::UnexpectedReply)
        }
    }

    /// Sets `key` to `value` with a time-to-live.
    ///
    /// # Errors
    /// Connection or command failures.
    pub async fn set_ex(&self, key: &str, value: &str, ttl: Duration) -> Result<(), CacheError> {
        let mut conn = self.connection().await?;
        let () = conn.set_ex(key, value, ttl.as_secs().max(1)).await?;
        Ok(())
    }

    /// Gets a string value.
    ///
    /// # Errors
    /// Connection or command failures.
    pub async fn get(&self, key: &str) -> Result<Option<String>, CacheError> {
        let mut conn = self.connection().await?;
        Ok(conn.get(key).await?)
    }

    /// Publishes a message on a pub/sub channel; returns how many subscribers
    /// received it. Fire-and-forget: nobody listening is not an error.
    ///
    /// # Errors
    /// Connection or command failures.
    pub async fn publish(&self, channel: &str, message: &str) -> Result<u64, CacheError> {
        let mut conn = self.connection().await?;
        Ok(conn.publish(channel, message).await?)
    }

    /// Deletes a key; returns whether it existed.
    ///
    /// # Errors
    /// Connection or command failures.
    pub async fn del(&self, key: &str) -> Result<bool, CacheError> {
        let mut conn = self.connection().await?;
        let removed: u64 = conn.del(key).await?;
        Ok(removed > 0)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn malformed_url_is_rejected_without_echoing_it() {
        let url = SecretString::from("not-a-url:secretpw".to_owned());
        let err = RedisHandle::new(&url).expect_err("must fail");
        assert!(!err.to_string().contains("secretpw"), "{err}");
    }

    #[test]
    fn debug_output_redacts_the_url() {
        let url = SecretString::from("redis://:hunter2@127.0.0.1:6379/0".to_owned());
        let handle = RedisHandle::new(&url).expect("valid url");
        let dbg = format!("{handle:?}");
        assert!(!dbg.contains("hunter2"), "{dbg}");
        assert!(dbg.contains("connected: false"), "{dbg}");
    }

    #[tokio::test]
    async fn ping_fails_cleanly_when_redis_is_down_and_retries_later() {
        // Port 1 on localhost: nothing listens there.
        let url = SecretString::from("redis://:topsecret@127.0.0.1:1/0".to_owned());
        let handle = RedisHandle::new(&url).expect("valid url");
        let err = handle.ping().await.expect_err("nothing listens on port 1");
        assert!(!err.to_string().contains("topsecret"), "{err}");
        // The failed attempt must not be cached as a permanent state.
        assert!(!handle.manager.initialized());
    }
}
