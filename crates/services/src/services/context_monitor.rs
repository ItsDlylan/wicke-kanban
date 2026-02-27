use std::sync::Arc;

use executors::logs::{
    NormalizedEntryType, TokenUsageInfo, utils::patch::extract_normalized_entry_from_patch,
};
use tokio::{sync::oneshot, task::JoinHandle};
use utils::{log_msg::LogMsg, msg_store::MsgStore};
use uuid::Uuid;

/// Adaptive threshold tuning parameter (section 4.2, Proposition 2).
/// Higher values make the threshold more sensitive to tool density.
const TOOL_DENSITY_LAMBDA: f64 = 0.15;

#[derive(Debug, Clone)]
pub struct ContextThresholdEvent {
    pub execution_process_id: Uuid,
    pub tokens_used: u32,
    pub context_window: u32,
    pub utilization: f64,
}

pub struct ContextMonitor;

impl ContextMonitor {
    /// Subscribes to a MsgStore broadcast channel and sends a one-shot signal
    /// when token utilization crosses the given threshold (0.0 to 1.0).
    /// Implements adaptive threshold adjustment based on tool density (section 4.2).
    pub fn watch(
        execution_process_id: Uuid,
        msg_store: Arc<MsgStore>,
        threshold: f64,
    ) -> (JoinHandle<()>, oneshot::Receiver<ContextThresholdEvent>) {
        let (tx, rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            let mut receiver = msg_store.get_receiver();
            let mut tracker = ToolDensityTracker::new();

            loop {
                match receiver.recv().await {
                    Ok(log_msg) => {
                        // Track tool usage for adaptive threshold
                        tracker.observe(&log_msg);

                        // Compute adaptive threshold
                        let adaptive_threshold =
                            compute_adaptive_threshold(threshold, tracker.tool_density());

                        if let Some(event) = Self::check_threshold(
                            execution_process_id,
                            &log_msg,
                            adaptive_threshold,
                        ) {
                            let _ = tx.send(event);
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "Context monitor lagged {n} messages for exec {execution_process_id}"
                        );
                    }
                }
            }
        });

        (handle, rx)
    }

    fn check_threshold(
        execution_process_id: Uuid,
        log_msg: &LogMsg,
        threshold: f64,
    ) -> Option<ContextThresholdEvent> {
        let patch = match log_msg {
            LogMsg::JsonPatch(patch) => patch,
            _ => return None,
        };

        let (_index, entry) = extract_normalized_entry_from_patch(patch)?;

        let TokenUsageInfo {
            total_tokens,
            model_context_window,
        } = match entry.entry_type {
            NormalizedEntryType::TokenUsageInfo(info) => info,
            _ => return None,
        };

        if model_context_window == 0 {
            return None;
        }

        let utilization = total_tokens as f64 / model_context_window as f64;

        if utilization >= threshold {
            Some(ContextThresholdEvent {
                execution_process_id,
                tokens_used: total_tokens,
                context_window: model_context_window,
                utilization,
            })
        } else {
            None
        }
    }
}

/// Adaptive threshold formula (section 4.2, Proposition 2):
/// `theta_adaptive(alpha) = theta_base - lambda * tool_density(alpha)`
///
/// Tool-heavy agents (high tool density) get a lower threshold, triggering
/// succession earlier since tool-call tokens consume context faster.
/// The threshold is clamped to [0.3, theta_base] to avoid extreme values.
fn compute_adaptive_threshold(base_threshold: f64, tool_density: f64) -> f64 {
    let adjusted = base_threshold - TOOL_DENSITY_LAMBDA * tool_density;
    adjusted.clamp(0.3, base_threshold)
}

/// Tracks the ratio of tool-use messages to total messages for computing
/// adaptive context thresholds.
struct ToolDensityTracker {
    tool_messages: u32,
    total_messages: u32,
}

impl ToolDensityTracker {
    fn new() -> Self {
        Self {
            tool_messages: 0,
            total_messages: 0,
        }
    }

    fn observe(&mut self, log_msg: &LogMsg) {
        let patch = match log_msg {
            LogMsg::JsonPatch(patch) => patch,
            _ => return,
        };

        if let Some((_index, entry)) = extract_normalized_entry_from_patch(patch) {
            match &entry.entry_type {
                NormalizedEntryType::ToolUse { .. } => {
                    self.tool_messages += 1;
                    self.total_messages += 1;
                }
                NormalizedEntryType::AssistantMessage
                | NormalizedEntryType::UserMessage
                | NormalizedEntryType::SystemMessage => {
                    self.total_messages += 1;
                }
                _ => {}
            }
        }
    }

    fn tool_density(&self) -> f64 {
        if self.total_messages == 0 {
            0.0
        } else {
            self.tool_messages as f64 / self.total_messages as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use executors::logs::{
        ActionType, NormalizedEntry, NormalizedEntryType, TokenUsageInfo, ToolStatus,
        utils::patch::ConversationPatch,
    };

    use super::*;

    fn make_token_usage_patch(total_tokens: u32, model_context_window: u32) -> LogMsg {
        let entry = NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::TokenUsageInfo(TokenUsageInfo {
                total_tokens,
                model_context_window,
            }),
            content: String::new(),
            metadata: None,
        };
        LogMsg::JsonPatch(ConversationPatch::add_normalized_entry(0, entry))
    }

    fn make_assistant_msg_patch() -> LogMsg {
        let entry = NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::AssistantMessage,
            content: "hello".to_string(),
            metadata: None,
        };
        LogMsg::JsonPatch(ConversationPatch::add_normalized_entry(0, entry))
    }

    fn make_tool_use_patch() -> LogMsg {
        let entry = NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: "bash".to_string(),
                action_type: ActionType::CommandRun {
                    command: "ls".to_string(),
                    result: None,
                    category: Default::default(),
                },
                status: ToolStatus::Success,
            },
            content: "ran command".to_string(),
            metadata: None,
        };
        LogMsg::JsonPatch(ConversationPatch::add_normalized_entry(0, entry))
    }

    #[tokio::test]
    async fn fires_when_threshold_crossed() {
        let msg_store = Arc::new(MsgStore::new());
        let exec_id = Uuid::new_v4();

        let (handle, rx) = ContextMonitor::watch(exec_id, msg_store.clone(), 0.8);

        // Yield to let the spawned task subscribe to the broadcast channel
        tokio::task::yield_now().await;

        // Below threshold — should not fire
        msg_store.push(make_token_usage_patch(70_000, 200_000));

        // Non-token-usage patch — should be ignored
        msg_store.push(make_assistant_msg_patch());

        // Non-patch message — should be ignored
        msg_store.push(LogMsg::Stdout("hello".to_string()));

        // Above threshold — should fire
        msg_store.push(make_token_usage_patch(170_000, 200_000));

        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx)
            .await
            .expect("timed out waiting for threshold event")
            .expect("channel closed without event");

        assert_eq!(event.execution_process_id, exec_id);
        assert_eq!(event.tokens_used, 170_000);
        assert_eq!(event.context_window, 200_000);
        assert!((event.utilization - 0.85).abs() < 0.001);

        handle.abort();
    }

    #[tokio::test]
    async fn does_not_fire_below_threshold() {
        let msg_store = Arc::new(MsgStore::new());
        let exec_id = Uuid::new_v4();

        let (handle, rx) = ContextMonitor::watch(exec_id, msg_store.clone(), 0.8);

        // Yield to let the spawned task subscribe to the broadcast channel
        tokio::task::yield_now().await;

        msg_store.push(make_token_usage_patch(50_000, 200_000));
        msg_store.push(make_token_usage_patch(100_000, 200_000));

        // Close the channel by dropping the store (the receiver will get Closed)
        drop(msg_store);

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), rx).await;

        // Should either timeout or get a RecvError (channel closed without event)
        match result {
            Ok(Ok(_)) => panic!("should not have received a threshold event"),
            _ => {} // expected: timeout or channel closed
        }

        handle.abort();
    }

    #[tokio::test]
    async fn adaptive_threshold_lowers_for_tool_heavy_sessions() {
        // With high tool density, the threshold should be lowered, triggering earlier
        let msg_store = Arc::new(MsgStore::new());
        let exec_id = Uuid::new_v4();

        let (handle, rx) = ContextMonitor::watch(exec_id, msg_store.clone(), 0.8);

        tokio::task::yield_now().await;

        // Push many tool use messages to increase tool density
        for _ in 0..8 {
            msg_store.push(make_tool_use_patch());
        }
        // 2 assistant messages
        for _ in 0..2 {
            msg_store.push(make_assistant_msg_patch());
        }
        // tool_density = 8/10 = 0.8
        // adaptive threshold = 0.8 - 0.15 * 0.8 = 0.68

        // This is above 0.68 but below the original 0.8 threshold
        msg_store.push(make_token_usage_patch(140_000, 200_000)); // 0.7 utilization

        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx)
            .await
            .expect("timed out: adaptive threshold should have triggered")
            .expect("channel closed without event");

        assert_eq!(event.tokens_used, 140_000);
        assert!((event.utilization - 0.7).abs() < 0.001);

        handle.abort();
    }

    #[test]
    fn check_threshold_ignores_non_patch() {
        let exec_id = Uuid::new_v4();
        let msg = LogMsg::Stdout("hello".to_string());
        assert!(ContextMonitor::check_threshold(exec_id, &msg, 0.8).is_none());
    }

    #[test]
    fn check_threshold_ignores_non_token_patches() {
        let exec_id = Uuid::new_v4();
        let msg = make_assistant_msg_patch();
        assert!(ContextMonitor::check_threshold(exec_id, &msg, 0.8).is_none());
    }

    #[test]
    fn check_threshold_returns_event_above_threshold() {
        let exec_id = Uuid::new_v4();
        let msg = make_token_usage_patch(180_000, 200_000);
        let event = ContextMonitor::check_threshold(exec_id, &msg, 0.8);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.tokens_used, 180_000);
        assert!((event.utilization - 0.9).abs() < 0.001);
    }

    #[test]
    fn check_threshold_returns_none_below_threshold() {
        let exec_id = Uuid::new_v4();
        let msg = make_token_usage_patch(50_000, 200_000);
        assert!(ContextMonitor::check_threshold(exec_id, &msg, 0.8).is_none());
    }

    #[test]
    fn check_threshold_handles_zero_context_window() {
        let exec_id = Uuid::new_v4();
        let msg = make_token_usage_patch(100, 0);
        assert!(ContextMonitor::check_threshold(exec_id, &msg, 0.8).is_none());
    }

    #[test]
    fn test_compute_adaptive_threshold() {
        // No tool usage => no adjustment
        assert_eq!(compute_adaptive_threshold(0.6, 0.0), 0.6);

        // 50% tool density => 0.6 - 0.15 * 0.5 = 0.525
        assert!((compute_adaptive_threshold(0.6, 0.5) - 0.525).abs() < 0.001);

        // 100% tool density => 0.6 - 0.15 * 1.0 = 0.45
        assert!((compute_adaptive_threshold(0.6, 1.0) - 0.45).abs() < 0.001);

        // Result clamped to minimum 0.3
        assert_eq!(compute_adaptive_threshold(0.3, 1.0), 0.3);
    }

    #[test]
    fn test_tool_density_tracker() {
        let mut tracker = ToolDensityTracker::new();

        // Empty tracker
        assert_eq!(tracker.tool_density(), 0.0);

        // Add some messages
        tracker.observe(&make_assistant_msg_patch());
        tracker.observe(&make_tool_use_patch());
        tracker.observe(&make_assistant_msg_patch());
        tracker.observe(&make_tool_use_patch());

        // 2 tool / 4 total = 0.5
        assert!((tracker.tool_density() - 0.5).abs() < 0.001);

        // Non-patch messages are ignored
        tracker.observe(&LogMsg::Stdout("hello".to_string()));
        assert!((tracker.tool_density() - 0.5).abs() < 0.001);
    }
}
