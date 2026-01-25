//! Integration tests for the WASM sandbox.
//!
//! The WASM module is embedded in the crate, so no external files are needed.

#![cfg(feature = "wasm")]

use littrs::{PyValue, WasmSandbox, WasmSandboxConfig};

#[test]
fn test_basic_arithmetic() {
    let mut sandbox = WasmSandbox::new().unwrap();

    assert_eq!(sandbox.execute("2 + 2").unwrap(), PyValue::Int(4));
    assert_eq!(sandbox.execute("10 * 5").unwrap(), PyValue::Int(50));
    assert_eq!(sandbox.execute("100 / 4").unwrap(), PyValue::Float(25.0));
}

#[test]
fn test_variables() {
    let mut sandbox = WasmSandbox::new().unwrap();

    sandbox.execute("x = 10").unwrap();
    sandbox.execute("y = 20").unwrap();
    assert_eq!(sandbox.execute("x + y").unwrap(), PyValue::Int(30));
}

#[test]
fn test_for_loop() {
    let mut sandbox = WasmSandbox::new().unwrap();

    let result = sandbox
        .execute(
            r#"
total = 0
for i in range(10):
    total = total + i
total
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(45));
}

#[test]
fn test_register_tool() {
    let mut sandbox = WasmSandbox::new().unwrap();

    sandbox
        .register_fn("double", |args| {
            let n = args.get(0).and_then(|v| v.as_int()).unwrap_or(0);
            PyValue::Int(n * 2)
        })
        .unwrap();

    assert_eq!(sandbox.execute("double(21)").unwrap(), PyValue::Int(42));
}

#[test]
fn test_tool_with_dict_result() {
    let mut sandbox = WasmSandbox::new().unwrap();

    sandbox
        .register_fn("get_user", |args| {
            let id = args.get(0).and_then(|v| v.as_int()).unwrap_or(0);
            PyValue::Dict(vec![
                ("id".to_string(), PyValue::Int(id)),
                ("name".to_string(), PyValue::Str("Test User".to_string())),
            ])
        })
        .unwrap();

    let result = sandbox
        .execute(
            r#"
user = get_user(42)
user['name']
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("Test User".to_string()));
}

#[test]
fn test_set_variable() {
    let mut sandbox = WasmSandbox::new().unwrap();

    sandbox.set_variable("config_value", 100i64).unwrap();
    assert_eq!(
        sandbox.execute("config_value * 2").unwrap(),
        PyValue::Int(200)
    );
}

#[test]
fn test_fuel_limit() {
    let config = WasmSandboxConfig::default().with_fuel(1000); // Very low fuel
    let mut sandbox = WasmSandbox::with_config(config).unwrap();

    // This infinite loop should run out of fuel
    let result = sandbox.execute(
        r#"
x = 0
while True:
    x = x + 1
"#,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("fuel") || err.to_string().contains("Fuel"),
        "Expected fuel error, got: {}",
        err
    );
}

#[test]
fn test_remaining_fuel() {
    let config = WasmSandboxConfig::default().with_fuel(1_000_000);
    let mut sandbox = WasmSandbox::with_config(config).unwrap();

    let initial_fuel = sandbox.remaining_fuel().unwrap();
    sandbox.execute("2 + 2").unwrap();
    let remaining_fuel = sandbox.remaining_fuel().unwrap();

    assert!(remaining_fuel < initial_fuel);
}

#[test]
fn test_reset() {
    let mut sandbox = WasmSandbox::new().unwrap();

    sandbox.execute("x = 42").unwrap();
    assert_eq!(sandbox.execute("x").unwrap(), PyValue::Int(42));

    sandbox.reset().unwrap();

    // After reset, x should not be defined
    let result = sandbox.execute("x");
    assert!(result.is_err());
}

#[test]
fn test_complex_computation() {
    let mut sandbox = WasmSandbox::new().unwrap();

    let result = sandbox
        .execute(
            r#"
# Find sum of even numbers from 1 to 100
total = 0
for i in range(1, 101):
    if i % 2 == 0:
        total = total + i
total
"#,
        )
        .unwrap();

    assert_eq!(result, PyValue::Int(2550)); // Sum of 2+4+6+...+100
}
