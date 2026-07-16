use crate::protocol::{ServerNotification, TurnError, TurnStatus};
use crate::Event;

#[derive(Clone, Debug, Default)]
pub struct EventCollector {
    thread_id: Option<String>,
    turn_id: Option<String>,
    agent_message: String,
    latest_diff: Option<String>,
    completed: bool,
    terminal_status: Option<TurnStatus>,
    errors: Vec<TurnError>,
}

impl EventCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_turn(thread_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self {
            thread_id: Some(thread_id.into()),
            turn_id: Some(turn_id.into()),
            ..Self::default()
        }
    }

    pub fn observe(&mut self, event: &Event) {
        if let Event::Notification(notification) = event {
            self.observe_notification(notification);
        }
    }

    pub fn observe_notification(&mut self, notification: &ServerNotification) {
        match notification {
            ServerNotification::ItemAgentMessageDelta(delta)
                if self.matches_turn(&delta.thread_id, &delta.turn_id) =>
            {
                self.agent_message.push_str(&delta.delta);
            }
            ServerNotification::TurnDiffUpdated(diff)
                if self.matches_turn(&diff.thread_id, &diff.turn_id) =>
            {
                self.latest_diff = Some(diff.diff.clone());
            }
            ServerNotification::TurnCompleted(completed)
                if self.matches_turn(&completed.thread_id, &completed.turn.id) =>
            {
                self.terminal_status = Some(completed.turn.status);
                self.completed = matches!(
                    completed.turn.status,
                    TurnStatus::Completed | TurnStatus::Interrupted | TurnStatus::Failed
                );
                if let Some(error) = &completed.turn.error {
                    self.errors.push(error.clone());
                }
            }
            ServerNotification::Error(error)
                if self.matches_turn(&error.thread_id, &error.turn_id) =>
            {
                self.errors.push(error.error.clone());
            }
            _ => {}
        }
    }

    pub fn agent_message(&self) -> &str {
        &self.agent_message
    }

    pub fn latest_diff(&self) -> Option<&str> {
        self.latest_diff.as_deref()
    }

    pub fn is_complete(&self) -> bool {
        self.completed
    }

    pub fn terminal_status(&self) -> Option<&TurnStatus> {
        self.terminal_status.as_ref()
    }

    pub fn errors(&self) -> &[TurnError] {
        &self.errors
    }

    fn matches_turn(&self, thread_id: &str, turn_id: &str) -> bool {
        self.thread_id.as_deref().is_none_or(|id| id == thread_id)
            && self.turn_id.as_deref().is_none_or(|id| id == turn_id)
    }
}
