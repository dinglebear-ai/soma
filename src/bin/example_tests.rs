use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn http_server_requests_are_rejected_by_local_binary() {
    assert!(is_http_server_request(&args(&[])));
    assert!(is_http_server_request(&args(&["serve"])));
    assert!(is_http_server_request(&args(&["serve", "mcp"])));
}

#[test]
fn local_cli_and_stdio_requests_stay_in_local_binary() {
    assert!(!is_http_server_request(&args(&["doctor"])));
    assert!(!is_http_server_request(&args(&["mcp"])));
    assert!(!is_http_server_request(&args(&["setup", "plugin-hook"])));
}
