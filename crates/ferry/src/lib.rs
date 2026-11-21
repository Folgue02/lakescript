//! ferry's projects, repositories and dependencies, also used by `lks-lsp`
//! to find the modules of a project *(see `ideas/tooling/ferry.md`)*.

use std::io::IsTerminal;
use std::sync::OnceLock;

use lakescript::terminal::{Paint, When};

pub mod doc;
pub mod home;
pub mod project;
pub mod resolve;

/// The colors for stderr, where ferry says what it does.
pub static PAINT: OnceLock<Paint> = OnceLock::new();

pub fn paint() -> Paint {
    *PAINT.get_or_init(|| Paint(When::Auto.colors(std::io::stderr().is_terminal())))
}

/// A line about what ferry is doing, like cargo's: the verb in green, lined up on the right.
pub fn status(verb: &str, text: &str) {
    eprintln!("{} {text}", paint().good(&format!("{verb:>12}")));
}

/// Why ferry stopped. The message is already what the user sees.
#[derive(Debug)]
pub struct Fail(pub String);

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Fail(message)
    }
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Fail {
    fn from(message: &str) -> Self {
        Fail(message.to_string())
    }
}

