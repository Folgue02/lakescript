use crate::token::Span;

/// An error found in a script, with the place it points to.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    /// The path of the module the error is in, when it isn't the script
    /// being run. Filled in by the engine.
    pub file: Option<String>,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Diagnostic { message: message.into(), span, file: None }
    }

    /// The simple first version from the notes: `file:line:column: error: message`.
    /// `path` is the name of the script being run, used when the error isn't in a module.
    pub fn render(&self, path: &str) -> String {
        let path = self.file.as_deref().unwrap_or(path);
        format!(
            "{path}:{}:{}: error: {}",
            self.span.line, self.span.column, self.message
        )
    }
}
