use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::RwLock,
    task::JoinHandle,
    time::{Duration, interval},
};
use ts_rs::TS;

/// Shared cache file written by Claude Code's statusline script every ~30s.
const USAGE_CACHE_FILE: &str = ".claude/usage.json";

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ClaudeUsageData {
    pub configured: bool,
    pub usage: Option<serde_json::Value>,
    pub last_updated_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl Default for ClaudeUsageData {
    fn default() -> Self {
        Self {
            configured: false,
            usage: None,
            last_updated_at: None,
            error: None,
        }
    }
}

pub type UsageCache = Arc<RwLock<Option<ClaudeUsageData>>>;

/// Response shape from the Anthropic usage API (cached in ~/.claude/usage.json).
#[derive(Deserialize)]
struct UsageApiResponse {
    five_hour: Option<UsageWindow>,
    seven_day: Option<UsageWindow>,
}

#[derive(Deserialize)]
struct UsageWindow {
    utilization: f64,
    resets_at: Option<String>,
}

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
        // Read the cache file every 10s — it's just a local file read, very cheap.
        // The file itself is refreshed every ~30s by Claude Code's statusline script.
        let mut interval = interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            self.poll().await;
        }
    }

    /// Read and parse the shared usage cache file.
    /// Returns Ok(Some) for valid usage, Ok(None) if file exists but has an error/unrecognized
    /// response, and Err if the file doesn't exist or can't be read.
    fn read_cache_file() -> Result<Option<UsageApiResponse>, ()> {
        let home = std::env::var_os("HOME").ok_or(())?;
        let path = std::path::Path::new(&home).join(USAGE_CACHE_FILE);
        let contents = std::fs::read_to_string(&path).map_err(|_| ())?;
        // File exists — try to parse as usage response
        Ok(serde_json::from_str(&contents).ok())
    }

    /// Transform raw API response into the shape the frontend expects.
    fn transform_usage(api: &UsageApiResponse) -> serde_json::Value {
        let mut windows = serde_json::Map::new();

        if let Some(ref w) = api.five_hour {
            let mut entry = serde_json::Map::new();
            entry.insert("percentage".into(), serde_json::json!(w.utilization));
            if let Some(ref r) = w.resets_at {
                entry.insert("reset_at".into(), serde_json::json!(r));
            }
            windows.insert("5h".into(), serde_json::Value::Object(entry));
        }

        if let Some(ref w) = api.seven_day {
            let mut entry = serde_json::Map::new();
            entry.insert("percentage".into(), serde_json::json!(w.utilization));
            if let Some(ref r) = w.resets_at {
                entry.insert("reset_at".into(), serde_json::json!(r));
            }
            windows.insert("7d".into(), serde_json::Value::Object(entry));
        }

        serde_json::json!({ "usage_windows": windows })
    }

    async fn poll(&self) {
        let data = match Self::read_cache_file() {
            Ok(Some(api)) => ClaudeUsageData {
                configured: true,
                usage: Some(Self::transform_usage(&api)),
                last_updated_at: Some(Utc::now()),
                error: None,
            },
            Ok(None) => {
                // File exists but contains an error response (e.g. rate limit) —
                // still configured, just no data right now.
                tracing::debug!("UsagePoller: cache file exists but has no usage data");
                ClaudeUsageData {
                    configured: true,
                    usage: None,
                    last_updated_at: Some(Utc::now()),
                    error: None,
                }
            }
            Err(()) => {
                // File doesn't exist yet — not configured
                tracing::debug!("UsagePoller: cache file not found");
                ClaudeUsageData::default()
            }
        };

        let mut guard = self.cache.write().await;
        *guard = Some(data);
    }
}
