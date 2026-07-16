//! In-process tracing capture for tests.
//!
//! Install a temporary subscriber that writes events into an in-memory buffer,
//! run code that emits tracing events, then assert on the captured output. Use
//! this to lock down a structured-logging contract (e.g. that every action
//! dispatch logs `surface`, `action`, and `outcome`) so it cannot silently
//! regress.
//!
//! Tracing's default subscriber is thread-local, so concurrent capturing tests
//! would clobber each other. [`tracing_test_lock`] serializes them: hold the
//! guard for the whole capture. Pair with `#[tokio::test(flavor =
//! "current_thread")]` so awaits stay on the thread whose default you set.

use std::io::Write;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// Process-wide lock serializing tracing-capture tests. Hold the returned guard
/// for the entire capture so another test's subscriber cannot interleave.
pub fn tracing_test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A shared, in-memory buffer a tracing subscriber can write into.
#[derive(Clone, Default)]
pub struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    pub fn new() -> Self {
        Self::default()
    }

    /// Captured output decoded as UTF-8 (lossy).
    pub fn contents(&self) -> String {
        let bytes = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// A `MakeWriter` handle to pass to a `tracing_subscriber` fmt layer.
    pub fn writer(&self) -> SharedWriter {
        SharedWriter(self.0.clone())
    }
}

/// Write handle for a [`SharedBuf`]; implements `io::Write` and `MakeWriter`.
#[derive(Clone)]
pub struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
    type Writer = SharedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_emitted_events() {
        let _lock = tracing_test_lock();
        let buf = SharedBuf::new();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buf.writer())
            .with_ansi(false)
            .without_time()
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(surface = "test", "captured event");
        });
        let logs = buf.contents();
        assert!(logs.contains("captured event"), "logs were: {logs}");
        assert!(logs.contains("surface"), "logs were: {logs}");
    }
}
