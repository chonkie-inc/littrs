use std::sync::Arc;

use crate::error::Result;
use crate::eval::{Evaluator, ToolFn};
use crate::tool::ToolInfo;
use crate::value::PyValue;

/// A secure Python sandbox for executing untrusted code.
///
/// The sandbox provides a minimal Python subset that can execute code
/// safely without access to the file system, network, or other resources.
///
/// # Example
///
/// ```
/// use litter::{Sandbox, PyValue};
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
pub struct Sandbox {
    evaluator: Evaluator,
    tool_infos: Vec<ToolInfo>,
}

impl Sandbox {
    /// Create a new sandbox instance.
    pub fn new() -> Self {
        Self {
            evaluator: Evaluator::new(),
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
    /// use litter::{Sandbox, PyValue};
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
        self.evaluator.register_tool(name, Arc::new(f) as ToolFn);
    }

    /// Register a tool with metadata that can be called from Python code.
    ///
    /// The metadata is used to generate Python documentation for the LLM's
    /// system prompt via [`describe_tools`](Self::describe_tools).
    ///
    /// # Example
    ///
    /// ```
    /// use litter::{Sandbox, PyValue, ToolInfo};
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
        self.evaluator
            .register_tool(info.name.clone(), Arc::new(f) as ToolFn);
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
    /// use litter::{Sandbox, PyValue, ToolInfo};
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
    /// use litter::{Sandbox, PyValue};
    ///
    /// let mut sandbox = Sandbox::new();
    /// sandbox.set_variable("x", PyValue::Int(42));
    ///
    /// let result = sandbox.execute("x * 2").unwrap();
    /// assert_eq!(result, PyValue::Int(84));
    /// ```
    pub fn set_variable(&mut self, name: impl Into<String>, value: impl Into<PyValue>) {
        self.evaluator.set_variable(name, value.into());
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
    /// use litter::{Sandbox, PyValue};
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
        self.evaluator.execute(code)
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}
