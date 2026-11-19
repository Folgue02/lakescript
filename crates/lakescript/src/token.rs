use std::fmt;

/// A place in the source. `start` and `end` are character indexes, `line` and
/// `column` start at 1 and point at the first character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// A piece of an interpolated string (`f"..."`, `%x"..."`): plain text, or the
/// tokens of an expression between `{` and `}`, ending with an `Eof` token.
#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Text(String),
    Code(Vec<Token>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Let,
    Const,
    Type,
    Use,
    Pub,
    If,
    Else,
    For,
    In,
    While,
    Switch,
    Case,
    Default,
    Break,
    Continue,
    Return,
    Yield,
    Try,
    Catch,
    Finally,
    Throw,
    True,
    False,
    Null,
}

impl Keyword {
    pub fn from_word(word: &str) -> Option<Keyword> {
        use Keyword::*;
        Some(match word {
            "let" => Let,
            "const" => Const,
            "type" => Type,
            "use" => Use,
            "pub" => Pub,
            "if" => If,
            "else" => Else,
            "for" => For,
            "in" => In,
            "while" => While,
            "switch" => Switch,
            "case" => Case,
            "default" => Default,
            "break" => Break,
            "continue" => Continue,
            "return" => Return,
            "yield" => Yield,
            "try" => Try,
            "catch" => Catch,
            "finally" => Finally,
            "throw" => Throw,
            "true" => True,
            "false" => False,
            "null" => Null,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        use Keyword::*;
        match self {
            Let => "let",
            Const => "const",
            Type => "type",
            Use => "use",
            Pub => "pub",
            If => "if",
            Else => "else",
            For => "for",
            In => "in",
            While => "while",
            Switch => "switch",
            Case => "case",
            Default => "default",
            Break => "break",
            Continue => "continue",
            Return => "return",
            Yield => "yield",
            Try => "try",
            Catch => "catch",
            Finally => "finally",
            Throw => "throw",
            True => "true",
            False => "false",
            Null => "null",
        }
    }
}

/// The test operators: `-f`, `-like`, `-notin`...
pub const TEST_OPERATORS: &[&str] = &[
    "f", "d", "x", "r", "w", "like", "match", "in", "notlike", "notmatch", "notin",
];

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Values
    Int(i64),
    Float(f64),
    Str(String),
    FStr(Vec<StringPart>),
    Regex { pattern: String, flags: String },
    Command(Vec<StringPart>),
    WordList(Vec<String>),
    SymbolList(Vec<String>),
    Symbol(String),

    // Names
    Variable(String),
    Ident(String),
    Keyword(Keyword),
    /// A test operator, without the `-` (`"f"`, `"like"`...).
    TestOp(String),

    // Arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,
    PlusPlus,
    MinusMinus,

    // Comparison and logic
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    AndAnd,
    OrOr,
    Bang,

    // Bitwise
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    /// `>>`. The parser splits it into two `>` when it closes nested generics.
    Shr,
    UShr,

    // Assignment
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    StarStarEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    UShrEq,

    // Null operators and the ternary
    Question,
    QuestionDot,
    QuestionQuestion,
    /// `?[`, the nullable index.
    QuestionBracket,

    // Punctuation
    Colon,
    ColonColon,
    Dot,
    DotDot,
    DotDotEq,
    Ellipsis,
    FatArrow,
    Arrow,
    Comma,
    Semicolon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,

    Eof,
}

impl TokenKind {
    /// The token as it's written in code, for error messages.
    pub fn describe(&self) -> String {
        use TokenKind::*;
        let s = match self {
            Int(n) => return n.to_string(),
            Float(n) => return n.to_string(),
            Str(_) | FStr(_) => "a string",
            Regex { .. } => "a regex",
            Command(_) => "a command",
            WordList(_) => "a %w list",
            SymbolList(_) => "a %i list",
            Symbol(name) => return format!(":{name}"),
            Variable(name) => return format!("${name}"),
            Ident(name) => return format!("`{name}`"),
            Keyword(k) => return format!("`{}`", k.as_str()),
            TestOp(op) => return format!("`-{op}`"),
            Plus => "`+`",
            Minus => "`-`",
            Star => "`*`",
            Slash => "`/`",
            Percent => "`%`",
            StarStar => "`**`",
            PlusPlus => "`++`",
            MinusMinus => "`--`",
            EqEq => "`==`",
            NotEq => "`!=`",
            Lt => "`<`",
            Gt => "`>`",
            LtEq => "`<=`",
            GtEq => "`>=`",
            AndAnd => "`&&`",
            OrOr => "`||`",
            Bang => "`!`",
            Amp => "`&`",
            Pipe => "`|`",
            Caret => "`^`",
            Tilde => "`~`",
            Shl => "`<<`",
            Shr => "`>>`",
            UShr => "`>>>`",
            Eq => "`=`",
            PlusEq => "`+=`",
            MinusEq => "`-=`",
            StarEq => "`*=`",
            SlashEq => "`/=`",
            PercentEq => "`%=`",
            StarStarEq => "`**=`",
            AmpEq => "`&=`",
            PipeEq => "`|=`",
            CaretEq => "`^=`",
            ShlEq => "`<<=`",
            ShrEq => "`>>=`",
            UShrEq => "`>>>=`",
            Question => "`?`",
            QuestionDot => "`?.`",
            QuestionQuestion => "`??`",
            QuestionBracket => "`?[`",
            Colon => "`:`",
            ColonColon => "`::`",
            Dot => "`.`",
            DotDot => "`..`",
            DotDotEq => "`..=`",
            Ellipsis => "`...`",
            FatArrow => "`=>`",
            Arrow => "`->`",
            Comma => "`,`",
            Semicolon => "`;`",
            LParen => "`(`",
            RParen => "`)`",
            LBracket => "`[`",
            RBracket => "`]`",
            LBrace => "`{`",
            RBrace => "`}`",
            Eof => "the end of the file",
        };
        s.to_string()
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe())
    }
}
