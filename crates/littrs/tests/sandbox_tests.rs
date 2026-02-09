use littrs::{Limits, PyValue, Sandbox};
use std::f64::consts::PI;

#[test]
fn test_basic_arithmetic() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("2 + 2").unwrap(), PyValue::Int(4));
    assert_eq!(sandbox.run("10 - 3").unwrap(), PyValue::Int(7));
    assert_eq!(sandbox.run("4 * 5").unwrap(), PyValue::Int(20));
    assert_eq!(sandbox.run("10 / 4").unwrap(), PyValue::Float(2.5));
    assert_eq!(sandbox.run("10 // 3").unwrap(), PyValue::Int(3));
    assert_eq!(sandbox.run("10 % 3").unwrap(), PyValue::Int(1));
    assert_eq!(sandbox.run("2 ** 8").unwrap(), PyValue::Int(256));
}

#[test]
fn test_variables() {
    let mut sandbox = Sandbox::new();

    sandbox.run("x = 10").unwrap();
    sandbox.run("y = 20").unwrap();
    assert_eq!(sandbox.run("x + y").unwrap(), PyValue::Int(30));
}

#[test]
fn test_strings() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("'hello' + ' ' + 'world'").unwrap(),
        PyValue::Str("hello world".to_string())
    );
    assert_eq!(
        sandbox.run("'ab' * 3").unwrap(),
        PyValue::Str("ababab".to_string())
    );
}

#[test]
fn test_lists() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("[1, 2, 3]").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );

    sandbox.run("nums = [10, 20, 30]").unwrap();
    assert_eq!(sandbox.run("nums[0]").unwrap(), PyValue::Int(10));
    assert_eq!(sandbox.run("nums[-1]").unwrap(), PyValue::Int(30));
}

#[test]
fn test_dict() {
    let mut sandbox = Sandbox::new();

    sandbox.run("data = {'name': 'Alice', 'age': 30}").unwrap();
    assert_eq!(
        sandbox.run("data['name']").unwrap(),
        PyValue::Str("Alice".to_string())
    );
    assert_eq!(sandbox.run("data['age']").unwrap(), PyValue::Int(30));
}

#[test]
fn test_comparisons() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("5 > 3").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("5 < 3").unwrap(), PyValue::Bool(false));
    assert_eq!(sandbox.run("5 == 5").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("5 != 3").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("1 < 2 < 3").unwrap(), PyValue::Bool(true));
}

#[test]
fn test_boolean_ops() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("True and False").unwrap(), PyValue::Bool(false));
    assert_eq!(sandbox.run("True or False").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("not True").unwrap(), PyValue::Bool(false));
}

#[test]
fn test_if_statement() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
x = 10
if x > 5:
    result = 'big'
else:
    result = 'small'
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("big".to_string()));
}

#[test]
fn test_for_loop() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
total = 0
for i in range(5):
    total = total + i
total
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(10)); // 0+1+2+3+4
}

#[test]
fn test_while_loop() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
count = 0
while count < 5:
    count = count + 1
count
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(5));
}

#[test]
fn test_builtin_len() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("len('hello')").unwrap(), PyValue::Int(5));
    assert_eq!(sandbox.run("len([1, 2, 3])").unwrap(), PyValue::Int(3));
}

#[test]
fn test_builtin_range() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("range(3)").unwrap(),
        PyValue::List(vec![PyValue::Int(0), PyValue::Int(1), PyValue::Int(2)])
    );
    assert_eq!(
        sandbox.run("range(1, 4)").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
    assert_eq!(
        sandbox.run("range(0, 10, 2)").unwrap(),
        PyValue::List(vec![
            PyValue::Int(0),
            PyValue::Int(2),
            PyValue::Int(4),
            PyValue::Int(6),
            PyValue::Int(8)
        ])
    );
}

#[test]
fn test_builtin_sum_min_max() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("sum([1, 2, 3, 4])").unwrap(), PyValue::Int(10));
    assert_eq!(sandbox.run("min(5, 3, 8)").unwrap(), PyValue::Int(3));
    assert_eq!(sandbox.run("max(5, 3, 8)").unwrap(), PyValue::Int(8));
    assert_eq!(sandbox.run("min([5, 3, 8])").unwrap(), PyValue::Int(3));
}

#[test]
fn test_register_tool() {
    let mut sandbox = Sandbox::new();

    sandbox.register_fn("double", |args| {
        let n = args[0].as_int().unwrap_or(0);
        PyValue::Int(n * 2)
    });

    assert_eq!(sandbox.run("double(21)").unwrap(), PyValue::Int(42));
}

#[test]
fn test_tool_with_multiple_args() {
    let mut sandbox = Sandbox::new();

    sandbox.register_fn("add_all", |args| {
        let sum: i64 = args.iter().filter_map(|v| v.as_int()).sum();
        PyValue::Int(sum)
    });

    assert_eq!(
        sandbox.run("add_all(1, 2, 3, 4, 5)").unwrap(),
        PyValue::Int(15)
    );
}

#[test]
fn test_tool_returning_dict() {
    let mut sandbox = Sandbox::new();

    sandbox.register_fn("get_user", |args| {
        let id = args[0].as_int().unwrap_or(0);
        PyValue::Dict(vec![
            (PyValue::Str("id".to_string()), PyValue::Int(id)),
            (
                PyValue::Str("name".to_string()),
                PyValue::Str("Test User".to_string()),
            ),
        ])
    });

    let result = sandbox
        .run(
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
    let mut sandbox = Sandbox::new();

    sandbox.set("config_value", PyValue::Int(100));
    assert_eq!(sandbox.run("config_value * 2").unwrap(), PyValue::Int(200));
}

#[test]
fn test_in_operator() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("'a' in 'abc'").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("'x' in 'abc'").unwrap(), PyValue::Bool(false));
    assert_eq!(sandbox.run("2 in [1, 2, 3]").unwrap(), PyValue::Bool(true));
    assert_eq!(
        sandbox.run("5 not in [1, 2, 3]").unwrap(),
        PyValue::Bool(true)
    );
}

#[test]
fn test_ternary_expression() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("'yes' if True else 'no'").unwrap(),
        PyValue::Str("yes".to_string())
    );
    assert_eq!(
        sandbox.run("'yes' if False else 'no'").unwrap(),
        PyValue::Str("no".to_string())
    );
}

#[test]
fn test_augmented_assignment() {
    let mut sandbox = Sandbox::new();

    sandbox.run("x = 10").unwrap();
    sandbox.run("x += 5").unwrap();
    assert_eq!(sandbox.run("x").unwrap(), PyValue::Int(15));

    sandbox.run("x *= 2").unwrap();
    assert_eq!(sandbox.run("x").unwrap(), PyValue::Int(30));
}

#[test]
fn test_negative_numbers() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("-5").unwrap(), PyValue::Int(-5));
    assert_eq!(sandbox.run("-5 + 10").unwrap(), PyValue::Int(5));
    assert_eq!(sandbox.run("abs(-42)").unwrap(), PyValue::Int(42));
}

#[test]
fn test_type_conversions() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("int(3.7)").unwrap(), PyValue::Int(3));
    assert_eq!(sandbox.run("float(5)").unwrap(), PyValue::Float(5.0));
    assert_eq!(
        sandbox.run("str(42)").unwrap(),
        PyValue::Str("42".to_string())
    );
    assert_eq!(sandbox.run("bool(1)").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("bool(0)").unwrap(), PyValue::Bool(false));
}

#[test]
fn test_list_subscript_assignment() {
    let mut sandbox = Sandbox::new();

    sandbox.run("nums = [1, 2, 3]").unwrap();
    sandbox.run("nums[1] = 99").unwrap();
    assert_eq!(sandbox.run("nums[1]").unwrap(), PyValue::Int(99));
}

#[test]
fn test_division_by_zero() {
    let mut sandbox = Sandbox::new();

    let result = sandbox.run("10 / 0");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Division by zero"));
}

#[test]
fn test_undefined_variable() {
    let mut sandbox = Sandbox::new();

    let result = sandbox.run("undefined_var");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not defined"));
}

#[test]
fn test_complex_expression() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
data = [1, 2, 3, 4, 5]
total = 0
for x in data:
    if x % 2 == 0:
        total = total + x
total
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(6)); // 2 + 4
}

#[test]
fn test_tool_with_keyword_arguments() {
    use littrs::ToolInfo;

    let mut sandbox = Sandbox::new();

    // Register a tool with named arguments
    let info = ToolInfo::new("greet", "Greet someone")
        .arg("name", "str", "The name")
        .arg_opt("greeting", "str", "The greeting")
        .returns("str");

    sandbox.register_tool(info, |args| {
        let name = args.get(0).and_then(|v| v.as_str()).unwrap_or("World");
        let greeting = args.get(1).and_then(|v| v.as_str()).unwrap_or("Hello");
        PyValue::Str(format!("{}, {}!", greeting, name))
    });

    // Test with positional args
    assert_eq!(
        sandbox.run("greet('Alice', 'Hi')").unwrap(),
        PyValue::Str("Hi, Alice!".to_string())
    );

    // Test with keyword args
    assert_eq!(
        sandbox.run("greet(name='Bob')").unwrap(),
        PyValue::Str("Hello, Bob!".to_string())
    );

    // Test with mixed positional and keyword args
    assert_eq!(
        sandbox.run("greet('Charlie', greeting='Hey')").unwrap(),
        PyValue::Str("Hey, Charlie!".to_string())
    );

    // Test with keyword args in different order
    assert_eq!(
        sandbox
            .run("greet(greeting='Welcome', name='Dave')")
            .unwrap(),
        PyValue::Str("Welcome, Dave!".to_string())
    );
}

#[test]
fn test_list_comprehension_basic() {
    let mut sandbox = Sandbox::new();

    // Basic list comprehension
    assert_eq!(
        sandbox.run("[x for x in range(5)]").unwrap(),
        PyValue::List(vec![
            PyValue::Int(0),
            PyValue::Int(1),
            PyValue::Int(2),
            PyValue::Int(3),
            PyValue::Int(4),
        ])
    );
}

#[test]
fn test_list_comprehension_with_expression() {
    let mut sandbox = Sandbox::new();

    // List comprehension with expression
    assert_eq!(
        sandbox.run("[x * 2 for x in range(4)]").unwrap(),
        PyValue::List(vec![
            PyValue::Int(0),
            PyValue::Int(2),
            PyValue::Int(4),
            PyValue::Int(6),
        ])
    );

    // Squares
    assert_eq!(
        sandbox.run("[x ** 2 for x in range(1, 5)]").unwrap(),
        PyValue::List(vec![
            PyValue::Int(1),
            PyValue::Int(4),
            PyValue::Int(9),
            PyValue::Int(16),
        ])
    );
}

#[test]
fn test_list_comprehension_with_filter() {
    let mut sandbox = Sandbox::new();

    // List comprehension with if filter
    assert_eq!(
        sandbox.run("[x for x in range(10) if x % 2 == 0]").unwrap(),
        PyValue::List(vec![
            PyValue::Int(0),
            PyValue::Int(2),
            PyValue::Int(4),
            PyValue::Int(6),
            PyValue::Int(8),
        ])
    );

    // Filter with expression
    assert_eq!(
        sandbox.run("[x * 2 for x in range(5) if x > 1]").unwrap(),
        PyValue::List(vec![PyValue::Int(4), PyValue::Int(6), PyValue::Int(8),])
    );
}

#[test]
fn test_list_comprehension_over_list() {
    let mut sandbox = Sandbox::new();

    sandbox.run("nums = [1, 2, 3, 4, 5]").unwrap();

    assert_eq!(
        sandbox.run("[n + 10 for n in nums]").unwrap(),
        PyValue::List(vec![
            PyValue::Int(11),
            PyValue::Int(12),
            PyValue::Int(13),
            PyValue::Int(14),
            PyValue::Int(15),
        ])
    );
}

#[test]
fn test_list_comprehension_over_string() {
    let mut sandbox = Sandbox::new();

    // Iterate over string characters
    assert_eq!(
        sandbox.run("[c for c in 'abc']").unwrap(),
        PyValue::List(vec![
            PyValue::Str("a".to_string()),
            PyValue::Str("b".to_string()),
            PyValue::Str("c".to_string()),
        ])
    );
}

#[test]
fn test_list_comprehension_nested() {
    let mut sandbox = Sandbox::new();

    // Nested comprehension (flattening)
    assert_eq!(
        sandbox
            .run("[x * y for x in range(1, 3) for y in range(1, 3)]")
            .unwrap(),
        PyValue::List(vec![
            PyValue::Int(1), // 1*1
            PyValue::Int(2), // 1*2
            PyValue::Int(2), // 2*1
            PyValue::Int(4), // 2*2
        ])
    );
}

#[test]
fn test_list_comprehension_multiple_filters() {
    let mut sandbox = Sandbox::new();

    // Multiple if conditions
    assert_eq!(
        sandbox
            .run("[x for x in range(20) if x % 2 == 0 if x % 3 == 0]")
            .unwrap(),
        PyValue::List(vec![
            PyValue::Int(0),
            PyValue::Int(6),
            PyValue::Int(12),
            PyValue::Int(18),
        ])
    );
}

// ============================================================================
// Function definitions (def)
// ============================================================================

#[test]
fn test_function_definition_basic() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def double(x):
    return x * 2
double(21)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(42));
}

#[test]
fn test_function_multiple_params() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def add(a, b):
    return a + b
add(10, 32)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(42));
}

#[test]
fn test_function_implicit_return_none() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def noop():
    x = 1
noop()
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::None);
}

#[test]
fn test_function_calling_tools() {
    let mut sandbox = Sandbox::new();
    sandbox.register_fn("double_it", |args| {
        let n = args[0].as_int().unwrap_or(0);
        PyValue::Int(n * 2)
    });

    let result = sandbox
        .run(
            r#"
def process(x):
    return double_it(x) + 1
process(10)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(21));
}

#[test]
fn test_nested_function_calls() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def square(x):
    return x * x
def sum_of_squares(a, b):
    return square(a) + square(b)
sum_of_squares(3, 4)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(25));
}

#[test]
fn test_function_with_loop() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def factorial(n):
    result = 1
    for i in range(1, n + 1):
        result = result * i
    return result
factorial(5)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(120));
}

#[test]
fn test_function_scope_isolation() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
x = 100
def f():
    x = 5
    return x
result = f()
x + result
"#,
        )
        .unwrap();
    // f() returns 5, global x is still 100
    assert_eq!(result, PyValue::Int(105));
}

#[test]
fn test_function_reads_globals() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
multiplier = 10
def scale(x):
    return x * multiplier
scale(5)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(50));
}

#[test]
fn test_recursive_function() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)
fib(10)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(55));
}

// ============================================================================
// Break and continue
// ============================================================================

#[test]
fn test_break_in_while() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
i = 0
while True:
    if i >= 5:
        break
    i = i + 1
i
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(5));
}

#[test]
fn test_break_in_for() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
found = -1
for x in range(100):
    if x * x > 50:
        found = x
        break
found
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(8)); // 8*8 = 64 > 50
}

#[test]
fn test_continue_in_for() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
total = 0
for i in range(10):
    if i % 2 == 0:
        continue
    total = total + i
total
"#,
        )
        .unwrap();
    // Sum of odd numbers 1+3+5+7+9 = 25
    assert_eq!(result, PyValue::Int(25));
}

#[test]
fn test_continue_in_while() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = []
i = 0
while i < 10:
    i = i + 1
    if i % 3 == 0:
        continue
    result.append(i)
result
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![
            PyValue::Int(1),
            PyValue::Int(2),
            PyValue::Int(4),
            PyValue::Int(5),
            PyValue::Int(7),
            PyValue::Int(8),
            PyValue::Int(10),
        ])
    );
}

#[test]
fn test_break_in_nested_loops() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = []
for i in range(5):
    for j in range(5):
        if j >= 2:
            break
        result.append(i * 10 + j)
result
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![
            PyValue::Int(0),
            PyValue::Int(1),
            PyValue::Int(10),
            PyValue::Int(11),
            PyValue::Int(20),
            PyValue::Int(21),
            PyValue::Int(30),
            PyValue::Int(31),
            PyValue::Int(40),
            PyValue::Int(41),
        ])
    );
}

// ============================================================================
// Resource limits
// ============================================================================

#[test]
fn test_instruction_limit_infinite_loop() {
    let mut sandbox = Sandbox::new();
    sandbox.limit(Limits {
        max_instructions: Some(1_000),
        ..Default::default()
    });

    let err = sandbox.run("while True: pass").unwrap_err();
    assert!(err.to_string().contains("Instruction limit"));
}

#[test]
fn test_recursion_limit() {
    let mut sandbox = Sandbox::new();
    sandbox.limit(Limits {
        max_recursion_depth: Some(10),
        ..Default::default()
    });

    let err = sandbox
        .run(
            r#"
def recurse(n):
    return recurse(n + 1)
recurse(0)
"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("Recursion limit"));
}

#[test]
fn test_within_limits_succeeds() {
    let mut sandbox = Sandbox::new();
    sandbox.limit(Limits {
        max_instructions: Some(10_000),
        max_recursion_depth: Some(50),
    });

    let result = sandbox
        .run(
            r#"
total = 0
for i in range(100):
    total = total + i
total
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(4950));
}

#[test]
fn test_instruction_limit_only() {
    let mut sandbox = Sandbox::new();
    sandbox.limit(Limits {
        max_instructions: Some(500),
        max_recursion_depth: None,
    });

    // Short code should succeed
    let result = sandbox.run("1 + 2").unwrap();
    assert_eq!(result, PyValue::Int(3));
}

#[test]
fn test_recursion_limit_only() {
    let mut sandbox = Sandbox::new();
    sandbox.limit(Limits {
        max_instructions: None,
        max_recursion_depth: Some(5),
    });

    // Moderate recursion within limit should succeed
    let result = sandbox
        .run(
            r#"
def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)
factorial(4)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(24));
}

// ============================================================================
// Default parameters
// ============================================================================

#[test]
fn test_default_param_basic() {
    let mut sandbox = Sandbox::new();

    // Call with both args
    let result = sandbox
        .run(
            r#"
def add(x, y=10):
    return x + y
add(5, 3)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(8));
}

#[test]
fn test_default_param_uses_default() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def add(x, y=10):
    return x + y
add(5)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(15));
}

#[test]
fn test_default_param_multiple_defaults() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(a, b=2, c=3):
    return a + b + c
[f(1), f(1, 20), f(1, 20, 30)]
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![PyValue::Int(6), PyValue::Int(24), PyValue::Int(51),])
    );
}

#[test]
fn test_default_param_with_keyword_override() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def greet(name, greeting="Hello"):
    return greeting + " " + name
greet("Alice", greeting="Hi")
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("Hi Alice".to_string()));
}

#[test]
fn test_default_param_string_default() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def greet(name, greeting="Hello"):
    return greeting + " " + name
greet("World")
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("Hello World".to_string()));
}

#[test]
fn test_default_param_none_default() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(x, y=None):
    if y is None:
        return x
    return x + y
[f(5), f(5, 3)]
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![PyValue::Int(5), PyValue::Int(8)])
    );
}

#[test]
fn test_default_param_negative_default() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(x, y=-1):
    return x + y
f(10)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(9));
}

#[test]
fn test_default_param_too_few_args_error() {
    let mut sandbox = Sandbox::new();

    let err = sandbox
        .run(
            r#"
def f(a, b, c=3):
    return a + b + c
f(1)
"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("missing"));
}

// ============================================================================
// Try/Except
// ============================================================================

#[test]
fn test_try_except_basic() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = "no error"
try:
    x = 1 / 0
except:
    result = "caught"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("caught".to_string()));
}

#[test]
fn test_try_except_no_error() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = "before"
try:
    result = "success"
except:
    result = "caught"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("success".to_string()));
}

#[test]
fn test_try_except_typed() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = "none"
try:
    x = 1 / 0
except ZeroDivisionError:
    result = "zero div"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("zero div".to_string()));
}

#[test]
fn test_try_except_with_as() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
try:
    x = 1 / 0
except ZeroDivisionError as e:
    msg = e
msg
"#,
        )
        .unwrap();
    // The error message should contain "Division by zero"
    if let PyValue::Str(s) = result {
        assert!(s.contains("ivision by zero") || s.contains("zero"));
    } else {
        panic!("Expected string, got {:?}", result);
    }
}

#[test]
fn test_try_except_type_mismatch_propagates() {
    let mut sandbox = Sandbox::new();

    // Try catching NameError when a ZeroDivisionError occurs — should not catch
    let err = sandbox
        .run(
            r#"
try:
    x = 1 / 0
except NameError:
    pass
"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("ivision by zero"));
}

#[test]
fn test_try_except_multiple_handlers() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = "none"
try:
    x = undefined_var
except ZeroDivisionError:
    result = "zero div"
except NameError:
    result = "name error"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("name error".to_string()));
}

#[test]
fn test_try_except_catch_all() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = "none"
try:
    x = undefined_var
except Exception:
    result = "caught"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("caught".to_string()));
}

#[test]
fn test_raise_basic() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = "none"
try:
    raise ValueError("bad value")
except ValueError:
    result = "caught value error"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("caught value error".to_string()));
}

#[test]
fn test_raise_with_message_as() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
try:
    raise ValueError("test message")
except ValueError as e:
    msg = e
msg
"#,
        )
        .unwrap();
    if let PyValue::Str(s) = result {
        assert!(s.contains("test message"));
    } else {
        panic!("Expected string, got {:?}", result);
    }
}

#[test]
fn test_bare_raise() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = "none"
try:
    try:
        x = 1 / 0
    except ZeroDivisionError:
        raise
except:
    result = "re-caught"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("re-caught".to_string()));
}

#[test]
fn test_try_except_else() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
result = "none"
try:
    x = 42
except:
    result = "error"
else:
    result = "no error"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("no error".to_string()));
}

#[test]
fn test_try_except_in_function() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def safe_divide(a, b):
    try:
        return a / b
    except ZeroDivisionError:
        return -1
safe_divide(10, 0)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(-1));
}

#[test]
fn test_uncaught_exception_propagates() {
    let mut sandbox = Sandbox::new();

    let err = sandbox
        .run(
            r#"
raise ValueError("this is not caught")
"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("ValueError"));
}

#[test]
fn test_resource_limit_uncatchable() {
    let mut sandbox = Sandbox::new();
    sandbox.limit(Limits {
        max_instructions: Some(100),
        ..Default::default()
    });

    let err = sandbox
        .run(
            r#"
try:
    while True:
        pass
except:
    pass
"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("Instruction limit"));
}

// ============================================================================
// *args and **kwargs
// ============================================================================

#[test]
fn test_varargs_basic() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(*args):
    return args
f(1, 2, 3)
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::Tuple(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
}

#[test]
fn test_varargs_with_positional() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(a, *args):
    return [a, args]
f(1, 2, 3)
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![
            PyValue::Int(1),
            PyValue::Tuple(vec![PyValue::Int(2), PyValue::Int(3)])
        ])
    );
}

#[test]
fn test_varargs_empty() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(a, *args):
    return args
f(1)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Tuple(vec![]));
}

#[test]
fn test_kwargs_basic() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(**kwargs):
    return kwargs
f(x=1, y=2)
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::Dict(vec![
            (PyValue::Str("x".to_string()), PyValue::Int(1)),
            (PyValue::Str("y".to_string()), PyValue::Int(2)),
        ])
    );
}

#[test]
fn test_kwargs_with_positional() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(a, **kwargs):
    return [a, kwargs]
f(1, x=2, y=3)
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![
            PyValue::Int(1),
            PyValue::Dict(vec![
                (PyValue::Str("x".to_string()), PyValue::Int(2)),
                (PyValue::Str("y".to_string()), PyValue::Int(3)),
            ])
        ])
    );
}

#[test]
fn test_kwargs_empty() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(a, **kwargs):
    return kwargs
f(1)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Dict(vec![]));
}

#[test]
fn test_varargs_and_kwargs_combined() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def f(a, b=2, *args, **kwargs):
    return [a, b, args, kwargs]
f(1, 10, 20, 30, x=99)
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![
            PyValue::Int(1),
            PyValue::Int(10),
            PyValue::Tuple(vec![PyValue::Int(20), PyValue::Int(30)]),
            PyValue::Dict(vec![(PyValue::Str("x".to_string()), PyValue::Int(99))]),
        ])
    );
}

#[test]
fn test_varargs_sum() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .run(
            r#"
def my_sum(*args):
    total = 0
    for x in args:
        total = total + x
    return total
my_sum(1, 2, 3, 4, 5)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Int(15));
}

#[test]
fn test_duplicate_keyword_error() {
    let mut sandbox = Sandbox::new();

    let err = sandbox
        .run(
            r#"
def f(a, b):
    return a + b
f(1, a=2)
"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("multiple values"));
}

#[test]
fn test_unexpected_keyword_without_kwargs() {
    let mut sandbox = Sandbox::new();

    let err = sandbox
        .run(
            r#"
def f(a, b):
    return a + b
f(1, 2, c=3)
"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("unexpected keyword"));
}

// ============================================================================
// Set tests
// ============================================================================

#[test]
fn test_set_literal() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("{1, 2, 3}").unwrap(),
        PyValue::Set(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
}

#[test]
fn test_set_dedup() {
    let mut sandbox = Sandbox::new();

    // Duplicates should be removed
    let result = sandbox.run("{1, 2, 2, 3, 3, 3}").unwrap();
    if let PyValue::Set(items) = &result {
        assert_eq!(items.len(), 3);
    } else {
        panic!("Expected Set, got {:?}", result);
    }
}

#[test]
fn test_set_empty_builtin() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("set()").unwrap(), PyValue::Set(vec![]));
}

#[test]
fn test_set_from_list() {
    let mut sandbox = Sandbox::new();

    let result = sandbox.run("set([1, 2, 2, 3])").unwrap();
    if let PyValue::Set(items) = &result {
        assert_eq!(items.len(), 3);
        assert!(items.contains(&PyValue::Int(1)));
        assert!(items.contains(&PyValue::Int(2)));
        assert!(items.contains(&PyValue::Int(3)));
    } else {
        panic!("Expected Set, got {:?}", result);
    }
}

#[test]
fn test_set_membership() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("2 in {1, 2, 3}").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("5 in {1, 2, 3}").unwrap(), PyValue::Bool(false));
    assert_eq!(
        sandbox.run("5 not in {1, 2, 3}").unwrap(),
        PyValue::Bool(true)
    );
}

#[test]
fn test_set_len() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("len({1, 2, 3})").unwrap(), PyValue::Int(3));
    assert_eq!(sandbox.run("len(set())").unwrap(), PyValue::Int(0));
}

#[test]
fn test_set_union() {
    let mut sandbox = Sandbox::new();

    let result = sandbox.run("{1, 2} | {2, 3}").unwrap();
    if let PyValue::Set(items) = &result {
        assert_eq!(items.len(), 3);
        assert!(items.contains(&PyValue::Int(1)));
        assert!(items.contains(&PyValue::Int(2)));
        assert!(items.contains(&PyValue::Int(3)));
    } else {
        panic!("Expected Set, got {:?}", result);
    }
}

#[test]
fn test_set_intersection() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("{1, 2, 3} & {2, 3, 4}").unwrap(),
        PyValue::Set(vec![PyValue::Int(2), PyValue::Int(3)])
    );
}

#[test]
fn test_set_difference() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("{1, 2, 3} - {2, 3, 4}").unwrap(),
        PyValue::Set(vec![PyValue::Int(1)])
    );
}

#[test]
fn test_set_symmetric_difference() {
    let mut sandbox = Sandbox::new();

    let result = sandbox.run("{1, 2, 3} ^ {2, 3, 4}").unwrap();
    if let PyValue::Set(items) = &result {
        assert_eq!(items.len(), 2);
        assert!(items.contains(&PyValue::Int(1)));
        assert!(items.contains(&PyValue::Int(4)));
    } else {
        panic!("Expected Set, got {:?}", result);
    }
}

#[test]
fn test_set_equality() {
    let mut sandbox = Sandbox::new();

    // Order-independent equality
    assert_eq!(
        sandbox.run("{3, 1, 2} == {1, 2, 3}").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.run("{1, 2} == {1, 2, 3}").unwrap(),
        PyValue::Bool(false)
    );
}

#[test]
fn test_set_subset_superset() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("{1, 2} <= {1, 2, 3}").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.run("{1, 2} < {1, 2, 3}").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.run("{1, 2, 3} < {1, 2, 3}").unwrap(),
        PyValue::Bool(false)
    );
    assert_eq!(
        sandbox.run("{1, 2, 3} >= {1, 2}").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.run("{1, 2, 3} > {1, 2}").unwrap(),
        PyValue::Bool(true)
    );
}

#[test]
fn test_set_methods() {
    let mut sandbox = Sandbox::new();

    // add
    sandbox.run("s = {1, 2}").unwrap();
    sandbox.run("s.add(3)").unwrap();
    assert_eq!(sandbox.run("3 in s").unwrap(), PyValue::Bool(true));

    // discard (no error if missing)
    sandbox.run("s.discard(99)").unwrap();

    // remove (error if missing)
    assert!(sandbox.run("s.remove(99)").is_err());

    // clear
    sandbox.run("s.clear()").unwrap();
    assert_eq!(sandbox.run("len(s)").unwrap(), PyValue::Int(0));
}

#[test]
fn test_set_method_union_intersection() {
    let mut sandbox = Sandbox::new();

    let result = sandbox.run("{1, 2}.union({2, 3})").unwrap();
    if let PyValue::Set(items) = &result {
        assert_eq!(items.len(), 3);
    } else {
        panic!("Expected Set");
    }

    assert_eq!(
        sandbox.run("{1, 2, 3}.intersection({2, 3, 4})").unwrap(),
        PyValue::Set(vec![PyValue::Int(2), PyValue::Int(3)])
    );

    assert_eq!(
        sandbox.run("{1, 2}.issubset({1, 2, 3})").unwrap(),
        PyValue::Bool(true)
    );

    assert_eq!(
        sandbox.run("{1, 2, 3}.issuperset({1, 2})").unwrap(),
        PyValue::Bool(true)
    );

    assert_eq!(
        sandbox.run("{1, 2}.isdisjoint({3, 4})").unwrap(),
        PyValue::Bool(true)
    );
}

#[test]
fn test_set_iteration() {
    let mut sandbox = Sandbox::new();

    // For loop over a set
    sandbox
        .run(
            r#"
result = []
for x in {3, 1, 2}:
    result.append(x)
"#,
        )
        .unwrap();
    let result = sandbox.run("sorted(result)").unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
}

#[test]
fn test_set_type_name() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("type({1, 2})").unwrap(),
        PyValue::Str("set".to_string())
    );
    assert_eq!(
        sandbox.run("isinstance({1}, 'set')").unwrap(),
        PyValue::Bool(true)
    );
}

#[test]
fn test_set_truthiness() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("bool({1, 2})").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("bool(set())").unwrap(), PyValue::Bool(false));
}

#[test]
fn test_set_update() {
    let mut sandbox = Sandbox::new();

    sandbox.run("s = {1, 2}").unwrap();
    sandbox.run("s.update({3, 4})").unwrap();
    assert_eq!(sandbox.run("len(s)").unwrap(), PyValue::Int(4));
    assert_eq!(sandbox.run("3 in s").unwrap(), PyValue::Bool(true));
}

#[test]
fn test_set_unhashable_rejected() {
    let mut sandbox = Sandbox::new();

    // Lists are not hashable and cannot be set elements
    assert!(sandbox.run("{[1, 2]}").is_err());
}

#[test]
fn test_set_print_format() {
    let mut sandbox = Sandbox::new();

    let out = sandbox.capture("print(set())").unwrap();
    assert_eq!(out.output, vec!["set()"]);

    let out = sandbox.capture("print({1})").unwrap();
    assert_eq!(out.output, vec!["{1}"]);
}

// ============================================================================
// Tuple tests
// ============================================================================

#[test]
fn test_tuple_literal() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("(1, 2, 3)").unwrap(),
        PyValue::Tuple(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
}

#[test]
fn test_tuple_single_element() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("(1,)").unwrap(),
        PyValue::Tuple(vec![PyValue::Int(1)])
    );
}

#[test]
fn test_tuple_empty() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("()").unwrap(), PyValue::Tuple(vec![]));
}

#[test]
fn test_tuple_immutable() {
    let mut sandbox = Sandbox::new();

    sandbox.run("t = (1, 2, 3)").unwrap();
    assert!(sandbox.run("t[0] = 99").is_err());
}

#[test]
fn test_tuple_indexing() {
    let mut sandbox = Sandbox::new();

    sandbox.run("t = (10, 20, 30)").unwrap();
    assert_eq!(sandbox.run("t[0]").unwrap(), PyValue::Int(10));
    assert_eq!(sandbox.run("t[-1]").unwrap(), PyValue::Int(30));
}

#[test]
fn test_tuple_concatenation() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("(1, 2) + (3, 4)").unwrap(),
        PyValue::Tuple(vec![
            PyValue::Int(1),
            PyValue::Int(2),
            PyValue::Int(3),
            PyValue::Int(4)
        ])
    );
}

#[test]
fn test_tuple_repetition() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("(1, 2) * 3").unwrap(),
        PyValue::Tuple(vec![
            PyValue::Int(1),
            PyValue::Int(2),
            PyValue::Int(1),
            PyValue::Int(2),
            PyValue::Int(1),
            PyValue::Int(2),
        ])
    );
}

#[test]
fn test_tuple_membership() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.run("2 in (1, 2, 3)").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("5 in (1, 2, 3)").unwrap(), PyValue::Bool(false));
}

#[test]
fn test_tuple_unpacking() {
    let mut sandbox = Sandbox::new();

    sandbox.run("a, b, c = (1, 2, 3)").unwrap();
    assert_eq!(sandbox.run("a").unwrap(), PyValue::Int(1));
    assert_eq!(sandbox.run("b").unwrap(), PyValue::Int(2));
    assert_eq!(sandbox.run("c").unwrap(), PyValue::Int(3));
}

#[test]
fn test_tuple_type_name() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("type((1, 2))").unwrap(),
        PyValue::Str("tuple".to_string())
    );
}

#[test]
fn test_tuple_builtin() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("tuple([1, 2, 3])").unwrap(),
        PyValue::Tuple(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
    assert_eq!(
        sandbox.run("tuple('abc')").unwrap(),
        PyValue::Tuple(vec![
            PyValue::Str("a".to_string()),
            PyValue::Str("b".to_string()),
            PyValue::Str("c".to_string()),
        ])
    );
}

#[test]
fn test_tuple_iteration() {
    let mut sandbox = Sandbox::new();

    sandbox
        .run(
            r#"
result = []
for x in (10, 20, 30):
    result.append(x)
"#,
        )
        .unwrap();
    assert_eq!(
        sandbox.run("result").unwrap(),
        PyValue::List(vec![PyValue::Int(10), PyValue::Int(20), PyValue::Int(30)])
    );
}

#[test]
fn test_tuple_comparison() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("(1, 2) == (1, 2)").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(sandbox.run("(1, 2) < (1, 3)").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("(1, 2) > (1, 1)").unwrap(), PyValue::Bool(true));
}

#[test]
fn test_tuple_as_dict_key() {
    let mut sandbox = Sandbox::new();

    sandbox.run("d = {}").unwrap();
    sandbox.run("d[(1, 2)] = 'hello'").unwrap();
    assert_eq!(
        sandbox.run("d[(1, 2)]").unwrap(),
        PyValue::Str("hello".to_string())
    );
}

// ============================================================================
// Non-string dict key tests
// ============================================================================

#[test]
fn test_dict_int_keys() {
    let mut sandbox = Sandbox::new();

    sandbox.run("d = {1: 'one', 2: 'two'}").unwrap();
    assert_eq!(
        sandbox.run("d[1]").unwrap(),
        PyValue::Str("one".to_string())
    );
    assert_eq!(
        sandbox.run("d[2]").unwrap(),
        PyValue::Str("two".to_string())
    );
}

#[test]
fn test_dict_bool_keys() {
    let mut sandbox = Sandbox::new();

    sandbox.run("d = {True: 'yes', False: 'no'}").unwrap();
    assert_eq!(
        sandbox.run("d[True]").unwrap(),
        PyValue::Str("yes".to_string())
    );
}

#[test]
fn test_dict_none_key() {
    let mut sandbox = Sandbox::new();

    sandbox.run("d = {None: 'nothing'}").unwrap();
    assert_eq!(
        sandbox.run("d[None]").unwrap(),
        PyValue::Str("nothing".to_string())
    );
}

#[test]
fn test_dict_mixed_keys() {
    let mut sandbox = Sandbox::new();

    sandbox
        .run("d = {1: 'int', 'a': 'str', (1,2): 'tuple'}")
        .unwrap();
    assert_eq!(
        sandbox.run("d[1]").unwrap(),
        PyValue::Str("int".to_string())
    );
    assert_eq!(
        sandbox.run("d['a']").unwrap(),
        PyValue::Str("str".to_string())
    );
    assert_eq!(
        sandbox.run("d[(1,2)]").unwrap(),
        PyValue::Str("tuple".to_string())
    );
}

#[test]
fn test_dict_unhashable_key_rejected() {
    let mut sandbox = Sandbox::new();

    // Lists are not hashable
    assert!(sandbox.run("{[1, 2]: 'bad'}").is_err());
}

#[test]
fn test_dict_int_key_membership() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("1 in {1: 'a', 2: 'b'}").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.run("3 in {1: 'a', 2: 'b'}").unwrap(),
        PyValue::Bool(false)
    );
}

#[test]
fn test_enumerate_returns_tuples() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("list(enumerate(['a', 'b']))").unwrap(),
        PyValue::List(vec![
            PyValue::Tuple(vec![PyValue::Int(0), PyValue::Str("a".to_string())]),
            PyValue::Tuple(vec![PyValue::Int(1), PyValue::Str("b".to_string())]),
        ])
    );
}

#[test]
fn test_zip_returns_tuples() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("list(zip([1, 2], ['a', 'b']))").unwrap(),
        PyValue::List(vec![
            PyValue::Tuple(vec![PyValue::Int(1), PyValue::Str("a".to_string())]),
            PyValue::Tuple(vec![PyValue::Int(2), PyValue::Str("b".to_string())]),
        ])
    );
}

#[test]
fn test_dict_items_returns_tuples() {
    let mut sandbox = Sandbox::new();

    // dict.items() should return list of tuples
    sandbox.run("d = {'a': 1}").unwrap();
    let result = sandbox.run("list(d.items())").unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![PyValue::Tuple(vec![
            PyValue::Str("a".to_string()),
            PyValue::Int(1),
        ])])
    );
}

// ============================================================================
// Tuple edge cases
// ============================================================================

#[test]
fn test_tuple_edge_cases() {
    let mut sandbox = Sandbox::new();

    // 1. Tuple slicing: (1,2,3,4)[1:3] → (2,3)
    assert_eq!(
        sandbox.run("(1,2,3,4)[1:3]").unwrap(),
        PyValue::Tuple(vec![PyValue::Int(2), PyValue::Int(3)])
    );

    // 2a. Tuple .index() method
    assert_eq!(
        sandbox.run("(10, 20, 30, 20).index(20)").unwrap(),
        PyValue::Int(1)
    );

    // 2b. Tuple .count() method
    assert_eq!(
        sandbox.run("(10, 20, 30, 20).count(20)").unwrap(),
        PyValue::Int(2)
    );

    // 3. len() on tuple
    assert_eq!(sandbox.run("len((1, 2, 3))").unwrap(), PyValue::Int(3));

    // 4. sorted() on tuple
    assert_eq!(
        sandbox.run("sorted((3, 1, 2))").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );

    // 5. tuple() with no args → empty tuple
    assert_eq!(sandbox.run("tuple()").unwrap(), PyValue::Tuple(vec![]));

    // 6. Nested tuples: ((1,2), (3,4))
    assert_eq!(
        sandbox.run("((1,2), (3,4))").unwrap(),
        PyValue::Tuple(vec![
            PyValue::Tuple(vec![PyValue::Int(1), PyValue::Int(2)]),
            PyValue::Tuple(vec![PyValue::Int(3), PyValue::Int(4)]),
        ])
    );

    // 7. Tuple in list comprehension: [x for x in (1,2,3)]
    assert_eq!(
        sandbox.run("[x for x in (1,2,3)]").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );

    // 8. min()/max() on tuple
    assert_eq!(sandbox.run("min((5, 2, 8))").unwrap(), PyValue::Int(2));
    assert_eq!(sandbox.run("max((5, 2, 8))").unwrap(), PyValue::Int(8));

    // 9. sum() on tuple
    assert_eq!(sandbox.run("sum((1, 2, 3))").unwrap(), PyValue::Int(6));

    // 10. any()/all() on tuple
    assert_eq!(sandbox.run("any((0, 0, 1))").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("any((0, 0, 0))").unwrap(), PyValue::Bool(false));
    assert_eq!(sandbox.run("all((1, 2, 3))").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("all((1, 0, 3))").unwrap(), PyValue::Bool(false));

    // 11. reversed() on tuple → list
    assert_eq!(
        sandbox.run("list(reversed((1, 2, 3)))").unwrap(),
        PyValue::List(vec![PyValue::Int(3), PyValue::Int(2), PyValue::Int(1)])
    );

    // 12. Print format for tuples
    let out = sandbox.capture("print((1, 2))").unwrap();
    assert_eq!(out.output, vec!["(1, 2)"]);

    let out = sandbox.capture("print((1,))").unwrap();
    assert_eq!(out.output, vec!["(1,)"]);

    let out = sandbox.capture("print(())").unwrap();
    assert_eq!(out.output, vec!["()"]);
}

// ============================================================================
// Set edge cases
// ============================================================================

#[test]
fn test_set_edge_cases() {
    let mut sandbox = Sandbox::new();

    // 1. set() from string → set of characters
    // Sets are unordered, so we check via sorted list
    assert_eq!(
        sandbox.run("sorted(set('abca'))").unwrap(),
        PyValue::List(vec![
            PyValue::Str("a".to_string()),
            PyValue::Str("b".to_string()),
            PyValue::Str("c".to_string()),
        ])
    );

    // 2. set() from tuple
    assert_eq!(
        sandbox.run("sorted(set((3, 1, 2, 1)))").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );

    // 3. sorted() on set
    assert_eq!(
        sandbox.run("sorted({3, 1, 2})").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );

    // 4. list() on set (order not guaranteed, so sort after)
    assert_eq!(
        sandbox.run("sorted(list({3, 1, 2}))").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );

    // 5. sum()/min()/max() on set
    assert_eq!(sandbox.run("sum({1, 2, 3})").unwrap(), PyValue::Int(6));
    assert_eq!(sandbox.run("min({5, 2, 8})").unwrap(), PyValue::Int(2));
    assert_eq!(sandbox.run("max({5, 2, 8})").unwrap(), PyValue::Int(8));

    // 6. any()/all() on set
    assert_eq!(sandbox.run("any({0, 1})").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.run("any({0})").unwrap(), PyValue::Bool(false));
    assert_eq!(sandbox.run("all({1, 2, 3})").unwrap(), PyValue::Bool(true));

    // 7. Set .pop() — removes an arbitrary element; just check it doesn't error
    //    and that the set shrinks by 1
    sandbox.run("s = {10, 20, 30}").unwrap();
    sandbox.run("s.pop()").unwrap();
    assert_eq!(sandbox.run("len(s)").unwrap(), PyValue::Int(2));

    // 8. Set .copy()
    sandbox.run("a = {1, 2, 3}").unwrap();
    sandbox.run("b = a.copy()").unwrap();
    assert_eq!(
        sandbox.run("sorted(b)").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );

    // 9. Set of mixed types: {1, 'a', True}
    //    In Python, True == 1 so {1, 'a', True} is {1, 'a'}
    //    Check that it doesn't crash at minimum
    let result = sandbox.run("len({1, 'a', True})");
    assert!(result.is_ok());

    // 10. Set containing tuples: {(1,2), (3,4)}
    assert_eq!(
        sandbox.run("sorted({(1,2), (3,4)})").unwrap(),
        PyValue::List(vec![
            PyValue::Tuple(vec![PyValue::Int(1), PyValue::Int(2)]),
            PyValue::Tuple(vec![PyValue::Int(3), PyValue::Int(4)]),
        ])
    );

    // 11. enumerate() over set — just check it works and produces tuples
    sandbox.run("s = {100}").unwrap();
    assert_eq!(
        sandbox.run("list(enumerate(s))").unwrap(),
        PyValue::List(vec![PyValue::Tuple(vec![
            PyValue::Int(0),
            PyValue::Int(100)
        ]),])
    );

    // 12. Nested set operations: ({1,2} | {3}) & {1, 3}
    assert_eq!(
        sandbox.run("sorted(({1,2} | {3}) & {1, 3})").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(3)])
    );

    // 13. set([1, 2, 2, 3]) dedup
    assert_eq!(
        sandbox.run("sorted(set([1, 2, 2, 3]))").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
}

// ============================================================================
// Dict key edge cases (non-string keys)
// ============================================================================

#[test]
fn test_dict_key_edge_cases() {
    let mut sandbox = Sandbox::new();

    // 1. dict.get() with int key
    sandbox.run("d = {1: 'one', 2: 'two'}").unwrap();
    assert_eq!(
        sandbox.run("d.get(1)").unwrap(),
        PyValue::Str("one".to_string())
    );
    assert_eq!(
        sandbox.run("d.get(99, 'missing')").unwrap(),
        PyValue::Str("missing".to_string())
    );

    // 2. dict.keys() returns non-string keys
    assert_eq!(
        sandbox.run("sorted(d.keys())").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2)])
    );

    // 3. dict.items() with int keys → list of tuples
    assert_eq!(
        sandbox.run("sorted(d.items())").unwrap(),
        PyValue::List(vec![
            PyValue::Tuple(vec![PyValue::Int(1), PyValue::Str("one".to_string())]),
            PyValue::Tuple(vec![PyValue::Int(2), PyValue::Str("two".to_string())]),
        ])
    );

    // 4. dict.pop() with int key
    sandbox.run("d = {1: 'a', 2: 'b'}").unwrap();
    assert_eq!(
        sandbox.run("d.pop(1)").unwrap(),
        PyValue::Str("a".to_string())
    );
    assert_eq!(sandbox.run("len(d)").unwrap(), PyValue::Int(1));

    // 5. dict.update() with non-string keys
    sandbox.run("d = {1: 'a'}").unwrap();
    sandbox.run("d.update({2: 'b', 3: 'c'})").unwrap();
    assert_eq!(sandbox.run("len(d)").unwrap(), PyValue::Int(3));
    assert_eq!(sandbox.run("d[2]").unwrap(), PyValue::Str("b".to_string()));

    // 6. Overwriting with same key: d = {1: 'a'}; d[1] = 'b'
    sandbox.run("d = {1: 'a'}").unwrap();
    sandbox.run("d[1] = 'b'").unwrap();
    assert_eq!(sandbox.run("d[1]").unwrap(), PyValue::Str("b".to_string()));
    assert_eq!(sandbox.run("len(d)").unwrap(), PyValue::Int(1));

    // 7. len() on dict with non-string keys
    assert_eq!(
        sandbox.run("len({10: 'x', 20: 'y', 30: 'z'})").unwrap(),
        PyValue::Int(3)
    );

    // 8. Iterating over dict with non-string keys (for k in d:)
    sandbox.run("d = {1: 'a', 2: 'b'}").unwrap();
    sandbox
        .run("keys = []\nfor k in d:\n    keys.append(k)")
        .unwrap();
    assert_eq!(
        sandbox.run("sorted(keys)").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2)])
    );
}

// ============================================================================
// Lambda and first-class function tests
// ============================================================================

#[test]
fn test_lambda_basic() {
    let mut sandbox = Sandbox::new();

    // Lambda assigned to variable and called
    sandbox.run("f = lambda x: x + 1").unwrap();
    assert_eq!(sandbox.run("f(5)").unwrap(), PyValue::Int(6));

    // Lambda with multiple params
    sandbox.run("add = lambda a, b: a + b").unwrap();
    assert_eq!(sandbox.run("add(3, 4)").unwrap(), PyValue::Int(7));
}

#[test]
fn test_lambda_inline_call() {
    let mut sandbox = Sandbox::new();

    // Inline lambda call: (lambda x: ...)(arg)
    assert_eq!(
        sandbox.run("(lambda x: x * 2)(21)").unwrap(),
        PyValue::Int(42)
    );
}

#[test]
fn test_lambda_no_params() {
    let mut sandbox = Sandbox::new();

    // Zero-param lambda
    sandbox.run("f = lambda: 42").unwrap();
    assert_eq!(sandbox.run("f()").unwrap(), PyValue::Int(42));
}

#[test]
fn test_lambda_with_defaults() {
    let mut sandbox = Sandbox::new();

    sandbox.run("f = lambda x, y=10: x + y").unwrap();
    assert_eq!(sandbox.run("f(5)").unwrap(), PyValue::Int(15));
    assert_eq!(sandbox.run("f(5, 20)").unwrap(), PyValue::Int(25));
}

#[test]
fn test_first_class_function_assign() {
    let mut sandbox = Sandbox::new();

    // def function assigned to variable and called through it
    sandbox.run("def double(x):\n    return x * 2").unwrap();
    sandbox.run("f = double").unwrap();
    assert_eq!(sandbox.run("f(21)").unwrap(), PyValue::Int(42));
}

#[test]
fn test_first_class_function_as_argument() {
    let mut sandbox = Sandbox::new();

    // Pass function as argument to another function
    sandbox.run("def apply(f, x):\n    return f(x)").unwrap();
    assert_eq!(
        sandbox.run("apply(lambda x: x * x, 5)").unwrap(),
        PyValue::Int(25)
    );
}

#[test]
fn test_function_type_name() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("type(lambda x: x)").unwrap(),
        PyValue::Str("function".to_string())
    );

    sandbox.run("def foo():\n    pass").unwrap();
    assert_eq!(
        sandbox.run("type(foo)").unwrap(),
        PyValue::Str("function".to_string())
    );
}

#[test]
fn test_function_truthiness() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("bool(lambda x: x)").unwrap(),
        PyValue::Bool(true)
    );
}

#[test]
fn test_sorted_with_reverse() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.run("sorted([3, 1, 2], reverse=True)").unwrap(),
        PyValue::List(vec![PyValue::Int(3), PyValue::Int(2), PyValue::Int(1)])
    );

    assert_eq!(
        sandbox.run("sorted([3, 1, 2], reverse=False)").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
}

#[test]
fn test_sorted_with_key() {
    let mut sandbox = Sandbox::new();

    // Sort strings by length
    assert_eq!(
        sandbox
            .run("sorted(['banana', 'apple', 'cherry'], key=lambda x: len(x))")
            .unwrap(),
        PyValue::List(vec![
            PyValue::Str("apple".to_string()),
            PyValue::Str("banana".to_string()),
            PyValue::Str("cherry".to_string()),
        ])
    );

    // Sort by absolute value
    assert_eq!(
        sandbox
            .run("sorted([-3, 1, -2, 4], key=lambda x: abs(x))")
            .unwrap(),
        PyValue::List(vec![
            PyValue::Int(1),
            PyValue::Int(-2),
            PyValue::Int(-3),
            PyValue::Int(4),
        ])
    );
}

#[test]
fn test_sorted_with_key_and_reverse() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox
            .run("sorted(['banana', 'apple', 'cherry'], key=lambda x: len(x), reverse=True)")
            .unwrap(),
        PyValue::List(vec![
            PyValue::Str("cherry".to_string()),
            PyValue::Str("banana".to_string()),
            PyValue::Str("apple".to_string()),
        ])
    );
}

#[test]
fn test_map_basic() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox
            .run("list(map(lambda x: x * 2, [1, 2, 3]))")
            .unwrap(),
        PyValue::List(vec![PyValue::Int(2), PyValue::Int(4), PyValue::Int(6)])
    );
}

#[test]
fn test_map_with_named_function() {
    let mut sandbox = Sandbox::new();

    sandbox.run("def square(x):\n    return x * x").unwrap();
    assert_eq!(
        sandbox.run("list(map(square, [1, 2, 3]))").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(4), PyValue::Int(9)])
    );
}

#[test]
fn test_filter_with_lambda() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox
            .run("list(filter(lambda x: x > 2, [1, 2, 3, 4]))")
            .unwrap(),
        PyValue::List(vec![PyValue::Int(3), PyValue::Int(4)])
    );
}

#[test]
fn test_filter_with_none() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox
            .run("list(filter(None, [0, 1, '', 'hello', False, True]))")
            .unwrap(),
        PyValue::List(vec![
            PyValue::Int(1),
            PyValue::Str("hello".to_string()),
            PyValue::Bool(true),
        ])
    );
}

#[test]
fn test_map_returns_list() {
    let mut sandbox = Sandbox::new();

    // map already returns a list (not a lazy iterator like Python 3)
    assert_eq!(
        sandbox.run("map(lambda x: x + 1, [10, 20])").unwrap(),
        PyValue::List(vec![PyValue::Int(11), PyValue::Int(21)])
    );
}

#[test]
fn test_filter_returns_list() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox
            .run("filter(lambda x: x > 0, [-1, 0, 1, 2])")
            .unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2)])
    );
}

#[test]
fn test_lambda_in_list_comprehension_equivalent() {
    let mut sandbox = Sandbox::new();

    // Use lambda with map to achieve list comprehension equivalent
    sandbox
        .run("result = list(map(lambda x: x ** 2, range(5)))")
        .unwrap();
    assert_eq!(
        sandbox.run("result").unwrap(),
        PyValue::List(vec![
            PyValue::Int(0),
            PyValue::Int(1),
            PyValue::Int(4),
            PyValue::Int(9),
            PyValue::Int(16),
        ])
    );
}

#[test]
fn test_not_callable_error() {
    let mut sandbox = Sandbox::new();

    let err = sandbox.run("(42)(1)").unwrap_err();
    assert!(err.to_string().contains("not callable"));
}

#[test]
fn test_function_value_in_data_structure() {
    let mut sandbox = Sandbox::new();

    // Store functions in a list
    sandbox
        .run("fns = [lambda x: x + 1, lambda x: x * 2]")
        .unwrap();
    assert_eq!(sandbox.run("fns[0](10)").unwrap(), PyValue::Int(11));
    assert_eq!(sandbox.run("fns[1](10)").unwrap(), PyValue::Int(20));
}

#[test]
fn test_lambda_with_varargs() {
    let mut sandbox = Sandbox::new();

    sandbox.run("f = lambda *args: len(args)").unwrap();
    assert_eq!(sandbox.run("f(1, 2, 3)").unwrap(), PyValue::Int(3));
}

#[test]
fn test_callable_value_via_call_value_kw() {
    let mut sandbox = Sandbox::new();

    // Call a lambda stored in a variable with keyword args
    sandbox.run("f = lambda x, y=10: x + y").unwrap();
    assert_eq!(sandbox.run("f(5, y=20)").unwrap(), PyValue::Int(25));
}

// ============================================================================
// Import system tests
// ============================================================================

#[test]
fn test_import_math() {
    let mut sandbox = Sandbox::with_builtins();
    assert_eq!(
        sandbox.run("import math; math.sqrt(16.0)").unwrap(),
        PyValue::Float(4.0)
    );
}

#[test]
fn test_import_math_pi() {
    let mut sandbox = Sandbox::with_builtins();
    assert_eq!(
        sandbox.run("import math; math.pi").unwrap(),
        PyValue::Float(PI)
    );
}

#[test]
fn test_from_math_import() {
    let mut sandbox = Sandbox::with_builtins();
    let result = sandbox.run("from math import pi; pi").unwrap();
    assert_eq!(result, PyValue::Float(PI));
}

#[test]
fn test_from_math_import_multiple() {
    let mut sandbox = Sandbox::with_builtins();
    sandbox
        .run("from math import pi, e, sqrt")
        .unwrap();
    assert_eq!(sandbox.run("pi").unwrap(), PyValue::Float(PI));
    assert_eq!(
        sandbox.run("e").unwrap(),
        PyValue::Float(std::f64::consts::E)
    );
}

#[test]
fn test_import_as() {
    let mut sandbox = Sandbox::with_builtins();
    assert_eq!(
        sandbox.run("import math as m; m.sqrt(25.0)").unwrap(),
        PyValue::Float(5.0)
    );
}

#[test]
fn test_from_import_as() {
    let mut sandbox = Sandbox::with_builtins();
    assert_eq!(
        sandbox
            .run("from math import sqrt as square_root; square_root(9.0)")
            .unwrap(),
        PyValue::Float(3.0)
    );
}

#[test]
fn test_import_json_loads() {
    let mut sandbox = Sandbox::with_builtins();
    let result = sandbox
        .run(r#"import json; json.loads('{"a": 1}')["a"]"#)
        .unwrap();
    assert_eq!(result, PyValue::Int(1));
}

#[test]
fn test_import_json_dumps() {
    let mut sandbox = Sandbox::with_builtins();
    let result = sandbox
        .run(r#"import json; json.dumps({"key": "value"})"#)
        .unwrap();
    assert_eq!(
        result,
        PyValue::Str(r#"{"key":"value"}"#.to_string())
    );
}

#[test]
fn test_from_json_import() {
    let mut sandbox = Sandbox::with_builtins();
    let result = sandbox
        .run(r#"from json import loads; loads('[1, 2, 3]')"#)
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
}

#[test]
fn test_import_typing() {
    let mut sandbox = Sandbox::with_builtins();
    // Typing imports should succeed without error (all values are None)
    sandbox
        .run("from typing import List, Dict, Optional, Any")
        .unwrap();
}

#[test]
fn test_import_typing_no_error() {
    let mut sandbox = Sandbox::with_builtins();
    let result = sandbox
        .run("from typing import Union; Union")
        .unwrap();
    assert_eq!(result, PyValue::None);
}

#[test]
fn test_import_nonexistent_module() {
    let mut sandbox = Sandbox::with_builtins();
    let err = sandbox.run("import os").unwrap_err();
    assert!(
        err.to_string().contains("ModuleNotFoundError"),
        "Expected ModuleNotFoundError, got: {}",
        err
    );
}

#[test]
fn test_from_import_nonexistent_attr() {
    let mut sandbox = Sandbox::with_builtins();
    let err = sandbox
        .run("from math import nonexistent")
        .unwrap_err();
    assert!(
        err.to_string().contains("AttributeError"),
        "Expected AttributeError, got: {}",
        err
    );
}

#[test]
fn test_custom_module_registration() {
    let mut sandbox = Sandbox::new();
    sandbox.module("mymod", |m| {
        m.constant("VERSION", PyValue::Str("1.0".to_string()));
        m.function("double", |args| {
            let x = args.first().and_then(|v| v.as_int()).unwrap_or(0);
            PyValue::Int(x * 2)
        });
    });

    assert_eq!(
        sandbox.run("import mymod; mymod.VERSION").unwrap(),
        PyValue::Str("1.0".to_string())
    );
    assert_eq!(
        sandbox.run("mymod.double(21)").unwrap(),
        PyValue::Int(42)
    );
}

#[test]
fn test_custom_module_from_import() {
    let mut sandbox = Sandbox::new();
    sandbox.module("tools", |m| {
        m.constant("PI", PyValue::Float(3.14));
        m.function("add", |args| {
            let a = args.first().and_then(|v| v.as_int()).unwrap_or(0);
            let b = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
            PyValue::Int(a + b)
        });
    });

    assert_eq!(
        sandbox.run("from tools import PI; PI").unwrap(),
        PyValue::Float(3.14)
    );
    assert_eq!(
        sandbox.run("from tools import add; add(3, 4)").unwrap(),
        PyValue::Int(7)
    );
}

#[test]
fn test_math_functions() {
    let mut sandbox = Sandbox::with_builtins();

    assert_eq!(
        sandbox.run("import math; math.floor(3.7)").unwrap(),
        PyValue::Int(3)
    );
    assert_eq!(
        sandbox.run("math.ceil(3.2)").unwrap(),
        PyValue::Int(4)
    );
    assert_eq!(
        sandbox.run("math.fabs(-5.0)").unwrap(),
        PyValue::Float(5.0)
    );
    assert_eq!(
        sandbox.run("math.isnan(math.nan)").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.run("math.isinf(math.inf)").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.run("math.pow(2.0, 10.0)").unwrap(),
        PyValue::Float(1024.0)
    );
}

#[test]
fn test_math_trig() {
    let mut sandbox = Sandbox::with_builtins();
    sandbox.run("import math").unwrap();

    // sin(0) = 0
    assert_eq!(
        sandbox.run("math.sin(0.0)").unwrap(),
        PyValue::Float(0.0)
    );
    // cos(0) = 1
    assert_eq!(
        sandbox.run("math.cos(0.0)").unwrap(),
        PyValue::Float(1.0)
    );
}

#[test]
fn test_math_gcd() {
    let mut sandbox = Sandbox::with_builtins();
    assert_eq!(
        sandbox.run("import math; math.gcd(12, 8)").unwrap(),
        PyValue::Int(4)
    );
}

#[test]
fn test_math_factorial() {
    let mut sandbox = Sandbox::with_builtins();
    assert_eq!(
        sandbox.run("import math; math.factorial(5)").unwrap(),
        PyValue::Int(120)
    );
}

#[test]
fn test_json_roundtrip() {
    let mut sandbox = Sandbox::with_builtins();
    let result = sandbox
        .run(r#"
import json
data = {"name": "Alice", "age": 30, "scores": [90, 85, 92]}
json.loads(json.dumps(data))
"#)
        .unwrap();
    // Verify the roundtrip preserves structure
    if let PyValue::Dict(pairs) = &result {
        assert_eq!(pairs.len(), 3);
    } else {
        panic!("Expected dict, got {:?}", result);
    }
}

#[test]
fn test_import_in_function() {
    let mut sandbox = Sandbox::with_builtins();
    let result = sandbox
        .run(
            r#"
def compute():
    import math
    return math.sqrt(144.0)
compute()
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Float(12.0));
}

#[test]
fn test_import_module_not_found_catchable() {
    let mut sandbox = Sandbox::with_builtins();
    let result = sandbox
        .run(
            r#"
try:
    import os
except Exception as e:
    result = "caught"
result
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::Str("caught".to_string()));
}

#[test]
fn test_attribute_access_on_module() {
    let mut sandbox = Sandbox::with_builtins();
    // Test that bare attribute access (not method call) works
    assert_eq!(
        sandbox.run("import math; x = math.pi; x").unwrap(),
        PyValue::Float(PI)
    );
}

#[test]
fn test_from_import_function_call_value() {
    let mut sandbox = Sandbox::with_builtins();
    // When we `from math import sqrt`, sqrt should be a NativeFunction
    // that's callable via call_function's variable lookup
    let result = sandbox
        .run("from math import sqrt; sqrt(49.0)")
        .unwrap();
    assert_eq!(result, PyValue::Float(7.0));
}

#[test]
fn test_with_builtins_has_all_modules() {
    let mut sandbox = Sandbox::with_builtins();
    // All three built-in modules should be importable
    sandbox.run("import json").unwrap();
    sandbox.run("import math").unwrap();
    sandbox.run("import typing").unwrap();
}
