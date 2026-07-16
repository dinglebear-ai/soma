pub fn generate_openapi_provider_js() -> &'static str {
    r#"
globalThis.openapi = {
  call: function (label, operationId, params) {
    if (typeof label !== "string" || typeof operationId !== "string") {
      throw new Error(JSON.stringify({ kind: "missing_param", message: "openapi.call(label, operationId, params) requires string label and operationId" }));
    }
    return callTool("openapi::" + label + "." + operationId, params == null ? {} : params);
  }
};
"#
}
