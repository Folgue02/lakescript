//! Turns tokens into a syntax tree, following the precedence table in
//! `ideas/language/operators.md`. Every function parses one level and calls the
//! next, tighter one.
//!
//! Phase 1 covers declarations, functions, `if`, loops, the operators, calls,
//! lists and maps. Types are parsed and kept in the tree for the checker, but
//! nothing uses them yet.

use std::rc::Rc;

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::token::{Keyword, Span, StringPart as TokenPart, Token, TokenKind};

type Result<T> = std::result::Result<T, Diagnostic>;

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>> {
    parse_with_budget(tokens, crate::stack::DEFAULT_BUDGET)
}

/// Like `parse`, allowing the parser to use `stack_budget` bytes of stack
/// before deeply nested code becomes an error.
pub fn parse_with_budget(tokens: Vec<Token>, stack_budget: usize) -> Result<Vec<Stmt>> {
    let mut parser = Parser::new(tokens);
    parser.stack_budget = stack_budget;
    parser.program()
}

/// Like `parse_with_budget`, but it goes on after a syntax error, for code
/// that's being written in an editor *(see `ideas/tooling/lsp.md`)*. It returns
/// the statements it could read, and every error it found, in order.
///
/// - A missing `;` at the end of a line is an error, and the statement ends there.
/// - `.` or `name::` with nothing after it is an error, and the name is empty
///   *(i.e. `$point.` while a method is being written)*.
/// - Any other error skips the statement, to its `;` or its block.
pub fn parse_recovering(tokens: Vec<Token>, stack_budget: usize) -> (Vec<Stmt>, Vec<Diagnostic>) {
    let mut parser = Parser::new(tokens);
    parser.stack_budget = stack_budget;
    parser.recover = true;
    let stmts = parser.program().unwrap_or_default();
    let mut errors = parser.errors;
    errors.sort_by_key(|e| e.span.start);
    (stmts, errors)
}

/// A signature of the checker's list of built-in methods and functions
/// *(see `checker/prelude.txt`)*: a function without a body, or a constant.
pub enum Signature {
    Function(FunctionDecl),
    Constant { ty: TypeExpr, name: String },
}

/// Reads signatures like `string read(string $path);` and `const string $separator;`.
pub fn parse_signatures(tokens: Vec<Token>) -> Result<Vec<Signature>> {
    let mut parser = Parser::new(tokens);
    let mut out = Vec::new();
    while !parser.check(&TokenKind::Eof) {
        if parser.eat(&TokenKind::Keyword(Keyword::Const)) {
            let ty = parser.parse_type()?;
            let name = parser.expect_variable("for the constant")?;
            parser.expect(&TokenKind::Semicolon, "after the constant")?;
            out.push(Signature::Constant { ty, name });
            continue;
        }
        let ret = parser.parse_type()?;
        let name = match parser.peek().kind.clone() {
            TokenKind::Keyword(k) => {
                parser.advance();
                k.as_str().to_string()
            }
            _ => parser.expect_ident("for the function")?,
        };
        let mut generics = Vec::new();
        if parser.eat(&TokenKind::Lt) {
            loop {
                generics.push(parser.expect_ident("for the type parameter")?);
                if !parser.eat(&TokenKind::Comma) {
                    break;
                }
            }
            parser.expect_closing_angle("after the type parameters")?;
        }
        let params = parser.params()?;
        parser.expect(&TokenKind::Semicolon, "after the signature")?;
        let body = FunctionBody::Expr(Expr { kind: ExprKind::Null, span: Span::default() });
        out.push(Signature::Function(FunctionDecl { name, generics, params, ret: Some(ret), body }));
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// How many `>` of the current `>>` or `>>>` token were already used to
    /// close generics *(i.e. `list<map<string, any>>`)*.
    split: u8,
    /// What the current token looks like when it's partly used.
    split_token: Token,
    last_span: Span,
    /// Where the stack was when parsing started, and how much it may use *(see `guard`)*.
    stack_base: usize,
    stack_budget: usize,
    /// Set while parsing a `case` value, where `(...) ->` is the value and
    /// the arrow of the case, not an anonymous function.
    in_case_pattern: bool,
    /// Going on after errors *(see `parse_recovering`)*.
    recover: bool,
    errors: Vec<Diagnostic>,
    /// Where the last name being written *(i.e. `$f.`)* ended: no expression goes on from there.
    hole: Option<usize>,
}

#[derive(Clone, Copy)]
struct Checkpoint {
    pos: usize,
    split: u8,
    errors: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        let eof = Token { kind: TokenKind::Eof, span: Span::default(), doc: None };
        Parser { tokens, pos: 0, split: 0, split_token: eof, last_span: Span::default(), stack_base: crate::stack::position(), stack_budget: crate::stack::DEFAULT_BUDGET, in_case_pattern: false, recover: false, errors: Vec::new(), hole: None }
    }

    /// The statements of a whole file.
    fn program(&mut self) -> Result<Vec<Stmt>> {
        let mut stmts = Vec::new();
        // `use` lines go at the top, before any other statement
        while self.check(&TokenKind::Keyword(Keyword::Use)) {
            let start = self.peek().span;
            match self.use_decl() {
                Ok(decl) => stmts.push(Stmt { kind: StmtKind::Use(decl), span: self.span_from(start) }),
                Err(e) if self.recover => self.skip_statement(e),
                Err(e) => return Err(e),
            }
        }
        while !self.check(&TokenKind::Eof) {
            // A `}` without its `{`. It's skipped here, since `statement` reads
            // nothing at a `}` when recovering *(an empty body)*
            if self.recover && self.check(&TokenKind::RBrace) {
                self.errors.push(Diagnostic::new("unexpected `}`", self.peek().span));
                self.advance();
                continue;
            }
            let before = self.pos;
            match self.statement() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) if self.recover => self.skip_statement(e),
                Err(e) => return Err(e),
            }
            // Recovering never stays in place: something is read every time
            if self.recover && self.pos == before {
                self.advance();
            }
        }
        Ok(stmts)
    }

    /// After an error, while recovering: notes it, and skips the rest of the
    /// statement, to its `;`, or to the end of the block it opened. A `}` that
    /// closes the block around it stays, for that block.
    fn skip_statement(&mut self, error: Diagnostic) {
        self.errors.push(error);
        let mut depth = 0usize;
        loop {
            match self.peek().kind {
                TokenKind::Eof => break,
                TokenKind::Semicolon if depth == 0 => {
                    self.advance();
                    break;
                }
                TokenKind::LBrace => depth += 1,
                // The end of the block around it, or a stray `}` that `program` skips
                TokenKind::RBrace if depth == 0 => break,
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        self.advance();
                        break;
                    }
                }
                _ => {}
            }
            self.advance();
        }
    }

    /// Notes an error and goes on, while recovering.
    fn note_error(&mut self, message: impl Into<String>, span: Span) -> bool {
        if self.recover {
            self.errors.push(Diagnostic::new(message, span));
        }
        self.recover
    }

    // ---- Cursor ----

    fn peek(&self) -> &Token {
        if self.split > 0 {
            return &self.split_token;
        }
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_at(&self, offset: usize) -> &TokenKind {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)].kind
    }

    fn check(&self, kind: &TokenKind) -> bool {
        &self.peek().kind == kind
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        self.split = 0;
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        self.last_span = token.span;
        token
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, context: &str) -> Result<Token> {
        if self.check(kind) {
            return Ok(self.advance());
        }
        let message = format!("expected {} {context}, found {}", kind.describe(), self.peek().kind);
        // A `;`, `)` or `]` forgotten at the end of a line, while it's being
        // written *(i.e. `outln($task.`)*: it ends there
        let next = self.peek();
        let ends_line = next.span.line > self.last_span.line || matches!(next.kind, TokenKind::RBrace | TokenKind::Eof);
        if matches!(kind, TokenKind::Semicolon | TokenKind::RParen | TokenKind::RBracket) && ends_line {
            let span = self.last_span;
            if self.note_error(message.clone(), span) {
                return Ok(Token { kind: kind.clone(), span, doc: None });
            }
        }
        self.error_here(message)
    }

    fn checkpoint(&self) -> Checkpoint {
        Checkpoint { pos: self.pos, split: self.split, errors: self.errors.len() }
    }

    fn restore(&mut self, checkpoint: Checkpoint) {
        self.pos = checkpoint.pos;
        self.split = checkpoint.split;
        self.errors.truncate(checkpoint.errors);
        if self.split > 0 {
            self.update_split_token();
        }
    }

    /// Uses one `>` to close generics, splitting `>>` and `>>>` if needed.
    fn expect_closing_angle(&mut self, context: &str) -> Result<()> {
        let total = match self.tokens[self.pos].kind {
            TokenKind::Gt => 1,
            TokenKind::Shr => 2,
            TokenKind::UShr => 3,
            _ => return self.error_here(format!("expected `>` {context}, found {}", self.peek().kind)),
        };
        if total - self.split == 1 {
            self.advance();
        } else {
            self.split += 1;
            self.update_split_token();
        }
        Ok(())
    }

    fn update_split_token(&mut self) {
        let real = &self.tokens[self.pos];
        let total = if real.kind == TokenKind::Shr { 2 } else { 3 };
        let remaining = total - self.split;
        let kind = if remaining == 1 { TokenKind::Gt } else { TokenKind::Shr };
        let mut span = real.span;
        span.start += self.split as usize;
        span.column += self.split as u32;
        self.split_token = Token { kind, span, doc: None };
    }

    fn span_from(&self, start: Span) -> Span {
        Span { start: start.start, end: self.last_span.end.max(start.end), line: start.line, column: start.column, file: start.file }
    }

    /// Stops deeply nested code with an error before it overflows the stack.
    /// Every function that can be reached again from itself calls it.
    fn guard(&self) -> Result<()> {
        if crate::stack::position().abs_diff(self.stack_base) > self.stack_budget {
            return self.error_here("this code is nested too deeply");
        }
        Ok(())
    }

    fn error_here<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(Diagnostic::new(message, self.peek().span))
    }

    fn expect_variable(&mut self, context: &str) -> Result<String> {
        match &self.peek().kind {
            TokenKind::Variable(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            other => self.error_here(format!("expected a variable {context}, found {other}")),
        }
    }

    fn expect_ident(&mut self, context: &str) -> Result<String> {
        match &self.peek().kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            other => self.error_here(format!("expected a name {context}, found {other}")),
        }
    }

    // ---- Statements ----

    fn statement(&mut self) -> Result<Stmt> {
        self.guard()?;
        let start = self.peek().span;
        // The body of an `if` or a loop not written yet: an empty one
        if matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) && self.note_error(format!("expected a statement, found {}", self.peek().kind), start) {
            return Ok(Stmt { kind: StmtKind::Block(Vec::new()), span: Span { end: start.start, ..start } });
        }
        let kind = match &self.peek().kind {
            TokenKind::Keyword(k @ (Keyword::Let | Keyword::Const)) if self.peek_at(1) == &TokenKind::LBracket => {
                let constant = *k == Keyword::Const;
                self.advance();
                let (names, rest) = self.list_pattern()?;
                self.expect(&TokenKind::Eq, "after the list of variables")?;
                let value = self.expression()?;
                self.expect(&TokenKind::Semicolon, "after the declaration")?;
                StmtKind::LetList { constant, names, rest, value }
            }
            TokenKind::Keyword(Keyword::Let) => {
                self.advance();
                let name = self.expect_variable("after `let`")?;
                self.declaration_value(false, name, None)?
            }
            TokenKind::Keyword(Keyword::Const) => {
                self.advance();
                let ty = if matches!(self.peek().kind, TokenKind::Variable(_)) { None } else { Some(self.parse_type()?) };
                let name = self.expect_variable("after `const`")?;
                self.declaration_value(true, name, ty)?
            }
            TokenKind::Keyword(k @ (Keyword::Class | Keyword::Interface | Keyword::Enum)) => {
                let kind = match *k {
                    Keyword::Class => ClassKind::Class,
                    Keyword::Interface => ClassKind::Interface,
                    _ => ClassKind::Enum,
                };
                StmtKind::Class(Rc::new(self.class_decl(kind, start)?))
            }
            // `command Git { ... }`. `command` alone is also the type of `%x"..."`
            TokenKind::Ident(word)
                if word == "command" && matches!(self.peek_at(1), TokenKind::Ident(n) if is_type_name(n)) && self.peek_at(2) == &TokenKind::LBrace =>
            {
                let doc = self.peek().doc.clone();
                self.advance();
                let name = self.expect_ident("for the command")?;
                StmtKind::Class(Rc::new(self.command_body(name, doc, start)?))
            }
            TokenKind::Keyword(Keyword::If) => self.if_statement()?,
            TokenKind::Keyword(Keyword::While) => {
                self.advance();
                self.expect(&TokenKind::LParen, "after `while`")?;
                let cond = self.expression()?;
                self.expect(&TokenKind::RParen, "after the condition")?;
                let body = Box::new(self.statement()?);
                StmtKind::While { cond, body }
            }
            TokenKind::Keyword(Keyword::For) => self.for_statement()?,
            TokenKind::Keyword(Keyword::Return) => {
                self.advance();
                let value = if self.check(&TokenKind::Semicolon) { None } else { Some(self.expression()?) };
                self.expect(&TokenKind::Semicolon, "after `return`")?;
                StmtKind::Return(value)
            }
            TokenKind::Keyword(Keyword::Break) => {
                self.advance();
                self.expect(&TokenKind::Semicolon, "after `break`")?;
                StmtKind::Break
            }
            TokenKind::Keyword(Keyword::Continue) => {
                self.advance();
                self.expect(&TokenKind::Semicolon, "after `continue`")?;
                StmtKind::Continue
            }
            TokenKind::Keyword(Keyword::Switch) => StmtKind::Switch(Box::new(self.switch(false)?)),
            TokenKind::Keyword(Keyword::Yield) => {
                self.advance();
                let value = self.expression()?;
                self.expect(&TokenKind::Semicolon, "after `yield`")?;
                StmtKind::Yield(value)
            }
            TokenKind::Keyword(Keyword::Try) if self.peek_at(1) == &TokenKind::LBrace => self.try_statement()?,
            TokenKind::Keyword(Keyword::Throw) => {
                self.advance();
                let value = self.expression()?;
                self.expect(&TokenKind::Semicolon, "after `throw`")?;
                StmtKind::Throw(value)
            }
            TokenKind::Keyword(Keyword::Assert) => {
                let text = self.peek().doc.clone().unwrap_or_default();
                self.advance();
                let cond = self.expression()?;
                self.expect(&TokenKind::Semicolon, "after the condition of `assert`")?;
                StmtKind::Assert { cond, text }
            }
            TokenKind::Keyword(k @ (Keyword::Catch | Keyword::Finally)) => {
                return self.error_here(format!("`{}` must come after a `try` block", k.as_str()));
            }
            TokenKind::Keyword(Keyword::Use) => {
                return self.error_here("`use` lines go at the top of the file, before any other statement");
            }
            TokenKind::Keyword(Keyword::Pub) => {
                self.advance();
                let inner = self.statement()?;
                let allowed = matches!(
                    inner.kind,
                    StmtKind::Function(_) | StmtKind::Let { constant: true, .. } | StmtKind::Class(_)
                );
                if !allowed {
                    return Err(Diagnostic::new("`pub` goes before functions, constants declared with `const`, classes, interfaces and enums", inner.span));
                }
                if let StmtKind::Let { name, ty: None, .. } = &inner.kind {
                    return Err(Diagnostic::new(
                        format!("`pub const ${name}` needs its type written, like a function's return type: `pub const string ${name} = ...;`"),
                        inner.span,
                    ));
                }
                StmtKind::Pub(Box::new(inner))
            }
            TokenKind::Keyword(k @ (Keyword::Else | Keyword::In | Keyword::Case | Keyword::Default)) => {
                return self.error_here(format!("unexpected `{}`", k.as_str()));
            }
            TokenKind::LBrace => StmtKind::Block(self.block()?),
            TokenKind::Semicolon => return self.error_here("unexpected `;`"),
            _ => match self.typed_statement()? {
                Some(kind) => kind,
                None => {
                    let expr = self.expression()?;
                    self.expect(&TokenKind::Semicolon, "after the expression")?;
                    StmtKind::Expr(expr)
                }
            },
        };
        Ok(Stmt { kind, span: self.span_from(start) })
    }

    /// `= value;` after the name of a declaration.
    fn declaration_value(&mut self, constant: bool, name: String, ty: Option<TypeExpr>) -> Result<StmtKind> {
        self.expect(&TokenKind::Eq, &format!("after `${name}`: variables are declared with a value"))?;
        let value = self.expression()?;
        self.expect(&TokenKind::Semicolon, "after the declaration")?;
        Ok(StmtKind::Let { constant, name, ty, value })
    }

    /// A statement that starts with a type: a declaration (`string $name = ...`)
    /// or a function definition (`string greet(...)`). Returns `None`, without
    /// moving, if the statement doesn't start with a type.
    fn typed_statement(&mut self) -> Result<Option<StmtKind>> {
        let checkpoint = self.checkpoint();
        let Ok(ty) = self.parse_type() else {
            self.restore(checkpoint);
            return Ok(None);
        };
        match self.peek().kind.clone() {
            TokenKind::Variable(name) => {
                self.advance();
                Ok(Some(self.declaration_value(false, name, Some(ty))?))
            }
            TokenKind::Ident(_) if matches!(self.peek_at(1), TokenKind::LParen | TokenKind::Lt) => {
                Ok(Some(StmtKind::Function(self.function(ty)?)))
            }
            _ => {
                self.restore(checkpoint);
                Ok(None)
            }
        }
    }

    fn function(&mut self, ret: TypeExpr) -> Result<FunctionDecl> {
        let name = self.expect_ident("for the function")?;
        let mut generics = Vec::new();
        if self.eat(&TokenKind::Lt) {
            loop {
                generics.push(self.expect_ident("for the type parameter")?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect_closing_angle("after the type parameters")?;
        }

        let params = self.params()?;

        let body = if self.eat(&TokenKind::Eq) {
            let expr = self.expression()?;
            self.expect(&TokenKind::Semicolon, "after the function")?;
            FunctionBody::Expr(expr)
        } else if self.check(&TokenKind::LBrace) {
            FunctionBody::Block(self.block()?)
        } else {
            return self.error_here(format!("expected `{{` or `=` for the body of `{name}`, found {}", self.peek().kind));
        };
        Ok(FunctionDecl { name, generics, params, ret: Some(ret), body })
    }

    // ---- Classes ----

    /// `class Name<T> extends Base { ... }`, an `interface` or an `enum`.
    fn class_decl(&mut self, kind: ClassKind, start: Span) -> Result<ClassDecl> {
        let doc = self.peek().doc.clone();
        self.advance();
        let word = kind.word();
        let name = self.expect_ident(&format!("for the {word}"))?;
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Err(Diagnostic::new(format!("the name of a {word} is written in PascalCase *(i.e. `{}`)*", pascal(&name)), self.span_from(start)));
        }
        if kind == ClassKind::Interface && !(name.len() > 1 && name.starts_with('I') && name[1..].starts_with(|c: char| c.is_ascii_uppercase())) {
            return Err(Diagnostic::new(
                format!("the name of an interface starts with `I` *(i.e. `I{}`)*, so it's never confused with a class", pascal(&name)),
                self.span_from(start),
            ));
        }
        let generics = self.type_params()?;
        let mut extends = Vec::new();
        if matches!(&self.peek().kind, TokenKind::Ident(w) if w == "extends") {
            self.advance();
            loop {
                extends.push(self.parse_type()?);
                if kind == ClassKind::Class || !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            if kind == ClassKind::Enum {
                return Err(Diagnostic::new("an enum can't extend anything", self.span_from(start)));
            }
        }
        self.expect(&TokenKind::LBrace, &format!("to start the {word}"))?;
        let values = if kind == ClassKind::Enum { self.enum_values()? } else { Vec::new() };
        let mut members = Vec::new();
        let mut implements = Vec::new();
        self.members(kind, Origin::Own, &mut members, &mut implements)?;
        Ok(ClassDecl { kind, name, generics, extends, implements, members, values, subcommands: Vec::new(), optional_subcommand: false, doc, span: self.span_from(start) })
    }

    /// The fields and subcommands of a command, after its name *(see `ideas/language/cli.md`)*.
    fn command_body(&mut self, name: String, doc: Option<String>, start: Span) -> Result<ClassDecl> {
        self.expect(&TokenKind::LBrace, "to start the command")?;
        let mut members = Vec::new();
        let mut subcommands = Vec::new();
        let mut optional_subcommand = false;
        loop {
            self.guard()?;
            let member_start = self.peek().span;
            let doc = self.peek().doc.clone();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.check(&TokenKind::Eof) {
                return self.error_here("expected `}` to close the command, found the end of the file");
            }
            let word = match &self.peek().kind {
                TokenKind::Ident(w) => w.clone(),
                _ => String::new(),
            };
            if word == "optional" || word == "subcommand" {
                if word == "optional" {
                    self.advance();
                    if !matches!(&self.peek().kind, TokenKind::Ident(w) if w == "subcommand") {
                        return self.error_here("`optional` goes before `subcommand`");
                    }
                    optional_subcommand = true;
                }
                self.advance();
                let sub = self.expect_ident("for the subcommand")?;
                if !is_type_name(&sub) {
                    return self.error_here(format!("the name of a subcommand is written in PascalCase *(i.e. `{}`)*", pascal(&sub)));
                }
                subcommands.push(self.command_body(format!("{name}::{sub}"), doc, member_start)?);
                continue;
            }
            let flag = if word == "flag" {
                self.advance();
                let mut flag = Flag { short: None, count: false };
                if self.eat(&TokenKind::LParen) {
                    let letter = self.expect_ident("for the short flag *(i.e. `flag(v)`)*")?;
                    let mut chars = letter.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) if c.is_ascii_alphanumeric() => flag.short = Some(c),
                        _ => return self.error_here(format!("a short flag is a single letter, not `{letter}`")),
                    }
                    if self.eat(&TokenKind::Comma) {
                        let what = self.expect_ident("after `,`")?;
                        if what != "count" {
                            return self.error_here(format!("expected `count` after the short flag, found `{what}`"));
                        }
                        flag.count = true;
                    }
                    self.expect(&TokenKind::RParen, "after the short flag")?;
                }
                Some(flag)
            } else {
                None
            };
            let variadic = self.eat(&TokenKind::Ellipsis);
            let ty = self.parse_type()?;
            let field = match self.peek().kind.clone() {
                TokenKind::Field(f) => f,
                TokenKind::Variable(v) => {
                    return self.error_here(format!("the arguments of a command are fields of its object, written with `@` *(i.e. `@{v}`)*"));
                }
                TokenKind::Ident(_) => return self.error_here("commands only have fields and subcommands, not methods"),
                other => return self.error_here(format!("expected a field like `@name` after the type, found {other}")),
            };
            self.advance();
            let value = if self.eat(&TokenKind::Eq) { Some(self.expression()?) } else { None };
            self.expect(&TokenKind::Semicolon, "after the field")?;
            let kind = MemberKind::Field { constant: false, ty, name: field, value, flag, variadic };
            members.push(Member { public: true, is_static: false, origin: Origin::Own, kind, doc, span: self.span_from(member_start) });
        }
        Ok(ClassDecl {
            kind: ClassKind::Command,
            name,
            generics: Vec::new(),
            extends: Vec::new(),
            implements: Vec::new(),
            members,
            values: Vec::new(),
            subcommands,
            optional_subcommand,
            doc,
            span: self.span_from(start),
        })
    }

    /// `AT_WORK, SICK(false);` at the start of an enum.
    fn enum_values(&mut self) -> Result<Vec<EnumValue>> {
        let mut values = Vec::new();
        while let TokenKind::Ident(name) = self.peek().kind.clone() {
            let start = self.peek().span;
            if !name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
                return self.error_here(format!("the values of an enum are written in SCREAMING_CASE *(i.e. `{}`)*, and end with `;`", name.to_ascii_uppercase()));
            }
            self.advance();
            let args = if self.check(&TokenKind::LParen) { self.arguments()? } else { Vec::new() };
            values.push(EnumValue { name, args, span: self.span_from(start) });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        if !self.check(&TokenKind::RBrace) {
            self.expect(&TokenKind::Semicolon, "after the values of the enum")?;
        }
        Ok(values)
    }

    /// The members of a class until its `}`, or of an `override { }` or `implement X { }` block.
    fn members(&mut self, kind: ClassKind, origin: Origin, out: &mut Vec<Member>, implements: &mut Vec<TypeExpr>) -> Result<()> {
        loop {
            self.guard()?;
            let start = self.peek().span;
            if self.eat(&TokenKind::RBrace) {
                return Ok(());
            }
            if self.check(&TokenKind::Eof) {
                return self.error_here(format!("expected `}}` to close the {}, found the end of the file", kind.word()));
            }
            let word = match &self.peek().kind {
                TokenKind::Ident(w) => w.clone(),
                _ => String::new(),
            };
            if (word == "override" || word == "implement") && origin == Origin::Own {
                if (word == "override" && kind != ClassKind::Class) || kind == ClassKind::Interface {
                    return self.error_here(format!("an {} can't have `{word}` blocks", kind.word()));
                }
                self.advance();
                let inner = if word == "override" { Origin::Override } else { Origin::Implement(self.parse_type()?) };
                if let Origin::Implement(ty) = &inner {
                    if implements.iter().any(|t: &TypeExpr| t.kind == ty.kind) {
                        return Err(Diagnostic::new(format!("`{ty}` has two `implement` blocks: its methods go in one"), ty.span));
                    }
                    implements.push(ty.clone());
                }
                self.expect(&TokenKind::LBrace, &format!("to start the `{word}` block"))?;
                self.members(kind, inner, out, implements)?;
                continue;
            }
            let public = self.eat(&TokenKind::Keyword(Keyword::Pub));
            let is_static = matches!(&self.peek().kind, TokenKind::Ident(w) if w == "static");
            if is_static {
                self.advance();
            }
            if matches!(&self.peek().kind, TokenKind::Ident(w) if w == "new") && self.peek_at(1) == &TokenKind::LParen {
                if is_static {
                    return self.error_here("a constructor can't be `static`");
                }
                if origin != Origin::Own {
                    return self.error_here("the constructor goes in the class itself, not in a block");
                }
                if kind == ClassKind::Interface {
                    return self.error_here("interfaces can't have a constructor, since they don't create objects");
                }
                self.advance();
                let params = self.params()?;
                let body = FunctionBody::Block(self.block()?);
                let decl = FunctionDecl { name: "new".into(), generics: Vec::new(), params, ret: None, body };
                out.push(Member { public, is_static: false, origin: origin.clone(), kind: MemberKind::Constructor(decl), doc: None, span: self.span_from(start) });
                continue;
            }
            let constant = self.eat(&TokenKind::Keyword(Keyword::Const));
            let ty = self.parse_type()?;
            match self.peek().kind.clone() {
                TokenKind::Field(name) | TokenKind::Variable(name) => {
                    let sigil_static = matches!(self.peek().kind, TokenKind::Variable(_));
                    self.advance();
                    if is_static {
                        return self.error_here("fields don't use `static`: a field written with `$` already belongs to the class");
                    }
                    if origin != Origin::Own {
                        return self.error_here("fields go in the class itself, not in a block");
                    }
                    if kind == ClassKind::Interface {
                        return self.error_here("interfaces can't have fields");
                    }
                    let value = if self.eat(&TokenKind::Eq) { Some(self.expression()?) } else { None };
                    self.expect(&TokenKind::Semicolon, "after the field")?;
                    let kind = MemberKind::Field { constant, ty, name, value, flag: None, variadic: false };
                    out.push(Member { public, is_static: sigil_static, origin: origin.clone(), kind, doc: None, span: self.span_from(start) });
                }
                TokenKind::Ident(_) => {
                    if constant {
                        return self.error_here("methods can't be `const`");
                    }
                    let decl;
                    let mut has_body = true;
                    if kind == ClassKind::Interface && self.method_without_body() {
                        let name = self.expect_ident("for the method")?;
                        let generics = self.type_params()?;
                        let params = self.params()?;
                        self.expect(&TokenKind::Semicolon, "after the method")?;
                        decl = FunctionDecl { name, generics, params, ret: Some(ty), body: FunctionBody::Block(Vec::new()) };
                        has_body = false;
                    } else {
                        decl = self.function(ty)?;
                    }
                    let kind = MemberKind::Method { decl, body: has_body };
                    out.push(Member { public, is_static, origin: origin.clone(), kind, doc: None, span: self.span_from(start) });
                }
                other => return self.error_here(format!("expected a field (`@name`, `$name`) or a method name after the type, found {other}")),
            }
        }
    }

    /// Whether the method being read ends with `;` after its parameters, without a body.
    fn method_without_body(&mut self) -> bool {
        let checkpoint = self.checkpoint();
        let result = (|| -> Result<bool> {
            self.expect_ident("")?;
            self.type_params()?;
            self.params()?;
            Ok(self.check(&TokenKind::Semicolon))
        })();
        self.restore(checkpoint);
        result.unwrap_or(false)
    }

    /// `<T, U>` after a name, or nothing.
    fn type_params(&mut self) -> Result<Vec<String>> {
        let mut generics = Vec::new();
        if self.eat(&TokenKind::Lt) {
            loop {
                generics.push(self.expect_ident("for the type parameter")?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect_closing_angle("after the type parameters")?;
        }
        Ok(generics)
    }

    /// `(...)` with the parameters of a function.
    fn params(&mut self) -> Result<Vec<Param>> {
        self.expect(&TokenKind::LParen, "before the parameters")?;
        let mut params: Vec<Param> = Vec::new();
        while !self.check(&TokenKind::RParen) {
            if params.last().is_some_and(|p| p.variadic) {
                return self.error_here("the `...` parameter must be the last one");
            }
            params.push(self.param()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "after the parameters")?;
        Ok(params)
    }

    /// `[$a, $b, ...$rest]`
    fn list_pattern(&mut self) -> Result<(Vec<String>, Option<String>)> {
        self.expect(&TokenKind::LBracket, "to start the list of variables")?;
        let mut names = Vec::new();
        let mut rest = None;
        while !self.check(&TokenKind::RBracket) {
            if self.eat(&TokenKind::Ellipsis) {
                rest = Some(self.expect_variable("after `...`")?);
                self.eat(&TokenKind::Comma);
                if !self.check(&TokenKind::RBracket) {
                    return self.error_here("`...$rest` can only be the last one");
                }
                break;
            }
            names.push(self.expect_variable("in the list of variables")?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "to close the list of variables")?;
        Ok((names, rest))
    }

    fn param(&mut self) -> Result<Param> {
        if let TokenKind::Field(name) = self.peek().kind.clone() {
            // `new(@name)`: stored in the field, with the field's type
            self.advance();
            let default = if self.eat(&TokenKind::Eq) { Some(self.expression()?) } else { None };
            return Ok(Param { name, ty: None, default, variadic: false, field: true });
        }
        let variadic = self.eat(&TokenKind::Ellipsis);
        let ty = if matches!(self.peek().kind, TokenKind::Variable(_)) { None } else { Some(self.parse_type()?) };
        let name = self.expect_variable("for the parameter")?;
        let default = if self.eat(&TokenKind::Eq) {
            if variadic {
                return self.error_here("a `...` parameter can't have a default value");
            }
            Some(self.expression()?)
        } else {
            None
        };
        Ok(Param { name, ty, default, variadic, field: false })
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(&TokenKind::LBrace, "to start the block")?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                let error = Diagnostic::new("expected `}` to close the block, found the end of the file", self.peek().span);
                if self.recover {
                    self.errors.push(error);
                    return Ok(stmts);
                }
                return Err(error);
            }
            let before = self.pos;
            match self.statement() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) if self.recover => self.skip_statement(e),
                Err(e) => return Err(e),
            }
            if self.recover && self.pos == before {
                self.advance();
            }
        }
        self.advance();
        Ok(stmts)
    }

    /// `use a/b/c;`, `use a/b/c as x;`, `use a/b/c::name;`, `use a/b/c::{x, $y as $z};`
    fn use_decl(&mut self) -> Result<UseDecl> {
        self.advance();
        let mut path = vec![self.path_part("after `use`")?];
        while self.eat(&TokenKind::Slash) {
            path.push(self.path_part("in the path of the module")?);
        }
        let start = self.peek().span;
        for part in &path[..path.len() - 1] {
            if part.starts_with(|c: char| c.is_ascii_uppercase()) {
                return Err(Diagnostic::new(
                    format!("`{part}` is a folder, and folders are written in lowercase: only the last part of a path can be a type *(i.e. `people/ChildClass`)*"),
                    start,
                ));
            }
        }
        let last = path.last().unwrap().clone();
        if last.starts_with(|c: char| c.is_ascii_uppercase()) {
            // `use people/ChildClass;` imports the type itself
            if self.check(&TokenKind::ColonColon) {
                return self.error_here(format!("`{last}` is a type, and `use .../{last};` imports it: nothing goes after `::`"));
            }
            let alias = if self.eat_as() { Some(self.expect_ident("after `as`")?) } else { None };
            self.expect(&TokenKind::Semicolon, "after `use`")?;
            let items = UseItems::Items(vec![UseItem { name: last, variable: false, alias }]);
            return Ok(UseDecl { path, items, type_module: true });
        }
        let items = if self.eat(&TokenKind::ColonColon) {
            let mut items = Vec::new();
            if self.eat(&TokenKind::LBrace) {
                while !self.check(&TokenKind::RBrace) {
                    items.push(self.use_item()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RBrace, "after the imported names")?;
            } else {
                items.push(self.use_item()?);
            }
            UseItems::Items(items)
        } else {
            let alias = if self.eat_as() { Some(self.expect_ident("after `as`")?) } else { None };
            UseItems::Module { alias }
        };
        self.expect(&TokenKind::Semicolon, "after `use`")?;
        Ok(UseDecl { path, items, type_module: false })
    }

    /// A folder or file name in a `use` path. Reserved words are allowed,
    /// since they're names of files *(i.e. `use text/case;`)*.
    fn path_part(&mut self, context: &str) -> Result<String> {
        if let TokenKind::Keyword(k) = self.peek().kind {
            self.advance();
            return Ok(k.as_str().to_string());
        }
        self.expect_ident(context)
    }

    fn use_item(&mut self) -> Result<UseItem> {
        let (name, variable) = match self.peek().kind.clone() {
            TokenKind::Variable(name) => (name, true),
            TokenKind::Ident(name) => (name, false),
            other => return self.error_here(format!("expected a function or a `$constant` to import, found {other}")),
        };
        self.advance();
        let alias = if self.eat_as() {
            Some(if variable { self.expect_variable("after `as`")? } else { self.expect_ident("after `as`")? })
        } else {
            None
        };
        Ok(UseItem { name, variable, alias })
    }

    fn eat_as(&mut self) -> bool {
        if matches!(&self.peek().kind, TokenKind::Ident(n) if n == "as") {
            self.advance();
            return true;
        }
        false
    }

    fn try_statement(&mut self) -> Result<StmtKind> {
        let start = self.peek().span;
        self.advance();
        let body = self.block()?;
        let mut catches = Vec::new();
        while self.eat(&TokenKind::Keyword(Keyword::Catch)) {
            self.expect(&TokenKind::LParen, "after `catch`")?;
            let ty = if matches!(self.peek().kind, TokenKind::Variable(_)) { None } else { Some(self.parse_type()?) };
            let name = self.expect_variable("for the exception")?;
            self.expect(&TokenKind::RParen, "after the exception")?;
            catches.push(Catch { ty, name, body: self.block()? });
        }
        let finally = if self.eat(&TokenKind::Keyword(Keyword::Finally)) { Some(self.block()?) } else { None };
        if catches.is_empty() && finally.is_none() {
            return Err(Diagnostic::new("a `try` block needs a `catch` or a `finally`", self.span_from(start)));
        }
        Ok(StmtKind::Try { body, catches, finally })
    }

    fn if_statement(&mut self) -> Result<StmtKind> {
        self.advance();
        self.expect(&TokenKind::LParen, "after `if`")?;
        let cond = self.expression()?;
        self.expect(&TokenKind::RParen, "after the condition")?;
        let then = Box::new(self.statement()?);
        let otherwise = if self.eat(&TokenKind::Keyword(Keyword::Else)) { Some(Box::new(self.statement()?)) } else { None };
        Ok(StmtKind::If { cond, then, otherwise })
    }

    fn for_statement(&mut self) -> Result<StmtKind> {
        self.advance();
        self.expect(&TokenKind::LParen, "after `for`")?;
        if self.check(&TokenKind::LBracket) {
            let (names, rest) = self.list_pattern()?;
            self.expect(&TokenKind::Keyword(Keyword::In), "in the `for`")?;
            let iterable = self.expression()?;
            self.expect(&TokenKind::RParen, "after the `for`")?;
            let body = Box::new(self.statement()?);
            return Ok(StmtKind::For { pattern: ForPattern::List { names, rest }, iterable, body });
        }
        let first = self.expect_variable("in the `for`")?;
        let pattern = if self.eat(&TokenKind::Comma) {
            ForPattern::Indexed { index: first, value: self.expect_variable("after `,`")? }
        } else if self.eat(&TokenKind::FatArrow) {
            ForPattern::KeyValue { key: first, value: self.expect_variable("after `=>`")? }
        } else {
            ForPattern::Value(first)
        };
        self.expect(&TokenKind::Keyword(Keyword::In), "in the `for`")?;
        let iterable = self.expression()?;
        self.expect(&TokenKind::RParen, "after the `for`")?;
        let body = Box::new(self.statement()?);
        Ok(StmtKind::For { pattern, iterable, body })
    }

    // ---- Types ----

    fn parse_type(&mut self) -> Result<TypeExpr> {
        self.guard()?;
        let start = self.peek().span;
        if self.eat(&TokenKind::Question) {
            let inner = self.type_atom()?;
            // `?(number) -> number`: a function type that isn't wrapped in its own parentheses
            if matches!(&inner.kind, TypeKind::Function { ret, .. } if ret.span.end == inner.span.end) {
                return Err(Diagnostic::new(
                    "`?` before a function type is ambiguous. Use `?((number) -> number)` for a nullable function, \
                     or `(number) -> ?number` for a function that returns a nullable value",
                    self.span_from(start),
                ));
            }
            return Ok(TypeExpr { kind: TypeKind::Nullable(Box::new(inner)), span: self.span_from(start) });
        }
        self.type_atom()
    }

    fn type_atom(&mut self) -> Result<TypeExpr> {
        let start = self.peek().span;
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                self.advance();
                // `module::Type`, a type alias from an imported module
                let name = match (&self.peek().kind, self.peek_at(1)) {
                    (TokenKind::ColonColon, TokenKind::Ident(member)) if self.split == 0 => {
                        let member = member.clone();
                        self.advance();
                        self.advance();
                        format!("{name}::{member}")
                    }
                    _ => name,
                };
                let mut args = Vec::new();
                if self.eat(&TokenKind::Lt) {
                    loop {
                        args.push(self.parse_type()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect_closing_angle("to close the type")?;
                }
                Ok(TypeExpr { kind: TypeKind::Named { name, args }, span: self.span_from(start) })
            }
            TokenKind::LParen => {
                self.advance();
                let mut params = Vec::new();
                let mut variadic = false;
                while !self.check(&TokenKind::RParen) {
                    if variadic {
                        return self.error_here("the `...` parameter must be the last one");
                    }
                    variadic = self.eat(&TokenKind::Ellipsis);
                    params.push(self.parse_type()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen, "in the type")?;
                if self.eat(&TokenKind::Arrow) {
                    let ret = Box::new(self.parse_type()?);
                    return Ok(TypeExpr { kind: TypeKind::Function { params, variadic, ret }, span: self.span_from(start) });
                }
                // `(T)`: parentheses for readability
                if params.len() == 1 && !variadic {
                    let mut inner = params.pop().unwrap();
                    inner.span = self.span_from(start);
                    return Ok(inner);
                }
                self.error_here("expected `->` after the parameter types of a function type")
            }
            other => self.error_here(format!("expected a type, found {other}")),
        }
    }

    // ---- Expressions ----

    fn expression(&mut self) -> Result<Expr> {
        self.guard()?;
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr> {
        self.guard()?;
        let start = self.peek().span;
        let target = self.ternary()?;
        let op = match self.peek().kind {
            TokenKind::Eq => None,
            TokenKind::PlusEq => Some(BinaryOp::Add),
            TokenKind::MinusEq => Some(BinaryOp::Sub),
            TokenKind::StarEq => Some(BinaryOp::Mul),
            TokenKind::SlashEq => Some(BinaryOp::Div),
            TokenKind::PercentEq => Some(BinaryOp::Rem),
            TokenKind::StarStarEq => Some(BinaryOp::Pow),
            TokenKind::AmpEq => Some(BinaryOp::BitAnd),
            TokenKind::PipeEq => Some(BinaryOp::BitOr),
            TokenKind::CaretEq => Some(BinaryOp::BitXor),
            TokenKind::ShlEq => Some(BinaryOp::Shl),
            TokenKind::ShrEq => Some(BinaryOp::Shr),
            TokenKind::UShrEq => Some(BinaryOp::UShr),
            _ => return Ok(target),
        };
        if !is_assignable(&target) {
            return self.error_here("only variables and indexes can be assigned to *(i.e. `$x = 1`, `$list[0] = 1`)*");
        }
        self.advance();
        let value = self.assignment()?;
        Ok(Expr {
            kind: ExprKind::Assign { op, target: Box::new(target), value: Box::new(value) },
            span: self.span_from(start),
        })
    }

    fn ternary(&mut self) -> Result<Expr> {
        self.guard()?;
        let start = self.peek().span;
        let cond = self.binary_expr(PREC_COALESCE)?;
        if !self.eat(&TokenKind::Question) {
            return Ok(cond);
        }
        let then = self.ternary()?;
        self.expect(&TokenKind::Colon, "in the ternary operator")?;
        let otherwise = self.ternary()?;
        Ok(Expr {
            kind: ExprKind::Ternary { cond: Box::new(cond), then: Box::new(then), otherwise: Box::new(otherwise) },
            span: self.span_from(start),
        })
    }

    fn binary(&self, op: BinaryOp, left: Expr, right: Expr, start: Span) -> Expr {
        Expr { kind: ExprKind::Binary { op, left: Box::new(left), right: Box::new(right) }, span: self.span_from(start) }
    }

    /// Every binary operator from `??` to `*`, as one loop with precedence
    /// numbers (a Pratt parser) instead of one function per level, so nested
    /// code uses little stack. `min` is the lowest precedence it may take.
    fn binary_expr(&mut self, min: u8) -> Result<Expr> {
        self.guard()?;
        let start = self.peek().span;
        let mut left = self.unary()?;
        loop {
            let Some((prec, infix)) = infix_op(&self.peek().kind) else { break };
            if prec < min {
                break;
            }
            self.advance();
            match infix {
                // `a..b` and `a..=b`. The end can be left out where nothing could
                // follow, like in `$text[-3..]`.
                Infix::Range(inclusive) => {
                    let end = if can_start_expression(&self.peek().kind) { Some(Box::new(self.binary_expr(PREC_RANGE + 1)?)) } else { None };
                    if end.is_none() && inclusive {
                        return self.error_here("`..=` needs an end *(i.e. `1..=5`)*");
                    }
                    left = Expr { kind: ExprKind::Range { start: Box::new(left), end, inclusive }, span: self.span_from(start) };
                    if matches!(self.peek().kind, TokenKind::DotDot | TokenKind::DotDotEq) {
                        return self.error_here("ranges can't be chained");
                    }
                }
                Infix::Op(op) => {
                    // `??` groups from the right, the rest from the left
                    let next = if prec == PREC_COALESCE { prec } else { prec + 1 };
                    let right = self.binary_expr(next)?;
                    left = self.binary(op, left, right, start);
                    if prec == PREC_RELATIONAL && infix_op(&self.peek().kind).is_some_and(|(p, _)| p == PREC_RELATIONAL) {
                        return self.error_here(format!(
                            "comparisons can't be chained: use `&&` *(i.e. `$a < $b && $b < $c`)*, found {}",
                            self.peek().kind
                        ));
                    }
                }
            }
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr> {
        self.guard()?;
        let start = self.peek().span;
        let op = match &self.peek().kind {
            TokenKind::Minus => UnaryOp::Neg,
            TokenKind::Plus => UnaryOp::Plus,
            TokenKind::Tilde => UnaryOp::BitNot,
            TokenKind::Bang => UnaryOp::Not,
            TokenKind::PlusPlus | TokenKind::MinusMinus => {
                let delta = if self.check(&TokenKind::PlusPlus) { 1 } else { -1 };
                self.advance();
                let target = self.unary()?;
                if !is_assignable(&target) {
                    return Err(Diagnostic::new("`++` and `--` only work on variables and indexes", target.span));
                }
                return Ok(Expr {
                    kind: ExprKind::Increment { target: Box::new(target), delta, prefix: true },
                    span: self.span_from(start),
                });
            }
            TokenKind::TestOp(name) if path_test_op(name).is_some() => return self.path_test(),
            _ => return self.power(),
        };
        self.advance();
        // `!-f $dir + '/x'` negates the whole test
        let operand = match &self.peek().kind {
            TokenKind::TestOp(name) if op == UnaryOp::Not && path_test_op(name).is_some() => self.path_test()?,
            _ => self.unary()?,
        };
        Ok(Expr { kind: ExprKind::Unary { op, operand: Box::new(operand) }, span: self.span_from(start) })
    }

    /// `-f path`. The path takes everything up to the comparison operators.
    fn path_test(&mut self) -> Result<Expr> {
        let start = self.peek().span;
        let TokenKind::TestOp(name) = self.advance().kind else { unreachable!() };
        let op = path_test_op(&name).unwrap();
        let path = self.binary_expr(PREC_RANGE)?;
        Ok(Expr { kind: ExprKind::PathTest { op, path: Box::new(path) }, span: self.span_from(start) })
    }

    /// `**` groups from the right, and its right side can start with a unary
    /// operator: `2 ** -1`.
    fn power(&mut self) -> Result<Expr> {
        let start = self.peek().span;
        let base = self.postfix()?;
        if !self.eat(&TokenKind::StarStar) {
            return Ok(base);
        }
        let exponent = self.unary()?;
        Ok(self.binary(BinaryOp::Pow, base, exponent, start))
    }

    fn postfix(&mut self) -> Result<Expr> {
        let start = self.peek().span;
        let mut expr = self.primary()?;
        loop {
            // While recovering, a `(` or `[` that starts a line is the next code, not a call or an index
            let new_line = self.peek().span.line > self.last_span.line;
            if self.recover && self.hole == Some(self.pos) {
                return Ok(expr);
            }
            if self.recover && new_line && matches!(self.peek().kind, TokenKind::LParen | TokenKind::LBracket | TokenKind::QuestionBracket) {
                return Ok(expr);
            }
            let kind = match self.peek().kind {
                TokenKind::Dot | TokenKind::QuestionDot => {
                    let dot = self.advance();
                    let nullable = dot.kind == TokenKind::QuestionDot;
                    // Nothing after the `.` yet, or only on the next line: a name being written
                    let named = matches!(self.peek().kind, TokenKind::Ident(_) | TokenKind::Keyword(_)) && self.peek().span.line == dot.span.line;
                    if !named && self.note_error(format!("expected a field or a method after `.`, found {}", self.peek().kind), dot.span) {
                        // Nothing goes on from a name being written: what's after it is the next code
                        let kind = ExprKind::Field { target: Box::new(expr), name: String::new() };
                        self.hole = Some(self.pos);
                        return Ok(Expr { kind, span: self.span_from(start) });
                    } else if matches!(self.peek().kind, TokenKind::Ident(_)) && self.peek_at(1) != &TokenKind::LParen {
                        // `$point.x`: a field
                        if nullable {
                            return self.error_here("`?.` doesn't work with fields yet: check for `null` first");
                        }
                        let name = self.expect_ident("after `.`")?;
                        ExprKind::Field { target: Box::new(expr), name }
                    } else {
                        self.method(expr, nullable)?
                    }
                }
                TokenKind::LParen => {
                    let args = self.arguments()?;
                    ExprKind::Call { callee: Box::new(expr), args }
                }
                TokenKind::LBracket | TokenKind::QuestionBracket => {
                    let nullable = self.advance().kind == TokenKind::QuestionBracket;
                    let index = self.expression()?;
                    self.expect(&TokenKind::RBracket, "after the index")?;
                    ExprKind::Index { target: Box::new(expr), index: Box::new(index), nullable }
                }
                TokenKind::PlusPlus | TokenKind::MinusMinus => {
                    if !is_assignable(&expr) {
                        return self.error_here("`++` and `--` only work on variables and indexes");
                    }
                    let delta = if self.advance().kind == TokenKind::PlusPlus { 1 } else { -1 };
                    ExprKind::Increment { target: Box::new(expr), delta, prefix: false }
                }
                _ => return Ok(expr),
            };
            expr = Expr { kind, span: self.span_from(start) };
        }
    }

    /// After `.` or `?.`: a method call, `.as(type)` or `.is(type)`.
    fn method(&mut self, receiver: Expr, nullable: bool) -> Result<ExprKind> {
        // Reserved words can be method names *(i.e. `$result.throw()`)*
        let name = match self.peek().kind.clone() {
            TokenKind::Keyword(k) => {
                self.advance();
                k.as_str().to_string()
            }
            _ => self.expect_ident("after `.`")?,
        };
        if !self.check(&TokenKind::LParen) {
            return self.error_here(format!("methods are always called with parentheses: `.{name}()`"));
        }
        if name == "as" || name == "is" {
            self.advance();
            let ty = self.parse_type()?;
            self.expect(&TokenKind::RParen, &format!("after the type in `.{name}()`"))?;
            let check = if name == "as" { TypeCheckKind::As } else { TypeCheckKind::Is };
            return Ok(ExprKind::TypeCheck { receiver: Box::new(receiver), ty, check, nullable });
        }
        let args = self.arguments()?;
        Ok(ExprKind::MethodCall { receiver: Box::new(receiver), method: name, args, nullable })
    }

    fn arguments(&mut self) -> Result<Vec<Arg>> {
        self.expect(&TokenKind::LParen, "before the arguments")?;
        let mut args: Vec<Arg> = Vec::new();
        while !self.check(&TokenKind::RParen) {
            let arg = if self.eat(&TokenKind::Ellipsis) {
                Arg { name: None, value: self.expression()?, spread: true }
            } else if matches!(self.peek().kind, TokenKind::Ident(_)) && self.peek_at(1) == &TokenKind::Colon {
                let name = self.expect_ident("for the argument")?;
                self.advance();
                Arg { name: Some(name), value: self.expression()?, spread: false }
            } else {
                Arg { name: None, value: self.expression()?, spread: false }
            };
            if arg.name.is_none() && args.iter().any(|a| a.name.is_some()) {
                return Err(Diagnostic::new("positional arguments go before named ones", arg.value.span));
            }
            args.push(arg);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "after the arguments")?;
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr> {
        let start = self.peek().span;
        let token = self.peek().clone();
        let kind = match token.kind {
            TokenKind::Int(n) => {
                self.advance();
                ExprKind::Int(n)
            }
            TokenKind::Float(n) => {
                self.advance();
                ExprKind::Float(n)
            }
            TokenKind::Str(s) => {
                self.advance();
                ExprKind::Str(s)
            }
            TokenKind::FStr(parts) => {
                self.advance();
                ExprKind::Interpolated(string_parts(parts)?)
            }
            TokenKind::Command(parts) => {
                self.advance();
                ExprKind::Command(string_parts(parts)?)
            }
            TokenKind::Regex { pattern, flags } => {
                self.advance();
                ExprKind::Regex { pattern, flags }
            }
            TokenKind::Symbol(name) => {
                self.advance();
                ExprKind::Symbol(name)
            }
            TokenKind::WordList(words) => {
                self.advance();
                let items = words.into_iter().map(|w| Expr { kind: ExprKind::Str(w), span: start }).collect();
                ExprKind::List { elem_type: None, items }
            }
            TokenKind::SymbolList(words) => {
                self.advance();
                let items = words.into_iter().map(|w| Expr { kind: ExprKind::Symbol(w), span: start }).collect();
                ExprKind::List { elem_type: None, items }
            }
            TokenKind::Keyword(Keyword::True) => {
                self.advance();
                ExprKind::Bool(true)
            }
            TokenKind::Keyword(Keyword::False) => {
                self.advance();
                ExprKind::Bool(false)
            }
            TokenKind::Keyword(Keyword::Null) => {
                self.advance();
                ExprKind::Null
            }
            TokenKind::Variable(name) => {
                self.advance();
                ExprKind::Variable(name)
            }
            TokenKind::At => {
                self.advance();
                ExprKind::This
            }
            TokenKind::Field(name) => {
                self.advance();
                if name == "super" {
                    if self.check(&TokenKind::LParen) {
                        ExprKind::SuperNew(self.arguments()?)
                    } else if self.eat(&TokenKind::Dot) {
                        let method = self.expect_ident("after `@super.`")?;
                        if !self.check(&TokenKind::LParen) {
                            return self.error_here(format!("`@super` only calls methods: `@super.{method}()`"));
                        }
                        ExprKind::SuperMethod { method, args: self.arguments()? }
                    } else {
                        return self.error_here("`@super` is followed by `(...)` for the superclass's constructor, or by `.method()`");
                    }
                } else if self.check(&TokenKind::LParen) {
                    let this = Expr { kind: ExprKind::This, span: start };
                    ExprKind::MethodCall { receiver: Box::new(this), method: name, args: self.arguments()?, nullable: false }
                } else {
                    ExprKind::Field { target: Box::new(Expr { kind: ExprKind::This, span: start }), name }
                }
            }
            TokenKind::Ident(name) if is_type_name(&name) && self.peek_at(1) == &TokenKind::Lt => {
                // `Stack<string>()`: the types of a generic class, before calling it
                let checkpoint = self.checkpoint();
                match self.parse_type() {
                    Ok(TypeExpr { kind: TypeKind::Named { args, .. }, .. }) if self.check(&TokenKind::LParen) => ExprKind::TypeArgs { name, args },
                    _ => {
                        self.restore(checkpoint);
                        self.advance();
                        ExprKind::Function(name)
                    }
                }
            }
            TokenKind::Ident(name) if is_type_name(&name) && self.peek_at(1) == &TokenKind::ColonColon => {
                self.advance();
                self.advance();
                match self.peek().kind.clone() {
                    TokenKind::Variable(member) => {
                        self.advance();
                        ExprKind::Static { class: name, name: member, variable: true }
                    }
                    TokenKind::Ident(member) => {
                        self.advance();
                        ExprKind::Static { class: name, name: member, variable: false }
                    }
                    other => {
                        if !self.note_error(format!("expected a method, a value or a `$field` after `{name}::`, found {other}"), self.last_span) {
                            return self.error_here(format!("expected a method, a value or a `$field` after `{name}::`, found {other}"));
                        }
                        ExprKind::Static { class: name, name: String::new(), variable: false }
                    }
                }
            }
            TokenKind::Ident(name) => {
                self.advance();
                if self.eat(&TokenKind::ColonColon) {
                    match self.peek().kind.clone() {
                        TokenKind::Variable(member) => {
                            self.advance();
                            ExprKind::ModuleConstant { module: name, name: member }
                        }
                        TokenKind::Ident(member) => {
                            self.advance();
                            ExprKind::ModuleFunction { module: name, name: member }
                        }
                        other => {
                            if !self.note_error(format!("expected a function or a `$constant` after `{name}::`, found {other}"), self.last_span) {
                                return self.error_here(format!("expected a function or a `$constant` after `{name}::`, found {other}"));
                            }
                            ExprKind::ModuleFunction { module: name, name: String::new() }
                        }
                    }
                } else {
                    ExprKind::Function(name)
                }
            }
            TokenKind::LParen if self.lambda_ahead() => {
                let params = self.params()?;
                let body = if self.eat(&TokenKind::Arrow) {
                    FunctionBody::Expr(self.expression()?)
                } else if self.check(&TokenKind::Eq) {
                    return self.error_here("anonymous functions use `->` before their value *(i.e. `($x) -> $x + 1`)*; `=` is only for named functions");
                } else {
                    FunctionBody::Block(self.block()?)
                };
                let decl = FunctionDecl { name: "anonymous".into(), generics: Vec::new(), params, ret: None, body };
                ExprKind::Lambda(Rc::new(decl))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.expression()?;
                self.expect(&TokenKind::RParen, "to close the parentheses")?;
                return Ok(Expr { kind: inner.kind, span: self.span_from(start) });
            }
            TokenKind::LBracket => {
                self.advance();
                ExprKind::List { elem_type: None, items: self.list_items()? }
            }
            TokenKind::LBrace => {
                self.advance();
                ExprKind::Map { types: None, entries: self.map_entries()? }
            }
            TokenKind::Lt => self.typed_collection()?,
            TokenKind::Keyword(Keyword::If) => self.if_expression()?,
            TokenKind::Keyword(Keyword::Switch) => ExprKind::Switch(Box::new(self.switch(true)?)),
            TokenKind::Keyword(Keyword::Try) => {
                self.advance();
                // `try` applies to the whole expression after it
                ExprKind::Try(Box::new(self.expression()?))
            }
            TokenKind::Keyword(Keyword::Throw) => {
                self.advance();
                // Like `try`, `throw` applies to the whole expression after it
                ExprKind::Throw(Box::new(self.expression()?))
            }
            other => return self.error_here(format!("expected an expression, found {other}")),
        };
        Ok(Expr { kind, span: self.span_from(start) })
    }

    /// `(...)` followed by `{` or `->` is an anonymous function. `=` is
    /// matched too, only to explain that anonymous functions use `->`. In a
    /// `case` value, `(...) ->` is the value and the arrow of the case.
    fn lambda_ahead(&self) -> bool {
        let mut depth = 0usize;
        for (i, token) in self.tokens.iter().enumerate().skip(self.pos) {
            match token.kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace | TokenKind::QuestionBracket => depth += 1,
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        let next = self.tokens.get(i + 1).map(|t| &t.kind);
                        return match next {
                            Some(TokenKind::LBrace | TokenKind::Eq) => true,
                            Some(TokenKind::Arrow) => !self.in_case_pattern,
                            _ => false,
                        };
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
        }
        false
    }

    /// `if (cond) a else b` used as a value. The `else` is required.
    fn if_expression(&mut self) -> Result<ExprKind> {
        self.advance();
        self.expect(&TokenKind::LParen, "after `if`")?;
        let cond = self.expression()?;
        self.expect(&TokenKind::RParen, "after the condition")?;
        let then = self.branch()?;
        if !self.eat(&TokenKind::Keyword(Keyword::Else)) {
            return self.error_here("an `if` used as a value needs an `else`, so it always has a value");
        }
        let otherwise = self.branch()?;
        Ok(ExprKind::If { cond: Box::new(cond), then: Box::new(then), otherwise: Box::new(otherwise) })
    }

    fn branch(&mut self) -> Result<Branch> {
        if self.check(&TokenKind::LBrace) {
            let start = self.peek().span;
            let stmts = self.block()?;
            return Ok(Branch::Block(stmts, self.span_from(start)));
        }
        Ok(Branch::Expr(self.expression()?))
    }

    /// `switch (...) { case ... -> ... default -> ... }`. As a value, every
    /// body gives a value, and it needs a `default`.
    fn switch(&mut self, as_value: bool) -> Result<Switch> {
        let start = self.peek().span;
        self.advance();
        self.expect(&TokenKind::LParen, "after `switch`")?;
        let subject = self.expression()?;
        self.expect(&TokenKind::RParen, "after the value of the `switch`")?;
        self.expect(&TokenKind::LBrace, "to start the cases")?;
        let mut cases = Vec::new();
        let mut default = None;
        loop {
            match self.peek().kind {
                TokenKind::RBrace => {
                    self.advance();
                    break;
                }
                TokenKind::Keyword(Keyword::Case) => {
                    self.advance();
                    let mut patterns = vec![self.pattern()?];
                    while self.eat(&TokenKind::Comma) {
                        patterns.push(self.pattern()?);
                    }
                    self.expect(&TokenKind::Arrow, "after the case")?;
                    let body = self.case_body(as_value)?;
                    cases.push(Case { patterns, body });
                }
                TokenKind::Keyword(Keyword::Default) => {
                    if default.is_some() {
                        return self.error_here("a `switch` can only have one `default`");
                    }
                    self.advance();
                    self.expect(&TokenKind::Arrow, "after `default`")?;
                    default = Some(self.case_body(as_value)?);
                }
                _ => return self.error_here(format!("expected `case`, `default` or `}}`, found {}", self.peek().kind)),
            }
        }
        // A `switch` on an enum can have a case for every value instead, which the checker sees
        // or of a command's subcommands *(i.e. `case Git::Push ->`)*
        let enum_cases = cases.iter().all(|c| {
            c.patterns.iter().all(|p| match p {
                Pattern::Value(Expr { kind: ExprKind::Static { variable: false, .. }, .. }) => true,
                Pattern::Type(TypeExpr { kind: TypeKind::Named { name, .. }, .. }) => name.split_once("::").is_some_and(|(l, _)| is_type_name(l)),
                _ => false,
            })
        });
        if as_value && default.is_none() && !covers_both_bools(&cases) && !(enum_cases && !cases.is_empty()) {
            return Err(Diagnostic::new(
                "a `switch` used as a value must match every value: add a `default`",
                self.span_from(start),
            ));
        }
        Ok(Switch { subject, cases, default })
    }

    /// A case value, or a type *(i.e. `case string ->`)*. A name without `$`
    /// followed by `,` or `->` is a type.
    fn pattern(&mut self) -> Result<Pattern> {
        if matches!(self.peek().kind, TokenKind::Ident(_) | TokenKind::Question) {
            let checkpoint = self.checkpoint();
            if let Ok(ty) = self.parse_type() {
                // `Color::RED` is a value of an enum, and `shapes::Point` a type of a module
                // `Git::Commit`, a subcommand, is a type: values are in SCREAMING_CASE
                let enum_value = matches!(&ty.kind, TypeKind::Named { name, .. } if name.rsplit_once("::").is_some_and(|(left, right)| {
                    is_type_name(left) && !right.chars().any(|c| c.is_ascii_lowercase())
                }));
                if !enum_value && matches!(self.peek().kind, TokenKind::Comma | TokenKind::Arrow) {
                    if matches!(ty.kind, TypeKind::Function { .. }) {
                        return Err(Diagnostic::new("function types can't be used in a `case`", ty.span));
                    }
                    return Ok(Pattern::Type(ty));
                }
            }
            self.restore(checkpoint);
        }
        let outer = std::mem::replace(&mut self.in_case_pattern, true);
        let value = self.expression();
        self.in_case_pattern = outer;
        Ok(Pattern::Value(value?))
    }

    fn case_body(&mut self, as_value: bool) -> Result<CaseBody> {
        if as_value {
            let branch = self.branch()?;
            if matches!(branch, Branch::Expr(_)) {
                self.expect(&TokenKind::Semicolon, "after the value of the case")?;
            }
            return Ok(CaseBody::Value(branch));
        }
        Ok(CaseBody::Stmt(Box::new(self.statement()?)))
    }

    /// The elements after `[`, up to and including `]`.
    fn list_items(&mut self) -> Result<Vec<Expr>> {
        let mut items = Vec::new();
        while !self.check(&TokenKind::RBracket) {
            items.push(self.expression()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBracket, "to close the list")?;
        Ok(items)
    }

    /// The entries after `{`, up to and including `}`.
    fn map_entries(&mut self) -> Result<Vec<(Expr, Expr)>> {
        let mut entries = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let key = self.expression()?;
            self.expect(&TokenKind::FatArrow, "between the key and the value")?;
            let value = self.expression()?;
            entries.push((key, value));
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace, "to close the map")?;
        Ok(entries)
    }

    /// `<string>[...]` or `<string, number>{...}`.
    fn typed_collection(&mut self) -> Result<ExprKind> {
        self.advance();
        let mut types = vec![self.parse_type()?];
        while self.eat(&TokenKind::Comma) {
            types.push(self.parse_type()?);
        }
        self.expect_closing_angle("after the types")?;
        match (types.len(), &self.peek().kind) {
            (1, TokenKind::LBracket) => {
                self.advance();
                Ok(ExprKind::List { elem_type: types.pop(), items: self.list_items()? })
            }
            (2, TokenKind::LBrace) => {
                self.advance();
                let value = types.pop().unwrap();
                let key = types.pop().unwrap();
                Ok(ExprKind::Map { types: Some((key, value)), entries: self.map_entries()? })
            }
            (1, _) => self.error_here("expected `[` after the type of the list *(i.e. `<string>[]`)*"),
            (2, _) => self.error_here("expected `{` after the types of the map *(i.e. `<string, number>{}`)*"),
            _ => self.error_here("a list takes one type and a map takes two"),
        }
    }
}

/// Parses the expressions inside `f"..."` and `%x"..."`.
fn string_parts(parts: Vec<TokenPart>) -> Result<Vec<StringPart>> {
    parts
        .into_iter()
        .map(|part| match part {
            TokenPart::Text(text) => Ok(StringPart::Text(text)),
            TokenPart::Code(tokens) => {
                let mut parser = Parser::new(tokens);
                let expr = parser.expression()?;
                parser.expect(&TokenKind::Eof, "at the end of the interpolated expression")?;
                Ok(StringPart::Expr(expr))
            }
        })
        .collect()
}

fn covers_both_bools(cases: &[Case]) -> bool {
    let has = |b: bool| {
        cases.iter().any(|c| c.patterns.iter().any(|p| matches!(p, Pattern::Value(Expr { kind: ExprKind::Bool(v), .. }) if *v == b)))
    };
    has(true) && has(false)
}

fn is_assignable(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::Variable(_) | ExprKind::Index { nullable: false, .. } | ExprKind::Field { .. } | ExprKind::Static { variable: true, .. }
    )
}

/// Types, and so classes, are written in PascalCase *(i.e. `Point::origin()`)*,
/// and modules in lowercase *(i.e. `paths::load()`)*.
fn pascal(name: &str) -> String {
    name.split('_').filter(|w| !w.is_empty()).map(|w| {
        let mut c = w.chars();
        c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
    }).collect()
}

pub fn is_type_name(name: &str) -> bool {
    name == "Self" || name.starts_with(|c: char| c.is_ascii_uppercase())
}

const PREC_COALESCE: u8 = 1;
const PREC_RELATIONAL: u8 = 8;
const PREC_RANGE: u8 = 9;

enum Infix {
    Op(BinaryOp),
    /// `..` (`false`) or `..=` (`true`)
    Range(bool),
}

/// The binary operators and their precedence, from `??` (1) to `*` (12).
/// The order follows the table in `ideas/language/operators.md`.
fn infix_op(kind: &TokenKind) -> Option<(u8, Infix)> {
    use BinaryOp::*;
    let (prec, op) = match kind {
        TokenKind::QuestionQuestion => (PREC_COALESCE, Coalesce),
        TokenKind::OrOr => (2, Or),
        TokenKind::AndAnd => (3, And),
        TokenKind::Pipe => (4, BitOr),
        TokenKind::Caret => (5, BitXor),
        TokenKind::Amp => (6, BitAnd),
        TokenKind::EqEq => (7, Eq),
        TokenKind::NotEq => (7, NotEq),
        TokenKind::DotDot => return Some((PREC_RANGE, Infix::Range(false))),
        TokenKind::DotDotEq => return Some((PREC_RANGE, Infix::Range(true))),
        TokenKind::Shl => (10, Shl),
        TokenKind::Shr => (10, Shr),
        TokenKind::UShr => (10, UShr),
        TokenKind::Plus => (11, Add),
        TokenKind::Minus => (11, Sub),
        TokenKind::Star => (12, Mul),
        TokenKind::Slash => (12, Div),
        TokenKind::Percent => (12, Rem),
        other => return relational_op(other).map(|op| (PREC_RELATIONAL, Infix::Op(op))),
    };
    Some((prec, Infix::Op(op)))
}

fn relational_op(kind: &TokenKind) -> Option<BinaryOp> {
    Some(match kind {
        TokenKind::Lt => BinaryOp::Lt,
        TokenKind::Gt => BinaryOp::Gt,
        TokenKind::LtEq => BinaryOp::LtEq,
        TokenKind::GtEq => BinaryOp::GtEq,
        TokenKind::TestOp(name) => match name.as_str() {
            "like" => BinaryOp::Like,
            "match" => BinaryOp::Match,
            "in" => BinaryOp::In,
            "notlike" => BinaryOp::NotLike,
            "notmatch" => BinaryOp::NotMatch,
            "notin" => BinaryOp::NotIn,
            _ => return None,
        },
        _ => return None,
    })
}

fn path_test_op(name: &str) -> Option<PathTestOp> {
    Some(match name {
        "f" => PathTestOp::File,
        "d" => PathTestOp::Dir,
        "x" => PathTestOp::Exec,
        "r" => PathTestOp::Read,
        "w" => PathTestOp::Write,
        _ => return None,
    })
}

fn can_start_expression(kind: &TokenKind) -> bool {
    use TokenKind::*;
    match kind {
        Int(_) | Float(_) | Str(_) | FStr(_) | Regex { .. } | Command(_) | WordList(_) | SymbolList(_) | Symbol(_)
        | Variable(_) | Ident(_) | LParen | LBracket | LBrace | Lt | Minus | Plus | Bang | Tilde | PlusPlus
        | MinusMinus => true,
        Keyword(k) => matches!(k, self::Keyword::True | self::Keyword::False | self::Keyword::Null | self::Keyword::If | self::Keyword::Switch | self::Keyword::Try),
        TestOp(name) => path_test_op(name).is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn stmts(source: &str) -> String {
        let stmts = parse(tokenize(source).unwrap()).unwrap_or_else(|e| panic!("{}: {source}", e.message));
        stmts.iter().map(|s| s.to_string()).collect::<Vec<_>>().join("\n")
    }

    fn expr(source: &str) -> String {
        stmts(&format!("{source};"))
    }

    fn error(source: &str) -> String {
        parse(tokenize(source).unwrap()).unwrap_err().message
    }

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(expr("1 + 2 * 3"), "(+ 1 (* 2 3))");
        assert_eq!(expr("(1 + 2) * 3"), "(* (+ 1 2) 3)");
        assert_eq!(expr("10 - 4 - 3"), "(- (- 10 4) 3)");
        assert_eq!(expr("2 ** 3 ** 2"), "(** 2 (** 3 2))");
        assert_eq!(expr("-2 ** 2"), "(- (** 2 2))");
        assert_eq!(expr("2 ** -1"), "(** 2 (- 1))");
        assert_eq!(expr("1 << 2 + 3"), "(<< 1 (+ 2 3))");
    }

    #[test]
    fn logic_and_comparison() {
        assert_eq!(expr("$a > 1 && $b < 2 || !$c"), "(|| (&& (> $a 1) (< $b 2)) (! $c))");
        assert_eq!(expr("$a == 1 & $b != 2"), "(& (== $a 1) (!= $b 2))");
        assert_eq!(expr("$a | $b ^ $c & $d"), "(| $a (^ $b (& $c $d)))");
        assert!(error("$a < $b < $c;").contains("can't be chained"));
    }

    #[test]
    fn null_operators_and_ternary() {
        assert_eq!(expr("$a ?? $b ?? 'c'"), "(?? $a (?? $b \"c\"))");
        assert_eq!(expr("$n > 1 ? 'items' : 'item'"), "(? (> $n 1) \"items\" \"item\")");
        assert_eq!(expr("$a ? $b : $c ? 1 : 2"), "(? $a $b (? $c 1 2))");
        assert_eq!(expr("$user?.name()"), "(?.name $user)");
        assert_eq!(expr("$d?['user']?['name']"), "(?[] (?[] $d \"user\") \"name\")");
    }

    #[test]
    fn assignment() {
        assert_eq!(expr("$a = $b = 1"), "(= $a (= $b 1))");
        assert_eq!(expr("$x += 5"), "(+= $x 5)");
        assert_eq!(expr("$list[0] **= 2"), "(**= ([] $list 0) 2)");
        assert_eq!(expr("$x++"), "(++post $x)");
        assert_eq!(expr("--$x"), "(--pre $x)");
        assert!(error("1 = 2;").contains("only variables"));
        assert!(error("$a?['x'] = 1;").contains("only variables"));
        assert!(error("5++;").contains("only work on variables"));
    }

    #[test]
    fn ranges() {
        assert_eq!(expr("1..5"), "(.. 1 5)");
        assert_eq!(expr("1..=$n + 1"), "(..= 1 (+ $n 1))");
        assert_eq!(expr("$s[-3..]"), "([] $s (.. (- 3) _))");
        assert_eq!(expr("(1..10).step(2)"), "(.step (.. 1 10) 2)");
        assert!(error("1..5..10;").contains("can't be chained"));
    }

    #[test]
    fn test_operators() {
        assert_eq!(expr("$file -like '*.txt' && -f $file"), "(&& (-like $file \"*.txt\") (-f $file))");
        assert_eq!(expr("-f $dir + '/config.txt'"), "(-f (+ $dir \"/config.txt\"))");
        assert_eq!(expr("!-f $dir + '/x'"), "(! (-f (+ $dir \"/x\")))");
        assert_eq!(expr("'a' -notin $names"), "(-notin \"a\" $names)");
        assert_eq!(expr("-d $p == false"), "(== (-d $p) false)");
    }

    #[test]
    fn calls_and_methods() {
        assert_eq!(expr("greet('Folgue')"), "(call greet \"Folgue\")");
        assert_eq!(expr("greet"), "greet");
        assert_eq!(expr("createFile('d', extension: 'jpg')"), "(call createFile \"d\" (extension: \"jpg\"))");
        assert_eq!(expr("greet(...$names)"), "(call greet (... $names))");
        assert_eq!(expr("$n.even?().toString()"), "(.toString (.even? $n))");
        assert_eq!(expr("5.even?()"), "(.even? 5)");
        assert_eq!(expr("$data['name'].as(string).upper()"), "(.upper (.as ([] $data \"name\") string))");
        assert_eq!(expr("$v.is(list<map<string, any>>)"), "(.is $v list<map<string, any>>)");
        assert_eq!(expr("$list.length"), "(. $list length)");
        assert_eq!(expr("@name"), "@name");
        assert_eq!(expr("@greet(1)"), "(.greet @ 1)");
        assert_eq!(expr("Point::origin()"), "(call Point::origin)");
        assert_eq!(expr("Self::$count"), "Self::$count");
        assert_eq!(expr("@super(1)"), "(@super 1)");
        assert_eq!(expr("@super.hi()"), "(@super.hi)");
        assert!(error("f(a: 1, 2);").contains("positional arguments go before"));
    }

    #[test]
    fn collections() {
        assert_eq!(expr("[1, 2, 3,]"), "(list 1 2 3)");
        assert_eq!(expr("<string>[]"), "(list<string>)");
        // At the start of a statement `{` is a block, so maps are tested after `=`
        assert_eq!(stmts("let $m = {'a' => 1, :b => [2],};"), "(let $m (map (\"a\" 1) (:b (list 2))))");
        assert_eq!(stmts("{ outln(1); }"), "{ (call outln 1) }");
        assert_eq!(expr("<string, number>{}"), "(map<string, number>)");
        assert_eq!(expr("<list<string>>[]"), "(list<list<string>>)");
        assert_eq!(expr("%w[a b]"), "(list \"a\" \"b\")");
        assert_eq!(expr("%i[a b]"), "(list :a :b)");
    }

    #[test]
    fn strings() {
        assert_eq!(expr("f\"Hello {$name.upper()}!\""), "(f \"Hello \" (.upper $name) \"!\")");
        assert_eq!(expr("%x\"rm {$file}\""), "(x \"rm \" $file)");
        assert_eq!(expr("%r\"^\\d+$\"i"), "(regex \"^\\\\d+$\" \"i\")");
    }

    #[test]
    fn declarations() {
        assert_eq!(stmts("let $age = 5;"), "(let $age 5)");
        assert_eq!(stmts("const $city = 'Madrid';"), "(const $city \"Madrid\")");
        assert_eq!(stmts("?string $nick = null;"), "(let ?string $nick null)");
        assert_eq!(stmts("const list<any> $mixed = [];"), "(const list<any> $mixed (list))");
        assert_eq!(stmts("map<string, list<number>> $m = {};"), "(let map<string, list<number>> $m (map))");
        assert!(error("let $x;").contains("declared with a value"));
    }

    #[test]
    fn functions() {
        assert_eq!(
            stmts("string greet(string $name = 'user') = f\"Hello {$name}\";"),
            "(fn string greet ((string $name = \"user\")) = (f \"Hello \" $name))"
        );
        assert_eq!(stmts("void log(...string $parts) { outln($parts); }"), "(fn void log ((...string $parts)) { (call outln $parts) })");
        assert_eq!(stmts("T first<T>(list<T> $items) = $items[0];"), "(fn T first<T> ((list<T> $items)) = ([] $items 0))");
        assert_eq!(stmts("bool even?(number $n) = $n % 2 == 0;"), "(fn bool even? ((number $n)) = (== (% $n 2) 0))");
        assert_eq!(
            stmts("((number) -> number) makeAdder(number $n) = $n;"),
            "(fn (number) -> number makeAdder ((number $n)) = $n)"
        );
        assert_eq!(stmts("void f($a, $b) {}"), "(fn void f (($a) ($b)) {})");
        assert!(error("void f(...string $a, $b) {}").contains("last one"));
    }

    #[test]
    fn types() {
        assert_eq!(stmts("void f((string) -> bool $p) {}"), "(fn void f (((string) -> bool $p)) {})");
        assert_eq!(stmts("void f((number) -> (number) -> number $p) {}"), "(fn void f (((number) -> (number) -> number $p)) {})");
        assert_eq!(stmts("void f((...string) -> string $p) {}"), "(fn void f (((...string) -> string $p)) {})");
        assert_eq!(stmts("void f(?((number) -> number) $p) {}"), "(fn void f ((?((number) -> number) $p)) {})");
        assert_eq!(stmts("void f((number) -> ?number $p) {}"), "(fn void f (((number) -> ?number $p)) {})");
        assert!(error("void f(?(number) -> number $p) {}").contains("ambiguous"));
    }

    #[test]
    fn control_flow() {
        assert_eq!(
            stmts("if ($a) outln(1); else if ($b) { outln(2); } else outln(3);"),
            "(if $a (call outln 1) (if $b { (call outln 2) } (call outln 3)))"
        );
        assert_eq!(stmts("while ($i < 3) $i++;"), "(while (< $i 3) (++post $i))");
        assert_eq!(stmts("for ($i in 1..5) {}"), "(for ($i) (.. 1 5) {})");
        assert_eq!(stmts("for ($i, $f in $fruits) {}"), "(for ($i, $f) $fruits {})");
        assert_eq!(stmts("for ($k => $v in $m) { break; continue; }"), "(for ($k => $v) $m { (break) (continue) })");
        assert_eq!(stmts("void f() { return; }"), "(fn void f () { (return) })");
        assert!(error("outln(1)").contains("expected `;`"));
    }

    #[test]
    fn statements_starting_with_names() {
        // A call, not a declaration or a function
        assert_eq!(stmts("outln(1);"), "(call outln 1)");
        assert_eq!(stmts("foo < $x;"), "(< foo $x)");
        assert_eq!(stmts("(1 + 2);"), "(+ 1 2)");
    }

    #[test]
    fn error_positions() {
        let err = parse(tokenize("let $a = 1;\nlet $b = (2;").unwrap()).unwrap_err();
        assert_eq!((err.span.line, err.span.column), (2, 12));
        assert!(err.message.contains("expected `)`"));
    }

    #[test]
    fn spans() {
        let stmts = parse(tokenize("let $x = 1 + 22;").unwrap()).unwrap();
        let StmtKind::Let { value, .. } = &stmts[0].kind else { panic!() };
        assert_eq!((value.span.start, value.span.end), (9, 15));
    }

    /// What `parse_recovering` reads, and the lines of its errors.
    fn recovered(source: &str) -> (String, Vec<u32>) {
        let (stmts, errors) = parse_recovering(tokenize(source).unwrap(), crate::stack::DEFAULT_BUDGET);
        (stmts.iter().map(|s| s.to_string()).collect::<Vec<_>>().join("\n"), errors.iter().map(|e| e.span.line).collect())
    }

    #[test]
    fn going_on_after_errors() {
        // A `;` forgotten at the end of a line
        assert_eq!(recovered("let $a = 1\nouts($a);"), ("(let $a 1)\n(call outs $a)".into(), vec![1]));
        // A name being written after `.` or `::`
        let (tree, errors) = recovered("$list.\nouts(1);\nfs::\n");
        assert!(tree.starts_with("(. $list )\n(call outs 1)"), "{tree}");
        let (tree, _) = recovered("const $f = ($s) -> $s.\n(string) -> bool $g = $f;");
        assert!(tree.starts_with("(const $f (lambda (($s)) = (. $s )))\n"), "{tree}");
        assert_eq!(errors, vec![1, 1, 3, 3]);
        let (tree, _) = recovered("outln($p.);");
        assert!(tree.contains("(. $p )"), "{tree}");
        // Parentheses not closed yet, at the end of a line
        let (tree, errors) = recovered("outln($list.map(($x) -> $x.\nouts(1);");
        assert!(tree.starts_with("(call outln (.map $list (lambda"), "{tree}");
        assert!(tree.ends_with("\n(call outs 1)"), "{tree}");
        assert_eq!(errors.len(), 4);
        // `}` without its `{`, alone or after a body not written yet: it ends
        for source in ["}", "}}\nouts(1);", "if (1)\n}", "while (true)"] {
            let (_, errors) = recovered(source);
            assert!(!errors.is_empty(), "{source}");
        }
        // A body not written yet
        let (tree, _) = recovered("for ($n in $list) {\n    if ($n.\n}\nouts(1);");
        assert!(tree.contains("(if (. $n ) {})"), "{tree}");
        // A broken statement is skipped, also inside a block
        let (tree, errors) = recovered("void f() {\n    let = 2;\n    outln(1);\n}\nouts(2);");
        assert_eq!(tree, "(fn void f () { (call outln 1) })\n(call outs 2)");
        assert_eq!(errors, vec![2]);
        let (tree, errors) = recovered("if (1 +) { outln(1); }\nouts(2);\n}");
        assert_eq!(tree, "(call outs 2)");
        // And the `}` without its `{`
        assert_eq!(errors.len(), 2);
        // The end of the file inside a block
        let (tree, errors) = recovered("void f() {\n    outln(1);\n");
        assert_eq!(tree, "(fn void f () { (call outln 1) })");
        assert_eq!(errors, vec![3]);
    }

    #[test]
    fn without_recovering_errors_stop() {
        assert!(error("let $a = 1\nouts($a);").contains("expected `;`"));
        // Written on two lines, it's a method
        assert_eq!(stmts("$list.\nouts(1);"), "(.outs $list 1)");
    }
}
