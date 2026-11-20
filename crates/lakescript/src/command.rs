//! Commands: `%x"..."` and `process::command()` *(see Command in
//! `ideas/language/builtin_types.md` and `ideas/stdlib/types/command.md`)*.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::value::{NativeContext, NativeError, Value};

pub const COMMAND_FAILED: &str = "CommandFailed";

#[derive(Clone)]
pub enum CommandKind {
    /// `process::shell()`: run through `sh -c`, or `cmd /C` on Windows.
    Shell(String),
    /// Run directly, without a shell: `%x"..."` and `process::command()`.
    Direct { program: String, args: Vec<String> },
}

#[derive(Clone)]
pub struct CommandValue {
    pub kind: CommandKind,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub input: Option<String>,
    /// `.readFrom()`: a file for its stdin.
    pub read_from: Option<String>,
    /// `.writeTo()` or `.appendTo()` (`true`): a file for its stdout.
    pub write_to: Option<(String, bool)>,
    pub quiet: bool,
    pub timeout: Option<f64>,
    /// `.pipe()`: the command its output goes to.
    pub next: Option<Box<CommandValue>>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    /// Output to the terminal, throws if it fails.
    Run,
    /// Returns stdout, throws if it fails.
    Output,
    /// Output to the terminal, returns the exit code.
    Status,
    /// No output, returns whether it worked.
    Succeeds,
}

impl CommandValue {
    pub fn new(kind: CommandKind) -> Self {
        CommandValue { kind, cwd: None, env: Vec::new(), input: None, read_from: None, write_to: None, quiet: false, timeout: None, next: None }
    }

    /// The commands of a pipeline, in order: just this one without `.pipe()`.
    pub fn stages(&self) -> Vec<&CommandValue> {
        let mut out = vec![self];
        while let Some(next) = &out.last().unwrap().next {
            out.push(next);
        }
        out
    }

    /// Changes every command of a pipeline.
    pub fn each_mut(&mut self, mut f: impl FnMut(&mut CommandValue)) {
        let mut current = Some(self);
        while let Some(c) = current {
            f(c);
            current = c.next.as_deref_mut();
        }
    }

    /// The last command of a pipeline: where its output goes.
    pub fn last_mut(&mut self) -> &mut CommandValue {
        match self.next {
            Some(ref mut next) => next.last_mut(),
            None => self,
        }
    }

    /// `$a.pipe($b)`: a copy of this one, whose output goes to `$b`.
    pub fn piped_to(&self, other: &CommandValue) -> CommandValue {
        let mut copy = self.clone();
        let mut tail = &mut copy;
        while tail.next.is_some() {
            tail = tail.next.as_mut().unwrap();
        }
        tail.next = Some(Box::new(other.clone()));
        copy
    }

    /// The command as it will run, for errors and `toString()`.
    pub fn display(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for stage in self.stages() {
            parts.push(match &stage.kind {
                CommandKind::Shell(script) => script.clone(),
                CommandKind::Direct { program, args } => {
                    let mut words = vec![quote_if_needed(program)];
                    words.extend(args.iter().map(|a| quote_if_needed(a)));
                    words.join(" ")
                }
            });
        }
        let mut text = parts.join(" | ");
        if let Some(file) = &self.read_from {
            text.push_str(&format!(" < {}", quote_if_needed(file)));
        }
        if let Some((file, append)) = &self.stages().last().unwrap().write_to {
            text.push_str(&format!(" {} {}", if *append { ">>" } else { ">" }, quote_if_needed(file)));
        }
        text
    }
}

/// Splits the text of `%x"..."` into the program and its arguments. `parts`
/// are the text written in it (strings) and the values of its `{...}`
/// (lists of arguments). Spaces separate arguments, `'...'` and `"..."` keep
/// spaces in one, and a value next to text is part of the same argument.
pub fn split_command(parts: &[Value]) -> Result<(String, Vec<String>), String> {
    let mut words: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    let mut quote: Option<char> = None;
    for part in parts {
        match part {
            Value::Str(text) => {
                for c in text.chars() {
                    match (quote, c) {
                        (Some(q), c) if c == q => quote = None,
                        (Some(_), c) => current.get_or_insert_with(String::new).push(c),
                        (None, '\'' | '"') => {
                            quote = Some(c);
                            current.get_or_insert_with(String::new);
                        }
                        (None, c) if c.is_whitespace() => {
                            if let Some(word) = current.take() {
                                words.push(word);
                            }
                        }
                        (None, c) => current.get_or_insert_with(String::new).push(c),
                    }
                }
            }
            Value::List(values) => {
                let values = values.borrow();
                let glued = current.is_some() || quote.is_some();
                if glued {
                    // Next to text, or inside quotes, a value is part of that argument
                    if values.len() != 1 {
                        return Err("a list in a command is several arguments, so it can't be next to other text or inside quotes".into());
                    }
                    current.get_or_insert_with(String::new).push_str(&values[0].to_string());
                } else if values.len() == 1 {
                    current = Some(values[0].to_string());
                } else {
                    words.extend(values.iter().map(|v| v.to_string()));
                }
            }
            _ => unreachable!("the compiler gives strings and lists"),
        }
    }
    if let Some(q) = quote {
        return Err(format!("a `{q}` in the command is never closed"));
    }
    if let Some(word) = current.take() {
        words.push(word);
    }
    if words.is_empty() {
        return Err("the command is empty: it needs at least the program to run".into());
    }
    let program = words.remove(0);
    Ok((program, words))
}

/// Quotes a value for `sh`, so it's always a single argument and never runs as code.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn quote_if_needed(text: &str) -> String {
    let safe = !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,@%+".contains(c));
    if safe { text.to_string() } else { shell_quote(text) }
}

fn failed(message: impl Into<String>) -> NativeError {
    NativeError::new(COMMAND_FAILED, message)
}

/// Runs a command, or a pipeline of commands.
pub fn execute(command: &CommandValue, mode: Mode, ctx: &mut NativeContext) -> Result<Value, NativeError> {
    let shown = command.display();
    let stages = command.stages();
    let last = stages.len() - 1;
    let output_file = stages[last].write_to.clone();
    if mode == Mode::Output && output_file.is_some() {
        return Err(NativeError::new(
            crate::exceptions::VALUE_ERROR,
            format!("`{shown}` writes its output to a file, so `.output()` has nothing to return. Use `.run()`"),
        ));
    }
    let terminal = ctx.inherit_stdio;

    // What the script printed so far comes before what the commands print
    let _ = ctx.output.flush();
    let _ = ctx.error.flush();

    let mut children: Vec<std::process::Child> = Vec::new();
    let mut previous_stdout = None;
    let mut writer = None;
    let mut err_readers = Vec::new();
    for (i, stage) in stages.iter().enumerate() {
        let mut process = match &stage.kind {
            CommandKind::Shell(script) => {
                let mut p = if cfg!(windows) { Command::new("cmd") } else { Command::new("sh") };
                p.arg(if cfg!(windows) { "/C" } else { "-c" }).arg(script);
                p
            }
            CommandKind::Direct { program, args } => {
                let mut p = Command::new(program);
                p.args(args);
                p
            }
        };
        ctx.state.prepare(&mut process);
        if let Some(cwd) = &stage.cwd {
            process.current_dir(ctx.state.resolve(cwd));
        }
        for (name, value) in &stage.env {
            process.env(name, value);
        }
        // stdin: the command before it, the first one's input or file, or the terminal
        let stdin = if let Some(out) = previous_stdout.take() {
            Stdio::from(out)
        } else if command.input.is_some() {
            Stdio::piped()
        } else if let Some(file) = &command.read_from {
            let path = ctx.state.resolve(file);
            Stdio::from(std::fs::File::open(&path).map_err(|e| failed(format!("can't read '{file}' for `{shown}`: {e}")))?)
        } else if terminal && mode != Mode::Succeeds {
            Stdio::inherit()
        } else {
            Stdio::null()
        };
        let stdout = if i < last {
            Stdio::piped()
        } else if let Some((file, append)) = &output_file {
            let path = ctx.state.resolve(file);
            let opened = std::fs::OpenOptions::new().create(true).write(true).append(*append).truncate(!*append).open(&path);
            Stdio::from(opened.map_err(|e| failed(format!("can't write '{file}' for `{shown}`: {e}")))?)
        } else {
            match mode {
                Mode::Output => Stdio::piped(),
                Mode::Succeeds => Stdio::null(),
                _ if terminal => Stdio::inherit(),
                _ => Stdio::piped(),
            }
        };
        let stderr = if stage.quiet || mode == Mode::Succeeds {
            Stdio::null()
        } else if terminal {
            Stdio::inherit()
        } else {
            Stdio::piped()
        };
        process.stdin(stdin).stdout(stdout).stderr(stderr);
        let mut child = process.spawn().map_err(|e| {
            for c in children.iter_mut() {
                let _ = c.kill();
            }
            match e.kind() {
                std::io::ErrorKind::NotFound => failed(format!("can't run `{shown}`: the program wasn't found")),
                _ => failed(format!("can't run `{shown}`: {e}")),
            }
        })?;
        if i == 0 {
            if let (Some(mut stdin), Some(text)) = (child.stdin.take(), &command.input) {
                let text = text.clone();
                writer = Some(std::thread::spawn(move || {
                    let _ = stdin.write_all(text.as_bytes());
                }));
            }
        }
        if i < last {
            previous_stdout = child.stdout.take();
        }
        err_readers.push(read_all(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>)));
        children.push(child);
    }
    let out_reader = read_all(children[last].stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));

    // Every command must finish; the timeout is for all of them
    let deadline = command.timeout.map(|s| Instant::now() + Duration::from_secs_f64(s.max(0.0)));
    let mut codes = Vec::new();
    for child in children.iter_mut() {
        let status = match deadline {
            None => child.wait().map_err(|e| failed(format!("`{shown}` failed: {e}")))?,
            Some(deadline) => loop {
                if let Some(status) = child.try_wait().map_err(|e| failed(format!("`{shown}` failed: {e}")))? {
                    break status;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(failed(format!("`{shown}` didn't finish in {} seconds", command.timeout.unwrap_or(0.0))));
                }
                std::thread::sleep(Duration::from_millis(5));
            },
        };
        codes.push(exit_code(&status));
    }
    if let Some(w) = writer {
        let _ = w.join();
    }
    let out = out_reader.map(|r| r.join().unwrap_or_default()).unwrap_or_default();
    let mut err = Vec::new();
    for r in err_readers.into_iter().flatten() {
        err.extend(r.join().unwrap_or_default());
    }

    // When the output isn't the terminal, it goes where the script's goes
    if mode != Mode::Output {
        let _ = ctx.output.write_all(&out);
    }
    let _ = ctx.error.write_all(&err);

    // Like `set -o pipefail`: a pipeline fails when any of its commands does
    let code = codes.iter().rev().find(|c| **c != 0).copied().unwrap_or(0);
    let check = |code: i32| -> Result<(), NativeError> {
        if code == 0 { Ok(()) } else { Err(failed(format!("`{shown}` failed with exit code {code}"))) }
    };
    match mode {
        Mode::Run => {
            check(code)?;
            Ok(Value::Null)
        }
        Mode::Output => {
            check(code)?;
            let mut text = String::from_utf8_lossy(&out).into_owned();
            if text.ends_with('\n') {
                text.pop();
                if text.ends_with('\r') {
                    text.pop();
                }
            }
            Ok(Value::string(&text))
        }
        Mode::Status => Ok(Value::Int(code as i64)),
        Mode::Succeeds => Ok(Value::Bool(code == 0)),
    }
}

/// Reads all of a pipe in a thread, so a command can't block on a full one.
fn read_all(pipe: Option<Box<dyn Read + Send>>) -> Option<std::thread::JoinHandle<Vec<u8>>> {
    pipe.map(|mut p| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = p.read_to_end(&mut buffer);
            buffer
        })
    })
}

fn exit_code(status: &std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}
