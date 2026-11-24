//! The bytecode the compiler writes and the VM runs *(see `ideas/tooling/vm.md`)*.
//! It's private to the library: it can change in any version.
//!
//! The VM is a stack machine. A function's frame is a window of the stack:
//! its local variables come first, in numbered slots, and the values being
//! computed are pushed above them. A local variable that a closure uses lives
//! in a cell instead, so the closure and the function share it.

use std::rc::Rc;

use crate::ast::{BinaryOp, PathTestOp, TypeExpr, TypeKind, UnaryOp};
use crate::token::Span;
use crate::value::Value;

/// One instruction. Numbers in them are indexes: into the frame's slots or
/// cells, the closure's upvalues, the module's globals, or the tables of the
/// `Proto`. Jumps go to an instruction's position in `Proto::code`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    // ---- Values ----
    Const(u32),
    Int(i32),
    Null,
    True,
    False,
    Pop,
    Dup,

    // ---- Variables ----
    LoadLocal(u32),
    /// Pops into a slot.
    StoreLocal(u32),
    LoadCell(u32),
    StoreCell(u32),
    /// Pops into a new cell, so a variable declared again (in the next pass of a
    /// loop) is a new variable, and closures made before keep the old one.
    DeclareCell(u32),
    /// Moves a parameter from its slot to a new cell.
    SlotToCell { slot: u32, cell: u32 },
    LoadUpvalue(u32),
    StoreUpvalue(u32),
    LoadGlobal(u32),
    /// Like `LoadGlobal`, but `null` when it isn't declared, for the classes of a type.
    LoadGlobalOrNull(u32),
    /// Assigns an existing global variable.
    StoreGlobal(u32),
    /// Pops into a global, which must not exist yet.
    DefineGlobal { global: u32, constant: bool },
    LoadBuiltin(u32),
    /// Saves the height of the stack into a slot, for `RestoreSp`.
    SaveSp(u32),
    /// Drops what was pushed since `SaveSp`.
    RestoreSp(u32),

    // ---- Operators ----
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    /// Any other binary operator except `&&`, `||` and `??`.
    Binary(BinaryOp),
    Unary(UnaryOp),
    /// `-like`, `-notlike`, `-match`, `-notmatch`
    Pattern(BinaryOp),
    PathTest(PathTestOp),
    /// Checks the value on top is a number, for `++` and `--`.
    CheckNumber,
    /// Checks the value on top is a bool, for the right side of `&&` (`false`) or `||` (`true`).
    CheckBool(bool),

    // ---- Jumps ----
    Jump(u32),
    /// Pops a condition, which must be a bool. `keyword` says which one it was
    /// for the error: `0` is `if`, `1` is `while`, `2` is `? :`.
    JumpIfFalse { target: u32, keyword: u8 },
    /// Compares the two values on top with `==`, `!=`, `<`, `>`, `<=` or `>=`,
    /// and jumps when it's false. It's the condition of an `if`, `while` or
    /// `? :` in one instruction.
    JumpIfNot { op: BinaryOp, target: u32 },
    /// Pops a bool the VM made itself.
    JumpIfTrueRaw(u32),
    JumpIfFalseRaw(u32),
    /// The left side of `&&` (`and: true`) or `||`: jumps keeping it when it
    /// decides the result, or pops it.
    JumpLogic { target: u32, and: bool },
    /// Jumps keeping the value when it isn't `null`, for `??`.
    JumpIfNotNull(u32),
    /// Jumps keeping the value when it's `null`, for `?.` and `?[`.
    JumpIfNull(u32),

    // ---- Collections and strings ----
    /// Checks that the value on top can be a map key.
    CheckKey,
    List(u32),
    /// Pops pairs of key and value.
    Map(u32),
    /// Joins the values into a string.
    Interpolate(u32),
    /// Turns the value on top into the arguments it gives `%x"..."`: a list of strings.
    QuoteArgument,
    /// Splits the parts into a program and its arguments, for `%x"..."`.
    Command(u32),
    Regex(u32),
    /// A type as a value: the number of a type of the proto.
    TypeValue(u32),
    /// Checks the value on top is a whole number, for the ends of a range.
    RangeBound,
    Range { inclusive: bool },
    Index,
    /// `$text[2..]`: pops the start.
    SliceFrom,
    /// Pops the value, the index and the container, and pushes the value back.
    SetIndex,
    /// Changes `container[index]` with `op` and the value on top. `mode`: `0`
    /// for `op=`, `1` for `++$x` and `--$x`, `2` for `$x++` and `$x--`.
    UpdateIndex { op: BinaryOp, mode: u8 },
    /// `let [$a, ...$rest] = ...`: checks the list has `names` elements (or
    /// more, with a rest) and pushes them, and then the rest as a list.
    Destructure { names: u32, rest: bool },

    // ---- Types ----
    /// `.is(type)`. `catch`: the type of a `catch`, whose errors are `TypeError`s.
    /// `classes`: how many classes are on top, the ones the type names *(see `class_names`)*.
    Is { ty: u32, catch: bool, classes: u8 },
    As { ty: u32, classes: u8 },
    /// `$x!`: throws a `NullError` when the value on top is `null`. The
    /// constant is how the value is written *(i.e. `$list[0]`)*, or `null`.
    NonNull(u32),

    // ---- Classes ----
    /// Makes a class, described by `Proto::classes`, from what's on the stack:
    /// the superclass, the interfaces, `init`, `new`, the methods and the static methods.
    Class(u32),
    /// `$object.name`, with the name in `Proto::names`.
    GetField(u32),
    /// Pops the value and the object, and pushes the value back.
    SetField(u32),
    /// `Class::name` or `Class::$name`: pops the class.
    Static { name: u32, variable: bool },
    /// Pops the value and the class, and pushes the value back.
    SetStatic(u32),
    /// Gives a class a static field: pops the value and the class.
    DefineStatic { name: u32, constant: bool },
    /// Gives a command its subcommand *(i.e. `Git::Commit`)*: pops it and the command.
    DefineMember(u32),
    /// `@super(...)`: the class whose superclass's constructor runs, `@` and the arguments are on the stack.
    SuperNew(u32),
    SuperNewComplex(u32),
    /// `@super.name(...)`: the class, `@` and the arguments are on the stack.
    SuperMethod { name: u32, argc: u32 },
    /// Creates a value of an enum: the enum and the arguments are on the stack.
    EnumValue { name: u32, argc: u32 },
    EnumValueComplex { name: u32, call: u32 },

    // ---- Functions ----
    Closure(u32),
    Call(u32),
    /// A call with named or spread arguments, described by `Proto::calls`.
    CallComplex(u32),
    CallMethod { name: u32, argc: u32 },
    CallMethodComplex { name: u32, call: u32 },
    Return,
    /// Jumps when the parameter was given, to skip its default.
    JumpIfGiven { param: u32, target: u32 },

    // ---- Modules ----
    /// Imports a whole module under a name. It's loaded the first time it's used.
    UseModule { module: u32, path: u32 },
    /// Imports some names of a module, described by `Proto::imports`.
    UseItems(u32),
    /// `name::function` or `name::$constant`. `name` is in `Proto::names`.
    ModuleMember { module: u32, name: u32, constant: bool },

    // ---- Exceptions ----
    Throw,
    /// Throws again the exception on top, from where it was first thrown.
    Rethrow,
    /// Starts a `try` whose `catch`es are at `target`.
    PushCatch(u32),
    /// Starts a `try` with a `finally` at `target`. What stopped the body is kept in `slot`.
    PushFinally { target: u32, slot: u32 },
    /// Starts a `try` expression, whose error is handled at `target`.
    PushTry(u32),
    PopHandler,
    /// Ends a `finally` that was run because of an exception or `exit()`, to carry on with it.
    EndFinally(u32),
    /// `try expr`: wraps the value on top in a `result`.
    WrapOk,
    WrapError,
    /// Throws an `Exception` with this message *(in `Proto::names`)*.
    Fail(u32),
    /// Throws when no case of a `switch` used as a value matches the value in the slot.
    NoCase(u32),
    /// Whether the value in the slot matches the pattern on top, for a `case`.
    CaseMatches(u32),

    // ---- Loops ----
    /// Gets a `for` ready: pops what it loops over into `slot` (and the two after it).
    ForPrepare { slot: u32, kind: u8 },
    /// The next pass of a `for`, or a jump to `exit` when it's done. Pushes the
    /// value, or the index or key and then the value.
    ForNext { slot: u32, exit: u32, kind: u8 },
}

/// What a `for` gives each pass.
pub mod for_kind {
    /// `for ($x in ...)`, and `for ([...] in ...)`, which destructures the value.
    pub const VALUE: u8 = 0;
    /// `for ($i, $x in ...)`
    pub const INDEXED: u8 = 1;
    /// `for ($k => $v in ...)`
    pub const KEY_VALUE: u8 = 2;
    /// `for ([$a, $b] in ...)`
    pub const LIST: u8 = 3;
}

/// Where a closure finds a variable of the function around it.
#[derive(Debug, Clone, Copy)]
pub enum Capture {
    /// A cell of the frame that creates the closure.
    Cell(u32),
    /// An upvalue of the function that creates the closure.
    Upvalue(u32),
}

/// How the arguments of a complex call are laid out on the stack, in order.
#[derive(Debug, Clone)]
pub enum ArgKind {
    Positional,
    /// `...$list`
    Spread(Span),
    /// `name: value`
    Named(Rc<str>, Span),
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: Rc<str>,
    pub has_default: bool,
    pub variadic: bool,
}

/// What `Op::Class` needs to make a class.
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: Rc<str>,
    pub interface: bool,
    pub is_enum: bool,
    pub parent: bool,
    /// How many interfaces it implements (or extends, for an interface), pushed after the superclass.
    pub interfaces: u32,
    /// For an interface: its methods without a default.
    pub required: Vec<Rc<str>>,
    pub init: bool,
    pub constructor: bool,
    /// The fields declared in the class itself: name, `const`, nullable, has a value.
    pub fields: Vec<(Rc<str>, bool, bool, bool)>,
    /// The type of each field, for `json::parse($text, Class)`.
    pub field_types: Vec<Option<Rc<TypeExpr>>>,
    pub methods: Vec<Rc<str>>,
    pub static_methods: Vec<Rc<str>>,
    /// For a command: its arguments. The enums of its fields are pushed after the interfaces.
    pub command: Option<Rc<CommandInfo>>,
}

/// What `Git::parse()` and `Git::help()` need *(see `ideas/language/cli.md`)*.
#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub doc: Option<String>,
    pub args: Vec<ArgInfo>,
    /// The name of each subcommand in its command *(i.e. `Commit`)*, and its `///` comment.
    pub subcommands: Vec<(Rc<str>, Option<String>)>,
    pub optional_subcommand: bool,
    /// How many enums the fields use, pushed in order.
    pub enums: u32,
}

#[derive(Debug, Clone)]
pub struct ArgInfo {
    pub name: Rc<str>,
    pub doc: Option<String>,
    /// Given by name *(i.e. `--keep 3`)*, instead of by position.
    pub flag: bool,
    pub short: Option<char>,
    pub count: bool,
    pub variadic: bool,
    pub kind: ArgType,
    pub nullable: bool,
    pub has_value: bool,
    /// The default, as it's written, for the help.
    pub default: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArgType {
    Str,
    Number,
    Bool,
    /// One of the values of the enum with this number *(see `CommandInfo::enums`)*.
    Enum(u32),
}

/// The names of the classes a type uses, in order and without repeating them
/// *(i.e. `Point` and `Line` in `map<string, ?Point> | list<Line>`)*. The
/// compiler pushes their values before `Is` and `As`.
pub fn class_names(ty: &TypeExpr) -> Vec<&str> {
    fn walk<'a>(ty: &'a TypeExpr, out: &mut Vec<&'a str>) {
        match &ty.kind {
            TypeKind::Named { name, args } => {
                if crate::types::is_class_name(name) && !out.contains(&name.as_str()) {
                    out.push(name);
                }
                for a in args {
                    walk(a, out);
                }
            }
            TypeKind::Nullable(inner) => walk(inner, out),
            TypeKind::Function { .. } => {}
        }
    }
    let mut out = Vec::new();
    walk(ty, &mut out);
    out
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: Vec<String>,
    /// Name in the module, whether it's a constant, and the global it goes into.
    pub items: Vec<(String, bool, u32, String)>,
}

/// What a place in the code that reads a field or calls a method found out
/// the last time: for objects of this class, it's the field or the method
/// with this number. The next time, an object of the same class skips the
/// search by name. A class never changes once it's made, so what was found
/// stays right.
#[derive(Default)]
pub struct InlineCache {
    /// The class, by its address. `0` before anything was found.
    class: std::cell::Cell<usize>,
    slot: std::cell::Cell<u32>,
    /// Keeps the address of the class from being used by another one.
    keep: std::cell::RefCell<Option<std::rc::Weak<crate::value::Class>>>,
}

impl InlineCache {
    /// The number found for this class, if it's the one found last.
    #[inline(always)]
    pub fn get(&self, class: &Rc<crate::value::Class>) -> Option<usize> {
        (self.class.get() == Rc::as_ptr(class) as usize).then(|| self.slot.get() as usize)
    }

    pub fn remember(&self, class: &Rc<crate::value::Class>, slot: usize) {
        self.class.set(Rc::as_ptr(class) as usize);
        self.slot.set(slot as u32);
        *self.keep.borrow_mut() = Some(Rc::downgrade(class));
    }
}

impl std::fmt::Debug for InlineCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InlineCache")
    }
}

/// A regex of the code, compiled the first time it runs.
#[derive(Default)]
pub struct RegexCache(pub std::cell::OnceCell<Rc<crate::value::RegexValue>>);

impl std::fmt::Debug for RegexCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RegexCache")
    }
}

/// A compiled function, or a whole script or module.
#[derive(Debug, Default)]
pub struct Proto {
    pub name: Rc<str>,
    /// How tracebacks name it *(i.e. `report::build`, `a lambda in build`, `the script`)*.
    pub shown: Rc<str>,
    pub params: Vec<Param>,
    pub code: Vec<Op>,
    /// The place in the source of each instruction, for errors.
    pub spans: Vec<Span>,
    pub consts: Vec<Value>,
    pub names: Vec<Rc<str>>,
    pub types: Vec<TypeExpr>,
    pub regexes: Vec<(String, String)>,
    /// The compiled regex of each of `regexes`.
    pub regex_cache: Vec<RegexCache>,
    /// What the places that read or set a field, and the ones that call a
    /// method, found out, for each of `names`.
    pub field_cache: Vec<InlineCache>,
    pub method_cache: Vec<InlineCache>,
    pub protos: Vec<Rc<Proto>>,
    pub calls: Vec<Vec<ArgKind>>,
    pub imports: Vec<Import>,
    pub classes: Vec<ClassInfo>,
    pub captures: Vec<Capture>,
    /// How many slots and cells a frame of this function needs.
    pub slots: u32,
    pub cells: u32,
    /// Whether a call with exactly one value per parameter can skip binding.
    pub simple: bool,
}


impl Proto {
    /// The instructions as text, with the functions inside after them, for `lks --bytecode`.
    pub fn listing(&self) -> String {
        let mut out = String::new();
        self.write_listing(&mut out, &self.name);
        out
    }

    fn write_listing(&self, out: &mut String, path: &str) {
        use std::fmt::Write;
        let params: Vec<&str> = self.params.iter().map(|p| &*p.name).collect();
        let _ = writeln!(out, "{path}({}): {} slots, {} cells", params.join(", "), self.slots, self.cells);
        for (i, op) in self.code.iter().enumerate() {
            let span = self.spans[i];
            let detail = match op {
                Op::Const(c) => format!("  {:?}", self.consts[*c as usize]),
                Op::CallMethod { name, .. }
                | Op::CallMethodComplex { name, .. }
                | Op::ModuleMember { name, .. }
                | Op::Fail(name)
                | Op::GetField(name)
                | Op::SetField(name)
                | Op::Static { name, .. }
                | Op::SetStatic(name)
                | Op::DefineStatic { name, .. }
                | Op::DefineMember(name)
                | Op::SuperMethod { name, .. }
                | Op::EnumValue { name, .. }
                | Op::EnumValueComplex { name, .. } => {
                    format!("  {}", self.names[*name as usize])
                }
                _ => String::new(),
            };
            let _ = writeln!(out, "  {i:4}  {:>4}:{:<3}  {op:?}{detail}", span.line, span.column);
        }
        for (i, inner) in self.protos.iter().enumerate() {
            let _ = writeln!(out);
            inner.write_listing(out, &format!("{path}/{i}:{}", inner.name));
        }
    }
}
