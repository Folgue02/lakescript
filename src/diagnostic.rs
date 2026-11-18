use crate::token::Span;

/// An error found in a script, with the place it points to.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Diagnostic { message: message.into(), span }
    }

    /// The simple first version from the notes: `file:line:column: error: message`.
    pub fn render(&self, path: &str) -> String {
        format!(
            "{path}:{}:{}: error: {}",
            self.span.line, self.span.column, self.message
        )
    }
}
