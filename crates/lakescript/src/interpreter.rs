//! Runs the syntax tree directly (phase 1 of `ideas/roadmap.md`).

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::methods;
use crate::scope::{self, Env, SetError};
use crate::token::Span;
use crate::value::{Function, Key, NativeContext, Range, UserFunction, Value};

/// Why the normal flow stopped.
pub enum Flow {
    Break(Span),
    Continue(Span),
    Return(Value, Span),
    Error(Diagnostic),
}

pub type Exec<T> = Result<T, Flow>;

pub fn error<T>(message: impl Into<String>, span: Span) -> Exec<T> {
    Err(Flow::Error(Diagnostic::new(message, span)))
}

pub struct Interpreter<'a> {
    pub output: &'a mut dyn Write,
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
    pub fn new(output: &'a mut dyn Write, stack_budget: usize) -> Self {
        Interpreter { output, stack_base: stack_position(), stack_budget }
    }

    /// Runs the statements of a whole script.
    pub fn run(&mut self, program: &[Stmt], env: &Env) -> Result<(), Diagnostic> {
        for stmt in program {
            match self.exec(stmt, env) {
                Ok(()) => {}
                Err(Flow::Error(diagnostic)) => return Err(diagnostic),
                Err(Flow::Break(span)) => return Err(Diagnostic::new("`break` outside a loop", span)),
                Err(Flow::Continue(span)) => return Err(Diagnostic::new("`continue` outside a loop", span)),
                Err(Flow::Return(_, span)) => return Err(Diagnostic::new("`return` outside a function", span)),
            }
        }
        Ok(())
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
            other => error(
                format!("the condition of `{keyword}` must be a bool, but this is a {}", other.type_name()),
                cond.span,
            ),
        }
    }

    fn exec_for(&mut self, pattern: &ForPattern, iterable: &Expr, body: &Stmt, env: &Env) -> Exec<()> {
        let value = self.eval(iterable, env)?;
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

    pub fn eval(&mut self, expr: &Expr, env: &Env) -> Exec<Value> {
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
            ExprKind::Regex { .. } => return error("regexes aren't supported yet", span),
            ExprKind::Command(_) => return error("commands aren't supported yet", span),
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
            ExprKind::PathTest { .. } => return error("path tests like `-f` aren't supported yet", span),
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
                let new = arithmetic(BinaryOp::Add, &old, &Value::Int(*delta as i64)).map_err(|m| flow_error(m, span))?;
                self.store(target, new.clone(), env)?;
                if *prefix { new } else { old }
            }
            ExprKind::Call { callee, args } => {
                let callee_value = self.eval(callee, env)?;
                let Value::Function(function) = callee_value else {
                    return error(format!("a {} can't be called", callee_value.type_name()), callee.span);
                };
                let (positional, named) = self.eval_args(args, env)?;
                self.call(&function, positional, named, span)?
            }
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
            ExprKind::TypeCheck { .. } => return error("`.as()` and `.is()` aren't supported yet", span),
            ExprKind::Index { target, index, nullable } => {
                let container = self.eval(target, env)?;
                if *nullable && matches!(container, Value::Null) {
                    return Ok(Value::Null);
                }
                self.eval_index(container, index, env)?
            }
        })
    }

    fn eval_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, env: &Env, span: Span) -> Exec<Value> {
        // The ones that don't always evaluate their right side
        match op {
            BinaryOp::And | BinaryOp::Or => {
                let name = if op == BinaryOp::And { "&&" } else { "||" };
                let Value::Bool(l) = self.eval(left, env)? else {
                    return error(format!("both sides of `{name}` must be bools"), left.span);
                };
                if (op == BinaryOp::And && !l) || (op == BinaryOp::Or && l) {
                    return Ok(Value::Bool(l));
                }
                return match self.eval(right, env)? {
                    Value::Bool(r) => Ok(Value::Bool(r)),
                    _ => error(format!("both sides of `{name}` must be bools"), right.span),
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
        binary(op, l, r).map_err(|m| flow_error(m, span))
    }

    fn eval_assign(&mut self, op: Option<BinaryOp>, target: &Expr, value: &Expr, env: &Env, span: Span) -> Exec<Value> {
        let new = match op {
            None => self.eval(value, env)?,
            Some(op) => {
                let old = self.eval(target, env)?;
                let r = self.eval(value, env)?;
                binary(op, old, r).map_err(|m| flow_error(m, span))?
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
                        let pos = list_position(&i, len).map_err(|m| flow_error(m, index.span))?;
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
            return slice(&container, range, len).map_err(|m| flow_error(m, index.span));
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
                    return slice(&container, range, len).map_err(|m| flow_error(m, index.span));
                }
                let pos = list_position(&i, len).map_err(|m| flow_error(m, index.span))?;
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
        mut positional: Vec<Value>,
        mut named: Vec<(String, Value, Span)>,
        span: Span,
    ) -> Exec<Value> {
        match &**function {
            Function::Native(native) => {
                if !named.is_empty() {
                    return error(format!("`{}` doesn't take named arguments", native.name), span);
                }
                let mut context = NativeContext { output: &mut *self.output };
                (native.func)(&mut context, positional).map_err(|m| flow_error(m, span))
            }
            Function::User(user) => {
                if stack_position().abs_diff(self.stack_base) > self.stack_budget {
                    return error("too many nested calls. Is there a recursion that never ends?", span);
                }
                let decl = &user.decl;
                let call_env = scope::new_scope(&user.env);
                positional.reverse(); // so the next one is popped from the end
                for param in &decl.params {
                    let value = if param.variadic {
                        positional.reverse();
                        Value::list(std::mem::take(&mut positional))
                    } else if let Some(v) = positional.pop() {
                        if named.iter().any(|(n, ..)| n == &param.name) {
                            return error(format!("`{}` was given twice", param.name), span);
                        }
                        v
                    } else if let Some(i) = named.iter().position(|(n, ..)| n == &param.name) {
                        named.remove(i).1
                    } else if let Some(default) = &param.default {
                        // Evaluated on every call, in the function's own scope
                        self.eval(default, &call_env)?
                    } else {
                        return error(format!("`{}` needs a value for `${}`", decl.name, param.name), span);
                    };
                    if !scope::declare(&call_env, &param.name, value, false) {
                        return error(format!("the parameter `${}` of `{}` has the name of a variable that already exists", param.name, decl.name), span);
                    }
                }
                if !positional.is_empty() {
                    return error(format!("`{}` takes {} arguments, but got more", decl.name, decl.params.len()), span);
                }
                if let Some((name, _, arg_span)) = named.first() {
                    return error(format!("`{}` has no parameter named `{name}`", decl.name), *arg_span);
                }

                let result = match &decl.body {
                    FunctionBody::Expr(expr) => self.eval(expr, &call_env),
                    FunctionBody::Block(stmts) => match self.exec_block(stmts, &call_env) {
                        Ok(()) => Ok(Value::Null),
                        Err(Flow::Return(value, _)) => Ok(value),
                        Err(Flow::Break(s)) => error("`break` outside a loop", s),
                        Err(Flow::Continue(s)) => error("`continue` outside a loop", s),
                        Err(e) => Err(e),
                    },
                };
                result
            }
        }
    }
}

fn flow_error(message: String, span: Span) -> Flow {
    Flow::Error(Diagnostic::new(message, span))
}

/// A value in an error message: `5`, `'abc'`, a list...
fn describe(value: &Value) -> String {
    match value {
        Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Null => format!("`{value}`"),
        Value::Str(_) => format!("the string `{value:?}`"),
        other => format!("a {}", other.type_name()),
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
        (UnaryOp::BitNot, Value::Float(_)) => error("bitwise operators need whole numbers", span),
        (UnaryOp::Not, _) => error(format!("`!` needs a bool, and this is a {}", value.type_name()), span),
        _ => error(format!("`{}` needs a number, and this is a {}", op.symbol(), value.type_name()), span),
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
        Like | NotLike | Match | NotMatch => Err(format!("`{}` isn't supported yet", op.symbol())),
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
