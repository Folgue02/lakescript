//! Colors for errors and help texts in a terminal.

use lakescript::terminal::{Paint, When};
use lakescript::{CapturedOutput, Engine};

fn error() -> lakescript::Diagnostic {
    Engine::with_output(CapturedOutput::new()).run("number $a = 1;\n$a = 'x';").unwrap_err()
}

#[test]
fn errors_are_the_same_without_colors() {
    let source = "number $a = 1;\n$a = 'x';";
    let e = error();
    assert_eq!(e.render_with_source_painted("s.lks", source, Paint(false)), e.render_with_source("s.lks", source));
    assert_eq!(e.render_painted("s.lks", Paint(false)), e.render("s.lks"));
    assert!(!e.render_with_source("s.lks", source).contains('\x1b'));
}

#[test]
fn errors_have_colors_like_rust() {
    let text = error().render_with_source_painted("s.lks", "number $a = 1;\n$a = 'x';", Paint(true));
    assert!(text.starts_with("\x1b[1;31merror:\x1b[0m \x1b[1m"), "{text:?}");
    assert!(text.contains("\x1b[1;34m-->\x1b[0m s.lks:2:6"), "{text:?}");
    assert!(text.contains("\x1b[1;31m^^^\x1b[0m"), "{text:?}");
}

#[test]
fn help_texts_have_headings_and_options_in_color() {
    let help = Paint(true).help("Runs it.\n\nUsage: x [options]\n\nOptions:\n  -h, --help  Show this help\n              more about it\n");
    assert!(help.contains("\x1b[1;32mUsage:\x1b[0m"));
    assert!(help.contains("\x1b[1;32mOptions:\x1b[0m"));
    assert!(help.contains("  \x1b[1;36m-h, --help\x1b[0m  Show this help\n"));
    assert!(help.contains("\n              more about it\n"));
    assert!(help.starts_with("Runs it.\n"));
    assert_eq!(Paint(false).help("Usage: x\n  -h  Help\n"), "Usage: x\n  -h  Help\n");
}

#[test]
fn when_to_use_colors() {
    assert_eq!(When::parse("always"), Some(When::Always));
    assert_eq!(When::parse("sometimes"), None);
    assert!(When::Always.colors(false));
    assert!(!When::Never.colors(true));
}
