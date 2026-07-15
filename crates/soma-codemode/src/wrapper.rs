#![allow(dead_code)]

pub(crate) const CODE_MODE_MAIN_SHAPE_ERROR: &str =
    "codemode code must evaluate to an async arrow function: async () => { ... }";

pub(crate) const CODE_MODE_VALUE_CODEC_JS: &str = r#"
function __somaBase64FromBytes(bytes) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i];
    const b = i + 1 < bytes.length ? bytes[i + 1] : 0;
    const c = i + 2 < bytes.length ? bytes[i + 2] : 0;
    const triple = (a << 16) | (b << 8) | c;
    out += alphabet[(triple >> 18) & 63];
    out += alphabet[(triple >> 12) & 63];
    out += i + 1 < bytes.length ? alphabet[(triple >> 6) & 63] : "=";
    out += i + 2 < bytes.length ? alphabet[triple & 63] : "=";
  }
  return out;
}
function __somaBytesFromBase64(data) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let clean = String(data || "").replace(/=+$/, "");
  let buffer = 0;
  let bits = 0;
  const out = [];
  for (let i = 0; i < clean.length; i++) {
    const value = alphabet.indexOf(clean[i]);
    if (value < 0) continue;
    buffer = (buffer << 6) | value;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out.push((buffer >> bits) & 255);
    }
  }
  return new Uint8Array(out);
}
var __somaBinaryTypes = {
  Int8Array: typeof Int8Array !== "undefined" ? Int8Array : null,
  Uint8Array: typeof Uint8Array !== "undefined" ? Uint8Array : null,
  Uint8ClampedArray: typeof Uint8ClampedArray !== "undefined" ? Uint8ClampedArray : null,
  Int16Array: typeof Int16Array !== "undefined" ? Int16Array : null,
  Uint16Array: typeof Uint16Array !== "undefined" ? Uint16Array : null,
  Int32Array: typeof Int32Array !== "undefined" ? Int32Array : null,
  Uint32Array: typeof Uint32Array !== "undefined" ? Uint32Array : null,
  Float32Array: typeof Float32Array !== "undefined" ? Float32Array : null,
  Float64Array: typeof Float64Array !== "undefined" ? Float64Array : null,
  BigInt64Array: typeof BigInt64Array !== "undefined" ? BigInt64Array : null,
  BigUint64Array: typeof BigUint64Array !== "undefined" ? BigUint64Array : null,
  DataView: typeof DataView !== "undefined" ? DataView : null,
  ArrayBuffer: typeof ArrayBuffer !== "undefined" ? ArrayBuffer : null
};
function __somaEncodeResult(value) {
  if (value == null) return value;
  if (typeof ArrayBuffer !== "undefined" && value instanceof ArrayBuffer) {
    return { __somaBinary: "base64", type: "ArrayBuffer", data: __somaBase64FromBytes(new Uint8Array(value)) };
  }
  if (typeof ArrayBuffer !== "undefined" && ArrayBuffer.isView && ArrayBuffer.isView(value)) {
    return { __somaBinary: "base64", type: value.constructor && value.constructor.name || "TypedArray", data: __somaBase64FromBytes(new Uint8Array(value.buffer, value.byteOffset, value.byteLength)) };
  }
  if (Array.isArray(value)) return value.map(__somaEncodeResult);
  if (typeof value === "object") {
    if (typeof value.toJSON === "function") return __somaEncodeResult(value.toJSON());
    const out = {};
    for (const key of Object.keys(value)) out[key] = __somaEncodeResult(value[key]);
    return out;
  }
  return value;
}
function __somaDecodeResult(value) {
  if (value == null) return value;
  if (
    typeof value === "object" &&
    value.__somaBinary === "base64" &&
    typeof value.data === "string" &&
    typeof value.type === "string" &&
    Object.prototype.hasOwnProperty.call(__somaBinaryTypes, value.type) &&
    __somaBinaryTypes[value.type]
  ) {
    const bytes = __somaBytesFromBase64(value.data);
    if (value.type === "ArrayBuffer") {
      return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    }
    const Ctor = __somaBinaryTypes[value.type];
    if (value.type === "DataView") {
      return new DataView(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
    }
    if (value.type === "Uint8Array" || value.type === "Uint8ClampedArray") {
      return new Ctor(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    }
    return new Ctor(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
  }
  if (Array.isArray(value)) return value.map(__somaDecodeResult);
  if (typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value)) out[key] = __somaDecodeResult(value[key]);
    return out;
  }
  return value;
}
"#;

pub(crate) fn code_mode_main_invoker(code: &str) -> String {
    let mut body = String::new();
    body.push_str("  const __codeModeMain = (");
    body.push_str(code);
    body.push_str(");\n");
    body.push_str("  if (typeof __codeModeMain !== \"function\") {\n");
    body.push_str("    throw new TypeError(");
    body.push_str(
        &serde_json::to_string(CODE_MODE_MAIN_SHAPE_ERROR)
            .unwrap_or_else(|_| "\"codemode code must be an async arrow function\"".to_string()),
    );
    body.push_str(");\n");
    body.push_str("  }\n");
    body.push_str("  return __somaEncodeResult(await __codeModeMain());\n");
    body
}

pub(crate) fn async_iife(code: &str) -> String {
    format!("(async () => {{\n{code}\n}})()")
}

pub(crate) fn code_mode_runner_script(code: &str, proxy: &str) -> String {
    let invoker = code_mode_main_invoker(code);
    format!(
        r#"
globalThis.__somaPendingOperations = new Map();
globalThis.__somaSnippetStack = [];
globalThis.__somaSnippetResolveCount = 0;
globalThis.__somaSnippetResolvedBytes = 0;
globalThis.__somaSnippetMaxDepth = 8;
globalThis.__somaSnippetMaxResolves = 32;
globalThis.__somaSnippetMaxBytes = 262144;
{codec}
globalThis.callTool = (id, params = {{}}) => {{
  if (typeof id !== "string" || id.trim() === "") throw new TypeError("callTool id must be a non-empty string");
  if (params === null || typeof params !== "object" || Array.isArray(params)) throw new TypeError("callTool params must be a JSON object");
  return new Promise((resolve, reject) => {{
    const seq = globalThis.__somaEmitToolCall(id, __somaEncodeResult(params));
    globalThis.__somaPendingOperations.set(seq, {{ kind: "tool", resolve, reject }});
  }});
}};
globalThis.writeArtifact = (path, content, options = {{}}) => {{
  if (typeof path !== "string" || path.trim() === "") throw new TypeError("writeArtifact path must be a non-empty string");
  if (typeof content !== "string") throw new TypeError("writeArtifact content must be a string");
  const contentType = options && typeof options === "object" ? (options.contentType ?? null) : null;
  return new Promise((resolve, reject) => {{
    const seq = globalThis.__somaEmitArtifactWrite(path, content, contentType);
    globalThis.__somaPendingOperations.set(seq, {{ kind: "artifact", resolve, reject }});
  }});
}};
globalThis.__somaRunSnippet = (name, input = {{}}) => {{
  if (globalThis.__somaSnippetStack.indexOf(name) !== -1) return Promise.reject(new Error(JSON.stringify({{kind:"snippet_recursion_limit", message:"snippet recursion detected"}})));
  if (globalThis.__somaSnippetStack.length >= globalThis.__somaSnippetMaxDepth) return Promise.reject(new Error(JSON.stringify({{kind:"snippet_depth_exceeded", message:"snippet depth limit exceeded"}})));
  if (globalThis.__somaSnippetResolveCount >= globalThis.__somaSnippetMaxResolves) return Promise.reject(new Error(JSON.stringify({{kind:"snippet_resolve_limit", message:"snippet resolve limit exceeded"}})));
  globalThis.__somaSnippetResolveCount++;
  return new Promise((resolve, reject) => {{
    const seq = globalThis.__somaEmitSnippetResolve(String(name), __somaEncodeResult(input));
    globalThis.__somaPendingOperations.set(seq, {{ kind: "snippet", name: String(name), resolve, reject }});
  }});
}};
globalThis.__somaCodemodeStep = (name, fn) => {{
  if (typeof fn !== "function") return Promise.reject(new Error(JSON.stringify({{kind:"invalid_param", message:"codemode.step requires a function"}})));
  return new Promise((resolve, reject) => {{
    const seq = globalThis.__somaEmitStepBegin(String(name));
    globalThis.__somaPendingOperations.set(seq, {{ kind: "step_begin", name: String(name), fn, resolve, reject }});
  }});
}};
globalThis.__somaSettlePendingOperation = (message) => {{
  const input = JSON.parse(message);
  const pending = globalThis.__somaPendingOperations.get(input.seq);
  if (!pending) throw new Error("runner received a response for an unknown operation");
  globalThis.__somaPendingOperations.delete(input.seq);
  if (input.type === "tool_result") return pending.resolve(__somaDecodeResult(input.result));
  if (input.type === "tool_error") return pending.reject(new Error(JSON.stringify({{kind: input.kind, message: input.message}})));
  if (input.type === "snippet_resolved") {{
    if (pending.kind !== "snippet") throw new Error("runner received snippet code for a non-snippet operation");
    globalThis.__somaSnippetResolvedBytes += input.code.length;
    if (globalThis.__somaSnippetResolvedBytes > globalThis.__somaSnippetMaxBytes) return pending.reject(new Error(JSON.stringify({{kind:"snippet_budget_exceeded", message:"resolved snippet code budget exceeded"}})));
    return Promise.resolve().then(async () => {{
      globalThis.__somaSnippetStack.push(pending.name);
      try {{ return await (eval("(" + input.code + ")"))(__somaDecodeResult(input.input)); }}
      finally {{ globalThis.__somaSnippetStack.pop(); }}
    }}).then(pending.resolve, pending.reject);
  }}
  if (input.type === "step_decision") {{
    if (Object.prototype.hasOwnProperty.call(input, "replay") && input.replay !== null && input.replay !== undefined) return pending.resolve(__somaDecodeResult(input.replay));
    return Promise.resolve().then(pending.fn).then((value) => {{
      const encoded = __somaEncodeResult(value === undefined ? null : value);
      return new Promise((resolve, reject) => {{
        globalThis.__somaEmitStepResult(input.seq, encoded);
        globalThis.__somaPendingOperations.set(input.seq, {{ kind: "step_result", value, resolve, reject }});
      }});
    }}).then(pending.resolve, pending.reject);
  }}
  if (input.type === "step_recorded") return pending.resolve(pending.value);
  throw new Error("runner received unexpected protocol message");
}};
{proxy}
globalThis.__somaMainPromise = (async () => {{
{invoker}}})();
"#,
        codec = CODE_MODE_VALUE_CODEC_JS,
        invoker = invoker,
        proxy = proxy,
    )
}
