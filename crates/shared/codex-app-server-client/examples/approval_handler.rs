//! Shows how to route app-server requests through a custom policy.
//!
//! For canned policies, use `DenyAllApprovalHandler`, `ReadOnlyApprovalHandler`,
//! or `AllowAllApprovalHandler` directly.

use codex_app_server_client::protocol::ThreadStartParams;
use codex_app_server_client::{
    CodexSession, FnApprovalHandler, ServerRequestReply, SessionOptions,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> codex_app_server_client::Result<()> {
    let mut session = CodexSession::spawn(SessionOptions::new(
        "codex_app_server_client_approval_example",
        env!("CARGO_PKG_VERSION"),
    ))
    .await?;
    let handler = FnApprovalHandler::new(|request| {
        eprintln!("denying app-server request {}", request.method_name());
        ServerRequestReply::Error {
            code: -32000,
            message: "example policy denied this request".to_owned(),
            data: None,
        }
    });

    let _thread = session.start_thread(ThreadStartParams::new()).await?;
    if let Ok(Some(notification)) =
        tokio::time::timeout(Duration::from_secs(1), session.next_notification(&handler)).await
    {
        println!("first notification: {}", notification.method_name());
    }
    Ok(())
}
