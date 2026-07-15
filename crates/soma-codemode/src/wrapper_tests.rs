use super::wrapper::{async_iife, code_mode_main_invoker, CODE_MODE_VALUE_CODEC_JS};

#[test]
fn wrapper_invokes_async_function_contract() {
    let body = code_mode_main_invoker("async () => 1");
    assert!(body.contains("const __codeModeMain"));
    assert!(body.contains("__somaEncodeResult"));
}

#[test]
fn codec_uses_soma_names() {
    assert!(CODE_MODE_VALUE_CODEC_JS.contains("__somaEncodeResult"));
    assert!(async_iife("return 1;").contains("return 1;"));
}
