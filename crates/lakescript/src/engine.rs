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
use crate::modules::Globals;
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
    pub(crate) globals: Rc<Globals>,
    pub(crate) host: Host,
    pub(crate) stack_budget: usize,
    /// Whether scripts are type checked before they run.
    check_types: bool,
    /// The signatures of the built-in methods and the standard library, read the first time they're needed.
    prelude: Option<Rc<crate::checker::prelude::Prelude>>,
    check_state: crate::checker::State,
    check_cache: HashMap<PathBuf, Result<crate::checker::prelude::ModuleSig, Diagnostic>>,
    check_classes: HashMap<(PathBuf, String), crate::checker::types::ClassRef>,
    check_declared: HashMap<PathBuf, crate::checker::prelude::ModuleSig>,
    /// Functions registered by the program embedding Lakescript. The checker knows nothing about their types.
    host_functions: Vec<String>,
    builtins_ready: bool,
    /// Whether checks note what they find about each name *(see `set_names`)*.
    record_names: bool,
    found: Vec<crate::checker::names::Found>,
    /// The script of the last check, to find its names in.
    checked_source: String,
    /// Where the next checks look for what can be written *(see `set_completion`)*.
    completion: Option<crate::checker::complete::Request>,
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
        let globals = Rc::new(Globals::default());
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
            builtins: Vec::new(),
            builtin_names: HashMap::new(),
            rng: crate::host::seed(),
        };
        let mut engine = Engine {
            globals,
            host,
            stack_budget: 1024 * 1024,
            check_types: true,
            prelude: None,
            check_state: Default::default(),
            check_cache: HashMap::new(),
            check_classes: HashMap::new(),
            check_declared: HashMap::new(),
            host_functions: Vec::new(),
            builtins_ready: false,
            record_names: false,
            found: Vec::new(),
            checked_source: String::new(),
            completion: None,
        };
        engine.register_builtins();
        engine.builtins_ready = true;
        engine
    }

    /// Whether scripts are type checked before they run. It's on by default.
    /// Since types never change what the code does, a script that passed the
    /// check behaves the same without it.
    pub fn set_type_check(&mut self, check: bool) {
        self.check_types = check;
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
    /// many nested calls" or "nested too deeply" error, instead of overflowing
    /// the stack and crashing the program. The default, 1 MB, is safe on
    /// threads with a stack of 2 MB or more. The stack is used to read deeply
    /// nested code and when Rust code calls a script's function *(i.e.
    /// `.map()`)*. Calls between a script's functions don't use it, but their
    /// depth is limited by this too: one call for every 64 bytes, from 256 up
    /// to 100,000.
    pub fn set_stack_budget(&mut self, bytes: usize) {
        self.stack_budget = bytes;
    }

    /// Adds a function that scripts can call. Returning `Err` throws an
    /// exception in the script: a plain message is an `Exception`, and a
    /// `NativeError` can choose the kind *(i.e. `ValueError`)*.
    pub fn register(&mut self, name: &str, func: impl Fn(&mut NativeContext, Vec<Value>) -> NativeResult + 'static) {
        if self.builtins_ready {
            self.host_functions.push(name.to_string());
        }
        let function = Value::Function(Rc::new(Function::Native(NativeFunction { name: name.to_string(), params: Vec::new(), func: Box::new(func) })));
        self.add_builtin(name, function);
    }

    /// A value every script can use without `use`.
    fn add_builtin(&mut self, name: &str, function: Value) {
        // Replacing a built-in on purpose is allowed, so the host can override `outln`
        match self.host.builtin_names.get(name) {
            Some(&i) => self.host.builtins[i as usize] = function,
            None => {
                self.host.builtin_names.insert(name.to_string(), self.host.builtins.len() as u32);
                self.host.builtins.push(function);
            }
        }
    }

    /// Runs a script given as text. Variables and functions it declares are
    /// kept, so the next `run` can use them.
    pub fn run(&mut self, source: &str) -> Result<Outcome, Diagnostic> {
        let program = crate::parse_budgeted(source, 0, self.stack_budget)?;
        if self.check_types {
            self.check(&program).map_err(|d| self.locate(d))?;
        }
        let compiler = crate::compiler::Compiler::new(&self.globals, &self.host.builtin_names, self.stack_budget);
        let proto = compiler.script(&program).map_err(|d| self.locate(d))?;
        let mut interpreter = Interpreter::new(&mut self.host, self.stack_budget);
        let result = interpreter.run(proto, self.globals.clone());
        let _ = self.host.output.flush();
        let _ = self.host.error.flush();
        result.map_err(|d| self.locate(d))
    }

    /// The bytecode a script compiles to, as text, without running it. It's
    /// for debugging Lakescript itself *(`lks --bytecode`)*: the bytecode
    /// changes between versions.
    pub fn bytecode(&mut self, source: &str) -> Result<String, Diagnostic> {
        let program = crate::parse_budgeted(source, 0, self.stack_budget)?;
        let globals = Globals::default();
        let compiler = crate::compiler::Compiler::new(&globals, &self.host.builtin_names, self.stack_budget);
        Ok(compiler.script(&program)?.listing())
    }

    /// Runs a script file. Its folder is where `use` looks first (the folder
    /// of the real file, if it's reached through a link), and
    /// `process::scriptPath()` returns it.
    pub fn run_file(&mut self, path: &Path) -> Result<Outcome, Diagnostic> {
        let source = self.open_file(path)?;
        self.run(&source)
    }

    /// Type checks a script without running it. Its folder is where `use`
    /// looks first, like with `run_file`, and the modules it uses are checked too.
    pub fn check_file(&mut self, path: &Path) -> Result<(), Diagnostic> {
        let source = self.open_file(path)?;
        self.check_source(&source)
    }

    /// Type checks a script given as text without running it.
    pub fn check_source(&mut self, source: &str) -> Result<(), Diagnostic> {
        self.found.clear();
        if self.record_names {
            self.checked_source = source.to_string();
        }
        if !self.editing() {
            let program = crate::parse_budgeted(source, 0, self.stack_budget)?;
            return self.check(&program).map_err(|d| self.locate(d));
        }
        // For an editor, the code is checked also after a syntax error, for its names
        let (program, errors) = crate::parser::parse_recovering(crate::lexer::tokenize_file(source, 0)?, self.stack_budget);
        let result = self.check(&program).map_err(|d| self.locate(d));
        match errors.into_iter().next() {
            Some(error) => Err(error),
            None => result,
        }
    }

    /// Whether the checks are for an editor: they go on after syntax errors.
    fn editing(&self) -> bool {
        self.record_names || self.completion.is_some()
    }

    /// Makes the checks find what can be written at a place: the names in
    /// scope there, and the members of the value before a `.` *(see
    /// `completion`)*. `file` is the file as errors name it, or `None` for
    /// the script being checked. `offset` counts characters.
    pub fn set_completion(&mut self, file: Option<String>, offset: usize) {
        self.completion = Some(crate::checker::complete::Request { file, offset, result: None });
    }

    /// What the last check found at the place given to `set_completion`.
    pub fn completion(&mut self) -> Option<crate::checker::complete::Completion> {
        self.completion.as_mut()?.result.take()
    }

    /// Makes the checks note what they find about each name: its type, where
    /// it's declared and its `///` comment, for editors. `names` gives them.
    /// It's off by default. A module already checked by this engine isn't
    /// checked again, so its names are only in the first check that uses it.
    pub fn set_names(&mut self, record: bool) {
        self.record_names = record;
    }

    /// The names the last check found, up to its first error, when `set_names` is on.
    pub fn names(&mut self) -> Vec<crate::checker::names::Name> {
        let found = std::mem::take(&mut self.found);
        let loader = &self.host.loader;
        let script = &self.checked_source;
        crate::checker::names::locate(found, |file| match file {
            None => Some(script.clone()),
            Some(path) => loader.read(Path::new(path)).ok(),
        })
    }

    /// Uses this text for a file instead of what's on the disk, for every
    /// later check or run: the file an editor has open, before it's saved.
    pub fn set_file_text(&mut self, path: &Path, text: impl Into<String>) {
        let real = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.host.loader.overlay.insert(real, text.into());
    }

    /// Type checks a script given as text, as if it were the file at `path`:
    /// its folder is where `use` looks first, like with `check_file`.
    pub fn check_text(&mut self, path: &Path, text: &str) -> Result<(), Diagnostic> {
        self.use_script_path(path);
        self.check_source(text)
    }

    /// Reads a script and makes its folder the first place `use` looks.
    pub(crate) fn open_file(&mut self, path: &Path) -> Result<String, Diagnostic> {
        let real = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let source = self
            .host
            .loader
            .read(&real)
            .map_err(|e| Diagnostic::new(format!("can't read {}: {e}", path.display()), Default::default()))?;
        self.use_script_path(path);
        Ok(source)
    }

    fn use_script_path(&mut self, path: &Path) {
        let real = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(folder) = real.parent()
            && !self.host.loader.roots.iter().any(|r| r == folder)
        {
            self.host.loader.roots.insert(0, folder.to_path_buf());
        }
        self.host.loader.files[0] = path.display().to_string();
        self.host.state.script_path = Some(real);
    }

    fn check(&mut self, program: &[crate::ast::Stmt]) -> Result<(), Diagnostic> {
        let prelude = self.prelude.get_or_insert_with(|| Rc::new(crate::checker::prelude::Prelude::load())).clone();
        let recover = self.editing();
        let modules = crate::checker::Modules {
            loader: &self.host.loader,
            cache: &mut self.check_cache,
            checking: Vec::new(),
            classes: &mut self.check_classes,
            declared: &mut self.check_declared,
            declaring: Vec::new(),
            names: if self.record_names { Some(&mut self.found) } else { None },
            complete: self.completion.as_mut(),
            recover,
        };
        let mut checker = crate::checker::Checker::new(&prelude, modules, &mut self.check_state, &self.host_functions);
        checker.stack_budget = self.stack_budget;
        checker.check_program(program)
    }

    /// Fills in the file of an error that happened in a module.
    pub(crate) fn locate(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        let file = diagnostic.span.file as usize;
        if file > 0 && diagnostic.file.is_none() {
            diagnostic.file = self.host.loader.files.get(file).cloned();
        }
        diagnostic
    }

    fn register_builtins(&mut self) {
        self.register("outln", |ctx, args| print(&mut *ctx.output, args, "outln", true));
        // What a failed `assert` calls *(see `assertion` in compiler.rs)*
        self.register("__assertFailed", |_, args| {
            let text = match args.first() {
                Some(Value::Str(s)) => s.to_string(),
                _ => String::new(),
            };
            let mut message = format!("assert failed: {text}");
            if let [_, left, right] = args.as_slice() {
                message.push_str(&format!("\n  left:  {}\n  right: {}", left.nested(), right.nested()));
                // `==` compares lists and maps by reference
                let what = match (left, right) {
                    (Value::List(_), Value::List(_)) => Some("lists"),
                    (Value::Map(_), Value::Map(_)) => Some("maps"),
                    _ => None,
                };
                if let Some(what) = what.filter(|_| left.equals(right) && !left.same(right)) {
                    message.push_str(&format!("\n  They're two {what} with the same values: `==` checks if they're the same {}, and `.equals()` compares their values", &what[..what.len() - 1]));
                }
            }
            Err(NativeError::new(crate::exceptions::ASSERTION_ERROR, message))
        });
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
        for (name, method, _) in crate::operators::INTERFACES {
            let class = crate::value::Class {
                name: Rc::from(*name),
                interface: true,
                exception: None,
                is_enum: false,
                values: std::cell::RefCell::new(Vec::new()),
                parent: None,
                interfaces: Vec::new(),
                required: vec![Rc::from(*method)],
                fields: Vec::new(),
                methods: HashMap::new(),
                statics: std::cell::RefCell::new(indexmap::IndexMap::new()),
                constructor: None,
                init: None,
                command: None,
            };
            self.add_builtin(name, Value::Class(Rc::new(class)));
        }
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
