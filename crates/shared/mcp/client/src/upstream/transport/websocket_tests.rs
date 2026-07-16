use rmcp::model::{ClientRequest, ErrorCode, ErrorData, NumberOrString, ServerResult};
use rmcp::service::RxJsonRpcMessage;
use rmcp::RoleClient;

use super::*;

#[test]
fn json_rpc_frame_codec_round_trips_requests_responses_and_errors() {
    let request = TxJsonRpcMessage::<RoleClient>::request(
        ClientRequest::PingRequest(Default::default()),
        NumberOrString::Number(7),
    );
    let encoded_request = encode_client_message(&request).expect("encode request");
    let decoded_request: serde_json::Value =
        serde_json::from_str(&encoded_request).expect("decode request json");
    assert_eq!(decoded_request["jsonrpc"], "2.0");
    assert_eq!(decoded_request["id"], 7);

    let response = serde_json::to_string(&RxJsonRpcMessage::<RoleClient>::response(
        ServerResult::empty(()),
        NumberOrString::Number(9),
    ))
    .expect("encode response");
    let decoded_response = decode_server_message(&response).expect("decode response");
    assert!(matches!(
        decoded_response,
        RxJsonRpcMessage::<RoleClient>::Response(_)
    ));

    let error = serde_json::to_string(&RxJsonRpcMessage::<RoleClient>::error(
        ErrorData::new(ErrorCode::METHOD_NOT_FOUND, "method not found", None),
        Some(NumberOrString::Number(11)),
    ))
    .expect("encode error");
    let decoded_error = decode_server_message(&error).expect("decode error");
    assert!(matches!(
        decoded_error,
        RxJsonRpcMessage::<RoleClient>::Error(_)
    ));
}
