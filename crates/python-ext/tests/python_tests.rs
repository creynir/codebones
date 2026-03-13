use codebones_python_ext::Codebones;
use pyo3::prelude::*;

#[test]
fn test_python_api_get_exception_handling() {
    // 4. Python API `get` Exception Handling
    pyo3::Python::initialize();
    Python::attach(|_py| {
        let result = Codebones::get("NonExistentSymbol".to_string());
        assert!(result.is_err(), "Expected an error for non-existent symbol");
    });
}

#[test]
fn test_python_api_e2e() {
    // 5. Python API End-to-End
    pyo3::Python::initialize();
    Python::attach(|_py| {
        let _ = Codebones::index(".".to_string());
        let results = Codebones::search("test".to_string()).unwrap();

        assert!(!results.is_empty(), "Expected symbols from the fixture");
    });
}
