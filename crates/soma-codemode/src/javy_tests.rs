use super::javy::quickjs;

#[test]
fn exposes_quickjs_runtime_without_external_javy_crate() {
    let runtime = quickjs::Runtime::new().expect("runtime");
    let context = quickjs::Context::full(&runtime).expect("context");

    context.with(|cx| {
        let value: i32 = cx.eval("1 + 2").expect("eval");
        assert_eq!(value, 3);
    });
}
