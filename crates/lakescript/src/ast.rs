//! The syntax tree. `Display` prints it as S-expressions, which the tests and
//! `lks --ast` use: `1 + 2 * 3` is `(+ 1 (* 2 3))`.

use std::fmt;

use crate::token::Span;

// ---- Types ----

#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    /// `string`, `list<string>`, `map<string, number>`, a type alias...
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
    Function(FunctionDecl),
    /// `type Name = ...;`
    TypeAlias { name: String, ty: TypeExpr },
    If { cond: Expr, then: Box<Stmt>, otherwise: Option<Box<Stmt>> },
    While { cond: Expr, body: Box<Stmt> },
    For { pattern: ForPattern, iterable: Expr, body: Box<Stmt> },
    Break,
    Continue,
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub body: FunctionBody,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionBody {
    Block(Vec<Stmt>),
    /// `= expression;`
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub variadic: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForPattern {
    /// `for ($x in ...)`
    Value(String),
    /// `for ($i, $x in ...)`
    Indexed { index: String, value: String },
    /// `for ($k => $v in ...)`
    KeyValue { key: String, value: String },
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
        }
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dots = if self.variadic { "..." } else { "" };
        let ty = self.ty.as_ref().map(|t| format!("{t} ")).unwrap_or_default();
        let default = self.default.as_ref().map(|d| format!(" = {d}")).unwrap_or_default();
        write!(f, "({dots}{ty}${}{default})", self.name)
    }
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
                let body = match &decl.body {
                    FunctionBody::Expr(e) => format!("= {e}"),
                    FunctionBody::Block(stmts) => block(stmts),
                };
                write!(f, "(fn {} {}{generics} ({params}) {body})", decl.ret, decl.name)
            }
            TypeAlias { name, ty } => write!(f, "(type {name} {ty})"),
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
