//! Tool metadata and registration types.
//!
//! This module provides types for describing tools (functions) that can be
//! called from Python code, including their signatures and documentation.

use std::fmt;

use crate::value::TypeError;

/// Error that occurs when calling a tool.
#[derive(Debug, Clone)]
pub enum ToolCallError {
    /// A required argument was not provided
    MissingArgument { name: String },
    /// An argument had the wrong type
    TypeError { arg: String, error: TypeError },
    /// Tool execution failed
    ExecutionError { message: String },
}

impl ToolCallError {
    /// Create a missing argument error.
    pub fn missing_argument(name: impl Into<String>) -> Self {
        Self::MissingArgument { name: name.into() }
    }

    /// Create a type error.
    pub fn type_error(arg: impl Into<String>, error: TypeError) -> Self {
        Self::TypeError {
            arg: arg.into(),
            error,
        }
    }

    /// Create an execution error.
    pub fn execution_error(message: impl Into<String>) -> Self {
        Self::ExecutionError {
            message: message.into(),
        }
    }
}

impl fmt::Display for ToolCallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingArgument { name } => {
                write!(f, "missing required argument: {}", name)
            }
            Self::TypeError { arg, error } => {
                write!(f, "argument '{}': {}", arg, error)
            }
            Self::ExecutionError { message } => {
                write!(f, "execution error: {}", message)
            }
        }
    }
}

impl std::error::Error for ToolCallError {}

/// Trait for tools that can be registered with a sandbox.
///
/// This trait is automatically implemented by the `#[tool]` macro.
/// It provides a unified interface for registering tools ergonomically.
///
/// # Example
///
/// ```ignore
/// use littrs::{Sandbox, tool};
///
/// #[tool(description = "Add two numbers.")]
/// fn add(a: i64, b: i64) -> i64 { a + b }
///
/// let mut sandbox = Sandbox::new();
/// sandbox.add(add);  // Ergonomic registration
/// ```
pub trait Tool {
    /// Get the tool's metadata.
    fn info() -> &'static ToolInfo;

    /// Call the tool with the given arguments.
    fn call(args: Vec<crate::PyValue>) -> crate::PyValue;
}

/// Information about a tool's argument.
#[derive(Debug, Clone)]
pub struct ArgInfo {
    /// The argument name (e.g., "city")
    pub name: String,
    /// The Python type (e.g., "str", "int", "list[str]")
    pub python_type: String,
    /// Description of the argument
    pub description: String,
    /// Whether the argument is required (no default value)
    pub required: bool,
}

impl ArgInfo {
    /// Create a new required argument.
    pub fn required(
        name: impl Into<String>,
        python_type: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            python_type: python_type.into(),
            description: description.into(),
            required: true,
        }
    }

    /// Create a new optional argument.
    pub fn optional(
        name: impl Into<String>,
        python_type: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            python_type: python_type.into(),
            description: description.into(),
            required: false,
        }
    }
}

/// Metadata about a tool that can be called from Python.
///
/// This is used to generate Python documentation for the LLM's system prompt.
///
/// # Example
///
/// ```
/// use littrs::ToolInfo;
///
/// let tool = ToolInfo::new("fetch_weather", "Get current weather for a city")
///     .arg("city", "str", "The city name")
///     .arg_opt("unit", "str", "Temperature unit (celsius or fahrenheit)")
///     .returns("dict");
///
/// println!("{}", tool.doc());
/// // Output:
/// // def fetch_weather(city: str, unit: str | None = None) -> dict:
/// //     """Get current weather for a city.
/// //
/// //     Args:
/// //         city: The city name
/// //         unit: Temperature unit (celsius or fahrenheit)
/// //     """
/// ```
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// The function name
    pub name: String,
    /// Description of what the tool does
    pub description: String,
    /// The arguments
    pub args: Vec<ArgInfo>,
    /// The return type (e.g., "dict", "str", "list\[int\]")
    pub returns: String,
}

impl ToolInfo {
    /// Create a new tool info with the given name and description.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            args: Vec::new(),
            returns: "None".to_string(),
        }
    }

    /// Add a required argument.
    pub fn arg(
        mut self,
        name: impl Into<String>,
        python_type: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.args
            .push(ArgInfo::required(name, python_type, description));
        self
    }

    /// Add an optional argument.
    pub fn arg_opt(
        mut self,
        name: impl Into<String>,
        python_type: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.args
            .push(ArgInfo::optional(name, python_type, description));
        self
    }

    /// Set the return type.
    pub fn returns(mut self, python_type: impl Into<String>) -> Self {
        self.returns = python_type.into();
        self
    }

    /// Generate a Python function signature.
    ///
    /// Example: `fetch_weather(city: str, unit: str | None = None) -> dict`
    pub fn signature(&self) -> String {
        let args: Vec<String> = self
            .args
            .iter()
            .map(|arg| {
                if arg.required {
                    format!("{}: {}", arg.name, arg.python_type)
                } else {
                    format!("{}: {} | None = None", arg.name, arg.python_type)
                }
            })
            .collect();

        format!("{}({}) -> {}", self.name, args.join(", "), self.returns)
    }

    /// Generate a full Python docstring with signature.
    ///
    /// Example:
    /// ```text
    /// def fetch_weather(city: str, unit: str | None = None) -> dict:
    ///     """Get current weather for a city.
    ///
    ///     Args:
    ///         city: The city name
    ///         unit: Temperature unit (celsius or fahrenheit)
    ///     """
    /// ```
    pub fn doc(&self) -> String {
        let mut doc = format!("def {}:\n", self.signature());
        doc.push_str(&format!("    \"\"\"{}\n", self.description));

        if !self.args.is_empty() {
            doc.push_str("\n    Args:\n");
            for arg in &self.args {
                doc.push_str(&format!("        {}: {}\n", arg.name, arg.description));
            }
        }

        doc.push_str("    \"\"\"");
        doc
    }
}

impl fmt::Display for ToolInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.doc())
    }
}

/// Generate Python documentation for multiple tools.
///
/// This is suitable for embedding in a system prompt.
pub fn describe_tools(tools: &[ToolInfo]) -> String {
    tools
        .iter()
        .map(|t| t.doc())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_signature_no_args() {
        let tool = ToolInfo::new("get_time", "Get the current time").returns("str");
        assert_eq!(tool.signature(), "get_time() -> str");
    }

    #[test]
    fn test_tool_signature_required_args() {
        let tool = ToolInfo::new("add", "Add two numbers")
            .arg("a", "int", "First number")
            .arg("b", "int", "Second number")
            .returns("int");
        assert_eq!(tool.signature(), "add(a: int, b: int) -> int");
    }

    #[test]
    fn test_tool_signature_mixed_args() {
        let tool = ToolInfo::new("search", "Search for items")
            .arg("query", "str", "Search query")
            .arg_opt("limit", "int", "Max results")
            .returns("list[str]");
        assert_eq!(
            tool.signature(),
            "search(query: str, limit: int | None = None) -> list[str]"
        );
    }

    #[test]
    fn test_tool_python_doc() {
        let tool = ToolInfo::new("fetch_weather", "Get current weather for a city.")
            .arg("city", "str", "The city name")
            .arg_opt("unit", "str", "Temperature unit")
            .returns("dict");

        let doc = tool.doc();
        assert!(doc.contains("def fetch_weather(city: str, unit: str | None = None) -> dict:"));
        assert!(doc.contains("\"\"\"Get current weather for a city."));
        assert!(doc.contains("Args:"));
        assert!(doc.contains("city: The city name"));
        assert!(doc.contains("unit: Temperature unit"));
    }

    #[test]
    fn test_describe_tools() {
        let tools = vec![
            ToolInfo::new("tool_a", "Does A").returns("str"),
            ToolInfo::new("tool_b", "Does B").returns("int"),
        ];

        let doc = describe_tools(&tools);
        assert!(doc.contains("def tool_a() -> str:"));
        assert!(doc.contains("def tool_b() -> int:"));
    }
}
