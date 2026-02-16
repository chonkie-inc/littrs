//! Built-in modules: json, math, typing.
//!
//! These modules are registered by [`crate::Sandbox::with_builtins()`] and provide
//! commonly-used Python standard library functionality for LLM-generated code.

use crate::sandbox::Sandbox;
use crate::value::PyValue;

/// Register all built-in modules on the given sandbox.
pub fn register_builtins(sandbox: &mut Sandbox) {
    register_json(sandbox);
    register_math(sandbox);
    register_typing(sandbox);
}

// ============================================================================
// json module
// ============================================================================

fn register_json(sandbox: &mut Sandbox) {
    sandbox.module("json", |m| {
        m.function("loads", json_loads);
        m.function("dumps", json_dumps);
    });
}

fn json_loads(args: Vec<PyValue>) -> PyValue {
    let s = match args.first() {
        Some(PyValue::Str(s)) => s.as_str(),
        _ => return PyValue::None,
    };
    match serde_json::from_str::<serde_json::Value>(s) {
        Ok(val) => json_value_to_pyvalue(&val),
        Err(_) => PyValue::None,
    }
}

fn json_dumps(args: Vec<PyValue>) -> PyValue {
    let val = match args.first() {
        Some(v) => v,
        None => return PyValue::Str("null".to_string()),
    };
    let json_val = pyvalue_to_json_value(val);
    match serde_json::to_string(&json_val) {
        Ok(s) => PyValue::Str(s),
        Err(_) => PyValue::None,
    }
}

fn json_value_to_pyvalue(val: &serde_json::Value) -> PyValue {
    match val {
        serde_json::Value::Null => PyValue::None,
        serde_json::Value::Bool(b) => PyValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                PyValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                PyValue::Float(f)
            } else {
                PyValue::None
            }
        }
        serde_json::Value::String(s) => PyValue::Str(s.clone()),
        serde_json::Value::Array(arr) => {
            PyValue::List(arr.iter().map(json_value_to_pyvalue).collect())
        }
        serde_json::Value::Object(obj) => PyValue::Dict(
            obj.iter()
                .map(|(k, v)| (PyValue::Str(k.clone()), json_value_to_pyvalue(v)))
                .collect(),
        ),
    }
}

fn pyvalue_to_json_value(val: &PyValue) -> serde_json::Value {
    match val {
        PyValue::None => serde_json::Value::Null,
        PyValue::Bool(b) => serde_json::Value::Bool(*b),
        PyValue::Int(i) => serde_json::Value::Number((*i).into()),
        PyValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        PyValue::Str(s) => serde_json::Value::String(s.clone()),
        PyValue::List(items) | PyValue::Tuple(items) => {
            serde_json::Value::Array(items.iter().map(pyvalue_to_json_value).collect())
        }
        PyValue::Dict(pairs) => {
            let map: serde_json::Map<String, serde_json::Value> = pairs
                .iter()
                .filter_map(|(k, v)| {
                    if let PyValue::Str(key) = k {
                        Some((key.clone(), pyvalue_to_json_value(v)))
                    } else {
                        None
                    }
                })
                .collect();
            serde_json::Value::Object(map)
        }
        PyValue::Set(items) => {
            serde_json::Value::Array(items.iter().map(pyvalue_to_json_value).collect())
        }
        PyValue::Function(_)
        | PyValue::Module { .. }
        | PyValue::NativeFunction(_)
        | PyValue::File(_) => serde_json::Value::Null,
    }
}

// ============================================================================
// math module
// ============================================================================

fn register_math(sandbox: &mut Sandbox) {
    sandbox.module("math", |m| {
        // Constants
        m.constant("pi", PyValue::Float(std::f64::consts::PI));
        m.constant("e", PyValue::Float(std::f64::consts::E));
        m.constant("inf", PyValue::Float(f64::INFINITY));
        m.constant("nan", PyValue::Float(f64::NAN));
        m.constant("tau", PyValue::Float(std::f64::consts::TAU));

        // Functions
        m.function("sqrt", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.sqrt()))
        });
        m.function("floor", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Int(x.floor() as i64))
        });
        m.function("ceil", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Int(x.ceil() as i64))
        });
        m.function("log", |args| {
            let x = match float_arg(&args) {
                Some(v) => v,
                None => return PyValue::None,
            };
            let result = if args.len() >= 2 {
                if let Some(base) = float_arg_at(&args, 1) {
                    x.ln() / base.ln()
                } else {
                    x.ln()
                }
            } else {
                x.ln()
            };
            PyValue::Float(result)
        });
        m.function("log2", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.log2()))
        });
        m.function("log10", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.log10()))
        });
        m.function("sin", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.sin()))
        });
        m.function("cos", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.cos()))
        });
        m.function("tan", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.tan()))
        });
        m.function("asin", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.asin()))
        });
        m.function("acos", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.acos()))
        });
        m.function("atan", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.atan()))
        });
        m.function("atan2", |args| {
            let y = match float_arg(&args) {
                Some(v) => v,
                None => return PyValue::None,
            };
            let x = match float_arg_at(&args, 1) {
                Some(v) => v,
                None => return PyValue::None,
            };
            PyValue::Float(y.atan2(x))
        });
        m.function("fabs", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.abs()))
        });
        m.function("pow", |args| {
            let x = match float_arg(&args) {
                Some(v) => v,
                None => return PyValue::None,
            };
            let y = match float_arg_at(&args, 1) {
                Some(v) => v,
                None => return PyValue::None,
            };
            PyValue::Float(x.powf(y))
        });
        m.function("isnan", |args| {
            float_arg(&args).map_or(PyValue::Bool(false), |x| PyValue::Bool(x.is_nan()))
        });
        m.function("isinf", |args| {
            float_arg(&args).map_or(PyValue::Bool(false), |x| PyValue::Bool(x.is_infinite()))
        });
        m.function("exp", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.exp()))
        });
        m.function("degrees", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.to_degrees()))
        });
        m.function("radians", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Float(x.to_radians()))
        });
        m.function("trunc", |args| {
            float_arg(&args).map_or(PyValue::None, |x| PyValue::Int(x.trunc() as i64))
        });
        m.function("gcd", |args| {
            let a = match args.first().and_then(|v| v.as_int()) {
                Some(v) => v.unsigned_abs(),
                None => return PyValue::None,
            };
            let b = match args.get(1).and_then(|v| v.as_int()) {
                Some(v) => v.unsigned_abs(),
                None => return PyValue::None,
            };
            PyValue::Int(gcd(a, b) as i64)
        });
        m.function("factorial", |args| {
            let n = match args.first().and_then(|v| v.as_int()) {
                Some(v) if v >= 0 => v as u64,
                _ => return PyValue::None,
            };
            if n > 20 {
                return PyValue::None; // overflow protection
            }
            let mut result: u64 = 1;
            for i in 2..=n {
                result = result.saturating_mul(i);
            }
            PyValue::Int(result as i64)
        });
    });
}

/// Extract a float from the first argument (coercing int → float).
fn float_arg(args: &[PyValue]) -> Option<f64> {
    float_arg_at(args, 0)
}

/// Extract a float from argument at the given index.
fn float_arg_at(args: &[PyValue], idx: usize) -> Option<f64> {
    args.get(idx).and_then(|v| v.as_float())
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

// ============================================================================
// typing module
// ============================================================================

fn register_typing(sandbox: &mut Sandbox) {
    sandbox.module("typing", |m| {
        // All typing names map to None — no runtime effect, just prevents import errors
        let typing_names = [
            "Any",
            "Union",
            "Optional",
            "List",
            "Dict",
            "Tuple",
            "Set",
            "FrozenSet",
            "Sequence",
            "Mapping",
            "MutableMapping",
            "Iterable",
            "Iterator",
            "Generator",
            "Callable",
            "Type",
            "ClassVar",
            "Final",
            "Literal",
            "TypeVar",
            "Generic",
            "Protocol",
            "NamedTuple",
            "TypedDict",
            "Annotated",
            "TypeAlias",
            "ParamSpec",
            "Concatenate",
            "TypeGuard",
            "Never",
            "NoReturn",
            "Self",
            "Unpack",
            "Required",
            "NotRequired",
        ];
        for name in typing_names {
            m.constant(name, PyValue::None);
        }
    });
}
