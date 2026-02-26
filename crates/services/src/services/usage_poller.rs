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

pub struct UsagePoller {
    cache: UsageCache,
    client: reqwest::Client,
}

/// Response shape from the Anthropic usage API.
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

impl UsagePoller {
    pub fn new(cache: UsageCache) -> Self {
        Self {
            cache,
            client: reqwest::Client::new(),
        }
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

    /// Retrieve the Claude OAuth token from the macOS Keychain.
    fn get_oauth_token() -> Option<String> {
        let output = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "Claude Code-credentials",
                "-w",
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let raw = String::from_utf8(output.stdout).ok()?;
        let json: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
        json.get("claudeAiOauth")?
            .get("accessToken")?
            .as_str()
            .map(|s| s.to_string())
    }

    /// Fetch usage data from the Anthropic API and transform it for the frontend.
    async fn fetch_usage(&self, token: &str) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .get("https://api.anthropic.com/api/oauth/usage")
            .bearer_auth(token)
            .header("anthropic-beta", "oauth-2025-04-20")
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("API returned {status}: {body}"));
        }

        let api: UsageApiResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))?;

        // Transform into the shape the frontend expects: { usage_windows: { "5h": ..., "7d": ... } }
        let mut windows = serde_json::Map::new();

        if let Some(w) = api.five_hour {
            let mut entry = serde_json::Map::new();
            entry.insert("percentage".into(), serde_json::json!(w.utilization));
            if let Some(ref r) = w.resets_at {
                entry.insert("reset_at".into(), serde_json::json!(r));
            }
            windows.insert("5h".into(), serde_json::Value::Object(entry));
        }

        if let Some(w) = api.seven_day {
            let mut entry = serde_json::Map::new();
            entry.insert("percentage".into(), serde_json::json!(w.utilization));
            if let Some(ref r) = w.resets_at {
                entry.insert("reset_at".into(), serde_json::json!(r));
            }
            windows.insert("7d".into(), serde_json::Value::Object(entry));
        }

        Ok(serde_json::json!({ "usage_windows": windows }))
    }

    async fn poll(&self) {
        tracing::debug!("UsagePoller: polling for usage data");

        let token = match Self::get_oauth_token() {
            Some(t) => t,
            None => {
                tracing::debug!("UsagePoller: no OAuth token found, skipping");
                let mut guard = self.cache.write().await;
                *guard = Some(ClaudeUsageData::default());
                return;
            }
        };

        let data = match self.fetch_usage(&token).await {
            Ok(usage) => ClaudeUsageData {
                configured: true,
                usage: Some(usage),
                last_updated_at: Some(Utc::now()),
                error: None,
            },
            Err(e) => {
                tracing::warn!("UsagePoller: failed to fetch usage: {e}");
                ClaudeUsageData {
                    configured: true,
                    usage: None,
                    last_updated_at: Some(Utc::now()),
                    error: Some(e),
                }
            }
        };

        let mut guard = self.cache.write().await;
        *guard = Some(data);
    }
}
