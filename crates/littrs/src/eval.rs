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
use crate::operators::{apply_binop, apply_cmpop};
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
