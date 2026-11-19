//! Turns source text into tokens.
//!
//! Rules from the notes that are decided here, not in the parser:
//! - A name followed directly by `?(` keeps the `?` as part of the name (`even?()`).
//! - A `-` directly followed by a letter is a test operator (`-f`, `-like`).
//! - A `:` directly followed by a letter is a symbol (`:active`).
//! - `?[` written without a space is the nullable index.
//! - `.` is only a decimal point when a digit follows it, so `1..5` and `5.even?()` work.

use crate::diagnostic::Diagnostic;
use crate::token::{Keyword, Span, StringPart, TEST_OPERATORS, Token, TokenKind};

pub fn tokenize(source: &str) -> Result<Vec<Token>, Diagnostic> {
    tokenize_file(source, 0)
}

/// Like `tokenize`, marking every span with the number of the file.
pub fn tokenize_file(source: &str, file: u32) -> Result<Vec<Token>, Diagnostic> {
    let mut lexer = Lexer::new(source);
    lexer.file = file;
    lexer.skip_shebang();
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token()?;
        let done = token.kind == TokenKind::Eof;
        tokens.push(token);
        if done {
            return Ok(tokens);
        }
    }
}

#[derive(Clone, Copy)]
struct Mark {
    pos: usize,
    line: u32,
    column: u32,
}

/// A character of a string body, or an interpolated expression.
enum Elem {
    Ch(char),
    Code(Vec<Token>),
}

#[derive(Clone, Copy, PartialEq)]
enum Escapes {
    /// `\n`, `\t`, `\u{...}`... Unknown escapes are an error.
    Standard,
    /// Regexes and commands: only `\"` is turned into `"`, every other
    /// backslash is kept for the regex engine or the shell.
    KeepBackslashes,
}

struct Lexer {
    file: u32,
    chars: Vec<char>,
    pos: usize,
    line: u32,
    column: u32,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Lexer { chars: source.chars().collect(), pos: 0, line: 1, column: 1, file: 0 }
    }

    // ---- Cursor ----

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn at(&self, offset: usize, c: char) -> bool {
        self.peek(offset) == Some(c)
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn advance_by(&mut self, n: usize) {
        for _ in 0..n {
            self.advance();
        }
    }

    fn eat(&mut self, c: char) -> bool {
        if self.at(0, c) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn mark(&self) -> Mark {
        Mark { pos: self.pos, line: self.line, column: self.column }
    }

    fn span_from(&self, mark: Mark) -> Span {
        Span { start: mark.pos, end: self.pos, line: mark.line, column: mark.column, file: self.file }
    }

    fn error<T>(&self, message: impl Into<String>, mark: Mark) -> Result<T, Diagnostic> {
        Err(Diagnostic::new(message, self.span_from(mark)))
    }

    // ---- Trivia ----

    fn skip_shebang(&mut self) {
        if self.at(0, '#') && self.at(1, '!') {
            while let Some(c) = self.peek(0) {
                if c == '\n' {
                    break;
                }
                self.advance();
            }
        }
    }

    fn skip_trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            match self.peek(0) {
                Some(c) if c.is_whitespace() => {
                    self.advance();
                }
                Some('/') if self.at(1, '/') => {
                    while let Some(c) = self.peek(0) {
                        if c == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                Some('/') if self.at(1, '*') => {
                    let start = self.mark();
                    self.advance_by(2);
                    loop {
                        if self.peek(0).is_none() {
                            return self.error("this block comment is never closed with `*/`", start);
                        }
                        if self.at(0, '*') && self.at(1, '/') {
                            self.advance_by(2);
                            break;
                        }
                        self.advance();
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    // ---- Tokens ----

    fn next_token(&mut self) -> Result<Token, Diagnostic> {
        self.skip_trivia()?;
        let start = self.mark();
        let kind = match self.peek(0) {
            None => TokenKind::Eof,
            Some(c) => self.lex_token(c, start)?,
        };
        Ok(Token { kind, span: self.span_from(start) })
    }

    fn lex_token(&mut self, c: char, start: Mark) -> Result<TokenKind, Diagnostic> {
        use TokenKind::*;

        // Prefixed strings, before plain names
        if c == 'f' && self.at(1, '"') {
            self.advance();
            return self.lex_double_quoted(start, true);
        }
        if c == 'r' && (self.at(1, '"') || self.at(1, '#')) {
            self.advance();
            return self.lex_raw_string(start);
        }
        if c == '"' {
            return self.lex_double_quoted(start, false);
        }
        if c.is_ascii_digit() {
            return self.lex_number(start);
        }
        if is_name_start(c) {
            return Ok(self.lex_name());
        }

        self.advance();
        let kind = match c {
            '$' => {
                if !self.peek(0).is_some_and(is_name_start) {
                    return self.error("`$` must be followed by a variable name, like `$name`", start);
                }
                Variable(self.read_name())
            }
            '\'' => self.lex_single_quoted(start)?,
            '%' => match (self.peek(0), self.peek(1)) {
                (Some('r'), Some('"')) => {
                    self.advance();
                    self.lex_regex(start)?
                }
                (Some('x'), Some('"')) => {
                    self.advance();
                    self.lex_command(start)?
                }
                (Some('w'), Some('[')) => {
                    self.advance_by(2);
                    WordList(self.lex_word_list(start, false)?)
                }
                (Some('i'), Some('[')) => {
                    self.advance_by(2);
                    SymbolList(self.lex_word_list(start, true)?)
                }
                (Some('='), _) => {
                    self.advance();
                    PercentEq
                }
                _ => Percent,
            },
            ':' => {
                if self.eat(':') {
                    ColonColon
                } else if self.peek(0).is_some_and(is_name_start) {
                    Symbol(self.read_name())
                } else {
                    Colon
                }
            }
            '?' => {
                if self.eat('.') {
                    QuestionDot
                } else if self.eat('?') {
                    QuestionQuestion
                } else if self.eat('[') {
                    QuestionBracket
                } else {
                    Question
                }
            }
            '-' => {
                if self.eat('>') {
                    Arrow
                } else if self.eat('-') {
                    MinusMinus
                } else if self.eat('=') {
                    MinusEq
                } else if self.peek(0).is_some_and(|c| c.is_ascii_alphabetic()) {
                    let name = self.read_name();
                    if !TEST_OPERATORS.contains(&name.as_str()) {
                        return self.error(
                            format!(
                                "unknown test operator `-{name}`. A `-` followed by a letter is a test operator; \
                                 to subtract, put a space after the `-`"
                            ),
                            start,
                        );
                    }
                    TestOp(name)
                } else {
                    Minus
                }
            }
            '+' => {
                if self.eat('+') {
                    PlusPlus
                } else if self.eat('=') {
                    PlusEq
                } else {
                    Plus
                }
            }
            '*' => {
                if self.eat('*') {
                    if self.eat('=') { StarStarEq } else { StarStar }
                } else if self.eat('=') {
                    StarEq
                } else {
                    Star
                }
            }
            '/' => {
                if self.eat('=') { SlashEq } else { Slash }
            }
            '=' => {
                if self.eat('=') {
                    EqEq
                } else if self.eat('>') {
                    FatArrow
                } else {
                    Eq
                }
            }
            '!' => {
                if self.eat('=') { NotEq } else { Bang }
            }
            '<' => {
                if self.eat('<') {
                    if self.eat('=') { ShlEq } else { Shl }
                } else if self.eat('=') {
                    LtEq
                } else {
                    Lt
                }
            }
            '>' => {
                if self.eat('>') {
                    if self.eat('>') {
                        if self.eat('=') { UShrEq } else { UShr }
                    } else if self.eat('=') {
                        ShrEq
                    } else {
                        Shr
                    }
                } else if self.eat('=') {
                    GtEq
                } else {
                    Gt
                }
            }
            '&' => {
                if self.eat('&') {
                    AndAnd
                } else if self.eat('=') {
                    AmpEq
                } else {
                    Amp
                }
            }
            '|' => {
                if self.eat('|') {
                    OrOr
                } else if self.eat('=') {
                    PipeEq
                } else {
                    Pipe
                }
            }
            '^' => {
                if self.eat('=') { CaretEq } else { Caret }
            }
            '.' => {
                if self.at(0, '.') && self.at(1, '.') {
                    self.advance_by(2);
                    Ellipsis
                } else if self.eat('.') {
                    if self.eat('=') { DotDotEq } else { DotDot }
                } else {
                    Dot
                }
            }
            '~' => Tilde,
            ',' => Comma,
            ';' => Semicolon,
            '(' => LParen,
            ')' => RParen,
            '[' => LBracket,
            ']' => RBracket,
            '{' => LBrace,
            '}' => RBrace,
            other => return self.error(format!("unexpected character `{other}`"), start),
        };
        Ok(kind)
    }

    // ---- Names ----

    fn read_name(&mut self) -> String {
        let mut name = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_ascii_alphanumeric() || c == '_' {
                name.push(c);
                self.advance();
            } else {
                break;
            }
        }
        name
    }

    fn lex_name(&mut self) -> TokenKind {
        let mut name = self.read_name();
        if let Some(keyword) = Keyword::from_word(&name) {
            return TokenKind::Keyword(keyword);
        }
        // `even?()`: the `?` is part of the name only when `(` follows right after it
        if self.at(0, '?') && self.at(1, '(') {
            self.advance();
            name.push('?');
        }
        TokenKind::Ident(name)
    }

    // ---- Numbers ----

    fn lex_number(&mut self, start: Mark) -> Result<TokenKind, Diagnostic> {
        let radix = match (self.peek(0), self.peek(1)) {
            (Some('0'), Some('x' | 'X')) => 16,
            (Some('0'), Some('b' | 'B')) => 2,
            (Some('0'), Some('o' | 'O')) => 8,
            _ => 10,
        };

        if radix != 10 {
            self.advance_by(2);
            let digits = self.read_digits(radix);
            self.check_number_end(start)?;
            if digits.is_empty() {
                return self.error("this number has no digits after its prefix", start);
            }
            let clean = self.clean_digits(&digits, radix, start)?;
            return match i64::from_str_radix(&clean, radix) {
                Ok(n) => Ok(TokenKind::Int(n)),
                Err(_) => self.error("this number doesn't fit in 64 bits", start),
            };
        }

        let mut text = self.read_digits(10);
        let mut is_float = false;
        if self.at(0, '.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
            text.push('.');
            text.push_str(&self.read_digits(10));
            is_float = true;
        }
        if matches!(self.peek(0), Some('e' | 'E')) {
            let digit_at = if matches!(self.peek(1), Some('+' | '-')) { 2 } else { 1 };
            if self.peek(digit_at).is_some_and(|c| c.is_ascii_digit()) {
                text.push('e');
                self.advance();
                if digit_at == 2 {
                    text.push(self.advance().unwrap());
                }
                text.push_str(&self.read_digits(10));
                is_float = true;
            }
        }
        self.check_number_end(start)?;

        let clean = self.clean_digits(&text, 10, start)?;
        if !is_float {
            if let Ok(n) = clean.parse::<i64>() {
                return Ok(TokenKind::Int(n));
            }
            // Too big for an integer: it's stored as a float, like the result of an operation
        }
        match clean.parse::<f64>() {
            Ok(n) => Ok(TokenKind::Float(n)),
            Err(_) => self.error("invalid number", start),
        }
    }

    /// Digits of the radix and underscores.
    fn read_digits(&mut self, radix: u32) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_digit(radix) || c == '_' {
                text.push(c);
                self.advance();
            } else {
                break;
            }
        }
        text
    }

    /// `_` can only go between two digits.
    fn clean_digits(&self, text: &str, radix: u32, start: Mark) -> Result<String, Diagnostic> {
        let chars: Vec<char> = text.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if c == '_' {
                let before = i > 0 && chars[i - 1].is_digit(radix);
                let after = chars.get(i + 1).is_some_and(|c| c.is_digit(radix));
                if !before || !after {
                    return self.error("`_` in a number can only go between two digits", start);
                }
            }
        }
        Ok(text.replace('_', ""))
    }

    fn check_number_end(&self, start: Mark) -> Result<(), Diagnostic> {
        if let Some(c) = self.peek(0) {
            if c.is_ascii_alphanumeric() || c == '_' {
                return self.error(format!("unexpected `{c}` in a number"), start);
            }
        }
        Ok(())
    }

    // ---- Strings ----

    fn lex_single_quoted(&mut self, start: Mark) -> Result<TokenKind, Diagnostic> {
        // The opening `'` is already read
        let mut raw = String::new();
        loop {
            match self.advance() {
                None | Some('\n') => return self.error("this string is never closed with `'`", start),
                Some('\'') => break,
                Some('\\') => {
                    raw.push('\\');
                    if let Some(c) = self.advance() {
                        raw.push(c);
                    }
                }
                Some(c) => raw.push(c),
            }
        }
        Ok(TokenKind::Str(self.unescape(&raw, Escapes::Standard, start)?))
    }

    /// `"..."`, `"""..."""`, `f"..."` and `f"""..."""`. The cursor is on the first `"`.
    fn lex_double_quoted(&mut self, start: Mark, interpolate: bool) -> Result<TokenKind, Diagnostic> {
        let multiline = self.at(0, '"') && self.at(1, '"') && self.at(2, '"');
        self.advance_by(if multiline { 3 } else { 1 });

        let mut elems = self.scan_body(start, interpolate, multiline, false)?;
        if multiline {
            elems = dedent(elems);
        }
        let parts = self.build_parts(elems, Escapes::Standard, start)?;

        if !interpolate {
            let text = match parts.into_iter().next() {
                Some(StringPart::Text(text)) => text,
                _ => String::new(),
            };
            return Ok(TokenKind::Str(text));
        }
        Ok(TokenKind::FStr(parts))
    }

    /// `r"..."` and `r#"..."#`, with no escapes. The cursor is after the `r`.
    fn lex_raw_string(&mut self, start: Mark) -> Result<TokenKind, Diagnostic> {
        let mut hashes = 0;
        while self.eat('#') {
            hashes += 1;
        }
        if !self.eat('"') {
            return self.error("a raw string starts with `r\"` or `r#\"`", start);
        }
        let mut text = String::new();
        loop {
            match self.advance() {
                None => return self.error("this raw string is never closed", start),
                Some('"') if (0..hashes).all(|i| self.at(i, '#')) => {
                    self.advance_by(hashes);
                    break;
                }
                Some(c) => text.push(c),
            }
        }
        Ok(TokenKind::Str(text))
    }

    /// `%r"..."` with optional flags. The cursor is on the `"`.
    fn lex_regex(&mut self, start: Mark) -> Result<TokenKind, Diagnostic> {
        self.advance();
        let elems = self.scan_body(start, false, false, false)?;
        let pattern = match self.build_parts(elems, Escapes::KeepBackslashes, start)?.pop() {
            Some(StringPart::Text(text)) => text,
            _ => String::new(),
        };
        let mut flags = String::new();
        while let Some(c) = self.peek(0) {
            if !c.is_ascii_alphabetic() {
                break;
            }
            if !"imsx".contains(c) {
                return self.error(format!("unknown regex flag `{c}`. The flags are `i`, `m`, `s` and `x`"), start);
            }
            flags.push(c);
            self.advance();
        }
        Ok(TokenKind::Regex { pattern, flags })
    }

    /// `%x"..."`. The cursor is on the `"`. It can span several lines.
    fn lex_command(&mut self, start: Mark) -> Result<TokenKind, Diagnostic> {
        self.advance();
        let elems = self.scan_body(start, true, false, true)?;
        Ok(TokenKind::Command(self.build_parts(elems, Escapes::KeepBackslashes, start)?))
    }

    /// `%w[...]` and `%i[...]`. The cursor is after the `[`.
    fn lex_word_list(&mut self, start: Mark, symbols: bool) -> Result<Vec<String>, Diagnostic> {
        let mut words = Vec::new();
        let mut word = String::new();
        loop {
            match self.advance() {
                None => return self.error("this list is never closed with `]`", start),
                Some(']') => break,
                Some(c) if c.is_whitespace() => {
                    if !word.is_empty() {
                        words.push(std::mem::take(&mut word));
                    }
                }
                Some(c) => word.push(c),
            }
        }
        if !word.is_empty() {
            words.push(word);
        }
        if symbols {
            if let Some(bad) = words.iter().find(|w| !is_valid_name(w)) {
                return self.error(format!("`{bad}` isn't a valid symbol name"), start);
            }
        }
        Ok(words)
    }

    /// Reads a string body up to its closing quote(s), keeping escapes as they
    /// are written so that multi-line strings can be dedented first.
    fn scan_body(
        &mut self,
        start: Mark,
        interpolate: bool,
        multiline: bool,
        allow_newlines: bool,
    ) -> Result<Vec<Elem>, Diagnostic> {
        let mut elems = Vec::new();
        loop {
            let Some(c) = self.peek(0) else {
                return self.error("this string is never closed with `\"`", start);
            };
            if c == '"' {
                if !multiline {
                    self.advance();
                    return Ok(elems);
                }
                if self.at(1, '"') && self.at(2, '"') {
                    self.advance_by(3);
                    return Ok(elems);
                }
            }
            if c == '\n' && !multiline && !allow_newlines {
                return self.error(
                    "this string is never closed with `\"`. Use `\"\"\"` for strings with several lines",
                    start,
                );
            }
            self.advance();
            match c {
                '\\' => {
                    elems.push(Elem::Ch('\\'));
                    if let Some(next) = self.advance() {
                        elems.push(Elem::Ch(next));
                    }
                }
                '{' if interpolate => {
                    if self.eat('{') {
                        elems.push(Elem::Ch('{'));
                    } else {
                        elems.push(Elem::Code(self.lex_interpolation()?));
                    }
                }
                '}' if interpolate => {
                    if self.eat('}') {
                        elems.push(Elem::Ch('}'));
                    } else {
                        let at = Mark { pos: self.pos - 1, line: self.line, column: self.column - 1 };
                        return self.error("a single `}` in an interpolated string. Write `}}` for a literal `}`", at);
                    }
                }
                c => elems.push(Elem::Ch(c)),
            }
        }
    }

    /// The tokens between `{` and its matching `}`. The `{` is already read.
    fn lex_interpolation(&mut self) -> Result<Vec<Token>, Diagnostic> {
        let open = Mark { pos: self.pos - 1, line: self.line, column: self.column - 1 };
        let mut tokens = Vec::new();
        let mut depth = 0;
        loop {
            let token = self.next_token()?;
            match token.kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace if depth == 0 => {
                    if tokens.is_empty() {
                        return self.error("empty `{}` in an interpolated string. Write `{{}}` for literal braces", open);
                    }
                    tokens.push(Token { kind: TokenKind::Eof, span: token.span });
                    return Ok(tokens);
                }
                TokenKind::RBrace => depth -= 1,
                TokenKind::Eof => return self.error("this `{` in the string is never closed with `}`", open),
                _ => {}
            }
            tokens.push(token);
        }
    }

    /// Joins the characters into text parts and processes their escapes.
    fn build_parts(&self, elems: Vec<Elem>, escapes: Escapes, start: Mark) -> Result<Vec<StringPart>, Diagnostic> {
        let mut parts = Vec::new();
        let mut raw = String::new();
        for elem in elems {
            match elem {
                Elem::Ch(c) => raw.push(c),
                Elem::Code(tokens) => {
                    if !raw.is_empty() {
                        parts.push(StringPart::Text(self.unescape(&std::mem::take(&mut raw), escapes, start)?));
                    }
                    parts.push(StringPart::Code(tokens));
                }
            }
        }
        if !raw.is_empty() || parts.is_empty() {
            parts.push(StringPart::Text(self.unescape(&raw, escapes, start)?));
        }
        Ok(parts)
    }

    fn unescape(&self, raw: &str, escapes: Escapes, start: Mark) -> Result<String, Diagnostic> {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            let Some(next) = chars.next() else {
                out.push('\\');
                break;
            };
            if escapes == Escapes::KeepBackslashes {
                if next != '"' {
                    out.push('\\');
                }
                out.push(next);
                continue;
            }
            match next {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                '\\' => out.push('\\'),
                '\'' => out.push('\''),
                '"' => out.push('"'),
                'u' => {
                    if chars.next() != Some('{') {
                        return self.error("a unicode escape is written `\\u{1F600}`", start);
                    }
                    let mut hex = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(h) if h.is_ascii_hexdigit() && hex.len() < 6 => hex.push(h),
                            _ => return self.error("a unicode escape is written `\\u{1F600}`", start),
                        }
                    }
                    let ch = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
                    match ch {
                        Some(ch) => out.push(ch),
                        None => return self.error(format!("`\\u{{{hex}}}` isn't a valid unicode character"), start),
                    }
                }
                other => {
                    return self.error(
                        format!("unknown escape `\\{other}`. Write `\\\\` for a backslash, or use a raw string `r\"...\"`"),
                        start,
                    );
                }
            }
        }
        Ok(out)
    }
}

/// Removes the indentation all the lines of a multi-line string share, like
/// Java's text blocks:
/// - A first line with only whitespace (right after the opening `"""`) is removed.
/// - The line of the closing `"""` counts for the indentation. If it only has
///   whitespace, it's removed, so the string ends with a new line.
/// - Lines with only whitespace become empty and don't count for the indentation.
fn dedent(elems: Vec<Elem>) -> Vec<Elem> {
    let mut lines: Vec<Vec<Elem>> = vec![Vec::new()];
    for elem in elems {
        match elem {
            Elem::Ch('\n') => lines.push(Vec::new()),
            other => lines.last_mut().unwrap().push(other),
        }
    }

    let blank = |line: &Vec<Elem>| line.iter().all(|e| matches!(e, Elem::Ch(' ' | '\t' | '\r')));
    let indent = |line: &Vec<Elem>| line.iter().take_while(|e| matches!(e, Elem::Ch(' ' | '\t'))).count();

    if lines.len() > 1 && blank(&lines[0]) {
        lines.remove(0);
    }
    if lines.len() == 1 {
        return lines.pop().unwrap();
    }

    let last = lines.len() - 1;
    let common = lines
        .iter()
        .enumerate()
        .filter(|(i, line)| *i == last || !blank(line))
        .map(|(_, line)| indent(line))
        .min()
        .unwrap_or(0);

    let mut out = Vec::new();
    let count = lines.len();
    for (i, line) in lines.into_iter().enumerate() {
        if i == last && blank(&line) {
            break; // the closing `"""` line
        }
        if !blank(&line) {
            out.extend(line.into_iter().skip(common));
        }
        if i + 1 < count {
            out.push(Elem::Ch('\n'));
        }
    }
    out
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_valid_name(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(is_name_start) && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        let mut tokens: Vec<TokenKind> = tokenize(source).unwrap().into_iter().map(|t| t.kind).collect();
        assert_eq!(tokens.pop(), Some(Eof));
        tokens
    }

    fn error(source: &str) -> String {
        tokenize(source).unwrap_err().message
    }

    fn ident(name: &str) -> TokenKind {
        Ident(name.to_string())
    }

    fn var(name: &str) -> TokenKind {
        Variable(name.to_string())
    }

    fn text(s: &str) -> StringPart {
        StringPart::Text(s.to_string())
    }

    /// The kinds inside an interpolated part, without its `Eof`.
    fn code(part: &StringPart) -> Vec<TokenKind> {
        match part {
            StringPart::Code(tokens) => {
                let mut kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind.clone()).collect();
                assert_eq!(kinds.pop(), Some(Eof));
                kinds
            }
            other => panic!("expected code, got {other:?}"),
        }
    }

    #[test]
    fn numbers() {
        assert_eq!(
            kinds("255 0xFF 0b1010 0o17 1_000_000 2.5 1e6 2.5e-3 0xFF_FF"),
            vec![Int(255), Int(255), Int(10), Int(15), Int(1_000_000), Float(2.5), Float(1e6), Float(2.5e-3), Int(0xFFFF)]
        );
    }

    #[test]
    fn huge_integer_becomes_float() {
        assert_eq!(kinds("99999999999999999999"), vec![Float(99999999999999999999.0)]);
    }

    #[test]
    fn number_errors() {
        assert!(error("1__0").contains("between two digits"));
        assert!(error("1_").contains("between two digits"));
        assert!(error("0x").contains("no digits"));
        assert!(error("5abc").contains("in a number"));
        assert!(error("0xFFFFFFFFFFFFFFFFFF").contains("64 bits"));
    }

    #[test]
    fn dots_after_numbers() {
        assert_eq!(kinds("1..5"), vec![Int(1), DotDot, Int(5)]);
        assert_eq!(kinds("1..=5"), vec![Int(1), DotDotEq, Int(5)]);
        assert_eq!(kinds("4.0.whole?()"), vec![Float(4.0), Dot, ident("whole?"), LParen, RParen]);
        assert_eq!(kinds("5.even?()"), vec![Int(5), Dot, ident("even?"), LParen, RParen]);
    }

    #[test]
    fn names_with_question_marks() {
        assert_eq!(
            kinds("$user.name?.upper()"),
            vec![var("user"), Dot, ident("name"), QuestionDot, ident("upper"), LParen, RParen]
        );
        assert_eq!(
            kinds("$n.even ? 1 : 2"),
            vec![var("n"), Dot, ident("even"), Question, Int(1), Colon, Int(2)]
        );
        assert_eq!(kinds("$ok?(1)"), vec![var("ok"), Question, LParen, Int(1), RParen]);
    }

    #[test]
    fn keywords_and_variables() {
        assert_eq!(
            kinds("let $type = null;"),
            vec![Keyword(crate::token::Keyword::Let), var("type"), Eq, Keyword(crate::token::Keyword::Null), Semicolon]
        );
        assert!(error("$ = 1").contains("variable name"));
    }

    #[test]
    fn nullable_index() {
        assert_eq!(kinds("$d?['a']"), vec![var("d"), QuestionBracket, Str("a".into()), RBracket]);
        assert_eq!(kinds("$ok ? ['a']"), vec![var("ok"), Question, LBracket, Str("a".into()), RBracket]);
    }

    #[test]
    fn symbols_and_colons() {
        assert_eq!(kinds(":active"), vec![Symbol("active".into())]);
        assert_eq!(kinds("paths::load"), vec![ident("paths"), ColonColon, ident("load")]);
        assert_eq!(kinds("name: 'x'"), vec![ident("name"), Colon, Str("x".into())]);
    }

    #[test]
    fn test_operators() {
        assert_eq!(kinds("-f $path"), vec![TestOp("f".into()), var("path")]);
        assert_eq!(kinds("!-f $p"), vec![Bang, TestOp("f".into()), var("p")]);
        assert_eq!(
            kinds("$f -notlike '*.txt'"),
            vec![var("f"), TestOp("notlike".into()), Str("*.txt".into())]
        );
        assert_eq!(kinds("$a - foo()"), vec![var("a"), Minus, ident("foo"), LParen, RParen]);
        assert!(error("$a -foo()").contains("unknown test operator `-foo`"));
    }

    #[test]
    fn operators() {
        assert_eq!(
            kinds("** **= ++ -- -> => ... .. ..= >>> >>>= >> >>= << <<= ?? ?. && || != == <= >= %= %"),
            vec![
                StarStar, StarStarEq, PlusPlus, MinusMinus, Arrow, FatArrow, Ellipsis, DotDot, DotDotEq, UShr, UShrEq,
                Shr, ShrEq, Shl, ShlEq, QuestionQuestion, QuestionDot, AndAnd, OrOr, NotEq, EqEq, LtEq, GtEq,
                PercentEq, Percent
            ]
        );
    }

    #[test]
    fn comments_and_shebang() {
        assert_eq!(
            kinds("#!/usr/bin/lks\n// line\n1 /* block\ncomment */ 2 /// doc\n"),
            vec![Int(1), Int(2)]
        );
        assert!(error("/* never closed").contains("never closed"));
    }

    #[test]
    fn plain_strings() {
        assert_eq!(kinds(r#"'it\'s' "say \"hi\"\n""#), vec![Str("it's".into()), Str("say \"hi\"\n".into())]);
        assert_eq!(kinds(r"'\u{1F600}'"), vec![Str("😀".into())]);
        assert_eq!(kinds(r#""{not interpolated}""#), vec![Str("{not interpolated}".into())]);
        assert!(error(r"'\q'").contains("unknown escape"));
        assert!(error("'never closed").contains("never closed"));
        assert!(error("\"two\nlines\"").contains("several lines"));
    }

    #[test]
    fn raw_strings() {
        assert_eq!(kinds(r###"r#"a\nb "quoted""#"###), vec![Str(r#"a\nb "quoted""#.into())]);
        assert_eq!(kinds(r#"r"C:\path""#), vec![Str(r"C:\path".into())]);
    }

    #[test]
    fn interpolated_strings() {
        let tokens = kinds(r#"f"Hello {$name}!""#);
        let FStr(parts) = &tokens[0] else { panic!() };
        assert_eq!(parts[0], text("Hello "));
        assert_eq!(code(&parts[1]), vec![var("name")]);
        assert_eq!(parts[2], text("!"));

        let tokens = kinds(r#"f"{{literal}} {$m['a'] + 1}""#);
        let FStr(parts) = &tokens[0] else { panic!() };
        assert_eq!(parts[0], text("{literal} "));
        assert_eq!(code(&parts[1]), vec![var("m"), LBracket, Str("a".into()), RBracket, Plus, Int(1)]);

        let tokens = kinds(r#"f"{ {'a' => 1}['a'] }""#);
        let FStr(parts) = &tokens[0] else { panic!() };
        assert_eq!(code(&parts[0])[0], LBrace);

        assert!(error(r#"f"a } b""#).contains("single `}`"));
        assert!(error(r#"f"{}""#).contains("empty"));
        assert!(error(r#"f"{$a""#).contains("never closed"));
    }

    #[test]
    fn multiline_strings() {
        let source = "\"\"\"\n        Usage: backup <folder>\n          --dry-run\n        \"\"\"";
        assert_eq!(kinds(source), vec![Str("Usage: backup <folder>\n  --dry-run\n".into())]);

        // Blank lines don't count for the indentation
        let source = "\"\"\"\n    a\n\n    b\n    \"\"\"";
        assert_eq!(kinds(source), vec![Str("a\n\nb\n".into())]);

        // Closing quotes after the text: no new line at the end
        let source = "\"\"\"\n  a\n  b\"\"\"";
        assert_eq!(kinds(source), vec![Str("a\nb".into())]);

        let source = "f\"\"\"\n    Name: {$name}\n    \"\"\"";
        let tokens = kinds(source);
        let FStr(parts) = &tokens[0] else { panic!() };
        assert_eq!(parts[0], text("Name: "));
        assert_eq!(code(&parts[1]), vec![var("name")]);
        assert_eq!(parts[2], text("\n"));
    }

    #[test]
    fn regexes() {
        assert_eq!(
            kinds(r#"%r"^\d+$"i"#),
            vec![Regex { pattern: r"^\d+$".into(), flags: "i".into() }]
        );
        assert_eq!(
            kinds(r#"%r"say \"hi\"""#),
            vec![Regex { pattern: "say \"hi\"".into(), flags: "".into() }]
        );
        assert!(error(r#"%r"a"q"#).contains("unknown regex flag"));
    }

    #[test]
    fn commands() {
        let tokens = kinds(r#"%x"rm {$file} | grep '\.txt'""#);
        let Command(parts) = &tokens[0] else { panic!() };
        assert_eq!(parts[0], text("rm "));
        assert_eq!(code(&parts[1]), vec![var("file")]);
        assert_eq!(parts[2], text(r" | grep '\.txt'"));
    }

    #[test]
    fn word_lists() {
        assert_eq!(
            kinds("%w[Oranges Apples\n Tomatoes] %i[a b]"),
            vec![
                WordList(vec!["Oranges".into(), "Apples".into(), "Tomatoes".into()]),
                SymbolList(vec!["a".into(), "b".into()])
            ]
        );
        assert!(error("%i[ok not-ok]").contains("valid symbol"));
    }

    #[test]
    fn spans() {
        let tokens = tokenize("let $a = 1;\n  $a++;").unwrap();
        let a = &tokens[5];
        assert_eq!(a.kind, var("a"));
        assert_eq!((a.span.line, a.span.column), (2, 3));
        assert_eq!(tokens[6].kind, PlusPlus);
        assert_eq!((tokens[6].span.line, tokens[6].span.column), (2, 5));
    }

    #[test]
    fn error_position() {
        let err = tokenize("let $a = 1;\nlet $b = 'oops;").unwrap_err();
        assert_eq!((err.span.line, err.span.column), (2, 10));
    }
}
