use std::{fs, sync::Arc, time::Instant};

use sha2::{Digest, Sha256};

use super::{MAX_WASM_ARTIFACT_BYTES, WasmArtifact, WasmRuntime, runtime_support::acquire_compile};

pub(super) fn artifact_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn read_artifact(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let length = fs::metadata(path).map_err(|error| error.to_string())?.len();
    if length > MAX_WASM_ARTIFACT_BYTES as u64 {
        return Err(format!(
            "WASM artifact exceeds {MAX_WASM_ARTIFACT_BYTES} bytes"
        ));
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_WASM_ARTIFACT_BYTES {
        return Err(format!(
            "WASM artifact exceeds {MAX_WASM_ARTIFACT_BYTES} bytes"
        ));
    }
    Ok(bytes)
}

pub(super) fn compile_artifact(
    runtime: &WasmRuntime,
    bytes: &[u8],
    deadline: Instant,
) -> Result<Arc<WasmArtifact>, String> {
    let _permit = acquire_compile(deadline)?;
    if Instant::now() >= deadline {
        return Err("WASM compilation deadline expired".to_owned());
    }
    let artifact = wasmtime::component::Component::from_binary(&runtime.engine, bytes)
        .map(WasmArtifact::Component)
        .or_else(|_| wasmtime::Module::from_binary(&runtime.engine, bytes).map(WasmArtifact::Core))
        .map(Arc::new)
        .map_err(|error| error.to_string())?;
    if Instant::now() >= deadline {
        return Err("WASM compilation exceeded its deadline".to_owned());
    }
    Ok(artifact)
}
