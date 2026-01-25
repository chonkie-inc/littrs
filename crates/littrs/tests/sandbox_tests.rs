use littrs::{PyValue, Sandbox};

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
