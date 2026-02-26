use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::RwLock,
    task::JoinHandle,
    time::{Duration, interval},
};
use ts_rs::TS;

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ClaudeUsageData {
    pub configured: bool,
    pub daily_input_tokens: Option<u64>,
    pub daily_output_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub last_updated: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl Default for ClaudeUsageData {
    fn default() -> Self {
        Self {
            configured: false,
            daily_input_tokens: None,
            daily_output_tokens: None,
            cache_creation_tokens: None,
            cache_read_tokens: None,
            last_updated: None,
            error: None,
        }
    }
}

pub type UsageCache = Arc<RwLock<Option<ClaudeUsageData>>>;

pub struct UsagePoller {
    cache: UsageCache,
}

impl UsagePoller {
    pub fn new(cache: UsageCache) -> Self {
        Self { cache }
    }

    pub fn spawn(cache: UsageCache) -> JoinHandle<()> {
        let poller = Self::new(cache);
        tokio::spawn(async move {
            poller.start().await;
        })
    }

    async fn start(&self) {
        let mut interval = interval(Duration::from_secs(300));

        loop {
            interval.tick().await;
            self.poll().await;
        }
    }

    async fn poll(&self) {
        tracing::debug!("UsagePoller: polling for usage data");
        let mut guard = self.cache.write().await;
        *guard = Some(ClaudeUsageData::default());
    }
}
