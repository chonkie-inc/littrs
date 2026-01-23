use std::collections::HashMap;
use std::fmt;

/// Error when converting a PyValue to a Rust type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    /// The expected Python type name
    pub expected: &'static str,
    /// The actual Python type name
    pub got: &'static str,
}

impl TypeError {
    pub fn new(expected: &'static str, got: &'static str) -> Self {
        Self { expected, got }
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}, got {}", self.expected, self.got)
    }
}

impl std::error::Error for TypeError {}

/// Trait for converting a PyValue to a Rust type.
///
/// This is used by the tool macro to validate and convert arguments
/// from Python to Rust with proper error messages.
///
/// # Example
///
/// ```
/// use litter::{PyValue, FromPyValue};
///
/// let value = PyValue::Str("hello".to_string());
/// let s: String = String::from_py_value(&value).unwrap();
/// assert_eq!(s, "hello");
///
/// let value = PyValue::Int(42);
/// let err = String::from_py_value(&value).unwrap_err();
/// assert_eq!(err.expected, "str");
/// assert_eq!(err.got, "int");
/// ```
pub trait FromPyValue: Sized {
    /// Convert a PyValue to this type.
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError>;

    /// The Python type name expected by this type (for error messages).
    fn expected_type() -> &'static str;
}

/// Represents a Python value in the sandbox.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PyValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<PyValue>),
    Dict(Vec<(String, PyValue)>),
}

impl PyValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            PyValue::None => "NoneType",
            PyValue::Bool(_) => "bool",
            PyValue::Int(_) => "int",
            PyValue::Float(_) => "float",
            PyValue::Str(_) => "str",
            PyValue::List(_) => "list",
            PyValue::Dict(_) => "dict",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            PyValue::None => false,
            PyValue::Bool(b) => *b,
            PyValue::Int(i) => *i != 0,
            PyValue::Float(f) => *f != 0.0,
            PyValue::Str(s) => !s.is_empty(),
            PyValue::List(l) => !l.is_empty(),
            PyValue::Dict(d) => !d.is_empty(),
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            PyValue::Int(i) => Some(*i),
            PyValue::Bool(b) => Some(if *b { 1 } else { 0 }),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            PyValue::Float(f) => Some(*f),
            PyValue::Int(i) => Some(*i as f64),
            PyValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            PyValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PyValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Format value for print() output.
    ///
    /// Unlike Display, this doesn't quote strings (matching Python's print behavior).
    pub fn to_print_string(&self) -> String {
        match self {
            PyValue::None => "None".to_string(),
            PyValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            PyValue::Int(i) => i.to_string(),
            PyValue::Float(f) => {
                if f.fract() == 0.0 {
                    format!("{}.0", f)
                } else {
                    f.to_string()
                }
            }
            PyValue::Str(s) => s.clone(), // No quotes for print
            PyValue::List(items) => {
                let inner: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                format!("[{}]", inner.join(", "))
            }
            PyValue::Dict(pairs) => {
                let inner: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("'{}': {}", k, v))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
        }
    }
}

impl fmt::Display for PyValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PyValue::None => write!(f, "None"),
            PyValue::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            PyValue::Int(i) => write!(f, "{}", i),
            PyValue::Float(fl) => {
                if fl.fract() == 0.0 {
                    write!(f, "{}.0", fl)
                } else {
                    write!(f, "{}", fl)
                }
            }
            PyValue::Str(s) => write!(f, "'{}'", s),
            PyValue::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            PyValue::Dict(pairs) => {
                write!(f, "{{")?;
                for (i, (key, value)) in pairs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "'{}': {}", key, value)?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl From<bool> for PyValue {
    fn from(b: bool) -> Self {
        PyValue::Bool(b)
    }
}

impl From<i64> for PyValue {
    fn from(i: i64) -> Self {
        PyValue::Int(i)
    }
}

impl From<i32> for PyValue {
    fn from(i: i32) -> Self {
        PyValue::Int(i as i64)
    }
}

impl From<f64> for PyValue {
    fn from(f: f64) -> Self {
        PyValue::Float(f)
    }
}

impl From<String> for PyValue {
    fn from(s: String) -> Self {
        PyValue::Str(s)
    }
}

impl From<&str> for PyValue {
    fn from(s: &str) -> Self {
        PyValue::Str(s.to_string())
    }
}

impl<T: Into<PyValue>> From<Vec<T>> for PyValue {
    fn from(v: Vec<T>) -> Self {
        PyValue::List(v.into_iter().map(Into::into).collect())
    }
}

// ============================================================================
// FromPyValue implementations
// ============================================================================

impl FromPyValue for PyValue {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        Ok(value.clone())
    }

    fn expected_type() -> &'static str {
        "any"
    }
}

impl FromPyValue for String {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::Str(s) => Ok(s.clone()),
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "str"
    }
}

impl FromPyValue for i64 {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::Int(i) => Ok(*i),
            // Python treats bools as ints
            PyValue::Bool(b) => Ok(if *b { 1 } else { 0 }),
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "int"
    }
}

impl FromPyValue for i32 {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::Int(i) => Ok(*i as i32),
            PyValue::Bool(b) => Ok(if *b { 1 } else { 0 }),
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "int"
    }
}

impl FromPyValue for f64 {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::Float(f) => Ok(*f),
            // Python allows int -> float coercion
            PyValue::Int(i) => Ok(*i as f64),
            PyValue::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "float"
    }
}

impl FromPyValue for f32 {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::Float(f) => Ok(*f as f32),
            PyValue::Int(i) => Ok(*i as f32),
            PyValue::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "float"
    }
}

impl FromPyValue for bool {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::Bool(b) => Ok(*b),
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "bool"
    }
}

impl<T: FromPyValue> FromPyValue for Option<T> {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::None => Ok(None),
            other => T::from_py_value(other).map(Some),
        }
    }

    fn expected_type() -> &'static str {
        // This is a simplification; ideally we'd say "T | None"
        "optional"
    }
}

impl<T: FromPyValue> FromPyValue for Vec<T> {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::List(items) => items.iter().map(T::from_py_value).collect(),
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "list"
    }
}

impl<V: FromPyValue> FromPyValue for HashMap<String, V> {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::Dict(pairs) => {
                let mut map = HashMap::new();
                for (k, v) in pairs {
                    map.insert(k.clone(), V::from_py_value(v)?);
                }
                Ok(map)
            }
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "dict"
    }
}

// ============================================================================
// Unit type for functions that return nothing
// ============================================================================

impl FromPyValue for () {
    fn from_py_value(value: &PyValue) -> Result<Self, TypeError> {
        match value {
            PyValue::None => Ok(()),
            other => Err(TypeError::new(Self::expected_type(), other.type_name())),
        }
    }

    fn expected_type() -> &'static str {
        "None"
    }
}

impl From<()> for PyValue {
    fn from(_: ()) -> Self {
        PyValue::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_from_py_value() {
        let value = PyValue::Str("hello".to_string());
        assert_eq!(String::from_py_value(&value).unwrap(), "hello");

        let value = PyValue::Int(42);
        let err = String::from_py_value(&value).unwrap_err();
        assert_eq!(err.expected, "str");
        assert_eq!(err.got, "int");
    }

    #[test]
    fn test_int_from_py_value() {
        assert_eq!(i64::from_py_value(&PyValue::Int(42)).unwrap(), 42);
        assert_eq!(i64::from_py_value(&PyValue::Bool(true)).unwrap(), 1);
        assert_eq!(i64::from_py_value(&PyValue::Bool(false)).unwrap(), 0);

        let err = i64::from_py_value(&PyValue::Str("42".into())).unwrap_err();
        assert_eq!(err.expected, "int");
        assert_eq!(err.got, "str");
    }

    #[test]
    fn test_float_from_py_value() {
        assert_eq!(f64::from_py_value(&PyValue::Float(3.14)).unwrap(), 3.14);
        assert_eq!(f64::from_py_value(&PyValue::Int(42)).unwrap(), 42.0);
        assert_eq!(f64::from_py_value(&PyValue::Bool(true)).unwrap(), 1.0);

        let err = f64::from_py_value(&PyValue::Str("3.14".into())).unwrap_err();
        assert_eq!(err.expected, "float");
    }

    #[test]
    fn test_bool_from_py_value() {
        assert!(bool::from_py_value(&PyValue::Bool(true)).unwrap());
        assert!(!bool::from_py_value(&PyValue::Bool(false)).unwrap());

        // bool doesn't coerce from int (unlike Python)
        let err = bool::from_py_value(&PyValue::Int(1)).unwrap_err();
        assert_eq!(err.expected, "bool");
    }

    #[test]
    fn test_option_from_py_value() {
        let none: Option<String> = Option::from_py_value(&PyValue::None).unwrap();
        assert_eq!(none, None);

        let some: Option<String> =
            Option::from_py_value(&PyValue::Str("hello".into())).unwrap();
        assert_eq!(some, Some("hello".to_string()));

        // Type error propagates
        let err = Option::<String>::from_py_value(&PyValue::Int(42)).unwrap_err();
        assert_eq!(err.expected, "str");
    }

    #[test]
    fn test_vec_from_py_value() {
        let list = PyValue::List(vec![
            PyValue::Int(1),
            PyValue::Int(2),
            PyValue::Int(3),
        ]);
        let vec: Vec<i64> = Vec::from_py_value(&list).unwrap();
        assert_eq!(vec, vec![1, 2, 3]);

        // Error on wrong inner type
        let list = PyValue::List(vec![PyValue::Int(1), PyValue::Str("two".into())]);
        let err = Vec::<i64>::from_py_value(&list).unwrap_err();
        assert_eq!(err.expected, "int");
        assert_eq!(err.got, "str");
    }

    #[test]
    fn test_hashmap_from_py_value() {
        let dict = PyValue::Dict(vec![
            ("a".to_string(), PyValue::Int(1)),
            ("b".to_string(), PyValue::Int(2)),
        ]);
        let map: HashMap<String, i64> = HashMap::from_py_value(&dict).unwrap();
        assert_eq!(map.get("a"), Some(&1));
        assert_eq!(map.get("b"), Some(&2));
    }

    #[test]
    fn test_type_error_display() {
        let err = TypeError::new("str", "int");
        assert_eq!(err.to_string(), "expected str, got int");
    }
}
