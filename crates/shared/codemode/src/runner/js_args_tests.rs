use super::js_args::javy_type_error;

#[test]
fn js_arg_helpers_are_available_to_runtime() {
    let _helper: fn(crate::javy::quickjs::Ctx<'_>, &str) -> crate::javy::quickjs::Error =
        |cx, message| javy_type_error(cx, message);
}
