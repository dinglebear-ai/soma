//! Small helpers shared by [`crate::service`] and [`crate::actions::internal`].

use serde_json::Value;

/// Truncates `value["data"]` (if it's an array) to `limit` items. A no-op
/// when `limit` is `None` or `value` has no `data` array.
pub(crate) fn truncate_data_array(value: &mut Value, limit: Option<usize>) {
    let Some(limit) = limit else {
        return;
    };
    if let Some(items) = value.get_mut("data").and_then(Value::as_array_mut) {
        items.truncate(limit);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn truncate_data_array_limits_when_given() {
        let mut value = json!({ "data": [1, 2, 3, 4] });

        truncate_data_array(&mut value, Some(2));

        assert_eq!(value, json!({ "data": [1, 2] }));
    }

    #[test]
    fn truncate_data_array_is_a_no_op_without_a_limit() {
        let mut value = json!({ "data": [1, 2, 3] });

        truncate_data_array(&mut value, None);

        assert_eq!(value, json!({ "data": [1, 2, 3] }));
    }

    #[test]
    fn truncate_data_array_ignores_values_without_a_data_array() {
        let mut value = json!({ "other": "field" });

        truncate_data_array(&mut value, Some(1));

        assert_eq!(value, json!({ "other": "field" }));
    }
}
