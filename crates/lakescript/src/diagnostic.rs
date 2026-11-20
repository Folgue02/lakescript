use crate::terminal::Paint;
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
        self.render_painted(path, Paint(false))
    }

    /// `render`, with colors when `paint` has them.
    pub fn render_painted(&self, path: &str, paint: Paint) -> String {
        let path = self.file.as_deref().unwrap_or(path);
        format!("{path}:{}:{}: {} {}", self.span.line, self.span.column, paint.error("error:"), paint.bold(&self.message))
    }

    /// Like Rust's errors: the message, where it is, and the line of code
    /// with a marker under the part that's wrong. `source` is the text of
    /// the file the error is in.
    ///
    /// ```text
    /// error: `$age` is a number, so it can't hold a string
    ///  --> script.lks:2:8
    ///   |
    /// 2 | $age = 'old';
    ///   |        ^^^^^
    /// ```
    pub fn render_with_source(&self, path: &str, source: &str) -> String {
        self.render_with_source_painted(path, source, Paint(false))
    }

    /// `render_with_source`, with colors when `paint` has them, like Rust's.
    pub fn render_with_source_painted(&self, path: &str, source: &str, paint: Paint) -> String {
        let path = self.file.as_deref().unwrap_or(path);
        let line_no = self.span.line as usize;
        let Some(line) = source.lines().nth(line_no.saturating_sub(1)) else {
            return self.render_painted(path, paint);
        };
        let column = (self.span.column as usize).max(1);
        let line_chars = line.chars().count();
        // Under the whole span when it's on this line, under one character otherwise
        let length = if self.span.end > self.span.start { self.span.end - self.span.start } else { 1 };
        let length = length.min(line_chars.saturating_sub(column - 1)).max(1);
        let gutter = " ".repeat(line_no.to_string().len());
        // Tabs are kept, so the marker lines up with the code above it
        let padding: String = line.chars().take(column - 1).map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
        let bar = paint.frame("|");
        format!(
            "{} {}\n{gutter}{} {path}:{}:{}\n{gutter} {bar}\n{} {bar} {line}\n{gutter} {bar} {padding}{}",
            paint.error("error:"),
            paint.bold(&self.message),
            paint.frame("-->"),
            self.span.line,
            self.span.column,
            paint.frame(&line_no.to_string()),
            paint.error(&"^".repeat(length))
        )
    }
}
