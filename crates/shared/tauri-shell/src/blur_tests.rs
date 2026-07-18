use super::{should_hide_on_blur, BlurDismissState};

#[test]
fn default_state_is_enabled() {
    let state = BlurDismissState::default();
    assert!(state.enabled());
}

#[test]
fn set_toggles_enabled() {
    let state = BlurDismissState::new(true);
    state.set(false);
    assert!(!state.enabled());
    state.set(true);
    assert!(state.enabled());
}

#[test]
fn hides_only_when_both_gate_and_pref_allow_it() {
    assert!(should_hide_on_blur(true, true));
    assert!(!should_hide_on_blur(false, true));
    assert!(!should_hide_on_blur(true, false));
    assert!(!should_hide_on_blur(false, false));
}
