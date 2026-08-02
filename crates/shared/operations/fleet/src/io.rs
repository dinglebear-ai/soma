use tokio::io::{AsyncRead, AsyncReadExt};

/// Drains an async stream to EOF while retaining at most the requested bytes.
pub(crate) async fn drain_bounded<R>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)>
where
    R: AsyncRead + Unpin,
{
    let mut retained = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        let keep = remaining.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }
    Ok((retained, truncated))
}

#[cfg(test)]
#[path = "io_tests.rs"]
mod tests;
