use std::sync::Arc;

use crate::compiler::Compiler;
use crate::error::Result;
use crate::tool::ToolInfo;
use crate::value::PyValue;
use crate::vm::{ToolFn, Vm};

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
/// // Execute Python code
/// let result = sandbox.execute(r#"
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
    /// let result = sandbox.execute("greet('Alice')").unwrap();
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
    /// system prompt via [`describe_tools`](Self::describe_tools).
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, PyValue, ToolInfo};
    ///
    /// let mut sandbox = Sandbox::new();
    ///
    /// let info = ToolInfo::new("fetch_weather", "Get weather for a city")
    ///     .arg_required("city", "str", "The city name")
    ///     .arg_optional("unit", "str", "Temperature unit")
    ///     .returns("dict");
    ///
    /// sandbox.register_tool(info, |args| {
    ///     let city = args.get(0).and_then(|v| v.as_str()).unwrap_or("Unknown");
    ///     PyValue::Dict(vec![
    ///         ("city".to_string(), PyValue::Str(city.to_string())),
    ///         ("temp".to_string(), PyValue::Int(22)),
    ///     ])
    /// });
    ///
    /// // The tool is now callable from Python
    /// let result = sandbox.execute("fetch_weather('Paris')").unwrap();
    ///
    /// // And documented for LLMs
    /// let docs = sandbox.describe_tools();
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

    /// Register a tool using the [`Tool`] trait.
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
    /// sandbox.register(add::Tool);  // Ergonomic!
    ///
    /// let result = sandbox.execute("add(2, 3)").unwrap();
    /// ```
    pub fn register<T: crate::tool::Tool + 'static>(&mut self, _: T) {
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
    ///         .arg_required("a", "int", "First number")
    ///         .arg_required("b", "int", "Second number")
    ///         .returns("int"),
    ///     |args| {
    ///         let a = args.get(0).and_then(|v| v.as_int()).unwrap_or(0);
    ///         let b = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
    ///         PyValue::Int(a + b)
    ///     },
    /// );
    ///
    /// let docs = sandbox.describe_tools();
    /// assert!(docs.contains("def get_time() -> str:"));
    /// assert!(docs.contains("def add(a: int, b: int) -> int:"));
    /// ```
    pub fn describe_tools(&self) -> String {
        crate::tool::describe_tools(&self.tool_infos)
    }

    /// Get the tool infos for all registered tools.
    pub fn tool_infos(&self) -> &[ToolInfo] {
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
    /// sandbox.set_variable("x", PyValue::Int(42));
    ///
    /// let result = sandbox.execute("x * 2").unwrap();
    /// assert_eq!(result, PyValue::Int(84));
    /// ```
    pub fn set_variable(&mut self, name: impl Into<String>, value: impl Into<PyValue>) {
        self.vm.set_variable(name, value.into());
    }

    /// Execute Python code in the sandbox.
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
    /// let result = sandbox.execute("2 + 2").unwrap();
    /// assert_eq!(result, PyValue::Int(4));
    ///
    /// // Multi-line code
    /// let result = sandbox.execute(r#"
    /// total = 0
    /// for i in range(10):
    ///     total = total + i
    /// total
    /// "#).unwrap();
    /// assert_eq!(result, PyValue::Int(45));
    /// ```
    pub fn execute(&mut self, code: &str) -> Result<PyValue> {
        let code_obj = Compiler::compile(code)?;
        self.vm.execute(code_obj)
    }

    /// Execute Python code and capture print output.
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
    /// let output = sandbox.execute_with_output(r#"
    /// x = 10
    /// print("x is", x)
    /// x * 2
    /// "#).unwrap();
    ///
    /// assert_eq!(output.printed, vec!["x is 10"]);
    /// assert_eq!(output.result.as_int(), Some(20));
    /// ```
    pub fn execute_with_output(&mut self, code: &str) -> Result<ExecuteOutput> {
        // Clear any previous print output
        self.vm.clear_print_buffer();

        // Execute the code
        let code_obj = Compiler::compile(code)?;
        let result = self.vm.execute(code_obj)?;

        // Capture print output
        let printed = self.vm.take_print_output();

        Ok(ExecuteOutput { result, printed })
    }

    /// Set resource limits for sandbox execution.
    ///
    /// Limits are enforced per `execute()` call. The instruction counter
    /// resets at the start of each execution.
    ///
    /// # Example
    ///
    /// ```
    /// use littrs::{Sandbox, ResourceLimits};
    ///
    /// let mut sandbox = Sandbox::new();
    /// sandbox.set_limits(ResourceLimits {
    ///     max_instructions: Some(1_000),
    ///     max_recursion_depth: Some(10),
    /// });
    ///
    /// // This will fail with InstructionLimitExceeded
    /// let err = sandbox.execute("while True: pass").unwrap_err();
    /// assert!(err.to_string().contains("Instruction limit"));
    /// ```
    pub fn set_limits(&mut self, limits: ResourceLimits) {
        self.vm
            .set_limits(limits.max_instructions, limits.max_recursion_depth);
    }

    /// Take and clear any accumulated print output.
    ///
    /// This is useful if you want to check what was printed after
    /// calling `execute()` multiple times.
    pub fn take_print_output(&mut self) -> Vec<String> {
        self.vm.take_print_output()
    }
}

/// Result of executing code with print output capture.
#[derive(Debug, Clone)]
pub struct ExecuteOutput {
    /// The result value of the last expression.
    pub result: PyValue,
    /// Lines printed via print() calls.
    pub printed: Vec<String>,
}

impl ExecuteOutput {
    /// Get all printed output as a single string (newline-separated).
    pub fn print_output(&self) -> String {
        self.printed.join("\n")
    }

    /// Check if anything was printed.
    pub fn has_output(&self) -> bool {
        !self.printed.is_empty()
    }
}

/// Resource limits for sandbox execution.
///
/// Both limits are optional — `None` means unlimited. Use [`Sandbox::set_limits`]
/// to apply limits before calling [`Sandbox::execute`].
///
/// # Example
///
/// ```
/// use littrs::{Sandbox, ResourceLimits};
///
/// let mut sandbox = Sandbox::new();
/// sandbox.set_limits(ResourceLimits {
///     max_instructions: Some(10_000),
///     max_recursion_depth: Some(50),
/// });
/// ```
#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    /// Maximum number of bytecode instructions per `execute()` call.
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
