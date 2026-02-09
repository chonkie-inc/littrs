//! Built-in Python functions for the sandbox.
//!
//! This module implements the built-in functions available in the sandbox:
//! - Type conversions: str, int, float, bool, list
//! - Sequences: len, range, sum, min, max
//! - I/O: print
//! - Math: abs

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::error::{Error, Result};
use crate::operators::compare_values;
use crate::value::PyValue;

/// Extract items from any iterable PyValue (list, tuple, set, dict keys, str chars).
fn to_iterable_items(val: &PyValue) -> Result<Vec<PyValue>> {
    match val {
        PyValue::List(items) | PyValue::Tuple(items) | PyValue::Set(items) => Ok(items.clone()),
        PyValue::Dict(pairs) => Ok(pairs.iter().map(|(k, _)| k.clone()).collect()),
        PyValue::Str(s) => Ok(s.chars().map(|c| PyValue::Str(c.to_string())).collect()),
        other => Err(Error::Type {
            expected: "iterable".to_string(),
            got: other.type_name().to_string(),
        }),
    }
}

/// Result of attempting to handle a builtin function call.
pub enum BuiltinResult {
    /// The function was handled and returned this value.
    Handled(Result<PyValue>),
    /// Not a builtin function, try other handlers.
    NotBuiltin,
}

/// Try to handle a builtin function call with pre-evaluated arguments.
pub fn try_builtin(
    func_name: &str,
    args: Vec<PyValue>,
    print_buffer: &mut Vec<String>,
) -> BuiltinResult {
    match func_name {
        "len" => BuiltinResult::Handled(builtin_len(args)),
        "str" => BuiltinResult::Handled(builtin_str(args)),
        "int" => BuiltinResult::Handled(builtin_int(args)),
        "float" => BuiltinResult::Handled(builtin_float(args)),
        "bool" => BuiltinResult::Handled(builtin_bool(args)),
        "list" => BuiltinResult::Handled(builtin_list(args)),
        "range" => BuiltinResult::Handled(builtin_range(args)),
        "enumerate" => BuiltinResult::Handled(builtin_enumerate(args)),
        "zip" => BuiltinResult::Handled(builtin_zip(args)),
        "reversed" => BuiltinResult::Handled(builtin_reversed(args)),
        "any" => BuiltinResult::Handled(builtin_any(args)),
        "all" => BuiltinResult::Handled(builtin_all(args)),
        "print" => BuiltinResult::Handled(builtin_print(args, print_buffer)),
        "abs" => BuiltinResult::Handled(builtin_abs(args)),
        "min" => BuiltinResult::Handled(builtin_min(args)),
        "max" => BuiltinResult::Handled(builtin_max(args)),
        "sum" => BuiltinResult::Handled(builtin_sum(args)),
        "isinstance" => BuiltinResult::Handled(builtin_isinstance(args)),
        "type" => BuiltinResult::Handled(builtin_type(args)),
        "tuple" => BuiltinResult::Handled(builtin_tuple(args)),
        "set" => BuiltinResult::Handled(builtin_set(args)),
        "repr" => BuiltinResult::Handled(builtin_repr(args)),
        "bin" => BuiltinResult::Handled(builtin_bin(args)),
        "hex" => BuiltinResult::Handled(builtin_hex(args)),
        "oct" => BuiltinResult::Handled(builtin_oct(args)),
        "divmod" => BuiltinResult::Handled(builtin_divmod(args)),
        "pow" => BuiltinResult::Handled(builtin_pow(args)),
        "hash" => BuiltinResult::Handled(builtin_hash(args)),
        _ => BuiltinResult::NotBuiltin,
    }
}

fn builtin_len(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("len() takes exactly 1 argument".to_string()));
    }
    let arg = &args[0];
    let len = match arg {
        PyValue::Str(s) => s.len(),
        PyValue::List(l) => l.len(),
        PyValue::Tuple(t) => t.len(),
        PyValue::Dict(d) => d.len(),
        PyValue::Set(s) => s.len(),
        _ => {
            return Err(Error::Type {
                expected: "sized".to_string(),
                got: arg.type_name().to_string(),
            });
        }
    };
    Ok(PyValue::Int(len as i64))
}

fn builtin_str(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("str() takes exactly 1 argument".to_string()));
    }
    Ok(PyValue::Str(format!("{}", args[0])))
}

fn builtin_int(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("int() takes exactly 1 argument".to_string()));
    }
    let arg = &args[0];
    let val = match arg {
        PyValue::Int(i) => *i,
        PyValue::Float(f) => *f as i64,
        PyValue::Bool(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        PyValue::Str(s) => s
            .parse()
            .map_err(|_| Error::Runtime(format!("invalid literal for int(): '{}'", s)))?,
        _ => {
            return Err(Error::Type {
                expected: "number or string".to_string(),
                got: arg.type_name().to_string(),
            });
        }
    };
    Ok(PyValue::Int(val))
}

fn builtin_float(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime(
            "float() takes exactly 1 argument".to_string(),
        ));
    }
    let arg = &args[0];
    let val = match arg {
        PyValue::Float(f) => *f,
        PyValue::Int(i) => *i as f64,
        PyValue::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        PyValue::Str(s) => s
            .parse()
            .map_err(|_| Error::Runtime(format!("invalid literal for float(): '{}'", s)))?,
        _ => {
            return Err(Error::Type {
                expected: "number or string".to_string(),
                got: arg.type_name().to_string(),
            });
        }
    };
    Ok(PyValue::Float(val))
}

fn builtin_bool(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime(
            "bool() takes exactly 1 argument".to_string(),
        ));
    }
    Ok(PyValue::Bool(args[0].is_truthy()))
}

fn builtin_list(args: Vec<PyValue>) -> Result<PyValue> {
    if args.is_empty() {
        return Ok(PyValue::List(vec![]));
    }
    if args.len() != 1 {
        return Err(Error::Runtime(
            "list() takes at most 1 argument".to_string(),
        ));
    }
    let items = to_iterable_items(&args[0])?;
    Ok(PyValue::List(items))
}

fn builtin_range(args: Vec<PyValue>) -> Result<PyValue> {
    let (start, stop, step) = match args.len() {
        1 => (
            0,
            args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })?,
            1,
        ),
        2 => (
            args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })?,
            args[1].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[1].type_name().to_string(),
            })?,
            1,
        ),
        3 => (
            args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })?,
            args[1].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[1].type_name().to_string(),
            })?,
            args[2].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[2].type_name().to_string(),
            })?,
        ),
        _ => return Err(Error::Runtime("range() takes 1 to 3 arguments".to_string())),
    };

    if step == 0 {
        return Err(Error::Runtime("range() step cannot be zero".to_string()));
    }

    let mut items = Vec::new();
    let mut i = start;
    if step > 0 {
        while i < stop {
            items.push(PyValue::Int(i));
            i += step;
        }
    } else {
        while i > stop {
            items.push(PyValue::Int(i));
            i += step;
        }
    }
    Ok(PyValue::List(items))
}

fn builtin_print(args: Vec<PyValue>, print_buffer: &mut Vec<String>) -> Result<PyValue> {
    let output: Vec<String> = args.iter().map(|v| v.to_print_string()).collect();
    print_buffer.push(output.join(" "));
    Ok(PyValue::None)
}

fn builtin_abs(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("abs() takes exactly 1 argument".to_string()));
    }
    match &args[0] {
        PyValue::Int(i) => Ok(PyValue::Int(i.abs())),
        PyValue::Float(f) => Ok(PyValue::Float(f.abs())),
        _ => Err(Error::Type {
            expected: "number".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_min(args: Vec<PyValue>) -> Result<PyValue> {
    if args.is_empty() {
        return Err(Error::Runtime(
            "min() requires at least 1 argument".to_string(),
        ));
    }

    if args.len() == 1
        && let Ok(items) = to_iterable_items(&args[0])
    {
        if items.is_empty() {
            return Err(Error::Runtime("min() arg is an empty sequence".to_string()));
        }
        return find_min(&items);
    }
    find_min(&args)
}

fn builtin_max(args: Vec<PyValue>) -> Result<PyValue> {
    if args.is_empty() {
        return Err(Error::Runtime(
            "max() requires at least 1 argument".to_string(),
        ));
    }

    if args.len() == 1
        && let Ok(items) = to_iterable_items(&args[0])
    {
        if items.is_empty() {
            return Err(Error::Runtime("max() arg is an empty sequence".to_string()));
        }
        return find_max(&items);
    }
    find_max(&args)
}

fn builtin_sum(args: Vec<PyValue>) -> Result<PyValue> {
    if args.is_empty() {
        return Err(Error::Runtime(
            "sum() requires at least 1 argument".to_string(),
        ));
    }
    let items = to_iterable_items(&args[0])?;

    let mut total = 0i64;
    let mut is_float = false;
    let mut total_float = 0.0f64;

    for item in &items {
        match item {
            PyValue::Int(i) => {
                if is_float {
                    total_float += *i as f64;
                } else {
                    total += *i;
                }
            }
            PyValue::Float(f) => {
                if !is_float {
                    is_float = true;
                    total_float = total as f64;
                }
                total_float += *f;
            }
            _ => {
                return Err(Error::Type {
                    expected: "number".to_string(),
                    got: item.type_name().to_string(),
                });
            }
        }
    }

    if is_float {
        Ok(PyValue::Float(total_float))
    } else {
        Ok(PyValue::Int(total))
    }
}

fn find_min(items: &[PyValue]) -> Result<PyValue> {
    let mut min = items[0].clone();
    for item in &items[1..] {
        if compare_values(item, &min, |a, b| a < b, |a, b| a < b)? {
            min = item.clone();
        }
    }
    Ok(min)
}

fn find_max(items: &[PyValue]) -> Result<PyValue> {
    let mut max = items[0].clone();
    for item in &items[1..] {
        if compare_values(item, &max, |a, b| a > b, |a, b| a > b)? {
            max = item.clone();
        }
    }
    Ok(max)
}

fn builtin_enumerate(args: Vec<PyValue>) -> Result<PyValue> {
    if args.is_empty() || args.len() > 2 {
        return Err(Error::Runtime(
            "enumerate() takes 1 or 2 arguments".to_string(),
        ));
    }

    let items = to_iterable_items(&args[0])?;

    let start = if args.len() > 1 {
        args[1].as_int().ok_or_else(|| Error::Type {
            expected: "int".to_string(),
            got: args[1].type_name().to_string(),
        })?
    } else {
        0
    };

    let result: Vec<PyValue> = items
        .into_iter()
        .enumerate()
        .map(|(i, v)| PyValue::Tuple(vec![PyValue::Int(start + i as i64), v]))
        .collect();

    Ok(PyValue::List(result))
}

fn builtin_zip(args: Vec<PyValue>) -> Result<PyValue> {
    if args.is_empty() {
        return Ok(PyValue::List(vec![]));
    }

    // Convert all arguments to lists
    let lists: Result<Vec<Vec<PyValue>>> = args.iter().map(to_iterable_items).collect();
    let lists = lists?;

    // Find the shortest length
    let min_len = lists.iter().map(|l| l.len()).min().unwrap_or(0);

    // Zip them together
    let result: Vec<PyValue> = (0..min_len)
        .map(|i| PyValue::Tuple(lists.iter().map(|l| l[i].clone()).collect()))
        .collect();

    Ok(PyValue::List(result))
}

fn builtin_reversed(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime(
            "reversed() takes exactly 1 argument".to_string(),
        ));
    }

    let mut items = to_iterable_items(&args[0])?;
    items.reverse();
    Ok(PyValue::List(items))
}

fn builtin_any(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("any() takes exactly 1 argument".to_string()));
    }

    let items = to_iterable_items(&args[0])?;
    Ok(PyValue::Bool(items.iter().any(|v| v.is_truthy())))
}

fn builtin_all(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("all() takes exactly 1 argument".to_string()));
    }

    let items = to_iterable_items(&args[0])?;
    Ok(PyValue::Bool(items.iter().all(|v| v.is_truthy())))
}

fn builtin_isinstance(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 2 {
        return Err(Error::Runtime(
            "isinstance() takes exactly 2 arguments".to_string(),
        ));
    }

    let type_name = args[1].as_str().ok_or_else(|| Error::Type {
        expected: "str (type name)".to_string(),
        got: args[1].type_name().to_string(),
    })?;

    let result = matches!(
        (type_name, &args[0]),
        ("str", PyValue::Str(_))
            | ("int", PyValue::Int(_))
            | ("float", PyValue::Float(_) | PyValue::Int(_))
            | ("bool", PyValue::Bool(_))
            | ("list", PyValue::List(_))
            | ("tuple", PyValue::Tuple(_))
            | ("dict", PyValue::Dict(_))
            | ("set", PyValue::Set(_))
            | ("None" | "NoneType", PyValue::None)
    );

    Ok(PyValue::Bool(result))
}

fn builtin_type(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime(
            "type() takes exactly 1 argument".to_string(),
        ));
    }

    Ok(PyValue::Str(args[0].type_name().to_string()))
}

fn builtin_tuple(args: Vec<PyValue>) -> Result<PyValue> {
    if args.is_empty() {
        return Ok(PyValue::Tuple(vec![]));
    }
    if args.len() != 1 {
        return Err(Error::Runtime(
            "tuple() takes at most 1 argument".to_string(),
        ));
    }
    let items = to_iterable_items(&args[0])?;
    Ok(PyValue::Tuple(items))
}

fn builtin_set(args: Vec<PyValue>) -> Result<PyValue> {
    if args.is_empty() {
        return Ok(PyValue::Set(vec![]));
    }
    if args.len() != 1 {
        return Err(Error::Runtime("set() takes at most 1 argument".to_string()));
    }
    let raw_items = to_iterable_items(&args[0])?;
    // Deduplicate and check hashability
    let mut items = Vec::new();
    for elem in raw_items {
        if !elem.is_hashable() {
            return Err(Error::Runtime(format!(
                "TypeError: unhashable type: '{}'",
                elem.type_name()
            )));
        }
        if !items.contains(&elem) {
            items.push(elem);
        }
    }
    Ok(PyValue::Set(items))
}

fn builtin_repr(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime(
            "repr() takes exactly 1 argument".to_string(),
        ));
    }
    Ok(PyValue::Str(format!("{}", args[0])))
}

fn builtin_bin(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("bin() takes exactly 1 argument".to_string()));
    }
    match &args[0] {
        PyValue::Int(n) => {
            if *n < 0 {
                Ok(PyValue::Str(format!("-0b{:b}", -n)))
            } else {
                Ok(PyValue::Str(format!("0b{:b}", n)))
            }
        }
        PyValue::Bool(b) => Ok(PyValue::Str(format!("0b{:b}", *b as i64))),
        other => Err(Error::Type {
            expected: "int".to_string(),
            got: other.type_name().to_string(),
        }),
    }
}

fn builtin_hex(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("hex() takes exactly 1 argument".to_string()));
    }
    match &args[0] {
        PyValue::Int(n) => {
            if *n < 0 {
                Ok(PyValue::Str(format!("-0x{:x}", -n)))
            } else {
                Ok(PyValue::Str(format!("0x{:x}", n)))
            }
        }
        PyValue::Bool(b) => Ok(PyValue::Str(format!("0x{:x}", *b as i64))),
        other => Err(Error::Type {
            expected: "int".to_string(),
            got: other.type_name().to_string(),
        }),
    }
}

fn builtin_oct(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime("oct() takes exactly 1 argument".to_string()));
    }
    match &args[0] {
        PyValue::Int(n) => {
            if *n < 0 {
                Ok(PyValue::Str(format!("-0o{:o}", -n)))
            } else {
                Ok(PyValue::Str(format!("0o{:o}", n)))
            }
        }
        PyValue::Bool(b) => Ok(PyValue::Str(format!("0o{:o}", *b as i64))),
        other => Err(Error::Type {
            expected: "int".to_string(),
            got: other.type_name().to_string(),
        }),
    }
}

fn builtin_divmod(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 2 {
        return Err(Error::Runtime(
            "divmod() takes exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (PyValue::Int(a), PyValue::Int(b)) => {
            if *b == 0 {
                return Err(Error::DivisionByZero);
            }
            let q = (*a as f64 / *b as f64).floor() as i64;
            let r = a - q * b;
            Ok(PyValue::Tuple(vec![PyValue::Int(q), PyValue::Int(r)]))
        }
        (a_val, b_val) => {
            let a = a_val.as_float().ok_or_else(|| Error::Type {
                expected: "number".to_string(),
                got: a_val.type_name().to_string(),
            })?;
            let b = b_val.as_float().ok_or_else(|| Error::Type {
                expected: "number".to_string(),
                got: b_val.type_name().to_string(),
            })?;
            if b == 0.0 {
                return Err(Error::DivisionByZero);
            }
            let q = (a / b).floor();
            let r = a - q * b;
            Ok(PyValue::Tuple(vec![PyValue::Float(q), PyValue::Float(r)]))
        }
    }
}

fn builtin_pow(args: Vec<PyValue>) -> Result<PyValue> {
    match args.len() {
        2 => {
            // 2-arg pow: same as ** operator
            match (&args[0], &args[1]) {
                (PyValue::Int(base), PyValue::Int(exp)) => {
                    if *exp < 0 {
                        Ok(PyValue::Float((*base as f64).powi(*exp as i32)))
                    } else {
                        Ok(PyValue::Int(base.wrapping_pow(*exp as u32)))
                    }
                }
                (a, b) => {
                    let base = a.as_float().ok_or_else(|| Error::Type {
                        expected: "number".to_string(),
                        got: a.type_name().to_string(),
                    })?;
                    let exp = b.as_float().ok_or_else(|| Error::Type {
                        expected: "number".to_string(),
                        got: b.type_name().to_string(),
                    })?;
                    Ok(PyValue::Float(base.powf(exp)))
                }
            }
        }
        3 => {
            // 3-arg pow: modular exponentiation, all ints
            let base = args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            let exp = args[1].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[1].type_name().to_string(),
            })?;
            let modulus = args[2].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[2].type_name().to_string(),
            })?;
            if modulus == 0 {
                return Err(Error::Runtime(
                    "ValueError: pow() 3rd argument cannot be 0".to_string(),
                ));
            }
            if exp < 0 {
                return Err(Error::Runtime(
                    "ValueError: pow() 2nd argument cannot be negative when 3rd argument specified"
                        .to_string(),
                ));
            }
            // Modular exponentiation by repeated squaring
            let mut result: i64 = 1;
            let mut base = base % modulus;
            let mut exp = exp;
            while exp > 0 {
                if exp % 2 == 1 {
                    result = ((result as i128 * base as i128) % modulus as i128) as i64;
                }
                exp /= 2;
                base = ((base as i128 * base as i128) % modulus as i128) as i64;
            }
            Ok(PyValue::Int(((result % modulus) + modulus) % modulus))
        }
        _ => Err(Error::Runtime(
            "pow() takes 2 or 3 arguments".to_string(),
        )),
    }
}

fn builtin_hash(args: Vec<PyValue>) -> Result<PyValue> {
    if args.len() != 1 {
        return Err(Error::Runtime(
            "hash() takes exactly 1 argument".to_string(),
        ));
    }
    if !args[0].is_hashable() {
        return Err(Error::Runtime(format!(
            "TypeError: unhashable type: '{}'",
            args[0].type_name()
        )));
    }
    Ok(PyValue::Int(hash_pyvalue(&args[0]) as i64))
}

fn hash_pyvalue(val: &PyValue) -> u64 {
    let mut hasher = DefaultHasher::new();
    match val {
        PyValue::None => {
            0u8.hash(&mut hasher);
        }
        PyValue::Bool(b) => {
            1u8.hash(&mut hasher);
            b.hash(&mut hasher);
        }
        PyValue::Int(i) => {
            2u8.hash(&mut hasher);
            i.hash(&mut hasher);
        }
        PyValue::Float(f) => {
            3u8.hash(&mut hasher);
            f.to_bits().hash(&mut hasher);
        }
        PyValue::Str(s) => {
            4u8.hash(&mut hasher);
            s.hash(&mut hasher);
        }
        PyValue::Tuple(items) => {
            5u8.hash(&mut hasher);
            for item in items {
                hash_pyvalue(item).hash(&mut hasher);
            }
        }
        _ => {
            // Unreachable if is_hashable() is checked first
            0u8.hash(&mut hasher);
        }
    }
    hasher.finish()
}
