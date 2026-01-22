//! Litter - A minimal, secure Python sandbox for AI agents
//!
//! Litter provides a safe execution environment for running untrusted Python code.
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
//! use litter::{Sandbox, PyValue};
//!
//! // Create a sandbox
//! let mut sandbox = Sandbox::new();
//!
//! // Register a tool
//! sandbox.register_fn("fetch_data", |args| {
//!     let id = args[0].as_int().unwrap_or(0);
//!     PyValue::Dict(vec![
//!         ("id".to_string(), PyValue::Int(id)),
//!         ("name".to_string(), PyValue::Str("Example".to_string())),
//!     ])
//! });
//!
//! // Execute code
//! let result = sandbox.execute(r#"
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
//! - `for` loops (over lists and strings)
//! - `while` loops
//!
//! ## Built-in Functions
//! - `len()`, `str()`, `int()`, `float()`, `bool()`, `list()`
//! - `range()`, `abs()`, `min()`, `max()`, `sum()`
//! - `print()` (no-op in sandbox)
//!
//! # Not Supported
//!
//! - `import` statements
//! - Class definitions
//! - Function definitions (`def`)
//! - Async/await
//! - Comprehensions
//! - Exceptions (try/except)
//! - File I/O
//! - Any standard library

mod error;
mod eval;
mod sandbox;
mod value;

#[cfg(feature = "wasm")]
mod wasm_error;
#[cfg(feature = "wasm")]
mod wasm_sandbox;

pub use error::{Error, Result};
pub use sandbox::Sandbox;
pub use value::PyValue;

#[cfg(feature = "wasm")]
pub use wasm_error::{Error as WasmError, Result as WasmResult};
#[cfg(feature = "wasm")]
pub use wasm_sandbox::{WasmSandbox, WasmSandboxConfig};
