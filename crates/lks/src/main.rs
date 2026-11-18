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
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Debugging aids
    let show_tokens = args.iter().any(|a| a == "--tokens");
    let show_ast = args.iter().any(|a| a == "--ast");
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: lks [--tokens] [--ast] <script.lks>");
        return ExitCode::from(2);
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("error: can't read {path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    if show_tokens || show_ast {
        return debug_print(path, &source, show_tokens, show_ast);
    }

    let mut engine = Engine::new();
    engine.set_stack_budget(STACK_SIZE - 16 * 1024 * 1024);
    match engine.run(&source) {
        Ok(()) => ExitCode::SUCCESS,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(path));
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
