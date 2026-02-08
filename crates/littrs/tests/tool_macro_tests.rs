//! Tests for the #[tool] proc macro.

use littrs::{PyValue, Sandbox};
use littrs_macros::tool;

/// Add two numbers together.
///
/// Args:
///     a: First number
///     b: Second number
#[tool]
fn add(a: i64, b: i64) -> i64 {
    a + b
}

/// Greet a person.
///
/// Args:
///     name: The person's name
///     prefix: Optional greeting prefix
#[tool]
fn greet(name: String, prefix: Option<String>) -> String {
    let p = prefix.unwrap_or_else(|| "Hello".to_string());
    format!("{}, {}!", p, name)
}

/// Return a dict with weather info.
#[tool]
fn get_weather(city: String) -> PyValue {
    PyValue::Dict(vec![
        (PyValue::Str("city".to_string()), PyValue::Str(city)),
        (PyValue::Str("temp".to_string()), PyValue::Int(22)),
        (
            PyValue::Str("unit".to_string()),
            PyValue::Str("celsius".to_string()),
        ),
    ])
}

#[test]
fn test_tool_info_generated() {
    // Check that INFO is generated with correct metadata
    assert_eq!(add::INFO.name, "add");
    assert_eq!(add::INFO.description, "Add two numbers together.");
    assert_eq!(add::INFO.args.len(), 2);
    assert_eq!(add::INFO.args[0].name, "a");
    assert_eq!(add::INFO.args[0].python_type, "int");
    assert!(add::INFO.args[0].required);
    assert_eq!(add::INFO.returns, "int");
}

#[test]
fn test_tool_info_with_optional_arg() {
    assert_eq!(greet::INFO.name, "greet");
    assert_eq!(greet::INFO.args.len(), 2);
    assert_eq!(greet::INFO.args[0].name, "name");
    assert!(greet::INFO.args[0].required);
    assert_eq!(greet::INFO.args[1].name, "prefix");
    assert!(!greet::INFO.args[1].required); // Optional
    assert_eq!(greet::INFO.returns, "str");
}

#[test]
fn test_tool_info_arg_descriptions() {
    // With #[arg(desc = "...")] attributes, descriptions are populated
    assert_eq!(greet::INFO.args[0].description, "The person's name");
    assert_eq!(greet::INFO.args[1].description, "Optional greeting prefix");
}

#[test]
fn test_tool_call_with_correct_args() {
    let result = add::call(vec![PyValue::Int(10), PyValue::Int(20)]);
    assert_eq!(result, PyValue::Int(30));
}

#[test]
fn test_tool_call_with_optional_arg_provided() {
    let result = greet::call(vec![
        PyValue::Str("Alice".to_string()),
        PyValue::Str("Hi".to_string()),
    ]);
    assert_eq!(result, PyValue::Str("Hi, Alice!".to_string()));
}

#[test]
fn test_tool_call_with_optional_arg_omitted() {
    let result = greet::call(vec![PyValue::Str("Bob".to_string())]);
    assert_eq!(result, PyValue::Str("Hello, Bob!".to_string()));
}

#[test]
fn test_tool_call_returns_pyvalue() {
    let result = get_weather::call(vec![PyValue::Str("Paris".to_string())]);
    if let PyValue::Dict(pairs) = result {
        let city = pairs
            .iter()
            .find(|(k, _)| k == &PyValue::Str("city".to_string()));
        assert_eq!(
            city,
            Some(&(
                PyValue::Str("city".to_string()),
                PyValue::Str("Paris".to_string())
            ))
        );
    } else {
        panic!("Expected Dict, got {:?}", result);
    }
}

#[test]
fn test_tool_call_with_wrong_type() {
    // Pass string instead of int
    let result = add::call(vec![PyValue::Str("10".to_string()), PyValue::Int(20)]);

    // Should return an error dict
    if let PyValue::Dict(pairs) = result {
        let error = pairs
            .iter()
            .find(|(k, _)| k == &PyValue::Str("error".to_string()));
        assert!(error.is_some(), "Expected error in result");
        if let Some((_, PyValue::Str(msg))) = error {
            assert!(
                msg.contains("argument 'a'"),
                "Error should mention argument name: {}",
                msg
            );
            assert!(
                msg.contains("int") || msg.contains("str"),
                "Error should mention types: {}",
                msg
            );
        }
    } else {
        panic!("Expected error Dict, got {:?}", result);
    }
}

#[test]
fn test_tool_call_with_missing_required_arg() {
    // Call add with no arguments
    let result = add::call(vec![]);

    // Should return an error dict
    if let PyValue::Dict(pairs) = result {
        let error = pairs
            .iter()
            .find(|(k, _)| k == &PyValue::Str("error".to_string()));
        assert!(error.is_some(), "Expected error in result");
        if let Some((_, PyValue::Str(msg))) = error {
            assert!(
                msg.contains("missing"),
                "Error should mention missing: {}",
                msg
            );
            assert!(
                msg.contains("a"),
                "Error should mention argument name: {}",
                msg
            );
        }
    } else {
        panic!("Expected error Dict, got {:?}", result);
    }
}

#[test]
fn test_register_tool_with_sandbox() {
    let mut sandbox = Sandbox::new();

    // Register the add tool
    sandbox.register_tool(add::INFO.clone(), add::call);

    // Call it from Python
    let result = sandbox.run("add(5, 7)").unwrap();
    assert_eq!(result, PyValue::Int(12));
}

#[test]
fn test_describe_tools_with_macro_generated_info() {
    let mut sandbox = Sandbox::new();
    sandbox.register_tool(add::INFO.clone(), add::call);
    sandbox.register_tool(greet::INFO.clone(), greet::call);

    let docs = sandbox.describe();
    assert!(docs.contains("def add(a: int, b: int) -> int:"));
    assert!(docs.contains("Add two numbers together."));
    assert!(docs.contains("def greet(name: str, prefix: str | None = None) -> str:"));
    assert!(docs.contains("Greet a person."));
}

#[test]
fn test_ergonomic_registration_with_tool_struct() {
    let mut sandbox = Sandbox::new();

    // Ergonomic registration using the generated Tool struct
    sandbox.add(add::Tool);
    sandbox.add(greet::Tool);

    // Call from Python
    let result = sandbox.run("add(3, 4)").unwrap();
    assert_eq!(result, PyValue::Int(7));

    let result = sandbox.run("greet('World')").unwrap();
    assert_eq!(result, PyValue::Str("Hello, World!".to_string()));

    // Check documentation is generated
    let docs = sandbox.describe();
    assert!(docs.contains("def add(a: int, b: int) -> int:"));
    assert!(docs.contains("def greet(name: str, prefix: str | None = None) -> str:"));
}
