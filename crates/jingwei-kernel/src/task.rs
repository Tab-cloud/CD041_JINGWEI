//! Deterministic durable-task terminal/cancellation state machine.

use serde::{Deserialize, Serialize};

/// Durable task state. Progress events never replace one of these states.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Pending,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
    NeedsReconciliation,
}

/// Validated task state wrapper.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskMachine {
    state: TaskState,
}

impl Default for TaskMachine {
    fn default() -> Self {
        Self {
            state: TaskState::Pending,
        }
    }
}

impl TaskMachine {
    pub const fn state(&self) -> TaskState {
        self.state
    }

    pub fn transition(&mut self, next: TaskState) -> Result<(), TaskTransitionError> {
        let allowed = matches!(
            (self.state, next),
            (
                TaskState::Pending,
                TaskState::Running | TaskState::Cancelled
            ) | (
                TaskState::Running,
                TaskState::CancelRequested
                    | TaskState::Succeeded
                    | TaskState::Failed
                    | TaskState::NeedsReconciliation
            ) | (
                TaskState::CancelRequested,
                TaskState::Cancelled | TaskState::Failed | TaskState::NeedsReconciliation
            )
        );
        if !allowed {
            return Err(TaskTransitionError {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

/// Illegal task state transition.
#[derive(Clone, Debug, thiserror::Error)]
#[error("illegal task transition from {from:?} to {to:?}")]
pub struct TaskTransitionError {
    pub from: TaskState,
    pub to: TaskState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_requires_confirmed_terminal_state() {
        let mut task = TaskMachine::default();
        task.transition(TaskState::Running).unwrap();
        task.transition(TaskState::CancelRequested).unwrap();
        assert!(!matches!(task.state(), TaskState::Cancelled));
        task.transition(TaskState::Cancelled).unwrap();
        assert_eq!(task.state(), TaskState::Cancelled);
        assert!(task.transition(TaskState::Succeeded).is_err());
    }
}
