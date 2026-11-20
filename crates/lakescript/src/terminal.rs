//! Colors for programs that show Lakescript's errors in a terminal, like
//! `lks` and `ferry`. Nothing here prints: it only builds text.

/// When to use colors, as `--color` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum When {
    /// Only on a terminal, unless `NO_COLOR` or `CLICOLOR_FORCE` say otherwise.
    #[default]
    Auto,
    Always,
    Never,
}

impl When {
    pub fn parse(text: &str) -> Option<When> {
        match text {
            "auto" => Some(When::Auto),
            "always" => Some(When::Always),
            "never" => Some(When::Never),
            _ => None,
        }
    }

    /// Whether to use colors on a stream. `NO_COLOR` turns them off and
    /// `CLICOLOR_FORCE` on *(see no-color.org)*, and a terminal that says
    /// it's `dumb` gets none.
    pub fn colors(self, is_terminal: bool) -> bool {
        let set = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0");
        match self {
            When::Always => true,
            When::Never => false,
            When::Auto if set("NO_COLOR") => false,
            When::Auto if set("CLICOLOR_FORCE") => true,
            When::Auto => is_terminal && std::env::var_os("TERM").is_none_or(|t| t != "dumb"),
        }
    }
}

/// Paints text with colors, or leaves it as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Paint(pub bool);

impl Paint {
    fn wrap(self, code: &str, text: &str) -> String {
        if self.0 && !text.is_empty() { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
    }

    /// Red: errors and the marker under the code that's wrong.
    pub fn error(self, text: &str) -> String {
        self.wrap("1;31", text)
    }

    /// Yellow: warnings.
    pub fn warning(self, text: &str) -> String {
        self.wrap("1;33", text)
    }

    /// Cyan: notes and hints, and names of commands and options.
    pub fn note(self, text: &str) -> String {
        self.wrap("1;36", text)
    }

    /// Green: what went well, and headings.
    pub fn good(self, text: &str) -> String {
        self.wrap("1;32", text)
    }

    /// Blue: the frame around the code in an error, `-->`, line numbers and `|`.
    pub fn frame(self, text: &str) -> String {
        self.wrap("1;34", text)
    }

    pub fn bold(self, text: &str) -> String {
        self.wrap("1", text)
    }

    pub fn dim(self, text: &str) -> String {
        self.wrap("2", text)
    }

    /// Colors a help text: headings like `Options:` in green, and the
    /// commands and options they list in cyan. A listed item is a line that
    /// starts with two spaces, up to the next two spaces in a row.
    pub fn help(self, text: &str) -> String {
        let mut out = String::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("Usage:") {
                out.push_str(&format!("{}{}", self.good("Usage:"), self.bold(rest)));
            } else if !line.starts_with(' ') && line.ends_with(':') {
                out.push_str(&self.good(line));
            } else if let Some(item) = line.strip_prefix("  ").filter(|l| !l.starts_with(' ')) {
                let end = item.find("  ").unwrap_or(item.len());
                out.push_str(&format!("  {}{}", self.note(&item[..end]), &item[end..]));
            } else {
                out.push_str(line);
            }
            out.push('\n');
        }
        out
    }
}
