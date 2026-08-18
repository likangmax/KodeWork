#![forbid(unsafe_code)]

pub mod clipboard;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StartupMode {
    Disabled,
    StartMinimized,
    StartVisible,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloseBehavior {
    Exit,
    MinimizeToTray,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecyclePolicy {
    pub startup: StartupMode,
    pub close_behavior: CloseBehavior,
    pub restore_sessions: bool,
    pub reconnect_attempts: u8,
    pub reconnect_backoff_ms: u32,
}

impl Default for LifecyclePolicy {
    fn default() -> Self {
        Self {
            startup: StartupMode::StartMinimized,
            close_behavior: CloseBehavior::MinimizeToTray,
            restore_sessions: true,
            reconnect_attempts: 5,
            reconnect_backoff_ms: 500,
        }
    }
}

impl LifecyclePolicy {
    #[must_use]
    pub fn backoff_for_attempt(&self, attempt: u8) -> u32 {
        let capped = attempt.min(self.reconnect_attempts);
        self.reconnect_backoff_ms
            .saturating_mul(2_u32.saturating_pow(u32::from(capped)))
            .min(30_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_tray_and_restore() {
        let policy = LifecyclePolicy::default();
        assert_eq!(policy.close_behavior, CloseBehavior::MinimizeToTray);
        assert!(policy.restore_sessions);
    }

    #[test]
    fn backoff_is_bounded() {
        let policy = LifecyclePolicy::default();
        assert_eq!(policy.backoff_for_attempt(0), 500);
        assert!(policy.backoff_for_attempt(255) <= 30_000);
    }
}
