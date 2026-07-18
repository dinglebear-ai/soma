//! `soma-tauri-shell` — reusable, product-neutral Tauri desktop shell mechanics.
//!
//! This crate owns generic Tauri window/tray/shortcut/blur/persistence and
//! command-result plumbing shared across Tauri-based desktop apps built on
//! Soma. It deliberately owns *mechanics*, not policy: it has no knowledge of
//! any particular product's settings shape, environment variable names,
//! OAuth policy, or UI. Callers (an app-local `src-tauri` package) supply the
//! product-specific labels, defaults, and business logic and call into these
//! helpers to do the actual Tauri API work.
//!
//! See `soma-architecture-refactor-plan-v3.md` section 3.14 for the crate's
//! ownership boundary.

pub mod app;
pub mod blur;
pub mod command;
pub mod persistence;
pub mod shortcut;
pub mod tray;
pub mod window;

pub use command::{CommandResult, TauriResultExt};
