use std::collections::HashMap;
use std::sync::Arc;

use rustpython_parser::ast::{
    BoolOp, CmpOp, Constant, Expr, Operator, Stmt, UnaryOp,
};
use rustpython_parser::{parse, Mode};

use crate::error::{Error, Result};
use crate::value::PyValue;

/// A registered tool function that can be called from Python code.
pub type ToolFn = Arc<dyn Fn(Vec<PyValue>) -> PyValue + Send + Sync>;

/// The evaluator that executes Python AST nodes.
pub struct Evaluator {
    /// Variable bindings in the current scope.
    variables: HashMap<String, PyValue>,
    /// Registered tool functions.
    tools: HashMap<String, ToolFn>,
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            tools: HashMap::new(),
        }
    }

    pub fn register_tool(&mut self, name: impl Into<String>, f: ToolFn) {
        self.tools.insert(name.into(), f);
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: PyValue) {
        self.variables.insert(name.into(), value);
    }

    pub fn execute(&mut self, code: &str) -> Result<PyValue> {
        let ast = parse(code, Mode::Module, "<sandbox>")
            .map_err(|e| Error::Parse(e.to_string()))?;

        let module = ast
            .as_module()
            .ok_or_else(|| Error::Parse("Expected module".to_string()))?;

        let mut result = PyValue::None;
        for stmt in &module.body {
            result = self.eval_stmt(stmt)?;
        }
        Ok(result)
    }

    fn eval_stmt(&mut self, stmt: &Stmt) -> Result<PyValue> {
        match stmt {
            Stmt::Expr(expr_stmt) => self.eval_expr(&expr_stmt.value),

            Stmt::Assign(assign) => {
                let value = self.eval_expr(&assign.value)?;
                for target in &assign.targets {
                    self.assign_target(target, value.clone())?;
                }
                Ok(PyValue::None)
            }

            Stmt::AugAssign(aug) => {
                let current = self.eval_expr(&aug.target)?;
                let right = self.eval_expr(&aug.value)?;
                let result = self.apply_binop(&aug.op, &current, &right)?;
                self.assign_target(&aug.target, result)?;
                Ok(PyValue::None)
            }

            Stmt::If(if_stmt) => {
                let test = self.eval_expr(&if_stmt.test)?;
                if test.is_truthy() {
                    self.eval_body(&if_stmt.body)
                } else {
                    self.eval_body(&if_stmt.orelse)
                }
            }

            Stmt::While(while_stmt) => {
                let mut result = PyValue::None;
                while self.eval_expr(&while_stmt.test)?.is_truthy() {
                    result = self.eval_body(&while_stmt.body)?;
                }
                Ok(result)
            }

            Stmt::For(for_stmt) => {
                let iter_value = self.eval_expr(&for_stmt.iter)?;
                let items = match iter_value {
                    PyValue::List(items) => items,
                    PyValue::Str(s) => s.chars().map(|c| PyValue::Str(c.to_string())).collect(),
                    other => {
                        return Err(Error::Type {
                            expected: "iterable".to_string(),
                            got: other.type_name().to_string(),
                        })
                    }
                };

                let mut result = PyValue::None;
                for item in items {
                    self.assign_target(&for_stmt.target, item)?;
                    result = self.eval_body(&for_stmt.body)?;
                }
                Ok(result)
            }

            Stmt::Pass(_) => Ok(PyValue::None),

            Stmt::Break(_) => Err(Error::Unsupported(
                "break outside loop not yet supported".to_string(),
            )),

            Stmt::Continue(_) => Err(Error::Unsupported(
                "continue outside loop not yet supported".to_string(),
            )),

            Stmt::Return(ret) => {
                let value = match &ret.value {
                    Some(expr) => self.eval_expr(expr)?,
                    None => PyValue::None,
                };
                Ok(value)
            }

            _ => Err(Error::Unsupported(format!(
                "Statement type not supported: {:?}",
                std::mem::discriminant(stmt)
            ))),
        }
    }

    fn eval_body(&mut self, body: &[Stmt]) -> Result<PyValue> {
        let mut result = PyValue::None;
        for stmt in body {
            result = self.eval_stmt(stmt)?;
        }
        Ok(result)
    }

    fn assign_target(&mut self, target: &Expr, value: PyValue) -> Result<()> {
        match target {
            Expr::Name(name) => {
                self.variables.insert(name.id.to_string(), value);
                Ok(())
            }
            Expr::Subscript(sub) => {
                let idx = self.eval_expr(&sub.slice)?;
                let idx = idx.as_int().ok_or_else(|| Error::Type {
                    expected: "int".to_string(),
                    got: idx.type_name().to_string(),
                })?;

                if let Expr::Name(name) = sub.value.as_ref() {
                    let list = self.variables.get_mut(&name.id.to_string()).ok_or_else(|| {
                        Error::NameError(name.id.to_string())
                    })?;

                    if let PyValue::List(items) = list {
                        let len = items.len() as i64;
                        let actual_idx = if idx < 0 { len + idx } else { idx } as usize;
                        if actual_idx < items.len() {
                            items[actual_idx] = value;
                            return Ok(());
                        }
                    }
                }
                Err(Error::Runtime("Cannot assign to subscript".to_string()))
            }
            _ => Err(Error::Unsupported(
                "Assignment target not supported".to_string(),
            )),
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<PyValue> {
        match expr {
            Expr::Constant(constant) => self.eval_constant(&constant.value),

            Expr::Name(name) => {
                // Check if it's a builtin constant name
                match name.id.as_str() {
                    "True" => return Ok(PyValue::Bool(true)),
                    "False" => return Ok(PyValue::Bool(false)),
                    "None" => return Ok(PyValue::None),
                    _ => {}
                }

                // Check variables
                if let Some(value) = self.variables.get(&name.id.to_string()) {
                    return Ok(value.clone());
                }

                // Check if it's a tool (will be used in Call)
                if self.tools.contains_key(name.id.as_str()) {
                    return Err(Error::NameError(format!(
                        "'{}' is a tool, not a variable",
                        name.id
                    )));
                }

                Err(Error::NameError(name.id.to_string()))
            }

            Expr::List(list) => {
                let items: Result<Vec<PyValue>> =
                    list.elts.iter().map(|e| self.eval_expr(e)).collect();
                Ok(PyValue::List(items?))
            }

            Expr::Dict(dict) => {
                let mut pairs = Vec::new();
                for (key, value) in dict.keys.iter().zip(dict.values.iter()) {
                    let key = match key {
                        Some(k) => {
                            let k = self.eval_expr(k)?;
                            match k {
                                PyValue::Str(s) => s,
                                _ => {
                                    return Err(Error::Type {
                                        expected: "str".to_string(),
                                        got: k.type_name().to_string(),
                                    })
                                }
                            }
                        }
                        None => return Err(Error::Unsupported("Dict unpacking".to_string())),
                    };
                    let value = self.eval_expr(value)?;
                    pairs.push((key, value));
                }
                Ok(PyValue::Dict(pairs))
            }

            Expr::BinOp(binop) => {
                let left = self.eval_expr(&binop.left)?;
                let right = self.eval_expr(&binop.right)?;
                self.apply_binop(&binop.op, &left, &right)
            }

            Expr::UnaryOp(unary) => {
                let operand = self.eval_expr(&unary.operand)?;
                match unary.op {
                    UnaryOp::Not => Ok(PyValue::Bool(!operand.is_truthy())),
                    UnaryOp::USub => match operand {
                        PyValue::Int(i) => Ok(PyValue::Int(-i)),
                        PyValue::Float(f) => Ok(PyValue::Float(-f)),
                        _ => Err(Error::Type {
                            expected: "number".to_string(),
                            got: operand.type_name().to_string(),
                        }),
                    },
                    UnaryOp::UAdd => match operand {
                        PyValue::Int(_) | PyValue::Float(_) => Ok(operand),
                        _ => Err(Error::Type {
                            expected: "number".to_string(),
                            got: operand.type_name().to_string(),
                        }),
                    },
                    UnaryOp::Invert => match operand {
                        PyValue::Int(i) => Ok(PyValue::Int(!i)),
                        _ => Err(Error::Type {
                            expected: "int".to_string(),
                            got: operand.type_name().to_string(),
                        }),
                    },
                }
            }

            Expr::BoolOp(boolop) => {
                match boolop.op {
                    BoolOp::And => {
                        for value in &boolop.values {
                            let v = self.eval_expr(value)?;
                            if !v.is_truthy() {
                                return Ok(v);
                            }
                        }
                        self.eval_expr(boolop.values.last().unwrap())
                    }
                    BoolOp::Or => {
                        for value in &boolop.values {
                            let v = self.eval_expr(value)?;
                            if v.is_truthy() {
                                return Ok(v);
                            }
                        }
                        self.eval_expr(boolop.values.last().unwrap())
                    }
                }
            }

            Expr::Compare(cmp) => {
                let mut left = self.eval_expr(&cmp.left)?;
                for (op, right_expr) in cmp.ops.iter().zip(cmp.comparators.iter()) {
                    let right = self.eval_expr(right_expr)?;
                    let result = self.apply_cmpop(op, &left, &right)?;
                    if !result {
                        return Ok(PyValue::Bool(false));
                    }
                    left = right;
                }
                Ok(PyValue::Bool(true))
            }

            Expr::IfExp(ifexp) => {
                let test = self.eval_expr(&ifexp.test)?;
                if test.is_truthy() {
                    self.eval_expr(&ifexp.body)
                } else {
                    self.eval_expr(&ifexp.orelse)
                }
            }

            Expr::Call(call) => {
                // Get function name
                let func_name = match call.func.as_ref() {
                    Expr::Name(name) => name.id.to_string(),
                    _ => {
                        return Err(Error::Unsupported(
                            "Only named function calls supported".to_string(),
                        ))
                    }
                };

                // Handle builtins
                match func_name.as_str() {
                    "len" => {
                        if call.args.len() != 1 {
                            return Err(Error::Runtime("len() takes exactly 1 argument".to_string()));
                        }
                        let arg = self.eval_expr(&call.args[0])?;
                        let len = match arg {
                            PyValue::Str(s) => s.len(),
                            PyValue::List(l) => l.len(),
                            PyValue::Dict(d) => d.len(),
                            _ => {
                                return Err(Error::Type {
                                    expected: "sized".to_string(),
                                    got: arg.type_name().to_string(),
                                })
                            }
                        };
                        return Ok(PyValue::Int(len as i64));
                    }
                    "str" => {
                        if call.args.len() != 1 {
                            return Err(Error::Runtime("str() takes exactly 1 argument".to_string()));
                        }
                        let arg = self.eval_expr(&call.args[0])?;
                        return Ok(PyValue::Str(format!("{}", arg)));
                    }
                    "int" => {
                        if call.args.len() != 1 {
                            return Err(Error::Runtime("int() takes exactly 1 argument".to_string()));
                        }
                        let arg = self.eval_expr(&call.args[0])?;
                        let val = match arg {
                            PyValue::Int(i) => i,
                            PyValue::Float(f) => f as i64,
                            PyValue::Bool(b) => if b { 1 } else { 0 },
                            PyValue::Str(s) => s.parse().map_err(|_| {
                                Error::Runtime(format!("invalid literal for int(): '{}'", s))
                            })?,
                            _ => {
                                return Err(Error::Type {
                                    expected: "number or string".to_string(),
                                    got: arg.type_name().to_string(),
                                })
                            }
                        };
                        return Ok(PyValue::Int(val));
                    }
                    "float" => {
                        if call.args.len() != 1 {
                            return Err(Error::Runtime("float() takes exactly 1 argument".to_string()));
                        }
                        let arg = self.eval_expr(&call.args[0])?;
                        let val = match arg {
                            PyValue::Float(f) => f,
                            PyValue::Int(i) => i as f64,
                            PyValue::Bool(b) => if b { 1.0 } else { 0.0 },
                            PyValue::Str(s) => s.parse().map_err(|_| {
                                Error::Runtime(format!("invalid literal for float(): '{}'", s))
                            })?,
                            _ => {
                                return Err(Error::Type {
                                    expected: "number or string".to_string(),
                                    got: arg.type_name().to_string(),
                                })
                            }
                        };
                        return Ok(PyValue::Float(val));
                    }
                    "bool" => {
                        if call.args.len() != 1 {
                            return Err(Error::Runtime("bool() takes exactly 1 argument".to_string()));
                        }
                        let arg = self.eval_expr(&call.args[0])?;
                        return Ok(PyValue::Bool(arg.is_truthy()));
                    }
                    "list" => {
                        if call.args.is_empty() {
                            return Ok(PyValue::List(vec![]));
                        }
                        if call.args.len() != 1 {
                            return Err(Error::Runtime("list() takes at most 1 argument".to_string()));
                        }
                        let arg = self.eval_expr(&call.args[0])?;
                        let items = match arg {
                            PyValue::List(l) => l,
                            PyValue::Str(s) => {
                                s.chars().map(|c| PyValue::Str(c.to_string())).collect()
                            }
                            _ => {
                                return Err(Error::Type {
                                    expected: "iterable".to_string(),
                                    got: arg.type_name().to_string(),
                                })
                            }
                        };
                        return Ok(PyValue::List(items));
                    }
                    "range" => {
                        let args: Result<Vec<PyValue>> = call
                            .args
                            .iter()
                            .map(|a| self.eval_expr(a))
                            .collect();
                        let args = args?;

                        let (start, stop, step) = match args.len() {
                            1 => (0, args[0].as_int().ok_or_else(|| Error::Type {
                                expected: "int".to_string(),
                                got: args[0].type_name().to_string(),
                            })?, 1),
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
                            _ => return Err(Error::Runtime(
                                "range() takes 1 to 3 arguments".to_string()
                            )),
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
                        return Ok(PyValue::List(items));
                    }
                    "print" => {
                        let args: Result<Vec<PyValue>> = call
                            .args
                            .iter()
                            .map(|a| self.eval_expr(a))
                            .collect();
                        let _ = args?;
                        return Ok(PyValue::None);
                    }
                    "abs" => {
                        if call.args.len() != 1 {
                            return Err(Error::Runtime("abs() takes exactly 1 argument".to_string()));
                        }
                        let arg = self.eval_expr(&call.args[0])?;
                        match arg {
                            PyValue::Int(i) => return Ok(PyValue::Int(i.abs())),
                            PyValue::Float(f) => return Ok(PyValue::Float(f.abs())),
                            _ => {
                                return Err(Error::Type {
                                    expected: "number".to_string(),
                                    got: arg.type_name().to_string(),
                                })
                            }
                        }
                    }
                    "min" => {
                        if call.args.is_empty() {
                            return Err(Error::Runtime("min() requires at least 1 argument".to_string()));
                        }
                        let args: Result<Vec<PyValue>> = call
                            .args
                            .iter()
                            .map(|a| self.eval_expr(a))
                            .collect();
                        let args = args?;

                        if args.len() == 1 {
                            if let PyValue::List(items) = &args[0] {
                                if items.is_empty() {
                                    return Err(Error::Runtime("min() arg is an empty sequence".to_string()));
                                }
                                return self.find_min(items);
                            }
                        }
                        return self.find_min(&args);
                    }
                    "max" => {
                        if call.args.is_empty() {
                            return Err(Error::Runtime("max() requires at least 1 argument".to_string()));
                        }
                        let args: Result<Vec<PyValue>> = call
                            .args
                            .iter()
                            .map(|a| self.eval_expr(a))
                            .collect();
                        let args = args?;

                        if args.len() == 1 {
                            if let PyValue::List(items) = &args[0] {
                                if items.is_empty() {
                                    return Err(Error::Runtime("max() arg is an empty sequence".to_string()));
                                }
                                return self.find_max(items);
                            }
                        }
                        return self.find_max(&args);
                    }
                    "sum" => {
                        if call.args.is_empty() {
                            return Err(Error::Runtime("sum() requires at least 1 argument".to_string()));
                        }
                        let arg = self.eval_expr(&call.args[0])?;
                        let items = match arg {
                            PyValue::List(items) => items,
                            _ => {
                                return Err(Error::Type {
                                    expected: "iterable".to_string(),
                                    got: arg.type_name().to_string(),
                                })
                            }
                        };

                        let mut total = 0i64;
                        let mut is_float = false;
                        let mut total_float = 0.0f64;

                        for item in items {
                            match item {
                                PyValue::Int(i) => {
                                    if is_float {
                                        total_float += i as f64;
                                    } else {
                                        total += i;
                                    }
                                }
                                PyValue::Float(f) => {
                                    if !is_float {
                                        is_float = true;
                                        total_float = total as f64;
                                    }
                                    total_float += f;
                                }
                                _ => {
                                    return Err(Error::Type {
                                        expected: "number".to_string(),
                                        got: item.type_name().to_string(),
                                    })
                                }
                            }
                        }

                        if is_float {
                            return Ok(PyValue::Float(total_float));
                        }
                        return Ok(PyValue::Int(total));
                    }
                    _ => {}
                }

                // Check if it's a registered tool
                if let Some(tool) = self.tools.get(&func_name).cloned() {
                    let args: Result<Vec<PyValue>> = call
                        .args
                        .iter()
                        .map(|a| self.eval_expr(a))
                        .collect();
                    return Ok(tool(args?));
                }

                Err(Error::NameError(func_name))
            }

            Expr::Subscript(sub) => {
                let value = self.eval_expr(&sub.value)?;
                let slice = self.eval_expr(&sub.slice)?;

                match (&value, &slice) {
                    (PyValue::List(items), PyValue::Int(idx)) => {
                        let len = items.len() as i64;
                        let actual_idx = if *idx < 0 { len + idx } else { *idx } as usize;
                        items.get(actual_idx).cloned().ok_or_else(|| {
                            Error::Runtime(format!("list index out of range: {}", idx))
                        })
                    }
                    (PyValue::Str(s), PyValue::Int(idx)) => {
                        let len = s.len() as i64;
                        let actual_idx = if *idx < 0 { len + idx } else { *idx } as usize;
                        s.chars()
                            .nth(actual_idx)
                            .map(|c| PyValue::Str(c.to_string()))
                            .ok_or_else(|| {
                                Error::Runtime(format!("string index out of range: {}", idx))
                            })
                    }
                    (PyValue::Dict(pairs), PyValue::Str(key)) => {
                        pairs
                            .iter()
                            .find(|(k, _)| k == key)
                            .map(|(_, v)| v.clone())
                            .ok_or_else(|| Error::Runtime(format!("KeyError: '{}'", key)))
                    }
                    _ => Err(Error::Type {
                        expected: "subscriptable".to_string(),
                        got: value.type_name().to_string(),
                    }),
                }
            }

            Expr::Attribute(attr) => {
                let value = self.eval_expr(&attr.value)?;
                let attr_name = attr.attr.as_str();

                match (&value, attr_name) {
                    (PyValue::Str(_), "upper" | "lower" | "strip" | "split") => {
                        Err(Error::Unsupported(format!(
                            "String method '{}' - use function call syntax",
                            attr_name
                        )))
                    }
                    (PyValue::List(_), "append" | "pop" | "extend") => {
                        Err(Error::Unsupported(format!(
                            "List method '{}' - use function call syntax",
                            attr_name
                        )))
                    }
                    _ => Err(Error::Unsupported(format!(
                        "Attribute access: {}.{}",
                        value.type_name(),
                        attr_name
                    ))),
                }
            }

            _ => Err(Error::Unsupported(format!(
                "Expression type not supported: {:?}",
                std::mem::discriminant(expr)
            ))),
        }
    }

    fn eval_constant(&self, constant: &Constant) -> Result<PyValue> {
        match constant {
            Constant::None => Ok(PyValue::None),
            Constant::Bool(b) => Ok(PyValue::Bool(*b)),
            Constant::Int(i) => {
                // Convert BigInt to i64
                let val: i64 = i.try_into().map_err(|_| {
                    Error::Runtime("Integer too large".to_string())
                })?;
                Ok(PyValue::Int(val))
            }
            Constant::Float(f) => Ok(PyValue::Float(*f)),
            Constant::Str(s) => Ok(PyValue::Str(s.clone())),
            Constant::Bytes(_) => Err(Error::Unsupported("Bytes literals".to_string())),
            Constant::Tuple(items) => {
                let values: Result<Vec<PyValue>> = items
                    .iter()
                    .map(|c| self.eval_constant(c))
                    .collect();
                Ok(PyValue::List(values?))
            }
            Constant::Complex { .. } => Err(Error::Unsupported("Complex numbers".to_string())),
            Constant::Ellipsis => Err(Error::Unsupported("Ellipsis".to_string())),
        }
    }

    fn apply_binop(&self, op: &Operator, left: &PyValue, right: &PyValue) -> Result<PyValue> {
        match op {
            Operator::Add => match (left, right) {
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
            Operator::Sub => self.numeric_binop(left, right, |a, b| a - b, |a, b| a - b),
            Operator::Mult => match (left, right) {
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
            Operator::Div => {
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
            Operator::FloorDiv => {
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
            Operator::Mod => {
                match (left, right) {
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
                }
            }
            Operator::Pow => {
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
            Operator::BitOr => self.int_binop(left, right, |a, b| a | b),
            Operator::BitXor => self.int_binop(left, right, |a, b| a ^ b),
            Operator::BitAnd => self.int_binop(left, right, |a, b| a & b),
            Operator::LShift => self.int_binop(left, right, |a, b| a << b),
            Operator::RShift => self.int_binop(left, right, |a, b| a >> b),
            _ => Err(Error::Unsupported(format!("Operator {:?}", op))),
        }
    }

    fn numeric_binop<F, G>(&self, left: &PyValue, right: &PyValue, int_op: F, float_op: G) -> Result<PyValue>
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

    fn int_binop<F>(&self, left: &PyValue, right: &PyValue, op: F) -> Result<PyValue>
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

    fn apply_cmpop(&self, op: &CmpOp, left: &PyValue, right: &PyValue) -> Result<bool> {
        match op {
            CmpOp::Eq => Ok(left == right),
            CmpOp::NotEq => Ok(left != right),
            CmpOp::Lt => self.compare_values(left, right, |a, b| a < b, |a, b| a < b),
            CmpOp::LtE => self.compare_values(left, right, |a, b| a <= b, |a, b| a <= b),
            CmpOp::Gt => self.compare_values(left, right, |a, b| a > b, |a, b| a > b),
            CmpOp::GtE => self.compare_values(left, right, |a, b| a >= b, |a, b| a >= b),
            CmpOp::In => {
                match right {
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
                }
            }
            CmpOp::NotIn => {
                let in_result = self.apply_cmpop(&CmpOp::In, left, right)?;
                Ok(!in_result)
            }
            CmpOp::Is => {
                match (left, right) {
                    (PyValue::None, PyValue::None) => Ok(true),
                    _ => Ok(false),
                }
            }
            CmpOp::IsNot => {
                let is_result = self.apply_cmpop(&CmpOp::Is, left, right)?;
                Ok(!is_result)
            }
        }
    }

    fn compare_values<F, G>(&self, left: &PyValue, right: &PyValue, int_cmp: F, float_cmp: G) -> Result<bool>
    where
        F: Fn(i64, i64) -> bool,
        G: Fn(f64, f64) -> bool,
    {
        match (left, right) {
            (PyValue::Int(a), PyValue::Int(b)) => Ok(int_cmp(*a, *b)),
            (PyValue::Float(a), PyValue::Float(b)) => Ok(float_cmp(*a, *b)),
            (PyValue::Int(a), PyValue::Float(b)) => Ok(float_cmp(*a as f64, *b)),
            (PyValue::Float(a), PyValue::Int(b)) => Ok(float_cmp(*a, *b as f64)),
            (PyValue::Str(a), PyValue::Str(b)) => Ok(a.cmp(b) == std::cmp::Ordering::Less && int_cmp(0, 1)
                || a.cmp(b) == std::cmp::Ordering::Equal && int_cmp(0, 0)
                || a.cmp(b) == std::cmp::Ordering::Greater && int_cmp(1, 0)),
            _ => Err(Error::Type {
                expected: "comparable types".to_string(),
                got: format!("{} and {}", left.type_name(), right.type_name()),
            }),
        }
    }

    fn find_min(&self, items: &[PyValue]) -> Result<PyValue> {
        let mut min = items[0].clone();
        for item in &items[1..] {
            if self.apply_cmpop(&CmpOp::Lt, item, &min)? {
                min = item.clone();
            }
        }
        Ok(min)
    }

    fn find_max(&self, items: &[PyValue]) -> Result<PyValue> {
        let mut max = items[0].clone();
        for item in &items[1..] {
            if self.apply_cmpop(&CmpOp::Gt, item, &max)? {
                max = item.clone();
            }
        }
        Ok(max)
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}
