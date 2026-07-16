use serde_json::Value;

pub(super) fn required_string_arg<'a>(
    cx: &crate::javy::quickjs::Ctx<'a>,
    args: &[crate::javy::quickjs::Value<'a>],
    index: usize,
    message: &str,
) -> crate::javy::quickjs::Result<String> {
    let value = args
        .get(index)
        .ok_or_else(|| javy_type_error(cx.clone(), message))?;
    let text = crate::javy::val_to_string(cx, value.clone())
        .map_err(|err| crate::javy::to_js_error(cx.clone(), err))?;
    if text.trim().is_empty() {
        Err(javy_type_error(cx.clone(), message))
    } else {
        Ok(text)
    }
}

pub(super) fn optional_string_arg<'a>(
    cx: &crate::javy::quickjs::Ctx<'a>,
    args: &[crate::javy::quickjs::Value<'a>],
    index: usize,
) -> crate::javy::quickjs::Result<Option<String>> {
    args.get(index)
        .filter(|value| !value.is_null() && !value.is_undefined())
        .map(|value| {
            crate::javy::val_to_string(cx, value.clone())
                .map_err(|err| crate::javy::to_js_error(cx.clone(), err))
        })
        .transpose()
}

pub(super) fn json_arg<'a>(
    cx: &crate::javy::quickjs::Ctx<'a>,
    args: &[crate::javy::quickjs::Value<'a>],
    index: usize,
    default: &str,
) -> crate::javy::quickjs::Result<Value> {
    let text = args
        .get(index)
        .map(|value| cx.json_stringify(value.clone()))
        .transpose()?
        .flatten()
        .map(|value| value.to_string())
        .transpose()?
        .unwrap_or_else(|| default.to_string());
    serde_json::from_str(&text).map_err(|err| {
        javy_type_error(
            cx.clone(),
            format!("argument must be JSON-serializable: {err}"),
        )
    })
}

pub(super) fn javy_type_error(
    cx: crate::javy::quickjs::Ctx<'_>,
    message: impl Into<String>,
) -> crate::javy::quickjs::Error {
    crate::javy::to_js_error(cx, anyhow::anyhow!(message.into()))
}
