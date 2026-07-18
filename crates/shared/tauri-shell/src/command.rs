//! Generic result/error helpers for Tauri command handlers.
//!
//! Tauri commands surface errors to the frontend as plain `String`s. Every
//! handler in a `src-tauri` package ends up writing the same
//! `.map_err(|err| err.to_string())` boilerplate; this module centralizes it.

/// The result shape every `#[tauri::command]` handler in a Soma-derived
/// desktop app should return: `Ok(T)` on success, or a display-formatted
/// error string the frontend can show directly.
pub type CommandResult<T> = Result<T, String>;

/// Extension trait converting any `Result<T, E: Display>` into a
/// [`CommandResult<T>`] without a manual `.map_err(|err| err.to_string())`
/// at every call site.
pub trait TauriResultExt<T> {
    fn command_result(self) -> CommandResult<T>;
}

impl<T, E: std::fmt::Display> TauriResultExt<T> for Result<T, E> {
    fn command_result(self) -> CommandResult<T> {
        self.map_err(|err| err.to_string())
    }
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
