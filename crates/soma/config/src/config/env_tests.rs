use super::*;

const ABSENT: &str = "SOMA_TEST_ENV_HELPER_MUST_REMAIN_UNSET_2E46972E";

#[test]
fn absent_values_preserve_existing_targets() {
    let mut string_value = "existing".to_owned();
    let mut optional = Some("existing".to_owned());
    let mut number = 7_u64;
    let mut optional_number = Some(8_u64);
    let mut boolean_value = true;
    let mut list_value = vec!["existing".to_owned()];

    string(ABSENT, &mut string_value);
    optional_string(ABSENT, &mut optional);
    parse(ABSENT, &mut number).expect("absent parse");
    optional_parse(ABSENT, &mut optional_number).expect("absent optional parse");
    boolean(ABSENT, &mut boolean_value).expect("absent bool");
    list(ABSENT, &mut list_value);

    assert_eq!(string_value, "existing");
    assert_eq!(optional.as_deref(), Some("existing"));
    assert_eq!(number, 7);
    assert_eq!(optional_number, Some(8));
    assert!(boolean_value);
    assert_eq!(list_value, ["existing"]);
}
