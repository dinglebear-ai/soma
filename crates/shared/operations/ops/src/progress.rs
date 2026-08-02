use std::convert::Infallible;

use serde::{Deserialize, Serialize};

use crate::{OperationId, OperationName, Timestamp};

const MAX_PHASE_CHARS: usize = 128;
const MAX_UNIT_CHARS: usize = 64;
const MAX_MESSAGE_CHARS: usize = 1_024;

/// One bounded monotonic progress update for an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProgressEvent {
    operation_id: OperationId,
    operation: OperationName,
    sequence: u64,
    occurred_at: Timestamp,
    phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl ProgressEvent {
    /// Creates a progress event without a known quantity.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        sequence: u64,
        occurred_at: Timestamp,
        phase: impl Into<String>,
    ) -> Result<Self, ProgressError> {
        if sequence == 0 {
            return Err(ProgressError::ZeroSequence);
        }
        let phase = phase.into();
        validate_text("phase", &phase, MAX_PHASE_CHARS)?;
        Ok(Self {
            operation_id,
            operation,
            sequence,
            occurred_at,
            phase,
            current: None,
            total: None,
            unit: None,
            message: None,
        })
    }

    /// Adds a current quantity, optional total, and optional unit.
    pub fn with_amount(
        mut self,
        current: u64,
        total: Option<u64>,
        unit: Option<impl Into<String>>,
    ) -> Result<Self, ProgressError> {
        if total.is_some_and(|total| total == 0 || current > total) {
            return Err(ProgressError::InvalidAmount { current, total });
        }
        let unit = unit.map(Into::into);
        if let Some(unit) = &unit {
            validate_text("unit", unit, MAX_UNIT_CHARS)?;
        }
        self.current = Some(current);
        self.total = total;
        self.unit = unit;
        Ok(self)
    }

    /// Adds a bounded human-readable progress message.
    pub fn with_message(mut self, message: impl Into<String>) -> Result<Self, ProgressError> {
        let message = message.into();
        validate_text("message", &message, MAX_MESSAGE_CHARS)?;
        self.message = Some(message);
        Ok(self)
    }

    /// Returns the operation identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the canonical operation name.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }

    /// Returns the monotonic, one-based event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the event timestamp.
    #[must_use]
    pub const fn occurred_at(&self) -> Timestamp {
        self.occurred_at
    }

    /// Returns the current operation phase.
    #[must_use]
    pub fn phase(&self) -> &str {
        &self.phase
    }

    /// Returns the current quantity when known.
    #[must_use]
    pub const fn current(&self) -> Option<u64> {
        self.current
    }

    /// Returns the total quantity when known.
    #[must_use]
    pub const fn total(&self) -> Option<u64> {
        self.total
    }

    /// Returns the quantity unit when present.
    #[must_use]
    pub fn unit(&self) -> Option<&str> {
        self.unit.as_deref()
    }

    /// Returns the bounded progress message when present.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// Consumer of operation progress events.
pub trait ProgressSink: Send + Sync {
    /// Sink-specific error.
    type Error;

    /// Delivers one progress event.
    fn report(&self, event: &ProgressEvent) -> Result<(), Self::Error>;
}

/// Progress sink that intentionally discards events.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopProgressSink;

impl ProgressSink for NoopProgressSink {
    type Error = Infallible;

    fn report(&self, _event: &ProgressEvent) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Invalid progress event.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProgressError {
    /// Sequence numbers are one-based.
    #[error("progress sequence must be greater than zero")]
    ZeroSequence,
    /// Current progress exceeded total or total was zero.
    #[error("invalid progress amount: current={current}, total={total:?}")]
    InvalidAmount {
        /// Current quantity.
        current: u64,
        /// Total quantity when known.
        total: Option<u64>,
    },
    /// Text was empty, oversized, or contained control characters.
    #[error("invalid progress {field}: expected 1..={max_chars} non-control characters")]
    InvalidText {
        /// Progress field.
        field: &'static str,
        /// Maximum accepted character count.
        max_chars: usize,
    },
}

fn validate_text(field: &'static str, value: &str, max_chars: usize) -> Result<(), ProgressError> {
    let chars = value.chars().count();
    if chars == 0 || chars > max_chars || value.chars().any(char::is_control) {
        return Err(ProgressError::InvalidText { field, max_chars });
    }
    Ok(())
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
