//! Commands: `%x"..."` and `process::command()` *(see Command in
//! `ideas/language/builtin_types.md` and `ideas/stdlib/types/command.md`)*.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::value::{NativeContext, NativeError, Value};

pub const COMMAND_FAILED: &str = "CommandFailed";

pub enum CommandKind {
    /// Run through `sh -c`, with the values already quoted.
    Shell(String),
    /// Run directly, without a shell.
    Direct { program: String, args: Vec<String> },
}

pub struct CommandValue {
    pub kind: CommandKind,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub input: Option<String>,
    pub quiet: bool,
    pub timeout: Option<f64>,
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
        CommandValue { kind, cwd: None, env: Vec::new(), input: None, quiet: false, timeout: None }
    }

    /// The command as the shell receives it.
    pub fn display(&self) -> String {
        match &self.kind {
            CommandKind::Shell(script) => script.clone(),
            CommandKind::Direct { program, args } => {
                let mut parts = vec![quote_if_needed(program)];
                parts.extend(args.iter().map(|a| quote_if_needed(a)));
                parts.join(" ")
            }
        }
    }
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

/// Runs a command.
pub fn execute(command: &CommandValue, mode: Mode, ctx: &mut NativeContext) -> Result<Value, NativeError> {
    let shown = command.display();
    let mut process = match &command.kind {
        CommandKind::Shell(script) => {
            if cfg!(windows) {
                return Err(failed(
                    "commands written with %x\"...\" aren't supported on Windows yet. Use process::command() to run a program directly",
                ));
            }
            let mut p = Command::new("sh");
            p.arg("-c").arg(script);
            p
        }
        CommandKind::Direct { program, args } => {
            let mut p = Command::new(program);
            p.args(args);
            p
        }
    };
    ctx.state.prepare(&mut process);
    if let Some(cwd) = &command.cwd {
        process.current_dir(ctx.state.resolve(cwd));
    }
    for (name, value) in &command.env {
        process.env(name, value);
    }

    let terminal = ctx.inherit_stdio;
    let stdout = match mode {
        Mode::Output => Stdio::piped(),
        Mode::Succeeds => Stdio::null(),
        _ if terminal => Stdio::inherit(),
        _ => Stdio::piped(),
    };
    let stderr = if command.quiet || mode == Mode::Succeeds {
        Stdio::null()
    } else if terminal {
        Stdio::inherit()
    } else {
        Stdio::piped()
    };
    let stdin = if command.input.is_some() {
        Stdio::piped()
    } else if terminal && mode != Mode::Succeeds {
        Stdio::inherit()
    } else {
        Stdio::null()
    };
    process.stdout(stdout).stderr(stderr).stdin(stdin);

    // What the script printed so far comes before what the command prints
    let _ = ctx.output.flush();
    let _ = ctx.error.flush();

    let mut child = process.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => failed(format!("can't run `{shown}`: the program wasn't found")),
        _ => failed(format!("can't run `{shown}`: {e}")),
    })?;

    let writer = match (child.stdin.take(), &command.input) {
        (Some(mut stdin), Some(text)) => {
            let text = text.clone();
            Some(std::thread::spawn(move || {
                let _ = stdin.write_all(text.as_bytes());
            }))
        }
        _ => None,
    };
    let read_all = |pipe: Option<Box<dyn Read + Send>>| {
        pipe.map(|mut p| {
            std::thread::spawn(move || {
                let mut buffer = Vec::new();
                let _ = p.read_to_end(&mut buffer);
                buffer
            })
        })
    };
    let out_reader = read_all(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err_reader = read_all(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));

    let status = match command.timeout {
        None => child.wait().map_err(|e| failed(format!("`{shown}` failed: {e}")))?,
        Some(seconds) => {
            let deadline = Instant::now() + Duration::from_secs_f64(seconds.max(0.0));
            loop {
                if let Some(status) = child.try_wait().map_err(|e| failed(format!("`{shown}` failed: {e}")))? {
                    break status;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(failed(format!("`{shown}` didn't finish in {seconds} seconds")));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    };
    if let Some(w) = writer {
        let _ = w.join();
    }
    let out = out_reader.map(|r| r.join().unwrap_or_default()).unwrap_or_default();
    let err = err_reader.map(|r| r.join().unwrap_or_default()).unwrap_or_default();

    // When the output isn't the terminal, it goes where the script's goes
    if mode != Mode::Output {
        let _ = ctx.output.write_all(&out);
    }
    let _ = ctx.error.write_all(&err);

    let code = exit_code(&status);
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
