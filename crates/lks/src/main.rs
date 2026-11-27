use std::io::IsTerminal;
// Faster for the many small values scripts make and drop *(see `lakescript::SmallAlloc`)*
#[global_allocator]
static ALLOCATOR: lakescript::SmallAlloc = lakescript::SmallAlloc;

use std::path::Path;
use std::sync::OnceLock;
use std::process::ExitCode;

use lakescript::terminal::{Paint, When};
use lakescript::Engine;

/// The interpreter walks the syntax tree with Rust's own stack, so scripts
/// run on a thread with a big one. Only the pages that are used take memory.
const STACK_SIZE: usize = 512 * 1024 * 1024;

const USAGE: &str = "\
Runs a Lakescript script.

Usage: lks [options] <script> [arguments for the script...]

Options go before the script. Everything after the script is passed to it,
even if it looks like an option of lks.

Options:
  --module-path <folders>
                   More folders where `use` looks for modules,
                   separated by `:`
  --color <when>   When to use colors: auto (on a terminal, the default),
                   always or never. NO_COLOR also turns them off
  --resources <folder>
                   Where std/resources reads the files of the script,
                   instead of resources/ next to it
  --name <name>    The name of the command in the Usage: line of the
                   script's --help, instead of the name of its file
  --no-type-check  Run the script without checking its types first,
                   for scripts that were already checked
  --check          Check the script and the modules it uses without
                   running it. Says nothing when there's no error
  --message-format <format>
                   How errors are written on stderr: text (the default)
                   or json, one object per line, for tools and agents
  --test           Run the tests of the script, its functions named test...,
                   instead of the script. An argument after the script
                   runs only the tests whose name has it
  -h, --help       Show this help
  -V, --version    Show the version of lks
  --               End the options: the next argument is the script,
                   even if it starts with `-`

For debugging lks itself:
  --tokens         Print the tokens of the script instead of running it
  --ast            Print the syntax tree of the script instead of running it
  --bytecode       Print the bytecode of the script instead of running it
";

fn main() -> ExitCode {
    let runner = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(run);
    match runner.map(|handle| handle.join()) {
        Ok(Ok(code)) => code,
        _ => {
            problem("the interpreter crashed. This is a bug in lks, not in your script");
            ExitCode::from(70)
        }
    }
}

#[derive(Default)]
struct Options {
    no_type_check: bool,
    /// `--check`: check the script without running it.
    check: bool,
    test: bool,
    show_tokens: bool,
    show_ast: bool,
    show_bytecode: bool,
    module_path: Vec<std::path::PathBuf>,
    /// `--name`: what the script is run as, for its `Usage:` line.
    name: Option<String>,
    /// `--resources`: where `std/resources` reads from.
    resources: Option<std::path::PathBuf>,
    color: When,
    /// `--message-format json`: errors as JSON, one object per line.
    json: bool,
}

/// The colors for stderr, decided once the options are read.
static PAINT: OnceLock<Paint> = OnceLock::new();

/// Whether errors are JSON *(`--message-format json`)*, decided once the options are read.
static JSON: OnceLock<bool> = OnceLock::new();

fn json() -> bool {
    *JSON.get().unwrap_or(&false)
}

/// An error that isn't at a place in the code *(i.e. a wrong command line)*,
/// as `lks: message` or as JSON.
fn problem(message: &str) {
    if json() {
        eprintln!("{}", lakescript::json_message("error", message, None));
    } else {
        eprintln!("{} {message}", paint().error("lks:"));
    }
}

fn paint() -> Paint {
    *PAINT.get_or_init(|| Paint(When::Auto.colors(std::io::stderr().is_terminal())))
}

/// What the command line asks for.
enum Request {
    Run { options: Options, script: String, args: Vec<String> },
    Help(When),
    Version,
}

/// The options of `lks` end at the first argument that isn't one: that's the
/// script, and every argument after it belongs to the script.
/// A wrong one is an error, with whether it asked for JSON before it.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Request, (String, bool)> {
    let mut options = Options::default();
    parse_options(args, &mut options).map_err(|message| (message, options.json))
}

fn parse_options(mut args: impl Iterator<Item = String>, options: &mut Options) -> Result<Request, String> {
    let script = loop {
        let Some(arg) = args.next() else {
            return Err("no script given".into());
        };
        match arg.as_str() {
            "--no-type-check" => options.no_type_check = true,
            "--check" => options.check = true,
            "--message-format" => match args.next().as_deref() {
                Some("text") => options.json = false,
                Some("json") => options.json = true,
                _ => return Err("`--message-format` needs `text` or `json` after it".into()),
            },
            "--test" => options.test = true,
            "--tokens" => options.show_tokens = true,
            "--ast" => options.show_ast = true,
            "--bytecode" => options.show_bytecode = true,
            "--color" => match args.next().as_deref().and_then(When::parse) {
                Some(when) => options.color = when,
                None => return Err("`--color` needs `auto`, `always` or `never` after it".into()),
            },
            "--module-path" => match args.next() {
                Some(folders) => options.module_path.extend(std::env::split_paths(&folders)),
                None => return Err("`--module-path` needs the folders after it".into()),
            },
            "--resources" => match args.next() {
                Some(folder) => options.resources = Some(folder.into()),
                None => return Err("`--resources` needs the folder after it".into()),
            },
            "--name" => match args.next() {
                Some(name) => options.name = Some(name),
                None => return Err("`--name` needs the name of the command after it".into()),
            },
            "-h" | "--help" => return Ok(Request::Help(options.color)),
            "-V" | "--version" => return Ok(Request::Version),
            "--" => match args.next() {
                Some(script) => break script,
                None => return Err("no script given after `--`".into()),
            },
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            _ => break arg,
        }
    };
    let args: Vec<String> = args.collect();
    if options.check {
        let wrong = if !args.is_empty() {
            Some("`lks --check` takes only the script, since it doesn't run it")
        } else if options.test {
            Some("`--check` checks a script without running it, and `--test` runs its tests: use one of them")
        } else if options.no_type_check {
            Some("`--check` checks the types of the script, and `--no-type-check` doesn't: use one of them")
        } else {
            None
        };
        if let Some(wrong) = wrong {
            return Err(wrong.into());
        }
    }
    Ok(Request::Run { options: std::mem::take(options), script, args })
}

fn run() -> ExitCode {
    let (mut options, script, args) = match parse_args(std::env::args().skip(1)) {
        Ok(Request::Run { options, script, args }) => (options, script, args),
        Ok(Request::Help(when)) => {
            print!("{}", Paint(when.colors(std::io::stdout().is_terminal())).help(USAGE));
            return ExitCode::SUCCESS;
        }
        Ok(Request::Version) => {
            println!("lks {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err((message, json)) => {
            let _ = JSON.set(json);
            if json {
                problem(&format!("{message}. Usage: lks [options] <script> [arguments for the script...]"));
            } else {
                let paint = paint();
                eprintln!("{} {message}", paint.error("lks:"));
                eprintln!("{} lks [options] <script> [arguments for the script...]. See `lks --help`", paint.good("Usage:"));
            }
            return ExitCode::from(2);
        }
    };

    let _ = PAINT.set(Paint(options.color.colors(std::io::stderr().is_terminal())));
    let _ = JSON.set(options.json);
    if options.show_tokens || options.show_ast || options.show_bytecode {
        let source = match std::fs::read_to_string(&script) {
            Ok(source) => source,
            Err(err) => {
                error(&format!("can't read {script}: {err}"), &script);
                return ExitCode::FAILURE;
            }
        };
        return debug_print(&script, &source, &options);
    }

    let mut engine = Engine::new();
    on_ctrl_c(engine.interrupt_flag());
    engine.set_stack_budget(STACK_SIZE - 16 * 1024 * 1024);
    engine.set_args(args.clone());
    engine.set_type_check(!options.no_type_check);
    if let Some(name) = &options.name {
        engine.set_command_name(name);
    }
    if let Some(folder) = &options.resources {
        engine.set_resources(folder);
    }
    for folder in options.module_path.drain(..) {
        engine.add_module_root(folder);
    }
    if let Some(extra) = std::env::var_os("LKS_PATH") {
        for folder in std::env::split_paths(&extra) {
            engine.add_module_root(folder);
        }
    }
    // A bundle made by `ferry bundle`, which checked its types already
    let bundle = lakescript::is_bundle(Path::new(&script));
    if options.check {
        if bundle {
            problem(&format!("{script} is a bundle, which `ferry bundle` checked when it made it: `lks --check` checks a script"));
            return ExitCode::from(2);
        }
        let result = engine.check_file(Path::new(&script));
        // Warnings first, which never change the exit code
        show_warnings(&engine, &script);
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(diagnostic) => {
                show_all(&engine, diagnostic, &script);
                ExitCode::FAILURE
            }
        };
    }
    if options.test {
        if bundle {
            problem(&format!("`lks --test` runs the tests of a script, and {script} is a bundle: its project's tests run with `ferry test`"));
            return ExitCode::from(2);
        }
        return test(&mut engine, &script, &args, options.color);
    }
    let result = if bundle {
        engine.set_type_check(false);
        engine.run_bundle(Path::new(&script))
    } else {
        engine.run_file(Path::new(&script))
    };
    match result {
        Ok(outcome) => ExitCode::from(outcome.code().clamp(0, 255) as u8),
        // Arguments that don't match the script's command: the message and
        // the usage, without the code, and code 2 like most command-line tools
        Err(ref diagnostic) if let Some(text) = diagnostic.usage() => {
            if json() {
                eprintln!("{}", show(diagnostic, &script));
            } else {
                eprintln!("{}{text}", paint().error("error: "));
            }
            ExitCode::from(2)
        }
        Err(diagnostic) => {
            show_all(&engine, diagnostic, &script);
            ExitCode::FAILURE
        }
    }
}

/// `lks --test`: the tests of the script, and how many failed. `args` can
/// have a filter for their names.
fn test(engine: &mut Engine, script: &str, args: &[String], color: When) -> ExitCode {
    let filter = match args {
        [] => None,
        [filter] => Some(filter.as_str()),
        _ => {
            problem("`lks --test` takes the script and one filter at most");
            return ExitCode::from(2);
        }
    };
    let out = Paint(color.colors(std::io::stdout().is_terminal()));
    let start = std::time::Instant::now();
    let results = match engine.run_tests(Path::new(script), filter) {
        Ok(results) => results,
        Err(diagnostic) => {
            show_all(engine, diagnostic, script);
            return ExitCode::FAILURE;
        }
    };
    for result in &results {
        print!("{}", lakescript::render_result(result, script, out));
    }
    print!("{}", lakescript::render_summary(&results, start.elapsed().as_secs_f64(), out));
    if results.iter().all(|r| r.passed()) { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Ctrl+C throws an `Interrupted` in the script, which can catch it *(see
/// ideas/language/errors.md)*. A second one before the script got to it
/// stops it, for a script stuck waiting *(i.e. reading stdin)*.
fn on_ctrl_c(flag: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    lakescript::on_ctrl_c(flag);
}

/// The errors of a check or a run that failed: every type error the check
/// found *(see Every error at once in ideas/tooling/lsp.md)*, or the one
/// error otherwise. As text, with a blank line between them.
fn show_all(engine: &Engine, diagnostic: lakescript::Diagnostic, script: &str) {
    let errors = if engine.errors().is_empty() { vec![diagnostic] } else { engine.errors().to_vec() };
    let shown: Vec<String> = errors.iter().map(|d| show(d, script)).collect();
    eprintln!("{}", shown.join(if json() { "\n" } else { "\n\n" }));
}

/// The warnings of a check *(see Warnings in ideas/tooling/diagnostics.md)*:
/// only `lks --check` shows them, never a run.
fn show_warnings(engine: &Engine, script: &str) {
    for warning in engine.warnings() {
        let file = warning.file.as_deref().unwrap_or(script);
        if json() {
            let source = std::fs::read_to_string(file).ok();
            eprintln!("{}", warning.render_json("warning", script, source.as_deref()));
        } else {
            let source = std::fs::read_to_string(file).ok();
            eprintln!("{}\n", warning.render_warning_painted(script, source.as_deref(), paint()));
        }
    }
}

/// An error about a file, not at a place in it *(i.e. it can't be read)*.
fn error(message: &str, file: &str) {
    if json() {
        eprintln!("{}", lakescript::json_message("error", message, Some(file)));
    } else {
        eprintln!("{} {message}", paint().error("error:"));
    }
}

/// An error with the line of code it points to, read from the file it's in,
/// or as JSON.
fn show(diagnostic: &lakescript::Diagnostic, script: &str) -> String {
    let file = diagnostic.file.as_deref().unwrap_or(script);
    // A bundle is a zip: its code isn't shown from it
    let bundle = lakescript::is_bundle(Path::new(file));
    if json() {
        let source = std::fs::read_to_string(file).ok().filter(|_| !bundle);
        return diagnostic.render_json("error", script, source.as_deref());
    }
    // What isn't at a place in the code *(i.e. a bundle that can't be read)*: only the message
    if diagnostic.span.line == 0 && diagnostic.trace.is_empty() {
        return format!("{} {}", paint().error("error:"), diagnostic.message);
    }
    match std::fs::read_to_string(file).ok().filter(|_| !bundle) {
        Some(source) if diagnostic.span.line > 0 => diagnostic.render_with_source_painted(script, &source, paint()),
        _ => diagnostic.render_painted(script, paint()),
    }
}

fn debug_print(path: &str, source: &str, options: &Options) -> ExitCode {
    let engine = Engine::new();
    let mut parts = Vec::new();
    if options.show_tokens {
        parts.push(engine.tokens(source));
    }
    if options.show_ast {
        parts.push(engine.syntax_tree(source));
    }
    if options.show_bytecode {
        parts.push(Engine::new().bytecode(source));
    }
    for part in parts {
        match part {
            Ok(text) => print!("{text}"),
            Err(diagnostic) => {
                eprintln!("{}", show(&diagnostic, path));
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
