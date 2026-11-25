//! `std/terminal/native`: the Rust part of `std/terminal` *(see `terminal.lks`
//! and `ideas/stdlib/terminal.md`)*. Prompts are drawn by `inquire` on stderr,
//! and the rest with `crossterm`. Without a person at a terminal, prompts read
//! their answers from stdin, a line each.

use std::cell::Cell;
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use super::{Args, Exports};
use crate::exceptions::{INTERRUPTED, TYPE_ERROR, VALUE_ERROR};
use crate::value::{Key, NativeContext, NativeError, Value};

pub fn register(m: &mut Exports) {
    m.function("style", style);
    m.function("strip", |_, values| {
        let a = Args::new("terminal::strip", values, 1, 1)?;
        Ok(Value::string(&strip(&a.str(0)?)))
    });
    m.function("visibleWidth", |_, values| {
        let a = Args::new("terminal::visibleWidth", values, 1, 1)?;
        Ok(Value::Int(width_of(&a.str(0)?) as i64))
    });
    m.function("interactive", |ctx, _| Ok(Value::Bool(interactive(ctx))));
    m.function("size", |ctx, _| {
        let size = match (ctx.inherit_stdio, crossterm::terminal::size()) {
            (true, Ok((w, h))) if w > 0 => vec![Value::Int(w as i64), Value::Int(h as i64)],
            _ => Vec::new(),
        };
        Ok(Value::list(size))
    });
    m.function("readKey", read_key);
    m.function("enterFullScreen", enter_full_screen);
    m.function("leaveFullScreen", leave_full_screen);
    m.function("moveTo", |ctx, values| {
        let a = Args::new("terminal::moveTo", values, 2, 2)?;
        let (x, y) = (a.whole(0)?, a.whole(1)?);
        if x < 0 || y < 0 || x > u16::MAX as i64 || y > u16::MAX as i64 {
            return Err(NativeError::new(VALUE_ERROR, format!("`terminal::moveTo` counts columns and rows from 0, and got {x} and {y}")));
        }
        crossterm::queue!(&mut ctx.output, crossterm::cursor::MoveTo(x as u16, y as u16)).map_err(write_failed)?;
        Ok(Value::Null)
    });
    m.function("clearLine", |ctx, _| {
        crossterm::queue!(&mut ctx.output, crossterm::terminal::Clear(crossterm::terminal::ClearType::UntilNewLine)).map_err(write_failed)?;
        Ok(Value::Null)
    });
    m.function("clear", |ctx, _| {
        write!(ctx.output, "\x1b[2J\x1b[H").and_then(|_| ctx.output.flush()).map_err(write_failed)?;
        Ok(Value::Null)
    });
    m.function("ask", ask);
    m.function("problem", problem);
    m.function("password", password);
    m.function("confirm", confirm);
    m.function("choose", choose);
    m.function("chooseMany", choose_many);
    m.function("drawProgress", draw_progress);
    m.function("filter", filter);
    m.function("pager", pager);
    m.function("table", table);
    m.function("edit", edit);
}

/// A person at a terminal: stdin and stderr are terminals, and the script
/// runs on them, not inside a program that captures what it prints. Every
/// prompt asks this first, so what the script wrote so far is written out
/// before anything is asked.
fn interactive(ctx: &mut NativeContext) -> bool {
    let _ = ctx.output.flush();
    ctx.inherit_stdio && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn write_failed(e: std::io::Error) -> NativeError {
    NativeError::new("IOError", format!("can't write to the terminal: {e}"))
}

fn interrupted() -> NativeError {
    NativeError::new(INTERRUPTED, "the script was interrupted with Ctrl+C")
}

/// What `inquire` says, as the errors of Lakescript. Ctrl+C and Escape stop
/// the prompt like Ctrl+C anywhere.
fn prompt_failed(e: inquire::InquireError) -> NativeError {
    use inquire::InquireError::*;
    match e {
        OperationInterrupted | OperationCanceled => interrupted(),
        NotTTY => NativeError::new("IOError", "there's no terminal to ask in"),
        other => NativeError::new("IOError", format!("can't ask: {other}")),
    }
}

// ---- Styles ----

/// The code of a style *(see `Style.lks`)*.
fn code(name: &str) -> Option<String> {
    const COLORS: [&str; 8] = ["BLACK", "RED", "GREEN", "YELLOW", "BLUE", "MAGENTA", "CYAN", "WHITE"];
    // From the 256 colors most terminals have
    const EXTRA: [(&str, u8); 5] = [("GRAY", 245), ("ORANGE", 208), ("PINK", 211), ("PURPLE", 135), ("BROWN", 130)];
    let (name, background) = match name.strip_prefix("ON_") {
        Some(rest) => (rest, true),
        None => (name, false),
    };
    let (base, bright) = match name.strip_prefix("BRIGHT_") {
        Some(rest) => (rest, true),
        None => (name, false),
    };
    if let Some(i) = COLORS.iter().position(|c| *c == base) {
        let start = match (background, bright) {
            (false, false) => 30,
            (false, true) => 90,
            (true, false) => 40,
            (true, true) => 100,
        };
        return Some((start + i).to_string());
    }
    if let Some((_, n)) = EXTRA.iter().find(|(c, _)| *c == name) {
        return Some(format!("{};5;{n}", if background { 48 } else { 38 }));
    }
    if background {
        return None;
    }
    Some(
        match name {
            "BOLD" => "1",
            "DIM" => "2",
            "ITALIC" => "3",
            "UNDERLINE" => "4",
            "DOUBLE_UNDERLINE" => "21",
            "CURLY_UNDERLINE" => "4:3",
            "OVERLINE" => "53",
            "STRIKE" => "9",
            "BLINK" => "5",
            "INVERSE" => "7",
            "HIDDEN" => "8",
            _ => return None,
        }
        .to_string(),
    )
}

/// Whether styles have colors: on a terminal, unless `NO_COLOR` *(see `crate::terminal::When`)*.
fn colors(ctx: &NativeContext) -> bool {
    crate::terminal::When::Auto.colors(ctx.inherit_stdio && std::io::stdout().is_terminal())
}

fn style(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::style", values, 2, 2)?;
    let text = a.str(0)?;
    let names = a.strings(1)?;
    if !colors(ctx) || names.is_empty() {
        return Ok(Value::string(&text));
    }
    let codes: Vec<String> = names.iter().filter_map(|n| code(n)).collect();
    Ok(Value::string(&format!("\x1b[{}m{text}\x1b[0m", codes.join(";"))))
}

/// The text without the codes of styles *(`ESC [ ... m`)*.
fn strip(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn width_of(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(strip(text).as_str())
}

// ---- The terminal ----

thread_local! {
    /// Whether the script is inside `terminal::fullScreen()`.
    static FULL_SCREEN: Cell<bool> = const { Cell::new(false) };
    /// The settings of the terminal before `fullScreen()` stopped showing what's typed.
    #[cfg(unix)]
    static SAVED: Cell<Option<libc::termios>> = const { Cell::new(None) };
}

/// Switches to the alternate screen, cleared and without the cursor, and
/// stops showing what's typed. `false` without a terminal, or inside
/// `fullScreen()` already, when there's nothing to do.
fn enter_full_screen(ctx: &mut NativeContext, _: Vec<Value>) -> Result<Value, NativeError> {
    use crossterm::{cursor, terminal};
    if FULL_SCREEN.get() || !interactive(ctx) || !std::io::stdout().is_terminal() {
        return Ok(Value::Bool(false));
    }
    crossterm::execute!(std::io::stdout(), terminal::EnterAlternateScreen, cursor::Hide, terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0))
        .map_err(write_failed)?;
    // Keys typed while the program draws would be shown where the cursor is
    #[cfg(unix)]
    unsafe {
        let mut settings: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut settings) == 0 {
            SAVED.set(Some(settings));
            settings.c_lflag &= !libc::ECHO;
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &settings);
        }
    }
    FULL_SCREEN.set(true);
    Ok(Value::Bool(true))
}

/// Puts the terminal back as it was before `enter_full_screen`.
fn leave_full_screen(ctx: &mut NativeContext, _: Vec<Value>) -> Result<Value, NativeError> {
    use crossterm::{cursor, terminal};
    let _ = ctx.output.flush();
    #[cfg(unix)]
    if let Some(settings) = SAVED.take() {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &settings);
        }
    }
    FULL_SCREEN.set(false);
    crossterm::execute!(std::io::stdout(), cursor::Show, terminal::LeaveAlternateScreen).map_err(write_failed)?;
    Ok(Value::Null)
}

fn read_key(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, poll, read};
    let a = Args::new("terminal::readKey", values, 0, 1)?;
    let timeout = match a.value(0) {
        Value::Null => None,
        _ => {
            let seconds = a.num(0)?;
            if !(seconds >= 0.0 && seconds.is_finite()) {
                return Err(NativeError::new(VALUE_ERROR, format!("`terminal::readKey` needs a timeout of 0 seconds or more, not {seconds}")));
            }
            Some(Instant::now() + Duration::from_secs_f64(seconds))
        }
    };
    if !interactive(ctx) {
        return Err(NativeError::new("IOError", "there's no terminal to read a key from"));
    }
    let raw = |on: bool| if on { crossterm::terminal::enable_raw_mode() } else { crossterm::terminal::disable_raw_mode() };
    raw(true).map_err(|e| NativeError::new("IOError", format!("can't read a key: {e}")))?;
    let full_screen = FULL_SCREEN.get();
    // A key, `None` when none came in time, or `'resize'` inside `fullScreen()`
    let key = loop {
        if let Some(deadline) = timeout {
            match poll(deadline.saturating_duration_since(Instant::now())) {
                Ok(true) => {}
                Ok(false) => break Ok(None),
                Err(e) => break Err(e),
            }
        }
        match read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => break Ok(Some(Ok(k))),
            Ok(Event::Resize(..)) if full_screen => break Ok(Some(Err("resize"))),
            Ok(_) => continue,
            Err(e) => break Err(e),
        }
    };
    let _ = raw(false);
    let key = match key.map_err(|e| NativeError::new("IOError", format!("can't read a key: {e}")))? {
        None => return Ok(Value::Null),
        Some(Err(event)) => return Ok(Value::string(event)),
        Some(Ok(key)) => key,
    };
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && key.code == KeyCode::Char('c') {
        return Err(interrupted());
    }
    let name = match key.code {
        KeyCode::Char(c) if ctrl => format!("ctrl+{c}"),
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "escape".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "shift+tab".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::F(n) => format!("f{n}"),
        _ => "unknown".into(),
    };
    Ok(Value::string(&name))
}

// ---- Prompts ----

/// The next line of stdin, without its new line, or `None` at its end.
fn read_line(ctx: &mut NativeContext) -> Result<Option<String>, NativeError> {
    let mut line = String::new();
    let read = ctx.input.read_line(&mut line).map_err(|e| NativeError::new("IOError", format!("can't read stdin: {e}")))?;
    if read == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim_end_matches(['\n', '\r']).to_string()))
}

/// The answer to `question` from stdin, or `default` when stdin has no more.
fn answer_from_stdin(ctx: &mut NativeContext, question: &str, default: Option<String>) -> Result<String, NativeError> {
    match (read_line(ctx)?, default) {
        (Some(line), Some(default)) if line.is_empty() => Ok(default),
        (Some(line), _) => Ok(line),
        (None, Some(default)) => Ok(default),
        (None, None) => Err(NativeError::new(VALUE_ERROR, format!("nothing answers '{question}': stdin has no more lines, and it has no default"))),
    }
}

fn optional_str(a: &Args, i: usize) -> Result<Option<String>, NativeError> {
    match a.value(i) {
        Value::Null => Ok(None),
        _ => a.str(i).map(|s| Some(s.to_string())),
    }
}

fn ask(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::ask", values, 2, 2)?;
    let question = a.str(0)?;
    let default = optional_str(&a, 1)?;
    let answer = if interactive(ctx) {
        let mut prompt = inquire::Text::new(&question);
        if let Some(d) = &default {
            prompt = prompt.with_default(d);
        }
        prompt.prompt().map_err(prompt_failed)?
    } else {
        answer_from_stdin(ctx, &question, default)?
    };
    Ok(Value::string(&answer))
}

/// What a `check:` of `ask` said was wrong: shown before asking again, or,
/// without a person to ask again, an error.
fn problem(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::ask", values, 1, 1)?;
    let text = a.str(0)?;
    if !interactive(ctx) {
        return Err(NativeError::new(VALUE_ERROR, &*text));
    }
    let paint = crate::terminal::Paint(crate::terminal::When::Auto.colors(true));
    eprintln!("{} {text}", paint.error("✗"));
    Ok(Value::Null)
}

fn password(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::password", values, 1, 1)?;
    let question = a.str(0)?;
    if !interactive(ctx) {
        return Err(NativeError::new("IOError", format!("can't ask '{question}' without a terminal: a password is never read from a pipe or a file")));
    }
    let answer = inquire::Password::new(&question).without_confirmation().prompt().map_err(prompt_failed)?;
    Ok(Value::string(&answer))
}

fn confirm(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::confirm", values, 2, 2)?;
    let question = a.str(0)?;
    let default = match a.value(1) {
        Value::Bool(b) => Some(b),
        _ => None,
    };
    if interactive(ctx) {
        let mut prompt = inquire::Confirm::new(&question);
        if let Some(d) = default {
            prompt = prompt.with_default(d);
        }
        return Ok(Value::Bool(prompt.prompt().map_err(prompt_failed)?));
    }
    let line = answer_from_stdin(ctx, &question, default.map(|d| if d { "y".to_string() } else { "n".to_string() }))?;
    match line.trim().to_lowercase().as_str() {
        "y" | "yes" | "true" => Ok(Value::Bool(true)),
        "n" | "no" | "false" => Ok(Value::Bool(false)),
        other => Err(NativeError::new(VALUE_ERROR, format!("'{other}' doesn't answer '{question}': it takes y, yes, n or no"))),
    }
}

fn choose(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::choose", values, 2, 2)?;
    let question = a.str(0)?;
    let options = a.strings(1)?;
    if options.is_empty() {
        return Err(NativeError::new(VALUE_ERROR, format!("'{question}' has no options to choose from")));
    }
    if interactive(ctx) {
        let picked = inquire::Select::new(&question, options).prompt().map_err(prompt_failed)?;
        return Ok(Value::string(&picked));
    }
    let line = answer_from_stdin(ctx, &question, None)?;
    match options.iter().find(|o| **o == line.trim()) {
        Some(o) => Ok(Value::string(o)),
        None => Err(NativeError::new(VALUE_ERROR, format!("'{}' isn't one of the options of '{question}': {}", line.trim(), options.join(", ")))),
    }
}

fn choose_many(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::chooseMany", values, 2, 2)?;
    let question = a.str(0)?;
    let options = a.strings(1)?;
    let picked: Vec<String> = if interactive(ctx) {
        inquire::MultiSelect::new(&question, options).prompt().map_err(prompt_failed)?
    } else {
        let line = answer_from_stdin(ctx, &question, Some(String::new()))?;
        let mut picked = Vec::new();
        for item in line.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if !options.iter().any(|o| o == item) {
                return Err(NativeError::new(VALUE_ERROR, format!("'{item}' isn't one of the options of '{question}': {}", options.join(", "))));
            }
            picked.push(item.to_string());
        }
        picked
    };
    Ok(super::strings(picked))
}

// ---- Progress ----

thread_local! {
    /// When a bar was last drawn, so a fast loop doesn't draw thousands of times.
    static LAST_DRAWN: Cell<Option<Instant>> = const { Cell::new(None) };
}

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn draw_progress(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::progress", values, 4, 4)?;
    // Only for a person: in a pipe or a log it would be noise
    if !ctx.inherit_stdio || !std::io::stderr().is_terminal() {
        return Ok(Value::Null);
    }
    let total = a.value(0).as_f64();
    let label = a.str(1)?;
    let done = a.num(2)?;
    let finish = matches!(a.value(3), Value::Bool(true));
    let now = Instant::now();
    let due = LAST_DRAWN.with(|l| l.get().is_none_or(|t| now.duration_since(t) >= Duration::from_millis(40)));
    if !due && !finish {
        return Ok(Value::Null);
    }
    LAST_DRAWN.with(|l| l.set(if finish { None } else { Some(now) }));
    let paint = crate::terminal::Paint(crate::terminal::When::Auto.colors(true));
    let label = if label.is_empty() { String::new() } else { format!("{label} ") };
    let line = match total {
        Some(total) if total > 0.0 => {
            let part = (done / total).clamp(0.0, 1.0);
            let width = 30;
            let full = (part * width as f64).round() as usize;
            let bar = format!("{}{}", "█".repeat(full), "░".repeat(width - full));
            format!("{label}{} {:>3}% {}/{}", paint.good(&bar), (part * 100.0).round(), crate::value::format_number(done), crate::value::format_number(total))
        }
        _ if finish => format!("{label}{} {}", paint.good("✓"), crate::value::format_number(done)),
        _ => format!("{label}{} {}", paint.good(&SPINNER[done as usize % SPINNER.len()].to_string()), crate::value::format_number(done)),
    };
    let mut err = std::io::stderr();
    let end = if finish { "\n" } else { "" };
    write!(err, "\r\x1b[2K{line}{end}").and_then(|_| err.flush()).map_err(write_failed)?;
    Ok(Value::Null)
}

// ---- Ready-made pieces ----

fn filter(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::filter", values, 2, 2)?;
    let question = a.str(0)?;
    let items = a.strings(1)?;
    if interactive(ctx) {
        return match inquire::Select::new(&question, items).with_page_size(12).prompt() {
            Ok(item) => Ok(Value::string(&item)),
            Err(inquire::InquireError::OperationCanceled) => Ok(Value::Null),
            Err(e) => Err(prompt_failed(e)),
        };
    }
    // Without a terminal, a line of stdin that's one of the items
    let line = read_line(ctx)?.unwrap_or_default();
    Ok(items.iter().find(|i| **i == line.trim()).map(|i| Value::string(i)).unwrap_or(Value::Null))
}

/// Writes text through `$PAGER`, or `less`, when the output is a terminal.
fn show_paged(ctx: &mut NativeContext, text: &str) -> Result<(), NativeError> {
    if ctx.inherit_stdio && std::io::stdout().is_terminal() {
        let pager = ctx.state.env_get("PAGER").filter(|p| !p.trim().is_empty()).unwrap_or_else(|| "less -R".to_string());
        let mut words = pager.split_whitespace();
        if let Some(program) = words.next() {
            let child = std::process::Command::new(program).args(words).stdin(std::process::Stdio::piped()).spawn();
            if let Ok(mut child) = child {
                if let Some(mut input) = child.stdin.take() {
                    let _ = input.write_all(text.as_bytes());
                }
                let _ = child.wait();
                return Ok(());
            }
        }
    }
    ctx.output.write_all(text.as_bytes()).and_then(|_| ctx.output.flush()).map_err(write_failed)
}

fn pager(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::pager", values, 1, 1)?;
    let mut text = a.str(0)?.to_string();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    show_paged(ctx, &text)?;
    Ok(Value::Null)
}

fn table(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::table", values, 2, 2)?;
    let Value::List(rows) = a.value(0) else { return Err(NativeError::new(TYPE_ERROR, "the rows of a table are a list")) };
    let rows = rows.borrow().clone();
    let mut header: Option<Vec<String>> = match a.value(1) {
        Value::Null => None,
        _ => Some(a.strings(1)?),
    };
    // Maps give the header with their keys, in the order of the first
    if header.is_none()
        && let Some(Value::Map(first)) = rows.first()
    {
        header = Some(first.borrow().keys().map(|k| k.to_value().to_string()).collect());
    }
    let mut cells: Vec<Vec<String>> = Vec::new();
    for row in &rows {
        cells.push(match row {
            Value::List(items) => items.borrow().iter().map(cell).collect(),
            Value::Map(map) => {
                let map = map.borrow();
                header.as_ref().map(|h| h.iter().map(|k| map.get(&Key::Str(k.as_str().into())).map(cell).unwrap_or_default()).collect()).unwrap_or_default()
            }
            other => vec![cell(other)],
        });
    }
    let columns = header.as_ref().map(Vec::len).unwrap_or(0).max(cells.iter().map(Vec::len).max().unwrap_or(0));
    let mut widths = vec![0; columns];
    for row in header.iter().chain(cells.iter()) {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(width_of(c));
        }
    }
    let line = |row: &[String]| -> String {
        let parts: Vec<String> = (0..columns)
            .map(|i| {
                let c = row.get(i).map(String::as_str).unwrap_or("");
                let pad = widths[i] - width_of(c);
                if i + 1 == columns { c.to_string() } else { format!("{c}{}", " ".repeat(pad)) }
            })
            .collect();
        parts.join("  ").trim_end().to_string()
    };
    let paint = crate::terminal::Paint(colors(ctx));
    let mut text = String::new();
    if let Some(h) = &header {
        text.push_str(&paint.bold(&line(h)));
        text.push('\n');
    }
    for row in &cells {
        text.push_str(&line(row));
        text.push('\n');
    }
    // Through the pager when it doesn't fit
    let (width, height) = crossterm::terminal::size().map(|(w, h)| (w as usize, h as usize)).unwrap_or((usize::MAX, usize::MAX));
    let wide = widths.iter().sum::<usize>() + 2 * columns.saturating_sub(1);
    if interactive(ctx) && (wide > width || text.lines().count() + 1 > height) {
        show_paged(ctx, &text)?;
    } else {
        ctx.output.write_all(text.as_bytes()).and_then(|_| ctx.output.flush()).map_err(write_failed)?;
    }
    Ok(Value::Null)
}

/// A value in a cell, as `outln` writes it, and `null` as nothing.
fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn edit(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("terminal::edit", values, 1, 1)?;
    let text = a.str(0)?;
    if !interactive(ctx) {
        return Err(NativeError::new("IOError", "can't open an editor without a terminal"));
    }
    let fallback = if cfg!(windows) { "notepad" } else { "vi" };
    let editor = ["VISUAL", "EDITOR"].iter().find_map(|v| ctx.state.env_get(v).filter(|e| !e.trim().is_empty())).unwrap_or_else(|| fallback.to_string());
    let file = std::env::temp_dir().join(format!("lakescript-edit-{}-{}.txt", std::process::id(), Instant::now().elapsed().as_nanos()));
    std::fs::write(&file, text.as_bytes()).map_err(|e| NativeError::new("IOError", format!("can't write the file to edit: {e}")))?;
    let mut words = editor.split_whitespace();
    let program = words.next().unwrap_or(fallback);
    let status = std::process::Command::new(program).args(words).arg(&file).status();
    let edited = std::fs::read_to_string(&file);
    let _ = std::fs::remove_file(&file);
    match status {
        Ok(s) if s.success() => edited.map(|t| Value::string(&t)).map_err(|e| NativeError::new("IOError", format!("can't read the edited file: {e}"))),
        Ok(s) => Err(NativeError::new("IOError", format!("the editor `{editor}` stopped with {s}"))),
        Err(e) => Err(NativeError::new("IOError", format!("can't open the editor `{editor}`: {e}"))),
    }
}
