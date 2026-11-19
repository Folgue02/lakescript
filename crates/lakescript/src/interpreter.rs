//! Runs the syntax tree directly (phase 1 of `ideas/roadmap.md`).

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::methods;
use crate::scope::{self, Env, SetError};
use crate::token::Span;
use crate::types;
use crate::exceptions;
use crate::patterns;
use crate::command::{CommandKind, CommandValue, shell_quote};
use crate::engine::Outcome;
use crate::host::Host;
use crate::modules::{Module, ModuleSlot, ModuleSource};
use crate::value::{ExceptionValue, Function, Key, NativeContext, NativeError, Range, RegexValue, ResultValue, UserFunction, Value};

/// Why the normal flow stopped.
pub enum Flow {
    Break(Span),
    Continue(Span),
    Return(Value, Span),
    /// The value of the block of an `if` or `switch` expression.
    Yield(Value, Span),
    /// An exception was thrown.
    Throw(Rc<ExceptionValue>),
    /// `exit()` was called with this code.
    Exit(i32),
}

const YIELD_OUTSIDE: &str = "`yield` can only be used in the block of an `if` or `switch` used as a value";

pub type Exec<T> = Result<T, Flow>;

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

pub struct Interpreter<'a> {
    /// Output, input, modules, the script's folder and environment...
    pub host: &'a mut Host,
    /// Where the stack was when the script started, and how much of it the
    /// script may use. Each call checks it, so a recursion that never ends
    /// stops with an error instead of crashing the host program.
    stack_base: usize,
    stack_budget: usize,
}

fn stack_position() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

impl<'a> Interpreter<'a> {
    pub fn new(host: &'a mut Host, stack_budget: usize) -> Self {
        Interpreter { host, stack_base: stack_position(), stack_budget }
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

    /// Runs the statements of a whole script.
    pub fn run(&mut self, program: &[Stmt], env: &Env) -> Result<Outcome, Diagnostic> {
        for stmt in program {
            match self.exec(stmt, env) {
                Ok(()) => {}
                Err(Flow::Exit(code)) => return Ok(Outcome::Exit(code)),
                Err(Flow::Throw(exception)) => return Err(uncaught(&exception)),
                Err(Flow::Break(span)) => return Err(Diagnostic::new("`break` outside a loop", span)),
                Err(Flow::Continue(span)) => return Err(Diagnostic::new("`continue` outside a loop", span)),
                Err(Flow::Return(_, span)) => return Err(Diagnostic::new("`return` outside a function", span)),
                Err(Flow::Yield(_, span)) => return Err(Diagnostic::new(YIELD_OUTSIDE, span)),
            }
        }
        Ok(Outcome::Finished)
    }

    // ---- Statements ----

    fn exec(&mut self, stmt: &Stmt, env: &Env) -> Exec<()> {
        match &stmt.kind {
            StmtKind::Let { constant, name, value, .. } => {
                let value = self.eval(value, env)?;
                if !scope::declare(env, name, value, *constant) {
                    return error(format!("`${name}` already exists. Variables can't be declared twice"), stmt.span);
                }
            }
            StmtKind::LetList { constant, names, rest, value } => {
                let value = self.eval(value, env)?;
                self.destructure(env, names, rest, value, *constant, stmt.span)?;
            }
            StmtKind::Yield(value) => {
                let value = self.eval(value, env)?;
                return Err(Flow::Yield(value, stmt.span));
            }
            StmtKind::Switch(switch) => {
                self.switch(switch, env, false)?;
            }
            StmtKind::Use(decl) => self.exec_use(decl, env, stmt.span)?,
            StmtKind::Pub(inner) => self.exec(inner, env)?,
            StmtKind::Throw(value) => {
                let value = self.eval(value, env)?;
                let Value::Exception(exception) = value else {
                    return error_as(
                        exceptions::TYPE_ERROR,
                        format!("only exceptions can be thrown, not a {} *(i.e. `throw ValueError('...')`)*", value.type_name()),
                        stmt.span,
                    );
                };
                // A new exception gets the place where it's thrown; a caught one keeps its own
                let exception = if exception.span == Span::default() {
                    exceptions::new(&exception.kind, exception.message.clone(), stmt.span)
                } else {
                    exception
                };
                return Err(Flow::Throw(exception));
            }
            StmtKind::Try { body, catches, finally } => {
                let mut result = self.exec_block(body, &scope::new_scope(env));
                if let Err(Flow::Throw(exception)) = &result {
                    let exception = exception.clone();
                    for catch in catches {
                        let value = Value::Exception(exception.clone());
                        let matched = match &catch.ty {
                            None => true,
                            Some(ty) => types::matches(&value, ty).map_err(|m| flow_error_as(exceptions::TYPE_ERROR, m, ty.span))?,
                        };
                        if matched {
                            let scope = scope::new_scope(env);
                            if !scope::declare(&scope, &catch.name, value, false) {
                                return error(format!("`${}` already exists. Variables can't be declared twice", catch.name), stmt.span);
                            }
                            result = self.exec_block(&catch.body, &scope);
                            break;
                        }
                    }
                }
                if let Some(finally) = finally {
                    self.exec_block(finally, &scope::new_scope(env))?;
                }
                return result;
            }
            StmtKind::Function(decl) => {
                let function = Function::User(UserFunction { decl: Rc::new(decl.clone()), env: env.clone() });
                if !scope::declare_function(env, &decl.name, Rc::new(function)) {
                    return error(format!("a function named `{}` already exists", decl.name), stmt.span);
                }
            }
            StmtKind::TypeAlias { .. } => {}
            StmtKind::If { cond, then, otherwise } => {
                if self.condition(cond, env, "if")? {
                    self.exec_scoped(then, env)?;
                } else if let Some(otherwise) = otherwise {
                    self.exec_scoped(otherwise, env)?;
                }
            }
            StmtKind::While { cond, body } => {
                while self.condition(cond, env, "while")? {
                    match self.exec_scoped(body, env) {
                        Err(Flow::Break(_)) => break,
                        Ok(()) | Err(Flow::Continue(_)) => {}
                        Err(other) => return Err(other),
                    }
                }
            }
            StmtKind::For { pattern, iterable, body } => self.exec_for(pattern, iterable, body, env)?,
            StmtKind::Break => return Err(Flow::Break(stmt.span)),
            StmtKind::Continue => return Err(Flow::Continue(stmt.span)),
            StmtKind::Return(value) => {
                let value = match value {
                    Some(expr) => self.eval(expr, env)?,
                    None => Value::Null,
                };
                return Err(Flow::Return(value, stmt.span));
            }
            StmtKind::Block(stmts) => self.exec_block(stmts, &scope::new_scope(env))?,
            StmtKind::Expr(expr) => {
                self.eval(expr, env)?;
            }
        }
        Ok(())
    }

    fn exec_block(&mut self, stmts: &[Stmt], env: &Env) -> Exec<()> {
        for stmt in stmts {
            self.exec(stmt, env)?;
        }
        Ok(())
    }

    /// Runs a body in its own scope, so declarations inside it stay inside.
    fn exec_scoped(&mut self, stmt: &Stmt, env: &Env) -> Exec<()> {
        match &stmt.kind {
            StmtKind::Block(stmts) => self.exec_block(stmts, &scope::new_scope(env)),
            _ => self.exec(stmt, &scope::new_scope(env)),
        }
    }

    fn condition(&mut self, cond: &Expr, env: &Env, keyword: &str) -> Exec<bool> {
        match self.eval(cond, env)? {
            Value::Bool(b) => Ok(b),
            other => error_as(
                exceptions::TYPE_ERROR,
                format!("the condition of `{keyword}` must be a bool, but this is a {}", other.type_name()),
                cond.span,
            ),
        }
    }

    fn exec_for(&mut self, pattern: &ForPattern, iterable: &Expr, body: &Stmt, env: &Env) -> Exec<()> {
        let value = self.eval(iterable, env)?;
        if let ForPattern::List { .. } = pattern {
            let Value::List(list) = &value else {
                return error(format!("`for ([...] in ...)` needs a list of lists, not a {}", value.type_name()), iterable.span);
            };
            return self.exec_for_list(list.clone(), pattern, body, env);
        }
        // What each pass gets: (index or key, value)
        let items: Vec<(Value, Value)> = match (&value, pattern) {
            (Value::Map(map), ForPattern::Value(_) | ForPattern::KeyValue { .. }) => {
                map.borrow().iter().map(|(k, v)| (k.to_value(), v.clone())).collect()
            }
            (Value::Map(_), ForPattern::Indexed { .. }) => {
                return error("use `for ($key => $value in $map)` to loop over a map with its keys", iterable.span);
            }
            (_, ForPattern::KeyValue { .. }) => {
                return error(format!("`=>` only works with maps, and this is a {}", value.type_name()), iterable.span);
            }
            (Value::List(list), _) => {
                // Looped by position, so elements added while looping are seen, like in JavaScript
                return self.exec_for_list(list.clone(), pattern, body, env);
            }
            (Value::Range(range), _) => {
                range.iter().enumerate().map(|(i, n)| (Value::Int(i as i64), Value::Int(n))).collect()
            }
            (Value::Str(s), _) => {
                s.chars().enumerate().map(|(i, c)| (Value::Int(i as i64), Value::string(&c.to_string()))).collect()
            }
            _ => return error(format!("can't loop over a {}", value.type_name()), iterable.span),
        };

        for (first, second) in items {
            let pass = scope::new_scope(env);
            match pattern {
                ForPattern::Value(name) => {
                    // A map without `=>` loops over its keys
                    let v = if matches!(value, Value::Map(_)) { first } else { second };
                    scope::declare(&pass, name, v, false);
                }
                ForPattern::Indexed { index, value: name } | ForPattern::KeyValue { key: index, value: name } => {
                    self.declare_loop_vars(&pass, index, first, name, second, iterable.span)?;
                }
                ForPattern::List { .. } => unreachable!("handled before"),
            }
            match self.exec_scoped(body, &pass) {
                Err(Flow::Break(_)) => break,
                Ok(()) | Err(Flow::Continue(_)) => {}
                Err(other) => return Err(other),
            }
        }
        Ok(())
    }

    fn exec_for_list(&mut self, list: Rc<RefCell<Vec<Value>>>, pattern: &ForPattern, body: &Stmt, env: &Env) -> Exec<()> {
        let mut i = 0;
        loop {
            let Some(item) = list.borrow().get(i).cloned() else { break };
            let pass = scope::new_scope(env);
            match pattern {
                ForPattern::Value(name) => {
                    scope::declare(&pass, name, item, false);
                }
                ForPattern::Indexed { index, value } => {
                    self.declare_loop_vars(&pass, index, Value::Int(i as i64), value, item, body.span)?;
                }
                ForPattern::List { names, rest } => {
                    self.destructure(&pass, names, rest, item, false, body.span)?;
                }
                ForPattern::KeyValue { .. } => unreachable!("checked before"),
            }
            match self.exec_scoped(body, &pass) {
                Err(Flow::Break(_)) => break,
                Ok(()) | Err(Flow::Continue(_)) => {}
                Err(other) => return Err(other),
            }
            i += 1;
        }
        Ok(())
    }

    /// `let [$a, $b, ...$rest] = $list;`
    fn destructure(&mut self, env: &Env, names: &[String], rest: &Option<String>, value: Value, constant: bool, span: Span) -> Exec<()> {
        let Value::List(list) = value else {
            return error(format!("only lists can be destructured, and this is a {}", value.type_name()), span);
        };
        let items = list.borrow().clone();
        match rest {
            None if items.len() != names.len() => {
                return error(format!("expected a list with {} elements, but it has {}", names.len(), items.len()), span);
            }
            Some(_) if items.len() < names.len() => {
                return error(format!("expected a list with at least {} elements, but it has {}", names.len(), items.len()), span);
            }
            _ => {}
        }
        let mut items = items.into_iter();
        for name in names {
            if !scope::declare(env, name, items.next().unwrap(), constant) {
                return error(format!("`${name}` already exists. Variables can't be declared twice"), span);
            }
        }
        if let Some(rest) = rest {
            if !scope::declare(env, rest, Value::list(items.collect()), constant) {
                return error(format!("`${rest}` already exists. Variables can't be declared twice"), span);
            }
        }
        Ok(())
    }

    /// The value of an `if` or `switch` branch.
    fn branch(&mut self, branch: &Branch, env: &Env) -> Exec<Value> {
        match branch {
            Branch::Expr(expr) => self.eval(expr, env),
            Branch::Block(stmts, span) => match self.exec_block(stmts, &scope::new_scope(env)) {
                Ok(()) => error("this block must give its value with `yield`", *span),
                Err(Flow::Yield(value, _)) => Ok(value),
                Err(other) => Err(other),
            },
        }
    }

    fn switch(&mut self, switch: &Switch, env: &Env, as_value: bool) -> Exec<Value> {
        let subject = self.eval(&switch.subject, env)?;
        for case in &switch.cases {
            for pattern in &case.patterns {
                let matched = match pattern {
                    Pattern::Value(expr) => match self.eval(expr, env)? {
                        Value::Range(range) if !matches!(subject, Value::Range(_)) => {
                            subject.as_whole().is_some_and(|n| range.contains(n))
                        }
                        value => subject.same(&value),
                    },
                    Pattern::Type(ty) => types::matches(&subject, ty).map_err(|m| flow_error(m, ty.span))?,
                };
                if matched {
                    return self.case_body(&case.body, env);
                }
            }
        }
        match &switch.default {
            Some(body) => self.case_body(body, env),
            None if as_value => error(format!("no case of the `switch` matches {}", describe(&subject)), switch.subject.span),
            None => Ok(Value::Null),
        }
    }

    fn case_body(&mut self, body: &CaseBody, env: &Env) -> Exec<Value> {
        match body {
            CaseBody::Value(branch) => self.branch(branch, env),
            CaseBody::Stmt(stmt) => {
                self.exec_scoped(stmt, env)?;
                Ok(Value::Null)
            }
        }
    }

    fn declare_loop_vars(&mut self, pass: &Env, a: &str, av: Value, b: &str, bv: Value, span: Span) -> Exec<()> {
        if a == b {
            return error(format!("`${a}` is used twice in the `for`"), span);
        }
        if !scope::declare(pass, a, av, false) || !scope::declare(pass, b, bv, false) {
            return error("a variable of the `for` already exists outside it", span);
        }
        Ok(())
    }

    // ---- Expressions ----

    /// Evaluates an expression. The common cases are handled here and the
    /// rest in `eval_rest`, so this function's stack frame stays small: it's
    /// on the stack once for every nested expression and call.
    pub fn eval(&mut self, expr: &Expr, env: &Env) -> Exec<Value> {
        match &expr.kind {
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Variable(name) => match scope::lookup(env, name) {
                Some(value) => Ok(value),
                None => undeclared(name, expr.span),
            },
            ExprKind::Binary { op, left, right } => self.eval_binary(*op, left, right, env, expr.span),
            ExprKind::Call { callee, args } => self.eval_call(callee, args, env, expr.span),
            _ => self.eval_rest(expr, env),
        }
    }

    #[inline(never)]
    fn eval_call(&mut self, callee: &Expr, args: &[Arg], env: &Env, span: Span) -> Exec<Value> {
        let callee_value = self.eval(callee, env)?;
        let Value::Function(function) = callee_value else {
            return error_as(exceptions::TYPE_ERROR, format!("a {} can't be called", callee_value.type_name()), callee.span);
        };
        let (positional, named) = self.eval_args(args, env)?;
        self.call(&function, positional, named, span)
    }

    #[inline(never)]
    fn eval_rest(&mut self, expr: &Expr, env: &Env) -> Exec<Value> {
        let span = expr.span;
        Ok(match &expr.kind {
            ExprKind::Int(n) => Value::Int(*n),
            ExprKind::Float(n) => Value::Float(*n),
            ExprKind::Str(s) => Value::string(s),
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Null => Value::Null,
            ExprKind::Symbol(name) => Value::Symbol(Rc::from(name.as_str())),
            ExprKind::Interpolated(parts) => {
                let mut text = String::new();
                for part in parts {
                    match part {
                        StringPart::Text(t) => text.push_str(t),
                        StringPart::Expr(e) => text.push_str(&self.eval(e, env)?.to_string()),
                    }
                }
                Value::string(&text)
            }
            ExprKind::Regex { pattern, flags } => {
                Value::Regex(self.regex(pattern, flags).map_err(|m| flow_error_as(exceptions::VALUE_ERROR, m, span))?)
            }
            ExprKind::Command(parts) => {
                let mut script = String::new();
                for part in parts {
                    match part {
                        StringPart::Text(t) => script.push_str(t),
                        StringPart::Expr(e) => {
                            let value = self.eval(e, env)?;
                            script.push_str(&command_argument(&value).map_err(|m| flow_error_as(exceptions::TYPE_ERROR, m, e.span))?);
                        }
                    }
                }
                Value::Command(Rc::new(RefCell::new(CommandValue::new(CommandKind::Shell(script)))))
            }
            ExprKind::ModuleFunction { module, name } => {
                let m = self.module_named(module, env, span)?;
                if !m.public.contains(name) {
                    return error(format!("`{}` has no `pub` function named `{name}`", m.path), span);
                }
                match scope::lookup_function(&m.env, name) {
                    Some(f) => Value::Function(f),
                    None => return error(format!("`{}` has no function named `{name}`", m.path), span),
                }
            }
            ExprKind::ModuleConstant { module, name } => {
                let m = self.module_named(module, env, span)?;
                if !m.public.contains(&format!("${name}")) {
                    return error(format!("`{}` has no `pub` constant named `${name}`", m.path), span);
                }
                match scope::lookup(&m.env, name) {
                    Some(v) => v,
                    None => return error(format!("`{}` has no constant named `${name}`", m.path), span),
                }
            }
            ExprKind::List { items, .. } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, env)?);
                }
                Value::list(values)
            }
            ExprKind::Map { entries, .. } => {
                let mut map = IndexMap::new();
                for (k, v) in entries {
                    let key = self.key(k, env)?;
                    let value = self.eval(v, env)?;
                    map.insert(key, value);
                }
                Value::Map(Rc::new(RefCell::new(map)))
            }
            ExprKind::Variable(name) => match scope::lookup(env, name) {
                Some(value) => value,
                None => return error(format!("`${name}` isn't declared"), span),
            },
            ExprKind::Function(name) => match scope::lookup_function(env, name) {
                Some(function) => Value::Function(function),
                None => return error(format!("there's no function named `{name}`. Functions must be defined before they're used"), span),
            },
            ExprKind::Unary { op, operand } => {
                let value = self.eval(operand, env)?;
                unary(*op, value, span)?
            }
            ExprKind::Binary { op, left, right } => self.eval_binary(*op, left, right, env, span)?,
            ExprKind::PathTest { op, path } => match self.eval(path, env)? {
                Value::Str(p) => Value::Bool(patterns::path_test(*op, &self.host.state.resolve(&p).display().to_string())),
                other => return error(format!("`{}` needs a path string, not a {}", op.symbol(), other.type_name()), path.span),
            },
            ExprKind::Range { start, end, inclusive } => {
                let Some(end) = end else {
                    return error("a range without an end can only be used as an index *(i.e. `$text[2..]`)*", span);
                };
                let start = self.range_bound(start, env)?;
                let end = self.range_bound(end, env)?;
                Value::Range(Range { start, end, inclusive: *inclusive, step: 1 })
            }
            ExprKind::Ternary { cond, then, otherwise } => {
                if self.condition(cond, env, "? :")? {
                    self.eval(then, env)?
                } else {
                    self.eval(otherwise, env)?
                }
            }
            ExprKind::Assign { op, target, value } => self.eval_assign(*op, target, value, env, span)?,
            ExprKind::Increment { target, delta, prefix } => {
                let old = self.eval(target, env)?;
                if old.as_f64().is_none() {
                    return error(format!("`++` and `--` only work on numbers, and this is a {}", old.type_name()), span);
                }
                let new = arithmetic(BinaryOp::Add, &old, &Value::Int(*delta as i64)).map_err(|m| op_error(m, span))?;
                self.store(target, new.clone(), env)?;
                if *prefix { new } else { old }
            }
            ExprKind::Call { callee, args } => self.eval_call(callee, args, env, span)?,
            ExprKind::MethodCall { receiver, method, args, nullable } => {
                let receiver = self.eval(receiver, env)?;
                if *nullable && matches!(receiver, Value::Null) {
                    return Ok(Value::Null);
                }
                let (positional, named) = self.eval_args(args, env)?;
                if !named.is_empty() {
                    return error("methods don't take named arguments yet", span);
                }
                methods::call(self, receiver, method, positional, span)?
            }
            ExprKind::TypeCheck { receiver, ty, check, nullable } => {
                let value = self.eval(receiver, env)?;
                if *nullable && matches!(value, Value::Null) {
                    return Ok(Value::Null);
                }
                let ok = types::matches(&value, ty).map_err(|m| flow_error(m, ty.span))?;
                match check {
                    TypeCheckKind::Is => Value::Bool(ok),
                    TypeCheckKind::As if ok => value,
                    TypeCheckKind::As => {
                        return error_as(exceptions::TYPE_ERROR, format!("`.as({ty})` failed: the value is {}", describe(&value)), span);
                    }
                }
            }
            ExprKind::Lambda(decl) => {
                Value::Function(Rc::new(Function::User(UserFunction { decl: decl.clone(), env: env.clone() })))
            }
            ExprKind::If { cond, then, otherwise } => {
                if self.condition(cond, env, "if")? {
                    self.branch(then, env)?
                } else {
                    self.branch(otherwise, env)?
                }
            }
            ExprKind::Switch(switch) => self.switch(switch, env, true)?,
            ExprKind::Try(inner) => match self.eval(inner, env) {
                Ok(value) => Value::Result(Rc::new(ResultValue::Ok(value))),
                Err(Flow::Throw(exception)) => Value::Result(Rc::new(ResultValue::Error(Value::Exception(exception)))),
                Err(other) => return Err(other),
            },
            ExprKind::Index { target, index, nullable } => {
                let container = self.eval(target, env)?;
                if *nullable && matches!(container, Value::Null) {
                    return Ok(Value::Null);
                }
                self.eval_index(container, index, env)?
            }
        })
    }

    #[inline(never)]
    fn eval_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, env: &Env, span: Span) -> Exec<Value> {
        // The ones that don't always evaluate their right side
        match op {
            BinaryOp::And | BinaryOp::Or => {
                let name = if op == BinaryOp::And { "&&" } else { "||" };
                let Value::Bool(l) = self.eval(left, env)? else {
                    return error_as(exceptions::TYPE_ERROR, format!("both sides of `{name}` must be bools"), left.span);
                };
                if (op == BinaryOp::And && !l) || (op == BinaryOp::Or && l) {
                    return Ok(Value::Bool(l));
                }
                return match self.eval(right, env)? {
                    Value::Bool(r) => Ok(Value::Bool(r)),
                    _ => error_as(exceptions::TYPE_ERROR, format!("both sides of `{name}` must be bools"), right.span),
                };
            }
            BinaryOp::Coalesce => {
                let l = self.eval(left, env)?;
                return if matches!(l, Value::Null) { self.eval(right, env) } else { Ok(l) };
            }
            _ => {}
        }
        let l = self.eval(left, env)?;
        let r = self.eval(right, env)?;
        match op {
            BinaryOp::Like | BinaryOp::NotLike => {
                let (Value::Str(text), Value::Str(pattern)) = (&l, &r) else {
                    return error_as(exceptions::TYPE_ERROR, format!("`{}` needs two strings, but got a {} and a {}", op.symbol(), l.type_name(), r.type_name()), span);
                };
                let found = patterns::like(text, pattern).map_err(|m| flow_error(m, right.span))?;
                Ok(Value::Bool(found == (op == BinaryOp::Like)))
            }
            BinaryOp::Match | BinaryOp::NotMatch => {
                let Value::Str(text) = &l else {
                    return error(format!("`{}` needs a string on its left, not a {}", op.symbol(), l.type_name()), left.span);
                };
                let regex = self.as_regex(&r).map_err(|m| flow_error_as(exceptions::VALUE_ERROR, m, right.span))?;
                Ok(Value::Bool(regex.compiled.is_match(text) == (op == BinaryOp::Match)))
            }
            _ => binary(op, l, r).map_err(|m| op_error(m, span)),
        }
    }

    fn eval_assign(&mut self, op: Option<BinaryOp>, target: &Expr, value: &Expr, env: &Env, span: Span) -> Exec<Value> {
        let new = match op {
            None => self.eval(value, env)?,
            Some(op) => {
                let old = self.eval(target, env)?;
                let r = self.eval(value, env)?;
                binary(op, old, r).map_err(|m| op_error(m, span))?
            }
        };
        self.store(target, new.clone(), env)?;
        Ok(new)
    }

    /// Writes into a variable or an index.
    fn store(&mut self, target: &Expr, value: Value, env: &Env) -> Exec<()> {
        match &target.kind {
            ExprKind::Variable(name) => match scope::assign(env, name, value) {
                Ok(()) => Ok(()),
                Err(SetError::Undeclared) => {
                    error(format!("`${name}` isn't declared. Declare it with `let ${name} = ...;`"), target.span)
                }
                Err(SetError::Constant) => error(format!("`${name}` is a constant, it can't be changed"), target.span),
            },
            ExprKind::Index { target: container, index, .. } => {
                let container = self.eval(container, env)?;
                match container {
                    Value::List(list) => {
                        let i = self.eval(index, env)?;
                        let len = list.borrow().len();
                        let pos = list_position(&i, len).map_err(|m| index_error(m, index.span))?;
                        list.borrow_mut()[pos] = value;
                        Ok(())
                    }
                    Value::Map(map) => {
                        let key = self.key(index, env)?;
                        map.borrow_mut().insert(key, value);
                        Ok(())
                    }
                    Value::Str(_) => error("strings can't be changed. Build a new one instead", target.span),
                    other => error(format!("a {} can't be indexed", other.type_name()), target.span),
                }
            }
            _ => error("only variables and indexes can be assigned to", target.span),
        }
    }

    fn eval_index(&mut self, container: Value, index: &Expr, env: &Env) -> Exec<Value> {
        // `$list[2..]`: a range without an end goes to the end
        if let ExprKind::Range { start, end: None, .. } = &index.kind {
            let start = self.range_bound(start, env)?;
            let len = match &container {
                Value::List(l) => l.borrow().len(),
                Value::Str(s) => s.chars().count(),
                other => return error(format!("a {} can't be sliced", other.type_name()), index.span),
            };
            let range = Range { start, end: len as i64, inclusive: false, step: 1 };
            return slice(&container, range, len).map_err(|m| index_error(m, index.span));
        }
        match container {
            Value::Map(map) => {
                let key = self.key(index, env)?;
                Ok(map.borrow().get(&key).cloned().unwrap_or(Value::Null))
            }
            Value::List(_) | Value::Str(_) => {
                let i = self.eval(index, env)?;
                let len = match &container {
                    Value::List(l) => l.borrow().len(),
                    Value::Str(s) => s.chars().count(),
                    _ => unreachable!(),
                };
                if let Value::Range(range) = i {
                    return slice(&container, range, len).map_err(|m| index_error(m, index.span));
                }
                let pos = list_position(&i, len).map_err(|m| index_error(m, index.span))?;
                Ok(match &container {
                    Value::List(l) => l.borrow()[pos].clone(),
                    Value::Str(s) => Value::string(&s.chars().nth(pos).unwrap().to_string()),
                    _ => unreachable!(),
                })
            }
            other => error(format!("a {} can't be indexed", other.type_name()), index.span),
        }
    }

    fn key(&mut self, expr: &Expr, env: &Env) -> Exec<Key> {
        let value = self.eval(expr, env)?;
        match Key::from_value(&value) {
            Some(key) => Ok(key),
            None => error(format!("map keys can be strings, numbers or symbols, not a {}", value.type_name()), expr.span),
        }
    }

    fn range_bound(&mut self, expr: &Expr, env: &Env) -> Exec<i64> {
        let value = self.eval(expr, env)?;
        match value.as_whole() {
            Some(n) => Ok(n),
            None => error(format!("ranges need whole numbers, and this is {}", describe(&value)), expr.span),
        }
    }

    // ---- Calls ----

    fn eval_args(&mut self, args: &[Arg], env: &Env) -> Exec<(Vec<Value>, Vec<(String, Value, Span)>)> {
        let mut positional = Vec::new();
        let mut named = Vec::new();
        for arg in args {
            let value = self.eval(&arg.value, env)?;
            if arg.spread {
                match value {
                    Value::List(list) => positional.extend(list.borrow().iter().cloned()),
                    other => return error(format!("only lists can be spread with `...`, not a {}", other.type_name()), arg.value.span),
                }
            } else if let Some(name) = &arg.name {
                named.push((name.clone(), value, arg.value.span));
            } else {
                positional.push(value);
            }
        }
        Ok((positional, named))
    }

    pub fn call(
        &mut self,
        function: &Rc<Function>,
        positional: Vec<Value>,
        named: Vec<(String, Value, Span)>,
        span: Span,
    ) -> Exec<Value> {
        match &**function {
            Function::Native(native) => self.call_native(native, positional, named, span),
            Function::User(user) => {
                if stack_position().abs_diff(self.stack_base) > self.stack_budget {
                    return error("too many nested calls. Is there a recursion that never ends?", span);
                }
                let call_env = self.bind_params(user, positional, named, span)?;
                match &user.decl.body {
                    FunctionBody::Expr(expr) => self.eval(expr, &call_env),
                    FunctionBody::Block(stmts) => match self.exec_block(stmts, &call_env) {
                        Ok(()) => Ok(Value::Null),
                        Err(Flow::Return(value, _)) => Ok(value),
                        Err(Flow::Break(s)) => error("`break` outside a loop", s),
                        Err(Flow::Continue(s)) => error("`continue` outside a loop", s),
                        Err(Flow::Yield(_, s)) => error(YIELD_OUTSIDE, s),
                        Err(e) => Err(e),
                    },
                }
            }
        }
    }

    #[inline(never)]
    fn call_native(
        &mut self,
        native: &crate::value::NativeFunction,
        positional: Vec<Value>,
        named: Vec<(String, Value, Span)>,
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

    // ---- Modules ----

    fn exec_use(&mut self, decl: &UseDecl, env: &Env, span: Span) -> Exec<()> {
        let source = self.host.loader.resolve(&decl.path).map_err(|m| flow_error(m, span))?;
        match &decl.items {
            UseItems::Module { alias } => {
                let name = alias.clone().unwrap_or_else(|| decl.path.last().cloned().unwrap_or_default());
                if !scope::declare_module(env, &name, ModuleSlot::Pending(source)) {
                    return error(format!("a module named `{name}` is already imported"), span);
                }
            }
            UseItems::Items(items) => {
                // Importing names needs their values now
                let module = self.load(source, span)?;
                for item in items {
                    let key = if item.variable { format!("${}", item.name) } else { item.name.clone() };
                    let local = item.alias.clone().unwrap_or_else(|| item.name.clone());
                    if !module.public.contains(&key) {
                        let what = if item.variable { "constant" } else { "function" };
                        return error(format!("`{}` has no `pub` {what} named `{key}`", module.path), span);
                    }
                    let declared = if item.variable {
                        let value = scope::lookup(&module.env, &item.name).unwrap_or(Value::Null);
                        scope::declare(env, &local, value, true)
                    } else {
                        match scope::lookup_function(&module.env, &item.name) {
                            Some(f) => scope::declare_function(env, &local, f),
                            None => false,
                        }
                    };
                    if !declared {
                        let shown = if item.variable { format!("${local}") } else { local };
                        return error(format!("`{shown}` already exists"), span);
                    }
                }
            }
        }
        Ok(())
    }

    /// The module imported as `name`, loading it the first time it's used.
    fn module_named(&mut self, name: &str, env: &Env, span: Span) -> Exec<Rc<Module>> {
        let Some(slot) = scope::lookup_module(env, name) else {
            return error(format!("there's no module named `{name}`. Import it with `use`"), span);
        };
        let pending = match &*slot.borrow() {
            ModuleSlot::Loaded(m) => return Ok(m.clone()),
            ModuleSlot::Pending(source) => source.clone(),
        };
        let module = self.load(pending, span)?;
        *slot.borrow_mut() = ModuleSlot::Loaded(module.clone());
        Ok(module)
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
        let program = crate::parse_file(&source, file).map_err(|d| flow_error(d.message, d.span))?;

        let env = scope::new_scope(&self.host.builtins);
        let mut public = std::collections::HashSet::new();
        self.host.loader.loading.push(path.clone());
        let result = (|| -> Exec<()> {
            for stmt in &program {
                let inner = match &stmt.kind {
                    StmtKind::Pub(inner) => {
                        match &inner.kind {
                            StmtKind::Function(decl) => public.insert(decl.name.clone()),
                            StmtKind::Let { name, .. } => public.insert(format!("${name}")),
                            _ => false,
                        };
                        &**inner
                    }
                    _ => stmt,
                };
                let declaration = matches!(
                    inner.kind,
                    StmtKind::Use(_) | StmtKind::Function(_) | StmtKind::TypeAlias { .. } | StmtKind::Let { constant: true, .. }
                );
                if !declaration {
                    return error(
                        "an imported module can only have declarations at its top level: functions, `const` and `type`",
                        stmt.span,
                    );
                }
                self.exec(stmt, &env)?;
            }
            Ok(())
        })();
        self.host.loader.loading.pop();
        result?;
        let module = Rc::new(Module { path: shown, env, public });
        self.host.loader.cache.insert(path, module.clone());
        Ok(module)
    }

    /// The scope of a call, with every parameter declared.
    #[inline(never)]
    fn bind_params(
        &mut self,
        user: &UserFunction,
        mut positional: Vec<Value>,
        mut named: Vec<(String, Value, Span)>,
        span: Span,
    ) -> Exec<Env> {
        let decl = &user.decl;
        let call_env = scope::new_scope(&user.env);
        positional.reverse(); // so the next one is popped from the end
        for param in &decl.params {
            let value = if param.variadic {
                positional.reverse();
                Value::list(std::mem::take(&mut positional))
            } else if let Some(v) = positional.pop() {
                if named.iter().any(|(n, ..)| n == &param.name) {
                    return error_as(exceptions::TYPE_ERROR, format!("`{}` was given twice", param.name), span);
                }
                v
            } else if let Some(i) = named.iter().position(|(n, ..)| n == &param.name) {
                named.remove(i).1
            } else if let Some(default) = &param.default {
                // Evaluated on every call, in the function's own scope
                self.eval(default, &call_env)?
            } else {
                return error_as(exceptions::TYPE_ERROR, format!("`{}` needs a value for `${}`", decl.name, param.name), span);
            };
            if !scope::declare(&call_env, &param.name, value, false) {
                return error(
                    format!("the parameter `${}` of `{}` has the name of a variable that already exists", param.name, decl.name),
                    span,
                );
            }
        }
        if !positional.is_empty() {
            return error_as(exceptions::TYPE_ERROR, format!("`{}` takes {} arguments, but got more", decl.name, decl.params.len()), span);
        }
        if let Some((name, _, arg_span)) = named.first() {
            return error_as(exceptions::TYPE_ERROR, format!("`{}` has no parameter named `{name}`", decl.name), *arg_span);
        }
        Ok(call_env)
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

#[cold]
#[inline(never)]
fn undeclared(name: &str, span: Span) -> Exec<Value> {
    error(format!("`${name}` isn't declared"), span)
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
