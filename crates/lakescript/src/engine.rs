//! The public way to run Lakescript, for `lks` and for any program that
//! embeds it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use crate::diagnostic::Diagnostic;
use crate::interpreter::Interpreter;
use crate::scope::{self, Env};
use crate::value::{Function, NativeContext, NativeFunction, RegexValue, Value};

/// Runs scripts. Every engine is independent: they share nothing.
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
    builtins: Env,
    globals: Env,
    output: Box<dyn Write>,
    stack_budget: usize,
    regexes: HashMap<String, Rc<RegexValue>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// An engine that writes to the process's stdout.
    pub fn new() -> Self {
        Self::with_output(std::io::stdout())
    }

    /// An engine that writes what scripts print to `output`.
    pub fn with_output(output: impl Write + 'static) -> Self {
        let builtins = scope::root_scope();
        let globals = scope::new_scope(&builtins);
        let mut engine = Engine { builtins, globals, output: Box::new(output), stack_budget: 1024 * 1024, regexes: HashMap::new() };
        engine.register_builtins();
        engine
    }

    /// How many bytes of stack a script may use before it stops with a "too
    /// many nested calls" error, instead of overflowing the stack and crashing
    /// the program. The default, 1 MB, is safe on threads with a stack of 2 MB
    /// or more. Give the script a thread with a bigger stack to allow deeper
    /// recursion.
    pub fn set_stack_budget(&mut self, bytes: usize) {
        self.stack_budget = bytes;
    }

    /// Adds a function that scripts can call. Returning `Err` stops the script
    /// with that message.
    pub fn register(
        &mut self,
        name: &str,
        func: impl Fn(&mut NativeContext, Vec<Value>) -> Result<Value, String> + 'static,
    ) {
        let function = Function::Native(NativeFunction { name: name.to_string(), func: Box::new(func) });
        // Replacing a built-in on purpose is allowed, so the host can override `outln`
        self.builtins.borrow_mut().remove_function(name);
        scope::declare_function(&self.builtins, name, Rc::new(function));
    }

    /// Runs a script. Variables and functions it declares are kept, so the
    /// next `run` can use them.
    pub fn run(&mut self, source: &str) -> Result<(), Diagnostic> {
        let program = crate::parse(source)?;
        let mut interpreter = Interpreter::new(&mut *self.output, self.stack_budget, &mut self.regexes);
        let result = interpreter.run(&program, &self.globals);
        let _ = self.output.flush();
        result
    }

    fn register_builtins(&mut self) {
        self.register("outln", |ctx, args| print(ctx, args, "outln", true));
        self.register("out", |ctx, args| print(ctx, args, "out", false));
        for (name, _) in crate::exceptions::TREE {
            self.register(name, move |_, args| match args.as_slice() {
                [] => Ok(Value::Exception(crate::exceptions::new(name, "", Default::default()))),
                [Value::Str(message)] => Ok(Value::Exception(crate::exceptions::new(name, &**message, Default::default()))),
                _ => Err(format!("`{name}` takes one message string")),
            });
        }
    }
}

fn print(ctx: &mut NativeContext, args: Vec<Value>, name: &str, newline: bool) -> Result<Value, String> {
    let text = match args.as_slice() {
        [] => String::new(),
        [value] => value.to_string(),
        _ => return Err(format!("`{name}` takes one value. Use an f-string to print several *(i.e. f\"{{$a}} {{$b}}\")*")),
    };
    let result = if newline { writeln!(ctx.output, "{text}") } else { write!(ctx.output, "{text}") };
    result.map_err(|e| format!("can't write the output: {e}"))?;
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
