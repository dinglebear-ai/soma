use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncReadExt};

use super::{PythonWorkerLogEntry, WorkerLogBuffer};

pub(super) async fn drain_stderr<R: AsyncRead + Unpin>(
    mut reader: R,
    retained: Arc<Mutex<WorkerLogBuffer>>,
    limit: usize,
) {
    let mut buffer = [0_u8; 4096];
    let mut pending = Vec::new();
    loop {
        let Ok(read) = reader.read(&mut buffer).await else {
            retain_pending(&retained, &pending, limit);
            return;
        };
        if read == 0 {
            retain_pending(&retained, &pending, limit);
            return;
        }
        if limit == 0 {
            continue;
        }
        pending.extend_from_slice(&buffer[..read]);
        while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
            let line = pending.drain(..=newline).collect::<Vec<_>>();
            retain_log_line(&retained, &line, limit);
        }
        if pending.len() > limit {
            let split = pending.len().saturating_sub(limit);
            pending.drain(..split);
        }
    }
}

fn retain_pending(retained: &Mutex<WorkerLogBuffer>, pending: &[u8], limit: usize) {
    if !pending.is_empty() && limit != 0 {
        retain_log_line(retained, pending, limit);
    }
}

fn retain_log_line(retained: &Mutex<WorkerLogBuffer>, line: &[u8], limit: usize) {
    let raw = String::from_utf8_lossy(line)
        .trim_end_matches(['\r', '\n'])
        .to_owned();
    if raw.is_empty() {
        return;
    }
    // Provider stderr is fully untrusted and can contain arbitrary secrets
    // without recognizable key names. Preserve only event shape and ordering;
    // never expose the provider-controlled payload through operator status.
    let message = "[redacted provider diagnostic]".to_owned();
    let size = message.len();
    let mut retained = retained
        .lock()
        .expect("Python worker log lock should not be poisoned");
    while retained.retained_bytes.saturating_add(size) > limit {
        let Some(removed) = retained.entries.pop_front() else {
            break;
        };
        retained.retained_bytes = retained
            .retained_bytes
            .saturating_sub(removed.message.len());
    }
    if size <= limit {
        let sequence = retained.next_sequence;
        retained.next_sequence = retained.next_sequence.saturating_add(1);
        retained.retained_bytes = retained.retained_bytes.saturating_add(size);
        retained.entries.push_back(PythonWorkerLogEntry {
            sequence,
            stream: "stderr",
            message,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn retains_and_redacts_unterminated_final_line() {
        let retained = Arc::new(Mutex::new(WorkerLogBuffer {
            entries: VecDeque::new(),
            retained_bytes: 0,
            next_sequence: 1,
        }));
        let (mut writer, reader) = tokio::io::duplex(128);
        writer
            .write_all(b"token=unterminated-secret")
            .await
            .expect("write diagnostic");
        drop(writer);

        drain_stderr(reader, retained.clone(), 128).await;

        let retained = retained.lock().expect("log buffer");
        assert_eq!(retained.entries.len(), 1);
        assert_eq!(
            retained.entries[0].message,
            "[redacted provider diagnostic]"
        );
    }
}
