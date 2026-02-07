//! Python bindings for littrs - a minimal, secure Python sandbox.
//!
//! This module provides Python access to the littrs sandbox, allowing
//! secure execution of untrusted Python code with tool registration.

use ::littrs::{
    PyValue, ResourceLimits, Sandbox as RustSandbox, WasmError, WasmSandbox as RustWasmSandbox,
    WasmSandboxConfig as RustWasmSandboxConfig,
};
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};

// ============================================================================
// PyValue conversion
// ============================================================================

/// Convert a littrs::PyValue to a Python object.
fn pyvalue_to_py(py: Python<'_>, value: &PyValue) -> PyObject {
    match value {
        PyValue::None => py.None(),
        PyValue::Bool(b) => b.into_py(py),
        PyValue::Int(i) => i.into_py(py),
        PyValue::Float(f) => f.into_py(py),
        PyValue::Str(s) => s.into_py(py),
        PyValue::List(items) => {
            let list: Vec<PyObject> = items.iter().map(|v| pyvalue_to_py(py, v)).collect();
            list.into_py(py)
        }
        PyValue::Dict(pairs) => {
            let dict = PyDict::new(py);
            for (k, v) in pairs {
                dict.set_item(k, pyvalue_to_py(py, v)).unwrap();
            }
            dict.into_py(py)
        }
    }
}

/// Convert a Python object to a littrs::PyValue.
fn py_to_pyvalue(obj: &Bound<'_, PyAny>) -> PyResult<PyValue> {
    if obj.is_none() {
        Ok(PyValue::None)
    } else if let Ok(b) = obj.downcast::<PyBool>() {
        Ok(PyValue::Bool(b.is_true()))
    } else if let Ok(i) = obj.downcast::<PyInt>() {
        Ok(PyValue::Int(i.extract()?))
    } else if let Ok(f) = obj.downcast::<PyFloat>() {
        Ok(PyValue::Float(f.extract()?))
    } else if let Ok(s) = obj.downcast::<PyString>() {
        Ok(PyValue::Str(s.to_string()))
    } else if let Ok(list) = obj.downcast::<PyList>() {
        let items: PyResult<Vec<_>> = list.iter().map(|item| py_to_pyvalue(&item)).collect();
        Ok(PyValue::List(items?))
    } else if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut pairs = Vec::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            pairs.push((key, py_to_pyvalue(&v)?));
        }
        Ok(PyValue::Dict(pairs))
    } else {
        Err(PyTypeError::new_err(format!(
            "Cannot convert {} to sandbox value",
            obj.get_type().name()?
        )))
    }
}

// ============================================================================
// Sandbox wrapper
// ============================================================================

/// A secure Python sandbox for executing untrusted code.
///
/// The sandbox provides a minimal Python subset that can execute code
/// safely without access to the file system, network, or other resources.
///
/// Example:
///     >>> from littrs import Sandbox
///     >>> sandbox = Sandbox()
///     >>> sandbox.execute("2 + 2")
///     4
///     >>> sandbox.execute("x = 10")
///     >>> sandbox.execute("x * 2")
///     20
#[pyclass]
struct Sandbox {
    inner: RustSandbox,
}

#[pymethods]
impl Sandbox {
    /// Create a new sandbox instance.
    #[new]
    fn new() -> Self {
        Self {
            inner: RustSandbox::new(),
        }
    }

    /// Execute Python code in the sandbox.
    ///
    /// Returns the value of the last expression, or None if the code
    /// ends with a statement.
    ///
    /// Args:
    ///     code: The Python code to execute.
    ///
    /// Returns:
    ///     The result of the last expression.
    ///
    /// Raises:
    ///     RuntimeError: If execution fails (syntax error, runtime error, etc.)
    fn execute(&mut self, py: Python<'_>, code: &str) -> PyResult<PyObject> {
        match self.inner.execute(code) {
            Ok(value) => Ok(pyvalue_to_py(py, &value)),
            Err(e) => Err(PyRuntimeError::new_err(format!("{}", e))),
        }
    }

    /// Execute Python code and capture print output.
    ///
    /// Returns a tuple of (result, printed_lines).
    ///
    /// Args:
    ///     code: The Python code to execute.
    ///
    /// Returns:
    ///     A tuple of (result, list of printed lines).
    fn execute_with_output(
        &mut self,
        py: Python<'_>,
        code: &str,
    ) -> PyResult<(PyObject, Vec<String>)> {
        match self.inner.execute_with_output(code) {
            Ok(output) => Ok((pyvalue_to_py(py, &output.result), output.printed)),
            Err(e) => Err(PyRuntimeError::new_err(format!("{}", e))),
        }
    }

    /// Set a variable in the sandbox's global scope.
    ///
    /// Args:
    ///     name: The variable name.
    ///     value: The value to set (must be a basic Python type).
    fn set_variable(&mut self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let pyvalue = py_to_pyvalue(value)?;
        self.inner.set_variable(name, pyvalue);
        Ok(())
    }

    /// Register a Python callable as a tool in the sandbox.
    ///
    /// The callable will be available as a function in the sandbox.
    ///
    /// Args:
    ///     name: The function name in the sandbox.
    ///     func: A Python callable that takes a list of arguments.
    ///
    /// Example:
    ///     >>> def add(args):
    ///     ...     return args[0] + args[1]
    ///     >>> sandbox.register_function("add", add)
    ///     >>> sandbox.execute("add(1, 2)")
    ///     3
    fn register_function(&mut self, py: Python<'_>, name: &str, func: PyObject) -> PyResult<()> {
        // Verify it's callable
        if !func.bind(py).is_callable() {
            return Err(PyTypeError::new_err("func must be callable"));
        }

        let func = func.clone_ref(py);
        self.inner.register_fn(name, move |args: Vec<PyValue>| {
            Python::with_gil(|py| {
                // Convert args to Python objects
                let py_args: Vec<PyObject> = args.iter().map(|v| pyvalue_to_py(py, v)).collect();

                // Call the function
                match func.call1(py, (py_args,)) {
                    Ok(result) => {
                        // Convert result back to PyValue
                        py_to_pyvalue(result.bind(py)).unwrap_or(PyValue::None)
                    }
                    Err(e) => {
                        // Return error as a dict
                        PyValue::Dict(vec![(
                            "error".to_string(),
                            PyValue::Str(format!("{}", e)),
                        )])
                    }
                }
            })
        });

        Ok(())
    }

    /// Set resource limits for sandbox execution.
    ///
    /// Limits are enforced per execute() call. The instruction counter
    /// resets at the start of each execution.
    ///
    /// Args:
    ///     max_instructions: Maximum bytecode instructions per execute() call.
    ///         None means unlimited.
    ///     max_recursion_depth: Maximum call-stack depth for user-defined functions.
    ///         None means unlimited.
    ///
    /// Example:
    ///     >>> sandbox = Sandbox()
    ///     >>> sandbox.set_limits(max_instructions=1000)
    ///     >>> sandbox.execute("while True: pass")  # raises RuntimeError
    #[pyo3(signature = (max_instructions=None, max_recursion_depth=None))]
    fn set_limits(
        &mut self,
        max_instructions: Option<u64>,
        max_recursion_depth: Option<usize>,
    ) {
        self.inner.set_limits(ResourceLimits {
            max_instructions,
            max_recursion_depth,
        });
    }

    /// Get tool documentation for all registered tools.
    ///
    /// Returns Python-style function signatures and docstrings.
    fn describe_tools(&self) -> String {
        self.inner.describe_tools()
    }
}

// ============================================================================
// WasmSandbox wrapper
// ============================================================================

/// Configuration for the WASM sandbox.
#[pyclass]
#[derive(Clone)]
struct WasmSandboxConfig {
    inner: RustWasmSandboxConfig,
}

#[pymethods]
impl WasmSandboxConfig {
    /// Create a new configuration with default values.
    ///
    /// Defaults:
    ///     - fuel: 10,000,000 (computation units)
    ///     - max_memory: 64MB
    #[new]
    fn new() -> Self {
        Self {
            inner: RustWasmSandboxConfig::default(),
        }
    }

    /// Set the fuel limit (computation units).
    ///
    /// When fuel runs out, execution stops with an error.
    /// This prevents infinite loops.
    fn with_fuel(&self, fuel: u64) -> Self {
        Self {
            inner: self.inner.clone().with_fuel(fuel),
        }
    }

    /// Remove the fuel limit (allow unlimited computation).
    fn with_unlimited_fuel(&self) -> Self {
        Self {
            inner: self.inner.clone().with_unlimited_fuel(),
        }
    }

    /// Set the maximum memory in bytes.
    fn with_max_memory(&self, bytes: usize) -> Self {
        Self {
            inner: self.inner.clone().with_max_memory_bytes(bytes),
        }
    }

    /// Remove the memory limit.
    fn with_unlimited_memory(&self) -> Self {
        Self {
            inner: self.inner.clone().with_unlimited_memory(),
        }
    }
}

/// A WASM-sandboxed Python execution environment.
///
/// This provides stronger isolation than the regular Sandbox by running
/// the Python interpreter inside a WebAssembly sandbox. This prevents
/// any access to the host system.
///
/// Note: WasmSandbox is not thread-safe and must be used from a single thread.
///
/// Example:
///     >>> from littrs import WasmSandbox
///     >>> sandbox = WasmSandbox()
///     >>> sandbox.execute("2 + 2")
///     4
#[pyclass(unsendable)]
struct WasmSandbox {
    inner: RustWasmSandbox,
}

#[pymethods]
impl WasmSandbox {
    /// Create a new WASM sandbox with default configuration.
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(config: Option<&WasmSandboxConfig>) -> PyResult<Self> {
        let inner = match config {
            Some(c) => RustWasmSandbox::with_config(c.inner.clone()),
            None => RustWasmSandbox::new(),
        };
        inner
            .map(|inner| Self { inner })
            .map_err(|e: WasmError| PyRuntimeError::new_err(e.to_string()))
    }

    /// Execute Python code in the WASM sandbox.
    ///
    /// Returns the value of the last expression.
    fn execute(&mut self, py: Python<'_>, code: &str) -> PyResult<PyObject> {
        match self.inner.execute(code) {
            Ok(value) => Ok(pyvalue_to_py(py, &value)),
            Err(e) => Err(PyRuntimeError::new_err(format!("{}", e))),
        }
    }

    /// Set a variable in the sandbox's global scope.
    fn set_variable(&mut self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let pyvalue = py_to_pyvalue(value)?;
        self.inner
            .set_variable(name, pyvalue)
            .map_err(|e: WasmError| PyRuntimeError::new_err(e.to_string()))
    }

    /// Register a Python callable as a tool in the WASM sandbox.
    fn register_function(&mut self, py: Python<'_>, name: &str, func: PyObject) -> PyResult<()> {
        if !func.bind(py).is_callable() {
            return Err(PyTypeError::new_err("func must be callable"));
        }

        let func = func.clone_ref(py);
        self.inner
            .register_fn(name, move |args: Vec<PyValue>| {
                Python::with_gil(|py| {
                    let py_args: Vec<PyObject> = args.iter().map(|v| pyvalue_to_py(py, v)).collect();
                    match func.call1(py, (py_args,)) {
                        Ok(result) => py_to_pyvalue(result.bind(py)).unwrap_or(PyValue::None),
                        Err(e) => PyValue::Dict(vec![(
                            "error".to_string(),
                            PyValue::Str(format!("{}", e)),
                        )]),
                    }
                })
            })
            .map_err(|e: WasmError| PyRuntimeError::new_err(e.to_string()))
    }

    /// Reset the sandbox state (clears variables but keeps tools).
    fn reset(&mut self) -> PyResult<()> {
        self.inner
            .reset()
            .map_err(|e: WasmError| PyRuntimeError::new_err(e.to_string()))
    }

    /// Get remaining fuel. Returns None if fuel tracking is disabled.
    fn remaining_fuel(&self) -> Option<u64> {
        self.inner.remaining_fuel()
    }

    /// Get current memory usage in bytes.
    fn memory_usage(&self) -> usize {
        self.inner.memory_usage()
    }
}

// ============================================================================
// Module definition
// ============================================================================

/// littrs - A minimal, secure Python sandbox for AI agents.
///
/// This module provides two sandbox implementations:
///
/// - `Sandbox`: Fast, in-process sandbox with basic isolation
/// - `WasmSandbox`: Stronger isolation via WebAssembly (recommended for untrusted code)
///
/// Example:
///     >>> from littrs import Sandbox, WasmSandbox
///
///     >>> # Simple sandbox
///     >>> sandbox = Sandbox()
///     >>> sandbox.execute("x = 10")
///     >>> sandbox.execute("x * 2")
///     20
///
///     >>> # WASM sandbox with fuel limit
///     >>> from littrs import WasmSandboxConfig
///     >>> config = WasmSandboxConfig().with_fuel(1000000)
///     >>> wasm = WasmSandbox(config)
///     >>> wasm.execute("sum(range(100))")
///     4950
#[pymodule]
fn littrs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Sandbox>()?;
    m.add_class::<WasmSandbox>()?;
    m.add_class::<WasmSandboxConfig>()?;
    Ok(())
}
