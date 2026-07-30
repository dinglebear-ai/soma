use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use super::{PythonSupervisorError, invalid_output, protocol_error};
use crate::python_protocol::PYTHON_RUNNER_MAX_FRAME_BYTES;

const FRAME_HEADER_BYTES: usize = 4;

pub(super) fn host_call_request_id(call: &crate::python_protocol::PythonRunnerHostCall) -> u64 {
    use crate::python_protocol::PythonRunnerHostCall;
    match call {
        PythonRunnerHostCall::Http { request_id, .. }
        | PythonRunnerHostCall::Secret { request_id, .. }
        | PythonRunnerHostCall::StateGet { request_id, .. }
        | PythonRunnerHostCall::StatePut { request_id, .. }
        | PythonRunnerHostCall::Log { request_id, .. }
        | PythonRunnerHostCall::Metric { request_id, .. }
        | PythonRunnerHostCall::Progress { request_id, .. } => *request_id,
    }
}

pub(super) async fn write_frame<T: Serialize>(
    writer: &mut OwnedWriteHalf,
    message: &T,
) -> Result<(), PythonSupervisorError> {
    let payload = serde_json::to_vec(message).map_err(|_| invalid_output())?;
    if payload.len() > PYTHON_RUNNER_MAX_FRAME_BYTES {
        return Err(PythonSupervisorError::new(
            "python_input_too_large",
            "Python runner frame exceeded its limit",
        ));
    }
    writer
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

pub(super) async fn read_frame<T: serde::de::DeserializeOwned>(
    reader: &mut OwnedReadHalf,
) -> Result<T, PythonSupervisorError> {
    let mut header = [0_u8; FRAME_HEADER_BYTES];
    reader.read_exact(&mut header).await?;
    let length = u32::from_be_bytes(header) as usize;
    if length > PYTHON_RUNNER_MAX_FRAME_BYTES {
        return Err(protocol_error());
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).await?;
    serde_json::from_slice(&payload).map_err(|_| protocol_error())
}
