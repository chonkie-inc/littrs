//! Python bindings for littrs - a minimal, secure Python sandbox.
//!
//! This module provides Python access to the littrs sandbox, allowing
//! secure execution of untrusted Python code with tool registration.

use ::littrs::{
    Limits, PyValue, Sandbox as RustSandbox, WasmError, WasmSandbox as RustWasmSandbox,
    WasmSandboxConfig as RustWasmSandboxConfig,
};
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyFrozenSet, PyInt, PyList, PySet, PyString, PyTuple};

// ============================================================================
// PyValue conversion
// ============================================================================

/// Convert a littrs::PyValue to a Python object.
fn pyvalue_to_py(py: Python<'_>, value: &PyValue) -> PyObject {
    match value {
        PyValue::None => py.None(),
        PyValue::Bool(b) => b.into_py_any(py).unwrap(),
        PyValue::Int(i) => i.into_py_any(py).unwrap(),
        PyValue::Float(f) => f.into_py_any(py).unwrap(),
        PyValue::Str(s) => s.into_py_any(py).unwrap(),
        PyValue::List(items) => {
            let list: Vec<PyObject> = items.iter().map(|v| pyvalue_to_py(py, v)).collect();
            list.into_py_any(py).unwrap()
        }
        PyValue::Tuple(items) => {
            let elements: Vec<PyObject> = items.iter().map(|v| pyvalue_to_py(py, v)).collect();
            PyTuple::new(py, &elements).unwrap().into_any().unbind()
        }
        PyValue::Dict(pairs) => {
            let dict = PyDict::new(py);
            for (k, v) in pairs {
                dict.set_item(pyvalue_to_py(py, k), pyvalue_to_py(py, v))
                    .unwrap();
            }
            dict.into_any().unbind()
        }
        PyValue::Set(items) => {
            let elements: Vec<PyObject> = items.iter().map(|v| pyvalue_to_py(py, v)).collect();
            PySet::new(py, &elements).unwrap().into_any().unbind()
        }
        PyValue::Function(f) => {
            if f.name == "<lambda>" {
                "<function <lambda>>".into_py_any(py).unwrap()
            } else {
                format!("<function {}>", f.name).into_py_any(py).unwrap()
            }
        }
        PyValue::Module { name, .. } => format!("<module '{}'>", name)
            .into_py_any(py)
            .unwrap(),
        PyValue::NativeFunction(key) => format!("<built-in function {}>", key)
            .into_py_any(py)
            .unwrap(),
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
    } else if let Ok(tuple) = obj.downcast::<PyTuple>() {
        let items: PyResult<Vec<_>> = tuple.iter().map(|item| py_to_pyvalue(&item)).collect();
        Ok(PyValue::Tuple(items?))
    } else if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut pairs = Vec::new();
        for (k, v) in dict.iter() {
            pairs.push((py_to_pyvalue(&k)?, py_to_pyvalue(&v)?));
        }
        Ok(PyValue::Dict(pairs))
    } else if let Ok(set) = obj.downcast::<PySet>() {
        let items: PyResult<Vec<_>> = set.iter().map(|item| py_to_pyvalue(&item)).collect();
        Ok(PyValue::Set(items?))
    } else if let Ok(fset) = obj.downcast::<PyFrozenSet>() {
        let items: PyResult<Vec<_>> = fset.iter().map(|item| py_to_pyvalue(&item)).collect();
        Ok(PyValue::Set(items?))
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
///     >>> sandbox.run("2 + 2")
///     4
///     >>> sandbox.run("x = 10")
///     >>> sandbox.run("x * 2")
///     20
#[pyclass]
struct Sandbox {
    inner: RustSandbox,
}

#[pymethods]
impl Sandbox {
    /// Create a new sandbox instance.
    ///
    /// Args:
    ///     builtins: If True, pre-register built-in modules (json, math, typing).
    ///         Defaults to False.
    ///
    /// Example:
    ///     >>> sandbox = Sandbox(builtins=True)
    ///     >>> sandbox.run("import math; math.sqrt(16.0)")
    ///     4.0
    #[new]
    #[pyo3(signature = (builtins=false))]
    fn new(builtins: bool) -> Self {
        Self {
            inner: if builtins {
                RustSandbox::with_builtins()
            } else {
                RustSandbox::new()
            },
        }
    }

    /// Run Python code in the sandbox.
    ///
    /// Returns the value of the last expression, or None if the code
    /// ends with a statement.
    ///
    /// Args:
    ///     code: The Python code to run.
    ///
    /// Returns:
    ///     The result of the last expression.
    ///
    /// Raises:
    ///     RuntimeError: If execution fails (syntax error, runtime error, etc.)
    fn run(&mut self, py: Python<'_>, code: &str) -> PyResult<PyObject> {
        match self.inner.run(code) {
            Ok(value) => Ok(pyvalue_to_py(py, &value)),
            Err(e) => Err(PyRuntimeError::new_err(format!("{}", e))),
        }
    }

    /// Run Python code and capture print output.
    ///
    /// Returns a tuple of (result, printed_lines).
    ///
    /// Args:
    ///     code: The Python code to run.
    ///
    /// Returns:
    ///     A tuple of (result, list of printed lines).
    fn capture(&mut self, py: Python<'_>, code: &str) -> PyResult<(PyObject, Vec<String>)> {
        match self.inner.capture(code) {
            Ok(output) => Ok((pyvalue_to_py(py, &output.value), output.output)),
            Err(e) => Err(PyRuntimeError::new_err(format!("{}", e))),
        }
    }

    /// Set a variable in the sandbox's global scope.
    ///
    /// Args:
    ///     name: The variable name.
    ///     value: The value to set (must be a basic Python type).
    fn set(&mut self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let pyvalue = py_to_pyvalue(value)?;
        self.inner.set(name, pyvalue);
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
    ///     >>> sandbox.register("add", add)
    ///     >>> sandbox.run("add(1, 2)")
    ///     3
    fn register(&mut self, py: Python<'_>, name: &str, func: PyObject) -> PyResult<()> {
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
                            PyValue::Str("error".to_string()),
                            PyValue::Str(format!("{}", e)),
                        )])
                    }
                }
            })
        });

        Ok(())
    }

    /// Register a module that can be imported from sandbox code.
    ///
    /// The module is defined by a dict mapping attribute names to values
    /// or callables. Callables become module functions; other values become
    /// constants.
    ///
    /// Args:
    ///     name: The module name (used in `import name`).
    ///     attrs: A dict of attribute names to values or callables.
    ///
    /// Example:
    ///     >>> sandbox = Sandbox()
    ///     >>> sandbox.module("mymod", {
    ///     ...     "VERSION": "1.0",
    ///     ...     "double": lambda args: args[0] * 2,
    ///     ... })
    ///     >>> sandbox.run("import mymod; mymod.double(21)")
    ///     42
    fn module(&mut self, _py: Python<'_>, name: &str, attrs: &Bound<'_, PyDict>) -> PyResult<()> {
        // Separate callables (functions) from constants
        let mut constants: Vec<(String, PyValue)> = Vec::new();
        let mut functions: Vec<(String, PyObject)> = Vec::new();

        for (k, v) in attrs.iter() {
            let attr_name = k.extract::<String>()?;
            if v.is_callable() {
                functions.push((attr_name, v.unbind()));
            } else {
                constants.push((attr_name, py_to_pyvalue(&v)?));
            }
        }

        self.inner.module(name, |m| {
            for (attr_name, value) in constants {
                m.constant(&attr_name, value);
            }
            for (attr_name, func) in functions {
                m.function(&attr_name, move |args: Vec<PyValue>| {
                    Python::with_gil(|py| {
                        let py_args: Vec<PyObject> =
                            args.iter().map(|v| pyvalue_to_py(py, v)).collect();
                        match func.call1(py, (py_args,)) {
                            Ok(result) => {
                                py_to_pyvalue(result.bind(py)).unwrap_or(PyValue::None)
                            }
                            Err(e) => PyValue::Dict(vec![(
                                PyValue::Str("error".to_string()),
                                PyValue::Str(format!("{}", e)),
                            )]),
                        }
                    })
                });
            }
        });

        Ok(())
    }

    /// Set resource limits for sandbox execution.
    ///
    /// Limits are enforced per run() call. The instruction counter
    /// resets at the start of each execution.
    ///
    /// Args:
    ///     max_instructions: Maximum bytecode instructions per run() call.
    ///         None means unlimited.
    ///     max_recursion_depth: Maximum call-stack depth for user-defined functions.
    ///         None means unlimited.
    ///
    /// Example:
    ///     >>> sandbox = Sandbox()
    ///     >>> sandbox.limit(max_instructions=1000)
    ///     >>> sandbox.run("while True: pass")  # raises RuntimeError
    #[pyo3(signature = (max_instructions=None, max_recursion_depth=None))]
    fn limit(&mut self, max_instructions: Option<u64>, max_recursion_depth: Option<usize>) {
        self.inner.limit(Limits {
            max_instructions,
            max_recursion_depth,
        });
    }

    /// Get tool documentation for all registered tools.
    ///
    /// Returns Python-style function signatures and docstrings.
    fn describe(&self) -> String {
        self.inner.describe()
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
///     >>> sandbox.run("2 + 2")
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

    /// Run Python code in the WASM sandbox.
    ///
    /// Returns the value of the last expression.
    fn run(&mut self, py: Python<'_>, code: &str) -> PyResult<PyObject> {
        match self.inner.run(code) {
            Ok(value) => Ok(pyvalue_to_py(py, &value)),
            Err(e) => Err(PyRuntimeError::new_err(format!("{}", e))),
        }
    }

    /// Set a variable in the sandbox's global scope.
    fn set(&mut self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let pyvalue = py_to_pyvalue(value)?;
        self.inner
            .set(name, pyvalue)
            .map_err(|e: WasmError| PyRuntimeError::new_err(e.to_string()))
    }

    /// Register a Python callable as a tool in the WASM sandbox.
    fn register(&mut self, py: Python<'_>, name: &str, func: PyObject) -> PyResult<()> {
        if !func.bind(py).is_callable() {
            return Err(PyTypeError::new_err("func must be callable"));
        }

        let func = func.clone_ref(py);
        self.inner
            .register_fn(name, move |args: Vec<PyValue>| {
                Python::with_gil(|py| {
                    let py_args: Vec<PyObject> =
                        args.iter().map(|v| pyvalue_to_py(py, v)).collect();
                    match func.call1(py, (py_args,)) {
                        Ok(result) => py_to_pyvalue(result.bind(py)).unwrap_or(PyValue::None),
                        Err(e) => PyValue::Dict(vec![(
                            PyValue::Str("error".to_string()),
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
///     >>> sandbox.run("x = 10")
///     >>> sandbox.run("x * 2")
///     20
///
///     >>> # WASM sandbox with fuel limit
///     >>> from littrs import WasmSandboxConfig
///     >>> config = WasmSandboxConfig().with_fuel(1000000)
///     >>> wasm = WasmSandbox(config)
///     >>> wasm.run("sum(range(100))")
///     4950
#[pymodule]
fn littrs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Sandbox>()?;
    m.add_class::<WasmSandbox>()?;
    m.add_class::<WasmSandboxConfig>()?;
    Ok(())
}
