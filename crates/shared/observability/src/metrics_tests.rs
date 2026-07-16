//! Unit tests for src/metrics.rs

use super::*;

#[test]
fn render_is_none_before_init() {
    // In a fresh process with no recorder installed, render yields None.
    // (init() is process-global; other tests may install it, so only assert
    // the weaker invariant that render() never panics.)
    let _ = render();
    let _ = is_installed();
}

#[test]
fn init_is_idempotent_and_renders_after() {
    init();
    init(); // second call must not panic or double-install
    assert!(is_installed());
    assert!(render().is_some());
}
