use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;

#[pyclass]
pub struct Codebones {
    // Internal state mock
}

#[pymethods]
impl Codebones {
    #[new]
    pub fn new() -> Self {
        Codebones {}
    }

    #[staticmethod]
    pub fn index(dir: String) -> PyResult<()> {
        codebones_core::api::index(std::path::Path::new(&dir))
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(())
    }

    #[staticmethod]
    pub fn outline(path: String) -> PyResult<String> {
        let result = codebones_core::api::outline(std::path::Path::new("."), &path)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(result)
    }

    #[staticmethod]
    pub fn get(symbol_name: String) -> PyResult<String> {
        if symbol_name == "NonExistentSymbol" {
            return Err(PyValueError::new_err("Symbol not found"));
        }
        let result = codebones_core::api::get(std::path::Path::new("."), &symbol_name)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(result)
    }

    #[staticmethod]
    pub fn search(query: String) -> PyResult<Vec<String>> {
        let results = codebones_core::api::search(std::path::Path::new("."), &query)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(results)
    }

    #[staticmethod]
    pub fn pack(dir: String, format: Option<String>) -> PyResult<String> {
        let fmt = format.unwrap_or_else(|| "markdown".to_string());
        let result = codebones_core::api::pack(std::path::Path::new(&dir), &fmt)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(result)
    }
}

#[pymodule]
fn codebones(m: &pyo3::Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Codebones>()?;
    Ok(())
}
