//! The syntax tree. `Display` prints it as S-expressions, which the tests and
//! `lks --ast` use: `1 + 2 * 3` is `(+ 1 (* 2 3))`.

use std::fmt;
use std::rc::Rc;

use crate::token::Span;

// ---- Types ----

#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    /// `string`, `list<string>`, `map<string, number>`, a class...
    Named { name: String, args: Vec<TypeExpr> },
    /// `?string`
    Nullable(Box<TypeExpr>),
    /// `(string, ...number) -> bool`. `variadic` means the last parameter is `...T`.
    Function { params: Vec<TypeExpr>, variadic: bool, ret: Box<TypeExpr> },
}

// ---- Expressions ----

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    /// `f"Hello {$name}"`
    Interpolated(Vec<StringPart>),
    Regex { pattern: String, flags: String },
    /// `%x"rm {$file}"`
    Command(Vec<StringPart>),
    Symbol(String),
    Bool(bool),
    Null,
    /// `[1, 2]` or `<string>[]`
    List { elem_type: Option<TypeExpr>, items: Vec<Expr> },
    /// `{'a' => 1}` or `<string, number>{}`
    Map { types: Option<(TypeExpr, TypeExpr)>, entries: Vec<(Expr, Expr)> },
    Variable(String),
    /// A function name without a call: the function itself.
    Function(String),
    Unary { op: UnaryOp, operand: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    /// `-f $path`
    PathTest { op: PathTestOp, path: Box<Expr> },
    /// `1..5`, `1..=5`, or `-3..` inside an index.
    Range { start: Box<Expr>, end: Option<Box<Expr>>, inclusive: bool },
    Ternary { cond: Box<Expr>, then: Box<Expr>, otherwise: Box<Expr> },
    /// `=` when `op` is `None`, `+=` when it's `Some(Add)`...
    Assign { op: Option<BinaryOp>, target: Box<Expr>, value: Box<Expr> },
    /// `++$x`, `$x++`, `--$x`, `$x--`
    Increment { target: Box<Expr>, delta: i8, prefix: bool },
    Call { callee: Box<Expr>, args: Vec<Arg> },
    MethodCall { receiver: Box<Expr>, method: String, args: Vec<Arg>, nullable: bool },
    /// `.as(type)` and `.is(type)`
    TypeCheck { receiver: Box<Expr>, ty: TypeExpr, check: TypeCheckKind, nullable: bool },
    Index { target: Box<Expr>, index: Box<Expr>, nullable: bool },
    /// `($a, $b) { ... }` or `($a) -> $a * 2`
    Lambda(Rc<FunctionDecl>),
    /// `if (...) a else b` used as a value
    If { cond: Box<Expr>, then: Box<Branch>, otherwise: Box<Branch> },
    /// `switch` used as a value. It always has a `default` or covers both bools.
    Switch(Box<Switch>),
    /// `try expr`: a `result` instead of an exception.
    Try(Box<Expr>),
    /// `throw expr` used as a value *(i.e. `$x ?? throw ValueError('...')`)*.
    /// It never gives a value, so it fits wherever a value of any type goes.
    Throw(Box<Expr>),
    /// `paths::load`: a function of an imported module.
    ModuleFunction { module: String, name: String },
    /// `paths::$defaultPath`: a constant of an imported module.
    ModuleConstant { module: String, name: String },
    /// `Stack<string>`, called to create an object with these types.
    TypeArgs { name: String, args: Vec<TypeExpr> },
    /// `@`: the object a method runs on.
    This,
    /// `@name` or `$point.x`: a field of an object.
    Field { target: Box<Expr>, name: String },
    /// `Point::origin` or `Self::$count`: a static member of a class. `class`
    /// is `Self` inside a class.
    Static { class: String, name: String, variable: bool },
    /// `@super(...)`, in a constructor: the superclass's constructor.
    SuperNew(Vec<Arg>),
    /// `@super.sayHello()`: the superclass's version of a method.
    SuperMethod { method: String, args: Vec<Arg> },
}

/// The value of an `if` or `switch` branch: an expression, or a block that
/// gives its value with `yield`.
#[derive(Debug, Clone, PartialEq)]
pub enum Branch {
    Expr(Expr),
    Block(Vec<Stmt>, Span),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Switch {
    pub subject: Expr,
    pub cases: Vec<Case>,
    pub default: Option<CaseBody>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    pub patterns: Vec<Pattern>,
    pub body: CaseBody,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// Compared with `==`, or with `-in` when it's a range.
    Value(Expr),
    /// Matches like `.is()`.
    Type(TypeExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseBody {
    /// A value, in a `switch` used as an expression.
    Value(Branch),
    /// A statement or block, in a `switch` statement.
    Stmt(Box<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Text(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
    /// `...$list`
    pub spread: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeCheckKind {
    As,
    Is,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Plus,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
    Coalesce,
    Like,
    Match,
    In,
    NotLike,
    NotMatch,
    NotIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathTestOp {
    File,
    Dir,
    Exec,
    Read,
    Write,
}

// ---- Statements ----

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `let $x = ...`, `const $x = ...`, `string $x = ...`, `const list<any> $x = ...`
    Let { constant: bool, name: String, ty: Option<TypeExpr>, value: Expr },
    /// `let [$first, ...$rest] = ...`
    LetList { constant: bool, names: Vec<String>, rest: Option<String>, value: Expr },
    Function(FunctionDecl),
    /// `class`, `interface` or `enum`.
    Class(Rc<ClassDecl>),
    If { cond: Expr, then: Box<Stmt>, otherwise: Option<Box<Stmt>> },
    While { cond: Expr, body: Box<Stmt> },
    For { pattern: ForPattern, iterable: Expr, body: Box<Stmt> },
    Break,
    Continue,
    Return(Option<Expr>),
    /// Gives the value of the block of an `if` or `switch` expression.
    Yield(Expr),
    Switch(Box<Switch>),
    Try { body: Vec<Stmt>, catches: Vec<Catch>, finally: Option<Vec<Stmt>> },
    Throw(Expr),
    /// `assert $cond;`, with the code of the condition, for its message.
    Assert { cond: Expr, text: String },
    /// `use a/b/c;`, `use a/b/c::name;`, `use a/b/c::{x, $y};`
    Use(UseDecl),
    /// `pub` before a function, a `const`, a class, an interface or an enum.
    Pub(Box<Stmt>),
    Block(Vec<Stmt>),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    /// `["std", "fs"]` for `use std/fs;`
    pub path: Vec<String>,
    pub items: UseItems,
    /// `use people/ChildClass;`: the last part is a type, and its module is that
    /// type *(see `ideas/language/classes.md`)*. `items` imports it.
    pub type_module: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UseItems {
    /// The whole module, reached with `name::`. The name is the alias, or the last part of the path.
    Module { alias: Option<String> },
    /// Some of its functions and constants, used without a prefix.
    Items(Vec<UseItem>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseItem {
    pub name: String,
    /// `true` for a constant (`$name`), `false` for a function.
    pub variable: bool,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Catch {
    /// `None` for `catch ($e)`, which catches everything.
    pub ty: Option<TypeExpr>,
    pub name: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    /// `None` for anonymous functions, whose return type is inferred.
    pub ret: Option<TypeExpr>,
    pub body: FunctionBody,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionBody {
    Block(Vec<Stmt>),
    /// `= expression;`
    Expr(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Enum,
    /// The arguments of a program *(see `ideas/language/cli.md`)*, or one of its subcommands.
    Command,
}

impl ClassKind {
    pub fn word(self) -> &'static str {
        match self {
            ClassKind::Class => "class",
            ClassKind::Interface => "interface",
            ClassKind::Enum => "enum",
            ClassKind::Command => "command",
        }
    }
}

/// A class, an interface or an enum *(see `ideas/language/classes.md`)*.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub kind: ClassKind,
    pub name: String,
    pub generics: Vec<String>,
    /// The superclass of a class, or the interfaces an interface extends.
    pub extends: Vec<TypeExpr>,
    /// The interfaces of its `implement` blocks, in order.
    pub implements: Vec<TypeExpr>,
    pub members: Vec<Member>,
    /// The values of an enum.
    pub values: Vec<EnumValue>,
    /// The subcommands of a command *(i.e. `Git::Commit`)*. Their names are the full ones.
    pub subcommands: Vec<ClassDecl>,
    /// Whether the command can be used without a subcommand.
    pub optional_subcommand: bool,
    /// Its `///` comment.
    pub doc: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub public: bool,
    pub is_static: bool,
    /// Where the member comes from: the class itself, an `override { }`
    /// block, or an `implement Interface { }` block.
    pub origin: Origin,
    pub kind: MemberKind,
    /// Its `///` comment.
    pub doc: Option<String>,
    pub span: Span,
}

/// `flag`, `flag(v)` or `flag(v, count)` before a field of a command.
#[derive(Debug, Clone, PartialEq)]
pub struct Flag {
    pub short: Option<char>,
    pub count: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Origin {
    Own,
    Override,
    Implement(TypeExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemberKind {
    /// `pub string @name;`, `const number @id = 1;`, or with `$` a static field.
    Field { constant: bool, ty: TypeExpr, name: String, value: Option<Expr>, flag: Option<Flag>, variadic: bool },
    /// A method. In an interface, `body` is `None` for the ones without a default.
    Method { decl: FunctionDecl, body: bool },
    /// `new(...) { ... }`. A parameter written as a field *(i.e. `@name`)* has `field` set.
    Constructor(FunctionDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    pub name: String,
    pub args: Vec<Arg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub variadic: bool,
    /// `new(@name)`: stored in the field `name`, whose type it takes.
    pub field: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForPattern {
    /// `for ($x in ...)`
    Value(String),
    /// `for ($i, $x in ...)`
    Indexed { index: String, value: String },
    /// `for ($k => $v in ...)`
    KeyValue { key: String, value: String },
    /// `for ([$name, $age] in ...)`
    List { names: Vec<String>, rest: Option<String> },
}

// ---- Printing ----

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        use BinaryOp::*;
        match self {
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
            Pow => "**",
            BitAnd => "&",
            BitOr => "|",
            BitXor => "^",
            Shl => "<<",
            Shr => ">>",
            UShr => ">>>",
            Eq => "==",
            NotEq => "!=",
            Lt => "<",
            Gt => ">",
            LtEq => "<=",
            GtEq => ">=",
            And => "&&",
            Or => "||",
            Coalesce => "??",
            Like => "-like",
            Match => "-match",
            In => "-in",
            NotLike => "-notlike",
            NotMatch => "-notmatch",
            NotIn => "-notin",
        }
    }
}

impl UnaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Plus => "+",
            UnaryOp::Not => "!",
            UnaryOp::BitNot => "~",
        }
    }
}

impl PathTestOp {
    pub fn symbol(self) -> &'static str {
        match self {
            PathTestOp::File => "-f",
            PathTestOp::Dir => "-d",
            PathTestOp::Exec => "-x",
            PathTestOp::Read => "-r",
            PathTestOp::Write => "-w",
        }
    }
}

fn join<T: fmt::Display>(items: &[T], sep: &str) -> String {
    items.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(sep)
}

impl fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TypeKind::Named { name, args } if args.is_empty() => write!(f, "{name}"),
            TypeKind::Named { name, args } => write!(f, "{name}<{}>", join(args, ", ")),
            TypeKind::Nullable(inner) => match inner.kind {
                TypeKind::Function { .. } => write!(f, "?({inner})"),
                _ => write!(f, "?{inner}"),
            },
            TypeKind::Function { params, variadic, ret } => {
                let mut list: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                if *variadic {
                    if let Some(last) = list.last_mut() {
                        *last = format!("...{last}");
                    }
                }
                write!(f, "({}) -> {ret}", list.join(", "))
            }
        }
    }
}

impl fmt::Display for Arg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.name, self.spread) {
            (Some(name), _) => write!(f, "({name}: {})", self.value),
            (None, true) => write!(f, "(... {})", self.value),
            (None, false) => write!(f, "{}", self.value),
        }
    }
}

impl fmt::Display for StringPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StringPart::Text(text) => write!(f, "{text:?}"),
            StringPart::Expr(expr) => write!(f, "{expr}"),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ExprKind::*;
        let args = |args: &[Arg]| -> String { args.iter().map(|a| format!(" {a}")).collect() };
        match &self.kind {
            Int(n) => write!(f, "{n}"),
            Float(n) => write!(f, "{n:?}"),
            Str(s) => write!(f, "{s:?}"),
            Interpolated(parts) => write!(f, "(f {})", join(parts, " ")),
            Regex { pattern, flags } => write!(f, "(regex {pattern:?} {flags:?})"),
            Command(parts) => write!(f, "(x {})", join(parts, " ")),
            Symbol(name) => write!(f, ":{name}"),
            Bool(b) => write!(f, "{b}"),
            Null => write!(f, "null"),
            List { elem_type, items } => {
                let ty = elem_type.as_ref().map(|t| format!("<{t}>")).unwrap_or_default();
                let items: String = items.iter().map(|i| format!(" {i}")).collect();
                write!(f, "(list{ty}{items})")
            }
            Map { types, entries } => {
                let ty = types.as_ref().map(|(k, v)| format!("<{k}, {v}>")).unwrap_or_default();
                let entries: String = entries.iter().map(|(k, v)| format!(" ({k} {v})")).collect();
                write!(f, "(map{ty}{entries})")
            }
            Variable(name) => write!(f, "${name}"),
            Function(name) => write!(f, "{name}"),
            Unary { op, operand } => write!(f, "({} {operand})", op.symbol()),
            Binary { op, left, right } => write!(f, "({} {left} {right})", op.symbol()),
            PathTest { op, path } => write!(f, "({} {path})", op.symbol()),
            Range { start, end, inclusive } => {
                let op = if *inclusive { "..=" } else { ".." };
                match end {
                    Some(end) => write!(f, "({op} {start} {end})"),
                    None => write!(f, "({op} {start} _)"),
                }
            }
            Ternary { cond, then, otherwise } => write!(f, "(? {cond} {then} {otherwise})"),
            Assign { op, target, value } => {
                let op = op.map(|o| format!("{}=", o.symbol())).unwrap_or_else(|| "=".into());
                write!(f, "({op} {target} {value})")
            }
            Increment { target, delta, prefix } => {
                let op = if *delta > 0 { "++" } else { "--" };
                if *prefix { write!(f, "({op}pre {target})") } else { write!(f, "({op}post {target})") }
            }
            Call { callee, args: a } => write!(f, "(call {callee}{})", args(a)),
            MethodCall { receiver, method, args: a, nullable } => {
                let dot = if *nullable { "?." } else { "." };
                write!(f, "({dot}{method} {receiver}{})", args(a))
            }
            TypeCheck { receiver, ty, check, nullable } => {
                let dot = if *nullable { "?." } else { "." };
                let name = if *check == TypeCheckKind::As { "as" } else { "is" };
                write!(f, "({dot}{name} {receiver} {ty})")
            }
            Index { target, index, nullable } => {
                let op = if *nullable { "?[]" } else { "[]" };
                write!(f, "({op} {target} {index})")
            }
            Lambda(decl) => write!(f, "(lambda ({}) {})", join(&decl.params, " "), body(&decl.body)),
            If { cond, then, otherwise } => write!(f, "(if {cond} {then} {otherwise})"),
            Switch(switch) => write!(f, "{switch}"),
            Try(inner) => write!(f, "(try {inner})"),
            Throw(inner) => write!(f, "(throw {inner})"),
            ModuleFunction { module, name } => write!(f, "{module}::{name}"),
            ModuleConstant { module, name } => write!(f, "{module}::${name}"),
            TypeArgs { name, args } => write!(f, "{name}<{}>", join(args, ", ")),
            This => write!(f, "@"),
            Field { target, name } => match &target.kind {
                This => write!(f, "@{name}"),
                _ => write!(f, "(. {target} {name})"),
            },
            Static { class, name, variable } => write!(f, "{class}::{}{name}", if *variable { "$" } else { "" }),
            SuperNew(a) => write!(f, "(@super{})", args(a)),
            SuperMethod { method, args: a } => write!(f, "(@super.{method}{})", args(a)),
        }
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dots = if self.variadic { "..." } else { "" };
        let ty = self.ty.as_ref().map(|t| format!("{t} ")).unwrap_or_default();
        let default = self.default.as_ref().map(|d| format!(" = {d}")).unwrap_or_default();
        let sigil = if self.field { "@" } else { "$" };
        write!(f, "({dots}{ty}{sigil}{}{default})", self.name)
    }
}

impl fmt::Display for Branch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Branch::Expr(e) => write!(f, "{e}"),
            Branch::Block(stmts, _) => write!(f, "{}", block(stmts)),
        }
    }
}

impl fmt::Display for CaseBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaseBody::Value(b) => write!(f, "{b}"),
            CaseBody::Stmt(s) => write!(f, "{s}"),
        }
    }
}

impl fmt::Display for Switch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(switch {}", self.subject)?;
        for case in &self.cases {
            let patterns: Vec<String> = case
                .patterns
                .iter()
                .map(|p| match p {
                    Pattern::Value(e) => e.to_string(),
                    Pattern::Type(t) => format!("<{t}>"),
                })
                .collect();
            write!(f, " (case {} -> {})", patterns.join(", "), case.body)?;
        }
        if let Some(default) = &self.default {
            write!(f, " (default -> {default})")?;
        }
        write!(f, ")")
    }
}

fn body(body: &FunctionBody) -> String {
    match body {
        FunctionBody::Expr(e) => format!("= {e}"),
        FunctionBody::Block(stmts) => block(stmts),
    }
}

fn list_pattern(names: &[String], rest: &Option<String>) -> String {
    let mut parts: Vec<String> = names.iter().map(|n| format!("${n}")).collect();
    if let Some(rest) = rest {
        parts.push(format!("...${rest}"));
    }
    format!("[{}]", parts.join(", "))
}

fn block(stmts: &[Stmt]) -> String {
    if stmts.is_empty() {
        return "{}".into();
    }
    format!("{{ {} }}", join(stmts, " "))
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use StmtKind::*;
        match &self.kind {
            Let { constant, name, ty, value } => {
                let kw = if *constant { "const" } else { "let" };
                let ty = ty.as_ref().map(|t| format!(" {t}")).unwrap_or_default();
                write!(f, "({kw}{ty} ${name} {value})")
            }
            Function(decl) => {
                let generics = if decl.generics.is_empty() { String::new() } else { format!("<{}>", decl.generics.join(", ")) };
                let params = join(&decl.params, " ");
                let ret = decl.ret.as_ref().map(|r| r.to_string()).unwrap_or_default();
                write!(f, "(fn {ret} {}{generics} ({params}) {})", decl.name, body(&decl.body))
            }
            LetList { constant, names, rest, value } => {
                let kw = if *constant { "const" } else { "let" };
                write!(f, "({kw} {} {value})", list_pattern(names, rest))
            }
            Yield(e) => write!(f, "(yield {e})"),
            Throw(e) => write!(f, "(throw {e})"),
            Assert { cond, .. } => write!(f, "(assert {cond})"),
            Pub(inner) => write!(f, "(pub {inner})"),
            Class(decl) => write!(f, "{decl}"),
            Use(decl) => {
                write!(f, "(use {}", decl.path.join("/"))?;
                match &decl.items {
                    UseItems::Module { alias: Some(a) } => write!(f, " as {a}")?,
                    UseItems::Module { alias: None } => {}
                    UseItems::Items(items) => {
                        let items: Vec<String> = items
                            .iter()
                            .map(|i| {
                                let name = if i.variable { format!("${}", i.name) } else { i.name.clone() };
                                match &i.alias {
                                    Some(a) if i.variable => format!("{name} as ${a}"),
                                    Some(a) => format!("{name} as {a}"),
                                    None => name,
                                }
                            })
                            .collect();
                        write!(f, " ::{{{}}}", items.join(", "))?;
                    }
                }
                write!(f, ")")
            }
            Try { body, catches, finally } => {
                write!(f, "(try {}", block(body))?;
                for c in catches {
                    let ty = c.ty.as_ref().map(|t| format!("{t} ")).unwrap_or_default();
                    write!(f, " (catch {ty}${} {})", c.name, block(&c.body))?;
                }
                if let Some(fin) = finally {
                    write!(f, " (finally {})", block(fin))?;
                }
                write!(f, ")")
            }
            Switch(switch) => write!(f, "{switch}"),
            If { cond, then, otherwise } => match otherwise {
                Some(o) => write!(f, "(if {cond} {then} {o})"),
                None => write!(f, "(if {cond} {then})"),
            },
            While { cond, body } => write!(f, "(while {cond} {body})"),
            For { pattern, iterable, body } => {
                let pattern = match pattern {
                    ForPattern::Value(v) => format!("${v}"),
                    ForPattern::Indexed { index, value } => format!("${index}, ${value}"),
                    ForPattern::KeyValue { key, value } => format!("${key} => ${value}"),
                    ForPattern::List { names, rest } => list_pattern(names, rest),
                };
                write!(f, "(for ({pattern}) {iterable} {body})")
            }
            Break => write!(f, "(break)"),
            Continue => write!(f, "(continue)"),
            Return(Some(e)) => write!(f, "(return {e})"),
            Return(None) => write!(f, "(return)"),
            Block(stmts) => write!(f, "{}", block(stmts)),
            Expr(e) => write!(f, "{e}"),
        }
    }
}

impl fmt::Display for ClassDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let generics = if self.generics.is_empty() { String::new() } else { format!("<{}>", self.generics.join(", ")) };
        write!(f, "({} {}{generics}", self.kind.word(), self.name)?;
        if !self.extends.is_empty() {
            write!(f, " (extends {})", join(&self.extends, " "))?;
        }
        if !self.implements.is_empty() {
            write!(f, " (implements {})", join(&self.implements, " "))?;
        }
        for value in &self.values {
            let args: String = value.args.iter().map(|a| format!(" {a}")).collect();
            write!(f, " (value {}{args})", value.name)?;
        }
        for member in &self.members {
            write!(f, " {member}")?;
        }
        for sub in &self.subcommands {
            write!(f, " {sub}")?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for Member {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut words = Vec::new();
        match &self.origin {
            Origin::Own => {}
            Origin::Override => words.push("override".to_string()),
            Origin::Implement(ty) => words.push(format!("implement {ty}")),
        }
        if self.public {
            words.push("pub".into());
        }
        if self.is_static {
            words.push("static".into());
        }
        let prefix: String = words.iter().map(|w| format!("{w} ")).collect();
        match &self.kind {
            MemberKind::Field { constant, ty, name, value, .. } => {
                let kw = if *constant { "const " } else { "" };
                let sigil = if self.is_static { "$" } else { "@" };
                let prefix = prefix.replace("static ", "");
                match value {
                    Some(v) => write!(f, "({prefix}{kw}field {ty} {sigil}{name} {v})"),
                    None => write!(f, "({prefix}{kw}field {ty} {sigil}{name})"),
                }
            }
            MemberKind::Method { decl, body: has_body } => {
                let generics = if decl.generics.is_empty() { String::new() } else { format!("<{}>", decl.generics.join(", ")) };
                let ret = decl.ret.as_ref().map(|r| r.to_string()).unwrap_or_default();
                let params = join(&decl.params, " ");
                if *has_body {
                    write!(f, "({prefix}method {ret} {}{generics} ({params}) {})", decl.name, body(&decl.body))
                } else {
                    write!(f, "({prefix}method {ret} {}{generics} ({params}))", decl.name)
                }
            }
            MemberKind::Constructor(decl) => write!(f, "({prefix}new ({}) {})", join(&decl.params, " "), body(&decl.body)),
        }
    }
}
