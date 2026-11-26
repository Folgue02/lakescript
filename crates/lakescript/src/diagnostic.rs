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
    /// For an exception that wasn't caught: the calls that led to it, the
    /// first one first, shown above it *(see Tracebacks in ideas/language/errors.md)*.
    pub trace: Vec<Call>,
    /// For an exception that wasn't caught, its type *(i.e. `ValueError`)*,
    /// which the message starts with. `None` for a syntax or type error.
    pub exception: Option<String>,
}

/// A call that led to an error.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// How the function is named *(i.e. `report::build`, `the script`)*.
    pub function: String,
    /// The module it's in, or `None` for the script being run.
    pub file: Option<String>,
    pub line: u32,
    pub column: u32,
    /// More than 1 for a function that called itself from the same place.
    pub times: u32,
}

impl Call {
    /// `at report::build (/home/ana/report.lks:2:19)`, with `script` as the
    /// name of the script being run.
    pub fn render(&self, script: &str) -> String {
        let file = self.file.as_deref().unwrap_or(script);
        let times = if self.times > 1 { format!(", {} times", self.times) } else { String::new() };
        format!("at {} ({file}:{}:{}){times}", self.function, self.line, self.column)
    }
}

impl Diagnostic {
    /// For an uncaught `UsageError`, its text: arguments that don't match the
    /// script's command *(see ideas/language/cli.md)*. That's the mistake of
    /// the person running the script, not of the script, so every way to run
    /// one shows only this, with the usage, and exits with code 2.
    pub fn usage(&self) -> Option<&str> {
        self.message.strip_prefix("UsageError: ")
    }

    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Diagnostic { message: message.into(), span, file: None, trace: Vec::new(), exception: None }
    }

    /// The simple first version from the notes: `file:line:column: error: message`.
    /// `path` is the name of the script being run, used when the error isn't in a module.
    pub fn render(&self, path: &str) -> String {
        self.render_painted(path, Paint(false))
    }

    /// `render`, with colors when `paint` has them.
    pub fn render_painted(&self, path: &str, paint: Paint) -> String {
        let calls = self.calls(path, paint);
        let path = self.file.as_deref().unwrap_or(path);
        format!("{calls}{path}:{}:{}: {} {}", self.span.line, self.span.column, paint.error("error:"), paint.bold(&self.message))
    }

    /// The lines of the calls that led to it, each with its new line.
    fn calls(&self, script: &str, paint: Paint) -> String {
        self.trace.iter().map(|c| format!("  {} {}\n", paint.dim("at"), &c.render(script)[3..])).collect()
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
        let script = path;
        let path = self.file.as_deref().unwrap_or(path);
        let line_no = self.span.line as usize;
        let Some(line) = source.lines().nth(line_no.saturating_sub(1)) else {
            return self.render_painted(script, paint);
        };
        let column = (self.span.column as usize).max(1);
        let length = self.marked(line);
        let gutter = " ".repeat(line_no.to_string().len());
        // Tabs are kept, so the marker lines up with the code above it
        let padding: String = line.chars().take(column - 1).map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
        let bar = paint.frame("|");
        let calls = self.calls(script, paint);
        format!(
            "{calls}{} {}\n{gutter}{} {path}:{}:{}\n{gutter} {bar}\n{} {bar} {line}\n{gutter} {bar} {padding}{}",
            paint.error("error:"),
            paint.bold(&self.message),
            paint.frame("-->"),
            self.span.line,
            self.span.column,
            paint.frame(&line_no.to_string()),
            paint.error(&"^".repeat(length))
        )
    }

    /// How many characters of `line`, the line it's on, are marked: the
    /// whole span when it's on this line, one character otherwise.
    fn marked(&self, line: &str) -> usize {
        let column = (self.span.column as usize).max(1);
        let length = if self.span.end > self.span.start { self.span.end - self.span.start } else { 1 };
        length.min(line.chars().count().saturating_sub(column - 1)).max(1)
    }

    /// One line of JSON, for tools and agents *(`--message-format json`, see
    /// Errors as JSON in ideas/tooling/diagnostics.md)*. `level` is `error` or
    /// `warning`, `path` the name of the script being run, and `source` the
    /// text of the file it's in, for how much of the line it marks.
    ///
    /// ```text
    /// {"type":"error","message":"...","exception":null,"file":"script.lks","line":2,"column":8,"length":5,"trace":[]}
    /// ```
    pub fn render_json(&self, level: &str, path: &str, source: Option<&str>) -> String {
        let placed = self.span.line > 0;
        let file = self.file.as_deref().unwrap_or(path);
        let length = match source.filter(|_| placed).and_then(|s| s.lines().nth(self.span.line as usize - 1)) {
            Some(line) => Some(self.marked(line)),
            None if placed => Some(if self.span.end > self.span.start { self.span.end - self.span.start } else { 1 }),
            None => None,
        };
        let number = |n: Option<usize>| n.map_or("null".to_string(), |n| n.to_string());
        let trace: Vec<String> = self
            .trace
            .iter()
            .map(|c| {
                format!(
                    "{{\"function\":{},\"file\":{},\"line\":{},\"column\":{},\"times\":{}}}",
                    json_string(&c.function),
                    json_string(c.file.as_deref().unwrap_or(path)),
                    c.line,
                    c.column,
                    c.times
                )
            })
            .collect();
        format!(
            "{{\"type\":{},\"message\":{},\"exception\":{},\"file\":{},\"line\":{},\"column\":{},\"length\":{},\"trace\":[{}]}}",
            json_string(level),
            json_string(&self.message),
            self.exception.as_deref().map_or("null".to_string(), json_string),
            json_string(file),
            number(placed.then_some(self.span.line as usize)),
            number(placed.then_some(self.span.column as usize)),
            number(length),
            trace.join(",")
        )
    }
}

/// A message that isn't at a place in the code, as one line of JSON with the
/// same keys as `Diagnostic::render_json` *(i.e. a wrong command line, or a
/// note of ferry)*. `file` is the file it's about, if any.
pub fn json_message(level: &str, message: &str, file: Option<&str>) -> String {
    format!(
        "{{\"type\":{},\"message\":{},\"exception\":null,\"file\":{},\"line\":null,\"column\":null,\"length\":null,\"trace\":[]}}",
        json_string(level),
        json_string(message),
        file.map_or("null".to_string(), json_string)
    )
}

/// Text as a JSON string, with its quotes.
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
