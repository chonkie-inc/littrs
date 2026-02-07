use littrs::{PyValue, ResourceLimits, Sandbox};

#[test]
fn test_basic_arithmetic() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.execute("2 + 2").unwrap(), PyValue::Int(4));
    assert_eq!(sandbox.execute("10 - 3").unwrap(), PyValue::Int(7));
    assert_eq!(sandbox.execute("4 * 5").unwrap(), PyValue::Int(20));
    assert_eq!(sandbox.execute("10 / 4").unwrap(), PyValue::Float(2.5));
    assert_eq!(sandbox.execute("10 // 3").unwrap(), PyValue::Int(3));
    assert_eq!(sandbox.execute("10 % 3").unwrap(), PyValue::Int(1));
    assert_eq!(sandbox.execute("2 ** 8").unwrap(), PyValue::Int(256));
}

#[test]
fn test_variables() {
    let mut sandbox = Sandbox::new();

    sandbox.execute("x = 10").unwrap();
    sandbox.execute("y = 20").unwrap();
    assert_eq!(sandbox.execute("x + y").unwrap(), PyValue::Int(30));
}

#[test]
fn test_strings() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.execute("'hello' + ' ' + 'world'").unwrap(),
        PyValue::Str("hello world".to_string())
    );
    assert_eq!(
        sandbox.execute("'ab' * 3").unwrap(),
        PyValue::Str("ababab".to_string())
    );
}

#[test]
fn test_lists() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.execute("[1, 2, 3]").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );

    sandbox.execute("nums = [10, 20, 30]").unwrap();
    assert_eq!(sandbox.execute("nums[0]").unwrap(), PyValue::Int(10));
    assert_eq!(sandbox.execute("nums[-1]").unwrap(), PyValue::Int(30));
}

#[test]
fn test_dict() {
    let mut sandbox = Sandbox::new();

    sandbox
        .execute("data = {'name': 'Alice', 'age': 30}")
        .unwrap();
    assert_eq!(
        sandbox.execute("data['name']").unwrap(),
        PyValue::Str("Alice".to_string())
    );
    assert_eq!(sandbox.execute("data['age']").unwrap(), PyValue::Int(30));
}

#[test]
fn test_comparisons() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.execute("5 > 3").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.execute("5 < 3").unwrap(), PyValue::Bool(false));
    assert_eq!(sandbox.execute("5 == 5").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.execute("5 != 3").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.execute("1 < 2 < 3").unwrap(), PyValue::Bool(true));
}

#[test]
fn test_boolean_ops() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.execute("True and False").unwrap(),
        PyValue::Bool(false)
    );
    assert_eq!(
        sandbox.execute("True or False").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(sandbox.execute("not True").unwrap(), PyValue::Bool(false));
}

#[test]
fn test_if_statement() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
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
        .execute(
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
        .execute(
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

    assert_eq!(sandbox.execute("len('hello')").unwrap(), PyValue::Int(5));
    assert_eq!(sandbox.execute("len([1, 2, 3])").unwrap(), PyValue::Int(3));
}

#[test]
fn test_builtin_range() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.execute("range(3)").unwrap(),
        PyValue::List(vec![PyValue::Int(0), PyValue::Int(1), PyValue::Int(2)])
    );
    assert_eq!(
        sandbox.execute("range(1, 4)").unwrap(),
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
    assert_eq!(
        sandbox.execute("range(0, 10, 2)").unwrap(),
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

    assert_eq!(
        sandbox.execute("sum([1, 2, 3, 4])").unwrap(),
        PyValue::Int(10)
    );
    assert_eq!(sandbox.execute("min(5, 3, 8)").unwrap(), PyValue::Int(3));
    assert_eq!(sandbox.execute("max(5, 3, 8)").unwrap(), PyValue::Int(8));
    assert_eq!(
        sandbox.execute("min([5, 3, 8])").unwrap(),
        PyValue::Int(3)
    );
}

#[test]
fn test_register_tool() {
    let mut sandbox = Sandbox::new();

    sandbox.register_fn("double", |args| {
        let n = args[0].as_int().unwrap_or(0);
        PyValue::Int(n * 2)
    });

    assert_eq!(sandbox.execute("double(21)").unwrap(), PyValue::Int(42));
}

#[test]
fn test_tool_with_multiple_args() {
    let mut sandbox = Sandbox::new();

    sandbox.register_fn("add_all", |args| {
        let sum: i64 = args.iter().filter_map(|v| v.as_int()).sum();
        PyValue::Int(sum)
    });

    assert_eq!(
        sandbox.execute("add_all(1, 2, 3, 4, 5)").unwrap(),
        PyValue::Int(15)
    );
}

#[test]
fn test_tool_returning_dict() {
    let mut sandbox = Sandbox::new();

    sandbox.register_fn("get_user", |args| {
        let id = args[0].as_int().unwrap_or(0);
        PyValue::Dict(vec![
            ("id".to_string(), PyValue::Int(id)),
            ("name".to_string(), PyValue::Str("Test User".to_string())),
        ])
    });

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
    let mut sandbox = Sandbox::new();

    sandbox.set_variable("config_value", PyValue::Int(100));
    assert_eq!(
        sandbox.execute("config_value * 2").unwrap(),
        PyValue::Int(200)
    );
}

#[test]
fn test_in_operator() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.execute("'a' in 'abc'").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.execute("'x' in 'abc'").unwrap(),
        PyValue::Bool(false)
    );
    assert_eq!(
        sandbox.execute("2 in [1, 2, 3]").unwrap(),
        PyValue::Bool(true)
    );
    assert_eq!(
        sandbox.execute("5 not in [1, 2, 3]").unwrap(),
        PyValue::Bool(true)
    );
}

#[test]
fn test_ternary_expression() {
    let mut sandbox = Sandbox::new();

    assert_eq!(
        sandbox.execute("'yes' if True else 'no'").unwrap(),
        PyValue::Str("yes".to_string())
    );
    assert_eq!(
        sandbox.execute("'yes' if False else 'no'").unwrap(),
        PyValue::Str("no".to_string())
    );
}

#[test]
fn test_augmented_assignment() {
    let mut sandbox = Sandbox::new();

    sandbox.execute("x = 10").unwrap();
    sandbox.execute("x += 5").unwrap();
    assert_eq!(sandbox.execute("x").unwrap(), PyValue::Int(15));

    sandbox.execute("x *= 2").unwrap();
    assert_eq!(sandbox.execute("x").unwrap(), PyValue::Int(30));
}

#[test]
fn test_negative_numbers() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.execute("-5").unwrap(), PyValue::Int(-5));
    assert_eq!(sandbox.execute("-5 + 10").unwrap(), PyValue::Int(5));
    assert_eq!(sandbox.execute("abs(-42)").unwrap(), PyValue::Int(42));
}

#[test]
fn test_type_conversions() {
    let mut sandbox = Sandbox::new();

    assert_eq!(sandbox.execute("int(3.7)").unwrap(), PyValue::Int(3));
    assert_eq!(sandbox.execute("float(5)").unwrap(), PyValue::Float(5.0));
    assert_eq!(
        sandbox.execute("str(42)").unwrap(),
        PyValue::Str("42".to_string())
    );
    assert_eq!(sandbox.execute("bool(1)").unwrap(), PyValue::Bool(true));
    assert_eq!(sandbox.execute("bool(0)").unwrap(), PyValue::Bool(false));
}

#[test]
fn test_list_subscript_assignment() {
    let mut sandbox = Sandbox::new();

    sandbox.execute("nums = [1, 2, 3]").unwrap();
    sandbox.execute("nums[1] = 99").unwrap();
    assert_eq!(sandbox.execute("nums[1]").unwrap(), PyValue::Int(99));
}

#[test]
fn test_division_by_zero() {
    let mut sandbox = Sandbox::new();

    let result = sandbox.execute("10 / 0");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Division by zero"));
}

#[test]
fn test_undefined_variable() {
    let mut sandbox = Sandbox::new();

    let result = sandbox.execute("undefined_var");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not defined"));
}

#[test]
fn test_complex_expression() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
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
        .arg_required("name", "str", "The name")
        .arg_optional("greeting", "str", "The greeting")
        .returns("str");

    sandbox.register_tool(info, |args| {
        let name = args.get(0).and_then(|v| v.as_str()).unwrap_or("World");
        let greeting = args
            .get(1)
            .and_then(|v| v.as_str())
            .unwrap_or("Hello");
        PyValue::Str(format!("{}, {}!", greeting, name))
    });

    // Test with positional args
    assert_eq!(
        sandbox.execute("greet('Alice', 'Hi')").unwrap(),
        PyValue::Str("Hi, Alice!".to_string())
    );

    // Test with keyword args
    assert_eq!(
        sandbox.execute("greet(name='Bob')").unwrap(),
        PyValue::Str("Hello, Bob!".to_string())
    );

    // Test with mixed positional and keyword args
    assert_eq!(
        sandbox.execute("greet('Charlie', greeting='Hey')").unwrap(),
        PyValue::Str("Hey, Charlie!".to_string())
    );

    // Test with keyword args in different order
    assert_eq!(
        sandbox.execute("greet(greeting='Welcome', name='Dave')").unwrap(),
        PyValue::Str("Welcome, Dave!".to_string())
    );
}

#[test]
fn test_list_comprehension_basic() {
    let mut sandbox = Sandbox::new();

    // Basic list comprehension
    assert_eq!(
        sandbox.execute("[x for x in range(5)]").unwrap(),
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
        sandbox.execute("[x * 2 for x in range(4)]").unwrap(),
        PyValue::List(vec![
            PyValue::Int(0),
            PyValue::Int(2),
            PyValue::Int(4),
            PyValue::Int(6),
        ])
    );

    // Squares
    assert_eq!(
        sandbox.execute("[x ** 2 for x in range(1, 5)]").unwrap(),
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
        sandbox.execute("[x for x in range(10) if x % 2 == 0]").unwrap(),
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
        sandbox.execute("[x * 2 for x in range(5) if x > 1]").unwrap(),
        PyValue::List(vec![
            PyValue::Int(4),
            PyValue::Int(6),
            PyValue::Int(8),
        ])
    );
}

#[test]
fn test_list_comprehension_over_list() {
    let mut sandbox = Sandbox::new();

    sandbox.execute("nums = [1, 2, 3, 4, 5]").unwrap();

    assert_eq!(
        sandbox.execute("[n + 10 for n in nums]").unwrap(),
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
        sandbox.execute("[c for c in 'abc']").unwrap(),
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
        sandbox.execute("[x * y for x in range(1, 3) for y in range(1, 3)]").unwrap(),
        PyValue::List(vec![
            PyValue::Int(1),  // 1*1
            PyValue::Int(2),  // 1*2
            PyValue::Int(2),  // 2*1
            PyValue::Int(4),  // 2*2
        ])
    );
}

#[test]
fn test_list_comprehension_multiple_filters() {
    let mut sandbox = Sandbox::new();

    // Multiple if conditions
    assert_eq!(
        sandbox.execute("[x for x in range(20) if x % 2 == 0 if x % 3 == 0]").unwrap(),
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
    sandbox.set_limits(ResourceLimits {
        max_instructions: Some(1_000),
        ..Default::default()
    });

    let err = sandbox.execute("while True: pass").unwrap_err();
    assert!(err.to_string().contains("Instruction limit"));
}

#[test]
fn test_recursion_limit() {
    let mut sandbox = Sandbox::new();
    sandbox.set_limits(ResourceLimits {
        max_recursion_depth: Some(10),
        ..Default::default()
    });

    let err = sandbox
        .execute(
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
    sandbox.set_limits(ResourceLimits {
        max_instructions: Some(10_000),
        max_recursion_depth: Some(50),
    });

    let result = sandbox
        .execute(
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
    sandbox.set_limits(ResourceLimits {
        max_instructions: Some(500),
        max_recursion_depth: None,
    });

    // Short code should succeed
    let result = sandbox.execute("1 + 2").unwrap();
    assert_eq!(result, PyValue::Int(3));
}

#[test]
fn test_recursion_limit_only() {
    let mut sandbox = Sandbox::new();
    sandbox.set_limits(ResourceLimits {
        max_instructions: None,
        max_recursion_depth: Some(5),
    });

    // Moderate recursion within limit should succeed
    let result = sandbox
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
            r#"
def f(a, b=2, c=3):
    return a + b + c
[f(1), f(1, 20), f(1, 20, 30)]
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![
            PyValue::Int(6),
            PyValue::Int(24),
            PyValue::Int(51),
        ])
    );
}

#[test]
fn test_default_param_with_keyword_override() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
        .execute(
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
    sandbox.set_limits(ResourceLimits {
        max_instructions: Some(100),
        ..Default::default()
    });

    let err = sandbox
        .execute(
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
        .execute(
            r#"
def f(*args):
    return args
f(1, 2, 3)
"#,
        )
        .unwrap();
    assert_eq!(
        result,
        PyValue::List(vec![PyValue::Int(1), PyValue::Int(2), PyValue::Int(3)])
    );
}

#[test]
fn test_varargs_with_positional() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
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
            PyValue::List(vec![PyValue::Int(2), PyValue::Int(3)])
        ])
    );
}

#[test]
fn test_varargs_empty() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
            r#"
def f(a, *args):
    return args
f(1)
"#,
        )
        .unwrap();
    assert_eq!(result, PyValue::List(vec![]));
}

#[test]
fn test_kwargs_basic() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
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
            ("x".to_string(), PyValue::Int(1)),
            ("y".to_string(), PyValue::Int(2)),
        ])
    );
}

#[test]
fn test_kwargs_with_positional() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
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
                ("x".to_string(), PyValue::Int(2)),
                ("y".to_string(), PyValue::Int(3)),
            ])
        ])
    );
}

#[test]
fn test_kwargs_empty() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
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
        .execute(
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
            PyValue::List(vec![PyValue::Int(20), PyValue::Int(30)]),
            PyValue::Dict(vec![("x".to_string(), PyValue::Int(99))]),
        ])
    );
}

#[test]
fn test_varargs_sum() {
    let mut sandbox = Sandbox::new();

    let result = sandbox
        .execute(
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
        .execute(
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
        .execute(
            r#"
def f(a, b):
    return a + b
f(1, 2, c=3)
"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("unexpected keyword"));
}
