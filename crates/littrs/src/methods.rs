//! Method implementations for Python types.
//!
//! This module contains the implementations of methods for str, list, and dict types.

use crate::error::{Error, Result};
use crate::value::{PyValue, SetIndex};

// ============================================================================
// Argument validation helpers
// ============================================================================

/// Check that `args` has exactly `n` elements, or return an error naming `func`.
pub(crate) fn check_args(func: &str, args: &[PyValue], n: usize) -> Result<()> {
    if args.len() != n {
        let msg = if n == 0 {
            format!("{}() takes no arguments", func)
        } else {
            format!(
                "{}() takes exactly {} argument{}",
                func,
                n,
                if n == 1 { "" } else { "s" }
            )
        };
        Err(Error::Runtime(msg))
    } else {
        Ok(())
    }
}

/// Check that `args.len()` is in `[min, max]`, or return an error naming `func`.
pub(crate) fn check_args_range(func: &str, args: &[PyValue], min: usize, max: usize) -> Result<()> {
    if args.len() < min || args.len() > max {
        Err(Error::Runtime(format!(
            "{}() takes {} to {} arguments",
            func, min, max
        )))
    } else {
        Ok(())
    }
}

/// Extract a `&str` from a `PyValue`, or return a type error.
pub(crate) fn arg_str(arg: &PyValue) -> Result<&str> {
    arg.as_str().ok_or_else(|| Error::Type {
        expected: "str".to_string(),
        got: arg.type_name().to_string(),
    })
}

/// Extract an `i64` from a `PyValue`, or return a type error.
pub(crate) fn arg_int(arg: &PyValue) -> Result<i64> {
    arg.as_int().ok_or_else(|| Error::Type {
        expected: "int".to_string(),
        got: arg.type_name().to_string(),
    })
}

/// Extract an `f64` from a `PyValue`, or return a type error.
pub(crate) fn arg_float(arg: &PyValue) -> Result<f64> {
    arg.as_float().ok_or_else(|| Error::Type {
        expected: "number".to_string(),
        got: arg.type_name().to_string(),
    })
}

// ============================================================================

/// Call a method on a string value.
pub fn call_str_method(s: &str, method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "lower" => {
            check_args("lower", &args, 0)?;
            Ok(PyValue::Str(s.to_lowercase()))
        }
        "upper" => {
            check_args("upper", &args, 0)?;
            Ok(PyValue::Str(s.to_uppercase()))
        }
        "strip" => {
            check_args("strip", &args, 0)?;
            Ok(PyValue::Str(s.trim().to_string()))
        }
        "lstrip" => {
            check_args("lstrip", &args, 0)?;
            Ok(PyValue::Str(s.trim_start().to_string()))
        }
        "rstrip" => {
            check_args("rstrip", &args, 0)?;
            Ok(PyValue::Str(s.trim_end().to_string()))
        }
        "split" => {
            let sep = args.first().and_then(|v| v.as_str());
            let parts: Vec<PyValue> = if let Some(sep) = sep {
                s.split(sep).map(|p| PyValue::Str(p.to_string())).collect()
            } else {
                s.split_whitespace()
                    .map(|p| PyValue::Str(p.to_string()))
                    .collect()
            };
            Ok(PyValue::List(parts))
        }
        "join" => {
            check_args("join", &args, 1)?;
            let items = match &args[0] {
                PyValue::List(items) => items,
                _ => {
                    return Err(Error::Type {
                        expected: "list".to_string(),
                        got: args[0].type_name().to_string(),
                    });
                }
            };
            let strings: Result<Vec<String>> = items
                .iter()
                .map(|v| match v {
                    PyValue::Str(s) => Ok(s.clone()),
                    _ => Err(Error::Type {
                        expected: "str".to_string(),
                        got: v.type_name().to_string(),
                    }),
                })
                .collect();
            Ok(PyValue::Str(strings?.join(s)))
        }
        "replace" => {
            check_args("replace", &args, 2)?;
            let old = arg_str(&args[0])?;
            let new = arg_str(&args[1])?;
            Ok(PyValue::Str(s.replace(old, new)))
        }
        "startswith" => {
            check_args("startswith", &args, 1)?;
            Ok(PyValue::Bool(s.starts_with(arg_str(&args[0])?)))
        }
        "endswith" => {
            check_args("endswith", &args, 1)?;
            Ok(PyValue::Bool(s.ends_with(arg_str(&args[0])?)))
        }
        "find" => {
            check_args("find", &args, 1)?;
            Ok(PyValue::Int(
                s.find(arg_str(&args[0])?).map(|i| i as i64).unwrap_or(-1),
            ))
        }
        "count" => {
            check_args("count", &args, 1)?;
            Ok(PyValue::Int(s.matches(arg_str(&args[0])?).count() as i64))
        }
        "isdigit" => {
            check_args("isdigit", &args, 0)?;
            Ok(PyValue::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()),
            ))
        }
        "isalpha" => {
            check_args("isalpha", &args, 0)?;
            Ok(PyValue::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_alphabetic()),
            ))
        }
        "isalnum" => {
            check_args("isalnum", &args, 0)?;
            Ok(PyValue::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()),
            ))
        }
        "title" => {
            check_args("title", &args, 0)?;
            let result: String = s
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        Some(c) => {
                            c.to_uppercase().collect::<String>()
                                + chars.as_str().to_lowercase().as_str()
                        }
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            Ok(PyValue::Str(result))
        }
        "capitalize" => {
            check_args("capitalize", &args, 0)?;
            let mut chars = s.chars();
            let result = match chars.next() {
                Some(c) => {
                    c.to_uppercase().collect::<String>() + chars.as_str().to_lowercase().as_str()
                }
                None => String::new(),
            };
            Ok(PyValue::Str(result))
        }
        "format" => str_format(s, args),
        "removeprefix" => {
            check_args("removeprefix", &args, 1)?;
            let prefix = arg_str(&args[0])?;
            Ok(PyValue::Str(
                s.strip_prefix(prefix).unwrap_or(s).to_string(),
            ))
        }
        "removesuffix" => {
            check_args("removesuffix", &args, 1)?;
            let suffix = arg_str(&args[0])?;
            Ok(PyValue::Str(
                s.strip_suffix(suffix).unwrap_or(s).to_string(),
            ))
        }
        "partition" => {
            check_args("partition", &args, 1)?;
            let sep = arg_str(&args[0])?;
            if let Some(pos) = s.find(sep) {
                Ok(PyValue::Tuple(vec![
                    PyValue::Str(s[..pos].to_string()),
                    PyValue::Str(sep.to_string()),
                    PyValue::Str(s[pos + sep.len()..].to_string()),
                ]))
            } else {
                Ok(PyValue::Tuple(vec![
                    PyValue::Str(s.to_string()),
                    PyValue::Str(String::new()),
                    PyValue::Str(String::new()),
                ]))
            }
        }
        "rpartition" => {
            check_args("rpartition", &args, 1)?;
            let sep = arg_str(&args[0])?;
            if let Some(pos) = s.rfind(sep) {
                Ok(PyValue::Tuple(vec![
                    PyValue::Str(s[..pos].to_string()),
                    PyValue::Str(sep.to_string()),
                    PyValue::Str(s[pos + sep.len()..].to_string()),
                ]))
            } else {
                Ok(PyValue::Tuple(vec![
                    PyValue::Str(String::new()),
                    PyValue::Str(String::new()),
                    PyValue::Str(s.to_string()),
                ]))
            }
        }
        "splitlines" => {
            let keepends = args.first().map(|v| v.is_truthy()).unwrap_or(false);
            let mut lines = Vec::new();
            let mut start = 0;
            let bytes = s.as_bytes();
            let len = bytes.len();
            let mut i = 0;
            while i < len {
                if bytes[i] == b'\r' && i + 1 < len && bytes[i + 1] == b'\n' {
                    if keepends {
                        lines.push(PyValue::Str(s[start..i + 2].to_string()));
                    } else {
                        lines.push(PyValue::Str(s[start..i].to_string()));
                    }
                    i += 2;
                    start = i;
                } else if bytes[i] == b'\n' || bytes[i] == b'\r' {
                    if keepends {
                        lines.push(PyValue::Str(s[start..i + 1].to_string()));
                    } else {
                        lines.push(PyValue::Str(s[start..i].to_string()));
                    }
                    i += 1;
                    start = i;
                } else {
                    i += 1;
                }
            }
            if start < len {
                lines.push(PyValue::Str(s[start..].to_string()));
            }
            Ok(PyValue::List(lines))
        }
        "center" => {
            check_args_range("center", &args, 1, 2)?;
            let width = arg_int(&args[0])? as usize;
            let fill = parse_fill_char(&args)?;
            let slen = s.chars().count();
            if slen >= width {
                Ok(PyValue::Str(s.to_string()))
            } else {
                let total_pad = width - slen;
                let left_pad = total_pad / 2;
                let right_pad = total_pad - left_pad;
                let mut result = String::with_capacity(width);
                for _ in 0..left_pad {
                    result.push(fill);
                }
                result.push_str(s);
                for _ in 0..right_pad {
                    result.push(fill);
                }
                Ok(PyValue::Str(result))
            }
        }
        "ljust" => {
            check_args_range("ljust", &args, 1, 2)?;
            let width = arg_int(&args[0])? as usize;
            let fill = parse_fill_char(&args)?;
            let slen = s.chars().count();
            if slen >= width {
                Ok(PyValue::Str(s.to_string()))
            } else {
                let mut result = s.to_string();
                for _ in 0..(width - slen) {
                    result.push(fill);
                }
                Ok(PyValue::Str(result))
            }
        }
        "rjust" => {
            check_args_range("rjust", &args, 1, 2)?;
            let width = arg_int(&args[0])? as usize;
            let fill = parse_fill_char(&args)?;
            let slen = s.chars().count();
            if slen >= width {
                Ok(PyValue::Str(s.to_string()))
            } else {
                let mut result = String::with_capacity(width);
                for _ in 0..(width - slen) {
                    result.push(fill);
                }
                result.push_str(s);
                Ok(PyValue::Str(result))
            }
        }
        "zfill" => {
            check_args("zfill", &args, 1)?;
            let width = arg_int(&args[0])? as usize;
            let slen = s.chars().count();
            if slen >= width {
                Ok(PyValue::Str(s.to_string()))
            } else {
                let (sign, rest) = if s.starts_with('+') || s.starts_with('-') {
                    (&s[..1], &s[1..])
                } else {
                    ("", s)
                };
                let pad = width - slen;
                let mut result = String::with_capacity(width);
                result.push_str(sign);
                for _ in 0..pad {
                    result.push('0');
                }
                result.push_str(rest);
                Ok(PyValue::Str(result))
            }
        }
        "swapcase" => {
            check_args("swapcase", &args, 0)?;
            let result: String = s
                .chars()
                .flat_map(|c| {
                    if c.is_uppercase() {
                        c.to_lowercase().collect::<Vec<_>>()
                    } else if c.is_lowercase() {
                        c.to_uppercase().collect::<Vec<_>>()
                    } else {
                        vec![c]
                    }
                })
                .collect();
            Ok(PyValue::Str(result))
        }
        "casefold" => {
            check_args("casefold", &args, 0)?;
            Ok(PyValue::Str(s.to_lowercase()))
        }
        _ => Err(Error::Unsupported(format!(
            "String method '{}' not implemented",
            method
        ))),
    }
}

/// Call a method on a tuple value (non-mutating).
pub fn call_tuple_method(items: &[PyValue], method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "index" => {
            check_args("index", &args, 1)?;
            for (i, item) in items.iter().enumerate() {
                if item == &args[0] {
                    return Ok(PyValue::Int(i as i64));
                }
            }
            Err(Error::Runtime("value not in tuple".to_string()))
        }
        "count" => {
            check_args("count", &args, 1)?;
            let count = items.iter().filter(|&item| item == &args[0]).count();
            Ok(PyValue::Int(count as i64))
        }
        _ => Err(Error::Unsupported(format!(
            "Tuple method '{}' not implemented",
            method
        ))),
    }
}

/// Call a method on a list value (non-mutating).
pub fn call_list_method(items: &[PyValue], method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "index" => {
            check_args("index", &args, 1)?;
            for (i, item) in items.iter().enumerate() {
                if item == &args[0] {
                    return Ok(PyValue::Int(i as i64));
                }
            }
            Err(Error::Runtime("value not in list".to_string()))
        }
        "count" => {
            check_args("count", &args, 1)?;
            let count = items.iter().filter(|&item| item == &args[0]).count();
            Ok(PyValue::Int(count as i64))
        }
        "copy" => {
            check_args("copy", &args, 0)?;
            Ok(PyValue::List(items.to_vec()))
        }
        _ => Err(Error::Unsupported(format!(
            "List method '{}' not implemented",
            method
        ))),
    }
}

/// Call a method on a dict value (non-mutating).
pub fn call_dict_method(
    pairs: &[(PyValue, PyValue)],
    method: &str,
    args: Vec<PyValue>,
) -> Result<PyValue> {
    match method {
        "get" => {
            check_args_range("get", &args, 1, 2)?;
            let key = &args[0];
            let default = args.get(1).cloned().unwrap_or(PyValue::None);
            Ok(pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(default))
        }
        "keys" => {
            check_args("keys", &args, 0)?;
            Ok(PyValue::List(
                pairs.iter().map(|(k, _)| k.clone()).collect(),
            ))
        }
        "values" => {
            check_args("values", &args, 0)?;
            Ok(PyValue::List(
                pairs.iter().map(|(_, v)| v.clone()).collect(),
            ))
        }
        "items" => {
            check_args("items", &args, 0)?;
            Ok(PyValue::List(
                pairs
                    .iter()
                    .map(|(k, v)| PyValue::Tuple(vec![k.clone(), v.clone()]))
                    .collect(),
            ))
        }
        "copy" => {
            check_args("copy", &args, 0)?;
            Ok(PyValue::Dict(pairs.to_vec()))
        }
        _ => Err(Error::Unsupported(format!(
            "Dict method '{}' not implemented",
            method
        ))),
    }
}

/// Mutating list methods (append, extend, pop, etc.)
#[allow(dead_code)]
pub fn mutate_list(items: &mut Vec<PyValue>, method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "append" => {
            check_args("append", &args, 1)?;
            items.push(args.into_iter().next().unwrap());
            Ok(PyValue::None)
        }
        "extend" => {
            check_args("extend", &args, 1)?;
            match &args[0] {
                PyValue::List(new_items) => {
                    items.extend(new_items.clone());
                }
                _ => {
                    return Err(Error::Type {
                        expected: "list".to_string(),
                        got: args[0].type_name().to_string(),
                    });
                }
            }
            Ok(PyValue::None)
        }
        "pop" => {
            let index = if args.is_empty() {
                None
            } else {
                Some(arg_int(&args[0])?)
            };
            if items.is_empty() {
                return Err(Error::Runtime("pop from empty list".to_string()));
            }
            let idx = match index {
                None => items.len() - 1,
                Some(i) => {
                    let len = items.len() as i64;
                    (if i < 0 { len + i } else { i }) as usize
                }
            };
            if idx >= items.len() {
                return Err(Error::Runtime("pop index out of range".to_string()));
            }
            Ok(items.remove(idx))
        }
        "clear" => {
            check_args("clear", &args, 0)?;
            items.clear();
            Ok(PyValue::None)
        }
        "insert" => {
            check_args("insert", &args, 2)?;
            let index = arg_int(&args[0])?;
            let len = items.len() as i64;
            let idx = if index < 0 {
                (len + index).max(0) as usize
            } else {
                (index as usize).min(items.len())
            };
            items.insert(idx, args[1].clone());
            Ok(PyValue::None)
        }
        "remove" => {
            check_args("remove", &args, 1)?;
            let pos = items.iter().position(|x| x == &args[0]);
            match pos {
                Some(idx) => {
                    items.remove(idx);
                    Ok(PyValue::None)
                }
                None => Err(Error::Runtime("value not in list".to_string())),
            }
        }
        "reverse" => {
            check_args("reverse", &args, 0)?;
            items.reverse();
            Ok(PyValue::None)
        }
        "sort" => {
            check_args("sort", &args, 0)?;
            items.sort_by(|a, b| match (a, b) {
                (PyValue::Int(x), PyValue::Int(y)) => x.cmp(y),
                (PyValue::Float(x), PyValue::Float(y)) => {
                    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                }
                (PyValue::Str(x), PyValue::Str(y)) => x.cmp(y),
                _ => std::cmp::Ordering::Equal,
            });
            Ok(PyValue::None)
        }
        _ => Err(Error::Unsupported(format!(
            "List method '{}' not implemented",
            method
        ))),
    }
}

/// Call a method on a set value (non-mutating).
pub fn call_set_method(items: &[PyValue], method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "copy" => {
            check_args("copy", &args, 0)?;
            Ok(PyValue::Set(items.to_vec()))
        }
        "union" => {
            check_args("union", &args, 1)?;
            let other = to_set_items(&args[0])?;
            let idx = SetIndex::new(items);
            let mut result = items.to_vec();
            for v in other {
                if !idx.contains(&v) {
                    result.push(v);
                }
            }
            Ok(PyValue::Set(result))
        }
        "intersection" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "intersection() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            let idx = SetIndex::new(&other);
            let result: Vec<PyValue> = items.iter().filter(|v| idx.contains(v)).cloned().collect();
            Ok(PyValue::Set(result))
        }
        "difference" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "difference() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            let idx = SetIndex::new(&other);
            let result: Vec<PyValue> = items.iter().filter(|v| !idx.contains(v)).cloned().collect();
            Ok(PyValue::Set(result))
        }
        "symmetric_difference" => {
            check_args("symmetric_difference", &args, 1)?;
            let other = to_set_items(&args[0])?;
            let idx_other = SetIndex::new(&other);
            let idx_self = SetIndex::new(items);
            let mut result: Vec<PyValue> = items
                .iter()
                .filter(|v| !idx_other.contains(v))
                .cloned()
                .collect();
            for v in &other {
                if !idx_self.contains(v) {
                    result.push(v.clone());
                }
            }
            Ok(PyValue::Set(result))
        }
        "issubset" => {
            check_args("issubset", &args, 1)?;
            let other = to_set_items(&args[0])?;
            let idx = SetIndex::new(&other);
            Ok(PyValue::Bool(items.iter().all(|v| idx.contains(v))))
        }
        "issuperset" => {
            check_args("issuperset", &args, 1)?;
            let other = to_set_items(&args[0])?;
            let idx = SetIndex::new(items);
            Ok(PyValue::Bool(other.iter().all(|v| idx.contains(v))))
        }
        "isdisjoint" => {
            check_args("isdisjoint", &args, 1)?;
            let other = to_set_items(&args[0])?;
            let idx = SetIndex::new(&other);
            Ok(PyValue::Bool(!items.iter().any(|v| idx.contains(v))))
        }
        _ => Err(Error::Unsupported(format!(
            "Set method '{}' not implemented",
            method
        ))),
    }
}

/// Extract the optional fill character from args[1], defaulting to space.
///
/// Used by `center`, `ljust`, and `rjust`. Expects args to already be range-checked.
fn parse_fill_char(args: &[PyValue]) -> Result<char> {
    if args.len() > 1 {
        let f = arg_str(&args[1])?;
        if f.chars().count() != 1 {
            return Err(Error::Runtime(
                "TypeError: The fill character must be exactly one character long".to_string(),
            ));
        }
        Ok(f.chars().next().unwrap())
    } else {
        Ok(' ')
    }
}

/// Implement `str.format(*args)` — basic positional and indexed substitution.
fn str_format(s: &str, args: Vec<PyValue>) -> Result<PyValue> {
    let mut result = String::new();
    let mut auto_idx = 0usize;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '{' {
            if i + 1 < len && chars[i + 1] == '{' {
                // Escaped brace: {{ → {
                result.push('{');
                i += 2;
                continue;
            }
            // Find closing }
            let start = i + 1;
            let mut end = start;
            while end < len && chars[end] != '}' {
                end += 1;
            }
            if end >= len {
                return Err(Error::Runtime(
                    "ValueError: Single '{' encountered in format string".to_string(),
                ));
            }
            let field: String = chars[start..end].iter().collect();
            let val = if field.is_empty() {
                // Auto-indexed: {}
                let idx = auto_idx;
                auto_idx += 1;
                args.get(idx).ok_or_else(|| {
                    Error::Runtime("IndexError: Replacement index out of range".to_string())
                })?
            } else if let Ok(idx) = field.parse::<usize>() {
                // Explicit index: {0}, {1}, ...
                args.get(idx).ok_or_else(|| {
                    Error::Runtime("IndexError: Replacement index out of range".to_string())
                })?
            } else {
                // Named field — not supported without kwargs
                return Err(Error::Runtime(format!("KeyError: '{}'", field)));
            };
            result.push_str(&val.to_print_string());
            i = end + 1;
        } else if chars[i] == '}' {
            if i + 1 < len && chars[i + 1] == '}' {
                // Escaped brace: }} → }
                result.push('}');
                i += 2;
            } else {
                return Err(Error::Runtime(
                    "ValueError: Single '}' encountered in format string".to_string(),
                ));
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    Ok(PyValue::Str(result))
}

/// Convert a PyValue to a Vec for set operations (accepts set, list, tuple).
fn to_set_items(value: &PyValue) -> Result<Vec<PyValue>> {
    match value {
        PyValue::Set(items) | PyValue::List(items) | PyValue::Tuple(items) => Ok(items.clone()),
        _ => Err(Error::Type {
            expected: "iterable".to_string(),
            got: value.type_name().to_string(),
        }),
    }
}

/// Mutating set methods (add, discard, remove, clear, update, pop)
#[allow(dead_code)]
pub fn mutate_set(items: &mut Vec<PyValue>, method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "add" => {
            check_args("add", &args, 1)?;
            let elem = &args[0];
            if !elem.is_hashable() {
                return Err(Error::Runtime(format!(
                    "TypeError: unhashable type: '{}'",
                    elem.type_name()
                )));
            }
            let idx = SetIndex::new(items);
            if !idx.contains(elem) {
                items.push(elem.clone());
            }
            Ok(PyValue::None)
        }
        "discard" => {
            check_args("discard", &args, 1)?;
            if let Some(pos) = items.iter().position(|v| v == &args[0]) {
                items.remove(pos);
            }
            Ok(PyValue::None)
        }
        "remove" => {
            check_args("remove", &args, 1)?;
            if let Some(pos) = items.iter().position(|v| v == &args[0]) {
                items.remove(pos);
                Ok(PyValue::None)
            } else {
                Err(Error::Runtime(format!("KeyError: {}", args[0])))
            }
        }
        "clear" => {
            check_args("clear", &args, 0)?;
            items.clear();
            Ok(PyValue::None)
        }
        "update" => {
            check_args("update", &args, 1)?;
            let other = to_set_items(&args[0])?;
            // Validate hashability first
            for v in &other {
                if !v.is_hashable() {
                    return Err(Error::Runtime(format!(
                        "TypeError: unhashable type: '{}'",
                        v.type_name()
                    )));
                }
            }
            // Collect new items, then extend
            let idx = SetIndex::new(items);
            let new_items: Vec<PyValue> = other.into_iter().filter(|v| !idx.contains(v)).collect();
            items.extend(new_items);
            Ok(PyValue::None)
        }
        "pop" => {
            check_args("pop", &args, 0)?;
            if items.is_empty() {
                Err(Error::Runtime("pop from an empty set".to_string()))
            } else {
                Ok(items.remove(0))
            }
        }
        _ => Err(Error::Unsupported(format!(
            "Set method '{}' not implemented",
            method
        ))),
    }
}

/// Mutating dict methods (update, setdefault, pop, clear)
#[allow(dead_code)]
pub fn mutate_dict(
    pairs: &mut Vec<(PyValue, PyValue)>,
    method: &str,
    args: Vec<PyValue>,
) -> Result<PyValue> {
    match method {
        "update" => {
            check_args("update", &args, 1)?;
            match &args[0] {
                PyValue::Dict(new_pairs) => {
                    for (k, v) in new_pairs {
                        if let Some(existing) = pairs.iter_mut().find(|(ek, _)| ek == k) {
                            existing.1 = v.clone();
                        } else {
                            pairs.push((k.clone(), v.clone()));
                        }
                    }
                }
                _ => {
                    return Err(Error::Type {
                        expected: "dict".to_string(),
                        got: args[0].type_name().to_string(),
                    });
                }
            }
            Ok(PyValue::None)
        }
        "setdefault" => {
            check_args_range("setdefault", &args, 1, 2)?;
            let key = &args[0];
            let default = args.get(1).cloned().unwrap_or(PyValue::None);

            if let Some((_, v)) = pairs.iter().find(|(k, _)| k == key) {
                return Ok(v.clone());
            }
            pairs.push((key.clone(), default.clone()));
            Ok(default)
        }
        "pop" => {
            check_args_range("pop", &args, 1, 2)?;
            let key = &args[0];
            let default = args.get(1).cloned();

            if let Some(pos) = pairs.iter().position(|(k, _)| k == key) {
                let (_, v) = pairs.remove(pos);
                return Ok(v);
            }
            match default {
                Some(d) => Ok(d),
                None => Err(Error::Runtime(format!("KeyError: {}", key))),
            }
        }
        "clear" => {
            check_args("clear", &args, 0)?;
            pairs.clear();
            Ok(PyValue::None)
        }
        _ => Err(Error::Unsupported(format!(
            "Dict method '{}' not implemented",
            method
        ))),
    }
}
