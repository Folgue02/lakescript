//! Compiles the syntax tree to bytecode *(see `ideas/tooling/vm.md`)*.
//!
//! Every variable is resolved here: to a slot of the frame, a cell (when a
//! closure uses it), an upvalue of the closure, a global, or a built-in
//! function. Running the code never looks a name up.
//!
//! The errors the old tree-walking interpreter gave while running (declaring
//! a variable twice, changing a constant) are still given while running: the
//! compiler writes a `Fail` instruction where they happen.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::*;
use crate::bytecode::{ArgInfo, ArgKind, ArgType, Capture, ClassInfo, CommandInfo, Import, Op, Param as ParamInfo, Proto, class_names, for_kind};
use crate::diagnostic::Diagnostic;
use crate::modules::Globals;
use crate::stack::position as stack_position;
use crate::token::Span;
use crate::value::Value;

pub const YIELD_OUTSIDE: &str = "`yield` can only be used in the block of an `if` or `switch` used as a value";

type Compiled<T = ()> = Result<T, Diagnostic>;

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Slot(u32),
    Cell(u32),
}

struct Local {
    key: String,
    place: Place,
    constant: bool,
}

/// Where a name was found.
#[derive(Clone, Copy)]
enum Resolved {
    Local(Place, bool),
    Upvalue(u32, bool),
    Global(u32),
    Builtin(u32),
}

struct Loop {
    breaks: Vec<usize>,
    continue_to: usize,
    /// How many handlers and expression blocks were active when the loop started.
    handlers: usize,
    expr_blocks: usize,
    sp_slot: u32,
}

/// The block of an `if` or `switch` used as a value.
struct ValueBlock {
    yields: Vec<usize>,
    handlers: usize,
}

#[derive(Clone)]
enum Handler {
    Catch,
    TryExpr,
    /// A `finally`, whose code is written again wherever the `try` is left.
    Finally(Rc<Vec<Stmt>>),
}

struct FnState {
    proto: Proto,
    locals: Vec<Local>,
    /// The number of locals, slots and cells when each open scope started.
    scopes: Vec<(usize, u32, u32)>,
    next_slot: u32,
    next_cell: u32,
    /// Names that closures inside this function use: its variables with these names live in cells.
    captured: HashSet<String>,
    upvalues: Vec<(String, Capture, bool)>,
    loops: Vec<Loop>,
    blocks: Vec<ValueBlock>,
    handlers: Vec<Handler>,
    expr_blocks: usize,
    /// The top level of a script or module, where declarations outside blocks are globals.
    top_level: bool,
    /// Functions whose places were made when their block started *(see `hoist`)*.
    hoisted: HashSet<String>,
}

impl FnState {
    fn new(name: &str, captured: HashSet<String>, top_level: bool) -> Self {
        FnState {
            proto: Proto { name: Rc::from(name), ..Default::default() },
            locals: Vec::new(),
            scopes: Vec::new(),
            hoisted: HashSet::new(),
            next_slot: 0,
            next_cell: 0,
            captured,
            upvalues: Vec::new(),
            loops: Vec::new(),
            blocks: Vec::new(),
            handlers: Vec::new(),
            expr_blocks: 0,
            top_level,
        }
    }
}

pub struct Compiler<'a> {
    globals: &'a Globals,
    builtins: &'a HashMap<String, u32>,
    functions: Vec<FnState>,
    /// Globals declared so far, in the order of the code.
    declared: HashSet<String>,
    /// Compiling a module imported with `use`: only declarations at its top level.
    module: bool,
    /// `report::` for the functions of the module `report`, for tracebacks.
    prefix: String,
    /// How tracebacks name the top level, when it isn't a script or a module *(i.e. `the template`)*.
    top_shown: Option<String>,
    /// What a module makes available with `pub`.
    pub public: HashSet<String>,
    /// The classes being compiled, the innermost last, for `Self`.
    classes: Vec<String>,
    span: Span,
    stack_base: usize,
    stack_budget: usize,
}

impl<'a> Compiler<'a> {
    pub fn new(globals: &'a Globals, builtins: &'a HashMap<String, u32>, stack_budget: usize) -> Self {
        Compiler {
            globals,
            builtins,
            functions: Vec::new(),
            declared: HashSet::new(),
            module: false,
            prefix: String::new(),
            top_shown: None,
            public: HashSet::new(),
            classes: Vec::new(),
            span: Span::default(),
            stack_base: stack_position(),
            stack_budget,
        }
    }

    /// Compiles a whole script.
    pub fn script(mut self, program: &[Stmt]) -> Compiled<Rc<Proto>> {
        self.top(program, "script")
    }

    /// Compiles a module imported with `use`, and what it makes public.
    pub fn module(mut self, program: &[Stmt], name: &str) -> Compiled<(Rc<Proto>, HashSet<String>)> {
        self.module = true;
        let stem = std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.prefix = format!("{stem}::");
        self.top_shown = Some(format!("the module {stem}"));
        let proto = self.top(program, name)?;
        Ok((proto, self.public))
    }

    /// Names the top level in tracebacks *(i.e. `the template`)*.
    pub fn shown_as(mut self, name: &str) -> Self {
        self.top_shown = Some(name.to_string());
        self
    }

    fn top(&mut self, program: &[Stmt], name: &str) -> Compiled<Rc<Proto>> {
        let mut captured = HashSet::new();
        collect_stmts(program, false, &mut captured);
        self.functions.push(FnState::new(name, captured, true));
        self.f().proto.shown = Rc::from(self.top_shown.clone().unwrap_or_else(|| "the script".to_string()));
        self.hoist(program);
        for stmt in program {
            if self.module {
                let inner = match &stmt.kind {
                    StmtKind::Pub(inner) => &**inner,
                    _ => stmt,
                };
                let declaration = matches!(
                    inner.kind,
                    StmtKind::Use(_) | StmtKind::Function(_) | StmtKind::Let { .. } | StmtKind::Class(_)
                );
                if !declaration {
                    self.span = stmt.span;
                    self.fail("an imported module can only have declarations at its top level: functions, `const`, `let`, classes, interfaces and enums");
                    continue;
                }
            }
            self.stmt(stmt)?;
        }
        self.emit(Op::Null);
        self.emit(Op::Return);
        Ok(Rc::new(self.finish()))
    }

    fn finish(&mut self) -> Proto {
        let f = self.functions.pop().expect("a function is being compiled");
        let mut proto = f.proto;
        proto.captures = f.upvalues.iter().map(|(_, c, _)| *c).collect();
        proto.regex_cache = proto.regexes.iter().map(|_| Default::default()).collect();
        proto.field_cache = proto.names.iter().map(|_| Default::default()).collect();
        proto.method_cache = proto.names.iter().map(|_| Default::default()).collect();
        proto
    }

    // ---- Emitting ----

    fn f(&mut self) -> &mut FnState {
        self.functions.last_mut().expect("a function is being compiled")
    }

    fn emit(&mut self, op: Op) -> usize {
        let span = self.span;
        self.emit_at(op, span)
    }

    fn emit_at(&mut self, op: Op, span: Span) -> usize {
        let proto = &mut self.f().proto;
        proto.code.push(op);
        proto.spans.push(span);
        proto.code.len() - 1
    }

    fn here(&mut self) -> u32 {
        self.f().proto.code.len() as u32
    }

    /// Points the jump at `at` to the next instruction.
    fn patch(&mut self, at: usize) {
        let target = self.here();
        self.patch_to(at, target);
    }

    fn patch_to(&mut self, at: usize, to: u32) {
        let op = &mut self.f().proto.code[at];
        match op {
            Op::Jump(t)
            | Op::JumpIfTrueRaw(t)
            | Op::JumpIfFalseRaw(t)
            | Op::JumpIfNotNull(t)
            | Op::JumpIfNull(t)
            | Op::PushCatch(t)
            | Op::PushTry(t)
            | Op::JumpIfFalse { target: t, .. }
            | Op::JumpIfNot { target: t, .. }
            | Op::JumpLogic { target: t, .. }
            | Op::JumpIfGiven { target: t, .. }
            | Op::PushFinally { target: t, .. }
            | Op::ForNext { exit: t, .. } => *t = to,
            other => unreachable!("{other:?} isn't a jump"),
        }
    }

    fn constant(&mut self, value: Value) -> u32 {
        let consts = &mut self.f().proto.consts;
        consts.push(value);
        consts.len() as u32 - 1
    }

    fn name(&mut self, name: &str) -> u32 {
        let names = &mut self.f().proto.names;
        if let Some(i) = names.iter().position(|n| &**n == name) {
            return i as u32;
        }
        names.push(Rc::from(name));
        names.len() as u32 - 1
    }

    fn ty(&mut self, ty: &TypeExpr) -> u32 {
        let ty = ty.clone();
        let types = &mut self.f().proto.types;
        types.push(ty);
        types.len() as u32 - 1
    }


    /// An error thrown when this point of the code runs.
    fn fail(&mut self, message: impl AsRef<str>) {
        let n = self.name(message.as_ref());
        self.emit(Op::Fail(n));
    }

    fn check_stack(&self, span: Span) -> Compiled {
        if stack_position().abs_diff(self.stack_base) > self.stack_budget + self.stack_budget / 8 {
            return Err(Diagnostic::new("the code is nested too deeply", span));
        }
        Ok(())
    }

    // ---- Scopes and names ----

    fn open_scope(&mut self) {
        let f = self.f();
        let mark = (f.locals.len(), f.next_slot, f.next_cell);
        f.scopes.push(mark);
    }

    fn close_scope(&mut self) {
        let f = self.f();
        let (locals, slot, cell) = f.scopes.pop().expect("a scope is open");
        f.locals.truncate(locals);
        f.next_slot = slot;
        f.next_cell = cell;
    }

    fn at_global_scope(&mut self) -> bool {
        let f = self.f();
        f.top_level && f.scopes.is_empty()
    }

    /// A slot for the compiler's own use, until the scope closes.
    fn hidden_slot(&mut self) -> u32 {
        let f = self.f();
        let slot = f.next_slot;
        f.next_slot += 1;
        f.proto.slots = f.proto.slots.max(f.next_slot);
        slot
    }

    fn new_place(&mut self, key: &str) -> Place {
        let f = self.f();
        if f.captured.contains(key) {
            let cell = f.next_cell;
            f.next_cell += 1;
            f.proto.cells = f.proto.cells.max(f.next_cell);
            Place::Cell(cell)
        } else {
            let slot = f.next_slot;
            f.next_slot += 1;
            f.proto.slots = f.proto.slots.max(f.next_slot);
            Place::Slot(slot)
        }
    }

    fn find_local(f: &FnState, key: &str) -> Option<(Place, bool)> {
        f.locals.iter().rev().find(|l| l.key == key).map(|l| (l.place, l.constant))
    }

    fn resolve_upvalue(&mut self, fi: usize, key: &str) -> Option<(u32, bool)> {
        if fi == 0 {
            return None;
        }
        if let Some(i) = self.functions[fi].upvalues.iter().position(|(k, ..)| k == key) {
            return Some((i as u32, self.functions[fi].upvalues[i].2));
        }
        let capture = match Self::find_local(&self.functions[fi - 1], key) {
            Some((Place::Cell(cell), constant)) => (Capture::Cell(cell), constant),
            // Can't happen: the names closures use are in cells. If it did, it's
            // reached as a global, which fails when it runs instead of crashing
            Some((Place::Slot(_), _)) => return None,
            None => {
                let (up, constant) = self.resolve_upvalue(fi - 1, key)?;
                (Capture::Upvalue(up), constant)
            }
        };
        let upvalues = &mut self.functions[fi].upvalues;
        upvalues.push((key.to_string(), capture.0, capture.1));
        Some((upvalues.len() as u32 - 1, capture.1))
    }

    fn global_declared(&self, key: &str) -> bool {
        self.declared.contains(key) || self.globals.is_declared(key)
    }

    /// Finds a name: `$x` for a variable, `f` for a function.
    fn resolve(&mut self, key: &str) -> Resolved {
        let fi = self.functions.len() - 1;
        if let Some((place, constant)) = Self::find_local(&self.functions[fi], key) {
            return Resolved::Local(place, constant);
        }
        if let Some((up, constant)) = self.resolve_upvalue(fi, key) {
            return Resolved::Upvalue(up, constant);
        }
        if !key.starts_with('$') && !self.global_declared(key) {
            if let Some(&b) = self.builtins.get(key) {
                return Resolved::Builtin(b);
            }
        }
        // It may be declared later: the VM checks when it's used
        Resolved::Global(self.globals.slot(key))
    }

    /// Whether a name can be seen from here, so it can't be declared again.
    fn visible(&self, key: &str) -> bool {
        self.functions.iter().any(|f| Self::find_local(f, key).is_some())
            || self.global_declared(key)
            || (!key.starts_with('$') && self.builtins.contains_key(key))
    }

    /// Declares a name, failing with `taken` when it already exists. The value
    /// is stored with `define`.
    fn declare(&mut self, key: &str, constant: bool, taken: &str) -> Target {
        if self.at_global_scope() {
            if !key.starts_with('$') && self.builtins.contains_key(key) {
                self.fail(taken);
            }
            self.declared.insert(key.to_string());
            return Target::Global(self.globals.slot(key), constant);
        }
        if self.visible(key) {
            self.fail(taken);
        }
        let place = self.new_place(key);
        self.f().locals.push(Local { key: key.to_string(), place, constant });
        Target::Local(place)
    }

    /// Pops the value on top into a name just declared.
    fn define(&mut self, target: Target) {
        match target {
            Target::Global(global, constant) => self.emit(Op::DefineGlobal { global, constant }),
            Target::Local(Place::Slot(s)) => self.emit(Op::StoreLocal(s)),
            Target::Local(Place::Cell(c)) => self.emit(Op::DeclareCell(c)),
        };
    }

    fn load(&mut self, resolved: Resolved) {
        match resolved {
            Resolved::Local(Place::Slot(s), _) => self.emit(Op::LoadLocal(s)),
            Resolved::Local(Place::Cell(c), _) => self.emit(Op::LoadCell(c)),
            Resolved::Upvalue(u, _) => self.emit(Op::LoadUpvalue(u)),
            Resolved::Global(g) => self.emit(Op::LoadGlobal(g)),
            Resolved::Builtin(b) => self.emit(Op::LoadBuiltin(b)),
        };
    }

    /// Pops the value on top into a variable that already exists.
    fn store(&mut self, resolved: Resolved, name: &str) {
        match resolved {
            Resolved::Local(_, true) | Resolved::Upvalue(_, true) => {
                self.emit(Op::Pop);
                self.fail(format!("`${name}` is a constant, it can't be changed"));
            }
            Resolved::Local(Place::Slot(s), _) => {
                self.emit(Op::StoreLocal(s));
            }
            Resolved::Local(Place::Cell(c), _) => {
                self.emit(Op::StoreCell(c));
            }
            Resolved::Upvalue(u, _) => {
                self.emit(Op::StoreUpvalue(u));
            }
            Resolved::Global(g) => {
                self.emit(Op::StoreGlobal(g));
            }
            Resolved::Builtin(_) => unreachable!("variables are never built-in"),
        }
    }

    // ---- Statements ----

    fn stmt(&mut self, stmt: &Stmt) -> Compiled {
        self.check_stack(stmt.span)?;
        self.span = stmt.span;
        match &stmt.kind {
            StmtKind::Let { constant, name, value, .. } => {
                let key = format!("${name}");
                let taken = format!("`${name}` already exists. Variables can't be declared twice");
                if !self.at_global_scope() && self.f().captured.contains(&key) {
                    // In a cell before its value, so a closure in the value can use it
                    let target = self.declare(&key, *constant, &taken);
                    self.emit(Op::Null);
                    self.define(target);
                    self.expr(value)?;
                    let Target::Local(Place::Cell(c)) = target else { unreachable!() };
                    self.emit(Op::StoreCell(c));
                } else {
                    self.expr(value)?;
                    self.span = stmt.span;
                    let target = self.declare(&key, *constant, &taken);
                    self.define(target);
                }
            }
            StmtKind::LetList { constant, names, rest, value } => {
                self.expr(value)?;
                self.span = stmt.span;
                self.destructure(names, rest, *constant)?;
            }
            StmtKind::Function(decl) if self.f().hoisted.remove(&decl.name) => {
                let proto = self.function(decl)?;
                self.emit(Op::Closure(proto));
                match self.resolve(&decl.name) {
                    Resolved::Local(Place::Cell(c), _) => self.emit(Op::StoreCell(c)),
                    Resolved::Local(Place::Slot(s), _) => self.emit(Op::StoreLocal(s)),
                    _ => unreachable!("hoisted functions are locals"),
                };
            }
            StmtKind::Class(decl) => self.class(decl)?,
            StmtKind::Function(decl) => {
                let taken = format!("a function named `{}` already exists", decl.name);
                let target = self.declare(&decl.name, false, &taken);
                if let Target::Local(Place::Cell(c)) = target {
                    // The cell exists before the closure, so the function can call itself
                    self.emit(Op::Null);
                    self.emit(Op::DeclareCell(c));
                    let proto = self.function(decl)?;
                    self.emit(Op::Closure(proto));
                    self.emit(Op::StoreCell(c));
                } else {
                    let proto = self.function(decl)?;
                    self.emit(Op::Closure(proto));
                    self.define(target);
                }
            }
            StmtKind::Pub(inner) => {
                match &inner.kind {
                    StmtKind::Function(decl) => {
                        self.public.insert(decl.name.clone());
                    }
                    StmtKind::Let { name, .. } => {
                        self.public.insert(format!("${name}"));
                    }
                    StmtKind::Class(decl) => {
                        self.public.insert(decl.name.clone());
                    }
                    _ => {}
                }
                self.stmt(inner)?;
            }
            StmtKind::If { cond, then, otherwise } => {
                let to_else = self.condition(cond, 0)?;
                self.scoped(then)?;
                match otherwise {
                    Some(otherwise) => {
                        let to_end = self.emit(Op::Jump(0));
                        self.patch(to_else);
                        self.scoped(otherwise)?;
                        self.patch(to_end);
                    }
                    None => self.patch(to_else),
                }
            }
            StmtKind::While { cond, body } => {
                self.open_scope();
                let sp_slot = self.hidden_slot();
                self.emit(Op::SaveSp(sp_slot));
                let start = self.here() as usize;
                let exit = self.condition(cond, 1)?;
                self.push_loop(start, sp_slot);
                self.scoped(body)?;
                self.emit(Op::Jump(start as u32));
                self.patch(exit);
                self.pop_loop();
                self.close_scope();
            }
            StmtKind::For { pattern, iterable, body, .. } => self.for_loop(pattern, iterable, body)?,
            StmtKind::Break | StmtKind::Continue => {
                let is_break = matches!(stmt.kind, StmtKind::Break);
                let Some(lp) = self.f().loops.last() else {
                    let what = if is_break { "break" } else { "continue" };
                    return Err(Diagnostic::new(format!("`{what}` outside a loop"), stmt.span));
                };
                let (handlers, expr_blocks, sp_slot, continue_to) = (lp.handlers, lp.expr_blocks, lp.sp_slot, lp.continue_to);
                self.leave_handlers(handlers)?;
                self.span = stmt.span;
                if self.f().expr_blocks > expr_blocks {
                    self.emit(Op::RestoreSp(sp_slot));
                }
                if is_break {
                    let jump = self.emit(Op::Jump(0));
                    self.f().loops.last_mut().unwrap().breaks.push(jump);
                } else {
                    self.emit(Op::Jump(continue_to as u32));
                }
            }
            StmtKind::Return(value) => {
                if self.f().top_level {
                    return Err(Diagnostic::new("`return` outside a function", stmt.span));
                }
                match value {
                    Some(value) => self.expr(value)?,
                    None => {
                        self.emit(Op::Null);
                    }
                }
                self.leave_handlers(0)?;
                self.emit_at(Op::Return, stmt.span);
            }
            StmtKind::Yield(value) => {
                let Some(block) = self.f().blocks.last() else {
                    return Err(Diagnostic::new(YIELD_OUTSIDE, stmt.span));
                };
                let handlers = block.handlers;
                self.expr(value)?;
                self.leave_handlers(handlers)?;
                let jump = self.emit(Op::Jump(0));
                self.f().blocks.last_mut().unwrap().yields.push(jump);
            }
            StmtKind::Switch(switch) => self.switch(switch, false)?,
            StmtKind::Try { body, catches, finally } => self.try_stmt(body, catches, finally.as_ref(), stmt.span)?,
            StmtKind::Throw(value) => {
                self.expr(value)?;
                self.emit_at(Op::Throw, stmt.span);
            }
            StmtKind::Assert { cond, text } => self.stmt(&assertion(cond, text, stmt.span))?,
            StmtKind::Use(decl) => self.use_decl(decl)?,
            StmtKind::Block(stmts) => self.block(stmts)?,
            StmtKind::Expr(expr) => self.expr_discard(expr)?,
        }
        Ok(())
    }

    /// Makes the places of a block's functions before anything in it runs,
    /// so functions declared further down can be called from the ones above.
    /// Only for locals: globals are already found when they're used.
    fn hoist(&mut self, stmts: &[Stmt]) {
        if self.at_global_scope() {
            return;
        }
        for stmt in stmts {
            let name = match &stmt.kind {
                StmtKind::Function(decl) => &decl.name,
                StmtKind::Class(decl) => &decl.name,
                _ => continue,
            };
            {
                let decl_name = name.clone();
                self.span = stmt.span;
                let taken = format!("`{decl_name}` already exists");
                let target = self.declare(&decl_name, false, &taken);
                // Empty until the declaration runs
                self.emit(Op::Null);
                self.define(target);
                self.f().hoisted.insert(decl_name);
            }
        }
    }

    fn block(&mut self, stmts: &[Stmt]) -> Compiled {
        self.open_scope();
        self.hoist(stmts);
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        self.close_scope();
        Ok(())
    }

    /// A body in its own scope, so declarations inside it stay inside.
    fn scoped(&mut self, stmt: &Stmt) -> Compiled {
        match &stmt.kind {
            StmtKind::Block(stmts) => self.block(stmts),
            _ => {
                self.open_scope();
                self.stmt(stmt)?;
                self.close_scope();
                Ok(())
            }
        }
    }

    fn push_loop(&mut self, continue_to: usize, sp_slot: u32) {
        let f = self.f();
        let lp = Loop { breaks: Vec::new(), continue_to, handlers: f.handlers.len(), expr_blocks: f.expr_blocks, sp_slot };
        f.loops.push(lp);
    }

    fn pop_loop(&mut self) {
        let lp = self.f().loops.pop().expect("a loop is open");
        for b in lp.breaks {
            self.patch(b);
        }
    }

    /// Leaves the `try`s above `down_to`, running their `finally`s, before a
    /// `break`, `continue`, `return` or `yield`.
    fn leave_handlers(&mut self, down_to: usize) -> Compiled {
        let saved = self.f().handlers.clone();
        for i in (down_to..saved.len()).rev() {
            self.emit(Op::PopHandler);
            if let Handler::Finally(stmts) = &saved[i] {
                self.f().handlers.truncate(i);
                self.block(stmts)?;
            }
        }
        self.f().handlers = saved;
        Ok(())
    }

    /// Declares the names of `[$a, $b, ...$rest]` from the list on top.
    fn destructure(&mut self, names: &[String], rest: &Option<String>, constant: bool) -> Compiled {
        self.emit(Op::Destructure { names: names.len() as u32, rest: rest.is_some() });
        let mut targets = Vec::new();
        for name in names.iter().chain(rest.iter()) {
            let taken = format!("`${name}` already exists. Variables can't be declared twice");
            targets.push(self.declare(&format!("${name}"), constant, &taken));
        }
        for target in targets.into_iter().rev() {
            self.define(target);
        }
        Ok(())
    }

    fn for_loop(&mut self, pattern: &ForPattern, iterable: &Expr, body: &Stmt) -> Compiled {
        let span = self.span;
        self.expr(iterable)?;
        self.open_scope();
        let slot = self.hidden_slot();
        self.hidden_slot();
        self.hidden_slot();
        let sp_slot = self.hidden_slot();
        let kind = match pattern {
            ForPattern::Value(_) => for_kind::VALUE,
            ForPattern::Indexed { .. } | ForPattern::IndexedList { .. } => for_kind::INDEXED,
            ForPattern::KeyValue { .. } | ForPattern::KeyValueList { .. } => for_kind::KEY_VALUE,
            ForPattern::List { .. } => for_kind::LIST,
        };
        self.emit_at(Op::ForPrepare { slot, kind }, iterable.span);
        self.emit(Op::SaveSp(sp_slot));
        let next = self.emit_at(Op::ForNext { slot, exit: 0, kind }, span);
        self.push_loop(next, sp_slot);
        self.open_scope();
        match pattern {
            ForPattern::Value(name) => {
                let taken = format!("`${name}` already exists. Variables can't be declared twice");
                let target = self.declare(&format!("${name}"), false, &taken);
                self.define(target);
            }
            ForPattern::Indexed { index: a, value: b } | ForPattern::KeyValue { key: a, value: b } => {
                if a == b {
                    self.fail(format!("`${a}` is used twice in the `for`"));
                }
                let taken = "a variable of the `for` already exists outside it";
                let first = self.declare(&format!("${a}"), false, taken);
                let second = self.declare(&format!("${b}"), false, taken);
                self.define(second);
                self.define(first);
            }
            ForPattern::List { names, rest } => self.destructure(names, rest, false)?,
            // The index or key is below the element: the element is destructured first
            ForPattern::IndexedList { index: first, names, rest } | ForPattern::KeyValueList { key: first, names, rest } => {
                if names.iter().chain(rest.iter()).any(|n| n == first) {
                    self.fail(format!("`${first}` is used twice in the `for`"));
                }
                let taken = "a variable of the `for` already exists outside it";
                let index = self.declare(&format!("${first}"), false, taken);
                self.destructure(names, rest, false)?;
                self.define(index);
            }
        }
        self.scoped(body)?;
        self.close_scope();
        self.emit(Op::Jump(next as u32));
        self.patch(next);
        self.pop_loop();
        self.close_scope();
        Ok(())
    }

    fn try_stmt(&mut self, body: &[Stmt], catches: &[Catch], finally: Option<&Vec<Stmt>>, span: Span) -> Compiled {
        let finally = finally.map(|f| Rc::new(f.clone()));
        self.open_scope();
        let mut push_finally = None;
        let mut pending = 0;
        if let Some(stmts) = &finally {
            pending = self.hidden_slot();
            push_finally = Some(self.emit(Op::PushFinally { target: 0, slot: pending }));
            self.f().handlers.push(Handler::Finally(stmts.clone()));
        }
        let mut to_after = Vec::new();
        if catches.is_empty() {
            self.block(body)?;
        } else {
            let push_catch = self.emit(Op::PushCatch(0));
            self.f().handlers.push(Handler::Catch);
            self.block(body)?;
            self.f().handlers.pop();
            self.emit(Op::PopHandler);
            to_after.push(self.emit(Op::Jump(0)));
            // The exception is on top of the stack
            self.patch(push_catch);
            for catch in catches {
                let mut to_next = None;
                // `catch ($e)` catches every `Exception`, but not an `Interrupted`, which is beside it
                let every = TypeExpr { kind: TypeKind::Named { name: "Exception".into(), args: Vec::new() }, span };
                if let Some(ty) = catch.ty.as_ref().or(Some(&every)) {
                    self.emit(Op::Dup);
                    let classes = self.type_classes(ty);
                    let t = self.ty(ty);
                    self.emit_at(Op::Is { ty: t, catch: true, classes }, ty.span);
                    to_next = Some(self.emit(Op::JumpIfFalseRaw(0)));
                }
                self.open_scope();
                self.span = span;
                let taken = format!("`${}` already exists. Variables can't be declared twice", catch.name);
                let target = self.declare(&format!("${}", catch.name), false, &taken);
                self.define(target);
                for stmt in &catch.body {
                    self.stmt(stmt)?;
                }
                self.close_scope();
                to_after.push(self.emit(Op::Jump(0)));
                if let Some(j) = to_next {
                    self.patch(j);
                }
            }
            self.emit(Op::Rethrow);
        }
        for j in to_after {
            self.patch(j);
        }
        if let (Some(stmts), Some(push)) = (finally, push_finally) {
            self.f().handlers.pop();
            self.emit(Op::PopHandler);
            self.block(&stmts)?;
            let to_end = self.emit(Op::Jump(0));
            // Reached with an exception or `exit()` waiting in the slot
            self.patch(push);
            self.block(&stmts)?;
            self.emit(Op::EndFinally(pending));
            self.patch(to_end);
        }
        self.close_scope();
        Ok(())
    }

    fn switch(&mut self, switch: &Switch, as_value: bool) -> Compiled {
        self.open_scope();
        let subject = self.hidden_slot();
        self.expr(&switch.subject)?;
        self.emit(Op::StoreLocal(subject));
        let mut to_end = Vec::new();
        for case in &switch.cases {
            let mut to_body = Vec::new();
            for pattern in &case.patterns {
                match pattern {
                    Pattern::Value(expr) => {
                        self.expr(expr)?;
                        self.emit(Op::CaseMatches(subject));
                    }
                    Pattern::Type(ty) => {
                        self.emit(Op::LoadLocal(subject));
                        let classes = self.type_classes(ty);
                        let t = self.ty(ty);
                        self.emit_at(Op::Is { ty: t, catch: false, classes }, ty.span);
                    }
                }
                to_body.push(self.emit(Op::JumpIfTrueRaw(0)));
            }
            let to_next = self.emit(Op::Jump(0));
            for j in to_body {
                self.patch(j);
            }
            self.case_body(&case.body, as_value)?;
            to_end.push(self.emit(Op::Jump(0)));
            self.patch(to_next);
        }
        match &switch.default {
            Some(body) => self.case_body(body, as_value)?,
            None if as_value => {
                self.emit_at(Op::NoCase(subject), switch.subject.span);
            }
            None => {}
        }
        for j in to_end {
            self.patch(j);
        }
        self.close_scope();
        Ok(())
    }

    fn case_body(&mut self, body: &CaseBody, as_value: bool) -> Compiled {
        match body {
            CaseBody::Value(branch) => {
                self.branch(branch)?;
                if !as_value {
                    self.emit(Op::Pop);
                }
            }
            CaseBody::Stmt(stmt) => {
                self.scoped(stmt)?;
                if as_value {
                    self.emit(Op::Null);
                }
            }
        }
        Ok(())
    }

    fn use_decl(&mut self, decl: &UseDecl) -> Compiled {
        let import = Import { path: decl.path.clone(), items: Vec::new() };
        match &decl.items {
            UseItems::Module { alias } => {
                let name = alias.clone().unwrap_or_else(|| decl.path.last().cloned().unwrap_or_default());
                let imported = self.globals.find_module(&name).is_some_and(|m| self.globals.modules.borrow()[m as usize].is_some());
                if imported || !self.declared.insert(format!("::{name}")) {
                    self.fail(format!("a module named `{name}` is already imported"));
                }
                let module = self.globals.module_slot(&name);
                let proto = &mut self.f().proto;
                proto.imports.push(import);
                let path = proto.imports.len() as u32 - 1;
                self.emit(Op::UseModule { module, path });
            }
            UseItems::Items(items) => {
                let mut import = import;
                for item in items {
                    let local = item.alias.clone().unwrap_or_else(|| item.name.clone());
                    let key = if item.variable { format!("${local}") } else { local.clone() };
                    let global = self.globals.slot(&key);
                    self.declared.insert(key.clone());
                    import.items.push((item.name.clone(), item.variable, global, key));
                }
                let proto = &mut self.f().proto;
                proto.imports.push(import);
                let n = proto.imports.len() as u32 - 1;
                self.emit(Op::UseItems(n));
            }
        }
        Ok(())
    }

    // ---- Functions ----

    /// Compiles a function or a lambda into the current function's table.
    fn function(&mut self, decl: &FunctionDecl) -> Compiled<u32> {
        let span = self.span;
        if decl.params.len() > 64 {
            return Err(Diagnostic::new("a function can't have more than 64 parameters", span));
        }
        let mut captured = HashSet::new();
        collect_function(decl, false, &mut captured);
        // Lambdas have no name: they're named by the function they're in
        let shown = match self.functions.last() {
            Some(outer) if decl.name == "anonymous" => format!("a lambda in {}", outer.proto.shown),
            _ => format!("{}{}", self.prefix, decl.name),
        };
        self.functions.push(FnState::new(&decl.name, captured, false));
        self.f().proto.shown = Rc::from(shown);
        let n = decl.params.len() as u32;
        self.f().next_slot = n;
        self.f().proto.slots = n;
        for (i, param) in decl.params.iter().enumerate() {
            let i = i as u32;
            // `@` and the parameters written as fields can't be named by the code
            let hidden = param.name == "@" || param.field;
            let key = if param.field { format!("$@{}", param.name) } else { format!("${}", param.name) };
            if !hidden && self.visible(&key) {
                self.fail(format!(
                    "the parameter `${}` of `{}` has the name of a variable that already exists",
                    param.name, decl.name
                ));
            }
            if let Some(default) = &param.default {
                let skip = self.emit(Op::JumpIfGiven { param: i, target: 0 });
                // Evaluated on every call, seeing the parameters before it
                self.expr(default)?;
                self.emit(Op::StoreLocal(i));
                self.patch(skip);
                self.span = span;
            }
            let place = if self.f().captured.contains(&key) {
                let f = self.f();
                let cell = f.next_cell;
                f.next_cell += 1;
                f.proto.cells = f.proto.cells.max(f.next_cell);
                self.emit(Op::SlotToCell { slot: i, cell });
                Place::Cell(cell)
            } else {
                Place::Slot(i)
            };
            self.f().locals.push(Local { key, place, constant: false });
            self.f().proto.params.push(ParamInfo {
                name: Rc::from(param.name.as_str()),
                has_default: param.default.is_some(),
                variadic: param.variadic,
            });
        }
        self.f().proto.simple = !decl.params.iter().any(|p| p.variadic);
        // `new(@name)`: the parameter goes into the field
        for param in decl.params.iter().filter(|p| p.field) {
            self.load_this();
            let resolved = self.resolve(&format!("$@{}", param.name));
            self.load(resolved);
            let n = self.name(&param.name);
            self.emit(Op::SetField(n));
            self.emit(Op::Pop);
        }
        match &decl.body {
            FunctionBody::Expr(expr) => {
                self.expr(expr)?;
                self.emit(Op::Return);
            }
            FunctionBody::Block(stmts) => {
                self.hoist(stmts);
                for stmt in stmts {
                    self.stmt(stmt)?;
                }
                self.emit(Op::Null);
                self.emit(Op::Return);
            }
        }
        self.span = span;
        let proto = Rc::new(self.finish());
        let protos = &mut self.f().proto.protos;
        protos.push(proto);
        Ok(protos.len() as u32 - 1)
    }

    // ---- Classes ----

    fn load_this(&mut self) {
        if self.visible("$@") {
            let resolved = self.resolve("$@");
            self.load(resolved);
        } else {
            self.fail("`@` can only be used in the methods and the constructor of a class");
            self.emit(Op::Null);
        }
    }

    /// Pushes a class by its name. `Self` is the class being compiled, and
    /// `Git::Commit` a subcommand of the command `Git`.
    fn load_class(&mut self, name: &str) {
        if let Some((outer, member)) = name.rsplit_once("::").filter(|(outer, _)| crate::parser::is_type_name(outer.split("::").next().unwrap_or(""))) {
            self.load_class(outer);
            let n = self.name(member);
            self.emit(Op::Static { name: n, variable: false });
            return;
        }
        let name = if name == "Self" {
            match self.classes.last() {
                Some(c) => c.clone(),
                None => {
                    self.fail("`Self` can only be used inside a class");
                    self.emit(Op::Null);
                    return;
                }
            }
        } else {
            name.to_string()
        };
        let resolved = self.resolve(&name);
        self.load(resolved);
    }

    /// Pushes the class or interface a type names *(i.e. `Base`, `shapes::Point`)*.
    fn load_type(&mut self, ty: &TypeExpr) {
        let TypeKind::Named { name, .. } = &ty.kind else {
            self.fail("expected a class or an interface");
            self.emit(Op::Null);
            return;
        };
        match name.split_once("::").filter(|(m, _)| !crate::parser::is_type_name(m)) {
            Some((module, member)) => match self.globals.find_module(module) {
                Some(m) => {
                    let n = self.name(member);
                    self.emit(Op::ModuleMember { module: m, name: n, constant: false });
                }
                None => {
                    self.fail(format!("there's no module named `{module}`. Import it with `use`"));
                    self.emit(Op::Null);
                }
            },
            None => self.load_class(name),
        }
    }

    /// Pushes the classes a type names, for `Is` and `As`. Returns how many.
    fn type_classes(&mut self, ty: &TypeExpr) -> u8 {
        let names: Vec<String> = class_names(ty).into_iter().map(String::from).collect();
        if names.len() > u8::MAX as usize {
            self.fail("a type can't name more than 255 classes");
            return 0;
        }
        for name in &names {
            if let Some((module, member)) = name.split_once("::").filter(|(m, _)| !crate::parser::is_type_name(m)) {
                // A class of an imported module
                match self.globals.find_module(module) {
                    Some(m) => {
                        let n = self.name(member);
                        self.emit(Op::ModuleMember { module: m, name: n, constant: false });
                    }
                    None => {
                        self.fail(format!("there's no module named `{module}`. Import it with `use`"));
                        self.emit(Op::Null);
                    }
                }
                continue;
            }
            if name.contains("::") {
                // A subcommand *(i.e. `Git::Commit`)*
                self.load_class(name);
                continue;
            }
            match self.resolve(name) {
                // An unknown name is `null`, and `Is` says it's an unknown type
                Resolved::Global(g) if name != "Self" => {
                    self.emit(Op::LoadGlobalOrNull(g));
                }
                _ => self.load_class(name),
            }
        }
        names.len() as u8
    }

    /// A method or constructor: a function whose first parameter is `@`.
    fn method(&mut self, decl: &FunctionDecl) -> Compiled<u32> {
        let mut with_this = decl.clone();
        with_this.params.insert(0, Param { name: "@".into(), ty: None, default: None, variadic: false, field: false });
        self.function(&with_this)
    }

    /// A class declaration: its parts are pushed, `Op::Class` makes it, and
    /// then its static fields get their values.
    fn class(&mut self, decl: &ClassDecl) -> Compiled {
        let interface = decl.kind == ClassKind::Interface;
        let span = self.span;
        // A subcommand *(i.e. `Git::Commit`)* is kept in its command, not in a variable
        let nested = decl.name.rsplit_once("::").map(|(outer, member)| (outer.to_string(), member.to_string()));
        let hoisted = nested.is_none() && self.f().hoisted.remove(&decl.name);
        let target = if hoisted || nested.is_some() {
            None
        } else {
            let taken = format!("`{}` already exists", decl.name);
            Some(self.declare(&decl.name, true, &taken))
        };
        if let Some((outer, _)) = &nested {
            self.load_class(outer);
        }
        // While the methods are compiled, `Self` is this class
        self.classes.push(decl.name.clone());
        let mut info = ClassInfo {
            name: Rc::from(decl.name.as_str()),
            interface,
            is_enum: decl.kind == ClassKind::Enum,
            parent: false,
            interfaces: 0,
            required: Vec::new(),
            init: false,
            constructor: false,
            fields: Vec::new(),
            field_types: Vec::new(),
            methods: Vec::new(),
            static_methods: Vec::new(),
            command: None,
        };
        if let (false, Some(parent)) = (interface, decl.extends.first()) {
            self.load_type(parent);
            info.parent = true;
        }
        // The interfaces it implements, or the ones an interface extends
        let mut implemented: Vec<&TypeExpr> = Vec::new();
        if interface {
            implemented.extend(decl.extends.iter());
        }
        implemented.extend(decl.implements.iter());
        for ty in &implemented {
            self.load_type(ty);
        }
        info.interfaces = implemented.len() as u32;
        if decl.kind == ClassKind::Command {
            info.command = Some(Rc::new(self.command_info(decl)));
        }
        // `init`: the values written where the fields are declared
        let valued: Vec<&Member> = decl
            .members
            .iter()
            .filter(|m| !m.is_static && matches!(&m.kind, MemberKind::Field { value: Some(_), .. }))
            .collect();
        if !valued.is_empty() {
            let mut stmts = Vec::new();
            for member in &valued {
                let MemberKind::Field { name, value: Some(value), .. } = &member.kind else { unreachable!() };
                let this = Expr { kind: ExprKind::This, span: member.span };
                let field = Expr { kind: ExprKind::Field { target: Box::new(this), name: name.clone(), nullable: false }, span: member.span };
                let assign = ExprKind::Assign { op: None, target: Box::new(field), value: Box::new(value.clone()) };
                stmts.push(Stmt { kind: StmtKind::Expr(Expr { kind: assign, span: member.span }), span: member.span });
            }
            let init = FunctionDecl { name: format!("{}.init", decl.name), generics: Vec::new(), params: Vec::new(), ret: None, body: FunctionBody::Block(stmts) };
            let proto = self.method(&init)?;
            self.emit(Op::Closure(proto));
            info.init = true;
        }
        for member in &decl.members {
            match &member.kind {
                MemberKind::Field { constant, ty, name, value, .. } if !member.is_static => {
                    let nullable = matches!(ty.kind, TypeKind::Nullable(_));
                    info.fields.push((Rc::from(name.as_str()), *constant, nullable, value.is_some()));
                    info.field_types.push(Some(Rc::new(ty.clone())));
                }
                MemberKind::Constructor(ctor) => {
                    if info.constructor {
                        self.fail(format!("`{}` has two constructors", decl.name));
                        continue;
                    }
                    let mut ctor = ctor.clone();
                    ctor.name = format!("{}.new", decl.name);
                    self.span = member.span;
                    let proto = self.method(&ctor)?;
                    self.emit(Op::Closure(proto));
                    info.constructor = true;
                }
                _ => {}
            }
        }
        if !decl.subcommands.is_empty() {
            // The subcommand that was given, set by `parse()`
            info.fields.push((Rc::from("subcommand"), true, decl.optional_subcommand, false));
            info.field_types.push(None);
        }
        // Methods of several `implement` blocks with the same name *(i.e. `mul`
        // of `IMul<number>` and `IMul<Vector>`)*: each one is a method of its
        // own, and the method with that name picks one by the arguments' types
        let mut methods: Vec<(&Member, FunctionDecl)> = Vec::new();
        let mut versions: HashMap<String, Vec<(Vec<Option<TypeExpr>>, String)>> = HashMap::new();
        for member in &decl.members {
            if let MemberKind::Method { decl: method, body: true } = &member.kind {
                if member.is_static {
                    continue;
                }
                let same = decl.members.iter().filter(|m| {
                    !m.is_static && matches!(&m.kind, MemberKind::Method { decl: d, body: true } if d.name == method.name)
                }).count();
                let mut method = method.clone();
                if same > 1 && matches!(member.origin, Origin::Implement(_)) {
                    let list = versions.entry(method.name.clone()).or_default();
                    let hidden = format!("{}#{}", method.name, list.len());
                    list.push((method.params.iter().map(|p| p.ty.clone()).collect(), hidden.clone()));
                    method.name = hidden;
                }
                methods.push((member, method));
            }
        }
        let mut dispatchers: Vec<FunctionDecl> = Vec::new();
        let mut names: Vec<&String> = versions.keys().collect();
        names.sort();
        for name in names {
            dispatchers.push(dispatcher(name, &versions[name], &decl.name, &decl.generics, decl.span));
        }
        for member in &decl.members {
            if let MemberKind::Method { decl: method, body: false } = &member.kind {
                if !member.is_static {
                    info.required.push(Rc::from(method.name.as_str()));
                }
            }
        }
        let synthetic: Vec<(Option<&Member>, FunctionDecl)> = methods
            .into_iter()
            .map(|(m, d)| (Some(m), d))
            .chain(dispatchers.into_iter().map(|d| (None, d)))
            .collect();
        for (member, method) in &synthetic {
            {
                let span = member.map(|m| m.span).unwrap_or(decl.span);
                if info.methods.iter().any(|m| **m == *method.name) {
                    self.fail(format!("`{}` has two methods named `{}`", decl.name, method.name));
                    continue;
                }
                self.span = span;
                let mut named = method.clone();
                named.name = format!("{}.{}", decl.name, method.name);
                if member.is_some_and(|m| matches!(m.origin, Origin::Implement(_))) {
                    self_checks(&mut named, &decl.name);
                }
                let proto = self.method(&named)?;
                self.emit(Op::Closure(proto));
                info.methods.push(Rc::from(method.name.as_str()));
            }
        }
        for member in &decl.members {
            if let MemberKind::Method { decl: method, .. } = &member.kind {
                if !member.is_static {
                    continue;
                }
                self.span = member.span;
                let mut named = method.clone();
                named.name = format!("{}::{}", decl.name, method.name);
                let proto = self.function(&named)?;
                self.emit(Op::Closure(proto));
                info.static_methods.push(Rc::from(method.name.as_str()));
            }
        }
        self.span = span;
        let classes = &mut self.f().proto.classes;
        classes.push(info);
        let index = classes.len() as u32 - 1;
        self.emit(Op::Class(index));
        match target {
            Some(target) => self.define(target),
            None if nested.is_some() => {
                let n = self.name(&nested.as_ref().unwrap().1);
                self.emit(Op::DefineMember(n));
            }
            None => match self.resolve(&decl.name) {
                Resolved::Local(Place::Cell(c), _) => {
                    self.emit(Op::StoreCell(c));
                }
                Resolved::Local(Place::Slot(s), _) => {
                    self.emit(Op::StoreLocal(s));
                }
                _ => unreachable!("hoisted classes are locals"),
            },
        }
        // The values of an enum, created once, in order
        for value in &decl.values {
            self.span = value.span;
            self.load_class(&decl.name);
            let n = self.name(&value.name);
            match self.args(&value.args)? {
                None => self.emit(Op::EnumValue { name: n, argc: value.args.len() as u32 }),
                Some(call) => self.emit(Op::EnumValueComplex { name: n, call }),
            };
        }
        // Static fields, in order, so one can use the ones before it
        for member in &decl.members {
            if let MemberKind::Field { constant, name, value, .. } = &member.kind {
                if !member.is_static {
                    continue;
                }
                self.span = member.span;
                self.load_class(&decl.name);
                match value {
                    Some(v) => self.expr(v)?,
                    None => {
                        self.emit(Op::Null);
                    }
                }
                let n = self.name(name);
                self.emit(Op::DefineStatic { name: n, constant: *constant });
            }
        }
        self.classes.pop();
        for sub in &decl.subcommands {
            self.class(sub)?;
        }
        self.span = span;
        Ok(())
    }

    /// What `Git::parse()` needs to know of a command's fields. The enums
    /// their types name are pushed, for `Op::Class`.
    fn command_info(&mut self, decl: &ClassDecl) -> CommandInfo {
        let (info, enums) = command_info(decl);
        for ty in enums {
            self.load_type(ty);
        }
        info
    }

    // ---- Expressions ----

    /// An expression whose value isn't used.
    fn expr_discard(&mut self, expr: &Expr) -> Compiled {
        match &expr.kind {
            ExprKind::Assign { op, target, value } => self.assign(*op, target, value, false, expr.span),
            ExprKind::Increment { target, delta, prefix } => self.increment(target, *delta, *prefix, false, expr.span),
            _ => {
                self.expr(expr)?;
                self.emit(Op::Pop);
                Ok(())
            }
        }
    }

    fn expr(&mut self, expr: &Expr) -> Compiled {
        self.check_stack(expr.span)?;
        let saved = self.span;
        self.span = expr.span;
        self.expr_inner(expr)?;
        self.span = saved;
        Ok(())
    }

    fn expr_inner(&mut self, expr: &Expr) -> Compiled {
        let span = expr.span;
        match &expr.kind {
            ExprKind::Int(n) => {
                match i32::try_from(*n) {
                    Ok(small) => self.emit(Op::Int(small)),
                    Err(_) => {
                        let c = self.constant(Value::Int(*n));
                        self.emit(Op::Const(c))
                    }
                };
            }
            ExprKind::Float(n) => {
                let c = self.constant(Value::Float(*n));
                self.emit(Op::Const(c));
            }
            ExprKind::Str(s) => {
                let c = self.constant(Value::string(s));
                self.emit(Op::Const(c));
            }
            ExprKind::Bool(true) => {
                self.emit(Op::True);
            }
            ExprKind::Bool(false) => {
                self.emit(Op::False);
            }
            ExprKind::Null => {
                self.emit(Op::Null);
            }
            ExprKind::Interpolated(parts) => {
                for part in parts {
                    match part {
                        StringPart::Text(t) => {
                            let c = self.constant(Value::string(t));
                            self.emit(Op::Const(c));
                        }
                        StringPart::Expr(e) => self.expr(e)?,
                    }
                }
                self.emit(Op::Interpolate(parts.len() as u32));
            }
            ExprKind::Type(ty) => {
                let t = self.ty(ty);
                self.emit(Op::TypeValue(t));
            }
            ExprKind::Bytes(bytes) => {
                let c = self.constant(Value::Bytes(Rc::from(bytes.as_slice())));
                self.emit(Op::Const(c));
            }
            ExprKind::Regex { pattern, flags } => {
                let regexes = &mut self.f().proto.regexes;
                regexes.push((pattern.clone(), flags.clone()));
                let r = regexes.len() as u32 - 1;
                self.emit(Op::Regex(r));
            }
            ExprKind::Command(parts) => {
                for part in parts {
                    match part {
                        StringPart::Text(t) => {
                            let c = self.constant(Value::string(t));
                            self.emit(Op::Const(c));
                        }
                        StringPart::Expr(e) => {
                            self.expr(e)?;
                            self.emit_at(Op::QuoteArgument, e.span);
                        }
                    }
                }
                self.emit(Op::Command(parts.len() as u32));
            }
            ExprKind::List { items, .. } => {
                for item in items {
                    match &item.kind {
                        ExprKind::Spread(inner) => self.expr(inner)?,
                        _ => self.expr(item)?,
                    }
                }
                if items.iter().any(|i| matches!(i.kind, ExprKind::Spread(_))) {
                    let kinds = items.iter().map(|i| if matches!(i.kind, ExprKind::Spread(_)) { ArgKind::Spread(i.span) } else { ArgKind::Positional }).collect();
                    let calls = &mut self.f().proto.calls;
                    calls.push(kinds);
                    let layout = calls.len() as u32 - 1;
                    self.emit(Op::ListSpread(layout));
                } else {
                    self.emit(Op::List(items.len() as u32));
                }
            }
            ExprKind::Spread(inner) => {
                return Err(Diagnostic::new("`...` only goes in calls and in lists *(i.e. `[...$a, ...$b]`)*", inner.span));
            }
            ExprKind::Map { entries, .. } => {
                for (k, v) in entries {
                    self.expr(k)?;
                    self.emit_at(Op::CheckKey, k.span);
                    self.expr(v)?;
                }
                self.emit(Op::Map(entries.len() as u32));
            }
            ExprKind::Variable(name) => {
                let resolved = self.resolve(&format!("${name}"));
                self.load(resolved);
            }
            ExprKind::Function(name) | ExprKind::TypeArgs { name, .. } => {
                let resolved = self.resolve(name);
                self.load(resolved);
            }
            ExprKind::Unary { op: UnaryOp::Neg, operand } if matches!(operand.kind, ExprKind::Int(n) if n > 0) => {
                let ExprKind::Int(n) = operand.kind else { unreachable!() };
                let negative = Expr { kind: ExprKind::Int(-n), span };
                self.expr_inner(&negative)?;
            }
            ExprKind::Unary { op, operand } => {
                self.expr(operand)?;
                self.emit(Op::Unary(*op));
            }
            ExprKind::Binary { op, left, right } => self.binary(*op, left, right)?,
            ExprKind::PathTest { op, path } => {
                self.expr(path)?;
                self.emit_at(Op::PathTest(*op), path.span);
            }
            ExprKind::Range { start, end, inclusive } => {
                let Some(end) = end else {
                    self.fail("a range without an end can only be used as an index *(i.e. `$text[2..]`)*");
                    return Ok(());
                };
                self.expr(start)?;
                self.emit_at(Op::RangeBound, start.span);
                self.expr(end)?;
                self.emit_at(Op::RangeBound, end.span);
                self.emit(Op::Range { inclusive: *inclusive });
            }
            ExprKind::Ternary { cond, then, otherwise } => {
                let to_else = self.condition(cond, 2)?;
                self.expr(then)?;
                let to_end = self.emit(Op::Jump(0));
                self.patch(to_else);
                self.expr(otherwise)?;
                self.patch(to_end);
            }
            ExprKind::Assign { op, target, value } => self.assign(*op, target, value, true, span)?,
            ExprKind::Increment { target, delta, prefix } => self.increment(target, *delta, *prefix, true, span)?,
            ExprKind::Call { callee, args } => {
                self.expr(callee)?;
                match self.args(args)? {
                    None => self.emit(Op::Call(args.len() as u32)),
                    Some(call) => self.emit(Op::CallComplex(call)),
                };
            }
            ExprKind::MethodCall { receiver, method, args, nullable } => {
                self.expr(receiver)?;
                let to_end = nullable.then(|| self.emit(Op::JumpIfNull(0)));
                let name = self.name(method);
                match self.args(args)? {
                    None => self.emit(Op::CallMethod { name, argc: args.len() as u32 }),
                    Some(call) => self.emit(Op::CallMethodComplex { name, call }),
                };
                if let Some(j) = to_end {
                    self.patch(j);
                }
            }
            ExprKind::NonNull(inner) => {
                self.expr(inner)?;
                let written = written(inner).map(|w| Value::string(&w)).unwrap_or(Value::Null);
                let c = self.constant(written);
                self.emit(Op::NonNull(c));
            }
            ExprKind::TypeCheck { receiver, ty, check, nullable } => {
                self.expr(receiver)?;
                let to_end = nullable.then(|| self.emit(Op::JumpIfNull(0)));
                let classes = self.type_classes(ty);
                let t = self.ty(ty);
                match check {
                    TypeCheckKind::Is => self.emit(Op::Is { ty: t, catch: false, classes }),
                    TypeCheckKind::As => self.emit(Op::As { ty: t, classes }),
                };
                if let Some(j) = to_end {
                    self.patch(j);
                }
            }
            ExprKind::Index { target, index, nullable } => {
                self.expr(target)?;
                let to_end = nullable.then(|| self.emit(Op::JumpIfNull(0)));
                if let ExprKind::Range { start, end: None, .. } = &index.kind {
                    // `$list[2..]`: to the end
                    self.expr(start)?;
                    self.emit_at(Op::RangeBound, start.span);
                    self.emit_at(Op::SliceFrom, index.span);
                } else {
                    self.expr(index)?;
                    self.emit_at(Op::Index, index.span);
                }
                if let Some(j) = to_end {
                    self.patch(j);
                }
            }
            ExprKind::Lambda(decl) => {
                let proto = self.function(decl)?;
                self.emit(Op::Closure(proto));
            }
            ExprKind::If { cond, then, otherwise } => {
                let to_else = self.condition(cond, 0)?;
                self.branch(then)?;
                let to_end = self.emit(Op::Jump(0));
                self.patch(to_else);
                self.branch(otherwise)?;
                self.patch(to_end);
            }
            ExprKind::Switch(switch) => self.switch(switch, true)?,
            ExprKind::Throw(value) => {
                self.expr(value)?;
                self.emit_at(Op::Throw, span);
            }
            ExprKind::Try(inner) => {
                let push = self.emit(Op::PushTry(0));
                self.f().handlers.push(Handler::TryExpr);
                self.expr(inner)?;
                self.f().handlers.pop();
                self.emit(Op::PopHandler);
                self.emit(Op::WrapOk);
                let to_end = self.emit(Op::Jump(0));
                self.patch(push);
                self.emit(Op::WrapError);
                self.patch(to_end);
            }
            ExprKind::This => self.load_this(),
            ExprKind::Field { target, name, nullable } => {
                self.expr(target)?;
                let to_end = nullable.then(|| self.emit(Op::JumpIfNull(0)));
                let n = self.name(name);
                self.emit(Op::GetField(n));
                if let Some(j) = to_end {
                    self.patch(j);
                }
            }
            ExprKind::Static { class, name, variable } => {
                self.load_class(class);
                let n = self.name(name);
                self.emit(Op::Static { name: n, variable: *variable });
            }
            ExprKind::SuperNew(args) => {
                let Some(class) = self.classes.last().cloned() else {
                    self.fail("`@super` can only be used inside a class");
                    self.emit(Op::Null);
                    return Ok(());
                };
                self.load_class(&class);
                self.load_this();
                match self.args(args)? {
                    None => self.emit(Op::SuperNew(args.len() as u32)),
                    Some(call) => self.emit(Op::SuperNewComplex(call)),
                };
            }
            ExprKind::SuperMethod { method, args } => {
                let Some(class) = self.classes.last().cloned() else {
                    self.fail("`@super` can only be used inside a class");
                    self.emit(Op::Null);
                    return Ok(());
                };
                if args.iter().any(|a| a.name.is_some() || a.spread) {
                    self.fail("`@super` methods only take plain arguments for now");
                }
                self.load_class(&class);
                self.load_this();
                for arg in args {
                    self.expr(&arg.value)?;
                }
                let n = self.name(method);
                self.emit(Op::SuperMethod { name: n, argc: args.len() as u32 });
            }
            ExprKind::ModuleFunction { module, name } | ExprKind::ModuleConstant { module, name } => {
                let constant = matches!(expr.kind, ExprKind::ModuleConstant { .. });
                match self.globals.find_module(module) {
                    Some(m) => {
                        let n = self.name(name);
                        self.emit(Op::ModuleMember { module: m, name: n, constant });
                    }
                    None => {
                        self.fail(format!("there's no module named `{module}`. Import it with `use`"));
                        self.emit(Op::Null);
                    }
                }
            }
        }
        Ok(())
    }

    /// A condition, and a jump that's taken when it's false. Returns the jump to patch.
    fn condition(&mut self, cond: &Expr, keyword: u8) -> Compiled<usize> {
        if let ExprKind::Binary { op, left, right } = &cond.kind {
            use BinaryOp::*;
            if matches!(op, Eq | NotEq | Lt | Gt | LtEq | GtEq) {
                self.check_stack(cond.span)?;
                self.expr(left)?;
                self.expr(right)?;
                return Ok(self.emit_at(Op::JumpIfNot { op: *op, target: 0 }, cond.span));
            }
        }
        self.expr(cond)?;
        Ok(self.emit_at(Op::JumpIfFalse { target: 0, keyword }, cond.span))
    }

    /// The value of an `if` or `switch` branch.
    fn branch(&mut self, branch: &Branch) -> Compiled {
        match branch {
            Branch::Expr(expr) => self.expr(expr),
            Branch::Block(stmts, span) => {
                let handlers = self.f().handlers.len();
                self.f().blocks.push(ValueBlock { yields: Vec::new(), handlers });
                self.f().expr_blocks += 1;
                self.block(stmts)?;
                self.span = *span;
                self.fail("this block must give its value with `yield`");
                self.f().expr_blocks -= 1;
                let block = self.f().blocks.pop().expect("a block is open");
                for y in block.yields {
                    self.patch(y);
                }
                Ok(())
            }
        }
    }

    fn binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> Compiled {
        match op {
            BinaryOp::And | BinaryOp::Or => {
                let and = op == BinaryOp::And;
                self.expr(left)?;
                let to_end = self.emit_at(Op::JumpLogic { target: 0, and }, left.span);
                self.expr(right)?;
                self.emit_at(Op::CheckBool(and), right.span);
                self.patch(to_end);
            }
            BinaryOp::Coalesce => {
                self.expr(left)?;
                let to_end = self.emit(Op::JumpIfNotNull(0));
                self.expr(right)?;
                self.patch(to_end);
            }
            _ => {
                self.expr(left)?;
                self.expr(right)?;
                let instruction = match op {
                    BinaryOp::Add => Op::Add,
                    BinaryOp::Sub => Op::Sub,
                    BinaryOp::Mul => Op::Mul,
                    BinaryOp::Div => Op::Div,
                    BinaryOp::Rem => Op::Rem,
                    BinaryOp::Eq => Op::Eq,
                    BinaryOp::NotEq => Op::NotEq,
                    BinaryOp::Lt => Op::Lt,
                    BinaryOp::Gt => Op::Gt,
                    BinaryOp::LtEq => Op::LtEq,
                    BinaryOp::GtEq => Op::GtEq,
                    BinaryOp::Like | BinaryOp::NotLike | BinaryOp::Match | BinaryOp::NotMatch => Op::Pattern(op),
                    other => Op::Binary(other),
                };
                self.emit(instruction);
            }
        }
        Ok(())
    }

    /// Pushes the arguments of a call. Returns the layout when it isn't only positional values.
    fn args(&mut self, args: &[Arg]) -> Compiled<Option<u32>> {
        for arg in args {
            self.expr(&arg.value)?;
        }
        if args.iter().all(|a| a.name.is_none() && !a.spread) {
            return Ok(None);
        }
        let kinds = args
            .iter()
            .map(|a| match (&a.name, a.spread) {
                (Some(name), _) => ArgKind::Named(Rc::from(name.as_str()), a.value.span),
                (None, true) => ArgKind::Spread(a.value.span),
                (None, false) => ArgKind::Positional,
            })
            .collect();
        let calls = &mut self.f().proto.calls;
        calls.push(kinds);
        Ok(Some(calls.len() as u32 - 1))
    }

    fn assign(&mut self, op: Option<BinaryOp>, target: &Expr, value: &Expr, keep: bool, span: Span) -> Compiled {
        match &target.kind {
            ExprKind::Variable(name) => {
                let resolved = self.resolve(&format!("${name}"));
                if let Some(op) = op {
                    self.load(resolved);
                    self.expr(value)?;
                    self.emit_at(binary_op(op), span);
                } else {
                    self.expr(value)?;
                }
                if keep {
                    self.emit(Op::Dup);
                }
                self.span = target.span;
                self.store(resolved, name);
                self.span = span;
            }
            ExprKind::Index { target: container, index, .. } => {
                self.expr(container)?;
                self.expr(index)?;
                self.expr(value)?;
                match op {
                    None => self.emit_at(Op::SetIndex, index.span),
                    Some(op) => self.emit_at(Op::UpdateIndex { op, mode: 0 }, index.span),
                };
                if !keep {
                    self.emit(Op::Pop);
                }
            }
            ExprKind::Field { target: object, name, .. } => {
                self.expr(object)?;
                let n = self.name(name);
                if let Some(op) = op {
                    self.emit(Op::Dup);
                    self.emit_at(Op::GetField(n), target.span);
                    self.expr(value)?;
                    self.emit_at(binary_op(op), span);
                } else {
                    self.expr(value)?;
                }
                self.emit_at(Op::SetField(n), target.span);
                if !keep {
                    self.emit(Op::Pop);
                }
            }
            ExprKind::Static { class, name, variable: true } => {
                self.load_class(class);
                let n = self.name(name);
                if let Some(op) = op {
                    self.emit(Op::Dup);
                    self.emit_at(Op::Static { name: n, variable: true }, target.span);
                    self.expr(value)?;
                    self.emit_at(binary_op(op), span);
                } else {
                    self.expr(value)?;
                }
                self.emit_at(Op::SetStatic(n), target.span);
                if !keep {
                    self.emit(Op::Pop);
                }
            }
            _ => {
                self.expr(value)?;
                self.fail("only variables, indexes and fields can be assigned to");
            }
        }
        Ok(())
    }

    fn increment(&mut self, target: &Expr, delta: i8, prefix: bool, keep: bool, span: Span) -> Compiled {
        match &target.kind {
            ExprKind::Variable(name) => {
                let resolved = self.resolve(&format!("${name}"));
                self.load(resolved);
                self.emit(Op::CheckNumber);
                if keep && !prefix {
                    self.emit(Op::Dup);
                }
                self.emit(Op::Int(delta as i32));
                self.emit(Op::Add);
                if keep && prefix {
                    self.emit(Op::Dup);
                }
                self.store(resolved, name);
            }
            ExprKind::Index { target: container, index, .. } => {
                self.expr(container)?;
                self.expr(index)?;
                self.emit(Op::Int(delta as i32));
                let mode = if prefix { 1 } else { 2 };
                self.emit_at(Op::UpdateIndex { op: BinaryOp::Add, mode }, span);
                if !keep {
                    self.emit(Op::Pop);
                }
            }
            ExprKind::Field { .. } | ExprKind::Static { .. } => {
                // `+= 1`, and for `$x++` the new value minus one
                let one = Expr { kind: ExprKind::Int(delta as i64), span };
                self.assign(Some(BinaryOp::Add), target, &one, keep, span)?;
                if keep && !prefix {
                    self.emit(Op::Int(-(delta as i32)));
                    self.emit(Op::Add);
                }
            }
            _ => {
                self.expr(target)?;
                self.fail("only variables, indexes and fields can be assigned to");
            }
        }
        Ok(())
    }
}

/// The method that picks a version of a method with several *(see `class`)*:
/// the first one whose parameters' types match the arguments.
fn dispatcher(name: &str, versions: &[(Vec<Option<TypeExpr>>, String)], class: &str, generics: &[String], span: Span) -> FunctionDecl {
    let at = |kind| Expr { kind, span };
    let count = versions.iter().map(|(p, _)| p.len()).max().unwrap_or(0);
    let params: Vec<Param> = (0..count)
        .map(|i| Param { name: format!("_arg{i}"), ty: None, default: None, variadic: false, field: false })
        .collect();
    let mut body = Vec::new();
    for (types, hidden) in versions {
        // Types that can be checked while the script runs: not the type parameters
        let mut cond: Option<Expr> = None;
        for (i, ty) in types.iter().enumerate() {
            let Some(ty) = ty else { continue };
            let mut ty = ty.clone();
            if let TypeKind::Named { name, .. } = &mut ty.kind {
                if name == "Self" {
                    *name = class.to_string();
                }
                if generics.contains(name) {
                    continue;
                }
            }
            let check = at(ExprKind::TypeCheck { receiver: Box::new(at(ExprKind::Variable(format!("_arg{i}")))), ty, check: TypeCheckKind::Is, nullable: false });
            cond = Some(match cond {
                None => check,
                Some(c) => at(ExprKind::Binary { op: BinaryOp::And, left: Box::new(c), right: Box::new(check) }),
            });
        }
        let args = (0..types.len()).map(|i| Arg { name: None, value: at(ExprKind::Variable(format!("_arg{i}"))), spread: false }).collect();
        let call = at(ExprKind::MethodCall { receiver: Box::new(at(ExprKind::This)), method: hidden.clone(), args, nullable: false });
        let ret = Stmt { kind: StmtKind::Return(Some(call)), span };
        body.push(match cond {
            Some(c) => Stmt { kind: StmtKind::If { cond: c, then: Box::new(ret), otherwise: None }, span },
            None => ret,
        });
    }
    let message = at(ExprKind::Str(format!("no version of `{class}.{name}` takes these arguments")));
    let error = at(ExprKind::Call { callee: Box::new(at(ExprKind::Function("TypeError".into()))), args: vec![Arg { name: None, value: message, spread: false }] });
    body.push(Stmt { kind: StmtKind::Throw(error), span });
    FunctionDecl { name: name.to_string(), generics: Vec::new(), params, ret: None, body: FunctionBody::Block(body) }
}

/// What a command's fields are as arguments, for `Git::parse()`, its
/// `--help` and its page of documentation *(see `ideas/tooling/docs.md`)*,
/// and the types of the enums they use, in the order of `ArgType::Enum`.
pub(crate) fn command_info(decl: &ClassDecl) -> (CommandInfo, Vec<&TypeExpr>) {
    let mut info = CommandInfo {
        doc: decl.doc.clone(),
        args: Vec::new(),
        subcommands: decl.subcommands.iter().map(|s| (Rc::from(s.name.rsplit("::").next().unwrap()), s.doc.clone())).collect(),
        optional_subcommand: decl.optional_subcommand,
        enums: 0,
    };
    let mut enums = Vec::new();
    for member in &decl.members {
        let MemberKind::Field { ty, name, value, flag, variadic, .. } = &member.kind else { continue };
        let (inner, nullable) = match &ty.kind {
            TypeKind::Nullable(inner) => (&**inner, true),
            _ => (ty, false),
        };
        let kind = match &inner.kind {
            TypeKind::Named { name, .. } if name == "string" => ArgType::Str,
            TypeKind::Named { name, .. } if name == "number" => ArgType::Number,
            TypeKind::Named { name, .. } if name == "bool" => ArgType::Bool,
            _ => {
                // An enum: its values are the choices
                enums.push(inner);
                info.enums += 1;
                ArgType::Enum(info.enums - 1)
            }
        };
        let default = value.as_ref().map(|v| match &v.kind {
            ExprKind::Str(s) => format!("'{s}'"),
            ExprKind::Int(n) => n.to_string(),
            ExprKind::Float(n) => n.to_string(),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Static { name, .. } => name.to_ascii_lowercase().replace('_', "-"),
            _ => "...".to_string(),
        });
        info.args.push(ArgInfo {
            name: Rc::from(name.as_str()),
            doc: member.doc.clone(),
            // `bool` fields are always flags
            flag: flag.is_some() || kind == ArgType::Bool,
            short: flag.as_ref().and_then(|f| f.short),
            count: flag.as_ref().is_some_and(|f| f.count),
            variadic: *variadic,
            kind,
            nullable,
            has_value: value.is_some(),
            default,
        });
    }
    (info, enums)
}

/// `assert $cond;` as the code that does it: `if (!$cond) __assertFailed('...');`.
/// For a comparison, both sides are kept in variables first, so its message
/// can show them *(see `ideas/tooling/testing.md`)*.
fn assertion(cond: &Expr, text: &str, span: Span) -> Stmt {
    let at = |kind| Expr { kind, span };
    let stmt = |kind| Stmt { kind, span };
    let fail = |args: Vec<Expr>| {
        let mut all = vec![Arg { name: None, value: at(ExprKind::Str(text.to_string())), spread: false }];
        all.extend(args.into_iter().map(|value| Arg { name: None, value, spread: false }));
        stmt(StmtKind::Expr(at(ExprKind::Call { callee: Box::new(at(ExprKind::Function("__assertFailed".into()))), args: all })))
    };
    let check = |cond: Expr, failed: Stmt| {
        stmt(StmtKind::If { cond: at(ExprKind::Unary { op: UnaryOp::Not, operand: Box::new(cond) }), then: Box::new(failed), otherwise: None })
    };
    match &cond.kind {
        ExprKind::Binary { op: op @ (BinaryOp::Eq | BinaryOp::NotEq | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::LtEq | BinaryOp::GtEq), left, right } => {
            let (l, r) = ("__assertLeft", "__assertRight");
            let side = |name: &str, value: &Expr| stmt(StmtKind::Let { constant: true, name: name.to_string(), ty: None, value: value.clone() });
            let var = |name: &str| at(ExprKind::Variable(name.to_string()));
            let compared = Expr { kind: ExprKind::Binary { op: *op, left: Box::new(var(l)), right: Box::new(var(r)) }, span: cond.span };
            stmt(StmtKind::Block(vec![side(l, left), side(r, right), check(compared, fail(vec![var(l), var(r)]))]))
        }
        // `.equals()` compares lists and maps by their values: both sides are shown too
        ExprKind::MethodCall { receiver, method, args, nullable: false } if method == "equals" && args.len() == 1 && args[0].name.is_none() && !args[0].spread => {
            let (l, r) = ("__assertLeft", "__assertRight");
            let side = |name: &str, value: &Expr| stmt(StmtKind::Let { constant: true, name: name.to_string(), ty: None, value: value.clone() });
            let var = |name: &str| at(ExprKind::Variable(name.to_string()));
            let arg = vec![Arg { name: None, value: var(r), spread: false }];
            let compared = Expr { kind: ExprKind::MethodCall { receiver: Box::new(var(l)), method: method.clone(), args: arg, nullable: false }, span: cond.span };
            stmt(StmtKind::Block(vec![side(l, receiver), side(r, &args[0].value), check(compared, fail(vec![var(l), var(r)]))]))
        }
        _ => check(cond.clone(), fail(Vec::new())),
    }
}

/// A method of an `implement` block can be called through the interface with
/// an object of another class. Its `Self` parameters check that they got one
/// of its own class, and throw a `TypeError` otherwise.
fn self_checks(method: &mut FunctionDecl, class: &str) {
    let mut checks = Vec::new();
    for param in &method.params {
        let is_self = matches!(&param.ty, Some(TypeExpr { kind: TypeKind::Named { name, args }, .. }) if args.is_empty() && (name == "Self" || name == class));
        if !is_self {
            continue;
        }
        let span = param.ty.as_ref().unwrap().span;
        let at = |kind| Expr { kind, span };
        let value = at(ExprKind::Variable(param.name.clone()));
        let ty = TypeExpr { kind: TypeKind::Named { name: class.to_string(), args: Vec::new() }, span };
        let check = at(ExprKind::TypeCheck { receiver: Box::new(value.clone()), ty, check: TypeCheckKind::Is, nullable: false });
        let message = at(ExprKind::Interpolated(vec![
            StringPart::Text(format!("`{}` needs a `{class}` for `${}`, not a ", method.name, param.name)),
            StringPart::Expr(at(ExprKind::MethodCall { receiver: Box::new(value), method: "typeName".into(), args: Vec::new(), nullable: false })),
        ]));
        let error = at(ExprKind::Call { callee: Box::new(at(ExprKind::Function("TypeError".into()))), args: vec![Arg { name: None, value: message, spread: false }] });
        let not = at(ExprKind::Unary { op: UnaryOp::Not, operand: Box::new(check) });
        checks.push(Stmt { kind: StmtKind::If { cond: not, then: Box::new(Stmt { kind: StmtKind::Throw(error), span }), otherwise: None }, span });
    }
    if checks.is_empty() {
        return;
    }
    let body = std::mem::replace(&mut method.body, FunctionBody::Block(Vec::new()));
    let rest = match body {
        FunctionBody::Block(stmts) => stmts,
        FunctionBody::Expr(e) => {
            let span = e.span;
            vec![Stmt { kind: StmtKind::Return(Some(e)), span }]
        }
    };
    checks.extend(rest);
    method.body = FunctionBody::Block(checks);
}

#[derive(Clone, Copy)]
enum Target {
    Global(u32, bool),
    Local(Place),
}

fn binary_op(op: BinaryOp) -> Op {
    match op {
        BinaryOp::Add => Op::Add,
        BinaryOp::Sub => Op::Sub,
        BinaryOp::Mul => Op::Mul,
        BinaryOp::Div => Op::Div,
        BinaryOp::Rem => Op::Rem,
        other => Op::Binary(other),
    }
}

// ---- Which names closures use ----
//
// A variable that a closure uses must live in a cell. Before compiling a
// function, every name used inside the functions nested in it is collected:
// its variables with those names go into cells. Because there's no shadowing,
// a name means the same variable everywhere it can be seen, so this is exact
// enough, and at worst a variable gets a cell it didn't need.

fn collect_function(decl: &FunctionDecl, inside: bool, out: &mut HashSet<String>) {
    for param in &decl.params {
        if let Some(default) = &param.default {
            collect_expr(default, inside, out);
        }
    }
    match &decl.body {
        FunctionBody::Expr(e) => collect_expr(e, inside, out),
        FunctionBody::Block(stmts) => collect_stmts(stmts, inside, out),
    }
}

fn collect_stmts(stmts: &[Stmt], inside: bool, out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_stmt(stmt, inside, out);
    }
}

fn collect_stmt(stmt: &Stmt, inside: bool, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Let { value, .. } | StmtKind::LetList { value, .. } => collect_expr(value, inside, out),
        StmtKind::Function(decl) => {
            if inside {
                out.insert(decl.name.clone());
            }
            collect_function(decl, true, out);
        }
        StmtKind::Break | StmtKind::Continue | StmtKind::Use(_) => {}
        StmtKind::If { cond, then, otherwise } => {
            collect_expr(cond, inside, out);
            collect_stmt(then, inside, out);
            if let Some(o) = otherwise {
                collect_stmt(o, inside, out);
            }
        }
        StmtKind::While { cond, body } => {
            collect_expr(cond, inside, out);
            collect_stmt(body, inside, out);
        }
        StmtKind::For { iterable, body, .. } => {
            collect_expr(iterable, inside, out);
            collect_stmt(body, inside, out);
        }
        StmtKind::Return(value) => {
            if let Some(v) = value {
                collect_expr(v, inside, out);
            }
        }
        StmtKind::Yield(e) | StmtKind::Throw(e) | StmtKind::Expr(e) => collect_expr(e, inside, out),
        StmtKind::Assert { cond, .. } => collect_expr(cond, inside, out),
        StmtKind::Switch(switch) => collect_switch(switch, inside, out),
        StmtKind::Try { body, catches, finally } => {
            collect_stmts(body, inside, out);
            for c in catches {
                collect_stmts(&c.body, inside, out);
            }
            if let Some(f) = finally {
                collect_stmts(f, inside, out);
            }
        }
        StmtKind::Pub(inner) => collect_stmt(inner, inside, out),
        StmtKind::Block(stmts) => collect_stmts(stmts, inside, out),
        StmtKind::Class(decl) => {
            // Its methods use the class itself, for `Self` and `@super`
            out.insert(decl.name.clone());
            for member in &decl.members {
                match &member.kind {
                    MemberKind::Method { decl, .. } | MemberKind::Constructor(decl) => collect_function(decl, true, out),
                    MemberKind::Field { value: Some(v), .. } => collect_expr(v, inside || !member.is_static, out),
                    MemberKind::Field { value: None, .. } => {}
                }
            }
            for value in &decl.values {
                for a in &value.args {
                    collect_expr(&a.value, inside, out);
                }
            }
        }
    }
}

fn collect_switch(switch: &Switch, inside: bool, out: &mut HashSet<String>) {
    collect_expr(&switch.subject, inside, out);
    for case in &switch.cases {
        for p in &case.patterns {
            if let Pattern::Value(e) = p {
                collect_expr(e, inside, out);
            }
        }
        collect_case(&case.body, inside, out);
    }
    if let Some(d) = &switch.default {
        collect_case(d, inside, out);
    }
}

fn collect_case(body: &CaseBody, inside: bool, out: &mut HashSet<String>) {
    match body {
        CaseBody::Value(b) => collect_branch(b, inside, out),
        CaseBody::Stmt(s) => collect_stmt(s, inside, out),
    }
}

fn collect_branch(branch: &Branch, inside: bool, out: &mut HashSet<String>) {
    match branch {
        Branch::Expr(e) => collect_expr(e, inside, out),
        Branch::Block(stmts, _) => collect_stmts(stmts, inside, out),
    }
}

fn collect_expr(expr: &Expr, inside: bool, out: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            if inside {
                out.insert(format!("${name}"));
            }
        }
        ExprKind::Function(name) | ExprKind::TypeArgs { name, .. } => {
            if inside {
                out.insert(name.clone());
            }
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Regex { .. }
        | ExprKind::Bytes(_)
        | ExprKind::Type(_)
        | ExprKind::Bool(_)
        | ExprKind::Null
        | ExprKind::ModuleFunction { .. }
        | ExprKind::ModuleConstant { .. } => {}
        ExprKind::This => {
            if inside {
                out.insert("$@".into());
            }
        }
        ExprKind::Static { class, .. } => {
            if inside && class != "Self" {
                out.insert(class.clone());
            }
        }
        ExprKind::Field { target, .. } => collect_expr(target, inside, out),
        ExprKind::SuperNew(args) | ExprKind::SuperMethod { args, .. } => {
            if inside {
                out.insert("$@".into());
            }
            for a in args {
                collect_expr(&a.value, inside, out);
            }
        }
        ExprKind::Interpolated(parts) | ExprKind::Command(parts) => {
            for part in parts {
                if let StringPart::Expr(e) = part {
                    collect_expr(e, inside, out);
                }
            }
        }
        ExprKind::List { items, .. } => {
            for item in items {
                collect_expr(item, inside, out);
            }
        }
        ExprKind::Spread(inner) => collect_expr(inner, inside, out),
        ExprKind::Map { entries, .. } => {
            for (k, v) in entries {
                collect_expr(k, inside, out);
                collect_expr(v, inside, out);
            }
        }
        ExprKind::Unary { operand: e, .. }
        | ExprKind::PathTest { path: e, .. }
        | ExprKind::Try(e)
        | ExprKind::Throw(e)
        | ExprKind::Increment { target: e, .. }
        | ExprKind::NonNull(e)
        | ExprKind::TypeCheck { receiver: e, .. } => collect_expr(e, inside, out),
        ExprKind::Binary { left, right, .. } => {
            collect_expr(left, inside, out);
            collect_expr(right, inside, out);
        }
        ExprKind::Range { start, end, .. } => {
            collect_expr(start, inside, out);
            if let Some(e) = end {
                collect_expr(e, inside, out);
            }
        }
        ExprKind::Ternary { cond, then, otherwise } => {
            collect_expr(cond, inside, out);
            collect_expr(then, inside, out);
            collect_expr(otherwise, inside, out);
        }
        ExprKind::Assign { target, value, .. } => {
            collect_expr(target, inside, out);
            collect_expr(value, inside, out);
        }
        ExprKind::Call { callee, args } => {
            collect_expr(callee, inside, out);
            for a in args {
                collect_expr(&a.value, inside, out);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            collect_expr(receiver, inside, out);
            for a in args {
                collect_expr(&a.value, inside, out);
            }
        }
        ExprKind::Index { target, index, .. } => {
            collect_expr(target, inside, out);
            collect_expr(index, inside, out);
        }
        ExprKind::Lambda(decl) => collect_function(decl, true, out),
        ExprKind::If { cond, then, otherwise } => {
            collect_expr(cond, inside, out);
            collect_branch(then, inside, out);
            collect_branch(otherwise, inside, out);
        }
        ExprKind::Switch(switch) => collect_switch(switch, inside, out),
    }
}

/// How a value is written, for the error of `$x!` *(i.e. `$list[0]`, `@part2`,
/// `$user.name`)*. `None` for anything longer, which the error shows with its line.
fn written(e: &Expr) -> Option<String> {
    Some(match &e.kind {
        ExprKind::Variable(v) => format!("${v}"),
        ExprKind::Field { target, name, nullable } => match &target.kind {
            ExprKind::This => format!("@{name}"),
            _ => format!("{}{}{name}", written(target)?, if *nullable { "?." } else { "." }),
        },
        ExprKind::Index { target, index, nullable } => {
            let i = match &index.kind {
                ExprKind::Int(n) => n.to_string(),
                ExprKind::Str(s) => format!("'{s}'"),
                _ => written(index)?,
            };
            format!("{}{}{i}]", written(target)?, if *nullable { "?[" } else { "[" })
        }
        ExprKind::MethodCall { receiver, method, args, nullable } if args.is_empty() => {
            format!("{}{}{method}()", written(receiver)?, if *nullable { "?." } else { "." })
        }
        ExprKind::Call { callee, args } if args.is_empty() => match &callee.kind {
            ExprKind::Function(name) => format!("{name}()"),
            _ => return None,
        },
        _ => return None,
    })
}
