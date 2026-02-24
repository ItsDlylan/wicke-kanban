use std::sync::Arc;

use executors::logs::{
    NormalizedEntryType, TokenUsageInfo, utils::patch::extract_normalized_entry_from_patch,
};
use tokio::{sync::oneshot, task::JoinHandle};
use utils::{log_msg::LogMsg, msg_store::MsgStore};
use uuid::Uuid;

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
    pub fn watch(
        execution_process_id: Uuid,
        msg_store: Arc<MsgStore>,
        threshold: f64,
    ) -> (JoinHandle<()>, oneshot::Receiver<ContextThresholdEvent>) {
        let (tx, rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            let mut receiver = msg_store.get_receiver();

            loop {
                match receiver.recv().await {
                    Ok(log_msg) => {
                        if let Some(event) =
                            Self::check_threshold(execution_process_id, &log_msg, threshold)
                        {
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

#[cfg(test)]
mod tests {
    use executors::logs::{
        NormalizedEntry, NormalizedEntryType, TokenUsageInfo, utils::patch::ConversationPatch,
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
}
