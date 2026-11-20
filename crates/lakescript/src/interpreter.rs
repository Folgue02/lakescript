//! The VM: runs the bytecode the compiler writes *(see `ideas/tooling/vm.md`)*.
//!
//! Frames live on the heap, so how deep the calls of a script go doesn't
//! depend on Rust's stack. Rust's stack is only used when Rust code calls a
//! script's function (a method like `.map()` calling its function), and each
//! of those checks the stack budget.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::{BinaryOp, UnaryOp};
use crate::bytecode::{ArgKind, Capture, Op, Proto, for_kind};
use crate::command::{CommandKind, CommandValue, shell_quote};
use crate::compiler::Compiler;
use crate::diagnostic::Diagnostic;
use crate::engine::Outcome;
use crate::exceptions;
use crate::host::Host;
use crate::methods;
use crate::modules::{Globals, Module, ModuleSlot, ModuleSource};
use crate::patterns;
use crate::stack::position as stack_position;
use crate::token::Span;
use crate::types;
use crate::value::{ExceptionValue, Function, Key, NativeContext, NativeError, Range, RegexValue, ResultValue, UserFunction, Value};

/// Why the normal flow stopped, when it crosses Rust code.
pub enum Flow {
    /// An exception was thrown.
    Throw(Rc<ExceptionValue>),
    /// `exit()` was called with this code.
    Exit(i32),
}

pub type Exec<T> = Result<T, Flow>;

/// Arguments given by name: the name, the value, and where it is.
pub type Named = Vec<(String, Value, Span)>;

/// Throws a generic `Exception`.
pub fn error<T>(message: impl Into<String>, span: Span) -> Exec<T> {
    Err(flow_error(message, span))
}

/// Throws one of the built-in exceptions *(i.e. `TypeError`)*.
pub fn error_as<T>(kind: &str, message: impl Into<String>, span: Span) -> Exec<T> {
    Err(flow_error_as(kind, message, span))
}

pub fn flow_error(message: impl Into<String>, span: Span) -> Flow {
    flow_error_as("Exception", message, span)
}

pub fn flow_error_as(kind: &str, message: impl Into<String>, span: Span) -> Flow {
    Flow::Throw(exceptions::new(kind, message, span))
}

/// The error of an operator: dividing by zero, or the wrong types.
pub fn op_error(message: String, span: Span) -> Flow {
    let kind = if message.contains("by zero") { exceptions::ARITHMETIC_ERROR } else { exceptions::TYPE_ERROR };
    flow_error_as(kind, message, span)
}

fn index_error(message: String, span: Span) -> Flow {
    let kind = if message.starts_with("index") { exceptions::INDEX_ERROR } else { exceptions::TYPE_ERROR };
    flow_error_as(kind, message, span)
}

/// How an uncaught exception is reported.
pub fn uncaught(exception: &ExceptionValue) -> Diagnostic {
    let message = if &*exception.kind == "Exception" {
        exception.message.clone()
    } else {
        format!("{}: {}", exception.kind, exception.message)
    };
    Diagnostic::new(message, exception.span)
}

const TOO_DEEP: &str = "too many nested calls. Is there a recursion that never ends?";

/// A call of a script's function that is running.
struct Frame {
    function: Rc<Function>,
    /// The next instruction, while this frame isn't the one running.
    ip: usize,
    /// Where its slots start in the stack. The function called is just below.
    base: usize,
    cells: Vec<Rc<RefCell<Value>>>,
    /// Which parameters were given, one bit each.
    given: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandlerKind {
    Catch,
    Finally,
    TryExpr,
}

/// A `try` that is running.
struct Handler {
    kind: HandlerKind,
    /// Where to go when something is thrown.
    ip: usize,
    frame: usize,
    /// The height of the stack when it started.
    sp: usize,
    /// For a `finally`: the slot where the exception or `exit()` waits.
    slot: usize,
}

pub struct Interpreter<'a> {
    /// Output, input, modules, the script's folder and environment...
    pub host: &'a mut Host,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    handlers: Vec<Handler>,
    /// Where Rust's stack was when the script started, and how much of it
    /// the script may use, for the calls from Rust code.
    stack_base: usize,
    stack_budget: usize,
    /// How many calls can be nested.
    max_frames: usize,
    /// What a frame's cells hold before their variable is declared.
    empty_cell: Rc<RefCell<Value>>,
}

impl<'a> Interpreter<'a> {
    pub fn new(host: &'a mut Host, stack_budget: usize) -> Self {
        Interpreter {
            host,
            stack: Vec::with_capacity(256),
            frames: Vec::with_capacity(16),
            handlers: Vec::new(),
            stack_base: stack_position(),
            stack_budget,
            max_frames: (stack_budget / 64).clamp(256, 100_000),
            empty_cell: Rc::new(RefCell::new(Value::Null)),
        }
    }

    /// What native functions and commands can use.
    pub fn context(&mut self) -> NativeContext<'_> {
        let host = &mut *self.host;
        NativeContext {
            output: &mut *host.output,
            error: &mut *host.error,
            input: &mut *host.input,
            input_piped: host.input_piped,
            inherit_stdio: host.inherit_stdio,
            exit_allowed: host.exit_allowed,
            state: &mut host.state,
            exit: None,
        }
    }

    /// Compiles a regex the first time it's used.
    pub fn regex(&mut self, pattern: &str, flags: &str) -> Result<Rc<RegexValue>, String> {
        let key = format!("{flags}/{pattern}");
        if let Some(regex) = self.host.regexes.get(&key) {
            return Ok(regex.clone());
        }
        let source = if flags.is_empty() { pattern.to_string() } else { format!("(?{flags}){pattern}") };
        let compiled = regex::Regex::new(&source).map_err(|e| format!("invalid regex: {}", regex_error(&e)))?;
        let value = Rc::new(RegexValue { pattern: pattern.to_string(), flags: flags.to_string(), compiled });
        self.host.regexes.insert(key, value.clone());
        Ok(value)
    }

    /// A regex from a `regex` value, or from a string that is compiled as one.
    pub fn as_regex(&mut self, value: &Value) -> Result<Rc<RegexValue>, String> {
        match value {
            Value::Regex(r) => Ok(r.clone()),
            Value::Str(s) => self.regex(s, ""),
            other => Err(format!("expected a regex or a string, not a {}", other.type_name())),
        }
    }

    /// Runs a compiled script.
    pub fn run(&mut self, proto: Rc<Proto>, globals: Rc<Globals>) -> Result<Outcome, Diagnostic> {
        let script = Rc::new(Function::User(UserFunction { proto, upvalues: Vec::new(), globals }));
        match self.call(&script, Vec::new(), Vec::new(), Span::default()) {
            Ok(_) => Ok(Outcome::Finished),
            Err(Flow::Exit(code)) => Ok(Outcome::Exit(code)),
            Err(Flow::Throw(exception)) => Err(uncaught(&exception)),
        }
    }

    // ---- Calls ----

    /// Calls a function from Rust code, and runs it to the end.
    pub fn call(
        &mut self,
        function: &Rc<Function>,
        positional: Vec<Value>,
        named: Named,
        span: Span,
    ) -> Exec<Value> {
        if let Function::Native(native) = &**function {
            return self.call_native(native, positional, named, span);
        }
        if stack_position().abs_diff(self.stack_base) > self.stack_budget {
            return error(TOO_DEEP, span);
        }
        let height = self.stack.len();
        if let Err(e) = self.enter(function.clone(), positional, named, span) {
            self.stack.truncate(height);
            return Err(e);
        }
        let entry = self.frames.len() - 1;
        self.execute(entry)
    }

    /// Starts a call. A native function runs now and its value is pushed;
    /// for a script's function, a frame is pushed and `true` returned.
    fn enter(&mut self, function: Rc<Function>, positional: Vec<Value>, named: Named, span: Span) -> Exec<bool> {
        match &*function {
            Function::Native(native) => {
                let value = self.call_native(native, positional, named, span)?;
                self.stack.push(value);
                Ok(false)
            }
            Function::User(user) => {
                let (values, given) = bind(user, positional, named, span)?;
                self.stack.push(Value::Function(function.clone()));
                let base = self.stack.len();
                self.stack.extend(values);
                self.push_frame(function, base, given, span)?;
                Ok(true)
            }
        }
    }

    #[inline(always)]
    fn push_frame(&mut self, function: Rc<Function>, base: usize, given: u64, span: Span) -> Exec<()> {
        if self.frames.len() >= self.max_frames {
            return error(TOO_DEEP, span);
        }
        let Function::User(user) = &*function else { unreachable!("only a script's functions get frames") };
        self.stack.resize(base + user.proto.slots as usize, Value::Null);
        let cells = if user.proto.cells == 0 { Vec::new() } else { vec![self.empty_cell.clone(); user.proto.cells as usize] };
        self.frames.push(Frame { function, ip: 0, base, cells, given });
        Ok(())
    }

    /// The `Call` instruction: the function is `argc` values below the top.
    #[inline(always)]
    fn call_op(&mut self, argc: usize, span: Span) -> Exec<bool> {
        let callee = self.stack.len() - argc - 1;
        let function = match &self.stack[callee] {
            Value::Function(f) => f.clone(),
            other => return error_as(exceptions::TYPE_ERROR, format!("a {} can't be called", other.type_name()), span),
        };
        if let Function::User(user) = &*function {
            if user.proto.simple && argc == user.proto.params.len() {
                self.push_frame(function, callee + 1, u64::MAX, span)?;
                return Ok(true);
            }
        }
        let args = self.stack.split_off(callee + 1);
        self.stack.pop();
        self.enter(function, args, Vec::new(), span)
    }

    /// The `CallComplex` instruction, with named or spread arguments.
    fn call_complex(&mut self, kinds: &[ArgKind], span: Span) -> Exec<bool> {
        let callee = self.stack.len() - kinds.len() - 1;
        let function = match &self.stack[callee] {
            Value::Function(f) => f.clone(),
            other => return error_as(exceptions::TYPE_ERROR, format!("a {} can't be called", other.type_name()), span),
        };
        let (positional, named) = self.complex_args(kinds, callee + 1)?;
        self.stack.pop();
        self.enter(function, positional, named, span)
    }

    /// Takes the arguments of a complex call off the stack.
    fn complex_args(&mut self, kinds: &[ArgKind], start: usize) -> Exec<(Vec<Value>, Named)> {
        let values = self.stack.split_off(start);
        let mut positional = Vec::with_capacity(values.len());
        let mut named = Vec::new();
        for (kind, value) in kinds.iter().zip(values) {
            match kind {
                ArgKind::Positional => positional.push(value),
                ArgKind::Spread(span) => match value {
                    Value::List(list) => positional.extend(list.borrow().iter().cloned()),
                    other => return error(format!("only lists can be spread with `...`, not a {}", other.type_name()), *span),
                },
                ArgKind::Named(name, span) => named.push((name.to_string(), value, *span)),
            }
        }
        Ok((positional, named))
    }

    #[inline(never)]
    fn call_native(
        &mut self,
        native: &crate::value::NativeFunction,
        positional: Vec<Value>,
        named: Named,
        span: Span,
    ) -> Exec<Value> {
        let mut args = positional;
        for (name, value, arg_span) in named {
            let Some(i) = native.params.iter().position(|p| *p == name) else {
                return if native.params.is_empty() {
                    error_as(exceptions::TYPE_ERROR, format!("`{}` doesn't take named arguments", native.name), span)
                } else {
                    error_as(exceptions::TYPE_ERROR, format!("`{}` has no parameter named `{name}`", native.name), arg_span)
                };
            };
            if i < args.len() && !matches!(args[i], Value::Null) {
                return error_as(exceptions::TYPE_ERROR, format!("`{name}` was given twice"), arg_span);
            }
            while args.len() <= i {
                args.push(Value::Null);
            }
            args[i] = value;
        }
        let mut context = self.context();
        let result = (native.func)(&mut context, args);
        let exit = context.exit;
        match (result, exit) {
            (_, Some(code)) => Err(Flow::Exit(code)),
            (Ok(value), None) => Ok(value),
            (Err(e), None) => Err(native_error(e, span)),
        }
    }

    // ---- Running ----

    /// Runs until the frame `entry` returns, handling what's thrown.
    fn execute(&mut self, entry: usize) -> Exec<Value> {
        loop {
            match self.dispatch(entry) {
                Ok(value) => return Ok(value),
                Err(flow) => self.unwind(flow, entry)?,
            }
        }
    }

    /// Finds the `try` that handles what was thrown, and goes there. When
    /// there's none in the frames of this run, they're dropped and the error
    /// goes back to the Rust code that started it.
    fn unwind(&mut self, flow: Flow, entry: usize) -> Exec<()> {
        while let Some(handler) = self.handlers.last() {
            if handler.frame < entry {
                break;
            }
            let handler = self.handlers.pop().expect("there's a handler");
            if handler.kind != HandlerKind::Finally && matches!(flow, Flow::Exit(_)) {
                // `exit()` only runs the `finally`s
                continue;
            }
            self.frames.truncate(handler.frame + 1);
            self.stack.truncate(handler.sp);
            let frame = &mut self.frames[handler.frame];
            frame.ip = handler.ip;
            let base = frame.base;
            match (handler.kind, flow) {
                (HandlerKind::Finally, Flow::Throw(e)) => self.stack[base + handler.slot] = Value::Exception(e),
                (HandlerKind::Finally, Flow::Exit(code)) => self.stack[base + handler.slot] = Value::Int(code as i64),
                (_, Flow::Throw(e)) => self.stack.push(Value::Exception(e)),
                (_, Flow::Exit(_)) => unreachable!("skipped above"),
            }
            return Ok(());
        }
        let base = self.frames[entry].base;
        self.frames.truncate(entry);
        self.stack.truncate(base - 1);
        Err(flow)
    }

    fn pop(&mut self) -> Value {
        self.stack.pop().expect("the compiler keeps the stack balanced")
    }

    fn top(&mut self) -> &mut Value {
        self.stack.last_mut().expect("the compiler keeps the stack balanced")
    }

    fn code_of(&self, frame: usize) -> (Rc<Proto>, Rc<Globals>) {
        match &*self.frames[frame].function {
            Function::User(user) => (user.proto.clone(), user.globals.clone()),
            Function::Native(_) => unreachable!("only a script's functions get frames"),
        }
    }

    fn globals_of(&self, frame: usize) -> Rc<Globals> {
        match &*self.frames[frame].function {
            Function::User(user) => user.globals.clone(),
            Function::Native(_) => unreachable!("only a script's functions get frames"),
        }
    }

    fn upvalue(&self, frame: usize, i: u32) -> &Rc<RefCell<Value>> {
        match &*self.frames[frame].function {
            Function::User(user) => &user.upvalues[i as usize],
            Function::Native(_) => unreachable!("only a script's functions get frames"),
        }
    }

    /// Runs instructions until the frame `entry` returns or something is thrown.
    fn dispatch(&mut self, entry: usize) -> Exec<Value> {
        let mut fi = self.frames.len() - 1;
        let (mut proto, mut globals) = self.code_of(fi);
        let mut ip = self.frames[fi].ip;
        let mut base = self.frames[fi].base;

        macro_rules! span {
            () => {
                proto.spans[ip - 1]
            };
        }
        macro_rules! reload {
            () => {{
                fi = self.frames.len() - 1;
                (proto, globals) = self.code_of(fi);
                ip = self.frames[fi].ip;
                base = self.frames[fi].base;
            }};
        }
        macro_rules! arith {
            ($op:expr, $int:expr, $float:expr) => {{
                let r = self.pop();
                let l = self.top();
                let fast = match (&*l, &r) {
                    (Value::Int(a), Value::Int(b)) => $int(*a, *b),
                    (Value::Float(a), Value::Float(b)) => $float(*a, *b),
                    _ => None,
                };
                *l = match fast {
                    Some(v) => v,
                    None => arithmetic($op, l, &r).map_err(|m| op_error(m, span!()))?,
                };
            }};
        }
        macro_rules! compare_op {
            ($op:expr, $cmp:tt) => {{
                let r = self.pop();
                let l = self.top();
                *l = match (&*l, &r) {
                    (Value::Int(a), Value::Int(b)) => Value::Bool(a $cmp b),
                    _ => compare($op, l, &r).map_err(|m| op_error(m, span!()))?,
                };
            }};
        }

        loop {
            let op = proto.code[ip];
            ip += 1;
            match op {
                // ---- Values ----
                Op::Const(c) => self.stack.push(proto.consts[c as usize].clone()),
                Op::Int(n) => self.stack.push(Value::Int(n as i64)),
                Op::Null => self.stack.push(Value::Null),
                Op::True => self.stack.push(Value::Bool(true)),
                Op::False => self.stack.push(Value::Bool(false)),
                Op::Pop => {
                    self.pop();
                }
                Op::Dup => {
                    let v = self.top().clone();
                    self.stack.push(v);
                }

                // ---- Variables ----
                Op::LoadLocal(s) => {
                    let v = self.stack[base + s as usize].clone();
                    self.stack.push(v);
                }
                Op::StoreLocal(s) => {
                    let v = self.pop();
                    self.stack[base + s as usize] = v;
                }
                Op::LoadCell(c) => {
                    let v = self.frames[fi].cells[c as usize].borrow().clone();
                    self.stack.push(v);
                }
                Op::StoreCell(c) => {
                    let v = self.pop();
                    *self.frames[fi].cells[c as usize].borrow_mut() = v;
                }
                Op::DeclareCell(c) => {
                    let v = self.pop();
                    self.frames[fi].cells[c as usize] = Rc::new(RefCell::new(v));
                }
                Op::SlotToCell { slot, cell } => {
                    let v = std::mem::replace(&mut self.stack[base + slot as usize], Value::Null);
                    self.frames[fi].cells[cell as usize] = Rc::new(RefCell::new(v));
                }
                Op::LoadUpvalue(u) => {
                    let v = self.upvalue(fi, u).borrow().clone();
                    self.stack.push(v);
                }
                Op::StoreUpvalue(u) => {
                    let v = self.pop();
                    *self.upvalue(fi, u).borrow_mut() = v;
                }
                Op::LoadGlobal(g) => {
                    let slots = globals.slots.borrow();
                    let global = &slots[g as usize];
                    if !global.declared {
                        return Err(undeclared(&global.key, span!()));
                    }
                    let v = global.value.clone();
                    drop(slots);
                    self.stack.push(v);
                }
                Op::StoreGlobal(g) => {
                    let v = self.pop();
                    let mut slots = globals.slots.borrow_mut();
                    let global = &mut slots[g as usize];
                    let name = global.key.trim_start_matches('$');
                    if !global.declared {
                        return error(format!("`${name}` isn't declared. Declare it with `let ${name} = ...;`"), span!());
                    }
                    if global.constant {
                        return error(format!("`${name}` is a constant, it can't be changed"), span!());
                    }
                    global.value = v;
                }
                Op::DefineGlobal { global: g, constant } => {
                    let v = self.pop();
                    let mut slots = globals.slots.borrow_mut();
                    let global = &mut slots[g as usize];
                    if global.declared {
                        return Err(already_exists(&global.key, span!()));
                    }
                    global.value = v;
                    global.declared = true;
                    global.constant = constant;
                }
                Op::LoadBuiltin(b) => {
                    let f = self.host.builtins[b as usize].clone();
                    self.stack.push(Value::Function(f));
                }
                Op::SaveSp(s) => {
                    let height = self.stack.len() as i64;
                    self.stack[base + s as usize] = Value::Int(height);
                }
                Op::RestoreSp(s) => {
                    if let Value::Int(height) = self.stack[base + s as usize] {
                        self.stack.truncate(height as usize);
                    }
                }

                // ---- Operators ----
                Op::Add => arith!(BinaryOp::Add, |a: i64, b: i64| a.checked_add(b).map(Value::Int), |a: f64, b: f64| Some(Value::Float(a + b))),
                Op::Sub => arith!(BinaryOp::Sub, |a: i64, b: i64| a.checked_sub(b).map(Value::Int), |a: f64, b: f64| Some(Value::Float(a - b))),
                Op::Mul => arith!(BinaryOp::Mul, |a: i64, b: i64| a.checked_mul(b).map(Value::Int), |a: f64, b: f64| Some(Value::Float(a * b))),
                Op::Div => arith!(BinaryOp::Div, |a: i64, b: i64| Some(Value::Float(a as f64 / b as f64)), |a: f64, b: f64| Some(Value::Float(a / b))),
                Op::Rem => arith!(
                    BinaryOp::Rem,
                    |a: i64, b: i64| if b == 0 { None } else { Some(Value::Int(a.wrapping_rem(b))) },
                    |a: f64, b: f64| if b == 0.0 { None } else { Some(Value::Float(a % b)) }
                ),
                Op::Eq => {
                    let r = self.pop();
                    let l = self.top();
                    *l = Value::Bool(l.same(&r));
                }
                Op::NotEq => {
                    let r = self.pop();
                    let l = self.top();
                    *l = Value::Bool(!l.same(&r));
                }
                Op::Lt => compare_op!(BinaryOp::Lt, <),
                Op::Gt => compare_op!(BinaryOp::Gt, >),
                Op::LtEq => compare_op!(BinaryOp::LtEq, <=),
                Op::GtEq => compare_op!(BinaryOp::GtEq, >=),
                Op::Binary(op) => {
                    let r = self.pop();
                    let l = self.pop();
                    let v = binary(op, l, r).map_err(|m| op_error(m, span!()))?;
                    self.stack.push(v);
                }
                Op::Unary(op) => {
                    let v = self.pop();
                    let v = unary(op, v, span!())?;
                    self.stack.push(v);
                }
                Op::Pattern(op) => {
                    let r = self.pop();
                    let l = self.pop();
                    let v = self.pattern(op, l, r, span!())?;
                    self.stack.push(v);
                }
                Op::PathTest(op) => {
                    let v = match self.pop() {
                        Value::Str(p) => Value::Bool(patterns::path_test(op, &self.host.state.resolve(&p).display().to_string())),
                        other => return error(format!("`{}` needs a path string, not a {}", op.symbol(), other.type_name()), span!()),
                    };
                    self.stack.push(v);
                }
                Op::CheckNumber => {
                    let v = self.top();
                    if v.as_f64().is_none() {
                        let message = format!("`++` and `--` only work on numbers, and this is a {}", v.type_name());
                        return error(message, span!());
                    }
                }
                Op::CheckBool(and) => {
                    if !matches!(self.top(), Value::Bool(_)) {
                        return Err(logic_error(and, span!()));
                    }
                }

                // ---- Jumps ----
                Op::Jump(t) => ip = t as usize,
                Op::JumpIfFalse { target, keyword } => match self.pop() {
                    Value::Bool(true) => {}
                    Value::Bool(false) => ip = target as usize,
                    other => {
                        let keyword = ["if", "while", "? :"][keyword as usize];
                        let message = format!("the condition of `{keyword}` must be a bool, but this is a {}", other.type_name());
                        return error_as(exceptions::TYPE_ERROR, message, span!());
                    }
                },
                Op::JumpIfNot { op, target } => {
                    let r = self.pop();
                    let l = self.pop();
                    let holds = match (&l, &r, op) {
                        (Value::Int(a), Value::Int(b), BinaryOp::Lt) => a < b,
                        (Value::Int(a), Value::Int(b), BinaryOp::Gt) => a > b,
                        (Value::Int(a), Value::Int(b), BinaryOp::LtEq) => a <= b,
                        (Value::Int(a), Value::Int(b), BinaryOp::GtEq) => a >= b,
                        (_, _, BinaryOp::Eq) => l.same(&r),
                        (_, _, BinaryOp::NotEq) => !l.same(&r),
                        _ => matches!(compare(op, &l, &r).map_err(|m| op_error(m, span!()))?, Value::Bool(true)),
                    };
                    if !holds {
                        ip = target as usize;
                    }
                }
                Op::JumpIfTrueRaw(t) => {
                    if matches!(self.pop(), Value::Bool(true)) {
                        ip = t as usize;
                    }
                }
                Op::JumpIfFalseRaw(t) => {
                    if !matches!(self.pop(), Value::Bool(true)) {
                        ip = t as usize;
                    }
                }
                Op::JumpLogic { target, and } => {
                    let Value::Bool(b) = *self.top() else {
                        return Err(logic_error(and, span!()));
                    };
                    if b != and {
                        ip = target as usize;
                    } else {
                        self.pop();
                    }
                }
                Op::JumpIfNotNull(t) => {
                    if matches!(self.top(), Value::Null) {
                        self.pop();
                    } else {
                        ip = t as usize;
                    }
                }
                Op::JumpIfNull(t) => {
                    if matches!(self.top(), Value::Null) {
                        ip = t as usize;
                    }
                }

                // ---- Collections and strings ----
                Op::CheckKey => {
                    let v = self.top();
                    if Key::from_value(v).is_none() {
                        return Err(key_error(v, span!()));
                    }
                }
                Op::List(n) => {
                    let items = self.stack.split_off(self.stack.len() - n as usize);
                    self.stack.push(Value::list(items));
                }
                Op::Map(n) => {
                    let items = self.stack.split_off(self.stack.len() - 2 * n as usize);
                    let mut map = IndexMap::with_capacity(n as usize);
                    let mut items = items.into_iter();
                    while let (Some(k), Some(v)) = (items.next(), items.next()) {
                        map.insert(Key::from_value(&k).expect("checked by CheckKey"), v);
                    }
                    self.stack.push(Value::Map(Rc::new(RefCell::new(map))));
                }
                Op::Interpolate(n) => {
                    let parts = self.stack.split_off(self.stack.len() - n as usize);
                    let mut text = String::new();
                    for part in parts {
                        match part {
                            Value::Str(s) => text.push_str(&s),
                            other => text.push_str(&other.to_string()),
                        }
                    }
                    self.stack.push(Value::string(&text));
                }
                Op::QuoteArgument => {
                    let v = self.pop();
                    let quoted = command_argument(&v).map_err(|m| flow_error_as(exceptions::TYPE_ERROR, m, span!()))?;
                    self.stack.push(Value::string(&quoted));
                }
                Op::Command(n) => {
                    let parts = self.stack.split_off(self.stack.len() - n as usize);
                    let script: String = parts.iter().map(|p| p.to_string()).collect();
                    self.stack.push(Value::Command(Rc::new(RefCell::new(CommandValue::new(CommandKind::Shell(script))))));
                }
                Op::Regex(r) => {
                    let (pattern, flags) = &proto.regexes[r as usize];
                    let regex = self.regex(pattern, flags).map_err(|m| flow_error_as(exceptions::VALUE_ERROR, m, span!()))?;
                    self.stack.push(Value::Regex(regex));
                }
                Op::RangeBound => {
                    let v = self.top();
                    match v.as_whole() {
                        Some(n) => *v = Value::Int(n),
                        None => return error(format!("ranges need whole numbers, and this is {}", describe(v)), span!()),
                    }
                }
                Op::Range { inclusive } => {
                    let end = self.pop().as_whole().unwrap_or(0);
                    let start = self.pop().as_whole().unwrap_or(0);
                    self.stack.push(Value::Range(Range { start, end, inclusive, step: 1 }));
                }
                Op::Index => {
                    let index = self.pop();
                    let container = self.pop();
                    let v = index_value(container, index, span!())?;
                    self.stack.push(v);
                }
                Op::SliceFrom => {
                    let start = self.pop().as_whole().unwrap_or(0);
                    let container = self.pop();
                    let len = match &container {
                        Value::List(l) => l.borrow().len(),
                        Value::Str(s) => s.chars().count(),
                        other => return error(format!("a {} can't be sliced", other.type_name()), span!()),
                    };
                    let range = Range { start, end: len as i64, inclusive: false, step: 1 };
                    let v = slice(&container, range, len).map_err(|m| index_error(m, span!()))?;
                    self.stack.push(v);
                }
                Op::SetIndex => {
                    let value = self.pop();
                    let index = self.pop();
                    let container = self.pop();
                    set_index(&container, index, value.clone(), span!())?;
                    self.stack.push(value);
                }
                Op::UpdateIndex { op, mode } => {
                    let rhs = self.pop();
                    let index = self.pop();
                    let container = self.pop();
                    let old = index_value(container.clone(), index.clone(), span!())?;
                    if mode > 0 && old.as_f64().is_none() {
                        return error(format!("`++` and `--` only work on numbers, and this is a {}", old.type_name()), span!());
                    }
                    let new = binary(op, old.clone(), rhs).map_err(|m| op_error(m, span!()))?;
                    set_index(&container, index, new.clone(), span!())?;
                    self.stack.push(if mode == 2 { old } else { new });
                }
                Op::Destructure { names, rest } => {
                    let v = self.pop();
                    self.destructure(v, names as usize, rest, span!())?;
                }

                // ---- Types ----
                Op::Is { ty, catch } => {
                    let v = self.pop();
                    let ok = types::matches(&v, &proto.types[ty as usize]).map_err(|m| {
                        if catch { flow_error_as(exceptions::TYPE_ERROR, m, span!()) } else { flow_error(m, span!()) }
                    })?;
                    self.stack.push(Value::Bool(ok));
                }
                Op::As(ty) => {
                    let ty = &proto.types[ty as usize];
                    let v = self.top();
                    if !types::matches(v, ty).map_err(|m| flow_error(m, span!()))? {
                        let message = format!("`.as({ty})` failed: the value is {}", describe(v));
                        return error_as(exceptions::TYPE_ERROR, message, span!());
                    }
                }

                // ---- Functions ----
                Op::Closure(p) => {
                    let inner = proto.protos[p as usize].clone();
                    let upvalues = inner
                        .captures
                        .iter()
                        .map(|c| match c {
                            Capture::Cell(i) => self.frames[fi].cells[*i as usize].clone(),
                            Capture::Upvalue(i) => self.upvalue(fi, *i).clone(),
                        })
                        .collect();
                    let function = UserFunction { proto: inner, upvalues, globals: self.globals_of(fi) };
                    self.stack.push(Value::Function(Rc::new(Function::User(function))));
                }
                Op::Call(argc) => {
                    self.frames[fi].ip = ip;
                    if self.call_op(argc as usize, span!())? {
                        reload!();
                    }
                }
                Op::CallComplex(c) => {
                    self.frames[fi].ip = ip;
                    if self.call_complex(&proto.calls[c as usize], span!())? {
                        reload!();
                    }
                }
                Op::CallMethod { name, argc } => {
                    self.frames[fi].ip = ip;
                    let args = self.stack.split_off(self.stack.len() - argc as usize);
                    let receiver = self.pop();
                    let v = methods::call(self, receiver, &proto.names[name as usize], args, span!())?;
                    self.stack.push(v);
                }
                Op::CallMethodComplex { name, call } => {
                    self.frames[fi].ip = ip;
                    let kinds = &proto.calls[call as usize];
                    let (args, _) = self.complex_args(kinds, self.stack.len() - kinds.len())?;
                    let receiver = self.pop();
                    let v = methods::call(self, receiver, &proto.names[name as usize], args, span!())?;
                    self.stack.push(v);
                }
                Op::Return => {
                    let value = self.pop();
                    let frame = self.frames.pop().expect("a frame is running");
                    self.stack.truncate(frame.base - 1);
                    if self.frames.len() == entry {
                        return Ok(value);
                    }
                    self.stack.push(value);
                    reload!();
                }
                Op::JumpIfGiven { param, target } => {
                    if param < 64 && self.frames[fi].given & (1 << param) != 0 {
                        ip = target as usize;
                    }
                }

                // ---- Modules ----
                Op::UseModule { module, path } => {
                    let source = self.host.loader.resolve(&proto.imports[path as usize].path).map_err(|m| flow_error(m, span!()))?;
                    globals.modules.borrow_mut()[module as usize] = Some(ModuleSlot::Pending(source));
                }
                Op::UseItems(i) => {
                    self.frames[fi].ip = ip;
                    self.use_items(&proto, i as usize, &globals, span!())?;
                }
                Op::ModuleMember { module, name, constant } => {
                    self.frames[fi].ip = ip;
                    let v = self.module_member(&globals, module as usize, &proto.names[name as usize], constant, span!())?;
                    self.stack.push(v);
                }

                // ---- Exceptions ----
                Op::Throw => {
                    let v = self.pop();
                    let Value::Exception(exception) = v else {
                        let message = format!("only exceptions can be thrown, not a {} *(i.e. `throw ValueError('...')`)*", v.type_name());
                        return error_as(exceptions::TYPE_ERROR, message, span!());
                    };
                    // A new exception gets the place where it's thrown; a caught one keeps its own
                    let exception = if exception.span == Span::default() {
                        exceptions::new(&exception.kind, exception.message.clone(), span!())
                    } else {
                        exception
                    };
                    return Err(Flow::Throw(exception));
                }
                Op::Rethrow => match self.pop() {
                    Value::Exception(e) => return Err(Flow::Throw(e)),
                    _ => unreachable!("only exceptions are caught"),
                },
                Op::PushCatch(t) => self.push_handler(HandlerKind::Catch, t, fi, 0),
                Op::PushTry(t) => self.push_handler(HandlerKind::TryExpr, t, fi, 0),
                Op::PushFinally { target, slot } => self.push_handler(HandlerKind::Finally, target, fi, slot),
                Op::PopHandler => {
                    self.handlers.pop();
                }
                Op::EndFinally(s) => match std::mem::replace(&mut self.stack[base + s as usize], Value::Null) {
                    Value::Exception(e) => return Err(Flow::Throw(e)),
                    Value::Int(code) => return Err(Flow::Exit(code as i32)),
                    _ => {}
                },
                Op::WrapOk => {
                    let v = self.pop();
                    self.stack.push(Value::Result(Rc::new(ResultValue::Ok(v))));
                }
                Op::WrapError => {
                    let v = self.pop();
                    self.stack.push(Value::Result(Rc::new(ResultValue::Error(v))));
                }
                Op::Fail(n) => return error(&*proto.names[n as usize], span!()),
                Op::NoCase(s) => {
                    let subject = &self.stack[base + s as usize];
                    return error(format!("no case of the `switch` matches {}", describe(subject)), span!());
                }
                Op::CaseMatches(s) => {
                    let pattern = self.pop();
                    let subject = &self.stack[base + s as usize];
                    let matched = match pattern {
                        Value::Range(range) if !matches!(subject, Value::Range(_)) => subject.as_whole().is_some_and(|n| range.contains(n)),
                        pattern => subject.same(&pattern),
                    };
                    self.stack.push(Value::Bool(matched));
                }

                // ---- Loops ----
                Op::ForPrepare { slot, kind } => {
                    let iterable = self.pop();
                    let (seq, extra) = for_sequence(iterable, kind, span!())?;
                    let at = base + slot as usize;
                    self.stack[at] = seq;
                    self.stack[at + 1] = Value::Int(0);
                    self.stack[at + 2] = extra;
                }
                Op::ForNext { slot, exit, kind } => {
                    let at = base + slot as usize;
                    let Value::Int(i) = self.stack[at + 1] else { unreachable!("the index of a `for`") };
                    let item = match &self.stack[at] {
                        Value::List(list) => list.borrow().get(i as usize).cloned(),
                        Value::Range(range) => range_at(range, i).map(Value::Int),
                        _ => unreachable!("set by ForPrepare"),
                    };
                    let Some(item) = item else {
                        ip = exit as usize;
                        continue;
                    };
                    self.stack[at + 1] = Value::Int(i + 1);
                    match kind {
                        for_kind::INDEXED => {
                            self.stack.push(Value::Int(i));
                            self.stack.push(item);
                        }
                        for_kind::KEY_VALUE => {
                            let value = match &self.stack[at + 2] {
                                Value::List(values) => values.borrow().get(i as usize).cloned().unwrap_or(Value::Null),
                                _ => Value::Null,
                            };
                            self.stack.push(item);
                            self.stack.push(value);
                        }
                        _ => self.stack.push(item),
                    }
                }
            }
        }
    }

    fn push_handler(&mut self, kind: HandlerKind, target: u32, frame: usize, slot: u32) {
        let sp = self.stack.len();
        self.handlers.push(Handler { kind, ip: target as usize, frame, sp, slot: slot as usize });
    }

    fn pattern(&mut self, op: BinaryOp, l: Value, r: Value, span: Span) -> Exec<Value> {
        match op {
            BinaryOp::Like | BinaryOp::NotLike => {
                let (Value::Str(text), Value::Str(pattern)) = (&l, &r) else {
                    let message = format!("`{}` needs two strings, but got a {} and a {}", op.symbol(), l.type_name(), r.type_name());
                    return error_as(exceptions::TYPE_ERROR, message, span);
                };
                let found = patterns::like(text, pattern).map_err(|m| flow_error(m, span))?;
                Ok(Value::Bool(found == (op == BinaryOp::Like)))
            }
            _ => {
                let Value::Str(text) = &l else {
                    return error(format!("`{}` needs a string on its left, not a {}", op.symbol(), l.type_name()), span);
                };
                let regex = self.as_regex(&r).map_err(|m| flow_error_as(exceptions::VALUE_ERROR, m, span))?;
                Ok(Value::Bool(regex.compiled.is_match(text) == (op == BinaryOp::Match)))
            }
        }
    }

    /// Pushes the elements of a list for `let [$a, $b, ...$rest]`.
    fn destructure(&mut self, value: Value, names: usize, rest: bool, span: Span) -> Exec<()> {
        let Value::List(list) = value else {
            return error(format!("only lists can be destructured, and this is a {}", value.type_name()), span);
        };
        let items = list.borrow();
        if !rest && items.len() != names {
            return error(format!("expected a list with {names} elements, but it has {}", items.len()), span);
        }
        if rest && items.len() < names {
            return error(format!("expected a list with at least {names} elements, but it has {}", items.len()), span);
        }
        self.stack.extend(items[..names].iter().cloned());
        if rest {
            self.stack.push(Value::list(items[names..].to_vec()));
        }
        Ok(())
    }

    // ---- Modules ----

    fn use_items(&mut self, proto: &Proto, i: usize, globals: &Globals, span: Span) -> Exec<()> {
        let import = &proto.imports[i];
        let source = self.host.loader.resolve(&import.path).map_err(|m| flow_error(m, span))?;
        // Importing names needs their values now
        let module = self.load(source, span)?;
        for (name, variable, global, local) in &import.items {
            let key = if *variable { format!("${name}") } else { name.clone() };
            if !*variable && !module.public.contains(&key) && module.public.contains(&format!("type {name}")) {
                // A type alias: only the checker uses it
                continue;
            }
            if !module.public.contains(&key) {
                let what = if *variable { "constant" } else { "function or type" };
                return error(format!("`{}` has no `pub` {what} named `{key}`", module.path), span);
            }
            let value = module.globals.get(&key).unwrap_or(Value::Null);
            let mut slots = globals.slots.borrow_mut();
            let slot = &mut slots[*global as usize];
            if slot.declared {
                return error(format!("`{local}` already exists"), span);
            }
            slot.value = value;
            slot.declared = true;
            slot.constant = true;
        }
        Ok(())
    }

    /// `name::function` or `name::$constant`, loading the module the first time.
    fn module_member(&mut self, globals: &Globals, m: usize, name: &str, constant: bool, span: Span) -> Exec<Value> {
        let slot = globals.modules.borrow()[m].clone();
        let module = match slot {
            Some(ModuleSlot::Loaded(module)) => module,
            Some(ModuleSlot::Pending(source)) => {
                let module = self.load(source, span)?;
                globals.modules.borrow_mut()[m] = Some(ModuleSlot::Loaded(module.clone()));
                module
            }
            None => return error(format!("the module of `{name}` isn't imported yet"), span),
        };
        let (key, what) = if constant { (format!("${name}"), "constant") } else { (name.to_string(), "function") };
        if !module.public.contains(&key) {
            return error(format!("`{}` has no `pub` {what} named `{key}`", module.path), span);
        }
        match module.globals.get(&key) {
            Some(v) => Ok(v),
            None => error(format!("`{}` has no {what} named `{key}`", module.path), span),
        }
    }

    /// Loads a module once: later loads give the same one.
    fn load(&mut self, source: ModuleSource, span: Span) -> Exec<Rc<Module>> {
        let path = match source {
            ModuleSource::Std(name) => {
                if let Some(m) = self.host.loader.std_cache.get(&name) {
                    return Ok(m.clone());
                }
                let module = crate::stdlib::build(&name).ok_or_else(|| flow_error(format!("there's no module `{name}`"), span))?;
                self.host.loader.std_cache.insert(name, module.clone());
                return Ok(module);
            }
            ModuleSource::File(path) => path,
        };
        if let Some(m) = self.host.loader.cache.get(&path) {
            return Ok(m.clone());
        }
        if self.host.loader.loading.contains(&path) {
            let chain: Vec<String> = self.host.loader.loading.iter().chain(std::iter::once(&path)).map(|p| p.display().to_string()).collect();
            return error(format!("circular import: {}", chain.join(" uses ")), span);
        }
        let shown = path.display().to_string();
        let source = std::fs::read_to_string(&path).map_err(|e| flow_error_as("IOError", format!("can't read the module '{shown}': {e}"), span))?;
        let file = self.host.loader.files.len() as u32;
        self.host.loader.files.push(shown.clone());
        let program = crate::parse_budgeted(&source, file, self.stack_budget).map_err(|d| flow_error(d.message, d.span))?;
        let globals = Rc::new(Globals::default());
        let (proto, public) = Compiler::new(&globals, &self.host.builtin_names, self.stack_budget)
            .module(&program, &shown)
            .map_err(|d| flow_error(d.message, d.span))?;

        self.host.loader.loading.push(path.clone());
        let body = Rc::new(Function::User(UserFunction { proto, upvalues: Vec::new(), globals: globals.clone() }));
        let result = self.call(&body, Vec::new(), Vec::new(), span);
        self.host.loader.loading.pop();
        result?;
        let module = Rc::new(Module { path: shown, globals, public });
        self.host.loader.cache.insert(path, module.clone());
        Ok(module)
    }
}

/// The values of a call's parameters, from its arguments, and which were given.
fn bind(user: &UserFunction, mut positional: Vec<Value>, mut named: Named, span: Span) -> Exec<(Vec<Value>, u64)> {
    let proto = &user.proto;
    let name = &proto.name;
    let mut values = Vec::with_capacity(proto.params.len());
    let mut given = 0u64;
    positional.reverse(); // so the next one is popped from the end
    for (i, param) in proto.params.iter().enumerate() {
        let bit = if i < 64 { 1 << i } else { 0 };
        let value = if param.variadic {
            positional.reverse();
            given |= bit;
            Value::list(std::mem::take(&mut positional))
        } else if let Some(v) = positional.pop() {
            if named.iter().any(|(n, ..)| **n == *param.name) {
                return error_as(exceptions::TYPE_ERROR, format!("`{}` was given twice", param.name), span);
            }
            given |= bit;
            v
        } else if let Some(j) = named.iter().position(|(n, ..)| **n == *param.name) {
            given |= bit;
            named.remove(j).1
        } else if param.has_default {
            Value::Null
        } else {
            return error_as(exceptions::TYPE_ERROR, format!("`{name}` needs a value for `${}`", param.name), span);
        };
        values.push(value);
    }
    if !positional.is_empty() {
        return error_as(exceptions::TYPE_ERROR, format!("`{name}` takes {} arguments, but got more", proto.params.len()), span);
    }
    if let Some((n, _, arg_span)) = named.first() {
        return error_as(exceptions::TYPE_ERROR, format!("`{name}` has no parameter named `{n}`"), *arg_span);
    }
    Ok((values, given))
}

/// What a `for` loops over, and the values of a map's keys.
fn for_sequence(iterable: Value, kind: u8, span: Span) -> Exec<(Value, Value)> {
    if kind == for_kind::LIST {
        return match iterable {
            Value::List(_) => Ok((iterable, Value::Null)),
            other => error(format!("`for ([...] in ...)` needs a list of lists, not a {}", other.type_name()), span),
        };
    }
    match (&iterable, kind) {
        (Value::Map(map), for_kind::VALUE | for_kind::KEY_VALUE) => {
            let map = map.borrow();
            let keys = Value::list(map.keys().map(|k| k.to_value()).collect());
            let values = Value::list(map.values().cloned().collect());
            Ok((keys, values))
        }
        (Value::Map(_), _) => error("use `for ($key => $value in $map)` to loop over a map with its keys", span),
        (_, for_kind::KEY_VALUE) => error(format!("`=>` only works with maps, and this is a {}", iterable.type_name()), span),
        // Looped by position, so elements added while looping are seen, like in JavaScript
        (Value::List(_) | Value::Range(_), _) => Ok((iterable, Value::Null)),
        (Value::Str(s), _) => Ok((Value::list(s.chars().map(|c| Value::string(c.encode_utf8(&mut [0; 4]))).collect()), Value::Null)),
        _ => error(format!("can't loop over a {}", iterable.type_name()), span),
    }
}

/// The `i`th number of a range, like `Range::iter`.
fn range_at(range: &Range, i: i64) -> Option<i64> {
    let down = range.start > range.end;
    let offset = i.checked_mul(range.step)?;
    let n = if down { range.start.checked_sub(offset)? } else { range.start.checked_add(offset)? };
    let inside = match (down, range.inclusive) {
        (false, false) => n < range.end,
        (false, true) => n <= range.end,
        (true, false) => n > range.end,
        (true, true) => n >= range.end,
    };
    inside.then_some(n)
}

fn index_value(container: Value, index: Value, span: Span) -> Exec<Value> {
    match container {
        Value::Map(map) => {
            let Some(key) = Key::from_value(&index) else { return Err(key_error(&index, span)) };
            Ok(map.borrow().get(&key).cloned().unwrap_or(Value::Null))
        }
        Value::List(_) | Value::Str(_) => {
            let len = match &container {
                Value::List(l) => l.borrow().len(),
                Value::Str(s) => s.chars().count(),
                _ => unreachable!(),
            };
            if let Value::Range(range) = index {
                return slice(&container, range, len).map_err(|m| index_error(m, span));
            }
            let pos = list_position(&index, len).map_err(|m| index_error(m, span))?;
            Ok(match &container {
                Value::List(l) => l.borrow()[pos].clone(),
                Value::Str(s) => Value::string(&s.chars().nth(pos).unwrap().to_string()),
                _ => unreachable!(),
            })
        }
        other => error(format!("a {} can't be indexed", other.type_name()), span),
    }
}

fn set_index(container: &Value, index: Value, value: Value, span: Span) -> Exec<()> {
    match container {
        Value::List(list) => {
            let len = list.borrow().len();
            let pos = list_position(&index, len).map_err(|m| index_error(m, span))?;
            list.borrow_mut()[pos] = value;
            Ok(())
        }
        Value::Map(map) => {
            let Some(key) = Key::from_value(&index) else { return Err(key_error(&index, span)) };
            map.borrow_mut().insert(key, value);
            Ok(())
        }
        Value::Str(_) => error("strings can't be changed. Build a new one instead", span),
        other => error(format!("a {} can't be indexed", other.type_name()), span),
    }
}

fn key_error(value: &Value, span: Span) -> Flow {
    flow_error(format!("map keys can be strings, numbers or symbols, not a {}", value.type_name()), span)
}

fn logic_error(and: bool, span: Span) -> Flow {
    let name = if and { "&&" } else { "||" };
    flow_error_as(exceptions::TYPE_ERROR, format!("both sides of `{name}` must be bools"), span)
}

#[cold]
fn undeclared(key: &str, span: Span) -> Flow {
    match key.strip_prefix('$') {
        Some(name) => flow_error(format!("`${name}` isn't declared"), span),
        None => flow_error(format!("there's no function named `{key}`. Functions must be defined before they're used"), span),
    }
}

#[cold]
fn already_exists(key: &str, span: Span) -> Flow {
    match key.strip_prefix('$') {
        Some(name) => flow_error(format!("`${name}` already exists. Variables can't be declared twice"), span),
        None => flow_error(format!("a function named `{key}` already exists"), span),
    }
}

/// A value in an error message: `5`, `'abc'`, a list...
fn describe(value: &Value) -> String {
    match value {
        Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Null => format!("`{value}`"),
        Value::Str(_) => format!("the string `{value:?}`"),
        other => format!("a {}", other.type_name()),
    }
}

fn native_error(e: NativeError, span: Span) -> Flow {
    flow_error_as(&e.kind, e.message, span)
}

/// A value in `%x"..."`, quoted so it's always a single argument. A list
/// becomes several arguments.
fn command_argument(value: &Value) -> Result<String, String> {
    match value {
        Value::Str(s) => Ok(shell_quote(s)),
        Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Symbol(_) => Ok(shell_quote(&value.to_string())),
        Value::List(items) => {
            let parts: Result<Vec<String>, String> = items.borrow().iter().map(command_argument).collect();
            Ok(parts?.join(" "))
        }
        other => Err(format!("a {} can't be used in a command", other.type_name())),
    }
}

/// The first line of a regex error, which says what's wrong.
fn regex_error(e: &regex::Error) -> String {
    match e {
        regex::Error::Syntax(text) => text.lines().last().unwrap_or("syntax error").trim_start_matches("error: ").to_string(),
        other => other.to_string(),
    }
}

/// The position for `$list[$i]`, with negative indexes counting from the end.
fn list_position(index: &Value, len: usize) -> Result<usize, String> {
    let Some(i) = index.as_whole() else {
        return Err(format!("indexes must be whole numbers, and this is {}", describe(index)));
    };
    let pos = if i < 0 { len as i64 + i } else { i };
    if pos < 0 || pos >= len as i64 {
        return Err(format!("index {i} is outside the list, which has {len} elements"));
    }
    Ok(pos as usize)
}

/// `$list[1..3]`: the ends are positions, negative ones count from the end, and
/// a slice never counts down.
fn slice(container: &Value, range: Range, len: usize) -> Result<Value, String> {
    if range.step != 1 {
        return Err("slices can't use a step".into());
    }
    let resolve = |i: i64| if i < 0 { (len as i64 + i).max(0) } else { i.min(len as i64) };
    let start = resolve(range.start);
    let mut end = resolve(range.end);
    if range.inclusive {
        end = (end + 1).min(len as i64);
    }
    let (start, end) = (start as usize, (end.max(start)) as usize);
    Ok(match container {
        Value::List(l) => Value::list(l.borrow()[start..end].to_vec()),
        Value::Str(s) => Value::string(&s.chars().skip(start).take(end - start).collect::<String>()),
        _ => unreachable!(),
    })
}

fn unary(op: UnaryOp, value: Value, span: Span) -> Exec<Value> {
    match (op, &value) {
        (UnaryOp::Neg, Value::Int(n)) => Ok(n.checked_neg().map(Value::Int).unwrap_or(Value::Float(-(*n as f64)))),
        (UnaryOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (UnaryOp::Plus, Value::Int(_) | Value::Float(_)) => Ok(value),
        (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (UnaryOp::BitNot, _) if value.as_whole().is_some() => Ok(Value::Int(!value.as_whole().unwrap())),
        (UnaryOp::BitNot, Value::Float(_)) => error_as(exceptions::TYPE_ERROR, "bitwise operators need whole numbers", span),
        (UnaryOp::Not, _) => error_as(exceptions::TYPE_ERROR, format!("`!` needs a bool, and this is a {}", value.type_name()), span),
        _ => error_as(exceptions::TYPE_ERROR, format!("`{}` needs a number, and this is a {}", op.symbol(), value.type_name()), span),
    }
}

/// Every binary operator except `&&`, `||` and `??`.
pub fn binary(op: BinaryOp, l: Value, r: Value) -> Result<Value, String> {
    use BinaryOp::*;
    match op {
        Add | Sub | Mul | Div | Rem | Pow => arithmetic(op, &l, &r),
        Eq => Ok(Value::Bool(l.same(&r))),
        NotEq => Ok(Value::Bool(!l.same(&r))),
        Lt | Gt | LtEq | GtEq => compare(op, &l, &r),
        Shl if matches!(l, Value::List(_)) => {
            if let Value::List(list) = &l {
                list.borrow_mut().push(r);
            }
            Ok(l)
        }
        BitAnd | BitOr | BitXor if matches!((&l, &r), (Value::Bool(_), Value::Bool(_))) => {
            let (Value::Bool(a), Value::Bool(b)) = (l, r) else { unreachable!() };
            Ok(Value::Bool(match op {
                BitAnd => a & b,
                BitOr => a | b,
                _ => a ^ b,
            }))
        }
        BitAnd | BitOr | BitXor | Shl | Shr | UShr => {
            let (Some(a), Some(b)) = (l.as_whole(), r.as_whole()) else {
                return Err(format!(
                    "`{}` needs whole numbers, but got a {} and a {}",
                    op.symbol(),
                    l.type_name(),
                    r.type_name()
                ));
            };
            let shift = (b & 63) as u32;
            Ok(Value::Int(match op {
                BitAnd => a & b,
                BitOr => a | b,
                BitXor => a ^ b,
                Shl => a.wrapping_shl(shift),
                Shr => a.wrapping_shr(shift),
                _ => ((a as u64) >> shift) as i64,
            }))
        }
        In | NotIn => {
            let found = contains(&r, &l)?;
            Ok(Value::Bool(if op == In { found } else { !found }))
        }
        Like | NotLike | Match | NotMatch => unreachable!("evaluated by the interpreter"),
        And | Or | Coalesce => unreachable!("evaluated lazily"),
    }
}

/// `-in`: the value is in the list, is a key of the map, or is in the range.
pub fn contains(container: &Value, value: &Value) -> Result<bool, String> {
    match container {
        Value::List(list) => Ok(list.borrow().iter().any(|item| item.equals(value))),
        Value::Map(map) => Ok(Key::from_value(value).is_some_and(|k| map.borrow().contains_key(&k))),
        Value::Range(range) => Ok(value.as_whole().is_some_and(|n| range.contains(n))),
        other => Err(format!("`-in` needs a list, a map or a range on its right, not a {}", other.type_name())),
    }
}

pub fn arithmetic(op: BinaryOp, l: &Value, r: &Value) -> Result<Value, String> {
    use BinaryOp::*;
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => {
            let (a, b) = (*a, *b);
            let float = |x: f64| Ok(Value::Float(x));
            match op {
                Add => a.checked_add(b).map_or_else(|| float(a as f64 + b as f64), |n| Ok(Value::Int(n))),
                Sub => a.checked_sub(b).map_or_else(|| float(a as f64 - b as f64), |n| Ok(Value::Int(n))),
                Mul => a.checked_mul(b).map_or_else(|| float(a as f64 * b as f64), |n| Ok(Value::Int(n))),
                Div => float(a as f64 / b as f64),
                Rem if b == 0 => Err("remainder by zero".into()),
                Rem => Ok(Value::Int(a.wrapping_rem(b))),
                Pow if b >= 0 && b <= u32::MAX as i64 => {
                    a.checked_pow(b as u32).map_or_else(|| float((a as f64).powf(b as f64)), |n| Ok(Value::Int(n)))
                }
                Pow => float((a as f64).powf(b as f64)),
                _ => unreachable!(),
            }
        }
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
            let (a, b) = (l.as_f64().unwrap(), r.as_f64().unwrap());
            Ok(Value::Float(match op {
                Add => a + b,
                Sub => a - b,
                Mul => a * b,
                Div => a / b,
                Rem if b == 0.0 => return Err("remainder by zero".into()),
                Rem => a % b,
                Pow => a.powf(b),
                _ => unreachable!(),
            }))
        }
        (Value::Str(a), Value::Str(b)) if op == Add => {
            let mut s = String::with_capacity(a.len() + b.len());
            s.push_str(a);
            s.push_str(b);
            Ok(Value::string(&s))
        }
        (Value::Str(_), _) | (_, Value::Str(_)) if op == Add => Err(format!(
            "`+` joins two strings, but got a {} and a {}. Use an f-string instead *(i.e. f\"Age: {{$age}}\")*",
            l.type_name(),
            r.type_name()
        )),
        _ => Err(format!("`{}` needs numbers, but got a {} and a {}", op.symbol(), l.type_name(), r.type_name())),
    }
}

fn compare(op: BinaryOp, l: &Value, r: &Value) -> Result<Value, String> {
    use std::cmp::Ordering;
    let ordering = match (l, r) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => l.as_f64().unwrap().partial_cmp(&r.as_f64().unwrap()),
        (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
        _ => {
            return Err(format!(
                "`{}` compares two numbers or two strings, but got a {} and a {}",
                op.symbol(),
                l.type_name(),
                r.type_name()
            ));
        }
    };
    // NaN is never smaller, bigger or equal to anything
    let Some(ordering) = ordering else { return Ok(Value::Bool(false)) };
    Ok(Value::Bool(match op {
        BinaryOp::Lt => ordering == Ordering::Less,
        BinaryOp::Gt => ordering == Ordering::Greater,
        BinaryOp::LtEq => ordering != Ordering::Greater,
        _ => ordering != Ordering::Less,
    }))
}
