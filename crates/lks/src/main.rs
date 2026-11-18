use std::process::ExitCode;

use lakescript::{lexer, parser};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Debugging aids while the interpreter doesn't exist yet
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

    let tokens = match lexer::tokenize(&source) {
        Ok(tokens) => tokens,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(path));
            return ExitCode::FAILURE;
        }
    };

    if show_tokens {
        for token in &tokens {
            println!("{}:{}\t{:?}", token.span.line, token.span.column, token.kind);
        }
    }

    let program = match parser::parse(tokens) {
        Ok(program) => program,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(path));
            return ExitCode::FAILURE;
        }
    };

    if show_ast {
        for stmt in &program {
            println!("{stmt}");
        }
    }
    ExitCode::SUCCESS
}
