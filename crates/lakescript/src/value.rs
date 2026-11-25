//! Values while a script runs.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::token::Span;

#[derive(Clone)]
pub enum Value {
    Null,
    Bool(bool),
    /// A `number` stored as an integer. The user never sees the difference with `Float`.
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    /// `bytes`: binary data that doesn't change, like a string of bytes.
    Bytes(Rc<[u8]>),
    /// A type given as a value *(i.e. `list<User>`)*, with the top level
    /// where the names in it are found.
    Type(Rc<(crate::ast::TypeExpr, std::rc::Weak<crate::modules::Globals>)>),
    List(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<IndexMap<Key, Value>>>),
    Range(Range),
    Function(Rc<Function>),
    Regex(Rc<RegexValue>),
    Exception(Rc<ExceptionValue>),
    /// What `try expr` returns.
    Result(Rc<ResultValue>),
    /// A command from `%x"..."` or `process::command()`. Its setup methods change it.
    Command(Rc<RefCell<crate::command::CommandValue>>),
    /// A class: called to create objects, and `::` reaches its static members.
    Class(Rc<Class>),
    /// An object of a class.
    Object(Rc<Object>),
    /// Rust data that a module of the standard library keeps in a value,
    /// which scripts never see *(i.e. an element of a page, for std/html)*.
    /// It's freed when nothing uses it anymore.
    Handle(Rc<dyn std::any::Any>),
}

/// A class while the script runs *(see `ideas/language/classes.md`)*.
pub struct Class {
    pub(crate) name: Rc<str>,
    pub(crate) interface: bool,
    /// The built-in exception it extends, directly or through its superclasses.
    pub(crate) exception: Option<Rc<str>>,
    pub(crate) is_enum: bool,
    /// The values of an enum, in order.
    pub(crate) values: RefCell<Vec<Value>>,
    pub(crate) parent: Option<Rc<Class>>,
    /// Every interface it implements, also through its superclasses and the
    /// interfaces they extend. For an interface, the ones it extends.
    pub(crate) interfaces: Vec<Rc<Class>>,
    /// For an interface: its methods without a default.
    pub(crate) required: Vec<Rc<str>>,
    /// Every field of its objects, the superclass's first.
    pub(crate) fields: Vec<FieldSlot>,
    /// Its methods and the ones it inherits, by name.
    pub(crate) methods: std::collections::HashMap<Rc<str>, Rc<Function>>,
    /// The same methods by number, which a place that calls one remembers
    /// *(see `bytecode::InlineCache`)*.
    pub(crate) method_list: Vec<Rc<Function>>,
    /// The method of each operator interface it implements, in the order of
    /// `operators::INTERFACES` *(i.e. its `equals?` for `==`)*, found once
    /// when the class is made instead of by name every time. Empty for the
    /// interfaces themselves.
    pub(crate) operators: Vec<Option<Rc<Function>>>,
    pub(crate) statics: RefCell<IndexMap<Rc<str>, StaticSlot>>,
    /// `new`, or `None` for the one that takes every field.
    pub(crate) constructor: Option<Rc<Function>>,
    /// Gives the fields declared in this class the values written where they're declared.
    pub(crate) init: Option<Rc<Function>>,
    /// Or, when they need no code to make, those values, by the field's
    /// place in its objects *(see `bytecode::PlainDefault`)*.
    pub(crate) defaults: Vec<(usize, crate::bytecode::PlainDefault)>,
    /// For a command: its arguments, and the enums of their types.
    pub(crate) command: Option<(Rc<crate::bytecode::CommandInfo>, Vec<Rc<Class>>)>,
}

#[derive(Clone)]
pub struct FieldSlot {
    pub(crate) name: Rc<str>,
    pub(crate) constant: bool,
    /// Whether its type is nullable, so the constructor can leave it as `null`.
    pub(crate) nullable: bool,
    /// Whether it has a value where it's declared.
    pub(crate) has_value: bool,
    /// Its type, and the top level of the module of its class, where the
    /// names in the type are found *(for `json::parse($text, Class)`)*.
    pub(crate) ty: Option<Rc<crate::ast::TypeExpr>>,
    pub(crate) home: std::rc::Weak<crate::modules::Globals>,
}

pub struct StaticSlot {
    pub(crate) value: Value,
    pub(crate) constant: bool,
}

pub struct Object {
    pub(crate) class: Rc<Class>,
    /// For a value of an enum: its name *(i.e. `SICK`)*.
    pub(crate) enum_name: Option<Rc<str>>,
    pub(crate) fields: RefCell<Vec<Value>>,
    /// While its constructor runs, its `const` fields can still be set.
    pub(crate) building: std::cell::Cell<bool>,
}

impl Class {
    pub fn field(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| &*f.name == name)
    }

    /// `true` if it's this class, one of its superclasses, or an interface it implements.
    pub fn is_a(&self, other: &Class) -> bool {
        if std::ptr::eq(self, other) || self.interfaces.iter().any(|i| std::ptr::eq(&**i, other)) {
            return true;
        }
        let mut class = self.parent.as_deref();
        while let Some(c) = class {
            if std::ptr::eq(c, other) {
                return true;
            }
            class = c.parent.as_deref();
        }
        false
    }
}

pub struct ExceptionValue {
    /// `"ValueError"`, `"FileNotFound"`, or the class of a custom one *(i.e. `"ConfigError"`)*.
    pub(crate) kind: Rc<str>,
    /// The built-in exception it is or extends, for `catch (ValueError $e)`.
    pub(crate) base: Rc<str>,
    /// For an exception of a class that extends a built-in one: its object, with its fields and methods.
    pub(crate) object: Option<Rc<Object>>,
    pub(crate) message: String,
    /// Where it was thrown.
    pub(crate) span: Span,
    /// The calls it was thrown through, taken the first time it's thrown, so
    /// throwing it again keeps them. `None` before it's thrown.
    pub(crate) trace: RefCell<Option<Rc<Vec<TraceEntry>>>>,
}

impl ExceptionValue {
    /// Where it's shown to be thrown: the last call of its trace, which leaves
    /// out the code of the standard library *(i.e. the line of the script that
    /// called `time::parse()`, not the one inside `time.lks`)*, or where it
    /// was thrown before it has a trace.
    pub fn place(&self) -> Span {
        self.trace.borrow().as_ref().and_then(|t| t.last()).map(|t| t.span).unwrap_or(self.span)
    }
}

/// A call that hadn't finished when an exception was thrown: the function
/// it was in, and where it was in it *(see Tracebacks in ideas/language/errors.md)*.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceEntry {
    pub(crate) function: Rc<str>,
    pub(crate) span: Span,
    /// More than 1 for a function that called itself from the same place.
    pub(crate) times: u32,
}

pub enum ResultValue {
    Ok(Value),
    Error(Value),
}

/// A compiled `%r"..."` regex.
pub struct RegexValue {
    pub(crate) pattern: String,
    pub(crate) flags: String,
    pub(crate) compiled: regex::Regex,
}

/// A range of whole numbers. `end` is excluded unless `inclusive`. It counts
/// down when `start` is bigger than `end`. `step` is always positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub(crate) start: i64,
    pub(crate) end: i64,
    pub(crate) inclusive: bool,
    pub(crate) step: i64,
}

impl Range {
    pub fn iter(self) -> impl Iterator<Item = i64> {
        let down = self.start > self.end;
        let mut next = Some(self.start);
        std::iter::from_fn(move || {
            let current = next?;
            let inside = match (down, self.inclusive) {
                (false, false) => current < self.end,
                (false, true) => current <= self.end,
                (true, false) => current > self.end,
                (true, true) => current >= self.end,
            };
            if !inside {
                next = None;
                return None;
            }
            next = if down { current.checked_sub(self.step) } else { current.checked_add(self.step) };
            Some(current)
        })
    }

    /// How many numbers it has, without walking it.
    pub fn len(self) -> usize {
        let (start, end) = (self.start as i128, self.end as i128);
        let span = if start <= end { end - start } else { start - end } + self.inclusive as i128;
        let step = self.step.max(1) as i128;
        ((span + step - 1) / step).clamp(0, usize::MAX as i128) as usize
    }

    pub fn contains(self, n: i64) -> bool {
        let (low, high) = if self.start <= self.end { (self.start, self.end) } else { (self.end, self.start) };
        let in_bounds = if self.inclusive {
            low <= n && n <= high
        } else if self.start <= self.end {
            low <= n && n < high
        } else {
            low < n && n <= high
        };
        in_bounds && (n - self.start).abs() % self.step == 0
    }
}

/// A map key. Numbers that are whole are always stored as `Int`, so `1` and
/// `1.0` are the same key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Int(i64),
    /// The bits of a float that isn't whole.
    Float(u64),
    Str(Rc<str>),
}

impl Key {
    pub fn from_value(value: &Value) -> Option<Key> {
        Some(match value {
            Value::Int(n) => Key::Int(*n),
            Value::Float(f) => match whole(*f) {
                Some(n) => Key::Int(n),
                None => Key::Float(f.to_bits()),
            },
            Value::Str(s) => Key::Str(s.clone()),
            _ => return None,
        })
    }

    pub fn to_value(&self) -> Value {
        match self {
            Key::Int(n) => Value::Int(*n),
            Key::Float(bits) => Value::Float(f64::from_bits(*bits)),
            Key::Str(s) => Value::Str(s.clone()),
        }
    }
}

/// The value of a float if it's a whole number that fits in an `i64`.
pub fn whole(f: f64) -> Option<i64> {
    (f.fract() == 0.0 && f >= -9.223_372_036_854_776e18 && f < 9.223_372_036_854_776e18).then_some(f as i64)
}

pub enum Function {
    User(UserFunction),
    Native(NativeFunction),
    /// `Git::parse()` or `Git::help()` of a command.
    Command { class: std::rc::Weak<Class>, help: bool },
    /// A function of the standard library that runs Lakescript code, so it
    /// needs the VM *(i.e. `template::render`)*.
    Vm(VmFunction),
}

/// What a `Function::Vm` runs: the VM, the arguments, and where it was called.
pub type VmFn = for<'a, 'b> fn(&'a mut crate::interpreter::Interpreter<'b>, Vec<Value>, crate::token::Span) -> crate::interpreter::Exec<Value>;

pub struct VmFunction {
    pub(crate) name: String,
    /// The names of the parameters, so they can be given by name.
    pub(crate) params: Vec<String>,
    pub(crate) func: VmFn,
}

/// A function written in Lakescript: its code, the variables of the functions
/// around it that it uses, and the top level of the script or module it's in.
pub struct UserFunction {
    pub(crate) proto: Rc<crate::bytecode::Proto>,
    pub(crate) upvalues: Vec<Rc<RefCell<Value>>>,
    pub(crate) globals: Rc<crate::modules::Globals>,
}

/// What a native function can use besides its arguments.
pub struct NativeContext<'a> {
    /// Where the script's stdout goes.
    pub output: &'a mut dyn std::io::Write,
    /// Where the script's stderr goes.
    pub error: &'a mut dyn std::io::Write,
    pub(crate) input: &'a mut dyn std::io::BufRead,
    pub(crate) input_piped: bool,
    pub(crate) inherit_stdio: bool,
    pub(crate) exit_allowed: bool,
    pub(crate) state: &'a mut crate::host::ScriptState,
    /// Set by `exit()`: the script stops with this code once the function returns.
    pub(crate) exit: Option<i32>,
}

/// An error from a native function: it's thrown as an exception of `kind`.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeError {
    /// One of the built-in exceptions *(i.e. `"ValueError"`)*.
    pub kind: String,
    pub message: String,
}

impl NativeError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        NativeError { kind: kind.to_string(), message: message.into() }
    }
}

/// A plain message is thrown as a generic `Exception`.
impl From<String> for NativeError {
    fn from(message: String) -> Self {
        NativeError::new("Exception", message)
    }
}

impl From<&str> for NativeError {
    fn from(message: &str) -> Self {
        NativeError::new("Exception", message)
    }
}

pub type NativeResult = Result<Value, NativeError>;

pub type NativeFn = dyn Fn(&mut NativeContext, Vec<Value>) -> NativeResult;

pub struct NativeFunction {
    pub(crate) name: String,
    /// The names of the parameters, so they can be given by name *(i.e.
    /// `json::stringify($v, indent: 2)`)*. Empty if they can't.
    pub(crate) params: Vec<String>,
    pub(crate) func: Box<NativeFn>,
}

impl Function {
    pub fn name(&self) -> &str {
        match self {
            Function::User(f) => &f.proto.name,
            Function::Native(f) => &f.name,
            Function::Vm(f) => &f.name,
            Function::Command { help: true, .. } => "help",
            Function::Command { .. } => "parse",
        }
    }
}

impl Value {
    /// A copy, like `clone()`, without a call for the values that are only
    /// bits *(numbers, bools and `null`)*: the VM copies those all the time.
    #[inline(always)]
    pub fn copy(&self) -> Value {
        match self {
            Value::Int(n) => Value::Int(*n),
            Value::Float(f) => Value::Float(*f),
            Value::Bool(b) => Value::Bool(*b),
            Value::Null => Value::Null,
            other => other.clone(),
        }
    }

    /// Drops it, without a call for the values that are only bits.
    #[inline(always)]
    pub fn discard(self) {
        match self {
            Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Null => std::mem::forget(self),
            other => drop(other),
        }
    }

    pub fn string(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) | Value::Float(_) => "number",
            Value::Str(_) => "string",
            Value::Bytes(_) => "bytes",
            Value::Type(_) => "type",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Range(_) => "range",
            Value::Function(_) => "function",
            Value::Regex(_) => "regex",
            Value::Exception(_) => "exception",
            Value::Result(_) => "result",
            Value::Command(_) => "command",
            Value::Class(_) => "class",
            Value::Object(_) => "object",
            Value::Handle(_) => "native",
        }
    }

    /// The name of its type for messages: the class of an object *(i.e. `Point`)*.
    pub fn type_label(&self) -> String {
        match self {
            Value::Object(o) => o.class.name.to_string(),
            other => other.type_name().to_string(),
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// The value as a whole number, if it is one *(i.e. `4.0` works, `4.5` doesn't)*.
    pub fn as_whole(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Float(f) => whole(*f),
            _ => None,
        }
    }

    /// `==`: by contents for numbers, booleans, null, strings and
    /// ranges, by reference for the rest. Values of different types are never equal.
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (a @ (Value::Int(_) | Value::Float(_)), b @ (Value::Int(_) | Value::Float(_))) => a.as_f64() == b.as_f64(),
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Range(a), Value::Range(b)) => a == b,
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
            (Value::Map(a), Value::Map(b)) => Rc::ptr_eq(a, b),
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Regex(a), Value::Regex(b)) => Rc::ptr_eq(a, b),
            (Value::Exception(a), Value::Exception(b)) => Rc::ptr_eq(a, b),
            (Value::Result(a), Value::Result(b)) => Rc::ptr_eq(a, b),
            (Value::Command(a), Value::Command(b)) => Rc::ptr_eq(a, b),
            (Value::Class(a), Value::Class(b)) => Rc::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            (Value::Handle(a), Value::Handle(b)) => std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b)),
            _ => false,
        }
    }

    /// `.equals()` and `-in`: like `==`, but lists and maps compare their contents.
    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::List(a), Value::List(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                // A pair that's already being compared is inside itself: equal as far as it goes
                let Some(_comparing) = Visiting::enter(&COMPARING, (Rc::as_ptr(a) as usize, Rc::as_ptr(b) as usize)) else { return true };
                let (a, b) = (a.borrow(), b.borrow());
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            (Value::Map(a), Value::Map(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let Some(_comparing) = Visiting::enter(&COMPARING, (Rc::as_ptr(a) as usize, Rc::as_ptr(b) as usize)) else { return true };
                let (a, b) = (a.borrow(), b.borrow());
                a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| v.equals(w)))
            }
            _ => self.same(other),
        }
    }

    /// How the value looks inside a list or a map *(i.e. `'a'` for a string)*.
    pub fn nested(&self) -> String {
        struct Nested<'a>(&'a Value);
        impl fmt::Display for Nested<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.write_nested(f)
            }
        }
        Nested(self).to_string()
    }

    /// How the value looks inside a list or a map: strings get quotes and
    /// symbols keep their `:`.
    fn write_nested(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")),
            other => write!(f, "{other}"),
        }
    }
}

thread_local! {
    /// The lists, maps and objects being written, to stop at one that contains itself.
    static PRINTING: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
    /// The pairs of lists or maps being compared by `.equals()`.
    static COMPARING: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
}

/// A value being walked, noted while it is, so a value that contains itself
/// is found instead of walked forever *(i.e. `$list << $list`)*.
pub(crate) struct Visiting(&'static std::thread::LocalKey<RefCell<Vec<(usize, usize)>>>);

impl Visiting {
    /// Notes that two lists or two maps are being compared, for the
    /// comparisons that call `equals?` *(see `Interpreter::values_equal`)*.
    /// `None` when they already are.
    pub(crate) fn comparing(a: usize, b: usize) -> Option<Visiting> {
        Visiting::enter(&COMPARING, (a, b))
    }

    /// `None` when `what` is already being walked.
    fn enter(stack: &'static std::thread::LocalKey<RefCell<Vec<(usize, usize)>>>, what: (usize, usize)) -> Option<Visiting> {
        stack.with(|s| {
            let mut s = s.borrow_mut();
            if s.contains(&what) {
                return None;
            }
            s.push(what);
            Some(Visiting(stack))
        })
    }
}

impl Drop for Visiting {
    fn drop(&mut self) {
        self.0.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

pub fn format_number(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if f == 0.0 {
        return "0".into();
    }
    let abs = f.abs();
    if !(1e-6..1e16).contains(&abs) {
        return format!("{f:e}");
    }
    match whole(f) {
        Some(n) => n.to_string(),
        None => f.to_string(),
    }
}

/// A size in bytes as people write it *(i.e. `1.5 KB`, `12 bytes`)*, for `.formatBytes()`.
pub fn format_bytes(f: f64) -> String {
    const UNITS: [&str; 6] = ["bytes", "KB", "MB", "GB", "TB", "PB"];
    let mut size = f;
    let mut unit = 0;
    while size.abs() >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} bytes", format_number(size))
    } else {
        format!("{} {}", format_number((size * 10.0).round() / 10.0), UNITS[unit])
    }
}

/// How `outln`, `toString()` and `f"{...}"` show a value.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{}", format_number(*n)),
            Value::Str(s) => write!(f, "{s}"),
            // Bytes aren't text, so they show how many there are *(i.e. `<bytes 1.2 KB>`)*
            Value::Bytes(b) => write!(f, "<bytes {}>", format_bytes(b.len() as f64)),
            Value::Type(t) => write!(f, "<type {}>", t.0),
            Value::List(items) => {
                // A list inside itself is written `[...]`, like in Python
                let Some(_printing) = Visiting::enter(&PRINTING, (Rc::as_ptr(items) as usize, 0)) else { return write!(f, "[...]") };
                write!(f, "[")?;
                for (i, item) in items.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    item.write_nested(f)?;
                }
                write!(f, "]")
            }
            Value::Map(entries) => {
                let Some(_printing) = Visiting::enter(&PRINTING, (Rc::as_ptr(entries) as usize, 0)) else { return write!(f, "{{...}}") };
                let entries = entries.borrow();
                if entries.is_empty() {
                    return write!(f, "{{}}");
                }
                write!(f, "{{")?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, " ")?;
                    key.to_value().write_nested(f)?;
                    write!(f, " => ")?;
                    value.write_nested(f)?;
                }
                write!(f, " }}")
            }
            Value::Range(r) => {
                let op = if r.inclusive { "..=" } else { ".." };
                write!(f, "{}{op}{}", r.start, r.end)?;
                if r.step != 1 {
                    write!(f, " step {}", r.step)?;
                }
                Ok(())
            }
            Value::Function(func) => write!(f, "<function {}>", func.name()),
            Value::Exception(e) => write!(f, "{}: {}", e.kind, e.message),
            Value::Command(c) => write!(f, "{}", c.borrow().display()),
            Value::Result(r) => match &**r {
                ResultValue::Ok(v) => {
                    write!(f, "ok(")?;
                    v.write_nested(f)?;
                    write!(f, ")")
                }
                ResultValue::Error(e) => write!(f, "error({e})"),
            },
            Value::Regex(r) => write!(f, "%r\"{}\"{}", r.pattern.replace('"', "\\\""), r.flags),
            Value::Class(c) => write!(f, "<{} {}>", if c.interface { "interface" } else { "class" }, c.name),
            Value::Handle(_) => write!(f, "<native>"),
            Value::Object(o) if o.enum_name.is_some() => write!(f, "{}::{}", o.class.name, o.enum_name.as_ref().unwrap()),
            Value::Object(o) => {
                let Some(_printing) = Visiting::enter(&PRINTING, (Rc::as_ptr(o) as usize, 0)) else { return write!(f, "{}(...)", o.class.name) };
                write!(f, "{}(", o.class.name)?;
                // Rust data of the standard library isn't shown *(i.e. the page of an `html::Element`)*
                let fields = o.fields.borrow();
                let shown = o.class.fields.iter().zip(fields.iter()).filter(|(_, v)| !matches!(v, Value::Handle(_)));
                for (i, (slot, value)) in shown.enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: ", slot.name)?;
                    value.write_nested(f)?;
                }
                write!(f, ")")
            }
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_nested(f)
    }
}
