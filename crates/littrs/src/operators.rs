//! Binary and comparison operators for the sandbox.
//!
//! This module implements Python operators:
//! - Arithmetic: +, -, *, /, //, %, **
//! - Bitwise: |, ^, &, <<, >>
//! - Comparison: ==, !=, <, <=, >, >=, in, not in, is, is not

use crate::bytecode::{BinOp, CmpOp};
use crate::error::{Error, Result};
use crate::value::PyValue;

/// Apply a binary operator to two values.
///
/// Handles arithmetic (+, -, *, /, //, %, **), bitwise (|, ^, &, <<, >>),
/// and special cases like string concatenation, string/list repetition,
/// and list concatenation.
pub fn apply_binop(op: &BinOp, left: &PyValue, right: &PyValue) -> Result<PyValue> {
    match op {
        BinOp::Add => match (left, right) {
            (PyValue::Int(a), PyValue::Int(b)) => Ok(PyValue::Int(a + b)),
            (PyValue::Float(a), PyValue::Float(b)) => Ok(PyValue::Float(a + b)),
            (PyValue::Int(a), PyValue::Float(b)) => Ok(PyValue::Float(*a as f64 + b)),
            (PyValue::Float(a), PyValue::Int(b)) => Ok(PyValue::Float(a + *b as f64)),
            (PyValue::Str(a), PyValue::Str(b)) => Ok(PyValue::Str(format!("{}{}", a, b))),
            (PyValue::List(a), PyValue::List(b)) => {
                let mut result = a.clone();
                result.extend(b.clone());
                Ok(PyValue::List(result))
            }
            _ => Err(Error::Type {
                expected: "compatible types for +".to_string(),
                got: format!("{} and {}", left.type_name(), right.type_name()),
            }),
        },
        BinOp::Sub => numeric_binop(left, right, |a, b| a - b, |a, b| a - b),
        BinOp::Mult => match (left, right) {
            (PyValue::Int(a), PyValue::Int(b)) => Ok(PyValue::Int(a * b)),
            (PyValue::Float(a), PyValue::Float(b)) => Ok(PyValue::Float(a * b)),
            (PyValue::Int(a), PyValue::Float(b)) => Ok(PyValue::Float(*a as f64 * b)),
            (PyValue::Float(a), PyValue::Int(b)) => Ok(PyValue::Float(a * *b as f64)),
            (PyValue::Str(s), PyValue::Int(n)) | (PyValue::Int(n), PyValue::Str(s)) => {
                if *n <= 0 {
                    Ok(PyValue::Str(String::new()))
                } else {
                    Ok(PyValue::Str(s.repeat(*n as usize)))
                }
            }
            (PyValue::List(l), PyValue::Int(n)) | (PyValue::Int(n), PyValue::List(l)) => {
                if *n <= 0 {
                    Ok(PyValue::List(vec![]))
                } else {
                    let mut result = Vec::new();
                    for _ in 0..*n {
                        result.extend(l.clone());
                    }
                    Ok(PyValue::List(result))
                }
            }
            _ => Err(Error::Type {
                expected: "compatible types for *".to_string(),
                got: format!("{} and {}", left.type_name(), right.type_name()),
            }),
        },
        BinOp::Div => {
            let a = left.as_float().ok_or_else(|| Error::Type {
                expected: "number".to_string(),
                got: left.type_name().to_string(),
            })?;
            let b = right.as_float().ok_or_else(|| Error::Type {
                expected: "number".to_string(),
                got: right.type_name().to_string(),
            })?;
            if b == 0.0 {
                Err(Error::DivisionByZero)
            } else {
                Ok(PyValue::Float(a / b))
            }
        }
        BinOp::FloorDiv => {
            let a = left.as_float().ok_or_else(|| Error::Type {
                expected: "number".to_string(),
                got: left.type_name().to_string(),
            })?;
            let b = right.as_float().ok_or_else(|| Error::Type {
                expected: "number".to_string(),
                got: right.type_name().to_string(),
            })?;
            if b == 0.0 {
                Err(Error::DivisionByZero)
            } else {
                let result = (a / b).floor();
                if matches!(left, PyValue::Int(_)) && matches!(right, PyValue::Int(_)) {
                    Ok(PyValue::Int(result as i64))
                } else {
                    Ok(PyValue::Float(result))
                }
            }
        }
        BinOp::Mod => match (left, right) {
            (PyValue::Int(a), PyValue::Int(b)) => {
                if *b == 0 {
                    Err(Error::DivisionByZero)
                } else {
                    Ok(PyValue::Int(a % b))
                }
            }
            _ => {
                let a = left.as_float().ok_or_else(|| Error::Type {
                    expected: "number".to_string(),
                    got: left.type_name().to_string(),
                })?;
                let b = right.as_float().ok_or_else(|| Error::Type {
                    expected: "number".to_string(),
                    got: right.type_name().to_string(),
                })?;
                if b == 0.0 {
                    Err(Error::DivisionByZero)
                } else {
                    Ok(PyValue::Float(a % b))
                }
            }
        },
        BinOp::Pow => {
            let a = left.as_float().ok_or_else(|| Error::Type {
                expected: "number".to_string(),
                got: left.type_name().to_string(),
            })?;
            let b = right.as_float().ok_or_else(|| Error::Type {
                expected: "number".to_string(),
                got: right.type_name().to_string(),
            })?;
            let result = a.powf(b);
            if matches!(left, PyValue::Int(_))
                && matches!(right, PyValue::Int(_))
                && result.fract() == 0.0
                && result >= i64::MIN as f64
                && result <= i64::MAX as f64
            {
                Ok(PyValue::Int(result as i64))
            } else {
                Ok(PyValue::Float(result))
            }
        }
        BinOp::BitOr => int_binop(left, right, |a, b| a | b),
        BinOp::BitXor => int_binop(left, right, |a, b| a ^ b),
        BinOp::BitAnd => int_binop(left, right, |a, b| a & b),
        BinOp::LShift => int_binop(left, right, |a, b| a << b),
        BinOp::RShift => int_binop(left, right, |a, b| a >> b),
    }
}

/// Apply a comparison operator to two values.
///
/// Returns a boolean result. For `In`/`NotIn`, checks membership in lists,
/// strings, and dicts. For `Is`/`IsNot`, only `None is None` is true.
pub fn apply_cmpop(op: &CmpOp, left: &PyValue, right: &PyValue) -> Result<bool> {
    match op {
        CmpOp::Eq => Ok(left == right),
        CmpOp::NotEq => Ok(left != right),
        CmpOp::Lt => compare_values(left, right, |a, b| a < b, |a, b| a < b),
        CmpOp::LtE => compare_values(left, right, |a, b| a <= b, |a, b| a <= b),
        CmpOp::Gt => compare_values(left, right, |a, b| a > b, |a, b| a > b),
        CmpOp::GtE => compare_values(left, right, |a, b| a >= b, |a, b| a >= b),
        CmpOp::In => match right {
            PyValue::List(items) => Ok(items.contains(left)),
            PyValue::Str(s) => {
                if let PyValue::Str(needle) = left {
                    Ok(s.contains(needle.as_str()))
                } else {
                    Err(Error::Type {
                        expected: "str".to_string(),
                        got: left.type_name().to_string(),
                    })
                }
            }
            PyValue::Dict(pairs) => {
                if let PyValue::Str(key) = left {
                    Ok(pairs.iter().any(|(k, _)| k == key))
                } else {
                    Err(Error::Type {
                        expected: "str".to_string(),
                        got: left.type_name().to_string(),
                    })
                }
            }
            _ => Err(Error::Type {
                expected: "container".to_string(),
                got: right.type_name().to_string(),
            }),
        },
        CmpOp::NotIn => {
            let in_result = apply_cmpop(&CmpOp::In, left, right)?;
            Ok(!in_result)
        }
        CmpOp::Is => match (left, right) {
            (PyValue::None, PyValue::None) => Ok(true),
            _ => Ok(false),
        },
        CmpOp::IsNot => {
            let is_result = apply_cmpop(&CmpOp::Is, left, right)?;
            Ok(!is_result)
        }
    }
}

/// Apply a numeric binary operation.
fn numeric_binop<F, G>(left: &PyValue, right: &PyValue, int_op: F, float_op: G) -> Result<PyValue>
where
    F: Fn(i64, i64) -> i64,
    G: Fn(f64, f64) -> f64,
{
    match (left, right) {
        (PyValue::Int(a), PyValue::Int(b)) => Ok(PyValue::Int(int_op(*a, *b))),
        (PyValue::Float(a), PyValue::Float(b)) => Ok(PyValue::Float(float_op(*a, *b))),
        (PyValue::Int(a), PyValue::Float(b)) => Ok(PyValue::Float(float_op(*a as f64, *b))),
        (PyValue::Float(a), PyValue::Int(b)) => Ok(PyValue::Float(float_op(*a, *b as f64))),
        _ => Err(Error::Type {
            expected: "numbers".to_string(),
            got: format!("{} and {}", left.type_name(), right.type_name()),
        }),
    }
}

/// Apply an integer binary operation.
fn int_binop<F>(left: &PyValue, right: &PyValue, op: F) -> Result<PyValue>
where
    F: Fn(i64, i64) -> i64,
{
    let a = left.as_int().ok_or_else(|| Error::Type {
        expected: "int".to_string(),
        got: left.type_name().to_string(),
    })?;
    let b = right.as_int().ok_or_else(|| Error::Type {
        expected: "int".to_string(),
        got: right.type_name().to_string(),
    })?;
    Ok(PyValue::Int(op(a, b)))
}

/// Compare two values with given comparison functions.
pub fn compare_values<F, G>(
    left: &PyValue,
    right: &PyValue,
    int_cmp: F,
    float_cmp: G,
) -> Result<bool>
where
    F: Fn(i64, i64) -> bool,
    G: Fn(f64, f64) -> bool,
{
    match (left, right) {
        (PyValue::Int(a), PyValue::Int(b)) => Ok(int_cmp(*a, *b)),
        (PyValue::Float(a), PyValue::Float(b)) => Ok(float_cmp(*a, *b)),
        (PyValue::Int(a), PyValue::Float(b)) => Ok(float_cmp(*a as f64, *b)),
        (PyValue::Float(a), PyValue::Int(b)) => Ok(float_cmp(*a, *b as f64)),
        (PyValue::Str(a), PyValue::Str(b)) => Ok(a.cmp(b) == std::cmp::Ordering::Less
            && int_cmp(0, 1)
            || a.cmp(b) == std::cmp::Ordering::Equal && int_cmp(0, 0)
            || a.cmp(b) == std::cmp::Ordering::Greater && int_cmp(1, 0)),
        _ => Err(Error::Type {
            expected: "comparable types".to_string(),
            got: format!("{} and {}", left.type_name(), right.type_name()),
        }),
    }
}
