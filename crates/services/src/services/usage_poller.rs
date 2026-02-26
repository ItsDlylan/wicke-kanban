use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{sync::RwLock, task::JoinHandle};
use tracing;
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

#[derive(Debug)]
enum PollError {
    NoToken,
    RateLimited,
    AuthError(String),
    NetworkError(String),
    ParseError(String),
}

#[cfg(target_os = "macos")]
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
        tracing::debug!("Failed to read Claude Code credentials from Keychain");
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    let json: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    let token = json
        .get("claudeAiOauth")?
        .get("accessToken")?
        .as_str()?
        .to_string();

    if token.is_empty() { None } else { Some(token) }
}

#[cfg(not(target_os = "macos"))]
fn get_oauth_token() -> Option<String> {
    None
}

pub struct UsagePoller {
    data: Arc<RwLock<Option<ClaudeUsageData>>>,
    client: reqwest::Client,
}

impl UsagePoller {
    pub fn new() -> (Self, Arc<RwLock<Option<ClaudeUsageData>>>) {
        let data = Arc::new(RwLock::new(None));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("Failed to build HTTP client");

        let poller = Self {
            data: Arc::clone(&data),
            client,
        };

        (poller, data)
    }

    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            self.run_loop().await;
        })
    }

    async fn run_loop(&self) {
        const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(30);
        const MAX_BACKOFF: Duration = Duration::from_secs(120);

        let mut current_backoff = DEFAULT_POLL_INTERVAL;

        loop {
            tokio::time::sleep(current_backoff).await;

            let token = match get_oauth_token() {
                Some(t) => t,
                None => {
                    let mut guard = self.data.write().await;
                    *guard = Some(ClaudeUsageData {
                        configured: false,
                        ..Default::default()
                    });
                    current_backoff = DEFAULT_POLL_INTERVAL;
                    continue;
                }
            };

            match self.poll_once(&token).await {
                Ok(body) => {
                    tracing::debug!("Usage poll successful");
                    let mut guard = self.data.write().await;
                    *guard = Some(ClaudeUsageData {
                        configured: true,
                        daily_input_tokens: body.get("daily_input_tokens").and_then(|v| v.as_u64()),
                        daily_output_tokens: body
                            .get("daily_output_tokens")
                            .and_then(|v| v.as_u64()),
                        cache_creation_tokens: body
                            .get("cache_creation_tokens")
                            .and_then(|v| v.as_u64()),
                        cache_read_tokens: body.get("cache_read_tokens").and_then(|v| v.as_u64()),
                        last_updated: Some(Utc::now()),
                        error: None,
                    });
                    current_backoff = DEFAULT_POLL_INTERVAL;
                }
                Err(PollError::NoToken) => {
                    let mut guard = self.data.write().await;
                    *guard = Some(ClaudeUsageData {
                        configured: false,
                        ..Default::default()
                    });
                    current_backoff = DEFAULT_POLL_INTERVAL;
                }
                Err(PollError::RateLimited) => {
                    tracing::warn!(
                        "Usage API rate limited, backing off to {:?}",
                        current_backoff * 2
                    );
                    current_backoff = (current_backoff * 2).min(MAX_BACKOFF);
                }
                Err(PollError::AuthError(msg)) => {
                    tracing::warn!("Usage API auth error: {}", msg);
                    let mut guard = self.data.write().await;
                    if let Some(ref mut data) = *guard {
                        data.error = Some("OAuth token expired or invalid".to_string());
                    } else {
                        *guard = Some(ClaudeUsageData {
                            configured: true,
                            error: Some("OAuth token expired or invalid".to_string()),
                            ..Default::default()
                        });
                    }
                    current_backoff = DEFAULT_POLL_INTERVAL;
                }
                Err(PollError::NetworkError(msg)) => {
                    tracing::warn!("Usage API network error: {}", msg);
                    let mut guard = self.data.write().await;
                    if let Some(ref mut data) = *guard {
                        data.error = Some(msg);
                    } else {
                        *guard = Some(ClaudeUsageData {
                            configured: true,
                            error: Some(msg),
                            ..Default::default()
                        });
                    }
                    current_backoff = DEFAULT_POLL_INTERVAL;
                }
                Err(PollError::ParseError(msg)) => {
                    tracing::warn!("Usage API parse error: {}", msg);
                    let mut guard = self.data.write().await;
                    if let Some(ref mut data) = *guard {
                        data.error = Some(format!("Failed to parse response: {}", msg));
                    } else {
                        *guard = Some(ClaudeUsageData {
                            configured: true,
                            error: Some(format!("Failed to parse response: {}", msg)),
                            ..Default::default()
                        });
                    }
                    current_backoff = DEFAULT_POLL_INTERVAL;
                }
            }
        }
    }

    async fn poll_once(&self, token: &str) -> Result<serde_json::Value, PollError> {
        let response = self
            .client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/json")
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("User-Agent", "claude-code/2.0.31")
            .send()
            .await
            .map_err(|e| PollError::NetworkError(e.to_string()))?;

        let status = response.status();

        match status.as_u16() {
            200 => {
                let body: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| PollError::ParseError(e.to_string()))?;
                Ok(body)
            }
            401 | 403 => {
                let msg = format!("OAuth token expired or invalid (HTTP {})", status.as_u16());
                Err(PollError::AuthError(msg))
            }
            429 => Err(PollError::RateLimited),
            _ => {
                let msg = format!("Unexpected HTTP status: {}", status.as_u16());
                Err(PollError::NetworkError(msg))
            }
        }
    }
}
