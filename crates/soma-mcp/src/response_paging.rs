use rmcp::{
    model::{CallToolResult, ContentBlock},
    ErrorData,
};
use serde_json::{json, Map, Value};

use soma_contracts::token_limit::MAX_RESPONSE_BYTES;
use soma_runtime::server::ResponsePageStore;

const RESPONSE_OFFSET_PARAM: &str = "_response_offset";
const RESPONSE_PAGE_BYTES_PARAM: &str = "_response_page_bytes";
const RESPONSE_CURSOR_PARAM: &str = "_response_cursor";
pub(crate) const ACTION_DISCRIMINATOR_FIELD: &str = "_soma_action";
const DEFAULT_RESPONSE_PAGE_BYTES: usize = 16_000;
const MAX_RESPONSE_PAGE_BYTES: usize = 16_000;
const MAX_RESPONSE_CURSOR_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResponsePageRequest {
    pub(super) cursor: Option<String>,
    pub(super) offset: usize,
    pub(super) page_bytes: usize,
}

impl Default for ResponsePageRequest {
    fn default() -> Self {
        Self {
            cursor: None,
            offset: 0,
            page_bytes: DEFAULT_RESPONSE_PAGE_BYTES,
        }
    }
}

impl ResponsePageRequest {
    pub(super) fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
}

pub(super) fn response_page_request(
    args: Option<&Map<String, Value>>,
) -> Result<ResponsePageRequest, ErrorData> {
    let Some(args) = args else {
        return Ok(ResponsePageRequest::default());
    };
    let cursor = optional_string_arg(args, RESPONSE_CURSOR_PARAM)?;
    let offset = optional_usize_arg(args, RESPONSE_OFFSET_PARAM)?.unwrap_or(0);
    let page_bytes = optional_usize_arg(args, RESPONSE_PAGE_BYTES_PARAM)?
        .unwrap_or(DEFAULT_RESPONSE_PAGE_BYTES)
        .min(MAX_RESPONSE_PAGE_BYTES);
    if page_bytes == 0 {
        return Err(ErrorData::invalid_params(
            format!("{RESPONSE_PAGE_BYTES_PARAM} must be greater than zero"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "invalid_response_page_bytes",
                "field": RESPONSE_PAGE_BYTES_PARAM,
                "retryable": true,
                "remediation": format!("Omit {RESPONSE_PAGE_BYTES_PARAM} or pass an integer from 1 to {MAX_RESPONSE_PAGE_BYTES}."),
            })),
        ));
    }
    if offset > 0 && cursor.is_none() {
        return Err(ErrorData::invalid_params(
            format!("{RESPONSE_CURSOR_PARAM} is required when {RESPONSE_OFFSET_PARAM} is set"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "missing_response_cursor",
                "field": RESPONSE_CURSOR_PARAM,
                "retryable": true,
                "remediation": format!("Use the {RESPONSE_CURSOR_PARAM} value returned by the previous mcp_response_page continuation."),
            })),
        ));
    }
    Ok(ResponsePageRequest {
        cursor,
        offset,
        page_bytes,
    })
}

fn optional_string_arg(
    args: &Map<String, Value>,
    field: &str,
) -> Result<Option<String>, ErrorData> {
    let Some(value) = args.get(field) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(ErrorData::invalid_params(
            format!("{field} must be a string"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "invalid_response_cursor",
                "field": field,
                "retryable": true,
                "remediation": format!("Pass {field} exactly as returned by the previous mcp_response_page continuation."),
            })),
        ));
    };
    if field == RESPONSE_CURSOR_PARAM && value.len() > MAX_RESPONSE_CURSOR_BYTES {
        return Err(ErrorData::invalid_params(
            format!("{field} exceeded {MAX_RESPONSE_CURSOR_BYTES} bytes"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "response_cursor_too_long",
                "field": field,
                "retryable": true,
                "remediation": format!("Pass {field} exactly as returned by the previous mcp_response_page continuation."),
            })),
        ));
    }
    Ok(Some(value.to_owned()))
}

fn optional_usize_arg(args: &Map<String, Value>, field: &str) -> Result<Option<usize>, ErrorData> {
    let Some(value) = args.get(field) else {
        return Ok(None);
    };
    let Some(value) = value.as_u64() else {
        return Err(ErrorData::invalid_params(
            format!("{field} must be an unsigned integer"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "invalid_response_page_arg",
                "field": field,
                "retryable": true,
                "remediation": format!("Pass {field} as a non-negative integer."),
            })),
        ));
    };
    usize::try_from(value).map(Some).map_err(|_| {
        ErrorData::invalid_params(
            format!("{field} is too large"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "response_page_arg_too_large",
                "field": field,
                "retryable": true,
                "remediation": format!("Pass a smaller {field} value."),
            })),
        )
    })
}

pub(super) fn strip_response_page_params(arguments: &mut Value) {
    let Some(arguments) = arguments.as_object_mut() else {
        return;
    };
    arguments.remove(RESPONSE_OFFSET_PARAM);
    arguments.remove(RESPONSE_PAGE_BYTES_PARAM);
    arguments.remove(RESPONSE_CURSOR_PARAM);
}

pub(super) fn tool_result_from_json(
    mut value: Value,
    response_pages: &ResponsePageStore,
    page_request: ResponsePageRequest,
    tool: &str,
    action: Option<&str>,
    continuation_args: Option<&Map<String, Value>>,
) -> Result<CallToolResult, ErrorData> {
    if let Some(cursor) = page_request.cursor.clone() {
        return tool_result_from_cached_page(response_pages, &cursor, page_request, tool, action);
    }
    add_action_discriminator(&mut value, action);

    // Compact JSON (not pretty) recovers ~30-40% of the 40 KB token budget.
    let text = serde_json::to_string(&value)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    if text.len() <= MAX_RESPONSE_BYTES && page_request.offset == 0 {
        let mut result = CallToolResult::structured(value);
        result.content = vec![ContentBlock::text(text)];
        return Ok(result);
    }

    let cursor = response_pages.insert(text.clone());
    let payload = response_page_payload(
        &text,
        page_request,
        tool,
        action,
        continuation_args,
        Some(&cursor),
    );
    let text = serde_json::to_string(&payload)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    let mut result = CallToolResult::structured(payload);
    result.content = vec![ContentBlock::text(text)];
    Ok(result)
}

fn add_action_discriminator(value: &mut Value, action: Option<&str>) {
    let (Some(action), Some(object)) = (action, value.as_object_mut()) else {
        return;
    };
    object.insert(
        ACTION_DISCRIMINATOR_FIELD.to_owned(),
        Value::String(action.to_owned()),
    );
}

pub(super) fn tool_result_from_cached_page(
    response_pages: &ResponsePageStore,
    cursor: &str,
    page_request: ResponsePageRequest,
    tool: &str,
    action: Option<&str>,
) -> Result<CallToolResult, ErrorData> {
    let Some(serialized) = response_pages.get(cursor) else {
        return Err(ErrorData::invalid_params(
            "response cursor not found or expired",
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "response_cursor_not_found",
                "field": RESPONSE_CURSOR_PARAM,
                "retryable": true,
                "remediation": "Re-run the original tool call to create a fresh response cursor.",
            })),
        ));
    };
    let payload =
        response_page_payload(&serialized, page_request, tool, action, None, Some(cursor));
    let text = serde_json::to_string(&payload)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    let mut result = CallToolResult::structured(payload);
    result.content = vec![ContentBlock::text(text)];
    Ok(result)
}

fn response_page_payload(
    serialized: &str,
    page_request: ResponsePageRequest,
    tool: &str,
    action: Option<&str>,
    continuation_args: Option<&Map<String, Value>>,
    cursor: Option<&str>,
) -> Value {
    let (offset, content, next_offset, has_more) =
        response_page_slice(serialized, page_request.offset, page_request.page_bytes);
    let continuation = has_more.then(|| {
        let arguments = continuation_arguments_with_page(
            continuation_args,
            action,
            next_offset,
            page_request.page_bytes,
            cursor,
        );
        json!({
            "tool": tool,
            "arguments": arguments,
            "note": "Call the same tool with the same original arguments plus these reserved continuation arguments.",
        })
    });

    json!({
        "kind": "mcp_response_page",
        "schema_version": 1,
        "code": "response_page",
        "message": "Tool response was returned as a scrollable serialized JSON page.",
        "truncated": false,
        "serialized_bytes": serialized.len(),
        "max_response_bytes": MAX_RESPONSE_BYTES,
        "content_format": "application/json-fragment",
        "content": content,
        "page": {
            "offset": offset,
            "page_bytes": page_request.page_bytes,
            "next_offset": next_offset,
            "has_more": has_more,
        },
        "continuation": continuation,
    })
}

fn continuation_arguments_with_page(
    arguments: Option<&Map<String, Value>>,
    action: Option<&str>,
    next_offset: usize,
    page_bytes: usize,
    cursor: Option<&str>,
) -> Value {
    let mut output = arguments.cloned().unwrap_or_default();
    if !output.contains_key("action") {
        output.insert(
            "action".to_owned(),
            action.map(Value::from).unwrap_or(Value::Null),
        );
    }
    if let Some(cursor) = cursor {
        output.insert(RESPONSE_CURSOR_PARAM.to_owned(), json!(cursor));
    }
    output.insert(RESPONSE_OFFSET_PARAM.to_owned(), json!(next_offset));
    output.insert(RESPONSE_PAGE_BYTES_PARAM.to_owned(), json!(page_bytes));
    Value::Object(output)
}

fn response_page_slice(
    serialized: &str,
    requested_offset: usize,
    page_bytes: usize,
) -> (usize, &str, usize, bool) {
    let mut offset = requested_offset.min(serialized.len());
    while offset < serialized.len() && !serialized.is_char_boundary(offset) {
        offset += 1;
    }

    let mut end = offset.saturating_add(page_bytes).min(serialized.len());
    while end > offset && !serialized.is_char_boundary(end) {
        end -= 1;
    }

    (
        offset,
        &serialized[offset..end],
        end,
        end < serialized.len(),
    )
}

#[cfg(test)]
#[path = "response_paging_tests.rs"]
mod tests;
