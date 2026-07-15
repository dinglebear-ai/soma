//! Runs `typify::TypeSpace::add_root_schema` against a candidate schema
//! inside `catch_unwind`, classifying the outcome so `bisect` can
//! distinguish "converts fine", "panics with the known typify-0.7.0
//! `merge.rs:427` shape", and "something else entirely" (a different panic,
//! or an ordinary `Result::Err`).

use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

#[derive(Debug, Clone)]
pub enum ProbeOutcome {
    /// typify converted the schema to Rust types without error.
    Success,
    /// Panicked with a message/location matching the known typify 0.7.0
    /// `merge.rs` "not yet implemented" failure mode this tool hunts for.
    TargetPanic {
        message: String,
        location: Option<String>,
    },
    /// Panicked, but not matching that specific signature - still worth
    /// surfacing to a human, just not what `bisect` is purpose-built to
    /// search for.
    OtherPanic {
        message: String,
        location: Option<String>,
    },
    /// typify returned an ordinary `Result::Err` - no panic, just a schema
    /// typify doesn't support for some other reason.
    TypifyError(String),
    /// The candidate JSON didn't even parse as a `schemars::schema::RootSchema`.
    InvalidRootSchema(String),
}

impl ProbeOutcome {
    pub fn reproduces_target(&self) -> bool {
        matches!(self, ProbeOutcome::TargetPanic { .. })
    }

    pub fn summary(&self) -> String {
        match self {
            ProbeOutcome::Success => "success".to_string(),
            ProbeOutcome::TargetPanic { message, location } => {
                format!("PANIC (target merge.rs shape) at {location:?}: {message}")
            }
            ProbeOutcome::OtherPanic { message, location } => {
                format!("panic (other, not the target shape) at {location:?}: {message}")
            }
            ProbeOutcome::TypifyError(e) => format!("typify error (no panic): {e}"),
            ProbeOutcome::InvalidRootSchema(e) => format!("invalid RootSchema JSON: {e}"),
        }
    }
}

/// Process-wide lock serializing every `probe()` call. `std::panic::set_hook`
/// / `take_hook` are global, process-wide state, not thread-local - without
/// this, two `probe()` calls running on different threads at once (e.g.
/// `cargo test`'s default parallel test execution) can install/restore each
/// other's hooks out of order and corrupt the captured panic location. The
/// bisection driver in `bisect.rs` only ever calls `probe()` sequentially on
/// one thread anyway, so this lock is uncontended in normal use; it exists
/// purely to make `probe()` itself safe to call concurrently.
fn probe_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Runs typify against `schema` (a full combined-schema JSON document, the
/// same shape `build.rs` feeds it) inside `catch_unwind`, classifying the
/// result. Temporarily installs a panic hook to capture the panic location
/// (the payload alone doesn't carry it), always restoring the previous hook
/// before returning - there is no early return between install and restore,
/// so this stays paired even on the panicking path. Serialized process-wide
/// via `probe_lock` - see its docs.
pub fn probe(schema: &Value) -> ProbeOutcome {
    let root: schemars::schema::RootSchema = match serde_json::from_value(schema.clone()) {
        Ok(r) => r,
        Err(e) => return ProbeOutcome::InvalidRootSchema(e.to_string()),
    };

    let _guard = probe_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let location: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let location_for_hook = Arc::clone(&location);
    // `probe_lock` only serializes against *other `probe()` calls* - it does
    // nothing to stop some unrelated panic on a different thread (e.g. an
    // assertion failure in an unrelated test running concurrently under
    // `cargo test`'s default parallel execution) from also invoking this
    // hook, since `panic::set_hook` is process-global, not per-thread.
    // Filtering on the calling thread's id keeps such a stray panic from
    // clobbering the location this specific `probe()` call is trying to
    // capture.
    let calling_thread = std::thread::current().id();
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() != calling_thread {
            return;
        }
        if let Some(loc) = info.location() {
            *location_for_hook.lock().unwrap() =
                Some(format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));
        }
    }));

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let settings = typify::TypeSpaceSettings::default();
        let mut type_space = typify::TypeSpace::new(&settings);
        type_space.add_root_schema(root)
    }));

    panic::set_hook(previous_hook);
    let captured_location = location.lock().unwrap().clone();

    match result {
        Ok(Ok(_)) => ProbeOutcome::Success,
        Ok(Err(e)) => ProbeOutcome::TypifyError(e.to_string()),
        Err(payload) => {
            // `&*payload`, not `&payload`: `payload: Box<dyn Any + Send>` is
            // itself a `'static` type, so it satisfies `Any`'s blanket impl
            // too - `&payload` would coerce to a trait object representing
            // the *Box's own* type identity, not the panic value it wraps,
            // making every downcast_ref below silently fail regardless of
            // the real payload type. Deref first to reach the actual
            // panic value before widening it to `&dyn Any + Send`.
            let message = panic_message(&*payload);
            // Requires BOTH signals, not just the message: a bare
            // "not yet implemented" match alone would misclassify any
            // unrelated `todo!()` panic anywhere in typify's call graph as
            // this specific known bug. The captured location narrows it to
            // the exact source file the target panic actually comes from.
            let is_target = message.contains("not yet implemented")
                && captured_location
                    .as_deref()
                    .is_some_and(|l| l.contains("merge.rs"));
            if is_target {
                ProbeOutcome::TargetPanic {
                    message,
                    location: captured_location,
                }
            } else {
                ProbeOutcome::OtherPanic {
                    message,
                    location: captured_location,
                }
            }
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

#[cfg(test)]
#[path = "typify_probe_tests.rs"]
mod tests;
