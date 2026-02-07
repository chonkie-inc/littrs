//! Bytecode instruction set and compiled code representation.
//!
//! This module defines the bytecodes that the compiler produces and the VM
//! executes. It also defines our own operator enums (replacing the ones from
//! rustpython_parser) so that only the compiler depends on the parser crate.

use crate::diagnostic::Span;
use crate::value::PyValue;

// ---------------------------------------------------------------------------
// Operator enums
// ---------------------------------------------------------------------------

/// Binary operators for arithmetic and bitwise operations.
///
/// These map 1:1 to Python's binary operators. The VM delegates the actual
/// computation to [`crate::operators::apply_binop`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mult,
    Div,
    FloorDiv,
    Mod,
    Pow,
    BitOr,
    BitXor,
    BitAnd,
    LShift,
    RShift,
}

/// Comparison operators.
///
/// Supports Python's full set of comparisons including chained comparisons
/// (`a < b < c`) which the compiler breaks into individual compare-and-jump
/// sequences. The VM delegates to [`crate::operators::apply_cmpop`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    In,
    NotIn,
    Is,
    IsNot,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Boolean negation: `not x`
    Not,
    /// Arithmetic negation: `-x`
    Neg,
    /// Unary plus: `+x` (identity for numbers)
    Pos,
    /// Bitwise invert: `~x`
    Invert,
}

// ---------------------------------------------------------------------------
// Bytecode instructions
// ---------------------------------------------------------------------------

/// A single bytecode instruction.
///
/// The VM interprets these using a simple `loop { match op { ... } }` dispatch.
/// All index arguments (`u32`) refer to entries in the corresponding pool of the
/// [`CodeObject`] that contains this instruction (constants, names, or functions).
#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum Op {
    // --- Stack manipulation ---
    /// Push `constants[i]` onto the stack.
    LoadConst(u32),

    /// Discard the top-of-stack value.
    Pop,

    /// Duplicate the top-of-stack value.
    Dup,

    /// Rotate the top N stack items so that TOS moves to position N.
    ///
    /// Used by chained comparisons to save intermediate values. For example,
    /// `RotN(3)` with stack `[a, b, c]` (c = TOS) produces `[c, a, b]`.
    RotN(u8),

    // --- Variables ---
    /// Push the value of variable `names[i]` onto the stack.
    ///
    /// Lookup order: frame locals → globals. Produces a `NameError` if not found.
    LoadName(u32),

    /// Pop TOS and store it into variable `names[i]`.
    ///
    /// At the top level, variables go into globals (persistent across calls).
    /// Inside a function body, they go into the frame's locals.
    StoreName(u32),

    // --- Operators ---
    /// Pop two values (right then left), apply the binary operator, push result.
    BinaryOp(BinOp),

    /// Pop one value, apply the unary operator, push result.
    UnaryOp(UnaryOp),

    /// Pop two values (right then left), compare, push a `Bool` result.
    CompareOp(CmpOp),

    // --- Short-circuit boolean operations ---
    /// Short-circuit AND helper.
    ///
    /// If TOS is falsy, jump to target (leaving TOS on the stack — it is the
    /// result). If TOS is truthy, pop it and continue to the next operand.
    /// This preserves Python's `and` semantics where `0 and 5` returns `0`.
    JumpIfFalseOrPop(u32),

    /// Short-circuit OR helper.
    ///
    /// If TOS is truthy, jump to target (leaving TOS on the stack — it is the
    /// result). If TOS is falsy, pop it and continue to the next operand.
    /// This preserves Python's `or` semantics where `0 or 5` returns `5`.
    JumpIfTrueOrPop(u32),

    // --- Control flow ---
    /// Unconditional jump to instruction index.
    Jump(u32),

    /// Pop TOS. If truthy, jump to target.
    #[allow(dead_code)]
    PopJumpIfTrue(u32),

    /// Pop TOS. If falsy, jump to target.
    PopJumpIfFalse(u32),

    // --- Collection constructors ---
    /// Pop N items from the stack, build a `List`, push it.
    ///
    /// Items are popped in reverse order so that the first pushed item becomes
    /// index 0 of the list.
    BuildList(u32),

    /// Pop 2*N items from the stack (alternating key, value), build a `Dict`.
    ///
    /// Keys must be strings. The pairs are ordered as pushed.
    BuildDict(u32),

    // --- Subscript ---
    /// Pop index and collection, push `collection[index]`.
    ///
    /// Supports list (with negative indices), string, and dict subscript.
    BinarySubscript,

    /// Pop value and index from the stack, mutate `names[i][index] = value`.
    ///
    /// The variable is looked up by name and modified in place.
    StoreSubscript(u32),

    // --- Slicing ---
    /// Pop step, stop, start, and object from the stack. Push `object[start:stop:step]`.
    ///
    /// Any of start/stop/step may be `PyValue::None` to indicate "unspecified".
    Slice,

    // --- Unpacking ---
    /// Pop TOS (must be a list), push its N elements onto the stack.
    ///
    /// Elements are pushed so that element[0] is TOS (and element[N-1] is deepest).
    /// This lets the compiler emit `StoreName` calls in forward target order.
    UnpackSequence(u32),

    // --- Iteration ---
    /// Pop TOS, convert it to an iterator, and store it in the current frame.
    ///
    /// Lists are used directly; strings are split into characters. The iterator
    /// state is kept in `CallFrame::iterators`, not on the value stack.
    GetIter,

    /// Advance the current frame's topmost iterator.
    ///
    /// If the iterator has more items, push the next item onto the value stack.
    /// If exhausted, pop the iterator from the frame and jump to target.
    ForIter(u32),

    /// Discard the current frame's topmost iterator without checking exhaustion.
    ///
    /// Used before `break` inside a `for` loop to clean up the iterator.
    PopIter,

    // --- Function calls ---
    /// Call a function by name with positional arguments.
    ///
    /// `names[name_idx]` is the function name. Pop `n_args` values from the
    /// stack (right-to-left) as arguments. Dispatch order: builtins → tools →
    /// user-defined functions. Push the return value.
    CallFunction(u32, u32),

    /// Call a function by name with positional and keyword arguments.
    ///
    /// Stack layout (bottom to top):
    /// `[positional_args..., kw_name_0, kw_val_0, kw_name_1, kw_val_1, ...]`
    ///
    /// `n_positional` positional args + `n_keyword` keyword pairs (2 values each).
    /// Keyword names are `PyValue::Str` constants.
    CallFunctionKw(u32, u32, u32),

    /// Call a non-mutating method on the object at TOS.
    ///
    /// Stack layout: `[args..., object]`. Pops object and args, dispatches to
    /// the appropriate method handler, pushes result.
    CallMethod(u32, u32),

    /// Call a mutating method on a named variable.
    ///
    /// Stack layout: `[args...]`. Looks up `names[var_name_idx]`, gets a `&mut`
    /// reference, and calls `mutate_list` or `mutate_dict`. Pushes the return
    /// value (typically `None`).
    ///
    /// This exists because mutating methods (`append`, `pop`, etc.) need to
    /// modify the variable in place rather than operating on a cloned value.
    CallMutMethod(u32, u32, u32),

    // --- F-strings ---
    /// Pop TOS, convert it to its print representation via `to_print_string()`,
    /// push the resulting string.
    FormatValue,

    /// Pop N strings from the stack, concatenate them, push the result.
    BuildString(u32),

    // --- Function definitions ---
    /// Register `functions[i]` as a user-defined callable and push `None`.
    ///
    /// The function is stored in the VM's function table (keyed by name) and
    /// can be called via `CallFunction`.
    MakeFunction(u32),

    /// Return from the current function.
    ///
    /// Pop TOS as the return value, pop the call frame, and push the return
    /// value onto the caller's stack. At the top level, this ends execution.
    ReturnValue,

    // --- Exception handling ---
    /// Raise an exception.
    ///
    /// Pop TOS (a string message), pop the exception-type name string below it,
    /// and raise as an exception. The VM maps these to internal `Error` variants.
    Raise,

    /// Re-raise the current exception from inside an `except` handler.
    ///
    /// Used for bare `raise` statements. If there is no active exception on
    /// the exception stack, this is a runtime error.
    Reraise,

    /// Check if the current exception matches a given type.
    ///
    /// Pop the exception type name (a string) from TOS. Check it against the
    /// active exception. Push `Bool(true)` if it matches, `Bool(false)` otherwise.
    CheckExcMatch,

    /// Discard the current exception from the exception stack.
    ///
    /// Emitted at the end of each `except` handler body.
    PopException,

    // --- Misc ---
    /// No operation. Used as a placeholder or for `pass` statements.
    Nop,
}

// ---------------------------------------------------------------------------
// Compiled code representation
// ---------------------------------------------------------------------------

/// An entry in the exception table mapping an instruction range to a handler.
///
/// When an error occurs at an instruction in `[start, end)`, the VM jumps to
/// `handler` and optionally binds the exception to `names[var_name]`.
#[derive(Debug, Clone)]
pub struct ExceptionEntry {
    /// First instruction index covered (inclusive).
    pub start: u32,
    /// Last instruction index covered (exclusive).
    pub end: u32,
    /// Instruction index of the handler to jump to.
    pub handler: u32,
    /// If `Some`, the exception message is stored in `names[i]`.
    pub var_name: Option<u32>,
}

/// A compiled unit of code — either a top-level script or a function body.
///
/// The `instructions` and `spans` vectors are always the same length: each
/// instruction has a corresponding source span for error reporting. Constants
/// and names are stored in pools and referenced by index from instructions.
#[derive(Debug, Clone)]
pub struct CodeObject {
    /// The bytecode instructions to execute.
    pub instructions: Vec<Op>,

    /// Constant pool: literal values referenced by `LoadConst(index)`.
    pub constants: Vec<PyValue>,

    /// Name pool: variable, function, and method names referenced by index.
    pub names: Vec<String>,

    /// Source span for each instruction (parallel to `instructions`).
    ///
    /// Used to produce error messages with accurate source locations even
    /// though the AST is no longer available at execution time.
    pub spans: Vec<Span>,

    /// Compiled function bodies referenced by `MakeFunction(index)`.
    pub functions: Vec<FunctionDef>,

    /// The original source code, stored for diagnostic error messages.
    pub source: String,

    /// Exception table for try/except handling.
    ///
    /// Each entry maps an instruction range to a handler location. When an
    /// error occurs, the VM searches this table (last entry first) for a
    /// matching range and jumps to the handler.
    pub exception_table: Vec<ExceptionEntry>,
}

impl CodeObject {
    /// Create a new empty code object for the given source code.
    pub fn new(source: String) -> Self {
        Self {
            instructions: Vec::new(),
            constants: Vec::new(),
            names: Vec::new(),
            spans: Vec::new(),
            functions: Vec::new(),
            source,
            exception_table: Vec::new(),
        }
    }
}

/// A compiled function definition, stored inside a parent [`CodeObject`].
///
/// When the VM encounters `MakeFunction(i)`, it registers `functions[i]` as
/// a callable. When the function is later called, the VM creates a new call
/// frame using `code` and binds arguments to `params`.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    /// The function name (used for error messages and as the key in the function table).
    pub name: String,

    /// Parameter names, in order. Used to bind positional arguments to local variables.
    pub params: Vec<String>,

    /// Default values for the last N parameters (same convention as CPython).
    ///
    /// `defaults[0]` corresponds to `params[params.len() - defaults.len()]`.
    /// Parameters without defaults must be provided by the caller.
    pub defaults: Vec<PyValue>,

    /// If present, the name of the `*args` parameter that collects excess
    /// positional arguments into a list.
    pub vararg: Option<String>,

    /// If present, the name of the `**kwargs` parameter that collects excess
    /// keyword arguments into a dict.
    pub kwarg: Option<String>,

    /// The compiled function body.
    pub code: CodeObject,
}
