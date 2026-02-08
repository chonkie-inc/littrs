//! WASM guest module for Littrs sandbox.
//!
//! This module is compiled to WebAssembly and provides the sandboxed
//! Python execution environment. It exports functions for the host
//! to interact with and imports functions for tool calls.

use std::cell::RefCell;

use littrs::{PyValue, Sandbox};

/// Result of an execution, serialized as JSON.
#[derive(serde::Serialize)]
#[serde(tag = "type")]
enum ExecuteResult {
    #[serde(rename = "ok")]
    Ok { value: PyValue },
    #[serde(rename = "error")]
    Error { message: String },
}

/// Request to call a tool, sent to host.
#[derive(serde::Serialize)]
struct ToolCallRequest<'a> {
    name: &'a str,
    args: &'a [PyValue],
}

/// Response from a tool call.
#[derive(serde::Deserialize)]
struct ToolCallResponse {
    value: PyValue,
}

// Thread-local storage for the sandbox and result buffer
thread_local! {
    static SANDBOX: RefCell<Option<Sandbox>> = const { RefCell::new(None) };
    static RESULT_BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static REGISTERED_TOOLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

// Import host function for tool calls
unsafe extern "C" {
    /// Call a tool on the host.
    /// - request_ptr: pointer to JSON request
    /// - request_len: length of request
    /// - response_ptr: pointer to write response
    /// - response_capacity: capacity of response buffer
    ///
    /// Returns: length of response written, or negative on error
    safe fn host_call_tool(
        request_ptr: *const u8,
        request_len: usize,
        response_ptr: *mut u8,
        response_capacity: usize,
    ) -> i32;
}

/// Allocate memory for the host to write into.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Deallocate memory.
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

/// Initialize the sandbox. Call this before execute.
#[unsafe(no_mangle)]
pub extern "C" fn init() {
    SANDBOX.with(|s| {
        *s.borrow_mut() = Some(Sandbox::new());
    });
}

/// Register a tool name. The actual implementation is on the host.
/// - name_ptr: pointer to tool name string
/// - name_len: length of tool name
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn register_tool(name_ptr: *const u8, name_len: usize) {
    let name = unsafe {
        let slice = std::slice::from_raw_parts(name_ptr, name_len);
        String::from_utf8_lossy(slice).into_owned()
    };

    let tool_name = name.clone();

    SANDBOX.with(|s| {
        if let Some(ref mut sandbox) = *s.borrow_mut() {
            sandbox.register_fn(&name, move |args: Vec<PyValue>| {
                call_host_tool(&tool_name, &args)
            });
        }
    });

    REGISTERED_TOOLS.with(|t| {
        t.borrow_mut().push(name);
    });
}

/// Call a tool on the host.
fn call_host_tool(name: &str, args: &[PyValue]) -> PyValue {
    let request = ToolCallRequest { name, args };
    let request_json = serde_json::to_vec(&request).unwrap_or_default();

    // Allocate response buffer
    let mut response_buf = vec![0u8; 64 * 1024]; // 64KB response buffer

    let response_len = host_call_tool(
        request_json.as_ptr(),
        request_json.len(),
        response_buf.as_mut_ptr(),
        response_buf.len(),
    );

    if response_len < 0 {
        return PyValue::None;
    }

    response_buf.truncate(response_len as usize);

    serde_json::from_slice::<ToolCallResponse>(&response_buf)
        .map(|r| r.value)
        .unwrap_or(PyValue::None)
}

/// Set a variable in the sandbox.
/// - name_ptr: pointer to variable name
/// - name_len: length of name
/// - value_ptr: pointer to JSON-encoded value
/// - value_len: length of value
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn set_variable(
    name_ptr: *const u8,
    name_len: usize,
    value_ptr: *const u8,
    value_len: usize,
) {
    let name = unsafe {
        let slice = std::slice::from_raw_parts(name_ptr, name_len);
        String::from_utf8_lossy(slice).into_owned()
    };

    let value_json = unsafe { std::slice::from_raw_parts(value_ptr, value_len) };

    let value: PyValue = serde_json::from_slice(value_json).unwrap_or(PyValue::None);

    SANDBOX.with(|s| {
        if let Some(ref mut sandbox) = *s.borrow_mut() {
            sandbox.set(name, value);
        }
    });
}

/// Execute Python code.
/// - code_ptr: pointer to code string
/// - code_len: length of code string
///
/// Returns: pointer to result JSON (use get_result_len for length)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn execute(code_ptr: *const u8, code_len: usize) -> *const u8 {
    let code = unsafe {
        let slice = std::slice::from_raw_parts(code_ptr, code_len);
        std::str::from_utf8_unchecked(slice)
    };

    let result = SANDBOX.with(|s| {
        let mut borrowed = s.borrow_mut();
        let sandbox = borrowed.as_mut().expect("Sandbox not initialized");
        sandbox.run(code)
    });

    let execute_result = match result {
        Ok(value) => ExecuteResult::Ok { value },
        Err(e) => ExecuteResult::Error {
            message: e.to_string(),
        },
    };

    let json = serde_json::to_vec(&execute_result).unwrap_or_default();

    RESULT_BUFFER.with(|buf| {
        *buf.borrow_mut() = json;
        buf.borrow().as_ptr()
    })
}

/// Get the length of the last result.
#[unsafe(no_mangle)]
pub extern "C" fn get_result_len() -> usize {
    RESULT_BUFFER.with(|buf| buf.borrow().len())
}

/// Reset the sandbox state.
#[unsafe(no_mangle)]
pub extern "C" fn reset() {
    SANDBOX.with(|s| {
        *s.borrow_mut() = Some(Sandbox::new());
    });

    // Re-register tools
    let tools: Vec<String> = REGISTERED_TOOLS.with(|t| t.borrow().clone());
    for name in tools {
        let tool_name = name.clone();
        SANDBOX.with(|s| {
            if let Some(ref mut sandbox) = *s.borrow_mut() {
                sandbox.register_fn(&name, move |args: Vec<PyValue>| {
                    call_host_tool(&tool_name, &args)
                });
            }
        });
    }
}
