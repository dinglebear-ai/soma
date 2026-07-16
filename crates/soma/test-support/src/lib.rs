// Test helper extraction point. Root crate keeps compatibility helpers for now.

pub mod tracing_capture;

pub use tracing_capture::{tracing_test_lock, SharedBuf, SharedWriter};
