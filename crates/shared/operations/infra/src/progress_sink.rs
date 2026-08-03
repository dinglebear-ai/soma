use std::fmt::Display;

use soma_ops::{ProgressEvent, ProgressSink};

/// Object-safe adapter for canonical operation progress delivery.
pub trait MutationProgressReporter: Send + Sync {
    /// Delivers one canonical progress event without changing execution truth.
    fn report(&self, event: &ProgressEvent) -> Result<(), String>;
}

impl<T> MutationProgressReporter for T
where
    T: ProgressSink,
    T::Error: Display,
{
    fn report(&self, event: &ProgressEvent) -> Result<(), String> {
        ProgressSink::report(self, event).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[path = "progress_sink_tests.rs"]
mod tests;
