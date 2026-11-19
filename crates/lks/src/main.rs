use std::path::Path;
use std::process::ExitCode;

use lakescript::{Engine, lexer, parser};

/// The interpreter walks the syntax tree with Rust's own stack, so scripts
/// run on a thread with a big one. Only the pages that are used take memory.
const STACK_SIZE: usize = 512 * 1024 * 1024;

fn main() -> ExitCode {
    let runner = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(run);
    match runner.map(|handle| handle.join()) {
        Ok(Ok(code)) => code,
        _ => {
            eprintln!("lks: the interpreter crashed. This is a bug in lks, not in your script");
            ExitCode::from(70)
        }
    }
}

fn run() -> ExitCode {
    // Options of `lks` go before the script; everything after it is for the script
    let mut args = std::env::args().skip(1);
    let mut show_tokens = false;
    let mut show_ast = false;
    let path = loop {
        match args.next() {
            Some(a) if a == "--tokens" => show_tokens = true,
            Some(a) if a == "--ast" => show_ast = true,
            Some(a) if a.starts_with("--") => {
                eprintln!("lks: unknown option {a}");
                return ExitCode::from(2);
            }
            Some(path) => break path,
            None => {
                eprintln!("usage: lks [--tokens] [--ast] <script.lks> [arguments...]");
                return ExitCode::from(2);
            }
        }
    };

    if show_tokens || show_ast {
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("error: can't read {path}: {err}");
                return ExitCode::FAILURE;
            }
        };
        return debug_print(&path, &source, show_tokens, show_ast);
    }

    let mut engine = Engine::new();
    engine.set_stack_budget(STACK_SIZE - 16 * 1024 * 1024);
    engine.set_args(args.collect());
    if let Some(extra) = std::env::var_os("LKS_PATH") {
        for folder in std::env::split_paths(&extra) {
            engine.add_module_root(folder);
        }
    }
    match engine.run_file(Path::new(&path)) {
        Ok(outcome) => ExitCode::from(outcome.code().clamp(0, 255) as u8),
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(&path));
            ExitCode::FAILURE
        }
    }
}

fn debug_print(path: &str, source: &str, show_tokens: bool, show_ast: bool) -> ExitCode {
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
            ExitCode::SUCCESS
        }
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(path));
            ExitCode::FAILURE
        }
    }
}
