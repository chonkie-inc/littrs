use std::collections::HashMap;
use std::sync::Arc;

use wasmtime::*;
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::WasiCtxBuilder;

use crate::wasm_error::{Error, Result};
use crate::PyValue;

/// Embedded WASM sandbox module.
/// This is the pre-compiled littrs-wasm binary.
const EMBEDDED_WASM: &[u8] = include_bytes!("../wasm/sandbox.wasm");

/// Tool function type.
pub type ToolFn = Arc<dyn Fn(Vec<PyValue>) -> PyValue + Send + Sync>;

/// Configuration for the WASM sandbox.
#[derive(Clone)]
pub struct WasmSandboxConfig {
    /// Maximum fuel (computation units). None = unlimited.
    pub fuel: Option<u64>,
    /// Maximum memory in bytes. None = unlimited.
    pub max_memory_bytes: Option<usize>,
    /// Maximum execution time in milliseconds. None = unlimited.
    pub timeout_ms: Option<u64>,
}

impl Default for WasmSandboxConfig {
    fn default() -> Self {
        Self {
            fuel: Some(10_000_000), // 10M fuel units
            max_memory_bytes: Some(64 * 1024 * 1024), // 64MB
            timeout_ms: None,
        }
    }
}

impl WasmSandboxConfig {
    /// Set the fuel limit.
    pub fn with_fuel(mut self, fuel: u64) -> Self {
        self.fuel = Some(fuel);
        self
    }

    /// Set no fuel limit.
    pub fn with_unlimited_fuel(mut self) -> Self {
        self.fuel = None;
        self
    }

    /// Set the maximum memory in bytes.
    pub fn with_max_memory_bytes(mut self, bytes: usize) -> Self {
        self.max_memory_bytes = Some(bytes);
        self
    }

    /// Set no memory limit.
    pub fn with_unlimited_memory(mut self) -> Self {
        self.max_memory_bytes = None;
        self
    }
}

/// Execution result from WASM module.
#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum ExecuteResult {
    #[serde(rename = "ok")]
    Ok { value: PyValue },
    #[serde(rename = "error")]
    Error { message: String },
}

/// Tool call request from WASM module.
#[derive(serde::Deserialize)]
struct ToolCallRequest {
    name: String,
    args: Vec<PyValue>,
}

/// Tool call response to WASM module.
#[derive(serde::Serialize)]
struct ToolCallResponse {
    value: PyValue,
}

/// A WASM-sandboxed Python execution environment.
pub struct WasmSandbox {
    store: Store<SandboxState>,
    instance: Instance,
    config: WasmSandboxConfig,
}

struct SandboxState {
    tools: HashMap<String, ToolFn>,
    memory: Option<Memory>,
    wasi: WasiP1Ctx,
}

impl WasmSandbox {
    /// Create a new WASM sandbox with default configuration.
    ///
    /// This uses the embedded WASM module, so no external files are needed.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{WasmSandbox, PyValue};
    ///
    /// let mut sandbox = WasmSandbox::new().unwrap();
    /// let result = sandbox.execute("1 + 2").unwrap();
    /// assert_eq!(result, PyValue::Int(3));
    /// ```
    pub fn new() -> Result<Self> {
        Self::with_config(WasmSandboxConfig::default())
    }

    /// Create a new WASM sandbox with custom configuration.
    ///
    /// This uses the embedded WASM module with the specified configuration.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{WasmSandbox, WasmSandboxConfig};
    ///
    /// let config = WasmSandboxConfig::default()
    ///     .with_fuel(1_000_000);
    /// let mut sandbox = WasmSandbox::with_config(config).unwrap();
    /// ```
    pub fn with_config(config: WasmSandboxConfig) -> Result<Self> {
        Self::from_bytes(EMBEDDED_WASM, config)
    }

    /// Create a new WASM sandbox from custom WASM bytes.
    ///
    /// This is useful if you want to use a custom-built WASM module
    /// instead of the embedded one.
    pub fn from_bytes(wasm_bytes: &[u8], config: WasmSandboxConfig) -> Result<Self> {
        let mut engine_config = Config::new();

        // Enable fuel consumption if configured
        if config.fuel.is_some() {
            engine_config.consume_fuel(true);
        }

        let engine = Engine::new(&engine_config)?;

        let module = Module::new(&engine, wasm_bytes)?;

        // Create minimal WASI context (no filesystem, no network, no env)
        let wasi_ctx = WasiCtxBuilder::new().build_p1();

        let state = SandboxState {
            tools: HashMap::new(),
            memory: None,
            wasi: wasi_ctx,
        };

        let mut store = Store::new(&engine, state);

        // Set fuel if configured
        if let Some(fuel) = config.fuel {
            store.set_fuel(fuel)?;
        }

        // Create linker with WASI
        let mut linker = Linker::new(&engine);
        preview1::add_to_linker_sync(&mut linker, |s: &mut SandboxState| &mut s.wasi)?;

        // Add our custom host function for tool calls
        linker.func_wrap(
            "env",
            "host_call_tool",
            |mut caller: Caller<'_, SandboxState>,
             request_ptr: i32,
             request_len: i32,
             response_ptr: i32,
             response_capacity: i32|
             -> i32 {
                let memory = match caller.data().memory {
                    Some(m) => m,
                    None => return -1,
                };

                // Read the request from WASM memory
                let request_bytes = {
                    let data = memory.data(&caller);
                    let start = request_ptr as usize;
                    let end = start + request_len as usize;
                    if end > data.len() {
                        return -1;
                    }
                    data[start..end].to_vec()
                };

                // Parse the request
                let request: ToolCallRequest = match serde_json::from_slice(&request_bytes) {
                    Ok(r) => r,
                    Err(_) => return -1,
                };

                // Call the tool
                let result = {
                    let tools = &caller.data().tools;
                    match tools.get(&request.name) {
                        Some(tool) => tool(request.args),
                        None => PyValue::None,
                    }
                };

                // Serialize the response
                let response = ToolCallResponse { value: result };
                let response_bytes = match serde_json::to_vec(&response) {
                    Ok(b) => b,
                    Err(_) => return -1,
                };

                // Write response to WASM memory
                if response_bytes.len() > response_capacity as usize {
                    return -1;
                }

                {
                    let data = memory.data_mut(&mut caller);
                    let start = response_ptr as usize;
                    let end = start + response_bytes.len();
                    if end > data.len() {
                        return -1;
                    }
                    data[start..end].copy_from_slice(&response_bytes);
                }

                response_bytes.len() as i32
            },
        )?;

        // Instantiate the module
        let instance = linker.instantiate(&mut store, &module)?;

        // Get and store the memory reference
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| Error::Wasm(wasmtime::Error::msg("No memory export")))?;
        store.data_mut().memory = Some(memory);

        // Initialize the sandbox
        let init = instance
            .get_typed_func::<(), ()>(&mut store, "init")
            .map_err(|_| Error::Wasm(wasmtime::Error::msg("No init export")))?;
        init.call(&mut store, ())?;

        Ok(Self {
            store,
            instance,
            config,
        })
    }

    /// Register a tool function that can be called from Python code.
    pub fn register_fn<F>(&mut self, name: impl Into<String>, f: F) -> Result<()>
    where
        F: Fn(Vec<PyValue>) -> PyValue + Send + Sync + 'static,
    {
        let name = name.into();

        // Store the tool in our state
        self.store
            .data_mut()
            .tools
            .insert(name.clone(), Arc::new(f));

        // Tell the WASM module about this tool
        let memory = self.store.data().memory.unwrap();
        let alloc = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")?;
        let register_tool = self
            .instance
            .get_typed_func::<(i32, i32), ()>(&mut self.store, "register_tool")?;

        // Allocate memory for the name
        let name_bytes = name.as_bytes();
        let name_ptr = alloc.call(&mut self.store, name_bytes.len() as i32)?;

        // Write the name to WASM memory
        {
            let data = memory.data_mut(&mut self.store);
            let start = name_ptr as usize;
            data[start..start + name_bytes.len()].copy_from_slice(name_bytes);
        }

        // Register the tool
        register_tool.call(&mut self.store, (name_ptr, name_bytes.len() as i32))?;

        Ok(())
    }

    /// Set a variable in the sandbox.
    pub fn set_variable(&mut self, name: impl Into<String>, value: impl Into<PyValue>) -> Result<()> {
        let name = name.into();
        let value = value.into();

        let memory = self.store.data().memory.unwrap();
        let alloc = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")?;
        let set_variable = self
            .instance
            .get_typed_func::<(i32, i32, i32, i32), ()>(&mut self.store, "set_variable")?;

        // Serialize value to JSON
        let value_json = serde_json::to_vec(&value)?;

        // Allocate memory for name and value
        let name_bytes = name.as_bytes();
        let name_ptr = alloc.call(&mut self.store, name_bytes.len() as i32)?;
        let value_ptr = alloc.call(&mut self.store, value_json.len() as i32)?;

        // Write to WASM memory
        {
            let data = memory.data_mut(&mut self.store);
            let name_start = name_ptr as usize;
            data[name_start..name_start + name_bytes.len()].copy_from_slice(name_bytes);

            let value_start = value_ptr as usize;
            data[value_start..value_start + value_json.len()].copy_from_slice(&value_json);
        }

        // Set the variable
        set_variable.call(
            &mut self.store,
            (
                name_ptr,
                name_bytes.len() as i32,
                value_ptr,
                value_json.len() as i32,
            ),
        )?;

        Ok(())
    }

    /// Execute Python code in the sandbox.
    pub fn execute(&mut self, code: &str) -> Result<PyValue> {
        // Reset fuel if configured
        if let Some(fuel) = self.config.fuel {
            self.store.set_fuel(fuel)?;
        }

        let memory = self.store.data().memory.unwrap();
        let alloc = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")?;
        let execute = self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "execute")?;
        let get_result_len = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "get_result_len")?;

        // Allocate memory for code
        let code_bytes = code.as_bytes();
        let code_ptr = alloc.call(&mut self.store, code_bytes.len() as i32)?;

        // Write code to WASM memory
        {
            let data = memory.data_mut(&mut self.store);
            let start = code_ptr as usize;
            data[start..start + code_bytes.len()].copy_from_slice(code_bytes);
        }

        // Execute the code
        let result_ptr = match execute.call(&mut self.store, (code_ptr, code_bytes.len() as i32)) {
            Ok(ptr) => ptr,
            Err(e) => {
                // Check if it's an out-of-fuel error
                // wasmtime can report this in different ways
                if e.downcast_ref::<wasmtime::Trap>()
                    .map(|t| *t == wasmtime::Trap::OutOfFuel)
                    .unwrap_or(false)
                {
                    return Err(Error::OutOfFuel);
                }
                // Also check remaining fuel
                if self.store.get_fuel().ok() == Some(0) {
                    return Err(Error::OutOfFuel);
                }
                return Err(Error::Wasm(e));
            }
        };

        // Get result length
        let result_len = get_result_len.call(&mut self.store, ())?;

        // Read result from WASM memory
        let result_bytes = {
            let data = memory.data(&self.store);
            let start = result_ptr as usize;
            let end = start + result_len as usize;
            data[start..end].to_vec()
        };

        // Parse the result
        let execute_result: ExecuteResult = serde_json::from_slice(&result_bytes)?;

        match execute_result {
            ExecuteResult::Ok { value } => Ok(value),
            ExecuteResult::Error { message } => Err(Error::Execution(message)),
        }
    }

    /// Reset the sandbox state (clears all variables but keeps registered tools).
    pub fn reset(&mut self) -> Result<()> {
        let reset = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "reset")?;
        reset.call(&mut self.store, ())?;
        Ok(())
    }

    /// Get the remaining fuel. Returns None if fuel tracking is disabled.
    pub fn remaining_fuel(&self) -> Option<u64> {
        self.store.get_fuel().ok()
    }

    /// Get the current memory usage in bytes.
    pub fn memory_usage(&self) -> usize {
        self.store
            .data()
            .memory
            .map(|m| m.data_size(&self.store))
            .unwrap_or(0)
    }
}
