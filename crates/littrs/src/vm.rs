//! Stack-based bytecode virtual machine.
//!
//! This module executes [`CodeObject`]s produced by the compiler. It uses a
//! simple fetch-decode-execute loop over a flat instruction array. All runtime
//! state — the value stack, call frames, variables, and iterators — lives here.
//!
//! The VM is the **only** module that mutates runtime state; the compiler is
//! pure translation and every other module (`builtins`, `methods`, `operators`,
//! `slice`) is a stateless helper.

use std::collections::HashMap;
use std::sync::Arc;

use crate::builtins::{BuiltinResult, try_builtin};
use crate::bytecode::{CodeObject, ExceptionEntry, FunctionDef, Op, UnaryOp};
use crate::diagnostic::{Diagnostic, Span};
use crate::error::{Error, Result};
use crate::methods;
use crate::operators::{apply_binop, apply_cmpop};
use crate::slice;
use crate::tool::ToolInfo;
use crate::value::PyValue;

/// An active exception on the exception stack.
#[derive(Clone, Debug)]
struct ExceptionState {
    /// Python exception type name (e.g. "ValueError", "TypeError").
    exception_type: String,
    /// The error message.
    message: String,
}

/// Type alias for tool callback functions registered by the host.
pub type ToolFn = Arc<dyn Fn(Vec<PyValue>) -> PyValue + Send + Sync>;

/// A registered tool with its callback and metadata.
#[derive(Clone)]
struct RegisteredTool {
    func: ToolFn,
    /// Parameter names, used to map keyword arguments to positions.
    arg_names: Vec<String>,
    /// Optional rich metadata for type validation and diagnostics.
    info: Option<ToolInfo>,
}

/// A mounted virtual file definition.
#[derive(Clone, Debug)]
pub struct MountEntry {
    /// Host filesystem path (for write-through).
    pub host_path: String,
    /// Whether the file is writable from sandbox code.
    pub writable: bool,
    /// File content read at mount time (or accumulated via writes).
    pub content: String,
}

/// Runtime state of an open file handle.
#[derive(Clone, Debug)]
struct FileState {
    /// The virtual path this file was opened with.
    virtual_path: String,
    /// Current buffer contents.
    buffer: String,
    /// Cursor position for reading.
    cursor: usize,
    /// Whether the file was opened in write mode.
    write_mode: bool,
    /// Whether the file has been closed.
    closed: bool,
}

/// State of a single iterator (used by `for` loops and comprehensions).
struct IterState {
    items: Vec<PyValue>,
    index: usize,
}

/// A single activation record on the call stack.
///
/// Each function call (including the top-level script) gets its own frame.
/// The frame owns a reference to the [`CodeObject`] it is executing, an
/// instruction pointer, local variables, and any active iterators.
struct CallFrame {
    /// The compiled code being executed.
    code: CodeObject,
    /// Instruction pointer — index of the *next* instruction to execute.
    ip: usize,
    /// Local variables for this frame (only used inside function bodies).
    locals: HashMap<String, PyValue>,
    /// Stack base: the index into the VM's value stack where this frame's
    /// operands begin. Used to isolate frames from each other.
    stack_base: usize,
    /// Active iterators for `for` loops within this frame.
    iterators: Vec<IterState>,
}

/// The bytecode virtual machine.
///
/// Holds all mutable runtime state: the value stack, global variables,
/// registered tools, user-defined functions, the call stack, and the
/// print buffer. A single `Vm` instance is meant to be reused across
/// multiple `execute()` calls (globals persist between calls, matching
/// the behaviour of the old `Evaluator`).
#[derive(Clone)]
pub struct Vm {
    /// The operand stack shared across all frames.
    stack: Vec<PyValue>,
    /// Global variables (persist across `execute()` calls).
    globals: HashMap<String, PyValue>,
    /// Host-registered tool functions.
    tools: HashMap<String, RegisteredTool>,
    /// Registered modules available for `import`.
    modules: HashMap<String, PyValue>,
    /// Captured output from `print()` calls.
    print_buffer: Vec<String>,
    /// Maximum number of bytecode instructions per `execute()` call.
    instruction_limit: Option<u64>,
    /// Maximum call-stack depth for user-defined functions.
    recursion_limit: Option<usize>,
    /// Instructions executed so far in the current `execute()` call.
    instruction_count: u64,
    /// Stack of active exceptions (for try/except handling).
    exception_stack: Vec<ExceptionState>,
    /// Mounted virtual files (virtual path → mount entry).
    mounts: HashMap<String, MountEntry>,
    /// Open file handles (handle id → file state).
    open_files: HashMap<u64, FileState>,
    /// Next file handle id to allocate.
    next_file_handle: u64,
}

// We implement Clone manually for the parts that need it, but CallFrame
// is not Clone (it only exists during execution). The `Clone` on `Vm`
// covers the persistent state between executions.

impl Vm {
    /// Create a new, empty VM.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            globals: HashMap::new(),
            tools: HashMap::new(),
            modules: HashMap::new(),
            print_buffer: Vec::new(),
            instruction_limit: None,
            recursion_limit: None,
            instruction_count: 0,
            exception_stack: Vec::new(),
            mounts: HashMap::new(),
            open_files: HashMap::new(),
            next_file_handle: 0,
        }
    }

    /// Register a tool function that can be called from Python code.
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

    /// Register a tool with full metadata for type validation and diagnostics.
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

    /// Set a global variable visible to Python code.
    pub fn set_variable(&mut self, name: impl Into<String>, value: PyValue) {
        self.globals.insert(name.into(), value);
    }

    /// Register a module that can be imported from Python code.
    pub fn register_module(&mut self, name: impl Into<String>, module: PyValue) {
        self.modules.insert(name.into(), module);
    }

    /// Take and clear the print buffer, returning all captured output.
    pub fn take_print_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.print_buffer)
    }

    /// Clear the print buffer without returning it.
    pub fn clear_print_buffer(&mut self) {
        self.print_buffer.clear();
    }

    /// Set resource limits for execution.
    pub fn set_limits(&mut self, instruction_limit: Option<u64>, recursion_limit: Option<usize>) {
        self.instruction_limit = instruction_limit;
        self.recursion_limit = recursion_limit;
    }

    /// Mount a virtual file visible to sandbox code.
    ///
    /// `content` is the initial file content (read from host at mount time).
    /// If `writable` is true, sandbox code can open the file in write mode.
    pub fn mount(&mut self, virtual_path: String, host_path: String, writable: bool, content: String) {
        self.mounts.insert(virtual_path, MountEntry {
            host_path,
            writable,
            content,
        });
    }

    /// Get current contents of all writable mounted files.
    pub fn get_writable_files(&self) -> HashMap<String, String> {
        self.mounts
            .iter()
            .filter(|(_, entry)| entry.writable)
            .map(|(path, entry)| (path.clone(), entry.content.clone()))
            .collect()
    }

    /// Execute a compiled [`CodeObject`].
    ///
    /// Returns the value left on top of the stack after the last instruction,
    /// or `PyValue::None` if the stack is empty. Global variables set during
    /// execution persist for subsequent calls.
    pub fn execute(&mut self, code: CodeObject) -> Result<PyValue> {
        // Reset instruction counter for this execution
        self.instruction_count = 0;

        // Set up the top-level frame
        let frame = CallFrame {
            code,
            ip: 0,
            locals: HashMap::new(),
            stack_base: self.stack.len(),
            iterators: Vec::new(),
        };

        let mut frames = vec![frame];
        let result = self.run(&mut frames)?;

        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Main execution loop
    // -----------------------------------------------------------------------

    /// The core fetch-decode-execute loop.
    ///
    /// Iterates over instructions in the current frame, dispatching each
    /// opcode to the appropriate handler. Manages the call stack for
    /// function calls and returns. Errors are intercepted and routed
    /// through the exception table for try/except handling.
    fn run(&mut self, frames: &mut Vec<CallFrame>) -> Result<PyValue> {
        loop {
            let frame = match frames.last() {
                Some(f) => f,
                None => {
                    // All frames exhausted — return whatever is on TOS
                    return Ok(self.stack.pop().unwrap_or(PyValue::None));
                }
            };

            // Check if we've reached the end of the current frame
            if frame.ip >= frame.code.instructions.len() {
                return self.end_frame(frames);
            }

            // Fetch
            let ip = frame.ip;
            let op = frame.code.instructions[ip].clone();
            let span = frame.code.spans[ip];

            // Advance ip before executing (so jumps can overwrite it)
            frames.last_mut().unwrap().ip += 1;

            // Check instruction limit (uncatchable)
            self.instruction_count += 1;
            if let Some(limit) = self.instruction_limit
                && self.instruction_count > limit
            {
                return Err(Error::InstructionLimitExceeded(limit));
            }

            // Dispatch the instruction, catching errors for exception handling
            let result = self.dispatch_op(op, span, ip, frames);

            if let Err(err) = result {
                // Resource limit errors are uncatchable
                if is_uncatchable(&err) {
                    return Err(err);
                }

                // Try to find an exception handler in the current frame or callers
                if self.handle_exception(frames, &err, ip)? {
                    continue; // Handler found — resume execution
                }

                // No handler found — propagate the error
                return Err(err);
            }
        }
    }

    /// Dispatch a single opcode. Separated from `run()` so that errors can
    /// be intercepted for try/except handling.
    fn dispatch_op(
        &mut self,
        op: Op,
        span: Span,
        _ip: usize,
        frames: &mut Vec<CallFrame>,
    ) -> Result<()> {
        match op {
            // --- Stack manipulation ---
            Op::LoadConst(i) => {
                let val = frames.last().unwrap().code.constants[i as usize].clone();
                self.stack.push(val);
            }
            Op::Pop => {
                self.stack.pop();
            }
            Op::Dup => {
                let val = self.stack.last().cloned().unwrap_or(PyValue::None);
                self.stack.push(val);
            }
            Op::RotN(n) => {
                let len = self.stack.len();
                if (n as usize) <= len {
                    let idx = len - n as usize;
                    let top = self.stack.pop().unwrap();
                    self.stack.insert(idx, top);
                }
            }

            // --- Variables ---
            Op::LoadName(i) => {
                let name = &frames.last().unwrap().code.names[i as usize];
                // Look up: locals → globals
                if let Some(val) = frames.last().unwrap().locals.get(name) {
                    self.stack.push(val.clone());
                } else if let Some(val) = self.globals.get(name) {
                    self.stack.push(val.clone());
                } else {
                    let name = name.clone();
                    return Err(Error::NameError(name));
                }
            }
            Op::StoreName(i) => {
                let name = frames.last().unwrap().code.names[i as usize].clone();
                let val = self.stack.pop().unwrap_or(PyValue::None);
                if frames.len() > 1 {
                    // Inside a function: store in locals
                    frames.last_mut().unwrap().locals.insert(name, val);
                } else {
                    // Top-level: store in globals
                    self.globals.insert(name, val);
                }
            }

            // --- Operators ---
            Op::BinaryOp(binop) => {
                let right = self.stack.pop().unwrap_or(PyValue::None);
                let left = self.stack.pop().unwrap_or(PyValue::None);
                let result = apply_binop(&binop, &left, &right)?;
                self.stack.push(result);
            }
            Op::UnaryOp(unary) => {
                let operand = self.stack.pop().unwrap_or(PyValue::None);
                let result = self.apply_unaryop(&unary, &operand)?;
                self.stack.push(result);
            }
            Op::CompareOp(cmpop) => {
                let right = self.stack.pop().unwrap_or(PyValue::None);
                let left = self.stack.pop().unwrap_or(PyValue::None);
                let result = apply_cmpop(&cmpop, &left, &right)?;
                self.stack.push(PyValue::Bool(result));
            }

            // --- Short-circuit boolean ---
            Op::JumpIfFalseOrPop(target) => {
                let tos = self.stack.last().cloned().unwrap_or(PyValue::None);
                if !tos.is_truthy() {
                    frames.last_mut().unwrap().ip = target as usize;
                } else {
                    self.stack.pop();
                }
            }
            Op::JumpIfTrueOrPop(target) => {
                let tos = self.stack.last().cloned().unwrap_or(PyValue::None);
                if tos.is_truthy() {
                    frames.last_mut().unwrap().ip = target as usize;
                } else {
                    self.stack.pop();
                }
            }

            // --- Control flow ---
            Op::Jump(target) => {
                frames.last_mut().unwrap().ip = target as usize;
            }
            Op::PopJumpIfTrue(target) => {
                let val = self.stack.pop().unwrap_or(PyValue::None);
                if val.is_truthy() {
                    frames.last_mut().unwrap().ip = target as usize;
                }
            }
            Op::PopJumpIfFalse(target) => {
                let val = self.stack.pop().unwrap_or(PyValue::None);
                if !val.is_truthy() {
                    frames.last_mut().unwrap().ip = target as usize;
                }
            }

            // --- Collections ---
            Op::BuildList(n) => {
                let start = self.stack.len() - n as usize;
                let items: Vec<PyValue> = self.stack.drain(start..).collect();
                self.stack.push(PyValue::List(items));
            }
            Op::BuildTuple(n) => {
                let start = self.stack.len() - n as usize;
                let items: Vec<PyValue> = self.stack.drain(start..).collect();
                self.stack.push(PyValue::Tuple(items));
            }
            Op::BuildSet(n) => {
                let start = self.stack.len() - n as usize;
                let raw: Vec<PyValue> = self.stack.drain(start..).collect();
                let mut items = Vec::with_capacity(n as usize);
                for elem in raw {
                    if !elem.is_hashable() {
                        return Err(Error::Runtime(format!(
                            "TypeError: unhashable type: '{}'",
                            elem.type_name()
                        )));
                    }
                    if !items.contains(&elem) {
                        items.push(elem);
                    }
                }
                self.stack.push(PyValue::Set(items));
            }
            Op::BuildDict(n) => {
                let start = self.stack.len() - (n as usize * 2);
                let raw: Vec<PyValue> = self.stack.drain(start..).collect();
                let mut pairs = Vec::with_capacity(n as usize);
                for chunk in raw.chunks(2) {
                    let key = chunk[0].clone();
                    if !key.is_hashable() {
                        return Err(Error::Runtime(format!(
                            "TypeError: unhashable type: '{}'",
                            key.type_name()
                        )));
                    }
                    pairs.push((key, chunk[1].clone()));
                }
                self.stack.push(PyValue::Dict(pairs));
            }

            // --- Subscript ---
            Op::BinarySubscript => {
                let index = self.stack.pop().unwrap_or(PyValue::None);
                let collection = self.stack.pop().unwrap_or(PyValue::None);
                let result = self.subscript(&collection, &index)?;
                self.stack.push(result);
            }
            Op::StoreSubscript(var_idx) => {
                let index = self.stack.pop().unwrap_or(PyValue::None);
                let value = self.stack.pop().unwrap_or(PyValue::None);
                let var_name = frames.last().unwrap().code.names[var_idx as usize].clone();
                self.store_subscript(frames, &var_name, &index, value)?;
            }

            // --- Slicing ---
            Op::Slice => {
                let step = self.stack.pop().unwrap_or(PyValue::None);
                let stop = self.stack.pop().unwrap_or(PyValue::None);
                let start = self.stack.pop().unwrap_or(PyValue::None);
                let obj = self.stack.pop().unwrap_or(PyValue::None);
                let result = self.apply_slice(&obj, &start, &stop, &step)?;
                self.stack.push(result);
            }

            // --- Unpacking ---
            Op::UnpackSequence(n) => {
                let val = self.stack.pop().unwrap_or(PyValue::None);
                let items = match val {
                    PyValue::List(items) | PyValue::Tuple(items) => items,
                    _ => {
                        return Err(Error::Type {
                            expected: "sequence".to_string(),
                            got: val.type_name().to_string(),
                        });
                    }
                };
                if items.len() != n as usize {
                    return Err(Error::Runtime(format!(
                        "cannot unpack: expected {} values, got {}",
                        n,
                        items.len()
                    )));
                }
                for item in items.into_iter().rev() {
                    self.stack.push(item);
                }
            }

            // --- Iteration ---
            Op::GetIter => {
                let val = self.stack.pop().unwrap_or(PyValue::None);
                let items = match val {
                    PyValue::List(items) | PyValue::Tuple(items) | PyValue::Set(items) => items,
                    PyValue::Dict(pairs) => pairs.into_iter().map(|(k, _)| k).collect(),
                    PyValue::Str(s) => s.chars().map(|c| PyValue::Str(c.to_string())).collect(),
                    other => {
                        return Err(Error::Type {
                            expected: "iterable".to_string(),
                            got: other.type_name().to_string(),
                        });
                    }
                };
                frames
                    .last_mut()
                    .unwrap()
                    .iterators
                    .push(IterState { items, index: 0 });
            }
            Op::ForIter(target) => {
                let frame = frames.last_mut().unwrap();
                let iter = frame.iterators.last_mut().unwrap();
                if iter.index < iter.items.len() {
                    let item = iter.items[iter.index].clone();
                    iter.index += 1;
                    self.stack.push(item);
                } else {
                    frame.iterators.pop();
                    frame.ip = target as usize;
                }
            }
            Op::PopIter => {
                frames.last_mut().unwrap().iterators.pop();
            }

            // --- Function calls ---
            Op::CallFunction(name_idx, n_args) => {
                let name = frames.last().unwrap().code.names[name_idx as usize].clone();
                self.call_function(frames, &name, n_args as usize, 0, span)?;
            }
            Op::CallFunctionKw(name_idx, n_pos, n_kw) => {
                let name = frames.last().unwrap().code.names[name_idx as usize].clone();
                self.call_function(frames, &name, n_pos as usize, n_kw as usize, span)?;
            }
            Op::CallMethod(method_idx, n_args) => {
                let method = frames.last().unwrap().code.names[method_idx as usize].clone();
                self.call_method(frames, &method, n_args as usize)?;
            }
            Op::CallMutMethod(var_idx, method_idx, n_args) => {
                let var_name = frames.last().unwrap().code.names[var_idx as usize].clone();
                let method = frames.last().unwrap().code.names[method_idx as usize].clone();
                self.call_mut_method(frames, &var_name, &method, n_args as usize)?;
            }
            Op::CallMutMethodKw(var_idx, method_idx, n_pos, n_kw) => {
                let var_name = frames.last().unwrap().code.names[var_idx as usize].clone();
                let method = frames.last().unwrap().code.names[method_idx as usize].clone();
                self.call_mut_method_kw(
                    frames,
                    &var_name,
                    &method,
                    n_pos as usize,
                    n_kw as usize,
                )?;
            }
            Op::CallValue(n_args) => {
                self.call_value(frames, n_args as usize, 0)?;
            }
            Op::CallValueKw(n_pos, n_kw) => {
                self.call_value(frames, n_pos as usize, n_kw as usize)?;
            }

            // --- F-strings ---
            Op::FormatValue => {
                let val = self.stack.pop().unwrap_or(PyValue::None);
                self.stack.push(PyValue::Str(val.to_print_string()));
            }
            Op::BuildString(n) => {
                let start = self.stack.len() - n as usize;
                let parts: Vec<PyValue> = self.stack.drain(start..).collect();
                let mut result = String::new();
                for part in parts {
                    if let PyValue::Str(s) = part {
                        result.push_str(&s);
                    } else {
                        result.push_str(&part.to_print_string());
                    }
                }
                self.stack.push(PyValue::Str(result));
            }

            // --- Function definitions ---
            Op::MakeFunction(i) => {
                let func_def = frames.last().unwrap().code.functions[i as usize].clone();
                self.stack.push(PyValue::Function(Box::new(func_def)));
            }
            Op::ReturnValue => {
                let retval = self.stack.pop().unwrap_or(PyValue::None);
                let finished = frames.pop().unwrap();
                self.stack.truncate(finished.stack_base);
                if frames.is_empty() {
                    // Returning from top-level — push the value back so
                    // the run() loop can return it via end_frame or TOS
                    self.stack.push(retval);
                    return Ok(());
                }
                self.stack.push(retval);
            }

            // --- Imports ---
            Op::ImportModule(name_idx) => {
                let name = frames.last().unwrap().code.names[name_idx as usize].clone();
                if let Some(module) = self.modules.get(&name) {
                    self.stack.push(module.clone());
                } else {
                    return Err(Error::Runtime(format!(
                        "ModuleNotFoundError: No module named '{}'",
                        name
                    )));
                }
            }
            Op::LoadAttr(attr_idx) => {
                let attr_name = frames.last().unwrap().code.names[attr_idx as usize].clone();
                let obj = self.stack.pop().unwrap_or(PyValue::None);
                match &obj {
                    PyValue::Module { name, attrs } => {
                        if let Some((_, val)) = attrs.iter().find(|(k, _)| k == &attr_name) {
                            self.stack.push(val.clone());
                        } else {
                            return Err(Error::Runtime(format!(
                                "AttributeError: module '{}' has no attribute '{}'",
                                name, attr_name
                            )));
                        }
                    }
                    _ => {
                        return Err(Error::Runtime(format!(
                            "AttributeError: '{}' object has no attribute '{}'",
                            obj.type_name(),
                            attr_name
                        )));
                    }
                }
            }

            // --- Exception handling ---
            Op::Raise => {
                let message = self.stack.pop().unwrap_or(PyValue::None);
                let exc_type = self.stack.pop().unwrap_or(PyValue::None);
                let type_name = match &exc_type {
                    PyValue::Str(s) => s.clone(),
                    _ => "Exception".to_string(),
                };
                let msg = match &message {
                    PyValue::Str(s) => s.clone(),
                    PyValue::None => type_name.clone(),
                    other => other.to_print_string(),
                };
                return Err(Error::Runtime(format!("{}: {}", type_name, msg)));
            }
            Op::Reraise => {
                if let Some(exc) = self.exception_stack.last() {
                    return Err(Error::Runtime(format!(
                        "{}: {}",
                        exc.exception_type, exc.message
                    )));
                }
                return Err(Error::Runtime(
                    "No active exception to re-raise".to_string(),
                ));
            }
            Op::CheckExcMatch => {
                let type_name = self.stack.pop().unwrap_or(PyValue::None);
                let expected = match &type_name {
                    PyValue::Str(s) => s.as_str(),
                    _ => "Exception",
                };
                let matches = if let Some(exc) = self.exception_stack.last() {
                    exception_matches(&exc.exception_type, expected)
                } else {
                    false
                };
                self.stack.push(PyValue::Bool(matches));
            }
            Op::PopException => {
                self.exception_stack.pop();
            }

            // --- Misc ---
            Op::Nop => {}
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Exception handling
    // -----------------------------------------------------------------------

    /// Try to find and activate an exception handler for the given error.
    ///
    /// Walks the call stack from the current frame upward, searching each
    /// frame's exception table for a handler covering the faulting instruction.
    /// If found, pushes the exception onto the exception stack, optionally
    /// binds it to a variable, sets the IP to the handler, and returns `true`.
    /// Returns `false` if no handler is found.
    fn handle_exception(
        &mut self,
        frames: &mut Vec<CallFrame>,
        err: &Error,
        fault_ip: usize,
    ) -> Result<bool> {
        let exc_type = error_to_exception_type(err);
        let message = err.to_string();
        let mut is_first_frame = true;

        while !frames.is_empty() {
            let ip_to_check = if is_first_frame {
                is_first_frame = false;
                fault_ip
            } else {
                // For caller frames, the IP points past the call instruction
                frames.last().unwrap().ip.saturating_sub(1)
            };

            let handler = find_handler(&frames.last().unwrap().code.exception_table, ip_to_check);

            if let Some(entry) = handler {
                let handler_target = entry.handler;
                let var_name_idx = entry.var_name;
                let n_frames = frames.len();

                // Read stack_base and var name before taking mutable ref
                let stack_base = frames.last().unwrap().stack_base;
                let var_info =
                    var_name_idx.map(|idx| frames.last().unwrap().code.names[idx as usize].clone());

                // Clean up the stack back to this frame's base
                self.stack.truncate(stack_base);

                // Push exception state
                self.exception_stack.push(ExceptionState {
                    exception_type: exc_type.to_string(),
                    message: message.clone(),
                });

                // Optionally bind exception message to a variable
                if let Some(var) = var_info {
                    if n_frames > 1 {
                        frames
                            .last_mut()
                            .unwrap()
                            .locals
                            .insert(var, PyValue::Str(message.clone()));
                    } else {
                        self.globals.insert(var, PyValue::Str(message.clone()));
                    }
                }

                // Jump to handler
                frames.last_mut().unwrap().ip = handler_target as usize;
                return Ok(true);
            }

            // No handler in this frame — pop it and try the caller
            let finished = frames.pop().unwrap();
            self.stack.truncate(finished.stack_base);
        }

        Ok(false)
    }

    // -----------------------------------------------------------------------
    // End-of-frame handling
    // -----------------------------------------------------------------------

    /// Handle reaching the end of a frame's instructions.
    fn end_frame(&mut self, frames: &mut Vec<CallFrame>) -> Result<PyValue> {
        let finished = frames.pop().unwrap();

        if frames.is_empty() {
            // Top-level script finished.
            // Merge locals into globals (top-level assignments persist).
            for (k, v) in finished.locals {
                self.globals.insert(k, v);
            }
            // Return TOS if available, otherwise None
            if self.stack.len() > finished.stack_base {
                Ok(self.stack.pop().unwrap())
            } else {
                Ok(PyValue::None)
            }
        } else {
            // Function frame ended without explicit ReturnValue → implicit return None
            self.stack.truncate(finished.stack_base);
            self.stack.push(PyValue::None);
            // Continue executing the caller
            self.run(frames)
        }
    }

    // -----------------------------------------------------------------------
    // Operator helpers
    // -----------------------------------------------------------------------

    /// Apply a unary operator to a value.
    fn apply_unaryop(&self, op: &UnaryOp, operand: &PyValue) -> Result<PyValue> {
        match op {
            UnaryOp::Not => Ok(PyValue::Bool(!operand.is_truthy())),
            UnaryOp::Neg => match operand {
                PyValue::Int(i) => Ok(PyValue::Int(-i)),
                PyValue::Float(f) => Ok(PyValue::Float(-f)),
                _ => Err(Error::Type {
                    expected: "number".to_string(),
                    got: operand.type_name().to_string(),
                }),
            },
            UnaryOp::Pos => match operand {
                PyValue::Int(_) | PyValue::Float(_) => Ok(operand.clone()),
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

    // -----------------------------------------------------------------------
    // Subscript helpers
    // -----------------------------------------------------------------------

    /// Perform `collection[index]`.
    fn subscript(&self, collection: &PyValue, index: &PyValue) -> Result<PyValue> {
        match (collection, index) {
            (PyValue::List(items), PyValue::Int(idx)) => {
                let len = items.len() as i64;
                let actual = if *idx < 0 { len + idx } else { *idx } as usize;
                items
                    .get(actual)
                    .cloned()
                    .ok_or_else(|| Error::Runtime(format!("list index out of range: {}", idx)))
            }
            (PyValue::Tuple(items), PyValue::Int(idx)) => {
                let len = items.len() as i64;
                let actual = if *idx < 0 { len + idx } else { *idx } as usize;
                items
                    .get(actual)
                    .cloned()
                    .ok_or_else(|| Error::Runtime(format!("tuple index out of range: {}", idx)))
            }
            (PyValue::Str(s), PyValue::Int(idx)) => {
                let len = s.len() as i64;
                let actual = if *idx < 0 { len + idx } else { *idx } as usize;
                s.chars()
                    .nth(actual)
                    .map(|c| PyValue::Str(c.to_string()))
                    .ok_or_else(|| Error::Runtime(format!("string index out of range: {}", idx)))
            }
            (PyValue::Dict(pairs), key) if key.is_hashable() => pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| Error::Runtime(format!("KeyError: {}", key))),
            _ => Err(Error::Type {
                expected: "subscriptable".to_string(),
                got: collection.type_name().to_string(),
            }),
        }
    }

    /// Perform `variable[index] = value`, mutating the variable in place.
    fn store_subscript(
        &mut self,
        frames: &mut [CallFrame],
        var_name: &str,
        index: &PyValue,
        value: PyValue,
    ) -> Result<()> {
        let var = self.lookup_var_mut(frames, var_name)?;

        match var {
            PyValue::List(items) => {
                let idx = index.as_int().ok_or_else(|| Error::Type {
                    expected: "int".to_string(),
                    got: index.type_name().to_string(),
                })?;
                let len = items.len() as i64;
                let actual = if idx < 0 { len + idx } else { idx } as usize;
                if actual < items.len() {
                    items[actual] = value;
                    return Ok(());
                }
                Err(Error::Runtime("list index out of range".to_string()))
            }
            PyValue::Dict(pairs) => {
                if !index.is_hashable() {
                    return Err(Error::Runtime(format!(
                        "TypeError: unhashable type: '{}'",
                        index.type_name()
                    )));
                }
                if let Some(existing) = pairs.iter_mut().find(|(k, _)| k == index) {
                    existing.1 = value;
                } else {
                    pairs.push((index.clone(), value));
                }
                Ok(())
            }
            PyValue::Tuple(_) => Err(Error::Runtime(
                "TypeError: 'tuple' object does not support item assignment".to_string(),
            )),
            _ => Err(Error::Runtime("Cannot assign to subscript".to_string())),
        }
    }

    // -----------------------------------------------------------------------
    // Slice helper
    // -----------------------------------------------------------------------

    /// Apply a slice operation: `obj[start:stop:step]`.
    fn apply_slice(
        &self,
        obj: &PyValue,
        start: &PyValue,
        stop: &PyValue,
        step: &PyValue,
    ) -> Result<PyValue> {
        let to_opt = |v: &PyValue| -> Result<Option<i64>> {
            match v {
                PyValue::None => Ok(None),
                PyValue::Int(i) => Ok(Some(*i)),
                _ => Err(Error::Type {
                    expected: "int".to_string(),
                    got: v.type_name().to_string(),
                }),
            }
        };

        let lower = to_opt(start)?;
        let upper = to_opt(stop)?;
        let step_val = to_opt(step)?;

        if let Some(s) = step_val
            && s == 0
        {
            return Err(Error::Runtime("slice step cannot be zero".to_string()));
        }

        match obj {
            PyValue::List(items) => slice::slice_list(items, lower, upper, step_val),
            PyValue::Tuple(items) => slice::slice_tuple(items, lower, upper, step_val),
            PyValue::Str(s) => slice::slice_string(s, lower, upper, step_val),
            _ => Err(Error::Type {
                expected: "list, tuple, or str".to_string(),
                got: obj.type_name().to_string(),
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Function / tool call dispatch
    // -----------------------------------------------------------------------

    /// Dispatch a function call: builtins → tools → user functions.
    ///
    /// Pops `n_pos` positional args and `n_kw` keyword pairs (each pair is
    /// two stack entries: name string + value) from the stack, dispatches to
    /// the appropriate handler, and pushes the return value.
    fn call_function(
        &mut self,
        frames: &mut Vec<CallFrame>,
        name: &str,
        n_pos: usize,
        n_kw: usize,
        span: Span,
    ) -> Result<()> {
        // Pop keyword args (name, value pairs) in reverse
        let mut kw_pairs: Vec<(String, PyValue)> = Vec::with_capacity(n_kw);
        for _ in 0..n_kw {
            let value = self.stack.pop().unwrap_or(PyValue::None);
            let key_val = self.stack.pop().unwrap_or(PyValue::None);
            let key = match key_val {
                PyValue::Str(s) => s,
                _ => {
                    return Err(Error::Runtime(
                        "keyword argument name must be a string".to_string(),
                    ));
                }
            };
            kw_pairs.push((key, value));
        }
        kw_pairs.reverse();

        // Pop positional args
        let start = self.stack.len() - n_pos;
        let pos_args: Vec<PyValue> = self.stack.drain(start..).collect();

        // 0. Callable-aware builtins (sorted, map, filter, open)
        match name {
            "sorted" => {
                let result = self.builtin_sorted(frames, pos_args, kw_pairs)?;
                self.stack.push(result);
                return Ok(());
            }
            "map" if n_kw == 0 => {
                let result = self.builtin_map(frames, pos_args)?;
                self.stack.push(result);
                return Ok(());
            }
            "filter" if n_kw == 0 => {
                let result = self.builtin_filter(frames, pos_args)?;
                self.stack.push(result);
                return Ok(());
            }
            "open" if n_kw == 0 => {
                let result = self.builtin_open(pos_args)?;
                self.stack.push(result);
                return Ok(());
            }
            _ => {}
        }

        // 1. Try builtins (no keyword support for builtins)
        if n_kw == 0 {
            match try_builtin(name, pos_args.clone(), &mut self.print_buffer) {
                BuiltinResult::Handled(result) => {
                    self.stack.push(result?);
                    return Ok(());
                }
                BuiltinResult::NotBuiltin => {}
            }
        }

        // 2. Try registered tools
        if let Some(tool) = self.tools.get(name).cloned() {
            let result = self.call_tool(name, &tool, pos_args, &kw_pairs, span)?;
            self.stack.push(result);
            return Ok(());
        }

        // 3. Try user-defined functions or native functions (look up in locals then globals)
        let func = frames
            .last()
            .and_then(|f| f.locals.get(name))
            .or_else(|| self.globals.get(name))
            .cloned();

        if let Some(PyValue::Function(func)) = func {
            return self.invoke_function_def(frames, &func, name, pos_args, kw_pairs);
        }

        if let Some(PyValue::NativeFunction(key)) = func {
            if let Some(tool) = self.tools.get(&key).cloned() {
                let result = (tool.func)(pos_args);
                self.stack.push(result);
                return Ok(());
            }
        }

        // 4. Nothing matched
        Err(Error::NameError(name.to_string()))
    }

    /// Bind arguments to a FunctionDef and push a new call frame.
    ///
    /// Shared by `call_function` (by-name lookup) and `call_value` (stack-based).
    fn invoke_function_def(
        &mut self,
        frames: &mut Vec<CallFrame>,
        func: &FunctionDef,
        name: &str,
        pos_args: Vec<PyValue>,
        kw_pairs: Vec<(String, PyValue)>,
    ) -> Result<()> {
        let n_params = func.params.len();
        let n_defaults = func.defaults.len();
        let n_required = n_params - n_defaults;
        let has_vararg = func.vararg.is_some();
        let has_kwarg = func.kwarg.is_some();

        // Step 1: Bind positional args to named params
        let mut bound: Vec<Option<PyValue>> = vec![None; n_params];
        let mut extra_positional = Vec::new();

        for (i, val) in pos_args.into_iter().enumerate() {
            if i < n_params {
                bound[i] = Some(val);
            } else if has_vararg {
                extra_positional.push(val);
            } else {
                return Err(Error::Runtime(format!(
                    "{}() takes {} argument(s), {} given",
                    name,
                    n_params,
                    i + 1 + extra_positional.len()
                )));
            }
        }

        // Step 2: Map keyword args to named params or collect into **kwargs
        let mut extra_kwargs: Vec<(String, PyValue)> = Vec::new();

        for (kw_name, kw_val) in kw_pairs {
            if let Some(pos) = func.params.iter().position(|p| p == &kw_name) {
                if bound[pos].is_some() {
                    return Err(Error::Runtime(format!(
                        "{}() got multiple values for argument '{}'",
                        name, kw_name
                    )));
                }
                bound[pos] = Some(kw_val);
            } else if has_kwarg {
                extra_kwargs.push((kw_name, kw_val));
            } else {
                return Err(Error::Runtime(format!(
                    "{}() got an unexpected keyword argument '{}'",
                    name, kw_name
                )));
            }
        }

        // Step 3: Fill missing params from defaults
        #[allow(clippy::needless_range_loop)]
        for i in 0..n_params {
            if bound[i].is_none() {
                let default_idx = i as isize - n_required as isize;
                if default_idx >= 0 && (default_idx as usize) < n_defaults {
                    bound[i] = Some(func.defaults[default_idx as usize].clone());
                } else {
                    return Err(Error::Runtime(format!(
                        "{}() missing required argument: '{}'",
                        name, func.params[i]
                    )));
                }
            }
        }

        // Check recursion limit before pushing a new frame
        if let Some(limit) = self.recursion_limit
            && frames.len() >= limit
        {
            return Err(Error::RecursionLimitExceeded(limit));
        }

        // Build locals from parameters
        let mut locals = HashMap::new();
        for (param, val) in func.params.iter().zip(bound.into_iter()) {
            locals.insert(param.clone(), val.unwrap());
        }

        // Bind *args (tuple in Python)
        if let Some(ref vararg_name) = func.vararg {
            locals.insert(vararg_name.clone(), PyValue::Tuple(extra_positional));
        }

        // Bind **kwargs
        if let Some(ref kwarg_name) = func.kwarg {
            let kwargs_pairs: Vec<(PyValue, PyValue)> = extra_kwargs
                .into_iter()
                .map(|(k, v)| (PyValue::Str(k), v))
                .collect();
            locals.insert(kwarg_name.clone(), PyValue::Dict(kwargs_pairs));
        }

        let new_frame = CallFrame {
            code: func.code.clone(),
            ip: 0,
            locals,
            stack_base: self.stack.len(),
            iterators: Vec::new(),
        };
        frames.push(new_frame);
        Ok(())
    }

    /// Call a callable value on the stack.
    ///
    /// Pops keyword pairs, positional args, and the callable from the stack.
    /// Dispatches to `invoke_function_def` for `PyValue::Function`.
    fn call_value(&mut self, frames: &mut Vec<CallFrame>, n_pos: usize, n_kw: usize) -> Result<()> {
        // Pop keyword args (name, value pairs) in reverse
        let mut kw_pairs: Vec<(String, PyValue)> = Vec::with_capacity(n_kw);
        for _ in 0..n_kw {
            let value = self.stack.pop().unwrap_or(PyValue::None);
            let key_val = self.stack.pop().unwrap_or(PyValue::None);
            let key = match key_val {
                PyValue::Str(s) => s,
                _ => {
                    return Err(Error::Runtime(
                        "keyword argument name must be a string".to_string(),
                    ));
                }
            };
            kw_pairs.push((key, value));
        }
        kw_pairs.reverse();

        // Pop positional args
        let start = self.stack.len() - n_pos;
        let pos_args: Vec<PyValue> = self.stack.drain(start..).collect();

        // Pop the callable
        let callable = self.stack.pop().unwrap_or(PyValue::None);

        match callable {
            PyValue::Function(func) => {
                let name = func.name.clone();
                self.invoke_function_def(frames, &func, &name, pos_args, kw_pairs)
            }
            PyValue::NativeFunction(key) => {
                if let Some(tool) = self.tools.get(&key).cloned() {
                    let result = (tool.func)(pos_args);
                    self.stack.push(result);
                    Ok(())
                } else {
                    Err(Error::Runtime(format!(
                        "TypeError: native function '{}' not found in tools",
                        key
                    )))
                }
            }
            other => Err(Error::Runtime(format!(
                "TypeError: '{}' object is not callable",
                other.type_name()
            ))),
        }
    }

    /// Synchronously invoke a function and return its result.
    ///
    /// Creates a fresh frame stack, runs the function to completion, and
    /// returns the result. Used by builtins like `sorted(key=...)`, `map`,
    /// and `filter` that need to call user functions during execution.
    fn invoke_sync(
        &mut self,
        func: &FunctionDef,
        args: Vec<PyValue>,
        outer_frames: &mut [CallFrame],
    ) -> Result<PyValue> {
        let mut locals = HashMap::new();
        for (param, val) in func.params.iter().zip(args) {
            locals.insert(param.clone(), val);
        }
        let frame = CallFrame {
            code: func.code.clone(),
            ip: 0,
            locals,
            stack_base: self.stack.len(),
            iterators: Vec::new(),
        };

        // Check recursion limit (count outer frames + 1 for the new frame)
        if let Some(limit) = self.recursion_limit
            && (outer_frames.len() + 1) >= limit
        {
            return Err(Error::RecursionLimitExceeded(limit));
        }

        let mut frames = vec![frame];
        self.run(&mut frames)
    }

    // -----------------------------------------------------------------------
    // Callable-aware builtins
    // -----------------------------------------------------------------------

    /// `sorted(iterable, key=None, reverse=False)`
    fn builtin_sorted(
        &mut self,
        frames: &mut [CallFrame],
        pos_args: Vec<PyValue>,
        kw_pairs: Vec<(String, PyValue)>,
    ) -> Result<PyValue> {
        if pos_args.is_empty() || pos_args.len() > 1 {
            return Err(Error::Runtime(
                "sorted() requires exactly 1 positional argument".to_string(),
            ));
        }

        let mut items = match &pos_args[0] {
            PyValue::List(items) | PyValue::Tuple(items) | PyValue::Set(items) => items.clone(),
            PyValue::Dict(pairs) => pairs.iter().map(|(k, _)| k.clone()).collect(),
            PyValue::Str(s) => s.chars().map(|c| PyValue::Str(c.to_string())).collect(),
            other => {
                return Err(Error::Type {
                    expected: "iterable".to_string(),
                    got: other.type_name().to_string(),
                });
            }
        };

        let mut key_func: Option<FunctionDef> = None;
        let mut reverse = false;

        for (kw_name, kw_val) in kw_pairs {
            match kw_name.as_str() {
                "key" => match kw_val {
                    PyValue::Function(f) => key_func = Some(*f),
                    PyValue::None => {}
                    other => {
                        return Err(Error::Runtime(format!(
                            "TypeError: '{}' object is not callable",
                            other.type_name()
                        )));
                    }
                },
                "reverse" => {
                    reverse = kw_val.is_truthy();
                }
                other => {
                    return Err(Error::Runtime(format!(
                        "sorted() got an unexpected keyword argument '{}'",
                        other
                    )));
                }
            }
        }

        if let Some(ref func) = key_func {
            // Compute keys for each item
            let mut keyed: Vec<(PyValue, PyValue)> = Vec::with_capacity(items.len());
            for item in items {
                let key = self.invoke_sync(func, vec![item.clone()], frames)?;
                keyed.push((key, item));
            }
            keyed.sort_by(|(a, _), (b, _)| compare_for_sort(a, b));
            items = keyed.into_iter().map(|(_, item)| item).collect();
        } else {
            items.sort_by(compare_for_sort);
        }

        if reverse {
            items.reverse();
        }

        Ok(PyValue::List(items))
    }

    /// `map(func, iterable)` — apply func to each item, return list.
    fn builtin_map(&mut self, frames: &mut [CallFrame], pos_args: Vec<PyValue>) -> Result<PyValue> {
        if pos_args.len() != 2 {
            return Err(Error::Runtime(
                "map() requires exactly 2 arguments".to_string(),
            ));
        }
        let func = match &pos_args[0] {
            PyValue::Function(f) => (**f).clone(),
            other => {
                return Err(Error::Runtime(format!(
                    "TypeError: '{}' object is not callable",
                    other.type_name()
                )));
            }
        };

        let items = match &pos_args[1] {
            PyValue::List(items) | PyValue::Tuple(items) | PyValue::Set(items) => items.clone(),
            PyValue::Dict(pairs) => pairs.iter().map(|(k, _)| k.clone()).collect(),
            PyValue::Str(s) => s.chars().map(|c| PyValue::Str(c.to_string())).collect(),
            other => {
                return Err(Error::Type {
                    expected: "iterable".to_string(),
                    got: other.type_name().to_string(),
                });
            }
        };

        let mut results = Vec::with_capacity(items.len());
        for item in items {
            let result = self.invoke_sync(&func, vec![item], frames)?;
            results.push(result);
        }

        Ok(PyValue::List(results))
    }

    /// `filter(func_or_none, iterable)` — keep items where func returns truthy.
    fn builtin_filter(
        &mut self,
        frames: &mut [CallFrame],
        pos_args: Vec<PyValue>,
    ) -> Result<PyValue> {
        if pos_args.len() != 2 {
            return Err(Error::Runtime(
                "filter() requires exactly 2 arguments".to_string(),
            ));
        }

        let items = match &pos_args[1] {
            PyValue::List(items) | PyValue::Tuple(items) | PyValue::Set(items) => items.clone(),
            PyValue::Dict(pairs) => pairs.iter().map(|(k, _)| k.clone()).collect(),
            PyValue::Str(s) => s.chars().map(|c| PyValue::Str(c.to_string())).collect(),
            other => {
                return Err(Error::Type {
                    expected: "iterable".to_string(),
                    got: other.type_name().to_string(),
                });
            }
        };

        let mut results = Vec::new();

        match &pos_args[0] {
            PyValue::Function(func) => {
                let func = (**func).clone();
                for item in items {
                    let result = self.invoke_sync(&func, vec![item.clone()], frames)?;
                    if result.is_truthy() {
                        results.push(item);
                    }
                }
            }
            PyValue::None => {
                for item in items {
                    if item.is_truthy() {
                        results.push(item);
                    }
                }
            }
            other => {
                return Err(Error::Runtime(format!(
                    "TypeError: '{}' object is not callable",
                    other.type_name()
                )));
            }
        }

        Ok(PyValue::List(results))
    }

    // -----------------------------------------------------------------------
    // File I/O builtins
    // -----------------------------------------------------------------------

    /// Implement the `open()` builtin for mounted virtual files.
    fn builtin_open(&mut self, args: Vec<PyValue>) -> Result<PyValue> {
        let path = match args.first() {
            Some(PyValue::Str(s)) => s.clone(),
            _ => {
                return Err(Error::Runtime(
                    "TypeError: open() argument must be a string".to_string(),
                ));
            }
        };

        let mode = match args.get(1) {
            Some(PyValue::Str(s)) => s.clone(),
            None => "r".to_string(),
            _ => {
                return Err(Error::Runtime(
                    "TypeError: open() mode must be a string".to_string(),
                ));
            }
        };

        let entry = match self.mounts.get(&path) {
            Some(e) => e.clone(),
            None => {
                return Err(Error::Runtime(format!(
                    "FileNotFoundError: [Errno 2] No such file or directory: '{}'",
                    path
                )));
            }
        };

        let write_mode = mode.contains('w') || mode.contains('a');

        if write_mode && !entry.writable {
            return Err(Error::Runtime(format!(
                "PermissionError: [Errno 13] Permission denied: '{}'",
                path
            )));
        }

        let handle = self.next_file_handle;
        self.next_file_handle += 1;

        let buffer = if write_mode {
            String::new()
        } else {
            entry.content.clone()
        };

        self.open_files.insert(
            handle,
            FileState {
                virtual_path: path,
                buffer,
                cursor: 0,
                write_mode,
                closed: false,
            },
        );

        Ok(PyValue::File(handle))
    }

    /// Dispatch a method call on a file handle.
    fn call_file_method(
        &mut self,
        handle: u64,
        method: &str,
        args: Vec<PyValue>,
    ) -> Result<PyValue> {
        // Check if handle exists
        let file = match self.open_files.get(&handle) {
            Some(f) => f,
            None => {
                return Err(Error::Runtime(
                    "ValueError: I/O operation on closed file".to_string(),
                ));
            }
        };

        if file.closed {
            return Err(Error::Runtime(
                "ValueError: I/O operation on closed file".to_string(),
            ));
        }

        match method {
            "read" => {
                if file.write_mode {
                    return Err(Error::Runtime(
                        "UnsupportedOperation: not readable".to_string(),
                    ));
                }
                let content = file.buffer[file.cursor..].to_string();
                let len = file.buffer.len();
                self.open_files.get_mut(&handle).unwrap().cursor = len;
                Ok(PyValue::Str(content))
            }
            "readline" => {
                if file.write_mode {
                    return Err(Error::Runtime(
                        "UnsupportedOperation: not readable".to_string(),
                    ));
                }
                let remaining = &file.buffer[file.cursor..];
                let line = if let Some(pos) = remaining.find('\n') {
                    remaining[..=pos].to_string()
                } else {
                    remaining.to_string()
                };
                let advance = line.len();
                self.open_files.get_mut(&handle).unwrap().cursor += advance;
                Ok(PyValue::Str(line))
            }
            "readlines" => {
                if file.write_mode {
                    return Err(Error::Runtime(
                        "UnsupportedOperation: not readable".to_string(),
                    ));
                }
                let remaining = file.buffer[file.cursor..].to_string();
                let len = file.buffer.len();
                self.open_files.get_mut(&handle).unwrap().cursor = len;
                let lines: Vec<PyValue> = if remaining.is_empty() {
                    Vec::new()
                } else {
                    remaining
                        .split_inclusive('\n')
                        .map(|line| PyValue::Str(line.to_string()))
                        .collect()
                };
                Ok(PyValue::List(lines))
            }
            "write" => {
                if !file.write_mode {
                    return Err(Error::Runtime(
                        "UnsupportedOperation: not writable".to_string(),
                    ));
                }
                let text = match args.first() {
                    Some(PyValue::Str(s)) => s.clone(),
                    _ => {
                        return Err(Error::Runtime(
                            "TypeError: write() argument must be a string".to_string(),
                        ));
                    }
                };
                let count = text.len() as i64;
                let vpath = file.virtual_path.clone();

                let file_mut = self.open_files.get_mut(&handle).unwrap();
                file_mut.buffer.push_str(&text);

                // Write-through: update mount content
                let new_content = file_mut.buffer.clone();
                if let Some(mount) = self.mounts.get_mut(&vpath) {
                    mount.content = new_content.clone();
                    // Write to host path
                    let _ = std::fs::write(&mount.host_path, &new_content);
                }

                Ok(PyValue::Int(count))
            }
            "close" => {
                let vpath = file.virtual_path.clone();
                let is_write = file.write_mode;
                let file_mut = self.open_files.get_mut(&handle).unwrap();
                file_mut.closed = true;

                // Final flush on close for write mode
                if is_write {
                    let content = file_mut.buffer.clone();
                    if let Some(mount) = self.mounts.get_mut(&vpath) {
                        mount.content = content.clone();
                        let _ = std::fs::write(&mount.host_path, &content);
                    }
                }

                Ok(PyValue::None)
            }
            _ => Err(Error::Runtime(format!(
                "AttributeError: '_io.TextIOWrapper' object has no attribute '{}'",
                method
            ))),
        }
    }

    /// Call a registered tool, mapping keyword arguments and validating types.
    fn call_tool(
        &self,
        name: &str,
        tool: &RegisteredTool,
        pos_args: Vec<PyValue>,
        kw_pairs: &[(String, PyValue)],
        span: Span,
    ) -> Result<PyValue> {
        let mut final_args = pos_args;

        if !kw_pairs.is_empty() && !tool.arg_names.is_empty() {
            // Extend to accommodate all parameters
            let max_args = tool.arg_names.len();
            while final_args.len() < max_args {
                final_args.push(PyValue::None);
            }
            // Map each keyword to its parameter position
            for (kw_name, kw_val) in kw_pairs {
                if let Some(pos) = tool.arg_names.iter().position(|n| n == kw_name) {
                    if pos < final_args.len() {
                        final_args[pos] = kw_val.clone();
                    }
                } else {
                    let signature = tool.arg_names.join(", ");
                    let source = frames_source(name, span);
                    return Err(Error::Diagnostic(
                        Diagnostic::new(format!(
                            "`{}()` got an unexpected keyword argument `{}`",
                            name, kw_name
                        ))
                        .with_source(source)
                        .with_label(span, "unexpected argument")
                        .with_note(format!("function signature: {}({})", name, signature))
                        .with_help(format!(
                            "valid arguments are: {}",
                            tool.arg_names.join(", ")
                        )),
                    ));
                }
            }
        } else if !kw_pairs.is_empty() {
            // Tool has no arg_names — append keyword values in order (fallback)
            for (_, kw_val) in kw_pairs {
                final_args.push(kw_val.clone());
            }
        }

        // Validate argument types if the tool has info
        if let Some(ref info) = tool.info {
            for (i, (arg, arg_info)) in final_args.iter().zip(info.args.iter()).enumerate() {
                if !arg_info.required && matches!(arg, PyValue::None) {
                    continue;
                }
                if let Some(err) =
                    self.validate_arg_type(name, tool, i, arg, &arg_info.python_type, span)
                {
                    return Err(err);
                }
            }
        }

        Ok((tool.func)(final_args))
    }

    // -----------------------------------------------------------------------
    // Method call dispatch
    // -----------------------------------------------------------------------

    /// Call a non-mutating method on the object at TOS.
    ///
    /// Stack layout: `[args..., object]`. Pops the object and args, calls
    /// the appropriate method handler, pushes the result.
    fn call_method(
        &mut self,
        frames: &mut Vec<CallFrame>,
        method: &str,
        n_args: usize,
    ) -> Result<()> {
        // The object is on the stack below the args
        // Stack: [... object, arg0, arg1, ...]
        // We need to pop args first, then the object
        let args_start = self.stack.len() - n_args;
        let args: Vec<PyValue> = self.stack.drain(args_start..).collect();
        let object = self.stack.pop().unwrap_or(PyValue::None);

        // File handle methods — dispatch before type-based dispatch
        if let PyValue::File(handle) = &object {
            let result = self.call_file_method(*handle, method, args)?;
            self.stack.push(result);
            return Ok(());
        }

        match &object {
            PyValue::Module { name, attrs } => {
                // Look up method in module attrs
                let attr_val = attrs
                    .iter()
                    .find(|(k, _)| k == method)
                    .map(|(_, v)| v.clone());
                match attr_val {
                    Some(PyValue::NativeFunction(key)) => {
                        if let Some(tool) = self.tools.get(&key).cloned() {
                            let result = (tool.func)(args);
                            self.stack.push(result);
                            return Ok(());
                        }
                        return Err(Error::Runtime(format!(
                            "AttributeError: module '{}' function '{}' not found in tools",
                            name, method
                        )));
                    }
                    Some(PyValue::Function(func)) => {
                        let func_name = func.name.clone();
                        return self.invoke_function_def(
                            frames,
                            &func,
                            &func_name,
                            args,
                            Vec::new(),
                        );
                    }
                    Some(_) => {
                        return Err(Error::Runtime(format!(
                            "TypeError: '{}' attribute '{}' is not callable",
                            name, method
                        )));
                    }
                    None => {
                        return Err(Error::Runtime(format!(
                            "AttributeError: module '{}' has no attribute '{}'",
                            name, method
                        )));
                    }
                }
            }
            _ => {}
        }

        let result = match &object {
            PyValue::Str(s) => methods::call_str_method(s, method, args),
            PyValue::List(items) => methods::call_list_method(items, method, args),
            PyValue::Tuple(items) => methods::call_tuple_method(items, method, args),
            PyValue::Dict(pairs) => methods::call_dict_method(pairs, method, args),
            PyValue::Set(items) => methods::call_set_method(items, method, args),
            _ => Err(Error::Unsupported(format!(
                "Method '{}' not supported on type '{}'",
                method,
                object.type_name()
            ))),
        }?;

        self.stack.push(result);
        Ok(())
    }

    /// Call a mutating method on a named variable.
    ///
    /// Stack layout: `[args...]`. Looks up the variable by name, gets a
    /// mutable reference, and delegates to `mutate_list` or `mutate_dict`.
    fn call_mut_method(
        &mut self,
        frames: &mut [CallFrame],
        var_name: &str,
        method: &str,
        n_args: usize,
    ) -> Result<()> {
        let start = self.stack.len() - n_args;
        let args: Vec<PyValue> = self.stack.drain(start..).collect();

        let var = self.lookup_var_mut(frames, var_name)?;

        let result = match var {
            PyValue::List(items) => methods::mutate_list(items, method, args),
            PyValue::Dict(pairs) => methods::mutate_dict(pairs, method, args),
            PyValue::Set(items) => methods::mutate_set(items, method, args),
            PyValue::Tuple(_) => {
                return Err(Error::Runtime(format!(
                    "TypeError: 'tuple' object has no attribute '{}'",
                    method
                )));
            }
            _ => Err(Error::Unsupported(format!(
                "Mutating method '{}' not supported on type '{}'",
                method,
                var.type_name()
            ))),
        }?;

        self.stack.push(result);
        Ok(())
    }

    /// Call a mutating method on a named variable with keyword arguments.
    fn call_mut_method_kw(
        &mut self,
        frames: &mut [CallFrame],
        var_name: &str,
        method: &str,
        n_pos: usize,
        n_kw: usize,
    ) -> Result<()> {
        // Pop keyword args (name, value pairs) in reverse
        let mut kw_pairs: Vec<(String, PyValue)> = Vec::with_capacity(n_kw);
        for _ in 0..n_kw {
            let value = self.stack.pop().unwrap_or(PyValue::None);
            let key_val = self.stack.pop().unwrap_or(PyValue::None);
            let key = match key_val {
                PyValue::Str(s) => s,
                _ => {
                    return Err(Error::Runtime(
                        "keyword argument name must be a string".to_string(),
                    ));
                }
            };
            kw_pairs.push((key, value));
        }
        kw_pairs.reverse();

        // Pop positional args
        let start = self.stack.len() - n_pos;
        let pos_args: Vec<PyValue> = self.stack.drain(start..).collect();

        // Check if this is list.sort with kwargs
        if method == "sort" {
            // Check the variable is a list
            let is_list = {
                let var = self.lookup_var_mut(frames, var_name)?;
                matches!(var, PyValue::List(_))
            };
            if is_list {
                let result = self.sort_list_in_place(frames, var_name, pos_args, kw_pairs)?;
                self.stack.push(result);
                return Ok(());
            }
        }

        // Fall through to regular mutating method (ignore kwargs)
        let var = self.lookup_var_mut(frames, var_name)?;
        let result = match var {
            PyValue::List(items) => methods::mutate_list(items, method, pos_args),
            PyValue::Dict(pairs) => methods::mutate_dict(pairs, method, pos_args),
            PyValue::Set(items) => methods::mutate_set(items, method, pos_args),
            _ => Err(Error::Unsupported(format!(
                "Mutating method '{}' not supported on type '{}'",
                method,
                var.type_name()
            ))),
        }?;

        self.stack.push(result);
        Ok(())
    }

    /// Sort a list in place with optional `key` and `reverse` kwargs.
    fn sort_list_in_place(
        &mut self,
        frames: &mut [CallFrame],
        var_name: &str,
        _pos_args: Vec<PyValue>,
        kw_pairs: Vec<(String, PyValue)>,
    ) -> Result<PyValue> {
        let mut key_func: Option<FunctionDef> = None;
        let mut reverse = false;

        for (kw_name, kw_val) in kw_pairs {
            match kw_name.as_str() {
                "key" => match kw_val {
                    PyValue::Function(f) => key_func = Some(*f),
                    PyValue::None => {}
                    other => {
                        return Err(Error::Runtime(format!(
                            "TypeError: '{}' object is not callable",
                            other.type_name()
                        )));
                    }
                },
                "reverse" => {
                    reverse = kw_val.is_truthy();
                }
                other => {
                    return Err(Error::Runtime(format!(
                        "sort() got an unexpected keyword argument '{}'",
                        other
                    )));
                }
            }
        }

        // Extract items from the variable to avoid borrow conflicts with invoke_sync
        let mut items = {
            let var = self.lookup_var_mut(frames, var_name)?;
            match var {
                PyValue::List(items) => std::mem::take(items),
                _ => unreachable!(),
            }
        };

        if let Some(ref func) = key_func {
            // Compute keys for each item
            let mut keyed: Vec<(PyValue, PyValue)> = Vec::with_capacity(items.len());
            for item in items {
                let key = self.invoke_sync(func, vec![item.clone()], frames)?;
                keyed.push((key, item));
            }
            keyed.sort_by(|(a, _), (b, _)| compare_for_sort(a, b));
            items = keyed.into_iter().map(|(_, item)| item).collect();
        } else {
            items.sort_by(compare_for_sort);
        }

        if reverse {
            items.reverse();
        }

        // Write sorted items back
        let var = self.lookup_var_mut(frames, var_name)?;
        if let PyValue::List(list_items) = var {
            *list_items = items;
        }

        Ok(PyValue::None)
    }

    // -----------------------------------------------------------------------
    // Variable lookup helpers
    // -----------------------------------------------------------------------

    /// Get a mutable reference to a variable, checking locals then globals.
    fn lookup_var_mut<'a>(
        &'a mut self,
        frames: &'a mut [CallFrame],
        name: &str,
    ) -> Result<&'a mut PyValue> {
        // Check locals in the current frame first
        if let Some(frame) = frames.last_mut()
            && frame.locals.contains_key(name)
        {
            return Ok(frame.locals.get_mut(name).unwrap());
        }
        // Then globals
        if self.globals.contains_key(name) {
            return Ok(self.globals.get_mut(name).unwrap());
        }
        Err(Error::NameError(name.to_string()))
    }

    // -----------------------------------------------------------------------
    // Type validation (for tool calls with rich diagnostics)
    // -----------------------------------------------------------------------

    /// Validate an argument's type against the expected type string.
    /// Returns `Some(Error)` on mismatch, `None` if valid.
    fn validate_arg_type(
        &self,
        func_name: &str,
        tool: &RegisteredTool,
        arg_index: usize,
        value: &PyValue,
        expected_type: &str,
        span: Span,
    ) -> Option<Error> {
        let actual_type = value.type_name();

        let is_compatible = match expected_type {
            "any" => true,
            "str" => matches!(value, PyValue::Str(_)),
            "int" => matches!(value, PyValue::Int(_)),
            "float" => matches!(value, PyValue::Float(_) | PyValue::Int(_)),
            "bool" => matches!(value, PyValue::Bool(_)),
            "list" => matches!(value, PyValue::List(_)),
            "tuple" => matches!(value, PyValue::Tuple(_)),
            "dict" => matches!(value, PyValue::Dict(_)),
            "set" => matches!(value, PyValue::Set(_)),
            "number" => matches!(value, PyValue::Int(_) | PyValue::Float(_)),
            _ => true,
        };

        if !is_compatible {
            let default_name = format!("arg{}", arg_index);
            let arg_name = tool
                .arg_names
                .get(arg_index)
                .map(|s| s.as_str())
                .unwrap_or(&default_name);

            let signature = if let Some(ref info) = tool.info {
                info.args
                    .iter()
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

            Some(Error::Diagnostic(
                Diagnostic::new(format!("type mismatch in call to `{}`", func_name))
                    .with_label(
                        span,
                        format!("expected `{}`, found `{}`", expected_type, actual_type),
                    )
                    .with_note(format!(
                        "parameter `{}` of `{}()` expects type `{}`",
                        arg_name, func_name, expected_type
                    ))
                    .with_note(format!("function signature: {}({})", func_name, signature))
                    .with_help(format!(
                        "the value `{}` has type `{}`, but `{}` is required",
                        value.to_print_string(),
                        actual_type,
                        expected_type
                    )),
            ))
        } else {
            None
        }
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper: produce an empty source string for diagnostic messages.
/// (The real source is in the CodeObject, but we don't thread it here.)
fn frames_source(_name: &str, _span: Span) -> String {
    String::new()
}

/// Map an internal `Error` to a Python exception type name.
fn error_to_exception_type(err: &Error) -> &'static str {
    match err {
        Error::Type { .. } => "TypeError",
        Error::DivisionByZero => "ZeroDivisionError",
        Error::NameError(_) => "NameError",
        Error::Runtime(msg) => {
            if msg.starts_with("ValueError") {
                "ValueError"
            } else if msg.starts_with("ZeroDivisionError") {
                "ZeroDivisionError"
            } else if msg.starts_with("TypeError") {
                "TypeError"
            } else if msg.starts_with("NameError") {
                "NameError"
            } else if msg.starts_with("ModuleNotFoundError") {
                "ModuleNotFoundError"
            } else if msg.starts_with("AttributeError") {
                "AttributeError"
            } else if msg.contains("index out of range") {
                "IndexError"
            } else if msg.contains("KeyError") {
                "KeyError"
            } else if msg.starts_with("FileNotFoundError") {
                "FileNotFoundError"
            } else if msg.starts_with("PermissionError") {
                "PermissionError"
            } else if msg.starts_with("UnsupportedOperation") {
                "UnsupportedOperation"
            } else if msg.starts_with("AssertionError") {
                "AssertionError"
            } else {
                "RuntimeError"
            }
        }
        Error::InstructionLimitExceeded(_) => "InstructionLimitExceeded",
        Error::RecursionLimitExceeded(_) => "RecursionLimitExceeded",
        Error::Parse(_) => "SyntaxError",
        Error::Unsupported(_) => "RuntimeError",
        Error::Diagnostic(_) => "RuntimeError",
    }
}

/// Check whether an error is uncatchable (resource limits).
fn is_uncatchable(err: &Error) -> bool {
    matches!(
        err,
        Error::InstructionLimitExceeded(_) | Error::RecursionLimitExceeded(_)
    )
}

/// Check if an exception type matches a handler type.
///
/// `Exception` and `BaseException` match everything. Otherwise the types
/// must match exactly.
fn exception_matches(actual: &str, expected: &str) -> bool {
    matches!(expected, "Exception" | "BaseException") || actual == expected
}

/// Compare two PyValues for sorting (used by `sorted()`).
fn compare_for_sort(a: &PyValue, b: &PyValue) -> std::cmp::Ordering {
    match (a, b) {
        (PyValue::Int(x), PyValue::Int(y)) => x.cmp(y),
        (PyValue::Float(x), PyValue::Float(y)) => {
            x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
        }
        (PyValue::Int(x), PyValue::Float(y)) => (*x as f64)
            .partial_cmp(y)
            .unwrap_or(std::cmp::Ordering::Equal),
        (PyValue::Float(x), PyValue::Int(y)) => x
            .partial_cmp(&(*y as f64))
            .unwrap_or(std::cmp::Ordering::Equal),
        (PyValue::Str(x), PyValue::Str(y)) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    }
}

/// Search the exception table for a handler covering the given instruction index.
///
/// Returns the last (innermost) matching entry, since nested try blocks
/// produce entries that appear later in the table.
fn find_handler(table: &[ExceptionEntry], ip: usize) -> Option<ExceptionEntry> {
    let ip = ip as u32;
    // Search in reverse so inner handlers take precedence
    table
        .iter()
        .rev()
        .find(|e| ip >= e.start && ip < e.end)
        .cloned()
}
