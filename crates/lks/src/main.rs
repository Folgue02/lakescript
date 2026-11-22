use std::io::IsTerminal;
use std::path::Path;
use std::sync::OnceLock;
use std::process::ExitCode;

use lakescript::terminal::{Paint, When};
use lakescript::{Engine, lexer, parser};

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
  --no-type-check  Run the script without checking its types first,
                   for scripts that were already checked
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
            eprintln!("{} the interpreter crashed. This is a bug in lks, not in your script", paint().error("lks:"));
            ExitCode::from(70)
        }
    }
}

#[derive(Default)]
struct Options {
    no_type_check: bool,
    test: bool,
    show_tokens: bool,
    show_ast: bool,
    show_bytecode: bool,
    module_path: Vec<std::path::PathBuf>,
    color: When,
}

/// The colors for stderr, decided once the options are read.
static PAINT: OnceLock<Paint> = OnceLock::new();

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
fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Request, String> {
    let mut options = Options::default();
    let script = loop {
        let Some(arg) = args.next() else {
            return Err("no script given".into());
        };
        match arg.as_str() {
            "--no-type-check" => options.no_type_check = true,
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
    Ok(Request::Run { options, script, args: args.collect() })
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
        Err(message) => {
            let paint = paint();
            eprintln!("{} {message}", paint.error("lks:"));
            eprintln!("{} lks [options] <script> [arguments for the script...]. See `lks --help`", paint.good("Usage:"));
            return ExitCode::from(2);
        }
    };

    let _ = PAINT.set(Paint(options.color.colors(std::io::stderr().is_terminal())));
    if options.show_tokens || options.show_ast || options.show_bytecode {
        let source = match std::fs::read_to_string(&script) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("{} can't read {script}: {err}", paint().error("error:"));
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
    for folder in options.module_path.drain(..) {
        engine.add_module_root(folder);
    }
    if let Some(extra) = std::env::var_os("LKS_PATH") {
        for folder in std::env::split_paths(&extra) {
            engine.add_module_root(folder);
        }
    }
    if options.test {
        return test(&mut engine, &script, &args, options.color);
    }
    match engine.run_file(Path::new(&script)) {
        Ok(outcome) => ExitCode::from(outcome.code().clamp(0, 255) as u8),
        // Arguments that don't match the script's command: the message and
        // the usage, without the code, and code 2 like most command-line tools
        Err(diagnostic) if diagnostic.message.starts_with("UsageError: ") => {
            let paint = paint();
            let text = &diagnostic.message["UsageError: ".len()..];
            let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
            eprintln!("{}{first}{rest}", paint.error("error: "));
            ExitCode::from(2)
        }
        Err(diagnostic) => {
            eprintln!("{}", show(&diagnostic, &script));
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
            eprintln!("{} `lks --test` takes the script and one filter at most", paint().error("lks:"));
            return ExitCode::from(2);
        }
    };
    let out = Paint(color.colors(std::io::stdout().is_terminal()));
    let start = std::time::Instant::now();
    let results = match engine.run_tests(Path::new(script), filter) {
        Ok(results) => results,
        Err(diagnostic) => {
            eprintln!("{}", show(&diagnostic, script));
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
    use std::sync::atomic::Ordering;
    let _ = ctrlc::set_handler(move || {
        if flag.swap(true, Ordering::Relaxed) {
            std::process::exit(130);
        }
    });
}

/// An error with the line of code it points to, read from the file it's in.
fn show(diagnostic: &lakescript::Diagnostic, script: &str) -> String {
    let file = diagnostic.file.as_deref().unwrap_or(script);
    match std::fs::read_to_string(file) {
        Ok(source) if diagnostic.span.line > 0 => diagnostic.render_with_source_painted(script, &source, paint()),
        _ => diagnostic.render_painted(script, paint()),
    }
}

fn debug_print(path: &str, source: &str, options: &Options) -> ExitCode {
    let (show_tokens, show_ast) = (options.show_tokens, options.show_ast);
    let result = lexer::tokenize(source).and_then(|tokens| {
        if show_tokens {
            for token in &tokens {
                println!("{}:{}\t{:?}", token.span.line, token.span.column, token.kind);
            }
        }
        parser::parse(tokens)
    });
    match result {
        Ok(program) => {
            if show_ast {
                for stmt in &program {
                    println!("{stmt}");
                }
            }
            if options.show_bytecode {
                match Engine::new().bytecode(source) {
                    Ok(listing) => print!("{listing}"),
                    Err(diagnostic) => {
                        eprintln!("{}", show(&diagnostic, path));
                        return ExitCode::FAILURE;
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(diagnostic) => {
            eprintln!("{}", show(&diagnostic, path));
            ExitCode::FAILURE
        }
    }
}
