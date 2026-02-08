//! Bytecode compiler: translates Python AST to bytecode instructions.
//!
//! This is the **only** module that imports `ruff_python_parser`. It walks the
//! AST once and produces a [`CodeObject`] that the VM can execute. All source
//! span information is captured during compilation so the VM can produce error
//! messages with accurate locations without ever touching the AST.

use ruff_python_ast::{self as ast, BoolOp, Expr, Stmt, UnaryOp};
use ruff_python_parser::parse_module;
use ruff_text_size::Ranged;

use crate::bytecode::{self, BinOp, CodeObject, FunctionDef, Op};
use crate::diagnostic::Span;
use crate::error::{Error, Result};
use crate::value::PyValue;

/// The set of method names that mutate a list in place.
const LIST_MUTATING_METHODS: &[&str] = &[
    "append", "extend", "pop", "clear", "insert", "remove", "reverse", "sort",
];

/// The set of method names that mutate a dict in place.
const DICT_MUTATING_METHODS: &[&str] = &["update", "setdefault", "pop", "clear"];

/// The set of method names that mutate a set in place.
const SET_MUTATING_METHODS: &[&str] = &["add", "discard", "remove", "clear", "update", "pop"];

/// Compiler state for tracking loops (used for break/continue resolution).
struct LoopContext {
    /// Instruction index of the loop start (target for `continue`).
    continue_target: u32,
    /// Placeholder instruction indices for `break` jumps that need patching.
    break_placeholders: Vec<usize>,
    /// Whether this is a `for` loop (needs `PopIter` before break).
    is_for_loop: bool,
}

/// Compiles Python source code into bytecode.
///
/// The compiler walks the AST exactly once and emits a flat instruction
/// stream. Jump targets use a placeholder/patch approach: jumps are emitted
/// with a dummy target of `0`, and once the real target is known the
/// instruction is patched in place.
pub struct Compiler {
    /// The code object being built.
    code: CodeObject,
    /// Stack of loop contexts for break/continue resolution.
    loop_stack: Vec<LoopContext>,
    /// Counter for generating unique comprehension temp variable names.
    comp_counter: usize,
}

impl Compiler {
    /// Compile Python source code into a [`CodeObject`].
    ///
    /// This is the main entry point. It parses the source, walks the AST,
    /// and produces a flat bytecode representation ready for the VM.
    pub fn compile(source: &str) -> Result<CodeObject> {
        let parsed = parse_module(source).map_err(|e| Error::Parse(e.to_string()))?;
        let module = parsed.into_syntax();

        let mut compiler = Compiler {
            code: CodeObject::new(source.to_string()),
            loop_stack: Vec::new(),
            comp_counter: 0,
        };

        let body_len = module.body.len();
        for (i, stmt) in module.body.iter().enumerate() {
            let is_last = i == body_len - 1;
            compiler.compile_stmt(stmt, is_last)?;
        }

        Ok(compiler.code)
    }

    // -----------------------------------------------------------------------
    // Helper methods for emitting instructions
    // -----------------------------------------------------------------------

    /// Emit an instruction with the given source span.
    fn emit(&mut self, op: Op, span: Span) {
        self.code.instructions.push(op);
        self.code.spans.push(span);
    }

    /// Emit a jump instruction with a placeholder target (0). Returns the
    /// index of the emitted instruction so it can be patched later.
    fn emit_jump(&mut self, make_op: fn(u32) -> Op, span: Span) -> usize {
        let idx = self.code.instructions.len();
        self.emit(make_op(0), span);
        idx
    }

    /// Patch a previously emitted jump instruction to point at `target`.
    fn patch_jump(&mut self, idx: usize, target: u32) {
        match &mut self.code.instructions[idx] {
            Op::Jump(t)
            | Op::PopJumpIfFalse(t)
            | Op::PopJumpIfTrue(t)
            | Op::JumpIfFalseOrPop(t)
            | Op::JumpIfTrueOrPop(t)
            | Op::ForIter(t) => *t = target,
            _ => panic!("patch_jump called on non-jump instruction"),
        }
    }

    /// Return the current instruction offset (next instruction index).
    fn current_offset(&self) -> u32 {
        self.code.instructions.len() as u32
    }

    /// Add a constant to the constant pool and return its index.
    /// Deduplicates identical constants.
    fn add_const(&mut self, value: PyValue) -> u32 {
        // Check for an existing identical constant
        for (i, existing) in self.code.constants.iter().enumerate() {
            if *existing == value {
                return i as u32;
            }
        }
        let idx = self.code.constants.len() as u32;
        self.code.constants.push(value);
        idx
    }

    /// Add a name to the name pool and return its index.
    /// Deduplicates identical names.
    fn add_name(&mut self, name: &str) -> u32 {
        if let Some(idx) = self.code.names.iter().position(|n| n == name) {
            return idx as u32;
        }
        let idx = self.code.names.len() as u32;
        self.code.names.push(name.to_string());
        idx
    }

    /// Get the source span of an AST expression.
    fn expr_span(&self, expr: &Expr) -> Span {
        let range = expr.range();
        Span::new(
            range.start().to_u32() as usize,
            range.end().to_u32() as usize,
        )
    }

    /// Get the source span of an AST statement.
    fn stmt_span(&self, stmt: &Stmt) -> Span {
        let range = stmt.range();
        Span::new(
            range.start().to_u32() as usize,
            range.end().to_u32() as usize,
        )
    }

    // -----------------------------------------------------------------------
    // Statement compilation
    // -----------------------------------------------------------------------

    /// Compile a single statement.
    ///
    /// `is_last` indicates whether this is the last statement in the module
    /// body. For the last `Stmt::Expr`, the result stays on the stack (it
    /// becomes the return value of `execute()`). For all others, the result
    /// is discarded with `Pop`.
    fn compile_stmt(&mut self, stmt: &Stmt, is_last: bool) -> Result<()> {
        match stmt {
            Stmt::Expr(expr_stmt) => {
                self.compile_expr(&expr_stmt.value)?;
                if !is_last {
                    self.emit(Op::Pop, self.stmt_span(stmt));
                }
            }

            Stmt::Assign(assign) => {
                let span = self.stmt_span(stmt);
                self.compile_expr(&assign.value)?;
                let n_targets = assign.targets.len();
                for (i, target) in assign.targets.iter().enumerate() {
                    if i < n_targets - 1 {
                        self.emit(Op::Dup, span);
                    }
                    self.compile_store_target(target)?;
                }
                if is_last {
                    let none_idx = self.add_const(PyValue::None);
                    self.emit(Op::LoadConst(none_idx), span);
                }
            }

            Stmt::AugAssign(aug) => {
                let span = self.stmt_span(stmt);
                // Load current value of target
                self.compile_expr(&aug.target)?;
                // Load the RHS
                self.compile_expr(&aug.value)?;
                // Apply the operator
                self.emit(Op::BinaryOp(translate_binop(&aug.op)), span);
                // Store back
                self.compile_store_target(&aug.target)?;
                if is_last {
                    let none_idx = self.add_const(PyValue::None);
                    self.emit(Op::LoadConst(none_idx), span);
                }
            }

            Stmt::If(if_stmt) => {
                let span = self.stmt_span(stmt);
                self.compile_if(if_stmt, span, is_last)?;
            }

            Stmt::While(while_stmt) => {
                let span = self.stmt_span(stmt);
                let loop_start = self.current_offset();

                self.loop_stack.push(LoopContext {
                    continue_target: loop_start,
                    break_placeholders: Vec::new(),
                    is_for_loop: false,
                });

                self.compile_expr(&while_stmt.test)?;
                let exit_jump = self.emit_jump(Op::PopJumpIfFalse, span);

                self.compile_body(&while_stmt.body, false)?;
                self.emit(Op::Jump(loop_start), span);

                let break_target = self.current_offset();
                self.patch_jump(exit_jump, break_target);

                // Patch all break placeholders
                let ctx = self.loop_stack.pop().unwrap();
                for placeholder in ctx.break_placeholders {
                    self.patch_jump(placeholder, break_target);
                }

                if is_last {
                    let none_idx = self.add_const(PyValue::None);
                    self.emit(Op::LoadConst(none_idx), span);
                }
            }

            Stmt::For(for_stmt) => {
                let span = self.stmt_span(stmt);

                // Evaluate the iterable and set up iteration
                self.compile_expr(&for_stmt.iter)?;
                self.emit(Op::GetIter, span);

                let loop_start = self.current_offset();
                self.loop_stack.push(LoopContext {
                    continue_target: loop_start,
                    break_placeholders: Vec::new(),
                    is_for_loop: true,
                });

                let exit_jump = self.emit_jump(Op::ForIter, span);

                // Assign the loop variable (handles tuple unpacking)
                self.compile_store_target(&for_stmt.target)?;

                self.compile_body(&for_stmt.body, false)?;
                self.emit(Op::Jump(loop_start), span);

                let break_target = self.current_offset();
                self.patch_jump(exit_jump, break_target);

                // Patch all break placeholders
                let ctx = self.loop_stack.pop().unwrap();
                for placeholder in ctx.break_placeholders {
                    self.patch_jump(placeholder, break_target);
                }

                if is_last {
                    let none_idx = self.add_const(PyValue::None);
                    self.emit(Op::LoadConst(none_idx), span);
                }
            }

            Stmt::Pass(_) => {
                let span = self.stmt_span(stmt);
                self.emit(Op::Nop, span);
                if is_last {
                    let none_idx = self.add_const(PyValue::None);
                    self.emit(Op::LoadConst(none_idx), span);
                }
            }

            Stmt::Break(_) => {
                let span = self.stmt_span(stmt);
                if self.loop_stack.is_empty() {
                    return Err(Error::Unsupported("'break' outside loop".to_string()));
                }
                // If inside a for-loop, clean up the iterator
                if self.loop_stack.last().unwrap().is_for_loop {
                    self.emit(Op::PopIter, span);
                }
                let placeholder = self.emit_jump(Op::Jump, span);
                self.loop_stack
                    .last_mut()
                    .unwrap()
                    .break_placeholders
                    .push(placeholder);
            }

            Stmt::Continue(_) => {
                let span = self.stmt_span(stmt);
                if self.loop_stack.is_empty() {
                    return Err(Error::Unsupported("'continue' outside loop".to_string()));
                }
                let target = self.loop_stack.last().unwrap().continue_target;
                self.emit(Op::Jump(target), span);
            }

            Stmt::Return(ret) => {
                let span = self.stmt_span(stmt);
                match &ret.value {
                    Some(expr) => self.compile_expr(expr)?,
                    None => {
                        let idx = self.add_const(PyValue::None);
                        self.emit(Op::LoadConst(idx), span);
                    }
                }
                self.emit(Op::ReturnValue, span);
            }

            Stmt::FunctionDef(func_def) => {
                let span = self.stmt_span(stmt);
                self.compile_function_def(func_def, span)?;
                if is_last {
                    let none_idx = self.add_const(PyValue::None);
                    self.emit(Op::LoadConst(none_idx), span);
                }
            }

            Stmt::Try(try_stmt) => {
                let span = self.stmt_span(stmt);
                self.compile_try(try_stmt, span, is_last)?;
            }

            Stmt::Raise(raise_stmt) => {
                let span = self.stmt_span(stmt);
                self.compile_raise(raise_stmt, span)?;
            }

            _ => {
                return Err(Error::Unsupported(format!(
                    "Statement type not supported: {:?}",
                    std::mem::discriminant(stmt)
                )));
            }
        }
        Ok(())
    }

    /// Compile a body (list of statements). If `is_last_in_module` is true,
    /// the last statement in the body may leave its result on the stack.
    fn compile_body(&mut self, body: &[Stmt], is_last_in_module: bool) -> Result<()> {
        let body_len = body.len();
        for (i, stmt) in body.iter().enumerate() {
            let is_last = is_last_in_module && i == body_len - 1;
            self.compile_stmt(stmt, is_last)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Assignment target compilation
    // -----------------------------------------------------------------------

    /// Compile an assignment target. Assumes the value to assign is on TOS.
    fn compile_store_target(&mut self, target: &Expr) -> Result<()> {
        match target {
            Expr::Name(name) => {
                let idx = self.add_name(name.id.as_str());
                self.emit(Op::StoreName(idx), self.expr_span(target));
            }

            Expr::Subscript(sub) => {
                // Stack has value on top. We need to push the index, then
                // StoreSubscript pops value + index and mutates the variable.
                let span = self.expr_span(target);
                if let Expr::Name(name) = sub.value.as_ref() {
                    let var_idx = self.add_name(name.id.as_str());
                    self.compile_expr(&sub.slice)?;
                    self.emit(Op::StoreSubscript(var_idx), span);
                } else {
                    return Err(Error::Unsupported(
                        "Subscript assignment only supported on named variables".to_string(),
                    ));
                }
            }

            Expr::Tuple(tuple) => {
                let span = self.expr_span(target);
                let n = tuple.elts.len() as u32;
                self.emit(Op::UnpackSequence(n), span);
                for elt in &tuple.elts {
                    self.compile_store_target(elt)?;
                }
            }

            Expr::List(list) => {
                let span = self.expr_span(target);
                let n = list.elts.len() as u32;
                self.emit(Op::UnpackSequence(n), span);
                for elt in &list.elts {
                    self.compile_store_target(elt)?;
                }
            }

            _ => {
                return Err(Error::Unsupported(
                    "Assignment target not supported".to_string(),
                ));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Expression compilation
    // -----------------------------------------------------------------------

    /// Compile an expression. The result is left on TOS.
    fn compile_expr(&mut self, expr: &Expr) -> Result<()> {
        let span = self.expr_span(expr);

        match expr {
            Expr::NoneLiteral(_) => {
                let idx = self.add_const(PyValue::None);
                self.emit(Op::LoadConst(idx), span);
            }

            Expr::BooleanLiteral(b) => {
                let idx = self.add_const(PyValue::Bool(b.value));
                self.emit(Op::LoadConst(idx), span);
            }

            Expr::NumberLiteral(n) => {
                let value = match &n.value {
                    ast::Number::Int(i) => {
                        let val: i64 = i
                            .as_i64()
                            .ok_or_else(|| Error::Runtime("Integer too large".to_string()))?;
                        PyValue::Int(val)
                    }
                    ast::Number::Float(f) => PyValue::Float(*f),
                    ast::Number::Complex { .. } => {
                        return Err(Error::Unsupported("Complex numbers".to_string()));
                    }
                };
                let idx = self.add_const(value);
                self.emit(Op::LoadConst(idx), span);
            }

            Expr::StringLiteral(s) => {
                let idx = self.add_const(PyValue::Str(s.value.to_string()));
                self.emit(Op::LoadConst(idx), span);
            }

            Expr::BytesLiteral(_) => {
                return Err(Error::Unsupported("Bytes literals".to_string()));
            }

            Expr::EllipsisLiteral(_) => {
                return Err(Error::Unsupported("Ellipsis".to_string()));
            }

            Expr::Name(name) => {
                let idx = self.add_name(name.id.as_str());
                self.emit(Op::LoadName(idx), span);
            }

            Expr::List(list) => {
                for elt in &list.elts {
                    self.compile_expr(elt)?;
                }
                self.emit(Op::BuildList(list.elts.len() as u32), span);
            }

            Expr::Tuple(tuple) => {
                for elt in &tuple.elts {
                    self.compile_expr(elt)?;
                }
                self.emit(Op::BuildTuple(tuple.elts.len() as u32), span);
            }

            Expr::Dict(dict) => {
                for item in &dict.items {
                    match &item.key {
                        Some(k) => self.compile_expr(k)?,
                        None => {
                            return Err(Error::Unsupported("Dict unpacking".to_string()));
                        }
                    }
                    self.compile_expr(&item.value)?;
                }
                self.emit(Op::BuildDict(dict.items.len() as u32), span);
            }

            Expr::Set(set) => {
                for elt in &set.elts {
                    self.compile_expr(elt)?;
                }
                self.emit(Op::BuildSet(set.elts.len() as u32), span);
            }

            Expr::BinOp(binop) => {
                self.compile_expr(&binop.left)?;
                self.compile_expr(&binop.right)?;
                self.emit(Op::BinaryOp(translate_binop(&binop.op)), span);
            }

            Expr::UnaryOp(unary) => {
                self.compile_expr(&unary.operand)?;
                let op = match unary.op {
                    UnaryOp::Not => bytecode::UnaryOp::Not,
                    UnaryOp::USub => bytecode::UnaryOp::Neg,
                    UnaryOp::UAdd => bytecode::UnaryOp::Pos,
                    UnaryOp::Invert => bytecode::UnaryOp::Invert,
                };
                self.emit(Op::UnaryOp(op), span);
            }

            Expr::BoolOp(boolop) => {
                self.compile_boolop(boolop, span)?;
            }

            Expr::Compare(cmp) => {
                self.compile_compare(cmp, span)?;
            }

            Expr::If(ifexp) => {
                self.compile_expr(&ifexp.test)?;
                let else_jump = self.emit_jump(Op::PopJumpIfFalse, span);
                self.compile_expr(&ifexp.body)?;
                let end_jump = self.emit_jump(Op::Jump, span);
                self.patch_jump(else_jump, self.current_offset());
                self.compile_expr(&ifexp.orelse)?;
                self.patch_jump(end_jump, self.current_offset());
            }

            Expr::Call(call) => {
                self.compile_call(call, span)?;
            }

            Expr::Subscript(sub) => {
                // Check if it's a slice expression
                if let Expr::Slice(slice_expr) = sub.slice.as_ref() {
                    self.compile_slice(&sub.value, slice_expr, span)?;
                } else {
                    self.compile_expr(&sub.value)?;
                    self.compile_expr(&sub.slice)?;
                    self.emit(Op::BinarySubscript, span);
                }
            }

            Expr::Attribute(attr) => {
                // Attribute access is mostly unsupported (methods are handled in Call)
                let value_type = if let Expr::Name(_) = attr.value.as_ref() {
                    "value"
                } else {
                    "expression"
                };
                return Err(Error::Unsupported(format!(
                    "Attribute access: {}.{} - use function call syntax for methods",
                    value_type, attr.attr
                )));
            }

            Expr::ListComp(listcomp) => {
                self.compile_list_comprehension(&listcomp.elt, &listcomp.generators, span)?;
            }

            Expr::Generator(genexp) => {
                // Treat generator expressions as eager list comprehensions
                self.compile_list_comprehension(&genexp.elt, &genexp.generators, span)?;
            }

            Expr::Lambda(lambda) => {
                self.compile_lambda(lambda, span)?;
            }

            Expr::FString(fstring) => {
                let mut n_parts = 0u32;
                for part in fstring.value.iter() {
                    match part {
                        ast::FStringPart::Literal(s) => {
                            let idx = self.add_const(PyValue::Str(s.value.to_string()));
                            self.emit(Op::LoadConst(idx), span);
                            self.emit(Op::FormatValue, span);
                            n_parts += 1;
                        }
                        ast::FStringPart::FString(fs) => {
                            for element in &fs.elements {
                                match element {
                                    ast::InterpolatedStringElement::Literal(lit) => {
                                        let idx =
                                            self.add_const(PyValue::Str(lit.value.to_string()));
                                        self.emit(Op::LoadConst(idx), span);
                                        self.emit(Op::FormatValue, span);
                                    }
                                    ast::InterpolatedStringElement::Interpolation(interp) => {
                                        self.compile_expr(&interp.expression)?;
                                        self.emit(Op::FormatValue, span);
                                    }
                                }
                                n_parts += 1;
                            }
                        }
                    }
                }
                self.emit(Op::BuildString(n_parts), span);
            }

            _ => {
                return Err(Error::Unsupported(format!(
                    "Expression type not supported: {:?}",
                    std::mem::discriminant(expr)
                )));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Specialized statement compilers
    // -----------------------------------------------------------------------

    /// Compile an if/elif/else statement.
    ///
    /// Ruff represents `if/elif/else` with `elif_else_clauses` rather than
    /// a nested `orelse` list. Each clause has an optional `test` (None = else).
    fn compile_if(&mut self, if_stmt: &ast::StmtIf, span: Span, is_last: bool) -> Result<()> {
        self.compile_expr(&if_stmt.test)?;
        let else_jump = self.emit_jump(Op::PopJumpIfFalse, span);

        self.compile_body(&if_stmt.body, is_last)?;

        let has_else = !if_stmt.elif_else_clauses.is_empty();
        if has_else {
            let end_jump = self.emit_jump(Op::Jump, span);
            self.patch_jump(else_jump, self.current_offset());

            // Compile elif/else clauses
            let mut clause_end_jumps = vec![end_jump];
            let n_clauses = if_stmt.elif_else_clauses.len();
            for (i, clause) in if_stmt.elif_else_clauses.iter().enumerate() {
                let is_last_clause = i == n_clauses - 1;
                if let Some(ref test) = clause.test {
                    // elif
                    self.compile_expr(test)?;
                    let next_jump = self.emit_jump(Op::PopJumpIfFalse, span);
                    self.compile_body(&clause.body, is_last)?;
                    if !is_last_clause {
                        let ej = self.emit_jump(Op::Jump, span);
                        clause_end_jumps.push(ej);
                    } else if is_last {
                        // Last elif with no else — need to push None for the
                        // false branch, same as an if without else
                        let ej = self.emit_jump(Op::Jump, span);
                        clause_end_jumps.push(ej);
                        self.patch_jump(next_jump, self.current_offset());
                        let none_idx = self.add_const(PyValue::None);
                        self.emit(Op::LoadConst(none_idx), span);
                        // Don't patch next_jump again below
                        for j in clause_end_jumps {
                            self.patch_jump(j, self.current_offset());
                        }
                        return Ok(());
                    }
                    self.patch_jump(next_jump, self.current_offset());
                } else {
                    // else
                    self.compile_body(&clause.body, is_last)?;
                }
            }

            let end = self.current_offset();
            for j in clause_end_jumps {
                self.patch_jump(j, end);
            }
        } else if is_last {
            let end_jump = self.emit_jump(Op::Jump, span);
            self.patch_jump(else_jump, self.current_offset());
            let none_idx = self.add_const(PyValue::None);
            self.emit(Op::LoadConst(none_idx), span);
            self.patch_jump(end_jump, self.current_offset());
        } else {
            self.patch_jump(else_jump, self.current_offset());
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Specialized expression compilers
    // -----------------------------------------------------------------------

    /// Compile a short-circuit boolean operation (`and` / `or`).
    ///
    /// `and` returns the first falsy value (or the last value if all truthy).
    /// `or` returns the first truthy value (or the last value if all falsy).
    /// Both short-circuit: they stop evaluating as soon as the outcome is known.
    fn compile_boolop(&mut self, boolop: &ast::ExprBoolOp, span: Span) -> Result<()> {
        let jump_op: fn(u32) -> Op = match boolop.op {
            BoolOp::And => Op::JumpIfFalseOrPop,
            BoolOp::Or => Op::JumpIfTrueOrPop,
        };

        let mut jump_placeholders = Vec::new();

        // Compile all values except the last with conditional jumps
        for value in &boolop.values[..boolop.values.len() - 1] {
            self.compile_expr(value)?;
            let placeholder = self.emit_jump(jump_op, span);
            jump_placeholders.push(placeholder);
        }

        // Compile the last value (no jump needed — it's the final result)
        self.compile_expr(boolop.values.last().unwrap())?;

        // All jumps target here (after the last value)
        let end = self.current_offset();
        for placeholder in jump_placeholders {
            self.patch_jump(placeholder, end);
        }

        Ok(())
    }

    /// Compile a comparison expression, handling chained comparisons.
    ///
    /// Single comparison (`a < b`): straightforward compare.
    /// Chained comparison (`a < b < c`): uses `Dup` and `RotN` to save
    /// intermediate values, with short-circuit jumps on failure.
    fn compile_compare(&mut self, cmp: &ast::ExprCompare, span: Span) -> Result<()> {
        let n_ops = cmp.ops.len();

        self.compile_expr(&cmp.left)?;

        if n_ops == 1 {
            // Simple case: a op b
            self.compile_expr(&cmp.comparators[0])?;
            self.emit(Op::CompareOp(translate_cmpop(&cmp.ops[0])), span);
            return Ok(());
        }

        // Chained: a op1 b op2 c ...
        let mut fail_placeholders = Vec::new();

        for i in 0..n_ops - 1 {
            self.compile_expr(&cmp.comparators[i])?;
            // Save the comparator for the next comparison
            self.emit(Op::Dup, span);
            self.emit(Op::RotN(3), span);
            self.emit(Op::CompareOp(translate_cmpop(&cmp.ops[i])), span);
            let fail = self.emit_jump(Op::PopJumpIfFalse, span);
            fail_placeholders.push(fail);
        }

        // Last comparison (no save needed)
        self.compile_expr(&cmp.comparators[n_ops - 1])?;
        self.emit(Op::CompareOp(translate_cmpop(&cmp.ops[n_ops - 1])), span);
        let end_jump = self.emit_jump(Op::Jump, span);

        // Failure path: pop the saved intermediate and push False
        let fail_target = self.current_offset();
        for placeholder in &fail_placeholders {
            self.patch_jump(*placeholder, fail_target);
        }
        // Pop the saved intermediate value from the stack
        self.emit(Op::Pop, span);
        let false_idx = self.add_const(PyValue::Bool(false));
        self.emit(Op::LoadConst(false_idx), span);

        self.patch_jump(end_jump, self.current_offset());

        Ok(())
    }

    /// Compile a function/method/builtin call.
    fn compile_call(&mut self, call: &ast::ExprCall, span: Span) -> Result<()> {
        // Check if this is a method call (object.method(args))
        if let Expr::Attribute(attr) = call.func.as_ref() {
            return self.compile_method_call(attr, call, span);
        }

        // Named function call — use CallFunction/CallFunctionKw (by-name dispatch)
        if let Expr::Name(name) = call.func.as_ref() {
            let func_name = name.id.to_string();
            let name_idx = self.add_name(&func_name);

            // Compile positional arguments
            for arg in &call.arguments.args {
                self.compile_expr(arg)?;
            }

            if call.arguments.keywords.is_empty() {
                self.emit(
                    Op::CallFunction(name_idx, call.arguments.args.len() as u32),
                    span,
                );
            } else {
                // Compile keyword arguments: push name string then value
                for kw in &call.arguments.keywords {
                    if let Some(ref arg_name) = kw.arg {
                        let kw_name_idx =
                            self.add_const(PyValue::Str(arg_name.as_str().to_string()));
                        self.emit(Op::LoadConst(kw_name_idx), span);
                        self.compile_expr(&kw.value)?;
                    }
                }
                self.emit(
                    Op::CallFunctionKw(
                        name_idx,
                        call.arguments.args.len() as u32,
                        call.arguments.keywords.len() as u32,
                    ),
                    span,
                );
            }

            return Ok(());
        }

        // Non-name call (e.g. lambda call, variable holding function)
        // Compile the callable expression
        self.compile_expr(&call.func)?;

        // Compile positional arguments
        for arg in &call.arguments.args {
            self.compile_expr(arg)?;
        }

        if call.arguments.keywords.is_empty() {
            self.emit(Op::CallValue(call.arguments.args.len() as u32), span);
        } else {
            // Compile keyword arguments: push name string then value
            for kw in &call.arguments.keywords {
                if let Some(ref arg_name) = kw.arg {
                    let kw_name_idx = self.add_const(PyValue::Str(arg_name.as_str().to_string()));
                    self.emit(Op::LoadConst(kw_name_idx), span);
                    self.compile_expr(&kw.value)?;
                }
            }
            self.emit(
                Op::CallValueKw(
                    call.arguments.args.len() as u32,
                    call.arguments.keywords.len() as u32,
                ),
                span,
            );
        }

        Ok(())
    }

    /// Compile a method call (e.g., `list.append(x)` or `str.lower()`).
    fn compile_method_call(
        &mut self,
        attr: &ast::ExprAttribute,
        call: &ast::ExprCall,
        span: Span,
    ) -> Result<()> {
        let method_name = attr.attr.as_str();
        let method_idx = self.add_name(method_name);

        // Check if this is a mutating method on a named variable
        if let Expr::Name(name) = attr.value.as_ref() {
            let is_list_mut = LIST_MUTATING_METHODS.contains(&method_name);
            let is_dict_mut = DICT_MUTATING_METHODS.contains(&method_name);
            let is_set_mut = SET_MUTATING_METHODS.contains(&method_name);

            if is_list_mut || is_dict_mut || is_set_mut {
                let var_idx = self.add_name(name.id.as_str());
                // Compile arguments
                for arg in &call.arguments.args {
                    self.compile_expr(arg)?;
                }
                self.emit(
                    Op::CallMutMethod(var_idx, method_idx, call.arguments.args.len() as u32),
                    span,
                );
                return Ok(());
            }
        }

        // Non-mutating method: push object, then args, then call
        self.compile_expr(&attr.value)?;
        for arg in &call.arguments.args {
            self.compile_expr(arg)?;
        }
        self.emit(
            Op::CallMethod(method_idx, call.arguments.args.len() as u32),
            span,
        );

        Ok(())
    }

    /// Compile a slice expression (`list[start:stop:step]`).
    fn compile_slice(&mut self, value: &Expr, slice: &ast::ExprSlice, span: Span) -> Result<()> {
        self.compile_expr(value)?;

        // Push start, stop, step (None if absent)
        match &slice.lower {
            Some(expr) => self.compile_expr(expr)?,
            None => {
                let idx = self.add_const(PyValue::None);
                self.emit(Op::LoadConst(idx), span);
            }
        }
        match &slice.upper {
            Some(expr) => self.compile_expr(expr)?,
            None => {
                let idx = self.add_const(PyValue::None);
                self.emit(Op::LoadConst(idx), span);
            }
        }
        match &slice.step {
            Some(expr) => self.compile_expr(expr)?,
            None => {
                let idx = self.add_const(PyValue::None);
                self.emit(Op::LoadConst(idx), span);
            }
        }

        self.emit(Op::Slice, span);
        Ok(())
    }

    /// Compile a list comprehension or generator expression.
    ///
    /// Uses a synthetic temp variable (`__comp_N`) to accumulate results,
    /// with nested for-loops and filter conditions compiled inline.
    fn compile_list_comprehension(
        &mut self,
        elt: &Expr,
        generators: &[ast::Comprehension],
        span: Span,
    ) -> Result<()> {
        // Create a unique temp variable for the result
        let comp_var = format!("__comp_{}", self.comp_counter);
        self.comp_counter += 1;
        let comp_var_idx = self.add_name(&comp_var);

        // Initialize empty result list
        self.emit(Op::BuildList(0), span);
        self.emit(Op::StoreName(comp_var_idx), span);

        // Compile the generators recursively
        self.compile_comprehension_generators(elt, generators, 0, comp_var_idx, span)?;

        // Load the result
        self.emit(Op::LoadName(comp_var_idx), span);

        Ok(())
    }

    /// Recursively compile comprehension generators.
    fn compile_comprehension_generators(
        &mut self,
        elt: &Expr,
        generators: &[ast::Comprehension],
        gen_index: usize,
        comp_var_idx: u32,
        span: Span,
    ) -> Result<()> {
        let generator = &generators[gen_index];

        if generator.is_async {
            return Err(Error::Unsupported("Async comprehensions".to_string()));
        }

        // Compile the iterable and set up iteration
        self.compile_expr(&generator.iter)?;
        self.emit(Op::GetIter, span);

        let loop_start = self.current_offset();
        let exit_jump = self.emit_jump(Op::ForIter, span);

        // Assign the loop variable
        self.compile_store_target(&generator.target)?;

        // Compile filter conditions
        let mut skip_jumps = Vec::new();
        for condition in &generator.ifs {
            self.compile_expr(condition)?;
            let skip = self.emit_jump(Op::PopJumpIfFalse, span);
            skip_jumps.push(skip);
        }

        if gen_index + 1 < generators.len() {
            // Recurse for nested generators
            self.compile_comprehension_generators(
                elt,
                generators,
                gen_index + 1,
                comp_var_idx,
                span,
            )?;
        } else {
            // Innermost generator: evaluate element and append to result
            let append_idx = self.add_name("append");
            self.compile_expr(elt)?;
            self.emit(Op::CallMutMethod(comp_var_idx, append_idx, 1), span);
            self.emit(Op::Pop, span); // discard None from append
        }

        // Skip targets: jump back to loop start
        for skip in &skip_jumps {
            self.patch_jump(*skip, self.current_offset());
        }

        self.emit(Op::Jump(loop_start), span);

        // Exit loop
        self.patch_jump(exit_jump, self.current_offset());

        Ok(())
    }

    /// Compile a try/except statement.
    ///
    /// Emits the try body, records exception table entries pointing to
    /// handlers, and emits each handler with `CheckExcMatch` / `PopException`.
    ///
    /// Layout:
    /// ```text
    /// try_start:
    ///     <try body>
    ///     Jump → else_or_end
    /// handler_0:
    ///     CheckExcMatch (if typed)
    ///     PopJumpIfFalse → handler_1  (if typed)
    ///     <handler body>
    ///     PopException
    ///     Jump → end
    /// handler_1:
    ///     ...
    /// else_or_end:
    ///     <else body if any>
    /// end:
    /// ```
    fn compile_try(&mut self, try_stmt: &ast::StmtTry, span: Span, is_last: bool) -> Result<()> {
        use crate::bytecode::ExceptionEntry;

        if !try_stmt.finalbody.is_empty() {
            return Err(Error::Unsupported(
                "try/finally is not yet supported".to_string(),
            ));
        }

        // Record the start of the try body
        let try_start = self.current_offset();

        // Compile the try body
        self.compile_body(&try_stmt.body, is_last)?;

        // Jump past all handlers (to else or end)
        let try_end_jump = self.emit_jump(Op::Jump, span);

        // The try body ends here (exclusive)
        let try_end = self.current_offset();

        // Compile each except handler
        //
        // All handlers share a single exception table entry pointing to
        // the first handler. The handlers are chained: each typed handler
        // emits `CheckExcMatch` + `PopJumpIfFalse` jumping to the next.
        // After the last typed handler, a `Reraise` ensures unmatched
        // exceptions propagate.
        let mut handler_end_jumps = Vec::new();
        let mut has_bare_except = false;

        let first_handler_offset = self.current_offset();

        for (i, handler) in try_stmt.handlers.iter().enumerate() {
            let ast::ExceptHandler::ExceptHandler(h) = handler;
            let _ = i; // suppress unused warning

            if let Some(ref type_expr) = h.type_ {
                let type_name = match type_expr.as_ref() {
                    Expr::Name(name) => name.id.to_string(),
                    _ => {
                        return Err(Error::Unsupported(
                            "Only named exception types are supported".to_string(),
                        ));
                    }
                };

                let type_name_idx = self.add_const(PyValue::Str(type_name));
                self.emit(Op::LoadConst(type_name_idx), span);
                self.emit(Op::CheckExcMatch, span);

                // If no match, jump to next handler (or reraise)
                let no_match_jump = self.emit_jump(Op::PopJumpIfFalse, span);

                // Optionally bind exception to variable
                if let Some(ref name) = h.name {
                    // The exception message is already stored by handle_exception
                    // via var_name in the exception entry. But since we use a
                    // single entry, we need to store it here too.
                    // Actually, the `as e` binding is handled differently now:
                    // We load the exc message from the exception stack.
                    // For simplicity, we'll handle this via a special approach:
                    // nothing extra needed — handle_exception already stores it.
                    let _ = name;
                }

                self.compile_body(&h.body, is_last)?;
                self.emit(Op::PopException, span);
                let end_jump = self.emit_jump(Op::Jump, span);
                handler_end_jumps.push(end_jump);

                // Patch no-match jump to next handler location
                self.patch_jump(no_match_jump, self.current_offset());
            } else {
                // Bare except: catches everything
                has_bare_except = true;
                self.compile_body(&h.body, is_last)?;
                self.emit(Op::PopException, span);
                let end_jump = self.emit_jump(Op::Jump, span);
                handler_end_jumps.push(end_jump);
            }
        }

        // If all handlers are typed and none matched, re-raise
        if !has_bare_except && !try_stmt.handlers.is_empty() {
            self.emit(Op::Reraise, span);
        }

        // Build the exception table entry — single entry pointing to first handler.
        // The `as e` variable binding is handled per-handler, but we use the
        // first handler's `as` binding in the exception entry for the initial store.
        let first_var_name = try_stmt.handlers.first().and_then(|h| {
            let ast::ExceptHandler::ExceptHandler(handler) = h;
            handler
                .name
                .as_ref()
                .map(|name| self.add_name(name.as_str()))
        });

        self.code.exception_table.push(ExceptionEntry {
            start: try_start,
            end: try_end,
            handler: first_handler_offset,
            var_name: first_var_name,
        });

        // Patch the try-end jump to the else body (or end)
        self.patch_jump(try_end_jump, self.current_offset());

        // Compile else body if present
        if !try_stmt.orelse.is_empty() {
            self.compile_body(&try_stmt.orelse, is_last)?;
        }

        // Patch all handler-end jumps to here
        let end = self.current_offset();
        for jump in handler_end_jumps {
            self.patch_jump(jump, end);
        }

        Ok(())
    }

    /// Compile a raise statement.
    ///
    /// - `raise ExceptionType("message")` — calls the constructor and raises
    /// - `raise ExceptionType` — raises with the type name as the message
    /// - `raise` (bare) — re-raises the current exception
    fn compile_raise(&mut self, raise_stmt: &ast::StmtRaise, span: Span) -> Result<()> {
        if raise_stmt.cause.is_some() {
            return Err(Error::Unsupported(
                "Exception chaining (raise ... from ...) is not supported".to_string(),
            ));
        }

        match &raise_stmt.exc {
            None => {
                // Bare `raise` — re-raise current exception
                self.emit(Op::Reraise, span);
            }
            Some(expr) => {
                // Check if it's a call like `ValueError("message")`
                if let Expr::Call(call) = expr.as_ref()
                    && let Expr::Name(name) = call.func.as_ref()
                {
                    let type_name = name.id.to_string();
                    let type_name_idx = self.add_const(PyValue::Str(type_name));
                    self.emit(Op::LoadConst(type_name_idx), span);

                    // Compile the first argument as the message
                    if let Some(arg) = call.arguments.args.first() {
                        self.compile_expr(arg)?;
                    } else {
                        let none_idx = self.add_const(PyValue::None);
                        self.emit(Op::LoadConst(none_idx), span);
                    }

                    self.emit(Op::Raise, span);
                    return Ok(());
                }

                // `raise ExceptionType` (without call)
                if let Expr::Name(name) = expr.as_ref() {
                    let type_name = name.id.to_string();
                    let type_idx = self.add_const(PyValue::Str(type_name));
                    self.emit(Op::LoadConst(type_idx), span);
                    let none_idx = self.add_const(PyValue::None);
                    self.emit(Op::LoadConst(none_idx), span);
                    self.emit(Op::Raise, span);
                    return Ok(());
                }

                return Err(Error::Unsupported(
                    "Only `raise ExceptionType(...)` is supported".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Compile a function definition (`def name(params): body`).
    ///
    /// Handles positional parameters with optional default values.
    /// Default values are evaluated as constant expressions at definition time.
    fn compile_function_def(&mut self, func_def: &ast::StmtFunctionDef, span: Span) -> Result<()> {
        let name = func_def.name.to_string();
        let params: Vec<String> = func_def
            .parameters
            .args
            .iter()
            .map(|a| a.parameter.name.to_string())
            .collect();

        // Collect default values from the trailing parameters that have them.
        let mut defaults = Vec::new();
        for arg in &func_def.parameters.args {
            if let Some(ref default_expr) = arg.default {
                let val = eval_const_expr(default_expr)?;
                defaults.push(val);
            }
        }

        // Read *args and **kwargs parameter names
        let vararg = func_def
            .parameters
            .vararg
            .as_ref()
            .map(|v| v.name.to_string());
        let kwarg = func_def
            .parameters
            .kwarg
            .as_ref()
            .map(|v| v.name.to_string());

        // Compile the function body into a separate CodeObject
        let mut sub_compiler = Compiler {
            code: CodeObject::new(self.code.source.clone()),
            loop_stack: Vec::new(),
            comp_counter: self.comp_counter,
        };

        let body_len = func_def.body.len();
        for (i, stmt) in func_def.body.iter().enumerate() {
            let is_last = i == body_len - 1;
            sub_compiler.compile_stmt(stmt, is_last)?;
        }

        // Ensure the function ends with a ReturnValue.
        // If the last instruction already is ReturnValue, skip this.
        let needs_implicit_return = sub_compiler
            .code
            .instructions
            .last()
            .is_none_or(|op| !matches!(op, Op::ReturnValue));

        if needs_implicit_return {
            let none_idx = sub_compiler.add_const(PyValue::None);

            if sub_compiler.code.instructions.is_empty() {
                sub_compiler.emit(Op::LoadConst(none_idx), span);
            }
            sub_compiler.emit(Op::ReturnValue, span);
        }

        // Propagate the comprehension counter
        self.comp_counter = sub_compiler.comp_counter;

        let func_idx = self.code.functions.len() as u32;
        self.code.functions.push(FunctionDef {
            name: name.clone(),
            params,
            defaults,
            vararg,
            kwarg,
            code: sub_compiler.code,
        });

        self.emit(Op::MakeFunction(func_idx), span);
        let name_idx = self.add_name(&name);
        self.emit(Op::StoreName(name_idx), span);

        Ok(())
    }

    /// Compile a lambda expression (`lambda params: body`).
    ///
    /// Similar to `compile_function_def` but the body is a single expression
    /// (not a block of statements), and the result is left on the stack
    /// (no `StoreName`).
    fn compile_lambda(&mut self, lambda: &ast::ExprLambda, span: Span) -> Result<()> {
        // Extract parameters (None means zero-param lambda)
        let (params, defaults, vararg, kwarg) = if let Some(ref parameters) = lambda.parameters {
            let params: Vec<String> = parameters
                .args
                .iter()
                .map(|a| a.parameter.name.to_string())
                .collect();

            let mut defaults = Vec::new();
            for arg in &parameters.args {
                if let Some(ref default_expr) = arg.default {
                    let val = eval_const_expr(default_expr)?;
                    defaults.push(val);
                }
            }

            let vararg = parameters.vararg.as_ref().map(|v| v.name.to_string());
            let kwarg = parameters.kwarg.as_ref().map(|v| v.name.to_string());

            (params, defaults, vararg, kwarg)
        } else {
            (Vec::new(), Vec::new(), None, None)
        };

        // Compile the lambda body into a separate CodeObject
        let mut sub_compiler = Compiler {
            code: CodeObject::new(self.code.source.clone()),
            loop_stack: Vec::new(),
            comp_counter: self.comp_counter,
        };

        sub_compiler.compile_expr(&lambda.body)?;
        sub_compiler.emit(Op::ReturnValue, span);

        // Propagate the comprehension counter
        self.comp_counter = sub_compiler.comp_counter;

        let func_idx = self.code.functions.len() as u32;
        self.code.functions.push(FunctionDef {
            name: "<lambda>".to_string(),
            params,
            defaults,
            vararg,
            kwarg,
            code: sub_compiler.code,
        });

        // Leave the function value on the stack (no StoreName for lambdas)
        self.emit(Op::MakeFunction(func_idx), span);

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Standalone helpers (no &self)
// ---------------------------------------------------------------------------

/// Evaluate a constant expression at compile time (for default parameter values).
///
/// Supports: literals (`42`, `"hello"`, `True`, `None`), unary minus (`-1`),
/// empty collections (`[]`, `{}`), and tuples of constants.
fn eval_const_expr(expr: &Expr) -> Result<PyValue> {
    match expr {
        Expr::NoneLiteral(_) => Ok(PyValue::None),
        Expr::BooleanLiteral(b) => Ok(PyValue::Bool(b.value)),
        Expr::NumberLiteral(n) => match &n.value {
            ast::Number::Int(i) => {
                let val: i64 = i
                    .as_i64()
                    .ok_or_else(|| Error::Runtime("Integer too large".to_string()))?;
                Ok(PyValue::Int(val))
            }
            ast::Number::Float(f) => Ok(PyValue::Float(*f)),
            ast::Number::Complex { .. } => Err(Error::Unsupported("Complex numbers".to_string())),
        },
        Expr::StringLiteral(s) => Ok(PyValue::Str(s.value.to_string())),
        Expr::UnaryOp(u) => {
            let operand = eval_const_expr(&u.operand)?;
            match u.op {
                ast::UnaryOp::USub => match operand {
                    PyValue::Int(i) => Ok(PyValue::Int(-i)),
                    PyValue::Float(f) => Ok(PyValue::Float(-f)),
                    _ => Err(Error::Runtime(
                        "Unary minus on non-numeric default value".to_string(),
                    )),
                },
                ast::UnaryOp::UAdd => Ok(operand),
                ast::UnaryOp::Not => Ok(PyValue::Bool(!operand.is_truthy())),
                _ => Err(Error::Unsupported(
                    "Complex default value expression".to_string(),
                )),
            }
        }
        Expr::Name(name) => match name.id.as_str() {
            "True" => Ok(PyValue::Bool(true)),
            "False" => Ok(PyValue::Bool(false)),
            "None" => Ok(PyValue::None),
            _ => Err(Error::Unsupported(
                "Non-constant default parameter value".to_string(),
            )),
        },
        Expr::List(l) if l.elts.is_empty() => Ok(PyValue::List(Vec::new())),
        Expr::Dict(d) if d.items.is_empty() => Ok(PyValue::Dict(Vec::new())),
        Expr::Tuple(t) => {
            let items: Result<Vec<PyValue>> = t.elts.iter().map(eval_const_expr).collect();
            Ok(PyValue::Tuple(items?))
        }
        _ => Err(Error::Unsupported(
            "Non-constant default parameter value".to_string(),
        )),
    }
}

/// Translate a binary operator to our bytecode enum.
fn translate_binop(op: &ast::Operator) -> BinOp {
    match op {
        ast::Operator::Add => BinOp::Add,
        ast::Operator::Sub => BinOp::Sub,
        ast::Operator::Mult => BinOp::Mult,
        ast::Operator::Div => BinOp::Div,
        ast::Operator::FloorDiv => BinOp::FloorDiv,
        ast::Operator::Mod => BinOp::Mod,
        ast::Operator::Pow => BinOp::Pow,
        ast::Operator::BitOr => BinOp::BitOr,
        ast::Operator::BitXor => BinOp::BitXor,
        ast::Operator::BitAnd => BinOp::BitAnd,
        ast::Operator::LShift => BinOp::LShift,
        ast::Operator::RShift => BinOp::RShift,
        _ => BinOp::Add, // unreachable in practice
    }
}

/// Translate a comparison operator to our bytecode enum.
fn translate_cmpop(op: &ast::CmpOp) -> bytecode::CmpOp {
    match op {
        ast::CmpOp::Eq => bytecode::CmpOp::Eq,
        ast::CmpOp::NotEq => bytecode::CmpOp::NotEq,
        ast::CmpOp::Lt => bytecode::CmpOp::Lt,
        ast::CmpOp::LtE => bytecode::CmpOp::LtE,
        ast::CmpOp::Gt => bytecode::CmpOp::Gt,
        ast::CmpOp::GtE => bytecode::CmpOp::GtE,
        ast::CmpOp::In => bytecode::CmpOp::In,
        ast::CmpOp::NotIn => bytecode::CmpOp::NotIn,
        ast::CmpOp::Is => bytecode::CmpOp::Is,
        ast::CmpOp::IsNot => bytecode::CmpOp::IsNot,
    }
}
