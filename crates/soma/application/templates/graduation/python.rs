use pyo3::{prelude::*, types::PyModule};

#[pyfunction]
fn invoke(input_json: &str) -> PyResult<String> {
    let input = serde_json::from_str(input_json)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
    let output = crate::core::invoke(input)
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
    serde_json::to_string(&output)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
}

#[pymodule]
fn _soma_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(invoke, module)?)?;
    Ok(())
}
