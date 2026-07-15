use std::io::{self, Read};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrainedStderr {
    pub text: String,
    pub truncated: bool,
}

pub fn drain_stderr_with_cap<R: Read>(
    mut reader: R,
    cap_bytes: usize,
) -> io::Result<DrainedStderr> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    let truncated = buf.len() > cap_bytes;
    if truncated {
        buf.truncate(cap_bytes);
    }
    Ok(DrainedStderr {
        text: String::from_utf8_lossy(&buf).into_owned(),
        truncated,
    })
}

#[cfg(test)]
#[path = "stderr_tests.rs"]
mod tests;
