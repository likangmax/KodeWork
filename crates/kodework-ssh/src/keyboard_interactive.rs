#![forbid(unsafe_code)]

use crate::SshError;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeyboardPrompt {
    pub prompt: String,
    pub echo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeyboardInteractiveRequest {
    pub request_id: u64,
    pub name: String,
    pub instructions: String,
    pub prompts: Vec<KeyboardPrompt>,
}

pub struct KeyboardInteractiveBroker {
    requests: Mutex<HashMap<u64, oneshot::Sender<Vec<String>>>>,
    pending: Mutex<VecDeque<KeyboardInteractiveRequest>>,
    next_id: AtomicU64,
    timeout: Duration,
}

impl std::fmt::Debug for KeyboardInteractiveBroker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pending = self.pending.lock().map(|guard| guard.len()).unwrap_or(0);
        formatter
            .debug_struct("KeyboardInteractiveBroker")
            .field("pending_requests", &pending)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl KeyboardInteractiveBroker {
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
            pending: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
            timeout,
        }
    }

    pub fn drain_requests(&self) -> Vec<KeyboardInteractiveRequest> {
        self.pending
            .lock()
            .map(|mut guard| guard.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn answer(&self, request_id: u64, responses: Vec<String>) -> bool {
        let sender = self
            .requests
            .lock()
            .ok()
            .and_then(|mut guard| guard.remove(&request_id));
        sender.is_some_and(|sender| sender.send(responses).is_ok())
    }

    pub async fn prompt(
        &self,
        name: String,
        instructions: String,
        prompts: Vec<KeyboardPrompt>,
    ) -> Result<Vec<String>, SshError> {
        if prompts.len() > 32 {
            return Err(SshError::InvalidConfiguration(
                "keyboard-interactive prompt count exceeds 32".into(),
            ));
        }
        let request_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = oneshot::channel();
        self.requests
            .lock()
            .map_err(|_| SshError::Cancelled)?
            .insert(request_id, sender);
        self.pending
            .lock()
            .map_err(|_| SshError::Cancelled)?
            .push_back(KeyboardInteractiveRequest {
                request_id,
                name,
                instructions,
                prompts,
            });
        let result = tokio::time::timeout(self.timeout, receiver)
            .await
            .map_err(|_| SshError::Timeout)?
            .map_err(|_| SshError::Cancelled);
        self.forget(request_id);
        result
    }

    fn forget(&self, request_id: u64) {
        if let Ok(mut guard) = self.requests.lock() {
            guard.remove(&request_id);
        }
        if let Ok(mut guard) = self.pending.lock() {
            guard.retain(|request| request.request_id != request_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn prompt_round_trip_and_cleanup() {
        let broker = Arc::new(KeyboardInteractiveBroker::new(Duration::from_secs(2)));
        let task_broker = Arc::clone(&broker);
        let task = tokio::spawn(async move {
            task_broker
                .prompt(
                    "OTP".into(),
                    "Enter the current code".into(),
                    vec![KeyboardPrompt {
                        prompt: "Code: ".into(),
                        echo: false,
                    }],
                )
                .await
        });
        let request = loop {
            if let Some(request) = broker.drain_requests().into_iter().next() {
                break request;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert!(broker.answer(request.request_id, vec!["123456".into()]));
        assert_eq!(
            task.await.unwrap_or_else(|error| unreachable!("{error}")),
            Ok(vec!["123456".into()])
        );
        assert!(broker.drain_requests().is_empty());
    }
}
