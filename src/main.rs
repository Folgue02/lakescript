use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: lks <script.lks>");
        return ExitCode::from(2);
    };

    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("error: can't read {path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    // Phase 1: the lexer, parser and interpreter go here.
    println!("{path}: {} lines read", source.lines().count());
    ExitCode::SUCCESS
}
