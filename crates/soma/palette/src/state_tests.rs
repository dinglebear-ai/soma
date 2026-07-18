use super::next_request_id;

#[test]
fn generated_palette_request_ids_are_valid_and_unique() {
    let first = next_request_id();
    let second = next_request_id();

    assert!(first.as_str().starts_with("palette-"));
    assert_ne!(first.as_str(), second.as_str());
}
