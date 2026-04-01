// SurrealDB connection pool using bb8
// Critical design: SINGLE pool for ALL tenants
// Tenant isolation is enforced by WASI component boundaries, not OS processes

use anyhow::Result;
use bb8::Pool;
use surrealdb::engine::any::{Any, connect};
use std::ops::Deref;

pub struct SurrealConnection {
    inner: surrealdb::Surreal<Any>,
    pub slot_id: u32,
}

impl Deref for SurrealConnection {
    type Target = surrealdb::Surreal<Any>;
    fn deref(&self) -> &Self::Target { &self.inner }
}

pub struct SurrealManager {
    url: String,
    ns:  String,
    db:  String,
    slot_counter: std::sync::atomic::AtomicU32,
}

#[async_trait::async_trait]
impl bb8::ManageConnection for SurrealManager {
    type Connection = SurrealConnection;
    type Error = surrealdb::Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let db = connect(&self.url).await?;
        db.use_ns(&self.ns).use_db(&self.db).await?;
        let slot_id = self.slot_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(SurrealConnection { inner: db, slot_id })
    }

    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        // Lightweight health check: SELECT 1
        conn.query("SELECT 1").await?;
        Ok(())
    }

    fn has_broken(&self, _: &mut Self::Connection) -> bool { false }
}

pub struct ConnectionPool {
    inner: Pool<SurrealManager>,
}

impl ConnectionPool {
    pub async fn new(url: &str, ns: &str, db: &str, max_size: u32) -> Result<Self> {
        let manager = SurrealManager {
            url: url.to_string(),
            ns:  ns.to_string(),
            db:  db.to_string(),
            slot_counter: std::sync::atomic::AtomicU32::new(0),
        };

        let pool = Pool::builder()
            .max_size(max_size)
            .min_idle(Some(std::cmp::min(4, max_size))) // Keep up to 4 warm, never > max_size
            .connection_timeout(std::time::Duration::from_secs(60))
            .idle_timeout(Some(std::time::Duration::from_secs(300)))
            .build(manager)
            .await?;

        Ok(Self { inner: pool })
    }

    pub async fn get(&self) -> Result<bb8::PooledConnection<'_, SurrealManager>> {
        self.inner.get().await.map_err(|e| anyhow::anyhow!("pool error: {}", e))
    }

    /// Non-allocating pool state — safe to call in hot paths
    pub fn state(&self) -> bb8::State {
        self.inner.state()
    }
}
