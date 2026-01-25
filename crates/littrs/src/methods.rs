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
                return Err(Error::Runtime("join() takes exactly 1 argument".to_string()));
            }
            let items = match &args[0] {
                PyValue::List(items) => items,
                _ => {
                    return Err(Error::Type {
                        expected: "list".to_string(),
                        got: args[0].type_name().to_string(),
                    })
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
                return Err(Error::Runtime("replace() takes exactly 2 arguments".to_string()));
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
                return Err(Error::Runtime("startswith() takes exactly 1 argument".to_string()));
            }
            let prefix = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Bool(s.starts_with(prefix)))
        }
        "endswith" => {
            if args.len() != 1 {
                return Err(Error::Runtime("endswith() takes exactly 1 argument".to_string()));
            }
            let suffix = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Bool(s.ends_with(suffix)))
        }
        "find" => {
            if args.len() != 1 {
                return Err(Error::Runtime("find() takes exactly 1 argument".to_string()));
            }
            let needle = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            Ok(PyValue::Int(s.find(needle).map(|i| i as i64).unwrap_or(-1)))
        }
        "count" => {
            if args.len() != 1 {
                return Err(Error::Runtime("count() takes exactly 1 argument".to_string()));
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
            Ok(PyValue::Bool(!s.is_empty() && s.chars().all(|c| c.is_ascii_digit())))
        }
        "isalpha" => {
            if !args.is_empty() {
                return Err(Error::Runtime("isalpha() takes no arguments".to_string()));
            }
            Ok(PyValue::Bool(!s.is_empty() && s.chars().all(|c| c.is_alphabetic())))
        }
        "isalnum" => {
            if !args.is_empty() {
                return Err(Error::Runtime("isalnum() takes no arguments".to_string()));
            }
            Ok(PyValue::Bool(!s.is_empty() && s.chars().all(|c| c.is_alphanumeric())))
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
                        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str().to_lowercase().as_str(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            Ok(PyValue::Str(result))
        }
        "capitalize" => {
            if !args.is_empty() {
                return Err(Error::Runtime("capitalize() takes no arguments".to_string()));
            }
            let mut chars = s.chars();
            let result = match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str().to_lowercase().as_str(),
                None => String::new(),
            };
            Ok(PyValue::Str(result))
        }
        _ => Err(Error::Unsupported(format!(
            "String method '{}' not implemented",
            method
        ))),
    }
}

/// Call a method on a list value (non-mutating).
pub fn call_list_method(items: &[PyValue], method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "index" => {
            if args.len() != 1 {
                return Err(Error::Runtime("index() takes exactly 1 argument".to_string()));
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
                return Err(Error::Runtime("count() takes exactly 1 argument".to_string()));
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
pub fn call_dict_method(pairs: &[(String, PyValue)], method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "get" => {
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime("get() takes 1 or 2 arguments".to_string()));
            }
            let key = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
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
                pairs.iter().map(|(k, _)| PyValue::Str(k.clone())).collect(),
            ))
        }
        "values" => {
            if !args.is_empty() {
                return Err(Error::Runtime("values() takes no arguments".to_string()));
            }
            Ok(PyValue::List(pairs.iter().map(|(_, v)| v.clone()).collect()))
        }
        "items" => {
            if !args.is_empty() {
                return Err(Error::Runtime("items() takes no arguments".to_string()));
            }
            Ok(PyValue::List(
                pairs
                    .iter()
                    .map(|(k, v)| PyValue::List(vec![PyValue::Str(k.clone()), v.clone()]))
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
                return Err(Error::Runtime("append() takes exactly 1 argument".to_string()));
            }
            items.push(args.into_iter().next().unwrap());
            Ok(PyValue::None)
        }
        "extend" => {
            if args.len() != 1 {
                return Err(Error::Runtime("extend() takes exactly 1 argument".to_string()));
            }
            match &args[0] {
                PyValue::List(new_items) => {
                    items.extend(new_items.clone());
                }
                _ => {
                    return Err(Error::Type {
                        expected: "list".to_string(),
                        got: args[0].type_name().to_string(),
                    })
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
                return Err(Error::Runtime("insert() takes exactly 2 arguments".to_string()));
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
                return Err(Error::Runtime("remove() takes exactly 1 argument".to_string()));
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
                return Err(Error::Runtime("sort() takes no arguments (reverse/key not yet supported)".to_string()));
            }
            items.sort_by(|a, b| {
                match (a, b) {
                    (PyValue::Int(x), PyValue::Int(y)) => x.cmp(y),
                    (PyValue::Float(x), PyValue::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
                    (PyValue::Str(x), PyValue::Str(y)) => x.cmp(y),
                    _ => std::cmp::Ordering::Equal,
                }
            });
            Ok(PyValue::None)
        }
        _ => Err(Error::Unsupported(format!(
            "List method '{}' not implemented",
            method
        ))),
    }
}

/// Mutating dict methods (update, setdefault, pop, clear)
#[allow(dead_code)]
pub fn mutate_dict(pairs: &mut Vec<(String, PyValue)>, method: &str, args: Vec<PyValue>) -> Result<PyValue> {
    match method {
        "update" => {
            if args.len() != 1 {
                return Err(Error::Runtime("update() takes exactly 1 argument".to_string()));
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
                    })
                }
            }
            Ok(PyValue::None)
        }
        "setdefault" => {
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime("setdefault() takes 1 or 2 arguments".to_string()));
            }
            let key = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            let default = args.get(1).cloned().unwrap_or(PyValue::None);

            if let Some((_, v)) = pairs.iter().find(|(k, _)| k == key) {
                return Ok(v.clone());
            }
            pairs.push((key.to_string(), default.clone()));
            Ok(default)
        }
        "pop" => {
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Runtime("pop() takes 1 or 2 arguments".to_string()));
            }
            let key = args[0].as_str().ok_or_else(|| Error::Type {
                expected: "str".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            let default = args.get(1).cloned();

            if let Some(pos) = pairs.iter().position(|(k, _)| k == key) {
                let (_, v) = pairs.remove(pos);
                return Ok(v);
            }
            match default {
                Some(d) => Ok(d),
                None => Err(Error::Runtime(format!("KeyError: '{}'", key))),
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
