//! Slicing implementation for Python sequences.
//!
//! This module handles Python-style slicing for lists and strings.

use crate::error::{Error, Result};
use crate::value::PyValue;

/// Slice a list with Python semantics.
///
/// Handles positive and negative indices, as well as step values.
pub fn slice_list(
    items: &[PyValue],
    lower: Option<i64>,
    upper: Option<i64>,
    step: Option<i64>,
) -> Result<PyValue> {
    let len = items.len() as i64;
    let step = step.unwrap_or(1);

    if step == 0 {
        return Err(Error::Runtime("slice step cannot be zero".to_string()));
    }

    if step > 0 {
        let start = match lower {
            Some(i) if i < 0 => (len + i).max(0) as usize,
            Some(i) => (i as usize).min(items.len()),
            None => 0,
        };
        let end = match upper {
            Some(i) if i < 0 => (len + i).max(0) as usize,
            Some(i) => (i as usize).min(items.len()),
            None => items.len(),
        };

        if start >= end {
            return Ok(PyValue::List(vec![]));
        }

        if step == 1 {
            Ok(PyValue::List(items[start..end].to_vec()))
        } else {
            let result: Vec<PyValue> = (start..end)
                .step_by(step as usize)
                .filter_map(|i| items.get(i).cloned())
                .collect();
            Ok(PyValue::List(result))
        }
    } else {
        // Negative step (reverse iteration)
        let start = match lower {
            Some(i) if i < 0 => (len + i) as usize,
            Some(i) => (i as usize).min(items.len().saturating_sub(1)),
            None => items.len().saturating_sub(1),
        };
        let end = match upper {
            Some(i) if i < 0 => ((len + i) as isize - 1).max(-1) as usize,
            Some(i) if i as usize >= items.len() => items.len(),
            Some(i) => i as usize,
            None => 0,
        };

        let mut result = Vec::new();
        let mut i = start as isize;
        let end_check = if upper.is_none() { -1 } else { end as isize };

        while i > end_check && i >= 0 && (i as usize) < items.len() {
            result.push(items[i as usize].clone());
            i += step as isize;
        }
        Ok(PyValue::List(result))
    }
}

/// Slice a tuple with Python semantics.
///
/// Same logic as `slice_list` but wraps result in `PyValue::Tuple`.
pub fn slice_tuple(
    items: &[PyValue],
    lower: Option<i64>,
    upper: Option<i64>,
    step: Option<i64>,
) -> Result<PyValue> {
    // Reuse slice_list logic, then convert List → Tuple
    let result = slice_list(items, lower, upper, step)?;
    match result {
        PyValue::List(items) => Ok(PyValue::Tuple(items)),
        other => Ok(other),
    }
}

/// Slice a string with Python semantics.
///
/// Handles proper Unicode character boundaries.
pub fn slice_string(
    s: &str,
    lower: Option<i64>,
    upper: Option<i64>,
    step: Option<i64>,
) -> Result<PyValue> {
    // Convert string to chars for proper Unicode handling
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;
    let step = step.unwrap_or(1);

    if step == 0 {
        return Err(Error::Runtime("slice step cannot be zero".to_string()));
    }

    if step > 0 {
        let start = match lower {
            Some(i) if i < 0 => (len + i).max(0) as usize,
            Some(i) => (i as usize).min(chars.len()),
            None => 0,
        };
        let end = match upper {
            Some(i) if i < 0 => (len + i).max(0) as usize,
            Some(i) => (i as usize).min(chars.len()),
            None => chars.len(),
        };

        if start >= end {
            return Ok(PyValue::Str(String::new()));
        }

        if step == 1 {
            Ok(PyValue::Str(chars[start..end].iter().collect()))
        } else {
            let result: String = (start..end)
                .step_by(step as usize)
                .filter_map(|i| chars.get(i))
                .collect();
            Ok(PyValue::Str(result))
        }
    } else {
        // Negative step (reverse)
        let start = match lower {
            Some(i) if i < 0 => (len + i) as usize,
            Some(i) => (i as usize).min(chars.len().saturating_sub(1)),
            None => chars.len().saturating_sub(1),
        };
        let end_check = match upper {
            Some(i) if i < 0 => (len + i) as isize - 1,
            Some(_) => -1,
            None => -1,
        };

        let mut result = String::new();
        let mut i = start as isize;

        while i > end_check && i >= 0 && (i as usize) < chars.len() {
            result.push(chars[i as usize]);
            i += step as isize;
        }
        Ok(PyValue::Str(result))
    }
}
