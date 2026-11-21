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
use crate::bytecode::{ArgKind, Capture, Op, Proto, class_names, for_kind};
use crate::command::{CommandKind, CommandValue, split_command};
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
use crate::value::{
    Class, ExceptionValue, FieldSlot, Function, Key, NativeContext, NativeError, Object, Range, RegexValue, ResultValue, StaticSlot, UserFunction,
    Value,
};

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
        if let Function::Command { class, help } = &**function {
            return self.command_function(class, *help, positional, span);
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
            Function::Command { class, help } => {
                let value = self.command_function(class, *help, positional, span)?;
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
            Value::Class(class) => {
                let class = class.clone();
                let args = self.stack.split_off(callee + 1);
                self.stack.pop();
                let object = self.construct(&class, args, Vec::new(), span)?;
                self.stack.push(object);
                return Ok(false);
            }
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
            Value::Class(class) => {
                let class = class.clone();
                let (positional, named) = self.complex_args(kinds, callee + 1)?;
                self.stack.pop();
                let object = self.construct(&class, positional, named, span)?;
                self.stack.push(object);
                return Ok(false);
            }
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
            _ => unreachable!("only a script's functions get frames"),
        }
    }

    fn globals_of(&self, frame: usize) -> Rc<Globals> {
        match &*self.frames[frame].function {
            Function::User(user) => user.globals.clone(),
            _ => unreachable!("only a script's functions get frames"),
        }
    }

    fn upvalue(&self, frame: usize, i: u32) -> &Rc<RefCell<Value>> {
        match &*self.frames[frame].function {
            Function::User(user) => &user.upvalues[i as usize],
            _ => unreachable!("only a script's functions get frames"),
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
                match fast {
                    Some(v) => *l = v,
                    None if matches!(&*l, Value::Object(_)) => {
                        // An object gives the operator through its interface *(i.e. `IAdd`)*
                        let left = l.clone();
                        self.frames[fi].ip = ip;
                        let v = self.operator($op, left, r, span!())?;
                        *self.top() = v;
                    }
                    None => *l = arithmetic($op, l, &r).map_err(|m| op_error(m, span!()))?,
                }
            }};
        }
        macro_rules! compare_op {
            ($op:expr, $cmp:tt) => {{
                let r = self.pop();
                let l = self.top();
                match (&*l, &r) {
                    (Value::Int(a), Value::Int(b)) => *l = Value::Bool(a $cmp b),
                    (Value::Object(_), _) => {
                        let left = l.clone();
                        self.frames[fi].ip = ip;
                        let v = self.operator($op, left, r, span!())?;
                        *self.top() = v;
                    }
                    _ => *l = compare($op, l, &r).map_err(|m| op_error(m, span!()))?,
                }
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
                Op::LoadGlobalOrNull(g) => {
                    let slots = globals.slots.borrow();
                    let global = &slots[g as usize];
                    let v = if global.declared { global.value.clone() } else { Value::Null };
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
                    let v = self.host.builtins[b as usize].clone();
                    self.stack.push(v);
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
                Op::Eq | Op::NotEq => {
                    let r = self.pop();
                    let l = self.pop();
                    let mut equal = if let (Value::Object(_), Value::Object(_)) = (&l, &r) {
                        self.frames[fi].ip = ip;
                        self.objects_equal(l, r, span!())?
                    } else {
                        l.same(&r)
                    };
                    if op == Op::NotEq {
                        equal = !equal;
                    }
                    self.stack.push(Value::Bool(equal));
                }
                Op::Lt => compare_op!(BinaryOp::Lt, <),
                Op::Gt => compare_op!(BinaryOp::Gt, >),
                Op::LtEq => compare_op!(BinaryOp::LtEq, <=),
                Op::GtEq => compare_op!(BinaryOp::GtEq, >=),
                Op::Binary(op) => {
                    let r = self.pop();
                    let l = self.pop();
                    let object = matches!(&l, Value::Object(_)) || matches!((&r, op), (Value::Object(_), BinaryOp::In | BinaryOp::NotIn));
                    let v = if object {
                        self.frames[fi].ip = ip;
                        self.operator(op, l, r, span!())?
                    } else {
                        binary(op, l, r).map_err(|m| op_error(m, span!()))?
                    };
                    self.stack.push(v);
                }
                Op::Unary(op) => {
                    let v = self.pop();
                    let v = if let (Value::Object(_), UnaryOp::Neg) = (&v, op) {
                        self.frames[fi].ip = ip;
                        self.call_operator_method(&v, "neg", "INeg", "-", Vec::new(), span!())?
                    } else {
                        unary(op, v, span!())?
                    };
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
                        (Value::Object(_), Value::Object(_), BinaryOp::Eq | BinaryOp::NotEq) => {
                            self.frames[fi].ip = ip;
                            let equal = self.objects_equal(l, r, span!())?;
                            if op == BinaryOp::Eq { equal } else { !equal }
                        }
                        (_, _, BinaryOp::Eq) => l.same(&r),
                        (_, _, BinaryOp::NotEq) => !l.same(&r),
                        (Value::Object(_), _, _) => {
                            self.frames[fi].ip = ip;
                            matches!(self.operator(op, l, r, span!())?, Value::Bool(true))
                        }
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
                    // A value of `{...}` in a command: always whole arguments, never split
                    let v = self.pop();
                    let args = command_arguments(&v).map_err(|m| flow_error_as(exceptions::TYPE_ERROR, m, span!()))?;
                    self.stack.push(Value::list(args.into_iter().map(|a| Value::string(&a)).collect()));
                }
                Op::Command(n) => {
                    let parts = self.stack.split_off(self.stack.len() - n as usize);
                    let (program, args) = split_command(&parts).map_err(|m| flow_error_as(exceptions::VALUE_ERROR, m, span!()))?;
                    self.stack.push(Value::Command(Rc::new(RefCell::new(CommandValue::new(CommandKind::Direct { program, args })))));
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
                    let v = if let Value::Object(_) = &container {
                        self.frames[fi].ip = ip;
                        self.call_operator_method(&container, "get", "IIndex", "[]", vec![index], span!())?
                    } else {
                        index_value(container, index, span!())?
                    };
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
                    if let Value::Object(_) = &container {
                        // `$x[k] = v` on an object is its `set`, from `ISetIndex`
                        self.frames[fi].ip = ip;
                        self.call_operator_method(&container, "set", "ISetIndex", "[] =", vec![index, value.clone()], span!())?;
                    } else {
                        set_index(&container, index, value.clone(), span!())?;
                    }
                    self.stack.push(value);
                }
                Op::UpdateIndex { op, mode } => {
                    let rhs = self.pop();
                    let index = self.pop();
                    let container = self.pop();
                    let object = matches!(container, Value::Object(_));
                    if object {
                        self.frames[fi].ip = ip;
                    }
                    let old = if object {
                        self.call_operator_method(&container, "get", "IIndex", "[]", vec![index.clone()], span!())?
                    } else {
                        index_value(container.clone(), index.clone(), span!())?
                    };
                    if mode > 0 && old.as_f64().is_none() {
                        return error(format!("`++` and `--` only work on numbers, and this is a {}", old.type_name()), span!());
                    }
                    let new = if matches!(old, Value::Object(_)) {
                        self.operator(op, old.clone(), rhs, span!())?
                    } else {
                        binary(op, old.clone(), rhs).map_err(|m| op_error(m, span!()))?
                    };
                    if object {
                        self.call_operator_method(&container, "set", "ISetIndex", "[] =", vec![index, new.clone()], span!())?;
                    } else {
                        set_index(&container, index, new.clone(), span!())?;
                    }
                    self.stack.push(if mode == 2 { old } else { new });
                }
                Op::Destructure { names, rest } => {
                    let v = self.pop();
                    self.destructure(v, names as usize, rest, span!())?;
                }

                // ---- Types ----
                Op::Is { ty, catch, classes } => {
                    let ty = &proto.types[ty as usize];
                    let found = self.stack.split_off(self.stack.len() - classes as usize);
                    let v = self.pop();
                    let pairs: Vec<(&str, &Value)> = class_names(ty).into_iter().zip(found.iter()).collect();
                    let ok = types::matches(&v, ty, &pairs).map_err(|m| {
                        if catch { flow_error_as(exceptions::TYPE_ERROR, m, span!()) } else { flow_error(m, span!()) }
                    })?;
                    self.stack.push(Value::Bool(ok));
                }
                Op::As { ty, classes } => {
                    let ty = &proto.types[ty as usize];
                    let found = self.stack.split_off(self.stack.len() - classes as usize);
                    let pairs: Vec<(&str, &Value)> = class_names(ty).into_iter().zip(found.iter()).collect();
                    let v = self.top();
                    if !types::matches(v, ty, &pairs).map_err(|m| flow_error(m, span!()))? {
                        let message = format!("`.as({ty})` failed: the value is {}", describe(v));
                        return error_as(exceptions::TYPE_ERROR, message, span!());
                    }
                }

                // ---- Classes ----
                Op::Class(i) => {
                    let class = self.make_class(&proto, i as usize, span!())?;
                    self.stack.push(Value::Class(class));
                }
                Op::GetField(n) => {
                    let object = self.pop();
                    let v = get_field(&object, &proto.names[n as usize], span!())?;
                    self.stack.push(v);
                }
                Op::SetField(n) => {
                    let value = self.pop();
                    let object = self.pop();
                    set_field(&object, &proto.names[n as usize], value.clone(), span!())?;
                    self.stack.push(value);
                }
                Op::Static { name, variable } => {
                    let class = self.pop();
                    let v = get_static(&class, &proto.names[name as usize], variable, span!())?;
                    self.stack.push(v);
                }
                Op::SetStatic(name) => {
                    let value = self.pop();
                    let class = self.pop();
                    set_static(&class, &proto.names[name as usize], value.clone(), span!())?;
                    self.stack.push(value);
                }
                Op::DefineMember(name) => {
                    let value = self.pop();
                    let Value::Class(class) = self.pop() else { unreachable!("the compiler pushes the command") };
                    class.statics.borrow_mut().insert(proto.names[name as usize].clone(), StaticSlot { value, constant: true });
                }
                Op::DefineStatic { name, constant } => {
                    let value = self.pop();
                    let Value::Class(class) = self.pop() else { unreachable!("the compiler pushes the class") };
                    let key: Rc<str> = Rc::from(format!("${}", proto.names[name as usize]).as_str());
                    class.statics.borrow_mut().insert(key, StaticSlot { value, constant });
                }
                Op::SuperNew(argc) => {
                    self.frames[fi].ip = ip;
                    let args = self.stack.split_off(self.stack.len() - argc as usize);
                    self.super_new(args, Vec::new(), span!())?;
                }
                Op::SuperNewComplex(c) => {
                    self.frames[fi].ip = ip;
                    let kinds = &proto.calls[c as usize];
                    let (args, named) = self.complex_args(kinds, self.stack.len() - kinds.len())?;
                    self.super_new(args, named, span!())?;
                }
                Op::EnumValue { name, argc } => {
                    self.frames[fi].ip = ip;
                    let args = self.stack.split_off(self.stack.len() - argc as usize);
                    self.enum_value(&proto.names[name as usize], args, Vec::new(), span!())?;
                }
                Op::EnumValueComplex { name, call } => {
                    self.frames[fi].ip = ip;
                    let kinds = &proto.calls[call as usize];
                    let (args, named) = self.complex_args(kinds, self.stack.len() - kinds.len())?;
                    self.enum_value(&proto.names[name as usize], args, named, span!())?;
                }
                Op::SuperMethod { name, argc } => {
                    self.frames[fi].ip = ip;
                    let mut args = self.stack.split_off(self.stack.len() - argc as usize);
                    let this = self.pop();
                    let Value::Class(class) = self.pop() else { unreachable!("the compiler pushes the class") };
                    let name = &proto.names[name as usize];
                    let method = class.parent.as_ref().and_then(|p| p.methods.get(&**name).cloned());
                    let Some(method) = method else {
                        return error(format!("the superclass of `{}` has no method `{name}`", class.name), span!());
                    };
                    args.insert(0, this);
                    let v = self.call(&method, args, Vec::new(), span!())?;
                    self.stack.push(v);
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
                    let at = self.stack.len() - argc as usize - 1;
                    let class = match &self.stack[at] {
                        Value::Object(o) => Some(o.class.clone()),
                        Value::Exception(e) => e.object.as_ref().map(|o| o.class.clone()),
                        _ => None,
                    };
                    if let Some(class) = class {
                        if let Some(method) = class.methods.get(&*proto.names[name as usize]).cloned() {
                            if let Function::User(user) = &*method {
                                if user.proto.simple && argc as usize + 1 == user.proto.params.len() {
                                    // The method's frame starts at the object, its `@`
                                    self.stack.insert(at, Value::Function(method.clone()));
                                    self.push_frame(method, at + 1, u64::MAX, span!())?;
                                    reload!();
                                    continue;
                                }
                            }
                            let args = self.stack.split_off(at);
                            if self.enter(method, args, Vec::new(), span!())? {
                                reload!();
                            }
                            continue;
                        }
                    }
                    let args = self.stack.split_off(self.stack.len() - argc as usize);
                    let receiver = self.pop();
                    let v = methods::call(self, receiver, &proto.names[name as usize], args, span!())?;
                    self.stack.push(v);
                }
                Op::CallMethodComplex { name, call } => {
                    self.frames[fi].ip = ip;
                    let kinds = &proto.calls[call as usize];
                    let (mut args, named) = self.complex_args(kinds, self.stack.len() - kinds.len())?;
                    let receiver = self.pop();
                    let class = match &receiver {
                        Value::Object(o) => Some(o.class.clone()),
                        Value::Exception(e) => e.object.as_ref().map(|o| o.class.clone()),
                        _ => None,
                    };
                    if let Some(class) = class {
                        if let Some(method) = class.methods.get(&*proto.names[name as usize]).cloned() {
                            args.insert(0, receiver);
                            if self.enter(method, args, named, span!())? {
                                reload!();
                            }
                            continue;
                        }
                    }
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
                        Rc::new(ExceptionValue {
                            kind: exception.kind.clone(),
                            base: exception.base.clone(),
                            object: exception.object.clone(),
                            message: exception.message.clone(),
                            span: span!(),
                        })
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

    fn use_items(&mut self, proto: &Proto, i: usize, globals: &Rc<Globals>, span: Span) -> Exec<()> {
        let import = &proto.imports[i];
        let source = self.host.loader.resolve(&import.path).map_err(|m| flow_error(m, span))?;
        // Importing names needs their values now
        let module = self.load(source, span)?;
        let loading = self.host.loader.in_progress.iter().find(|(_, m)| Rc::ptr_eq(m, &module)).map(|(p, _)| p.clone());
        let mut later = Vec::new();
        for (name, variable, global, local) in &import.items {
            let key = if *variable { format!("${name}") } else { name.clone() };
            if !module.public.contains(&key) {
                let what = if *variable { "constant" } else { "function or type" };
                return error(format!("`{}` has no `pub` {what} named `{key}`", module.path), span);
            }
            let value = match (module.globals.get(&key), &loading) {
                (Some(v), _) => v,
                // The module uses this one back and is still loading: the value comes when it finishes
                (None, Some(_)) => {
                    later.push((key.clone(), *global));
                    continue;
                }
                (None, None) => Value::Null,
            };
            let mut slots = globals.slots.borrow_mut();
            let slot = &mut slots[*global as usize];
            if slot.declared {
                return error(format!("`{local}` already exists"), span);
            }
            slot.value = value;
            slot.declared = true;
            slot.constant = true;
        }
        if let (Some(path), false) = (loading, later.is_empty()) {
            self.host.loader.pending.push((path, globals.clone(), later));
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
            None if self.host.loader.in_progress.values().any(|m| Rc::ptr_eq(m, &module)) => error(
                format!("`{key}` of `{}` isn't ready yet: that module is still loading, because it uses this one. Modules that use each other can only use each other's functions and classes once they're loaded", module.path),
                span,
            ),
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
        // Modules that use each other: the one still loading, as far as it got
        if let Some(m) = self.host.loader.in_progress.get(&path) {
            return Ok(m.clone());
        }
        let shown = path.display().to_string();
        let source = self.host.loader.read(&path).map_err(|e| flow_error_as("IOError", format!("can't read the module '{shown}': {e}"), span))?;
        let file = self.host.loader.files.len() as u32;
        self.host.loader.files.push(shown.clone());
        let program = crate::parse_budgeted(&source, file, self.stack_budget).map_err(|d| flow_error(d.message, d.span))?;
        let globals = Rc::new(Globals::default());
        let (proto, public) = Compiler::new(&globals, &self.host.builtin_names, self.stack_budget)
            .module(&program, &shown)
            .map_err(|d| flow_error(d.message, d.span))?;

        let module = Rc::new(Module { path: shown, globals: globals.clone(), public });
        self.host.loader.loading.push(path.clone());
        self.host.loader.in_progress.insert(path.clone(), module.clone());
        let body = Rc::new(Function::User(UserFunction { proto, upvalues: Vec::new(), globals }));
        let result = self.call(&body, Vec::new(), Vec::new(), span);
        self.host.loader.loading.pop();
        self.host.loader.in_progress.remove(&path);
        result?;
        self.host.loader.cache.insert(path.clone(), module.clone());
        // The names other modules imported from it while it was loading
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.host.loader.pending).into_iter().partition(|(p, ..)| *p == path);
        self.host.loader.pending = waiting;
        for (_, importer, names) in ready {
            for (key, global) in names {
                let mut slots = importer.slots.borrow_mut();
                let slot = &mut slots[global as usize];
                slot.value = module.globals.get(&key).unwrap_or(Value::Null);
                slot.declared = true;
                slot.constant = true;
            }
        }
        Ok(module)
    }
}

// ---- Classes ----

impl Interpreter<'_> {
    /// `Op::Class`: the parts were pushed in order, so they're popped backwards.
    fn make_class(&mut self, proto: &Proto, i: usize, span: Span) -> Exec<Rc<Class>> {
        let info = &proto.classes[i];
        let mut function = || match self.stack.pop() {
            Some(Value::Function(f)) => f,
            _ => unreachable!("the compiler pushes the parts of a class"),
        };
        let statics: Vec<Rc<Function>> = (0..info.static_methods.len()).map(|_| function()).collect();
        let methods: Vec<Rc<Function>> = (0..info.methods.len()).map(|_| function()).collect();
        let constructor = info.constructor.then(&mut function);
        let init = info.init.then(&mut function);
        // The enums of a command's fields
        let mut enums = Vec::new();
        for _ in 0..info.command.as_ref().map(|c| c.enums).unwrap_or(0) {
            match self.pop() {
                Value::Class(c) if c.is_enum => enums.push(c),
                other => return error(format!("the fields of a command can be strings, numbers, bools or enums, not {}", describe(&other)), span),
            }
        }
        enums.reverse();
        let mut implemented = Vec::new();
        for _ in 0..info.interfaces {
            match self.pop() {
                Value::Class(c) if c.interface => implemented.push(c),
                other => {
                    let what = if info.interface { "extend" } else { "implement" };
                    return error(format!("`{}` can only {what} interfaces, and this is {}", info.name, describe(&other)), span);
                }
            }
        }
        implemented.reverse();
        let mut exception = None;
        let parent = if info.parent {
            match self.pop() {
                Value::Class(c) => {
                    exception = c.exception.clone();
                    Some(c)
                }
                // `extends ValueError`: a built-in exception, which gives the field `message`
                Value::Function(f) if exceptions::is_exception_type(f.name()) => {
                    exception = Some(Rc::from(f.name()));
                    None
                }
                other => return error(format!("`{}` can only extend a class, not a {}", info.name, other.type_label()), span),
            }
        } else {
            None
        };
        let mut fields: Vec<FieldSlot> = match &parent {
            Some(p) => p.fields.clone(),
            None if exception.is_some() => vec![FieldSlot { name: Rc::from("message"), constant: true, nullable: false, has_value: true }],
            None => Vec::new(),
        };
        for (name, constant, nullable, has_value) in &info.fields {
            if fields.iter().any(|f| f.name == *name) {
                return error(format!("`{}` declares the field `{name}` again, which its superclass already has", info.name), span);
            }
            fields.push(FieldSlot { name: name.clone(), constant: *constant, nullable: *nullable, has_value: *has_value });
        }
        let mut table = parent.as_ref().map(|p| p.methods.clone()).unwrap_or_default();
        for (name, f) in info.methods.iter().zip(methods.into_iter().rev()) {
            table.insert(name.clone(), f);
        }
        // Every interface, also the ones of the superclass and the ones each interface extends
        let mut interfaces: Vec<Rc<Class>> = parent.as_ref().map(|p| p.interfaces.clone()).unwrap_or_default();
        for i in &implemented {
            for each in std::iter::once(i).chain(i.interfaces.iter()) {
                if !interfaces.iter().any(|known| Rc::ptr_eq(known, each)) {
                    interfaces.push(each.clone());
                }
            }
        }
        // The defaults of the interfaces, for the methods the class doesn't have
        for i in &interfaces {
            for (name, f) in &i.methods {
                table.entry(name.clone()).or_insert_with(|| f.clone());
            }
        }
        if !info.interface {
            for i in &interfaces {
                if let Some(missing) = i.required.iter().find(|m| !table.contains_key(*m)) {
                    return error(format!("`{}` implements `{}`, but it has no method `{missing}`", info.name, i.name), span);
                }
            }
        }
        let mut static_table = IndexMap::new();
        for (name, f) in info.static_methods.iter().zip(statics.into_iter().rev()) {
            static_table.insert(name.clone(), StaticSlot { value: Value::Function(f), constant: true });
        }
        let class = Rc::new(Class {
            name: info.name.clone(),
            interface: info.interface,
            exception,
            is_enum: info.is_enum,
            values: RefCell::new(Vec::new()),
            parent,
            interfaces,
            required: info.required.clone(),
            fields,
            methods: table,
            statics: RefCell::new(static_table),
            constructor,
            init,
            command: info.command.clone().map(|c| (c, enums)),
        });
        if info.is_enum {
            enum_statics(&class);
        }
        if class.command.is_some() {
            let mut statics = class.statics.borrow_mut();
            for help in [false, true] {
                let f = Function::Command { class: Rc::downgrade(&class), help };
                statics.insert(Rc::from(if help { "help" } else { "parse" }), StaticSlot { value: Value::Function(Rc::new(f)), constant: true });
            }
        }
        Ok(class)
    }

    /// Creates a value of an enum, when the enum is declared.
    fn enum_value(&mut self, name: &Rc<str>, args: Vec<Value>, named: Named, span: Span) -> Exec<()> {
        let Value::Class(class) = self.pop() else { unreachable!("the compiler pushes the enum") };
        let object = Rc::new(Object {
            class: class.clone(),
            enum_name: Some(name.clone()),
            fields: RefCell::new(vec![Value::Null; class.fields.len()]),
            building: std::cell::Cell::new(true),
        });
        let value = Value::Object(object.clone());
        self.init_fields(&class, &value, span)?;
        self.run_constructor(&class, &value, args, named, span)?;
        object.building.set(false);
        class.statics.borrow_mut().insert(name.clone(), StaticSlot { value: value.clone(), constant: true });
        class.values.borrow_mut().push(value);
        Ok(())
    }

    /// An object made from named values, for `Git::parse()`.
    pub(crate) fn construct_object(&mut self, class: &Rc<Class>, named: Named, span: Span) -> Exec<Value> {
        self.construct(class, Vec::new(), named, span)
    }

    /// `Point(1, 2)`: a new object, with its fields' values, after its constructor.
    fn construct(&mut self, class: &Rc<Class>, positional: Vec<Value>, named: Named, span: Span) -> Exec<Value> {
        if class.interface {
            return error_as(exceptions::TYPE_ERROR, format!("`{}` is an interface: it doesn't create objects, classes implement it", class.name), span);
        }
        if class.is_enum {
            return error_as(exceptions::TYPE_ERROR, format!("`{}` is an enum: its values are created where it's declared *(i.e. `{}::...`)*", class.name, class.name), span);
        }
        let mut fields = vec![Value::Null; class.fields.len()];
        if class.exception.is_some() {
            // The message of an exception is empty unless it's given
            fields[0] = Value::string("");
        }
        let object = Rc::new(Object { class: class.clone(), enum_name: None, fields: RefCell::new(fields), building: std::cell::Cell::new(true) });
        let value = Value::Object(object.clone());
        self.init_fields(class, &value, span)?;
        self.run_constructor(class, &value, positional, named, span)?;
        object.building.set(false);
        if let Some(base) = &class.exception {
            // An exception can be thrown and caught like the built-in ones
            let message = match &object.fields.borrow()[0] {
                Value::Str(s) => s.to_string(),
                other => other.to_string(),
            };
            return Ok(Value::Exception(Rc::new(ExceptionValue { kind: class.name.clone(), base: base.clone(), object: Some(object), message, span: Span::default() })));
        }
        Ok(value)
    }

    /// The values written where the fields are declared, the superclass's first.
    fn init_fields(&mut self, class: &Rc<Class>, object: &Value, span: Span) -> Exec<()> {
        if let Some(parent) = &class.parent {
            self.init_fields(parent, object, span)?;
        }
        if let Some(init) = &class.init {
            self.call(init, vec![object.clone()], Vec::new(), span)?;
        }
        Ok(())
    }

    /// Runs the class's `new` on the object, or the constructor that takes every field.
    fn run_constructor(&mut self, class: &Rc<Class>, object: &Value, mut positional: Vec<Value>, named: Named, span: Span) -> Exec<()> {
        if let Some(constructor) = &class.constructor {
            positional.insert(0, object.clone());
            self.call(constructor, positional, named, span)?;
            return Ok(());
        }
        let mut parent = class.parent.as_deref();
        while let Some(p) = parent {
            if p.constructor.is_some() {
                return error(
                    format!("`{}` needs its own `new`, which calls `@super(...)`, since its superclass `{}` has one", class.name, p.name),
                    span,
                );
            }
            parent = p.parent.as_deref();
        }
        let Value::Object(o) = object else { unreachable!("constructors run on objects") };
        if positional.len() > class.fields.len() {
            let n = class.fields.len();
            return error_as(
                exceptions::TYPE_ERROR,
                format!("`{}` takes {n} argument{}, but it was given {}", class.name, if n == 1 { "" } else { "s" }, positional.len()),
                span,
            );
        }
        let mut given = vec![false; class.fields.len()];
        {
            let mut fields = o.fields.borrow_mut();
            for (i, v) in positional.into_iter().enumerate() {
                fields[i] = v;
                given[i] = true;
            }
            for (name, v, arg_span) in named {
                let Some(i) = class.field(&name) else {
                    return error_as(exceptions::TYPE_ERROR, format!("`{}` has no field named `{name}`", class.name), arg_span);
                };
                if given[i] {
                    return error_as(exceptions::TYPE_ERROR, format!("`{name}` was given twice"), arg_span);
                }
                fields[i] = v;
                given[i] = true;
            }
        }
        for (i, slot) in class.fields.iter().enumerate() {
            if !given[i] && !slot.has_value && !slot.nullable {
                return error_as(exceptions::TYPE_ERROR, format!("`{}` needs a value for `{}`", class.name, slot.name), span);
            }
        }
        Ok(())
    }

    /// An operator on an object, through the method of its interface *(i.e.
    /// `$a + $b` is `$a.add($b)`, from `IAdd`)*.
    fn operator(&mut self, op: BinaryOp, l: Value, r: Value, span: Span) -> Exec<Value> {
        use BinaryOp::*;
        let (method, interface) = match op {
            Add => ("add", "IAdd"),
            Sub => ("sub", "ISub"),
            Mul => ("mul", "IMul"),
            Div => ("div", "IDiv"),
            Rem => ("mod", "IMod"),
            Pow => ("pow", "IPow"),
            Lt | Gt | LtEq | GtEq => {
                let order = self.call_operator_method(&l, "compare", "IOrdered", op.symbol(), vec![r], span)?;
                let Some(n) = order.as_f64() else {
                    return error_as(exceptions::TYPE_ERROR, format!("`compare` must return a number, not a {}", order.type_label()), span);
                };
                return Ok(Value::Bool(match op {
                    Lt => n < 0.0,
                    Gt => n > 0.0,
                    LtEq => n <= 0.0,
                    _ => n >= 0.0,
                }));
            }
            Eq | NotEq => {
                let equal = self.objects_equal(l, r, span)?;
                return Ok(Value::Bool(if op == Eq { equal } else { !equal }));
            }
            In | NotIn => {
                let found = self.call_operator_method(&r, "contains", "IContains", "-in", vec![l], span)?;
                let found = matches!(found, Value::Bool(true));
                return Ok(Value::Bool(if op == In { found } else { !found }));
            }
            other => {
                let message = format!("`{}` doesn't work on objects", other.symbol());
                return error_as(exceptions::TYPE_ERROR, message, span);
            }
        };
        self.call_operator_method(&l, method, interface, op.symbol(), vec![r], span)
    }

    /// Calls the method behind an operator, or says which interface gives it.
    pub(crate) fn call_operator_method(&mut self, object: &Value, method: &str, interface: &str, symbol: &str, mut args: Vec<Value>, span: Span) -> Exec<Value> {
        let Value::Object(o) = object else {
            return error_as(exceptions::TYPE_ERROR, format!("`{symbol}` needs an object here, not a {}", object.type_label()), span);
        };
        let implements = self.host.builtin_interface(interface).is_some_and(|i| o.class.is_a(&i));
        let found = o.class.methods.get(method).cloned().filter(|_| implements);
        let Some(function) = found else {
            let message = format!("`{}` has no `{symbol}`: implement `{interface}` to give it one", o.class.name);
            return error_as(exceptions::TYPE_ERROR, message, span);
        };
        args.insert(0, object.clone());
        self.call(&function, args, Vec::new(), span)
    }

    /// `==` on two objects: `equals?` when their class implements `IEquals`,
    /// and otherwise, whether they're the same object. Objects of unrelated
    /// classes are never equal.
    fn objects_equal(&mut self, l: Value, r: Value, span: Span) -> Exec<bool> {
        let (Value::Object(a), Value::Object(b)) = (&l, &r) else { return Ok(l.same(&r)) };
        if Rc::ptr_eq(a, b) {
            return Ok(true);
        }
        let related = a.class.is_a(&b.class) || b.class.is_a(&a.class);
        let equals = self.host.builtin_interface("IEquals").is_some_and(|i| a.class.is_a(&i));
        if !related || !equals {
            return Ok(false);
        }
        let v = self.call_operator_method(&l, "equals?", "IEquals", "==", vec![r], span)?;
        Ok(matches!(v, Value::Bool(true)))
    }

    /// `@super(...)`: the superclass's constructor, on the same object.
    fn super_new(&mut self, args: Vec<Value>, named: Named, span: Span) -> Exec<()> {
        let this = self.pop();
        let Value::Class(class) = self.pop() else { unreachable!("the compiler pushes the class") };
        let Some(parent) = class.parent.clone() else {
            if class.exception.is_some() {
                // The constructor of a built-in exception takes the message
                let message = args.into_iter().next().or_else(|| named.into_iter().find(|(n, ..)| n == "message").map(|(_, v, _)| v));
                if let (Value::Object(o), Some(m)) = (&this, message) {
                    o.fields.borrow_mut()[0] = m;
                }
                self.stack.push(Value::Null);
                return Ok(());
            }
            return error(format!("`{}` has no superclass for `@super(...)`", class.name), span);
        };
        self.run_constructor(&parent, &this, args, named, span)?;
        self.stack.push(Value::Null);
        Ok(())
    }
}

/// `values()`, `from()` and `fromIgnoreCase()`, which every enum has.
fn enum_statics(class: &Rc<Class>) {
    use crate::value::NativeFunction;
    let weak = Rc::downgrade(class);
    let mut statics = class.statics.borrow_mut();
    let mut add = |name: &str, params: &[&str], f: Box<crate::value::NativeFn>| {
        let function = NativeFunction { name: format!("{}::{name}", class.name), params: params.iter().map(|p| p.to_string()).collect(), func: f };
        statics.entry(Rc::from(name)).or_insert(StaticSlot { value: Value::Function(Rc::new(Function::Native(function))), constant: true });
    };
    let w = weak.clone();
    add("values", &[], Box::new(move |_, _| Ok(Value::list(w.upgrade().map(|c| c.values.borrow().clone()).unwrap_or_default()))));
    for ignore_case in [false, true] {
        let w = weak.clone();
        let name = if ignore_case { "fromIgnoreCase" } else { "from" };
        add(name, &["name"], Box::new(move |_, args| {
            let Some(class) = w.upgrade() else { return Ok(Value::Null) };
            let text = match args.first() {
                Some(Value::Str(s)) => s.clone(),
                Some(other) => return Err(NativeError::new(exceptions::TYPE_ERROR, format!("`{}::{name}` takes a string, not a {}", class.name, other.type_label()))),
                None => return Err(NativeError::new(exceptions::TYPE_ERROR, format!("`{}::{name}` takes the name of a value", class.name))),
            };
            let values = class.values.borrow();
            let found = values.iter().find(|v| match v {
                Value::Object(o) => {
                    let n = o.enum_name.as_deref().unwrap_or("");
                    if ignore_case { n.eq_ignore_ascii_case(&text) } else { n == &*text }
                }
                _ => false,
            });
            match found {
                Some(v) => Ok(v.clone()),
                None => {
                    let names: Vec<String> = values.iter().map(|v| match v {
                        Value::Object(o) => o.enum_name.as_deref().unwrap_or("").to_string(),
                        _ => String::new(),
                    }).collect();
                    Err(NativeError::new(exceptions::VALUE_ERROR, format!("`{}` has no value `{text}`: it has {}", class.name, names.join(", "))))
                }
            }
        }));
    }
}

fn get_field(object: &Value, name: &str, span: Span) -> Exec<Value> {
    if let Value::Exception(e) = object {
        if let Some(o) = &e.object {
            return get_field(&Value::Object(o.clone()), name, span);
        }
    }
    match object {
        Value::Object(o) => match o.class.field(name) {
            Some(i) => Ok(o.fields.borrow()[i].clone()),
            None => error(format!("`{}` has no field `{name}`", o.class.name), span),
        },
        Value::Null => error_as(exceptions::TYPE_ERROR, format!("can't read `.{name}` of `null`"), span),
        other => error(format!("a {} has no fields: `.{name}` only works on objects", other.type_name()), span),
    }
}

fn set_field(object: &Value, name: &str, value: Value, span: Span) -> Exec<()> {
    if let Value::Exception(e) = object {
        if let Some(o) = &e.object {
            return set_field(&Value::Object(o.clone()), name, value, span);
        }
    }
    match object {
        Value::Object(o) => match o.class.field(name) {
            Some(i) => {
                if o.class.fields[i].constant && !o.building.get() {
                    return error(format!("`{name}` is a `const` field of `{}`: it can only be set while the object is created", o.class.name), span);
                }
                o.fields.borrow_mut()[i] = value;
                Ok(())
            }
            None => error(format!("`{}` has no field `{name}`", o.class.name), span),
        },
        Value::Null => error_as(exceptions::TYPE_ERROR, format!("can't set `.{name}` of `null`"), span),
        other => error(format!("a {} has no fields: `.{name}` only works on objects", other.type_name()), span),
    }
}

fn get_static(class: &Value, name: &str, variable: bool, span: Span) -> Exec<Value> {
    let Value::Class(class) = class else {
        return error(format!("`::` needs a class, and this is a {}", class.type_label()), span);
    };
    let key = if variable { format!("${name}") } else { name.to_string() };
    match class.statics.borrow().get(key.as_str()) {
        Some(slot) => Ok(slot.value.clone()),
        None => error(format!("`{}` has no static member `{key}`", class.name), span),
    }
}

fn set_static(class: &Value, name: &str, value: Value, span: Span) -> Exec<()> {
    let Value::Class(class) = class else {
        return error(format!("`::` needs a class, and this is a {}", class.type_label()), span);
    };
    let key = format!("${name}");
    match class.statics.borrow_mut().get_mut(key.as_str()) {
        Some(slot) if slot.constant => error(format!("`{}::{key}` is a constant, it can't be changed", class.name), span),
        Some(slot) => {
            slot.value = value;
            Ok(())
        }
        None => error(format!("`{}` has no static field `{key}`", class.name), span),
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
            let Some(i) = index.as_whole() else {
                return Err(index_error(format!("indexes must be whole numbers, and this is {}", describe(&index)), span));
            };
            // Past either end, reading gives `null`, like a missing key of a map
            let pos = if i < 0 { len as i64 + i } else { i };
            if pos < 0 || pos >= len as i64 {
                return Ok(Value::Null);
            }
            let pos = pos as usize;
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
/// The arguments a value gives a command: one, or one per element of a list.
fn command_arguments(value: &Value) -> Result<Vec<String>, String> {
    match value {
        Value::Str(s) => Ok(vec![s.to_string()]),
        Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Symbol(_) => Ok(vec![value.to_string()]),
        Value::List(items) => {
            let mut out = Vec::new();
            for item in items.borrow().iter() {
                out.extend(command_arguments(item)?);
            }
            Ok(out)
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
