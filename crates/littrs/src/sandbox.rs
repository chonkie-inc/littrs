use std::collections::HashMap;
use std::sync::Arc;

use crate::compiler::Compiler;
use crate::error::Result;
use crate::tool::ToolInfo;
use crate::value::PyValue;
use crate::vm::{ToolFn, Vm};

/// Builder for constructing modules that can be imported from Python code.
///
/// # Example
///
/// ```
/// use littrs::{Sandbox, ModuleBuilder, PyValue};
///
/// let mut sandbox = Sandbox::new();
///
/// sandbox.module("mymod", |m| {
///     m.constant("VERSION", PyValue::Str("1.0".to_string()));
///     m.function("double", |args| {
///         let x = args.get(0).and_then(|v| v.as_int()).unwrap_or(0);
///         PyValue::Int(x * 2)
///     });
/// });
///
/// let result = sandbox.run("import mymod; mymod.double(21)").unwrap();
/// assert_eq!(result, PyValue::Int(42));
/// ```
pub struct ModuleBuilder {
    module_name: String,
    attrs: Vec<(String, PyValue)>,
    tools: Vec<(String, ToolFn)>,
}

impl ModuleBuilder {
    fn new(module_name: &str) -> Self {
        Self {
            module_name: module_name.to_string(),
            attrs: Vec::new(),
            tools: Vec::new(),
        }
    }

    /// Register a constant value as a module attribute.
    pub fn constant(&mut self, name: &str, value: PyValue) {
        self.attrs.push((name.to_string(), value));
    }

    /// Register a native function as a module attribute.
    ///
    /// The function will be callable as `module.function_name(args)`.
    pub fn function<F>(&mut self, name: &str, f: F)
    where
        F: Fn(Vec<PyValue>) -> PyValue + Send + Sync + 'static,
    {
        let tool_key = format!("__mod_{}__{}", self.module_name, name);
        self.attrs
            .push((name.to_string(), PyValue::NativeFunction(tool_key.clone())));
        self.tools.push((tool_key, Arc::new(f) as ToolFn));
    }
}

/// A secure Python sandbox for executing untrusted code.
///
/// The sandbox provides a minimal Python subset that can execute code
/// safely without access to the file system, network, or other resources.
///
/// # Example
///
/// ```
/// use littrs::{Sandbox, PyValue};
///
/// let mut sandbox = Sandbox::new();
///
/// // Register a tool that can be called from Python
/// sandbox.register_fn("add_numbers", |args| {
///     let a = args[0].as_int().unwrap_or(0);
///     let b = args[1].as_int().unwrap_or(0);
///     PyValue::Int(a + b)
/// });
///
/// // Run Python code
/// let result = sandbox.run(r#"
/// x = add_numbers(10, 20)
/// x * 2
/// "#).unwrap();
///
/// assert_eq!(result, PyValue::Int(60));
/// ```
#[derive(Clone)]
pub struct Sandbox {
    vm: Vm,
    tool_infos: Vec<ToolInfo>,
}

impl Sandbox {
    /// Create a new sandbox instance.
    pub fn new() -> Self {
        Self {
            vm: Vm::new(),
            tool_infos: Vec::new(),
        }
    }

    /// Create a new sandbox with built-in modules (json, math, typing) pre-registered.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, PyValue};
    ///
    /// let mut sandbox = Sandbox::with_builtins();
    /// let result = sandbox.run("import math; math.sqrt(16.0)").unwrap();
    /// assert_eq!(result, PyValue::Float(4.0));
    /// ```
    pub fn with_builtins() -> Self {
        let mut sandbox = Self::new();
        crate::modules::register_builtins(&mut sandbox);
        sandbox
    }

    /// Register a module that can be imported from Python code.
    ///
    /// The builder closure receives a [`ModuleBuilder`] for adding constants
    /// and functions to the module.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, PyValue};
    ///
    /// let mut sandbox = Sandbox::new();
    /// sandbox.module("utils", |m| {
    ///     m.constant("PI", PyValue::Float(3.14));
    ///     m.function("double", |args| {
    ///         let x = args.get(0).and_then(|v| v.as_int()).unwrap_or(0);
    ///         PyValue::Int(x * 2)
    ///     });
    /// });
    ///
    /// let result = sandbox.run("import utils; utils.double(5)").unwrap();
    /// assert_eq!(result, PyValue::Int(10));
    /// ```
    pub fn module<F>(&mut self, name: &str, builder_fn: F)
    where
        F: FnOnce(&mut ModuleBuilder),
    {
        let mut builder = ModuleBuilder::new(name);
        builder_fn(&mut builder);

        // Register all native function tools in the VM
        for (key, func) in builder.tools {
            self.vm.register_tool(key, func);
        }

        // Build the module value and register it
        let module = PyValue::Module {
            name: name.to_string(),
            attrs: builder.attrs,
        };
        self.vm.register_module(name, module);
    }

    /// Register a function that can be called from Python code.
    ///
    /// The function receives a vector of `PyValue` arguments and returns a `PyValue`.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, PyValue};
    ///
    /// let mut sandbox = Sandbox::new();
    ///
    /// sandbox.register_fn("greet", |args| {
    ///     let name = args.get(0)
    ///         .and_then(|v| v.as_str())
    ///         .unwrap_or("World");
    ///     PyValue::Str(format!("Hello, {}!", name))
    /// });
    ///
    /// let result = sandbox.run("greet('Alice')").unwrap();
    /// assert_eq!(result, PyValue::Str("Hello, Alice!".to_string()));
    /// ```
    pub fn register_fn<F>(&mut self, name: impl Into<String>, f: F)
    where
        F: Fn(Vec<PyValue>) -> PyValue + Send + Sync + 'static,
    {
        self.vm.register_tool(name, Arc::new(f) as ToolFn);
    }

    /// Register a tool with metadata that can be called from Python code.
    ///
    /// The metadata is used to generate Python documentation for the LLM's
    /// system prompt via [`describe`](Self::describe).
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, PyValue, ToolInfo};
    ///
    /// let mut sandbox = Sandbox::new();
    ///
    /// let info = ToolInfo::new("fetch_weather", "Get weather for a city")
    ///     .arg("city", "str", "The city name")
    ///     .arg_opt("unit", "str", "Temperature unit")
    ///     .returns("dict");
    ///
    /// sandbox.register_tool(info, |args| {
    ///     let city = args.get(0).and_then(|v| v.as_str()).unwrap_or("Unknown");
    ///     PyValue::Dict(vec![
    ///         (PyValue::Str("city".to_string()), PyValue::Str(city.to_string())),
    ///         (PyValue::Str("temp".to_string()), PyValue::Int(22)),
    ///     ])
    /// });
    ///
    /// // The tool is now callable from Python
    /// let result = sandbox.run("fetch_weather('Paris')").unwrap();
    ///
    /// // And documented for LLMs
    /// let docs = sandbox.describe();
    /// assert!(docs.contains("fetch_weather"));
    /// ```
    pub fn register_tool<F>(&mut self, info: ToolInfo, f: F)
    where
        F: Fn(Vec<PyValue>) -> PyValue + Send + Sync + 'static,
    {
        self.vm
            .register_tool_with_info(info.clone(), Arc::new(f) as ToolFn);
        self.tool_infos.push(info);
    }

    /// Register a tool using the [`Tool`](crate::Tool) trait.
    ///
    /// This is the most ergonomic way to register tools created with the
    /// `#[tool]` macro.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use littrs::Sandbox;
    /// use littrs_macros::tool;
    ///
    /// #[tool(description = "Add two numbers.")]
    /// fn add(a: i64, b: i64) -> i64 { a + b }
    ///
    /// let mut sandbox = Sandbox::new();
    /// sandbox.add(add::Tool);  // Ergonomic!
    ///
    /// let result = sandbox.run("add(2, 3)").unwrap();
    /// ```
    pub fn add<T: crate::tool::Tool + 'static>(&mut self, _: T) {
        let info = T::info().clone();
        self.vm
            .register_tool_with_info(info.clone(), Arc::new(T::call) as ToolFn);
        self.tool_infos.push(info);
    }

    /// Generate Python documentation for all registered tools.
    ///
    /// This is suitable for embedding in an LLM's system prompt for
    /// CodeAct-style agents.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, PyValue, ToolInfo};
    ///
    /// let mut sandbox = Sandbox::new();
    ///
    /// sandbox.register_tool(
    ///     ToolInfo::new("get_time", "Get the current time").returns("str"),
    ///     |_| PyValue::Str("12:00".to_string()),
    /// );
    ///
    /// sandbox.register_tool(
    ///     ToolInfo::new("add", "Add two numbers")
    ///         .arg("a", "int", "First number")
    ///         .arg("b", "int", "Second number")
    ///         .returns("int"),
    ///     |args| {
    ///         let a = args.get(0).and_then(|v| v.as_int()).unwrap_or(0);
    ///         let b = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
    ///         PyValue::Int(a + b)
    ///     },
    /// );
    ///
    /// let docs = sandbox.describe();
    /// assert!(docs.contains("def get_time() -> str:"));
    /// assert!(docs.contains("def add(a: int, b: int) -> int:"));
    /// ```
    pub fn describe(&self) -> String {
        crate::tool::describe_tools(&self.tool_infos)
    }

    /// Get the tool infos for all registered tools.
    pub fn tools(&self) -> &[ToolInfo] {
        &self.tool_infos
    }

    /// Set a variable in the sandbox's global scope.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, PyValue};
    ///
    /// let mut sandbox = Sandbox::new();
    /// sandbox.set("x", PyValue::Int(42));
    ///
    /// let result = sandbox.run("x * 2").unwrap();
    /// assert_eq!(result, PyValue::Int(84));
    /// ```
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<PyValue>) {
        self.vm.set_variable(name, value.into());
    }

    /// Run Python code in the sandbox.
    ///
    /// Returns the value of the last expression, or `PyValue::None` if the
    /// code ends with a statement.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The code has a syntax error
    /// - A runtime error occurs (undefined variable, type error, etc.)
    /// - Unsupported Python features are used
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, PyValue};
    ///
    /// let mut sandbox = Sandbox::new();
    ///
    /// // Simple expression
    /// let result = sandbox.run("2 + 2").unwrap();
    /// assert_eq!(result, PyValue::Int(4));
    ///
    /// // Multi-line code
    /// let result = sandbox.run(r#"
    /// total = 0
    /// for i in range(10):
    ///     total = total + i
    /// total
    /// "#).unwrap();
    /// assert_eq!(result, PyValue::Int(45));
    /// ```
    pub fn run(&mut self, code: &str) -> Result<PyValue> {
        let code_obj = Compiler::compile(code)?;
        self.vm.execute(code_obj)
    }

    /// Run Python code and capture print output.
    ///
    /// Returns both the result value and any output from print() calls.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::Sandbox;
    ///
    /// let mut sandbox = Sandbox::new();
    ///
    /// let output = sandbox.capture(r#"
    /// x = 10
    /// print("x is", x)
    /// x * 2
    /// "#).unwrap();
    ///
    /// assert_eq!(output.output, vec!["x is 10"]);
    /// assert_eq!(output.value.as_int(), Some(20));
    /// ```
    pub fn capture(&mut self, code: &str) -> Result<Output> {
        // Clear any previous print output
        self.vm.clear_print_buffer();

        // Execute the code
        let code_obj = Compiler::compile(code)?;
        let value = self.vm.execute(code_obj)?;

        // Capture print output
        let output = self.vm.take_print_output();

        Ok(Output { value, output })
    }

    /// Set resource limits for sandbox execution.
    ///
    /// Limits are enforced per `run()` call. The instruction counter
    /// resets at the start of each execution.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, Limits};
    ///
    /// let mut sandbox = Sandbox::new();
    /// sandbox.limit(Limits {
    ///     max_instructions: Some(1_000),
    ///     max_recursion_depth: Some(10),
    /// });
    ///
    /// // This will fail with InstructionLimitExceeded
    /// let err = sandbox.run("while True: pass").unwrap_err();
    /// assert!(err.to_string().contains("Instruction limit"));
    /// ```
    pub fn limit(&mut self, limits: Limits) {
        self.vm
            .set_limits(limits.max_instructions, limits.max_recursion_depth);
    }

    /// Take and clear any accumulated print output.
    ///
    /// This is useful if you want to check what was printed after
    /// calling `run()` multiple times.
    pub fn flush(&mut self) -> Vec<String> {
        self.vm.take_print_output()
    }

    /// Mount a virtual file visible to sandbox code.
    ///
    /// The file content is read from `host_path` at mount time. If `writable`
    /// is true, sandbox code can open the file in write mode and writes will
    /// be persisted back to `host_path`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use littrs::Sandbox;
    ///
    /// let mut sandbox = Sandbox::new();
    /// sandbox.mount("data.json", "./data/input.json", false);
    /// sandbox.mount("output.txt", "./output/result.txt", true);
    /// ```
    pub fn mount(
        &mut self,
        virtual_path: impl Into<String>,
        host_path: impl Into<String>,
        writable: bool,
    ) {
        let host = host_path.into();
        let content = std::fs::read_to_string(&host).unwrap_or_default();
        self.vm
            .mount(virtual_path.into(), host, writable, content);
    }

    /// Get current contents of all writable mounted files.
    ///
    /// Returns a map from virtual path to current file content.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use littrs::Sandbox;
    ///
    /// let mut sandbox = Sandbox::new();
    /// sandbox.mount("output.txt", "./output.txt", true);
    /// sandbox.run(r#"
    /// f = open("output.txt", "w")
    /// f.write("hello")
    /// f.close()
    /// "#).unwrap();
    /// let files = sandbox.files();
    /// assert_eq!(files.get("output.txt").unwrap(), "hello");
    /// ```
    pub fn files(&self) -> HashMap<String, String> {
        self.vm.get_writable_files()
    }
}

/// Result of running code with print output capture.
#[derive(Debug, Clone)]
pub struct Output {
    /// The result value of the last expression.
    pub value: PyValue,
    /// Lines printed via print() calls.
    pub output: Vec<String>,
}

/// Resource limits for sandbox execution.
///
/// Both limits are optional — `None` means unlimited. Use [`Sandbox::limit`]
/// to apply limits before calling [`Sandbox::run`].
///
/// # Example
///
/// ```
/// use littrs::{Sandbox, Limits};
///
/// let mut sandbox = Sandbox::new();
/// sandbox.limit(Limits {
///     max_instructions: Some(10_000),
///     max_recursion_depth: Some(50),
/// });
/// ```
#[derive(Debug, Clone, Default)]
pub struct Limits {
    /// Maximum number of bytecode instructions per `run()` call.
    /// `None` means unlimited.
    pub max_instructions: Option<u64>,
    /// Maximum call-stack depth for user-defined function calls.
    /// `None` means unlimited.
    pub max_recursion_depth: Option<usize>,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}
