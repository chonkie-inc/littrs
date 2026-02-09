//! Method implementations for Python types.
//!
//! This module contains the implementations of methods for str, list, and dict types.

use crate::error::{Error, Result};
use crate::value::PyValue;

/// Call a method on a string value.
pub fn call_str_method(s: &str, method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "lower" => {
            if !args.is_empty() {
                return Err(Error::Runtime("lower() takes no arguments".to_string()));
            }
            Ok(PyValue::Str(s.to_lowercase()))
        }
        "upper" => {
            if !args.is_empty() {
                return Err(Error::Runtime("upper() takes no arguments".to_string()));
            }
            Ok(PyValue::Str(s.to_uppercase()))
        }
        "strip" => {
            if !args.is_empty() {
                return Err(Error::Runtime("strip() takes no arguments".to_string()));
            }
            Ok(PyValue::Str(s.trim().to_string()))
        }
        "lstrip" => {
            if !args.is_empty() {
                return Err(Error::Runtime("lstrip() takes no arguments".to_string()));
            }
            Ok(PyValue::Str(s.trim_start().to_string()))
        }
        "rstrip" => {
            if !args.is_empty() {
                return Err(Error::Runtime("rstrip() takes no arguments".to_string()));
            }
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "join() takes exactly 1 argument".to_string(),
                ));
            }
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
            if args.len() != 2 {
                return Err(Error::Runtime(
                    "replace() takes exactly 2 arguments".to_string(),
                ));
            }
            let old = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            let new = args[1].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[1].type_name().to_string(),
            })?;
            Ok(PyValue::Str(s.replace(old, new)))
        }
        "startswith" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "startswith() takes exactly 1 argument".to_string(),
                ));
            }
            let prefix = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Bool(s.starts_with(prefix)))
        }
        "endswith" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "endswith() takes exactly 1 argument".to_string(),
                ));
            }
            let suffix = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Bool(s.ends_with(suffix)))
        }
        "find" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "find() takes exactly 1 argument".to_string(),
                ));
            }
            let needle = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Int(s.find(needle).map(|i| i as i64).unwrap_or(-1)))
        }
        "count" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "count() takes exactly 1 argument".to_string(),
                ));
            }
            let needle = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Int(s.matches(needle).count() as i64))
        }
        "isdigit" => {
            if !args.is_empty() {
                return Err(Error::Runtime("isdigit() takes no arguments".to_string()));
            }
            Ok(PyValue::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()),
            ))
        }
        "isalpha" => {
            if !args.is_empty() {
                return Err(Error::Runtime("isalpha() takes no arguments".to_string()));
            }
            Ok(PyValue::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_alphabetic()),
            ))
        }
        "isalnum" => {
            if !args.is_empty() {
                return Err(Error::Runtime("isalnum() takes no arguments".to_string()));
            }
            Ok(PyValue::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()),
            ))
        }
        "title" => {
            if !args.is_empty() {
                return Err(Error::Runtime("title() takes no arguments".to_string()));
            }
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
            if !args.is_empty() {
                return Err(Error::Runtime(
                    "capitalize() takes no arguments".to_string(),
                ));
            }
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "removeprefix() takes exactly 1 argument".to_string(),
                ));
            }
            let prefix = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Str(
                s.strip_prefix(prefix).unwrap_or(s).to_string(),
            ))
        }
        "removesuffix" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "removesuffix() takes exactly 1 argument".to_string(),
                ));
            }
            let suffix = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Str(
                s.strip_suffix(suffix).unwrap_or(s).to_string(),
            ))
        }
        "partition" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "partition() takes exactly 1 argument".to_string(),
                ));
            }
            let sep = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "rpartition() takes exactly 1 argument".to_string(),
                ));
            }
            let sep = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
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
            let keepends = args
                .first()
                .map(|v| v.is_truthy())
                .unwrap_or(false);
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
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime(
                    "center() takes 1 or 2 arguments".to_string(),
                ));
            }
            let width = args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })? as usize;
            let fill = if args.len() > 1 {
                let f = args[1].as_str().ok_or_else(|| Error::Type {
                    expected: "str".to_string(),
                    got: args[1].type_name().to_string(),
                })?;
                if f.chars().count() != 1 {
                    return Err(Error::Runtime(
                        "TypeError: The fill character must be exactly one character long"
                            .to_string(),
                    ));
                }
                f.chars().next().unwrap()
            } else {
                ' '
            };
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
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime(
                    "ljust() takes 1 or 2 arguments".to_string(),
                ));
            }
            let width = args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })? as usize;
            let fill = if args.len() > 1 {
                let f = args[1].as_str().ok_or_else(|| Error::Type {
                    expected: "str".to_string(),
                    got: args[1].type_name().to_string(),
                })?;
                if f.chars().count() != 1 {
                    return Err(Error::Runtime(
                        "TypeError: The fill character must be exactly one character long"
                            .to_string(),
                    ));
                }
                f.chars().next().unwrap()
            } else {
                ' '
            };
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
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime(
                    "rjust() takes 1 or 2 arguments".to_string(),
                ));
            }
            let width = args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })? as usize;
            let fill = if args.len() > 1 {
                let f = args[1].as_str().ok_or_else(|| Error::Type {
                    expected: "str".to_string(),
                    got: args[1].type_name().to_string(),
                })?;
                if f.chars().count() != 1 {
                    return Err(Error::Runtime(
                        "TypeError: The fill character must be exactly one character long"
                            .to_string(),
                    ));
                }
                f.chars().next().unwrap()
            } else {
                ' '
            };
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "zfill() takes exactly 1 argument".to_string(),
                ));
            }
            let width = args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })? as usize;
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
            if !args.is_empty() {
                return Err(Error::Runtime(
                    "swapcase() takes no arguments".to_string(),
                ));
            }
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
            if !args.is_empty() {
                return Err(Error::Runtime(
                    "casefold() takes no arguments".to_string(),
                ));
            }
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "index() takes exactly 1 argument".to_string(),
                ));
            }
            for (i, item) in items.iter().enumerate() {
                if item == &args[0] {
                    return Ok(PyValue::Int(i as i64));
                }
            }
            Err(Error::Runtime("value not in tuple".to_string()))
        }
        "count" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "count() takes exactly 1 argument".to_string(),
                ));
            }
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "index() takes exactly 1 argument".to_string(),
                ));
            }
            for (i, item) in items.iter().enumerate() {
                if item == &args[0] {
                    return Ok(PyValue::Int(i as i64));
                }
            }
            Err(Error::Runtime("value not in list".to_string()))
        }
        "count" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "count() takes exactly 1 argument".to_string(),
                ));
            }
            let count = items.iter().filter(|&item| item == &args[0]).count();
            Ok(PyValue::Int(count as i64))
        }
        "copy" => {
            if !args.is_empty() {
                return Err(Error::Runtime("copy() takes no arguments".to_string()));
            }
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
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime("get() takes 1 or 2 arguments".to_string()));
            }
            let key = &args[0];
            let default = args.get(1).cloned().unwrap_or(PyValue::None);
            Ok(pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(default))
        }
        "keys" => {
            if !args.is_empty() {
                return Err(Error::Runtime("keys() takes no arguments".to_string()));
            }
            Ok(PyValue::List(
                pairs.iter().map(|(k, _)| k.clone()).collect(),
            ))
        }
        "values" => {
            if !args.is_empty() {
                return Err(Error::Runtime("values() takes no arguments".to_string()));
            }
            Ok(PyValue::List(
                pairs.iter().map(|(_, v)| v.clone()).collect(),
            ))
        }
        "items" => {
            if !args.is_empty() {
                return Err(Error::Runtime("items() takes no arguments".to_string()));
            }
            Ok(PyValue::List(
                pairs
                    .iter()
                    .map(|(k, v)| PyValue::Tuple(vec![k.clone(), v.clone()]))
                    .collect(),
            ))
        }
        "copy" => {
            if !args.is_empty() {
                return Err(Error::Runtime("copy() takes no arguments".to_string()));
            }
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "append() takes exactly 1 argument".to_string(),
                ));
            }
            items.push(args.into_iter().next().unwrap());
            Ok(PyValue::None)
        }
        "extend" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "extend() takes exactly 1 argument".to_string(),
                ));
            }
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
                Some(args[0].as_int().ok_or_else(|| Error::Type {
                    expected: "int".to_string(),
                    got: args[0].type_name().to_string(),
                })?)
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
            if !args.is_empty() {
                return Err(Error::Runtime("clear() takes no arguments".to_string()));
            }
            items.clear();
            Ok(PyValue::None)
        }
        "insert" => {
            if args.len() != 2 {
                return Err(Error::Runtime(
                    "insert() takes exactly 2 arguments".to_string(),
                ));
            }
            let index = args[0].as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: args[0].type_name().to_string(),
            })?;
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "remove() takes exactly 1 argument".to_string(),
                ));
            }
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
            if !args.is_empty() {
                return Err(Error::Runtime("reverse() takes no arguments".to_string()));
            }
            items.reverse();
            Ok(PyValue::None)
        }
        "sort" => {
            if !args.is_empty() {
                return Err(Error::Runtime(
                    "sort() takes no arguments (reverse/key not yet supported)".to_string(),
                ));
            }
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
            if !args.is_empty() {
                return Err(Error::Runtime("copy() takes no arguments".to_string()));
            }
            Ok(PyValue::Set(items.to_vec()))
        }
        "union" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "union() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            let mut result = items.to_vec();
            for v in other {
                if !result.contains(&v) {
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
            let result: Vec<PyValue> = items
                .iter()
                .filter(|v| other.contains(v))
                .cloned()
                .collect();
            Ok(PyValue::Set(result))
        }
        "difference" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "difference() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            let result: Vec<PyValue> = items
                .iter()
                .filter(|v| !other.contains(v))
                .cloned()
                .collect();
            Ok(PyValue::Set(result))
        }
        "symmetric_difference" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "symmetric_difference() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            let mut result: Vec<PyValue> = items
                .iter()
                .filter(|v| !other.contains(v))
                .cloned()
                .collect();
            for v in &other {
                if !items.contains(v) {
                    result.push(v.clone());
                }
            }
            Ok(PyValue::Set(result))
        }
        "issubset" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "issubset() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            Ok(PyValue::Bool(items.iter().all(|v| other.contains(v))))
        }
        "issuperset" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "issuperset() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            Ok(PyValue::Bool(other.iter().all(|v| items.contains(v))))
        }
        "isdisjoint" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "isdisjoint() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            Ok(PyValue::Bool(!items.iter().any(|v| other.contains(v))))
        }
        _ => Err(Error::Unsupported(format!(
            "Set method '{}' not implemented",
            method
        ))),
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
                return Err(Error::Runtime(format!(
                    "KeyError: '{}'",
                    field
                )));
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
            if args.len() != 1 {
                return Err(Error::Runtime("add() takes exactly 1 argument".to_string()));
            }
            let elem = &args[0];
            if !elem.is_hashable() {
                return Err(Error::Runtime(format!(
                    "TypeError: unhashable type: '{}'",
                    elem.type_name()
                )));
            }
            if !items.contains(elem) {
                items.push(elem.clone());
            }
            Ok(PyValue::None)
        }
        "discard" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "discard() takes exactly 1 argument".to_string(),
                ));
            }
            if let Some(pos) = items.iter().position(|v| v == &args[0]) {
                items.remove(pos);
            }
            Ok(PyValue::None)
        }
        "remove" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "remove() takes exactly 1 argument".to_string(),
                ));
            }
            if let Some(pos) = items.iter().position(|v| v == &args[0]) {
                items.remove(pos);
                Ok(PyValue::None)
            } else {
                Err(Error::Runtime(format!("KeyError: {}", args[0])))
            }
        }
        "clear" => {
            if !args.is_empty() {
                return Err(Error::Runtime("clear() takes no arguments".to_string()));
            }
            items.clear();
            Ok(PyValue::None)
        }
        "update" => {
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "update() takes exactly 1 argument".to_string(),
                ));
            }
            let other = to_set_items(&args[0])?;
            for v in other {
                if !v.is_hashable() {
                    return Err(Error::Runtime(format!(
                        "TypeError: unhashable type: '{}'",
                        v.type_name()
                    )));
                }
                if !items.contains(&v) {
                    items.push(v);
                }
            }
            Ok(PyValue::None)
        }
        "pop" => {
            if !args.is_empty() {
                return Err(Error::Runtime("pop() takes no arguments".to_string()));
            }
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
            if args.len() != 1 {
                return Err(Error::Runtime(
                    "update() takes exactly 1 argument".to_string(),
                ));
            }
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
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime(
                    "setdefault() takes 1 or 2 arguments".to_string(),
                ));
            }
            let key = &args[0];
            let default = args.get(1).cloned().unwrap_or(PyValue::None);

            if let Some((_, v)) = pairs.iter().find(|(k, _)| k == key) {
                return Ok(v.clone());
            }
            pairs.push((key.clone(), default.clone()));
            Ok(default)
        }
        "pop" => {
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime("pop() takes 1 or 2 arguments".to_string()));
            }
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
            if !args.is_empty() {
                return Err(Error::Runtime("clear() takes no arguments".to_string()));
            }
            pairs.clear();
            Ok(PyValue::None)
        }
        _ => Err(Error::Unsupported(format!(
            "Dict method '{}' not implemented",
            method
        ))),
    }
}
