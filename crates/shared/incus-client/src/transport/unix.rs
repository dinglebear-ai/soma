//! Minimal hand-rolled HTTP/1.1 client over a Unix domain socket. Opens one
//! fresh `UnixStream` per request rather than pooling - see the module-level
//! design notes in the implementation plan this was built from
//! (`docs/superpowers/plans/2026-07-17-incus-client-crate.md`): a local
//! socket connect is cheap, and a fresh connection per request makes
//! concurrent requests trivially independent of each other, unlike a shared,
//! mutex-guarded stream would be.

use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::error::{Error, Result};
use crate::transport::Method;

/// Hard cap on a response body, enforced regardless of framing
/// (`Content-Length` or chunked). Mirrors `codex-app-server-client`'s
/// `MAX_LINE_BYTES` precedent.
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

/// Cap on the number of header lines a response may contain, independent of
/// the per-line `max_bytes` cap `read_line` already enforces on each line -
/// without this, a peer could send an unbounded number of small,
/// individually-legal header lines and grow `headers`/memory without bound.
const MAX_HEADER_COUNT: usize = 100;

/// A parsed HTTP response: status code, headers (lowercased names), and the
/// raw body bytes. Envelope (Incus sync/async/error JSON) parsing happens
/// one layer up, in `crate::transport`.
#[derive(Debug, Clone)]
pub(crate) struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RawResponse {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Confirms `path` is actually a Unix domain socket before we ever try to
/// connect to it, so a stale regular file or wrong path fails with a clear
/// error instead of an opaque connection-refused. Synchronous
/// (`std::fs::metadata`) - see [`check_is_socket_off_thread`] for the async
/// wrapper used on the request path; this sync version stays directly
/// testable and is the single source of truth for the check's logic.
pub(crate) fn check_is_socket(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path).map_err(Error::Transport)?;
    if !metadata.file_type().is_socket() {
        return Err(Error::Transport(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a Unix domain socket", path.display()),
        )));
    }
    Ok(())
}

/// Runs [`check_is_socket`] on the blocking-task pool rather than inline on
/// the calling Tokio worker thread. `std::fs::metadata` is a blocking
/// syscall; calling it directly inside an async fn (as this crate used to)
/// blocks that worker thread on every single request. Offloading it via
/// `spawn_blocking` keeps the friendlier "not a Unix domain socket" error
/// message (the alternative considered was dropping the check entirely and
/// relying on `UnixStream::connect`'s own, less specific, error).
async fn check_is_socket_off_thread(path: &Path) -> Result<()> {
    let owned_path = path.to_path_buf();
    tokio::task::spawn_blocking(move || check_is_socket(&owned_path))
        .await
        .map_err(|join_err| {
            Error::Transport(std::io::Error::other(format!(
                "socket check task panicked: {join_err}"
            )))
        })?
}

/// The request-shaping parameters for one call, grouped into a struct so
/// `execute`/`execute_capped` don't trip clippy's `too_many_arguments` lint
/// (`socket_path`, the cap, and `timeout` stay as separate parameters since
/// they're about the transport/call, not the request being made).
pub(crate) struct RequestSpec<'a> {
    pub(crate) method: Method,
    pub(crate) path: &'a str,
    pub(crate) query: &'a [(&'a str, &'a str)],
    pub(crate) body: Option<&'a [u8]>,
    pub(crate) if_match: Option<&'a str>,
}

/// Executes one HTTP request over a fresh connection to `socket_path`,
/// capping the response body at [`MAX_RESPONSE_BYTES`].
pub(crate) async fn execute(
    socket_path: &Path,
    spec: RequestSpec<'_>,
    timeout: Option<Duration>,
) -> Result<RawResponse> {
    execute_capped(socket_path, spec, MAX_RESPONSE_BYTES, timeout).await
}

/// [`execute`]'s implementation, parameterized over the cap so tests can
/// exercise the boundary condition without a 64 MiB fixture, and over the
/// per-request `timeout` (the plain-request path only - `wait_for_operation`
/// has its own, separate, server-side-bounded long-poll semantics).
pub(crate) async fn execute_capped(
    socket_path: &Path,
    spec: RequestSpec<'_>,
    max_response_bytes: usize,
    timeout: Option<Duration>,
) -> Result<RawResponse> {
    // Validate everything that gets interpolated into raw request-line/header
    // text *before* any I/O happens - a caller-supplied string containing
    // `\r\n` here could otherwise terminate the request early and smuggle a
    // second, fully attacker-controlled HTTP request onto this connection to
    // Incus's root-equivalent daemon. See `reject_control_chars`.
    let request_line = build_request_line(spec.method, spec.path, spec.query)?;
    if let Some(etag) = spec.if_match {
        reject_control_chars(etag, "If-Match header value")?;
    }

    let io = execute_io(
        socket_path,
        request_line,
        spec.if_match,
        spec.body,
        max_response_bytes,
    );

    match timeout {
        Some(duration) => tokio::time::timeout(duration, io)
            .await
            .map_err(|_elapsed| Error::Timeout { after: duration })?,
        None => io.await,
    }
}

/// The actual connect/write/read I/O for one request, split out of
/// [`execute_capped`] so only this portion - not the pre-I/O validation
/// above it - is subject to the caller's timeout.
async fn execute_io(
    socket_path: &Path,
    request_line: String,
    if_match: Option<&str>,
    body: Option<&[u8]>,
    max_response_bytes: usize,
) -> Result<RawResponse> {
    check_is_socket_off_thread(socket_path).await?;
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(Error::Transport)?;
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Assemble the request line and every header into one buffer and issue
    // a single `write_all` for it, rather than one `write_all` call per
    // header line - the body (when present) is written separately so a
    // large body doesn't get copied into this buffer first.
    let mut header_buf = String::with_capacity(request_line.len() + 128);
    header_buf.push_str(&request_line);
    header_buf.push_str("Host: localhost\r\nAccept: application/json\r\nConnection: close\r\n");
    if let Some(etag) = if_match {
        header_buf.push_str("If-Match: ");
        header_buf.push_str(etag);
        header_buf.push_str("\r\n");
    }
    if let Some(body) = body {
        header_buf.push_str("Content-Type: application/json\r\nContent-Length: ");
        header_buf.push_str(&body.len().to_string());
        header_buf.push_str("\r\n\r\n");
    } else {
        header_buf.push_str("\r\n");
    }

    write_half
        .write_all(header_buf.as_bytes())
        .await
        .map_err(Error::Transport)?;
    if let Some(body) = body {
        write_half.write_all(body).await.map_err(Error::Transport)?;
    }
    write_half.flush().await.map_err(Error::Transport)?;

    read_response(&mut reader, max_response_bytes).await
}

/// Rejects `value` if it contains a CR, LF, or other C0/DEL control
/// character before it's interpolated into a raw HTTP request line or
/// header value.
///
/// Every resource module builds request paths via `format!("/1.0/.../{name}")`
/// from caller-supplied identifiers (instance names, image fingerprints,
/// network/project/pool/volume/snapshot names) with zero encoding, and
/// `If-Match` is written via `format!("If-Match: {etag}\r\n")` the same way.
/// Without this check, a caller-supplied string containing `\r\n` could
/// terminate the request early and smuggle a second, fully
/// attacker-controlled HTTP request onto the same connection to Incus's
/// root-equivalent daemon - contrast with the query-string building in this
/// same module, which already percent-encodes via
/// `url::form_urlencoded::Serializer` and so isn't vulnerable to this.
fn reject_control_chars(value: &str, what: &str) -> Result<()> {
    if value.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err(Error::InvalidRequest(format!(
            "{what} contains a control character (e.g. CR or LF) and cannot be sent as raw \
             HTTP request text: {value:?}"
        )));
    }
    Ok(())
}

fn build_request_line(method: Method, path: &str, query: &[(&str, &str)]) -> Result<String> {
    reject_control_chars(path, "request path")?;
    if query.is_empty() {
        Ok(format!("{} {} HTTP/1.1\r\n", method.as_str(), path))
    } else {
        let query_string = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(query)
            .finish();
        Ok(format!(
            "{} {}?{} HTTP/1.1\r\n",
            method.as_str(),
            path,
            query_string
        ))
    }
}

async fn read_response<R>(reader: &mut BufReader<R>, max_bytes: usize) -> Result<RawResponse>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let status_line = read_line(reader, max_bytes).await?;
    let status = parse_status_line(&status_line)?;

    let mut headers = Vec::new();
    let mut header_section_bytes = 0usize;
    loop {
        let line = read_line(reader, max_bytes).await?;
        if line.is_empty() {
            break;
        }
        // `read_line` already bounds each individual line to `max_bytes`,
        // but a peer could still send an unbounded *number* of small,
        // individually-legal header lines - cap the cumulative header
        // section size and the header count independently of any one
        // line's length.
        header_section_bytes = header_section_bytes.saturating_add(line.len());
        if header_section_bytes > max_bytes || headers.len() >= MAX_HEADER_COUNT {
            return Err(Error::ResponseTooLarge { limit: max_bytes });
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok());
    let is_chunked = headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding") && value.eq_ignore_ascii_case("chunked")
    });

    let body = if let Some(length) = content_length {
        if length > max_bytes {
            return Err(Error::ResponseTooLarge { limit: max_bytes });
        }
        let mut buf = vec![0u8; length];
        reader
            .read_exact(&mut buf)
            .await
            .map_err(Error::Transport)?;
        buf
    } else if is_chunked {
        read_chunked_body(reader, max_bytes).await?
    } else {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = reader.read(&mut chunk).await.map_err(Error::Transport)?;
            if n == 0 {
                break;
            }
            if buf.len() + n > max_bytes {
                return Err(Error::ResponseTooLarge { limit: max_bytes });
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        buf
    };

    Ok(RawResponse {
        status,
        headers,
        body,
    })
}

async fn read_chunked_body<R>(reader: &mut BufReader<R>, max_bytes: usize) -> Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut body = Vec::new();
    loop {
        let size_line = read_line(reader, max_bytes).await?;
        let size = usize::from_str_radix(size_line.trim(), 16).map_err(|_| {
            Error::InvalidResponse(format!("invalid chunk size line: {size_line:?}"))
        })?;
        if size == 0 {
            // Consume the trailing CRLF after the terminating 0-size chunk.
            let _ = read_line(reader, max_bytes).await?;
            break;
        }
        // `size` is parsed from peer-controlled hex text with no upper
        // bound (only that it fits in a usize), so `body.len() + size`
        // could overflow - a debug-build panic, or a release-build wrap
        // that silently bypasses this cap entirely. Compare against the
        // remaining budget instead of adding.
        if size > max_bytes.saturating_sub(body.len()) {
            return Err(Error::ResponseTooLarge { limit: max_bytes });
        }
        let mut chunk = vec![0u8; size];
        reader
            .read_exact(&mut chunk)
            .await
            .map_err(Error::Transport)?;
        body.extend_from_slice(&chunk);
        // Each chunk is followed by a CRLF that isn't part of the payload.
        let _ = read_line(reader, max_bytes).await?;
    }
    Ok(body)
}

/// Reads one `\r\n`-terminated line (the `\r\n` stripped from the returned
/// string), enforcing `max_bytes` so a peer that never sends a newline can't
/// grow the buffer without bound.
///
/// Uses `AsyncBufReadExt::read_until` (the `BufReader` we're already
/// wrapping the socket in makes this available) instead of a hand-rolled
/// byte-by-byte loop. `read_until` itself has no size cap - given an
/// unbounded stream of bytes with no `\n`, it will keep pulling in more
/// data forever - so we wrap `reader` in `AsyncReadExt::take(max_bytes + 1)`
/// first, bounding the single `read_until` call to at most one byte past
/// the cap. That lets us tell "the line itself exceeds `max_bytes`" (we hit
/// exactly the take limit without finding `\n`) apart from "the connection
/// closed" (we hit EOF short of the limit).
async fn read_line<R>(reader: &mut BufReader<R>, max_bytes: usize) -> Result<String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let limit = (max_bytes as u64).saturating_add(1);
    let mut buf = Vec::new();
    reader
        .take(limit)
        .read_until(b'\n', &mut buf)
        .await
        .map_err(Error::Transport)?;

    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    } else if buf.len() as u64 >= limit {
        // The take() window was exhausted without ever finding '\n'.
        return Err(Error::ResponseTooLarge { limit: max_bytes });
    } else if buf.is_empty() {
        // Genuine underlying EOF with nothing read at all.
        return Err(Error::Transport(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "connection closed before a complete response was received",
        )));
    }
    // else: genuine underlying EOF after a partial, unterminated line -
    // treat the partial bytes as the final line (matches the pre-refactor
    // byte-by-byte reader's behavior).

    String::from_utf8(buf)
        .map_err(|err| Error::InvalidResponse(format!("response line was not valid UTF-8: {err}")))
}

fn parse_status_line(line: &str) -> Result<u16> {
    let mut parts = line.split_whitespace();
    let _http_version = parts
        .next()
        .ok_or_else(|| Error::InvalidResponse(format!("empty status line: {line:?}")))?;
    let status = parts
        .next()
        .ok_or_else(|| Error::InvalidResponse(format!("malformed status line: {line:?}")))?;
    status
        .parse::<u16>()
        .map_err(|_| Error::InvalidResponse(format!("non-numeric status code: {status:?}")))
}

#[cfg(test)]
#[path = "unix_tests.rs"]
pub(crate) mod tests;
