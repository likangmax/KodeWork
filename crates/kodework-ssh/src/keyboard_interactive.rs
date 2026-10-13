#![forbid(unsafe_code)]

use crate::SshError;
use kodework_domain::HostId;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;
use zeroize::Zeroize;

/// Wipes an answer that will never reach the SSH layer.
///
/// The copy handed to `russh` cannot be wiped from here (that API takes
/// ownership of the `Vec<String>`, and keeping a second copy to wipe would leave
/// strictly more plaintext in the process than it removes). This covers the
/// paths where the answer is dropped on our side instead: a stale prompt id, a
/// prompt that already timed out, or a workstation that no longer owns the
/// request. `Vec<Z>::zeroize` zeroes every element before clearing the vector.
fn wipe_responses(responses: &mut Vec<String>) {
    responses.zeroize();
}

/// Delivers an answer if the prompt is still waiting for it, otherwise wipes it.
///
/// Returns whether it was delivered, plus the buffer it was given: on success
/// that buffer is empty (the answer moved to the waiting authenticator), and on
/// failure it is the wiped answer, which makes the wipe observable to tests
/// instead of only inferable.
fn deliver_answer(
    sender: Option<oneshot::Sender<Vec<String>>>,
    mut responses: Vec<String>,
) -> (bool, Vec<String>) {
    match sender {
        Some(sender) => match sender.send(responses) {
            Ok(()) => (true, Vec::new()),
            Err(mut rejected) => {
                // The prompt is gone (timeout, cancellation, aborted connection).
                // The failed send hands the answer back, so wipe it here.
                wipe_responses(&mut rejected);
                (false, rejected)
            }
        },
        None => {
            // Unknown or already-consumed request id.
            wipe_responses(&mut responses);
            (false, responses)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeyboardPrompt {
    pub prompt: String,
    pub echo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeyboardInteractiveRequest {
    pub request_id: u64,
    pub owner_host_id: Option<HostId>,
    pub connection_generation: u64,
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

/// A cancelled/aborted authentication task must not leave an orphaned
/// keyboard-interactive request in the broker.
struct PendingKeyboardGuard<'a> {
    broker: &'a KeyboardInteractiveBroker,
    request_id: u64,
}

impl Drop for PendingKeyboardGuard<'_> {
    fn drop(&mut self) {
        self.broker.forget(self.request_id);
    }
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

    /// Non-destructive, owner-scoped snapshot: switching workstations cannot
    /// consume or misattribute another connection's OTP prompt.
    #[must_use]
    pub fn pending_for_owner(&self, host_id: HostId) -> Vec<KeyboardInteractiveRequest> {
        self.pending
            .lock()
            .map(|guard| {
                guard
                    .iter()
                    .filter(|request| request.owner_host_id == Some(host_id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn answer_for_owner(
        &self,
        host_id: HostId,
        request_id: u64,
        responses: Vec<String>,
    ) -> bool {
        let owned = self
            .pending
            .lock()
            .map(|guard| {
                guard.iter().any(|request| {
                    request.request_id == request_id && request.owner_host_id == Some(host_id)
                })
            })
            .unwrap_or(false);
        if !owned {
            // A stale answer (wrong workstation, or a prompt that already timed
            // out) is dropped here, so wipe it: this is the most reachable of the
            // paths where a one-time code or password is discarded without being
            // handed to the SSH layer.
            let mut responses = responses;
            wipe_responses(&mut responses);
            return false;
        }
        self.answer(request_id, responses)
    }

    /// Wake pending authentication immediately before a requested disconnect.
    pub fn cancel_for_owner(&self, host_id: HostId) {
        let ids = self
            .pending_for_owner(host_id)
            .into_iter()
            .map(|request| request.request_id)
            .collect::<Vec<_>>();
        for id in ids {
            // Dropping a pending sender makes the authenticator return Cancelled.
            if let Ok(mut guard) = self.requests.lock() {
                guard.remove(&id);
            }
            if let Ok(mut guard) = self.pending.lock() {
                guard.retain(|request| request.request_id != id);
            }
        }
    }

    pub fn answer(&self, request_id: u64, responses: Vec<String>) -> bool {
        let sender = self
            .requests
            .lock()
            .ok()
            .and_then(|mut guard| guard.remove(&request_id));
        let (applied, _wiped) = deliver_answer(sender, responses);
        if applied {
            if let Ok(mut guard) = self.pending.lock() {
                guard.retain(|request| request.request_id != request_id);
            }
        }
        applied
    }

    pub async fn prompt(
        &self,
        name: String,
        instructions: String,
        prompts: Vec<KeyboardPrompt>,
    ) -> Result<Vec<String>, SshError> {
        self.prompt_for_connection(None, 0, name, instructions, prompts)
            .await
    }

    pub async fn prompt_for_connection(
        &self,
        owner_host_id: Option<HostId>,
        connection_generation: u64,
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
        let _cleanup = PendingKeyboardGuard {
            broker: self,
            request_id,
        };
        self.pending
            .lock()
            .map_err(|_| SshError::Cancelled)?
            .push_back(KeyboardInteractiveRequest {
                request_id,
                owner_host_id,
                connection_generation,
                name,
                instructions,
                prompts,
            });
        match tokio::time::timeout(self.timeout, receiver).await {
            Ok(Ok(responses)) => Ok(responses),
            Ok(Err(_)) => Err(SshError::Cancelled),
            Err(_) => Err(SshError::Timeout),
        }
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
    async fn prompts_are_not_stolen_across_workstations_and_can_be_cancelled() {
        let broker = Arc::new(KeyboardInteractiveBroker::new(Duration::from_secs(3)));
        let owner_a = HostId::new();
        let owner_b = HostId::new();
        let spawn = |owner| {
            let broker = Arc::clone(&broker);
            tokio::spawn(async move {
                broker
                    .prompt_for_connection(
                        Some(owner),
                        7,
                        "OTP".into(),
                        "Enter verification code".into(),
                        vec![KeyboardPrompt {
                            prompt: "Code".into(),
                            echo: false,
                        }],
                    )
                    .await
            })
        };
        let a = spawn(owner_a);
        let b = spawn(owner_b);
        let (id_a, id_b) = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let a = broker.pending_for_owner(owner_a);
                let b = broker.pending_for_owner(owner_b);
                if let (Some(a), Some(b)) = (a.first(), b.first()) {
                    break (a.request_id, b.request_id);
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert_ne!(id_a, id_b);
        assert!(!broker.answer_for_owner(owner_a, id_b, vec!["bad".into()]));
        broker.cancel_for_owner(owner_a);
        assert_eq!(
            a.await.unwrap_or_else(|error| unreachable!("{error}")),
            Err(SshError::Cancelled)
        );
        assert!(broker.pending_for_owner(owner_a).is_empty());
        assert_eq!(broker.pending_for_owner(owner_b).len(), 1);
        assert!(broker.answer_for_owner(owner_b, id_b, vec!["123456".into()]));
        assert_eq!(
            b.await.unwrap_or_else(|error| unreachable!("{error}")),
            Ok(vec!["123456".into()])
        );
        assert!(broker.pending_for_owner(owner_b).is_empty());
    }

    #[tokio::test]
    async fn aborting_keyboard_interactive_task_forgets_pending_prompt() {
        let broker = Arc::new(KeyboardInteractiveBroker::new(Duration::from_secs(5)));
        let owner = HostId::new();
        let task = {
            let broker = Arc::clone(&broker);
            tokio::spawn(async move {
                broker
                    .prompt_for_connection(
                        Some(owner),
                        5,
                        "OTP".into(),
                        "Enter a code".into(),
                        vec![KeyboardPrompt {
                            prompt: "Code: ".into(),
                            echo: false,
                        }],
                    )
                    .await
            })
        };
        let request_id = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(request) = broker.pending_for_owner(owner).first() {
                    break request.request_id;
                }
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
        })
        .await
        .unwrap_or_else(|error| unreachable!("{error}"));
        task.abort();
        assert!(task.await.is_err());
        assert!(broker.pending_for_owner(owner).is_empty());
        assert!(!broker.answer_for_owner(owner, request_id, vec!["late".into()]));
    }

    #[tokio::test]
    async fn timed_out_otp_does_not_leave_ghost_prompt_or_pending_sender() {
        let broker = Arc::new(KeyboardInteractiveBroker::new(Duration::from_millis(180)));
        let owner = HostId::new();
        let task = {
            let broker = Arc::clone(&broker);
            tokio::spawn(async move {
                broker
                    .prompt_for_connection(
                        Some(owner),
                        3,
                        "OTP".into(),
                        "Enter a code".into(),
                        vec![KeyboardPrompt {
                            prompt: "Code: ".into(),
                            echo: false,
                        }],
                    )
                    .await
            })
        };
        let pending_id = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(request) = broker.pending_for_owner(owner).first() {
                    break request.request_id;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(
            task.await.unwrap_or_else(|error| unreachable!("{error}")),
            Err(SshError::Timeout)
        );
        assert!(broker.pending_for_owner(owner).is_empty());
        assert!(!broker.answer_for_owner(owner, pending_id, vec!["late".into()]));
    }

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

    /// A delivered answer must reach the authenticator byte for byte: the wipe
    /// must never touch the copy that is actually used.
    #[tokio::test]
    async fn a_delivered_answer_reaches_the_authenticator_unchanged() {
        let (sender, receiver) = oneshot::channel();
        let (applied, remainder) =
            deliver_answer(Some(sender), vec!["123456".into(), "otp".into()]);
        assert!(applied);
        assert!(remainder.is_empty());
        assert_eq!(
            receiver
                .await
                .unwrap_or_else(|error| unreachable!("{error}")),
            vec!["123456".to_string(), "otp".to_string()]
        );
    }

    /// Both ways an answer can fail to reach a waiting prompt are observable
    /// here: there is no answer at all, or the prompt was already gone.
    #[test]
    fn undeliverable_answers_are_wiped() {
        let answer = || vec!["123456".to_string(), "password".to_string()];
        let (applied, remainder) = deliver_answer(None, answer());
        assert!(!applied);
        assert!(
            remainder.is_empty(),
            "an answer with no waiting prompt must be wiped"
        );

        let (sender, receiver) = oneshot::channel::<Vec<String>>();
        drop(receiver);
        let (applied, remainder) = deliver_answer(Some(sender), answer());
        assert!(!applied);
        assert!(
            remainder.is_empty(),
            "an answer whose prompt is gone must be wiped"
        );
    }

    /// Guards the helper itself, so removing the wipe is a test failure rather
    /// than a silent regression.
    #[test]
    fn wiping_clears_every_response() {
        let mut responses = vec!["123456".to_string(), "hunter2".to_string()];
        wipe_responses(&mut responses);
        assert!(responses.is_empty());
    }

    /// The stale-answer path from the workstation-isolation test, checked for
    /// the property that matters: the rejected answer never reaches the prompt.
    #[tokio::test]
    async fn a_stale_answer_neither_reaches_its_prompt_nor_panics() {
        let broker = Arc::new(KeyboardInteractiveBroker::new(Duration::from_secs(5)));
        let owner = HostId::new();
        let other = HostId::new();
        let task = {
            let broker = Arc::clone(&broker);
            tokio::spawn(async move {
                broker
                    .prompt_for_connection(
                        Some(owner),
                        4,
                        "OTP".into(),
                        "Enter a code".into(),
                        vec![KeyboardPrompt {
                            prompt: "Code: ".into(),
                            echo: false,
                        }],
                    )
                    .await
            })
        };
        let request_id = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(request) = broker.pending_for_owner(owner).first() {
                    break request.request_id;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap_or_else(|error| unreachable!("{error}"));

        assert!(!broker.answer_for_owner(other, request_id, vec!["999999".into()]));
        assert!(!broker.answer(0, vec!["000000".into()]));
        assert!(broker.answer_for_owner(owner, request_id, vec!["123456".into()]));
        assert_eq!(
            task.await.unwrap_or_else(|error| unreachable!("{error}")),
            Ok(vec!["123456".into()])
        );
    }
}
