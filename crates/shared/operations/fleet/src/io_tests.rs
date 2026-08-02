use tokio::io::AsyncWriteExt;

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn bounded_drain_keeps_prefix_and_consumes_to_eof() {
    let (mut writer, reader) = tokio::io::duplex(64);
    let task = tokio::spawn(async move {
        writer.write_all(b"abcdefghij").await.unwrap();
    });
    let (bytes, truncated) = drain_bounded(reader, 4).await.unwrap();
    task.await.unwrap();
    assert_eq!(bytes, b"abcd");
    assert!(truncated);
}
