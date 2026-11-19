//! The public way to run Lakescript, for `lks` and for any program that
//! embeds it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::diagnostic::Diagnostic;
use crate::host::{Host, ScriptState};
use crate::interpreter::Interpreter;
use crate::modules::Loader;
use crate::scope::{self, Env};
use crate::value::{Function, NativeContext, NativeError, NativeFunction, NativeResult, Value};

/// How a script ended, when it didn't fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It ran to the end.
    Finished,
    /// It called `exit()` with this code.
    Exit(i32),
}

impl Outcome {
    /// The exit code for the process: `0`, or the code given to `exit()`.
    pub fn code(self) -> i32 {
        match self {
            Outcome::Finished => 0,
            Outcome::Exit(code) => code,
        }
    }
}

/// Runs scripts. Every engine is independent: they share nothing, and a
/// script never changes the real process's folder or environment.
///
/// ```
/// use lakescript::{Engine, CapturedOutput};
///
/// let output = CapturedOutput::new();
/// let mut engine = Engine::with_output(output.clone());
/// engine.run("outln(f\"{1 + 2}\");").unwrap();
/// assert_eq!(output.text(), "3\n");
/// ```
pub struct Engine {
    globals: Env,
    host: Host,
    stack_budget: usize,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// An engine connected to the process's stdin, stdout and stderr, like `lks`.
    pub fn new() -> Self {
        let mut engine = Self::build(Box::new(std::io::stdout()), Box::new(std::io::stderr()));
        engine.host.input = Box::new(std::io::BufReader::new(std::io::stdin()));
        engine.host.input_piped = !std::io::stdin().is_terminal();
        engine.host.inherit_stdio = true;
        engine
    }

    /// An engine that writes what scripts print to `output`. Its stderr is
    /// the process's, and its stdin is empty.
    pub fn with_output(output: impl Write + 'static) -> Self {
        Self::build(Box::new(output), Box::new(std::io::stderr()))
    }

    fn build(output: Box<dyn Write>, error: Box<dyn Write>) -> Self {
        let builtins = scope::root_scope();
        let globals = scope::new_scope(&builtins);
        let host = Host {
            output,
            error,
            input: Box::new(std::io::empty()),
            input_piped: true,
            inherit_stdio: false,
            exit_allowed: true,
            regexes: HashMap::new(),
            state: ScriptState::new(),
            loader: Loader::new(),
            builtins,
        };
        let mut engine = Engine { globals, host, stack_budget: 1024 * 1024 };
        engine.register_builtins();
        engine
    }

    /// Where `err` and `errln` write.
    pub fn set_error_output(&mut self, error: impl Write + 'static) {
        self.host.error = Box::new(error);
    }

    /// What `std/io` reads. `piped` is what `io::piped?()` answers.
    pub fn set_input(&mut self, input: impl BufRead + 'static, piped: bool) {
        self.host.input = Box::new(input);
        self.host.input_piped = piped;
    }

    /// The command-line arguments, for `process::args()`.
    pub fn set_args(&mut self, args: Vec<String>) {
        self.host.state.args = args;
    }

    /// Whether `exit()` may stop a script. When it's not allowed, `exit()`
    /// throws an exception instead. It's allowed by default.
    pub fn set_exit_allowed(&mut self, allowed: bool) {
        self.host.exit_allowed = allowed;
    }

    /// Adds a folder where `use` looks for modules, after the ones added before.
    pub fn add_module_root(&mut self, folder: impl Into<PathBuf>) {
        self.host.loader.roots.push(folder.into());
    }

    /// Leaves a module of the standard library out *(i.e. `"std/process"`)*,
    /// so scripts can't `use` it.
    pub fn disable_module(&mut self, name: &str) {
        self.host.loader.disabled.insert(name.to_string());
    }

    /// How many bytes of stack a script may use before it stops with a "too
    /// many nested calls" error, instead of overflowing the stack and crashing
    /// the program. The default, 1 MB, is safe on threads with a stack of 2 MB
    /// or more. Give the script a thread with a bigger stack to allow deeper
    /// recursion.
    pub fn set_stack_budget(&mut self, bytes: usize) {
        self.stack_budget = bytes;
    }

    /// Adds a function that scripts can call. Returning `Err` throws an
    /// exception in the script: a plain message is an `Exception`, and a
    /// `NativeError` can choose the kind *(i.e. `ValueError`)*.
    pub fn register(&mut self, name: &str, func: impl Fn(&mut NativeContext, Vec<Value>) -> NativeResult + 'static) {
        let function = Function::Native(NativeFunction { name: name.to_string(), params: Vec::new(), func: Box::new(func) });
        // Replacing a built-in on purpose is allowed, so the host can override `outln`
        self.host.builtins.borrow_mut().remove_function(name);
        scope::declare_function(&self.host.builtins, name, Rc::new(function));
    }

    /// Runs a script given as text. Variables and functions it declares are
    /// kept, so the next `run` can use them.
    pub fn run(&mut self, source: &str) -> Result<Outcome, Diagnostic> {
        let program = crate::parse(source)?;
        let mut interpreter = Interpreter::new(&mut self.host, self.stack_budget);
        let result = interpreter.run(&program, &self.globals);
        let _ = self.host.output.flush();
        let _ = self.host.error.flush();
        result.map_err(|d| self.locate(d))
    }

    /// Runs a script file. Its folder is where `use` looks first (the folder
    /// of the real file, if it's reached through a link), and
    /// `process::scriptPath()` returns it.
    pub fn run_file(&mut self, path: &Path) -> Result<Outcome, Diagnostic> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| Diagnostic::new(format!("can't read {}: {e}", path.display()), Default::default()))?;
        let real = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(folder) = real.parent() {
            self.host.loader.roots.insert(0, folder.to_path_buf());
        }
        self.host.loader.files[0] = path.display().to_string();
        self.host.state.script_path = Some(real);
        self.run(&source)
    }

    /// Fills in the file of an error that happened in a module.
    fn locate(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        let file = diagnostic.span.file as usize;
        if file > 0 {
            diagnostic.file = self.host.loader.files.get(file).cloned();
        }
        diagnostic
    }

    fn register_builtins(&mut self) {
        self.register("outln", |ctx, args| print(&mut *ctx.output, args, "outln", true));
        self.register("out", |ctx, args| print(&mut *ctx.output, args, "out", false));
        self.register("errln", |ctx, args| print(&mut *ctx.error, args, "errln", true));
        self.register("err", |ctx, args| print(&mut *ctx.error, args, "err", false));
        self.register("exit", |ctx, args| {
            let code = match args.as_slice() {
                [] => 0,
                [v] => match v.as_whole() {
                    Some(n) if (0..=255).contains(&n) => n as i32,
                    _ => return Err(NativeError::new("TypeError", "`exit` takes a whole number from 0 to 255")),
                },
                _ => return Err(NativeError::new("TypeError", "`exit` takes one exit code")),
            };
            if !ctx.exit_allowed {
                return Err("`exit` isn't allowed here".into());
            }
            ctx.exit = Some(code);
            Ok(Value::Null)
        });
        for (name, _) in crate::exceptions::TREE {
            self.register(name, move |_, args| match args.as_slice() {
                [] => Ok(Value::Exception(crate::exceptions::new(name, "", Default::default()))),
                [Value::Str(message)] => Ok(Value::Exception(crate::exceptions::new(name, &**message, Default::default()))),
                _ => Err(format!("`{name}` takes one message string").into()),
            });
        }
    }
}

fn print(to: &mut dyn Write, args: Vec<Value>, name: &str, newline: bool) -> NativeResult {
    let text = match args.as_slice() {
        [] => String::new(),
        [value] => value.to_string(),
        _ => {
            return Err(NativeError::new(
                "TypeError",
                format!("`{name}` takes one value. Use an f-string to print several *(i.e. f\"{{$a}} {{$b}}\")*"),
            ));
        }
    };
    let result = if newline { writeln!(to, "{text}") } else { write!(to, "{text}") };
    result.map_err(|e| NativeError::new("IOError", format!("can't write the output: {e}")))?;
    Ok(Value::Null)
}

/// Output kept in memory, to read what a script printed. Clones share the
/// same buffer.
#[derive(Clone, Default)]
pub struct CapturedOutput(Rc<RefCell<Vec<u8>>>);

impl CapturedOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow()).into_owned()
    }
}

impl Write for CapturedOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
