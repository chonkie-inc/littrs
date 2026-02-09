//! Littrs - A minimal, secure Python sandbox for AI agents
//!
//! Littrs provides a safe execution environment for running untrusted Python code.
//! It supports a minimal Python subset focused on the needs of AI agent tool calling.
//!
//! # Features
//!
//! - **Secure by default**: No file system, network, or OS access
//! - **Minimal Python subset**: Variables, control flow, functions, basic types
//! - **Tool registration**: Register Rust functions callable from Python
//! - **Fast startup**: Millisecond-level initialization
//!
//! # Quick Start
//!
//! ```
//! use littrs::{Sandbox, PyValue};
//!
//! // Create a sandbox
//! let mut sandbox = Sandbox::new();
//!
//! // Register a tool
//! sandbox.register_fn("fetch_data", |args| {
//!     let id = args[0].as_int().unwrap_or(0);
//!     PyValue::Dict(vec![
//!         (PyValue::Str("id".to_string()), PyValue::Int(id)),
//!         (PyValue::Str("name".to_string()), PyValue::Str("Example".to_string())),
//!     ])
//! });
//!
//! // Execute code
//! let result = sandbox.run(r#"
//! data = fetch_data(42)
//! data
//! "#).unwrap();
//! ```
//!
//! # Supported Python Features
//!
//! ## Types
//! - `None`, `bool`, `int`, `float`, `str`
//! - `list`, `dict` (string keys only)
//!
//! ## Operators
//! - Arithmetic: `+`, `-`, `*`, `/`, `//`, `%`, `**`
//! - Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`
//! - Boolean: `and`, `or`, `not`
//! - Bitwise: `|`, `^`, `&`, `<<`, `>>`
//!
//! ## Control Flow
//! - `if`/`elif`/`else`
//! - `for` loops (over lists and strings) with `break`/`continue`
//! - `while` loops with `break`/`continue`
//!
//! ## Functions
//! - `def` with positional parameters, default values, `*args`, `**kwargs`
//! - Recursive calls
//! - Nested function definitions
//!
//! ## Error Handling
//! - `try`/`except` with typed handlers and `as` binding
//! - `raise` with exception type and message
//! - `else` clause on try blocks
//!
//! ## Resource Limits
//! - Configurable instruction count limit (prevents infinite loops)
//! - Configurable recursion depth limit
//!
//! ## Built-in Functions
//! - `len()`, `str()`, `int()`, `float()`, `bool()`, `list()`
//! - `range()`, `abs()`, `min()`, `max()`, `sum()`
//! - `print()` (output captured via `capture()`)
//!
//! ## Imports
//! - `import module` / `import module as alias`
//! - `from module import name` / `from module import name as alias`
//! - Built-in modules: `json` (loads/dumps), `math` (constants + functions), `typing`
//! - Custom module registration via [`Sandbox::module`]
//!
//! # Not Supported
//!
//! - Class definitions
//! - Async/await
//! - `finally` blocks
//! - File I/O
//! - Relative imports

mod builtins;
mod bytecode;
mod compiler;
mod diagnostic;
mod error;
mod methods;
pub(crate) mod modules;
mod operators;
mod sandbox;
mod slice;
mod tool;
mod value;
mod vm;

#[cfg(feature = "wasm")]
mod wasm_error;
#[cfg(feature = "wasm")]
mod wasm_sandbox;

pub use diagnostic::{Diagnostic, FunctionCallDiagnostic, Label, Span};
pub use error::{Error, Result};
pub use sandbox::{Limits, ModuleBuilder, Output, Sandbox};
pub use tool::{ArgInfo, Tool, ToolCallError, ToolInfo};
pub use value::{FromPyValue, PyValue, TypeError};

// Re-export the macro when the macros feature is enabled
#[cfg(feature = "macros")]
pub use littrs_macros::tool;

#[cfg(feature = "wasm")]
pub use wasm_error::{Error as WasmError, Result as WasmResult};
#[cfg(feature = "wasm")]
pub use wasm_sandbox::{WasmSandbox, WasmSandboxConfig};
