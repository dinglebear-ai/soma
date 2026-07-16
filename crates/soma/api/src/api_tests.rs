use serde_json::json;

use super::{optional_name_params, rest_params};
use soma_contracts::actions::SomaAction;

#[test]
fn rest_dto_translation_stays_in_the_http_adapter() {
    assert_eq!(optional_name_params(None), json!({}));
    assert_eq!(
        rest_params(&SomaAction::Echo {
            message: "hello".to_owned(),
        }),
        json!({"message": "hello"})
    );
}
