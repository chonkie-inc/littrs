//! CodeAct-style Python evaluator.
//!
//! This module contains the core evaluator that executes Python AST nodes
//! in a sandboxed environment with registered tools.

use std::collections::HashMap;
use std::sync::Arc;

use rustpython_parser::ast::{
    BoolOp, Constant, Expr, Stmt, UnaryOp,
};
use rustpython_parser::ast::Ranged;
use rustpython_parser::{parse, Mode};

use crate::builtins::{try_builtin, BuiltinResult};
use crate::diagnostic::{Diagnostic, Span};
use crate::error::{Error, Result};
use crate::methods;
use crate::operators::{apply_binop, apply_cmpop};
use crate::slice;
use crate::tool::ToolInfo;
use crate::value::PyValue;

/// A registered tool function that can be called from Python code.
pub type ToolFn = Arc<dyn Fn(Vec<PyValue>) -> PyValue + Send + Sync>;

/// A registered tool with its function and argument names.
#[derive(Clone)]
struct RegisteredTool {
    func: ToolFn,
    /// Argument names for keyword argument support
    arg_names: Vec<String>,
    /// Optional tool info for type validation and diagnostics
    info: Option<ToolInfo>,
}

/// The evaluator that executes Python AST nodes.
#[derive(Clone)]
pub struct Evaluator {
    /// Variable bindings in the current scope.
    variables: HashMap<String, PyValue>,
    /// Registered tool functions with their argument names.
    tools: HashMap<String, RegisteredTool>,
    /// Buffer for print() output.
    print_buffer: Vec<String>,
    /// Current source code being executed (for diagnostics).
    current_source: String,
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            tools: HashMap::new(),
            print_buffer: Vec::new(),
            current_source: String::new(),
        }
    }

    /// Take and clear the print buffer, returning all captured print output.
    pub fn take_print_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.print_buffer)
    }

    /// Get the print buffer contents without clearing.
    #[allow(dead_code)]
    pub fn print_output(&self) -> &[String] {
        &self.print_buffer
    }

    /// Clear the print buffer.
    pub fn clear_print_buffer(&mut self) {
        self.print_buffer.clear();
    }

    /// Register a tool without argument names (positional only).
    pub fn register_tool(&mut self, name: impl Into<String>, f: ToolFn) {
        self.tools.insert(
            name.into(),
            RegisteredTool {
                func: f,
                arg_names: Vec::new(),
                info: None,
            },
        );
    }

    /// Register a tool with full info for type validation and diagnostics.
    pub fn register_tool_with_info(&mut self, info: ToolInfo, f: ToolFn) {
        let arg_names: Vec<String> = info.args.iter().map(|a| a.name.clone()).collect();
        self.tools.insert(
            info.name.clone(),
            RegisteredTool {
                func: f,
                arg_names,
                info: Some(info),
            },
        );
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: PyValue) {
        self.variables.insert(name.into(), value);
    }

    pub fn execute(&mut self, code: &str) -> Result<PyValue> {
        // Store source for diagnostics
        self.current_source = code.to_string();

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
                let result = apply_binop(&aug.op, &current, &right)?;
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

            Expr::ListComp(listcomp) => {
                self.eval_list_comprehension(&listcomp.elt, &listcomp.generators)
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
                apply_binop(&binop.op, &left, &right)
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

            Expr::BoolOp(boolop) => match boolop.op {
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
            },

            Expr::Compare(cmp) => {
                let mut left = self.eval_expr(&cmp.left)?;
                for (op, right_expr) in cmp.ops.iter().zip(cmp.comparators.iter()) {
                    let right = self.eval_expr(right_expr)?;
                    let result = apply_cmpop(op, &left, &right)?;
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

            Expr::Call(call) => self.eval_call(call),

            Expr::Subscript(sub) => {
                let value = self.eval_expr(&sub.value)?;

                // Check if it's a slice expression
                if let Expr::Slice(slice_expr) = sub.slice.as_ref() {
                    return self.eval_slice(&value, slice_expr);
                }

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

    fn eval_call(&mut self, call: &rustpython_parser::ast::ExprCall) -> Result<PyValue> {
        // Check for method calls (e.g., list.append(), str.lower())
        if let Expr::Attribute(attr) = call.func.as_ref() {
            return self.eval_method_call(attr, call);
        }

        // Get function name
        let func_name = match call.func.as_ref() {
            Expr::Name(name) => name.id.to_string(),
            _ => {
                return Err(Error::Unsupported(
                    "Only named function calls supported".to_string(),
                ))
            }
        };

        // Evaluate arguments first
        let args: Result<Vec<PyValue>> = call.args.iter().map(|a| self.eval_expr(a)).collect();
        let args = args?;

        // Try builtin functions first
        match try_builtin(&func_name, args.clone(), &mut self.print_buffer) {
            BuiltinResult::Handled(result) => return result,
            BuiltinResult::NotBuiltin => {}
        }

        // Check if it's a registered tool
        if let Some(tool) = self.tools.get(&func_name).cloned() {
            return self.eval_tool_call(&func_name, &tool, call, args);
        }

        Err(Error::NameError(func_name))
    }

    /// Evaluate a method call like `list.append(item)` or `str.lower()`
    fn eval_method_call(
        &mut self,
        attr: &rustpython_parser::ast::ExprAttribute,
        call: &rustpython_parser::ast::ExprCall,
    ) -> Result<PyValue> {
        let method_name = attr.attr.as_str();

        // Evaluate arguments
        let args: Result<Vec<PyValue>> = call.args.iter().map(|a| self.eval_expr(a)).collect();
        let args = args?;

        // For mutating methods, we need to get the variable name and mutate in place
        if let Expr::Name(name) = attr.value.as_ref() {
            let var_name = name.id.to_string();

            // Check if this is a mutating method that needs special handling
            match method_name {
                "append" | "extend" | "pop" | "clear" | "insert" | "remove" => {
                    return self.eval_list_mutating_method(&var_name, method_name, args);
                }
                "update" | "setdefault" => {
                    return self.eval_dict_mutating_method(&var_name, method_name, args);
                }
                _ => {}
            }
        }

        // For non-mutating methods, evaluate the value and call the method
        let value = self.eval_expr(&attr.value)?;
        self.call_method(&value, method_name, args)
    }

    /// Call a method on a value (non-mutating)
    fn call_method(&self, value: &PyValue, method: &str, args: Vec<PyValue>) -> Result<PyValue> {
        match value {
            PyValue::Str(s) => methods::call_str_method(s, method, args),
            PyValue::List(items) => methods::call_list_method(items, method, args),
            PyValue::Dict(pairs) => methods::call_dict_method(pairs, method, args),
            _ => Err(Error::Unsupported(format!(
                "Method '{}' not supported on type '{}'",
                method,
                value.type_name()
            ))),
        }
    }

    /// List mutating methods (append, extend, pop, etc.)
    fn eval_list_mutating_method(
        &mut self,
        var_name: &str,
        method: &str,
        args: Vec<PyValue>,
    ) -> Result<PyValue> {
        let list = self.variables.get_mut(var_name).ok_or_else(|| {
            Error::NameError(var_name.to_string())
        })?;

        let items = match list {
            PyValue::List(items) => items,
            _ => {
                return Err(Error::Type {
                    expected: "list".to_string(),
                    got: list.type_name().to_string(),
                })
            }
        };

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
            _ => Err(Error::Unsupported(format!(
                "List method '{}' not implemented",
                method
            ))),
        }
    }

    /// Dict mutating methods
    fn eval_dict_mutating_method(
        &mut self,
        var_name: &str,
        method: &str,
        args: Vec<PyValue>,
    ) -> Result<PyValue> {
        let dict = self.variables.get_mut(var_name).ok_or_else(|| {
            Error::NameError(var_name.to_string())
        })?;

        let pairs = match dict {
            PyValue::Dict(pairs) => pairs,
            _ => {
                return Err(Error::Type {
                    expected: "dict".to_string(),
                    got: dict.type_name().to_string(),
                })
            }
        };

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
            _ => Err(Error::Unsupported(format!(
                "Dict method '{}' not implemented",
                method
            ))),
        }
    }

    /// Evaluate a slice expression like `list[1:5]` or `str[:10]`
    fn eval_slice(
        &mut self,
        value: &PyValue,
        slice: &rustpython_parser::ast::ExprSlice,
    ) -> Result<PyValue> {
        // Evaluate slice bounds
        let lower = match &slice.lower {
            Some(expr) => Some(self.eval_expr(expr)?.as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: "non-int".to_string(),
            })?),
            None => None,
        };

        let upper = match &slice.upper {
            Some(expr) => Some(self.eval_expr(expr)?.as_int().ok_or_else(|| Error::Type {
                expected: "int".to_string(),
                got: "non-int".to_string(),
            })?),
            None => None,
        };

        let step = match &slice.step {
            Some(expr) => {
                let s = self.eval_expr(expr)?.as_int().ok_or_else(|| Error::Type {
                    expected: "int".to_string(),
                    got: "non-int".to_string(),
                })?;
                if s == 0 {
                    return Err(Error::Runtime("slice step cannot be zero".to_string()));
                }
                Some(s)
            }
            None => None,
        };

        match value {
            PyValue::List(items) => slice::slice_list(items, lower, upper, step),
            PyValue::Str(s) => slice::slice_string(s, lower, upper, step),
            _ => Err(Error::Type {
                expected: "list or str".to_string(),
                got: value.type_name().to_string(),
            }),
        }
    }

    fn eval_tool_call(
        &mut self,
        func_name: &str,
        tool: &RegisteredTool,
        call: &rustpython_parser::ast::ExprCall,
        positional_args: Vec<PyValue>,
    ) -> Result<PyValue> {
        let mut final_args = positional_args;

        // Handle keyword arguments if the tool has arg names defined
        if !call.keywords.is_empty() && !tool.arg_names.is_empty() {
            // Extend final_args to have enough slots for all possible args
            let max_args = tool.arg_names.len();
            while final_args.len() < max_args {
                final_args.push(PyValue::None);
            }

            // Map keyword arguments to their positions
            for kw in &call.keywords {
                if let Some(ref arg_name) = kw.arg {
                    let name = arg_name.as_str();
                    if let Some(pos) = tool.arg_names.iter().position(|n| n == name) {
                        let value = self.eval_expr(&kw.value)?;
                        if pos < final_args.len() {
                            final_args[pos] = value;
                        }
                    } else {
                        // Build rich diagnostic for unexpected argument
                        let kw_span = self.expr_span(&kw.value);
                        let signature = tool.arg_names.join(", ");
                        return Err(Error::Diagnostic(
                            Diagnostic::new(format!(
                                "`{}()` got an unexpected keyword argument `{}`",
                                func_name, name
                            ))
                            .with_source(&self.current_source)
                            .with_label(kw_span, "unexpected argument")
                            .with_note(format!("function signature: {}({})", func_name, signature))
                            .with_help(format!("valid arguments are: {}", tool.arg_names.join(", ")))
                        ));
                    }
                }
            }
        } else if !call.keywords.is_empty() {
            // Tool doesn't have arg names but keywords were used
            // Just append keyword values in order (fallback behavior)
            for kw in &call.keywords {
                let value = self.eval_expr(&kw.value)?;
                final_args.push(value);
            }
        }

        // Validate argument types if tool has info
        if let Some(ref info) = tool.info {
            for (i, (arg, arg_info)) in final_args.iter().zip(info.args.iter()).enumerate() {
                // Skip validation for optional arguments that are None
                if !arg_info.required && matches!(arg, PyValue::None) {
                    continue;
                }
                if let Some(err) = self.validate_arg_type(func_name, tool, call, i, arg, &arg_info.python_type) {
                    return Err(err);
                }
            }
        }

        Ok((tool.func)(final_args))
    }

    fn eval_constant(&self, constant: &Constant) -> Result<PyValue> {
        match constant {
            Constant::None => Ok(PyValue::None),
            Constant::Bool(b) => Ok(PyValue::Bool(*b)),
            Constant::Int(i) => {
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

    fn eval_list_comprehension(
        &mut self,
        elt: &Expr,
        generators: &[rustpython_parser::ast::Comprehension],
    ) -> Result<PyValue> {
        let mut results = Vec::new();
        self.eval_comprehension_recursive(elt, generators, 0, &mut results)?;
        Ok(PyValue::List(results))
    }

    fn eval_comprehension_recursive(
        &mut self,
        elt: &Expr,
        generators: &[rustpython_parser::ast::Comprehension],
        gen_index: usize,
        results: &mut Vec<PyValue>,
    ) -> Result<()> {
        if gen_index >= generators.len() {
            // All generators exhausted, evaluate the element expression
            let value = self.eval_expr(elt)?;
            results.push(value);
            return Ok(());
        }

        let generator = &generators[gen_index];

        // Check for async comprehension (not supported)
        if generator.is_async {
            return Err(Error::Unsupported("Async comprehensions".to_string()));
        }

        // Evaluate the iterable
        let iter_value = self.eval_expr(&generator.iter)?;
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

        // Iterate and apply filters
        for item in items {
            // Assign the loop variable
            self.assign_target(&generator.target, item)?;

            // Check all if conditions for this generator
            let mut pass_filters = true;
            for condition in &generator.ifs {
                let cond_value = self.eval_expr(condition)?;
                if !cond_value.is_truthy() {
                    pass_filters = false;
                    break;
                }
            }

            if pass_filters {
                // Recurse to next generator or evaluate element
                self.eval_comprehension_recursive(elt, generators, gen_index + 1, results)?;
            }
        }

        Ok(())
    }

    // === Diagnostic helpers ===

    /// Get span from an expression.
    fn expr_span(&self, expr: &Expr) -> Span {
        let range = expr.range();
        Span::new(range.start().to_usize(), range.end().to_usize())
    }

    /// Build a rich diagnostic for a type mismatch in a function call.
    fn build_type_mismatch_diagnostic(
        &self,
        func_name: &str,
        tool: &RegisteredTool,
        call: &rustpython_parser::ast::ExprCall,
        arg_index: usize,
        expected: &str,
        got: &str,
        actual_value: &PyValue,
    ) -> Diagnostic {
        let arg_spans: Vec<Span> = call.args.iter().map(|a| self.expr_span(a)).collect();

        let default_arg_name = format!("arg{}", arg_index);
        let arg_name = tool.arg_names.get(arg_index)
            .map(|s| s.as_str())
            .unwrap_or(&default_arg_name);

        let arg_span = arg_spans.get(arg_index).copied().unwrap_or_default();

        let signature = if let Some(ref info) = tool.info {
            info.args.iter()
                .map(|a| {
                    if a.required {
                        format!("{}: {}", a.name, a.python_type)
                    } else {
                        format!("{}?: {}", a.name, a.python_type)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            tool.arg_names.join(", ")
        };

        Diagnostic::new(format!("type mismatch in call to `{}`", func_name))
            .with_source(&self.current_source)
            .with_label(arg_span, format!("expected `{}`, found `{}`", expected, got))
            .with_note(format!(
                "parameter `{}` of `{}()` expects type `{}`",
                arg_name, func_name, expected
            ))
            .with_note(format!("function signature: {}({})", func_name, signature))
            .with_help(format!(
                "the value `{}` has type `{}`, but `{}` is required",
                actual_value.to_print_string(), got, expected
            ))
    }

    /// Validate argument type and return error diagnostic if mismatch.
    fn validate_arg_type(
        &self,
        func_name: &str,
        tool: &RegisteredTool,
        call: &rustpython_parser::ast::ExprCall,
        arg_index: usize,
        value: &PyValue,
        expected_type: &str,
    ) -> Option<Error> {
        let actual_type = value.type_name();

        let is_compatible = match expected_type {
            "any" => true,
            "str" => matches!(value, PyValue::Str(_)),
            "int" => matches!(value, PyValue::Int(_)),
            "float" => matches!(value, PyValue::Float(_) | PyValue::Int(_)),
            "bool" => matches!(value, PyValue::Bool(_)),
            "list" => matches!(value, PyValue::List(_)),
            "dict" => matches!(value, PyValue::Dict(_)),
            "number" => matches!(value, PyValue::Int(_) | PyValue::Float(_)),
            _ => true, // Unknown types pass through
        };

        if !is_compatible {
            Some(Error::Diagnostic(self.build_type_mismatch_diagnostic(
                func_name,
                tool,
                call,
                arg_index,
                expected_type,
                actual_type,
                value,
            )))
        } else {
            None
        }
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}
