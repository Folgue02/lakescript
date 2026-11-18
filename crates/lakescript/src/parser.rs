//! Turns tokens into a syntax tree, following the precedence table in
//! `ideas/language/operators.md`. Every function parses one level and calls the
//! next, tighter one.
//!
//! Phase 1 covers declarations, functions, `if`, loops, the operators, calls,
//! lists and maps. Types are parsed and kept in the tree for the checker, but
//! nothing uses them yet.

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::token::{Keyword, Span, StringPart as TokenPart, Token, TokenKind};

type Result<T> = std::result::Result<T, Diagnostic>;

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>> {
    let mut parser = Parser::new(tokens);
    let mut stmts = Vec::new();
    while !parser.check(&TokenKind::Eof) {
        stmts.push(parser.statement()?);
    }
    Ok(stmts)
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
}

#[derive(Clone, Copy)]
struct Checkpoint {
    pos: usize,
    split: u8,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        let eof = Token { kind: TokenKind::Eof, span: Span::default() };
        Parser { tokens, pos: 0, split: 0, split_token: eof, last_span: Span::default() }
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
            Ok(self.advance())
        } else {
            self.error_here(format!("expected {} {context}, found {}", kind.describe(), self.peek().kind))
        }
    }

    fn checkpoint(&self) -> Checkpoint {
        Checkpoint { pos: self.pos, split: self.split }
    }

    fn restore(&mut self, checkpoint: Checkpoint) {
        self.pos = checkpoint.pos;
        self.split = checkpoint.split;
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
        self.split_token = Token { kind, span };
    }

    fn span_from(&self, start: Span) -> Span {
        Span { start: start.start, end: self.last_span.end.max(start.end), line: start.line, column: start.column }
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
        let start = self.peek().span;
        let kind = match &self.peek().kind {
            TokenKind::Keyword(Keyword::Let) => {
                self.advance();
                if self.check(&TokenKind::LBracket) {
                    return self.error_here("destructuring isn't supported yet");
                }
                let name = self.expect_variable("after `let`")?;
                self.declaration_value(false, name, None)?
            }
            TokenKind::Keyword(Keyword::Const) => {
                self.advance();
                let ty = if matches!(self.peek().kind, TokenKind::Variable(_)) { None } else { Some(self.parse_type()?) };
                let name = self.expect_variable("after `const`")?;
                self.declaration_value(true, name, ty)?
            }
            TokenKind::Keyword(Keyword::Type) => {
                self.advance();
                let name = self.expect_ident("for the type alias")?;
                self.expect(&TokenKind::Eq, "after the name of the type alias")?;
                let ty = self.parse_type()?;
                self.expect(&TokenKind::Semicolon, "after the type alias")?;
                StmtKind::TypeAlias { name, ty }
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
            TokenKind::Keyword(k @ (Keyword::Switch | Keyword::Use | Keyword::Pub | Keyword::Yield | Keyword::Try | Keyword::Catch)) => {
                return self.error_here(format!("`{}` isn't supported yet", k.as_str()));
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

        let body = if self.eat(&TokenKind::Eq) {
            let expr = self.expression()?;
            self.expect(&TokenKind::Semicolon, "after the function")?;
            FunctionBody::Expr(expr)
        } else if self.check(&TokenKind::LBrace) {
            FunctionBody::Block(self.block()?)
        } else {
            return self.error_here(format!("expected `{{` or `=` for the body of `{name}`, found {}", self.peek().kind));
        };
        Ok(FunctionDecl { name, generics, params, ret, body })
    }

    fn param(&mut self) -> Result<Param> {
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
        Ok(Param { name, ty, default, variadic })
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(&TokenKind::LBrace, "to start the block")?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return self.error_here("expected `}` to close the block, found the end of the file");
            }
            stmts.push(self.statement()?);
        }
        self.advance();
        Ok(stmts)
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
            return self.error_here("destructuring isn't supported yet");
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
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr> {
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
        let start = self.peek().span;
        let cond = self.coalesce()?;
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

    fn coalesce(&mut self) -> Result<Expr> {
        let start = self.peek().span;
        let left = self.or()?;
        if !self.eat(&TokenKind::QuestionQuestion) {
            return Ok(left);
        }
        let right = self.coalesce()?;
        Ok(self.binary(BinaryOp::Coalesce, left, right, start))
    }

    fn binary(&self, op: BinaryOp, left: Expr, right: Expr, start: Span) -> Expr {
        Expr { kind: ExprKind::Binary { op, left: Box::new(left), right: Box::new(right) }, span: self.span_from(start) }
    }

    /// One left-associative level: `next (op next)*`.
    fn left_assoc(
        &mut self,
        next: fn(&mut Self) -> Result<Expr>,
        op_of: fn(&TokenKind) -> Option<BinaryOp>,
    ) -> Result<Expr> {
        let start = self.peek().span;
        let mut left = next(self)?;
        while let Some(op) = op_of(&self.peek().kind) {
            self.advance();
            let right = next(self)?;
            left = self.binary(op, left, right, start);
        }
        Ok(left)
    }

    fn or(&mut self) -> Result<Expr> {
        self.left_assoc(Self::and, |k| (k == &TokenKind::OrOr).then_some(BinaryOp::Or))
    }

    fn and(&mut self) -> Result<Expr> {
        self.left_assoc(Self::bit_or, |k| (k == &TokenKind::AndAnd).then_some(BinaryOp::And))
    }

    fn bit_or(&mut self) -> Result<Expr> {
        self.left_assoc(Self::bit_xor, |k| (k == &TokenKind::Pipe).then_some(BinaryOp::BitOr))
    }

    fn bit_xor(&mut self) -> Result<Expr> {
        self.left_assoc(Self::bit_and, |k| (k == &TokenKind::Caret).then_some(BinaryOp::BitXor))
    }

    fn bit_and(&mut self) -> Result<Expr> {
        self.left_assoc(Self::equality, |k| (k == &TokenKind::Amp).then_some(BinaryOp::BitAnd))
    }

    fn equality(&mut self) -> Result<Expr> {
        self.left_assoc(Self::relational, |k| match k {
            TokenKind::EqEq => Some(BinaryOp::Eq),
            TokenKind::NotEq => Some(BinaryOp::NotEq),
            _ => None,
        })
    }

    /// `<`, `>`, `<=`, `>=` and the binary test operators. They can't be chained.
    fn relational(&mut self) -> Result<Expr> {
        let start = self.peek().span;
        let left = self.range()?;
        let Some(op) = relational_op(&self.peek().kind) else {
            return Ok(left);
        };
        self.advance();
        let right = self.range()?;
        if relational_op(&self.peek().kind).is_some() {
            return self.error_here(format!(
                "comparisons can't be chained: use `&&` *(i.e. `$a < $b && $b < $c`)*, found {}",
                self.peek().kind
            ));
        }
        Ok(self.binary(op, left, right, start))
    }

    /// `a..b` and `a..=b`. The end can be left out where nothing could follow,
    /// like in `$text[-3..]`.
    fn range(&mut self) -> Result<Expr> {
        let start = self.peek().span;
        let left = self.shift()?;
        let inclusive = match self.peek().kind {
            TokenKind::DotDot => false,
            TokenKind::DotDotEq => true,
            _ => return Ok(left),
        };
        self.advance();
        let end = if can_start_expression(&self.peek().kind) { Some(Box::new(self.shift()?)) } else { None };
        if end.is_none() && inclusive {
            return self.error_here("`..=` needs an end *(i.e. `1..=5`)*");
        }
        if matches!(self.peek().kind, TokenKind::DotDot | TokenKind::DotDotEq) {
            return self.error_here("ranges can't be chained");
        }
        Ok(Expr { kind: ExprKind::Range { start: Box::new(left), end, inclusive }, span: self.span_from(start) })
    }

    fn shift(&mut self) -> Result<Expr> {
        self.left_assoc(Self::additive, |k| match k {
            TokenKind::Shl => Some(BinaryOp::Shl),
            TokenKind::Shr => Some(BinaryOp::Shr),
            TokenKind::UShr => Some(BinaryOp::UShr),
            _ => None,
        })
    }

    fn additive(&mut self) -> Result<Expr> {
        self.left_assoc(Self::multiplicative, |k| match k {
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Sub),
            _ => None,
        })
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        self.left_assoc(Self::unary, |k| match k {
            TokenKind::Star => Some(BinaryOp::Mul),
            TokenKind::Slash => Some(BinaryOp::Div),
            TokenKind::Percent => Some(BinaryOp::Rem),
            _ => None,
        })
    }

    fn unary(&mut self) -> Result<Expr> {
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
        let path = self.range()?;
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
            let kind = match self.peek().kind {
                TokenKind::Dot | TokenKind::QuestionDot => {
                    let nullable = self.advance().kind == TokenKind::QuestionDot;
                    self.method(expr, nullable)?
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
        let name = self.expect_ident("after `.`")?;
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
            TokenKind::Ident(name) => {
                self.advance();
                if self.check(&TokenKind::ColonColon) {
                    return self.error_here("modules aren't supported yet");
                }
                ExprKind::Function(name)
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
            TokenKind::Keyword(k @ (Keyword::If | Keyword::Switch | Keyword::Try)) => {
                return self.error_here(format!("`{}` as an expression isn't supported yet", k.as_str()));
            }
            other => return self.error_here(format!("expected an expression, found {other}")),
        };
        Ok(Expr { kind, span: self.span_from(start) })
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

fn is_assignable(expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::Variable(_) | ExprKind::Index { nullable: false, .. })
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
        Keyword(k) => matches!(k, self::Keyword::True | self::Keyword::False | self::Keyword::Null),
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
        assert!(error("$list.length;").contains("parentheses"));
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
        assert_eq!(stmts("type Predicate = (string) -> bool;"), "(type Predicate (string) -> bool)");
        assert_eq!(stmts("type F = (number) -> (number) -> number;"), "(type F (number) -> (number) -> number)");
        assert_eq!(stmts("type V = (...string) -> string;"), "(type V (...string) -> string)");
        assert_eq!(stmts("type N = ?((number) -> number);"), "(type N ?((number) -> number))");
        assert_eq!(stmts("type R = (number) -> ?number;"), "(type R (number) -> ?number)");
        assert!(error("type B = ?(number) -> number;").contains("ambiguous"));
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
}
