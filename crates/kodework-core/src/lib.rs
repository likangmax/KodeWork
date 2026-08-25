#![forbid(unsafe_code)]

pub mod session;
pub mod tunnel;

use kodework_domain::{
    classify_danger, Action, ActionMode, ConfirmationPolicy, ConnectionState, DangerLevel,
    RunStatus,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid connection state transition")]
    InvalidConnectionTransition,
    #[error("dangerous action requires confirmation")]
    ConfirmationRequired,
    #[error("interactive actions must run through a PTY")]
    InteractiveRequiresPty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionGeneration(u64);

impl ConnectionGeneration {
    #[must_use]
    pub const fn initial() -> Self {
        Self(0)
    }
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionSnapshot {
    pub state: ConnectionState,
    pub generation: ConnectionGeneration,
}

impl Default for ConnectionSnapshot {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            generation: ConnectionGeneration::initial(),
        }
    }
}

impl ConnectionSnapshot {
    pub fn transition(&mut self, next: ConnectionState) -> Result<(), CoreError> {
        if !kodework_domain::connection_transition(self.state, next) {
            return Err(CoreError::InvalidConnectionTransition);
        }
        if next == ConnectionState::Reconnecting {
            self.generation = self.generation.next();
        }
        self.state = next;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionPlan {
    pub mode: ActionMode,
    pub requires_confirmation: bool,
    pub initial_status: RunStatus,
}

/// Returns the server-authoritative confirmation decision for an action.
/// Danger is always recomputed from the command so stale renderer/database
/// metadata cannot downgrade a Review or Dangerous command to Safe.
#[must_use]
pub fn action_requires_confirmation(action: &Action) -> bool {
    let danger = classify_danger(&action.command);
    match action.confirmation {
        ConfirmationPolicy::Always => true,
        ConfirmationPolicy::OnDangerous | ConfirmationPolicy::Never => danger != DangerLevel::Safe,
    }
}

pub fn plan_action(action: &Action, confirmed: bool) -> Result<ActionPlan, CoreError> {
    let requires_confirmation = action_requires_confirmation(action);
    if requires_confirmation && !confirmed {
        return Err(CoreError::ConfirmationRequired);
    }
    if action.mode == ActionMode::Interactive && action.timeout_ms == Some(0) {
        return Err(CoreError::InteractiveRequiresPty);
    }
    Ok(ActionPlan {
        mode: action.mode,
        requires_confirmation,
        initial_status: RunStatus::Queued,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodework_domain::{ActionId, ConfirmationPolicy, ProjectId};

    fn action(mode: ActionMode, danger: DangerLevel) -> Action {
        Action {
            id: ActionId::new(),
            project_id: ProjectId::new(),
            name: "test".into(),
            command: match danger {
                DangerLevel::Safe => "echo ok",
                DangerLevel::Review => "python -c 'print(1)'",
                DangerLevel::Dangerous => "rm -rf /tmp/work",
            }
            .into(),
            mode,
            cwd: None,
            timeout_ms: None,
            danger_level: danger,
            confirmation: ConfirmationPolicy::OnDangerous,
            env: Default::default(),
        }
    }

    #[test]
    fn generation_changes_when_reconnecting() {
        let mut snapshot = ConnectionSnapshot::default();
        assert!(snapshot
            .transition(ConnectionState::ResolvingAddress)
            .is_ok());
        assert!(snapshot.transition(ConnectionState::Connecting).is_ok());
        assert!(snapshot
            .transition(ConnectionState::VerifyingHostKey)
            .is_ok());
        assert!(snapshot.transition(ConnectionState::Authenticating).is_ok());
        assert!(snapshot.transition(ConnectionState::Ready).is_ok());
        let before = snapshot.generation;
        assert!(snapshot.transition(ConnectionState::Reconnecting).is_ok());
        assert_ne!(before, snapshot.generation);
    }

    #[test]
    fn dangerous_action_requires_confirmation() {
        assert_eq!(
            plan_action(&action(ActionMode::Quick, DangerLevel::Dangerous), false),
            Err(CoreError::ConfirmationRequired)
        );
        assert!(plan_action(&action(ActionMode::Quick, DangerLevel::Dangerous), true).is_ok());
    }

    #[test]
    fn review_action_requires_confirmation_even_with_never_policy() {
        let mut value = action(ActionMode::Quick, DangerLevel::Review);
        value.confirmation = ConfirmationPolicy::Never;
        assert_eq!(
            plan_action(&value, false),
            Err(CoreError::ConfirmationRequired)
        );
        assert!(plan_action(&value, true).is_ok());
    }

    #[test]
    fn always_confirmation_cannot_be_bypassed() {
        let mut value = action(ActionMode::Quick, DangerLevel::Safe);
        value.confirmation = ConfirmationPolicy::Always;
        assert_eq!(
            plan_action(&value, false),
            Err(CoreError::ConfirmationRequired)
        );
        assert!(plan_action(&value, true).is_ok());
    }
}
