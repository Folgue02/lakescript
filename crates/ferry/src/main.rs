//! `ferry`, the tool for Lakescript projects: their file, their
//! dependencies, and installing their commands *(see ideas/tooling/ferry.md)*.

// Faster for the many small values scripts make and drop *(see `lakescript::SmallAlloc`)*
#[global_allocator]
static ALLOCATOR: lakescript::SmallAlloc = lakescript::SmallAlloc;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lakescript::terminal::{Paint, When};
use lakescript::{Diagnostic, Engine};

use ferry::home::Home;
use ferry::project::{self, Kind, MANIFEST, Project};
use ferry::resolve::{self, Dependency, Lock};
use ferry::{Fail, JSON, PAINT, json, paint, status};

/// Scripts run on a thread with a big stack, like with `lks`.
const STACK_SIZE: usize = 512 * 1024 * 1024;

const USAGE: &str = "\
The tool for Lakescript projects.

Usage: ferry <command> [arguments...]

Commands:
  new <author/name> [--lib] [--example]
                              Create a project in a new folder named like the project,
                              i.e. ferry new acme/http makes http/. --lib makes a library,
                              with tests/ and a module.md, and --example adds a module
                              and a command to try it out
  run [command] [args...]     Run one of the project's commands from bin/. The name
                              can be left out when there's only one
  task <name> [args...]       Run one of the project's own scripts from scripts/, to
                              automate what's done on it, i.e. ferry task release
  check                       Type check the whole project, its tests and scripts too,
                              without running it
  test [filter] [--jobs <n>]  Run the tests in tests/: their functions named test...
                              With a filter, only the ones whose name has it. The files
                              run at once; --jobs 1 runs them one at a time
  doc [--open]                Make the docs of the project in .ferry/doc/, and
                              open them with --open
  doc <repository:author/name@version> [--open]
                              Make the docs of a package
  doc std [--open]            The pages of the standard library
  doc <name>                  Write the docs of one thing in the terminal,
                              i.e. ferry doc units::bytes
  add <repository:author/name@version>
                              Add a dependency, i.e. ferry add github:acme/http@1.4.2.
                              The version can be a range: ^1.4.2 or ~1.4.2
  remove <package>            Remove a dependency, i.e. ferry remove github:acme/http
  update [package]            Move the dependencies, or one, to the newest version
                              their range takes, in ferry.lock
  bundle [command]            Make a .lkb in dist/ for each command of bin/, or only
                              for one: the program in one file, with its modules and
                              resources/, run with `lks todo.lkb` or `./todo.lkb`
  install [repository:author/name@version]
                              Install the commands of an application into ~/.ferry/bin:
                              the project in this folder, or one from a repository

Options:
  --color <when>              When to use colors: auto (on a terminal, the default),
                              always or never. NO_COLOR also turns them off.
                              It goes before the command
  --message-format <format>   How ferry writes what it says on stderr: text (the
                              default) or json, one object per line, for tools and
                              agents. It goes before the command
  -h, --help                  Show this help
  -V, --version               Show the version of ferry

FERRY_HOME replaces ~/.ferry, where ferry keeps what it downloads and installs.
";

/// A hint, in cyan.
fn note(text: &str) {
    if json() {
        eprintln!("{}", lakescript::json_message("note", text, None));
    } else {
        eprintln!("{} {text}", paint().note("note:"));
    }
}

/// An error that isn't at a place in the code.
fn error(text: &str) {
    if json() {
        eprintln!("{}", lakescript::json_message("error", text, None));
    } else {
        eprintln!("{} {text}", paint().error("error:"));
    }
}

/// A warning, at a place in a file of the project when it's about one
/// *(the file as seen from the project's folder, its line and column)*.
fn warning(text: &str, place: Option<(&str, u32, u32)>) {
    match place {
        _ if json() => {
            let Some((file, line, column)) = place else {
                eprintln!("{}", lakescript::json_message("warning", text, None));
                return;
            };
            let mut diagnostic = Diagnostic::new(text, lakescript::Span { start: 0, end: 0, line, column: column.max(1), file: 0 });
            diagnostic.file = Some(file.to_string());
            eprintln!("{}", diagnostic.render_json("warning", file, None));
        }
        Some((file, line, column)) => {
            let at = if column > 0 { format!("{file}:{line}:{column}") } else { format!("{file}:{line}") };
            eprintln!("{} {text}\n {} {at}\n", paint().warning("warning:"), paint().note("-->"));
        }
        None => eprintln!("{} {text}\n", paint().warning("warning:")),
    }
}

/// An error in the code, from `show_in`, with a blank line after it as text.
fn report(shown: &str) {
    if json() {
        eprintln!("{shown}");
    } else {
        eprintln!("{shown}\n");
    }
}

fn main() -> ExitCode {
    let runner = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(start);
    match runner.map(|handle| handle.join()) {
        Ok(Ok(code)) => code,
        _ => {
            error("ferry crashed. This is a bug in ferry");
            ExitCode::from(70)
        }
    }
}

fn start() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut color = When::Auto;
    while let Some(option) = args.first().filter(|a| *a == "--color" || *a == "--message-format").cloned() {
        if option == "--color" {
            match args.get(1).and_then(|w| When::parse(w)) {
                Some(when) => color = when,
                None => return usage_error("`--color` needs `auto`, `always` or `never` after it"),
            }
        } else {
            match args.get(1).map(String::as_str) {
                Some("text") => {}
                Some("json") => {
                    let _ = JSON.set(true);
                }
                _ => return usage_error("`--message-format` needs `text` or `json` after it"),
            }
        }
        args.drain(..2);
    }
    let _ = PAINT.set(Paint(color.colors(std::io::stderr().is_terminal())));
    let Some(command) = args.first() else {
        return usage_error("no command given");
    };
    let rest = &args[1..];
    let result = match command.as_str() {
        "-h" | "--help" | "help" => {
            print!("{}", Paint(color.colors(std::io::stdout().is_terminal())).help(USAGE));
            return ExitCode::SUCCESS;
        }
        "-V" | "--version" => {
            println!("ferry {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        "new" => {
            let (flags, ids): (Vec<&String>, Vec<&String>) = rest.iter().partition(|a| a.starts_with('-'));
            let library = flags.iter().any(|f| *f == "--lib");
            let example = flags.iter().any(|f| *f == "--example");
            match (ids.as_slice(), flags.iter().find(|f| **f != "--lib" && **f != "--example")) {
                ([id], None) => new(id, library, example),
                (_, Some(flag)) => return usage_error(&format!("`ferry new` has no option `{flag}`: it takes `--lib` and `--example`")),
                _ => return usage_error("`ferry new` takes the project, i.e. `ferry new folgue02/backup`, and maybe `--lib` or `--example`"),
            }
        }
        "run" => run(rest),
        "task" => task(rest),
        "check" => match rest {
            [] => check(),
            _ => return usage_error("`ferry check` takes no arguments"),
        },
        "doc" => doc(rest),
        "test" => {
            // `--jobs <n>` anywhere, and one filter at most
            let mut jobs = None;
            let mut filters = Vec::new();
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                if arg == "--jobs" {
                    match args.next().and_then(|n| n.parse::<usize>().ok()).filter(|n| *n > 0) {
                        Some(n) => jobs = Some(n),
                        None => return usage_error("`--jobs` takes how many files of tests run at once, i.e. `ferry test --jobs 1`"),
                    }
                } else {
                    filters.push(arg);
                }
            }
            match filters.as_slice() {
                [] => test(None, jobs),
                [filter] => test(Some(filter), jobs),
                _ => return usage_error("`ferry test` takes one filter at most, i.e. `ferry test bytes`"),
            }
        }
        "add" => match rest {
            [spec] => add(spec),
            _ => return usage_error("`ferry add` takes one package, i.e. `ferry add github:acme/http@1.4.2`"),
        },
        "bundle" => match rest {
            [] => bundle(None),
            [command] => bundle(Some(command)),
            _ => return usage_error("`ferry bundle` takes one command at most, i.e. `ferry bundle todo`"),
        },
        "install" => match rest {
            [] => install_here(),
            [spec] => install_package(spec),
            _ => return usage_error("`ferry install` takes one package at most"),
        },
        "remove" => match rest {
            [package] => remove(package),
            _ => return usage_error("`ferry remove` takes one dependency, i.e. `ferry remove github:acme/http`"),
        },
        "update" => match rest {
            [] => update(None),
            [package] => update(Some(package)),
            _ => return usage_error("`ferry update` takes one dependency at most, i.e. `ferry update github:acme/http`"),
        },
        "uninstall" => match rest {
            [] => uninstall(None),
            [project] => uninstall(Some(project)),
            _ => return usage_error("`ferry uninstall` takes one project at most, i.e. `ferry uninstall folgue02/backup`"),
        },
        other => return usage_error(&format!("there's no command `{other}`")),
    };
    match result {
        Ok(code) => code,
        Err(Fail(message)) => {
            error(&message);
            ExitCode::FAILURE
        }
    }
}

fn usage_error(message: &str) -> ExitCode {
    if json() {
        error(&format!("{message}. Usage: ferry <command> [arguments...]"));
        return ExitCode::from(2);
    }
    let paint = paint();
    eprintln!("{} {message}", paint.error("error:"));
    eprintln!("{} ferry <command> [arguments...]. See `ferry --help`", paint.good("Usage:"));
    ExitCode::from(2)
}

fn current_dir() -> Result<PathBuf, Fail> {
    std::env::current_dir().map_err(|e| format!("can't read the current folder: {e}").into())
}

/// A new project in a new folder. Only what every project needs, unless
/// `example`: then a module and a command that uses it, to try ferry out.
fn new(spec: &str, library: bool, example: bool) -> Result<ExitCode, Fail> {
    let Some((author, name)) = spec.split_once('/') else {
        return Err(format!("`{spec}` isn't a project. It's written `author/name`, i.e. `folgue02/backup`").into());
    };
    let id = project::id_of(author, name)?;
    let dir = current_dir()?.join(name);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()).into());
    }
    let kind = if library { "library" } else { "application" };
    let src = dir.join("src").join(&id);
    let write = |path: PathBuf, text: String| -> Result<(), Fail> {
        std::fs::create_dir_all(path.parent().expect("files are in folders")).map_err(|e| format!("can't create {}: {e}", path.display()))?;
        std::fs::write(&path, text).map_err(|e| format!("can't write {}: {e}", path.display()).into())
    };
    write(
        dir.join(MANIFEST),
        format!("[project]\nauthor = \"{author}\"\nname = \"{name}\"\nversion = \"0.1.0\"\nkind = \"{kind}\"\n\n[dependencies]\n"),
    )?;
    // What ferry makes inside the project, like the docs
    write(dir.join(".gitignore"), ".ferry/\ndist/\n".to_string())?;
    std::fs::create_dir_all(&src).map_err(|e| format!("can't create {}: {e}", src.display()))?;
    if example {
        write(src.join("greeting.lks"), "pub string greet(string $who) = f\"Hello, {$who}!\";\n".to_string())?;
    }
    if library {
        // The front page of its docs, and a place for its tests, which a library needs most
        write(src.join("module.md"), format!("# {id}\n\nWhat {name} is for, the first page of its docs (`ferry doc`).\n"))?;
        std::fs::create_dir_all(dir.join("tests")).map_err(|e| format!("can't create {}: {e}", dir.join("tests").display()))?;
    }
    if !library {
        let command = if example {
            format!("use std/process;\nuse {id}/greeting;\n\noutln(greeting::greet(process::args()[0] ?? 'world'));\n")
        } else {
            String::new()
        };
        write(dir.join("bin").join(format!("{name}.lks")), command)?;
    }
    status("Created", &format!("the {kind} {id} in {}", dir.display()));
    Ok(ExitCode::SUCCESS)
}

/// The project in the current folder, with every dependency downloaded and its lock file up to date.
fn open_project() -> Result<(Home, Project, Vec<Dependency>), Fail> {
    let home = Home::open()?;
    let project = Project::find(&current_dir()?)?;
    too_old(&project);
    project.modules()?;
    let (dependencies, lock) = resolve::resolve(&home, &project, &Lock::read(&project.dir)?)?;
    too_old_dependencies(&dependencies);
    lock.write(&project.dir)?;
    Ok((home, project, dependencies))
}

fn run(args: &[String]) -> Result<ExitCode, Fail> {
    let (_, project, dependencies) = open_project()?;
    let commands = project.commands();
    if project.manifest.project.kind == Kind::Library || commands.is_empty() {
        return Err(format!("{} has no commands to run: an application has them in bin/", project.id).into());
    }
    let (command, args) = match args.split_first() {
        Some((first, rest)) if commands.contains(first) => (first.clone(), rest),
        _ if commands.len() == 1 => (commands[0].clone(), args),
        Some((first, _)) if !first.starts_with('-') => {
            return Err(format!("{} has no command `{first}`. The ones there are: {}", project.id, commands.join(", ")).into());
        }
        _ => return Err(format!("{} has several commands, so say which one: {}", project.id, commands.join(", ")).into()),
    };
    let script = project.command_file(&command);
    let mut engine = engine(&resolve::module_path(&project, &dependencies));
    engine.set_args(args.to_vec());
    // `Usage: cinder`, what was typed, not `cinder.lks`
    engine.set_command_name(&command);
    engine.set_resources(&project.dir.join("resources"));
    match engine.run_file(&script) {
        Ok(outcome) => Ok(ExitCode::from(outcome.code().clamp(0, 255) as u8)),
        // The person's mistake in the arguments, like `lks` shows it
        Err(ref diagnostic) if let Some(text) = diagnostic.usage() => {
            if json() {
                eprintln!("{}", show_in(&project, diagnostic, &script));
            } else {
                error(text);
            }
            Ok(ExitCode::from(2))
        }
        Err(diagnostic) => {
            eprintln!("{}", show_all_in(&project, &engine, diagnostic, &script));
            Ok(ExitCode::FAILURE)
        }
    }
}

/// `ferry task <name> [arguments...]`: one of the project's own scripts, in
/// `scripts/`, run like a command *(see Scripts of a project in ideas/tooling/ferry.md)*.
fn task(args: &[String]) -> Result<ExitCode, Fail> {
    let (_, project, dependencies) = open_project()?;
    let tasks = project.tasks();
    let Some((name, args)) = args.split_first() else {
        let there = if tasks.is_empty() { "it has none: they're the scripts in scripts/".to_string() } else { format!("they're {}", tasks.join(", ")) };
        return Err(format!("`ferry task` takes the name of one of the tasks of {}: {there}", project.id).into());
    };
    if !tasks.contains(name) {
        let there = if tasks.is_empty() { "it has none: they're the scripts in scripts/".to_string() } else { format!("the ones there are: {}", tasks.join(", ")) };
        return Err(format!("{} has no task `{name}`, {there}", project.id).into());
    }
    let script = project.task_file(name);
    let mut engine = engine(&resolve::module_path(&project, &dependencies));
    engine.set_args(args.to_vec());
    engine.set_command_name(&format!("ferry task {name}"));
    engine.set_resources(&project.dir.join("resources"));
    match engine.run_file(&script) {
        Ok(outcome) => Ok(ExitCode::from(outcome.code().clamp(0, 255) as u8)),
        Err(ref diagnostic) if let Some(text) = diagnostic.usage() => {
            if json() {
                eprintln!("{}", show_in(&project, diagnostic, &script));
            } else {
                error(text);
            }
            Ok(ExitCode::from(2))
        }
        Err(diagnostic) => {
            eprintln!("{}", show_all_in(&project, &engine, diagnostic, &script));
            Ok(ExitCode::FAILURE)
        }
    }
}

/// The flag every engine shares, set on Ctrl+C *(see `on_ctrl_c` in lks)*.
fn interrupt() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, OnceLock};
    static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    FLAG.get_or_init(|| {
        let flag = Arc::new(AtomicBool::new(false));
        // A second Ctrl+C before a script got to the first stops ferry
        lakescript::on_ctrl_c(flag.clone());
        flag
    })
    .clone()
}

fn engine(module_path: &[PathBuf]) -> Engine {
    let mut engine = Engine::new();
    engine.set_interrupt_flag(interrupt());
    engine.set_stack_budget(STACK_SIZE - 16 * 1024 * 1024);
    for folder in module_path {
        engine.add_module_root(folder);
    }
    engine
}

fn check() -> Result<ExitCode, Fail> {
    let (_, project, dependencies) = open_project()?;
    let errors = check_project(&project, &dependencies)?;
    undocumented(&project)?;
    if errors > 0 {
        return Err(format!("{} has {errors} {}", project.id, if errors == 1 { "error" } else { "errors" }).into());
    }
    status("Checked", &format!("{}: no errors", project.id));
    Ok(ExitCode::SUCCESS)
}

/// Type checks every command and every module of a project, and prints the
/// errors. It returns how many different errors there were.
fn check_project(project: &Project, dependencies: &[Dependency]) -> Result<usize, Fail> {
    check_files(project, dependencies, true)
}

/// Checks the modules of the project, then its commands and, with `tests`,
/// the files of `tests/`. Each file is checked on its own, so each one shows
/// its first error. An error in a module is shown once, not once for every
/// file that uses it. Gives how many were shown.
fn check_files(project: &Project, dependencies: &[Dependency], tests: bool) -> Result<usize, Fail> {
    let module_path = resolve::module_path(project, dependencies);
    let mut shown = std::collections::HashSet::new();
    let mut report = |diagnostic: &Diagnostic, script: &Path| {
        let text = show_in(project, diagnostic, script);
        if shown.insert(text.clone()) {
            report(&text);
        }
    };
    let modules = project.modules()?;
    for module in &modules {
        let file = project.src().join(format!("{module}.lks"));
        let mut checker = engine(&module_path);
        if let Err(diagnostic) = checker.check_source(&format!("use {module} as module;\n")) {
            for error in all_errors(&checker, diagnostic) {
                report(&error, &file);
            }
        }
    }
    let test_files = if tests { project.test_files() } else { Vec::new() };
    let tasks = if tests { project.tasks() } else { Vec::new() };
    let scripts: Vec<PathBuf> = project
        .commands()
        .into_iter()
        .map(|c| project.command_file(&c))
        .chain(tasks.iter().map(|t| project.task_file(t)))
        .chain(test_files)
        .collect();
    let mut waiting = Vec::new();
    for script in &scripts {
        let mut checker = engine(&module_path);
        if let Err(diagnostic) = checker.check_file(script) {
            // A module it uses that has an error stops its check: its own code is checked once that's fixed
            if let Some(file) = &diagnostic.file {
                waiting.push((relative(project, script), relative(project, Path::new(file))));
            }
            for error in all_errors(&checker, diagnostic) {
                report(&error, script);
            }
        }
    }
    for (script, module) in waiting {
        note(&format!("{script} is checked once the error in {module}, which it uses, is fixed"));
    }
    for file in scripts.iter().cloned().chain(modules.iter().map(|m| project.src().join(format!("{m}.lks")))) {
        stale_comments(project, &file);
    }
    Ok(shown.len())
}

/// Every type error of a check or run that failed *(see Every error at once
/// in ideas/tooling/lsp.md)*, or the one error it gave otherwise.
fn all_errors(engine: &Engine, error: Diagnostic) -> Vec<Diagnostic> {
    if engine.errors().is_empty() { vec![error] } else { engine.errors().to_vec() }
}

/// Every error of a run that failed, each one as `show_in` writes it, with a blank line between them as text.
fn show_all_in(project: &Project, engine: &Engine, error: Diagnostic, script: &Path) -> String {
    let shown: Vec<String> = all_errors(engine, error).iter().map(|d| show_in(project, d, script)).collect();
    shown.join(if json() { "\n" } else { "\n\n" })
}

/// Warns when this ferry is older than the Lakescript the project needs,
/// its `lks` in `[project]`. It's a warning: an older one may still work.
fn too_old(project: &Project) {
    let Some(needed) = &project.manifest.project.lks else { return };
    let this = project::THIS;
    if project::newer(needed) {
        warning(&format!("{} needs Lakescript {needed} or newer, and this is ferry {this}: it may use what this one doesn't have", project.id), None);
    }
}

/// Warns about each dependency that needs a newer Lakescript, like
/// `too_old` for the project: one in the lock file can, after a `git pull`.
fn too_old_dependencies(dependencies: &[Dependency]) {
    let this = project::THIS;
    for dependency in dependencies {
        let Some(needed) = &dependency.project.manifest.project.lks else { continue };
        if project::newer(needed) {
            warning(
                &format!(
                    "{} {}, for {}, needs Lakescript {needed} or newer, and this is ferry {this}: it may use what this one doesn't have",
                    dependency.package, dependency.version, dependency.required_by
                ),
                None,
            );
        }
    }
}

/// Warns about each `pub` item of the project's modules without a `///`
/// comment, when `ferry.toml` asks for it with `[check]` and `docs = true`.
fn undocumented(project: &Project) -> Result<(), Fail> {
    if !project.manifest.check.docs {
        return Ok(());
    }
    for module in project.modules()? {
        let file = project.src().join(format!("{module}.lks"));
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(doc) = lakescript::docs::module_doc(&text) else { continue };
        let short = module.rsplit('/').next().unwrap_or(&module).to_string();
        let mut missing = Vec::new();
        for item in &doc.items {
            if item.doc.text.trim().is_empty() {
                missing.push((format!("{short}::{}", item.name), item.line));
            }
            // The values of an enum are often clear from their names
            for member in item.members.iter().filter(|m| m.kind != lakescript::docs::ItemKind::Value) {
                if member.doc.text.trim().is_empty() {
                    missing.push((format!("{short}::{}.{}", item.name, member.name), member.line));
                }
            }
        }
        for (name, line) in missing {
            warning(&format!("`{name}` is `pub`, and has no `///` comment"), Some((&relative(project, &file), line as u32, 0)));
        }
    }
    Ok(())
}

/// Warns about the comments of a file that don't match its code *(i.e. a
/// `$name:` for a parameter that was renamed)*. They never stop anything.
fn stale_comments(project: &Project, file: &Path) {
    let Ok(text) = std::fs::read_to_string(file) else { return };
    let Ok(warnings) = lakescript::docs::stale_params(&text) else { return };
    for w in warnings {
        warning(&w.message, Some((&relative(project, file), w.line as u32, w.column as u32)));
    }
}

/// `ferry doc`: the docs of the project, of a package, of the standard
/// library, or of one thing in the terminal *(see ideas/tooling/docs.md)*.
fn doc(args: &[String]) -> Result<ExitCode, Fail> {
    let open = args.iter().any(|a| a == "--open");
    let targets: Vec<&str> = args.iter().map(String::as_str).filter(|a| *a != "--open").collect();
    match targets.as_slice() {
        [] => doc_project(open),
        ["std"] => doc_std(open),
        [spec] if spec.contains(':') && !spec.contains("::") => doc_package(spec, open),
        [name] if !name.starts_with('-') => doc_name(name),
        _ => Ok(usage_error("`ferry doc` takes a package, `std` or a name, and maybe `--open`")),
    }
}

fn doc_project(open: bool) -> Result<ExitCode, Fail> {
    let (_, project, dependencies) = open_project()?;
    // Only what the docs are made from: the tests aren't in them
    let errors = check_files(&project, &dependencies, false)?;
    if errors > 0 {
        let what = if errors == 1 { "error" } else { "errors" };
        return Err(format!("{} has {errors} {what}, so its docs weren't made", project.id).into());
    }
    let mut site = ferry::doc::read(&project)?;
    // The pages of the standard library and of the dependencies go in the
    // same folder, so every link is relative and the folder works anywhere
    site.std = Some((Box::new(ferry::doc::std_site()), "std/".to_string()));
    for dependency in &dependencies {
        let package = &dependency.package;
        let base = format!("deps/{}/{}/{}/{}/", package.repository, package.author, package.name, dependency.version);
        site.dependencies.push((package.to_string(), dependency.version.clone(), Some(format!("{base}index.html"))));
        // Its items, which the project's pages link to and search
        site.packages.push((ferry::doc::read(&dependency.project)?, base));
    }
    let out = project.dir.join(".ferry").join("doc");
    let index = ferry::doc::write(&site, &out)?;
    if let Some((std, base)) = &site.std {
        ferry::doc::write(std, &out.join(base))?;
    }
    for ((package, version, _), (pages, base)) in site.dependencies.iter().zip(&site.packages) {
        ferry::doc::write(pages, &out.join(base))?;
        status("Documented", &format!("{package} {version}"));
    }
    status("Documented", &format!("{} in {}", project.id, relative_to_here(&index)));
    if open {
        open_page(&index);
    }
    Ok(ExitCode::SUCCESS)
}

/// The docs of a package in `~/.ferry/doc/`, made once per version.
fn package_docs(home: &Home, project: &Project, package: &project::Package, version: &str) -> Result<PathBuf, Fail> {
    let out = home.dir.join("doc").join(&package.repository).join(&package.author).join(&package.name).join(version);
    let index = out.join("index.html");
    if !index.is_file() {
        let site = ferry::doc::read(project)?;
        ferry::doc::write(&site, &out)?;
        status("Documented", &format!("{package} {version}"));
    }
    Ok(index)
}

fn doc_package(spec: &str, open: bool) -> Result<ExitCode, Fail> {
    let home = Home::open()?;
    // Without a version, the one of the project's lock file *(i.e. `ferry doc github:acme/http`)*
    let (package, version) = match project::parse_exact_spec(spec) {
        Ok(found) => found,
        Err(e) => match locked(&project::Package::parse(spec)?.id())? {
            Some(dependency) => (dependency.package, dependency.version),
            None => return Err(e.into()),
        },
    };
    let (dir, _) = home.fetch(&package, &version)?;
    let project = Project::read(&dir)?;
    if project.id != package.id() {
        return Err(package.wrong_id(&version, &project.id).into());
    }
    let index = package_docs(&home, &project, &package, &version)?;
    if open {
        open_page(&index);
    } else {
        println!("{}", index.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// The dependency of the project in the current folder with the ID `id`
/// *(i.e. `acme/http`)*, at the version of its lock file. `None` outside a
/// project, or when it isn't one of its dependencies.
fn locked(id: &str) -> Result<Option<Dependency>, Fail> {
    if Project::find(&current_dir()?).is_err() {
        return Ok(None);
    }
    let (_, _, dependencies) = open_project()?;
    Ok(dependencies.into_iter().find(|d| d.package.id() == id))
}

/// The pages of the standard library, which ferry has inside, written to
/// `~/.ferry/doc/std/<version>/`. Gives their front page.
fn std_docs(home: &Home) -> Result<PathBuf, Fail> {
    let dir = home.dir.join("doc").join("std").join(env!("CARGO_PKG_VERSION"));
    ferry::doc::write(&ferry::doc::std_site(), &dir)
}

fn doc_std(open: bool) -> Result<ExitCode, Fail> {
    let index = std_docs(&Home::open()?)?;
    if open {
        open_page(&index);
    } else {
        println!("{}", index.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn doc_name(name: &str) -> Result<ExitCode, Fail> {
    // Outside a project, only the standard library
    if Project::find(&current_dir()?).is_err() {
        return match ferry::doc::describe(&ferry::doc::std_site(), name) {
            Some(text) => {
                print!("{text}");
                Ok(ExitCode::SUCCESS)
            }
            None => Err(format!("there's nothing named `{name}` in the standard library").into()),
        };
    }
    let (_, project, dependencies) = open_project()?;
    // A dependency by its ID *(i.e. `ferry doc acme/http`)*: its pages
    if let Some(dependency) = dependencies.iter().find(|d| d.package.id() == name) {
        return doc_package(&format!("{}@{}", dependency.package, dependency.version), false);
    }
    // The project's own first, then its dependencies', then the standard library's
    let mut found = ferry::doc::describe(&ferry::doc::read(&project)?, name);
    for dependency in &dependencies {
        if found.is_none() {
            found = ferry::doc::describe(&ferry::doc::read(&dependency.project)?, name);
        }
    }
    match found.or_else(|| ferry::doc::describe(&ferry::doc::std_site(), name)) {
        Some(text) => {
            print!("{text}");
            Ok(ExitCode::SUCCESS)
        }
        None => Err(format!("there's nothing named `{name}` in the docs of {}, its dependencies or the standard library", project.id).into()),
    }
}

fn relative_to_here(path: &Path) -> String {
    let here = std::env::current_dir().ok().map(|d| lakescript::canonical(&d));
    let real = lakescript::canonical(path);
    match here.as_deref().and_then(|h| real.strip_prefix(h).ok()) {
        Some(short) => short.display().to_string(),
        None => real.display().to_string(),
    }
}

/// Opens a page in the browser. If it can't, says where the page is.
fn open_page(page: &Path) {
    let page = page.display().to_string();
    let opened = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&page).status()
    } else if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", "start", "", &page]).status()
    } else {
        std::process::Command::new("xdg-open").arg(&page).status()
    };
    if !opened.is_ok_and(|s| s.success()) {
        note(&format!("couldn't open a browser: the page is {page}"));
    }
}

/// `ferry test`: every file in `tests/`, and its tests whose name has
/// `filter`. The files run at once, `jobs` of them, each in its own engine
/// and thread, and what each one gives is printed whole, in their order.
fn test(filter: Option<&str>, jobs: Option<usize>) -> Result<ExitCode, Fail> {
    let (_, project, dependencies) = open_project()?;
    let files = project.test_files();
    status("Testing", &project.id);
    if files.is_empty() {
        note("there are no tests: they're the functions named `test...` of the files in tests/");
        return Ok(ExitCode::SUCCESS);
    }
    let module_path = resolve::module_path(&project, &dependencies);
    let jobs = jobs.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get())).clamp(1, files.len());
    let start = std::time::Instant::now();
    let mut results = Vec::new();
    let mut broken = 0;
    // What a file gave: its tests, or the error that stopped it, already shown
    enum Outcome {
        Ran(Vec<lakescript::TestResult>),
        Broken(String),
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (sender, received) = std::sync::mpsc::channel::<(usize, Outcome)>();
    std::thread::scope(|scope| -> Result<(), Fail> {
        for _ in 0..jobs {
            let sender = sender.clone();
            let (next, files, module_path, project) = (&next, &files, &module_path, &project);
            std::thread::Builder::new()
                .stack_size(STACK_SIZE)
                .spawn_scoped(scope, move || {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(file) = files.get(i) else { break };
                        let mut tests = engine(module_path);
                        tests.set_resources(&project.dir.join("resources"));
                        let outcome = match tests.run_tests(file, filter) {
                            Ok(found) => Outcome::Ran(found),
                            Err(diagnostic) => Outcome::Broken(show_all_in(project, &tests, diagnostic, file)),
                        };
                        if sender.send((i, outcome)).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|e| format!("can't start a thread for the tests: {e}"))?;
        }
        drop(sender);
        // Printed in the order of the files, each one as soon as those before it are
        let mut waiting = std::collections::BTreeMap::new();
        let mut shown_up_to = 0;
        for (i, outcome) in received {
            waiting.insert(i, outcome);
            while let Some(outcome) = waiting.remove(&shown_up_to) {
                let shown = relative(&project, &files[shown_up_to]);
                match outcome {
                    Outcome::Ran(found) if found.is_empty() => {}
                    // With JSON on stderr, the results are text on stdout, like `lks --test`'s
                    Outcome::Ran(found) if json() => {
                        println!("{shown}");
                        for result in &found {
                            print!("{}", lakescript::render_result(result, &shown, Paint(false)));
                        }
                        results.extend(found);
                    }
                    Outcome::Ran(found) => {
                        eprintln!("{shown}");
                        for result in &found {
                            eprint!("{}", lakescript::render_result(result, &shown, paint()));
                        }
                        results.extend(found);
                    }
                    Outcome::Broken(text) => {
                        broken += 1;
                        report(&text);
                    }
                }
                shown_up_to += 1;
            }
        }
        Ok(())
    })?;
    if json() {
        print!("{}", lakescript::render_summary(&results, start.elapsed().as_secs_f64(), Paint(false)));
    } else {
        eprint!("{}", lakescript::render_summary(&results, start.elapsed().as_secs_f64(), paint()));
    }
    if let Some(filter) = filter.filter(|_| results.is_empty() && broken == 0) {
        note(&format!("no test has `{filter}` in its name: the filter picks tests by their names, i.e. `testBytes`, not by their files"));
    }
    if broken > 0 {
        let files = if broken == 1 { "file of tests" } else { "files of tests" };
        return Err(format!("{broken} {files} couldn't run").into());
    }
    Ok(if results.iter().all(|r| r.passed()) { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// A file as seen from the project's folder *(i.e. `tests/units.lks`)*.
fn relative(project: &Project, file: &Path) -> String {
    if let Ok(short) = file.strip_prefix(&project.dir) {
        return short.display().to_string();
    }
    // The paths of the engine are real ones *(i.e. `/private/tmp` for `/tmp` on macOS)*
    let real = lakescript::canonical(file);
    match real.strip_prefix(lakescript::canonical(&project.dir)) {
        Ok(short) => short.display().to_string(),
        Err(_) => file.display().to_string(),
    }
}

/// An error in a project, with every path in it as seen from the project's
/// folder *(i.e. `src/me/app/math.lks`, `tests/math.lks`)*, the same for
/// modules, scripts and the calls that led to it.
fn show_in(project: &Project, diagnostic: &Diagnostic, script: &Path) -> String {
    let source = diagnostic.file.clone().map(PathBuf::from).unwrap_or_else(|| script.to_path_buf());
    let mut short = diagnostic.clone();
    short.file = diagnostic.file.as_ref().map(|f| relative(project, Path::new(f)));
    for call in &mut short.trace {
        call.file = call.file.as_ref().map(|f| relative(project, Path::new(f)));
    }
    let name = relative(project, script);
    if json() {
        let text = std::fs::read_to_string(&source).ok();
        return short.render_json("error", &name, text.as_deref());
    }
    match std::fs::read_to_string(&source) {
        Ok(text) if short.span.line > 0 => short.render_with_source_painted(&name, &text, paint()),
        _ => short.render_painted(&name, paint()),
    }
}

fn add(spec: &str) -> Result<ExitCode, Fail> {
    let (package, version) = project::parse_spec(spec)?;
    let home = Home::open()?;
    home.knows(&package)?;
    let mut project = Project::find(&current_dir()?)?;
    let before = project.dependencies.iter().find(|(p, _)| *p == package).map(|(_, v)| v.clone());
    if before.as_deref() == Some(version.as_str()) {
        note(&format!("{package} {version} is already a dependency"));
        return Ok(ExitCode::SUCCESS);
    }
    // Resolved with the new dependency first, so a clash leaves ferry.toml as it was
    project.dependencies.retain(|(p, _)| *p != package);
    project.dependencies.push((package.clone(), version.clone()));
    too_old(&project);
    let (dependencies, lock) = resolve::resolve(&home, &project, &Lock::read(&project.dir)?)?;
    too_old_dependencies(&dependencies);
    let file = project.dir.join(MANIFEST);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
    let mut text = with_dependency(&text, &package.to_string(), &version);
    // A range can't be read before 0.13.0, so the project says it needs that one,
    // and an older ferry says so instead of that the version is wrong
    let raise = version.starts_with(['^', '~']) && project.manifest.project.lks.as_deref().is_none_or(|lks| project::numbers(lks) < project::numbers(project::RANGES));
    if raise {
        text = with_lks(&text, project::RANGES);
    }
    std::fs::write(&file, &text).map_err(|e| format!("can't write {}: {e}", file.display()))?;
    lock.write(&project.dir)?;
    match before {
        Some(old) => status("Changed", &format!("{package} from {old} to {version}")),
        None => status("Added", &format!("{package} {version}")),
    }
    if raise {
        status("Set", &format!("lks = \"{}\" in {MANIFEST}: version ranges need Lakescript {} or newer", project::RANGES, project::RANGES));
    }
    Ok(ExitCode::SUCCESS)
}

/// `ferry.toml` with a dependency added or its version changed, keeping the rest of the file as it is.
fn with_dependency(text: &str, key: &str, version: &str) -> String {
    let line = format!("\"{key}\" = \"{version}\"");
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let Some(header) = lines.iter().position(|l| l.trim() == "[dependencies]") else {
        let mut text = text.trim_end().to_string();
        text.push_str(&format!("\n\n[dependencies]\n{line}\n"));
        return text;
    };
    let end = lines[header + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| header + 1 + i);
    let existing = (header + 1..end).find(|&i| {
        lines[i].split_once('=').is_some_and(|(k, _)| k.trim().trim_matches('"').trim_matches('\'') == key)
    });
    match existing {
        Some(i) => lines[i] = line,
        None => {
            let last = (header..end).rev().find(|&i| !lines[i].trim().is_empty()).unwrap_or(header);
            lines.insert(last + 1, line);
        }
    }
    lines.join("\n") + "\n"
}

/// `ferry.toml` with `lks` in `[project]` set to `version`, keeping the rest
/// of the file as it is.
fn with_lks(text: &str, version: &str) -> String {
    let line = format!("lks = \"{version}\"");
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let Some(header) = lines.iter().position(|l| l.trim() == "[project]") else { return text.to_string() };
    let end = lines[header + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| header + 1 + i);
    let existing = (header + 1..end).find(|&i| lines[i].split_once('=').is_some_and(|(k, _)| k.trim().trim_matches('"').trim_matches('\'') == "lks"));
    match existing {
        Some(i) => lines[i] = line,
        None => {
            let last = (header..end).rev().find(|&i| !lines[i].trim().is_empty()).unwrap_or(header);
            lines.insert(last + 1, line);
        }
    }
    lines.join("\n") + "\n"
}

/// `ferry.toml` without the dependency `key`, keeping everything else as it's written.
fn without_dependency(text: &str, key: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let Some(header) = lines.iter().position(|l| l.trim() == "[dependencies]") else { return text.to_string() };
    let end = lines[header + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| header + 1 + i);
    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, l)| !(*i > header && *i < end && l.split_once('=').is_some_and(|(k, _)| k.trim().trim_matches('"').trim_matches('\'') == key)))
        .map(|(_, l)| *l)
        .collect();
    kept.join("\n") + "\n"
}

/// The dependency of `project` named `name`: by its package *(i.e.
/// `github:acme/http`)* or its ID *(i.e. `acme/http`)*.
fn dependency_named(project: &Project, name: &str) -> Result<project::Package, Fail> {
    let found = project.dependencies.iter().find(|(p, _)| p.to_string() == name || p.id() == name);
    match found {
        Some((package, _)) => Ok(package.clone()),
        None => {
            let there: Vec<String> = project.dependencies.iter().map(|(p, _)| p.to_string()).collect();
            let there = if there.is_empty() { "it has none".to_string() } else { format!("they're {}", there.join(", ")) };
            Err(format!("{name} isn't a dependency of {}: {there}", project.id).into())
        }
    }
}

/// `ferry remove`: a dependency out of `ferry.toml` and the lock file.
fn remove(name: &str) -> Result<ExitCode, Fail> {
    let home = Home::open()?;
    let mut project = Project::find(&current_dir()?)?;
    let package = dependency_named(&project, name)?;
    project.dependencies.retain(|(p, _)| *p != package);
    let old = Lock::read(&project.dir)?;
    too_old(&project);
    let (dependencies, lock) = resolve::resolve(&home, &project, &old)?;
    too_old_dependencies(&dependencies);
    let file = project.dir.join(MANIFEST);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
    std::fs::write(&file, without_dependency(&text, &package.to_string())).map_err(|e| format!("can't write {}: {e}", file.display()))?;
    lock.write(&project.dir)?;
    // It, and what only it needed
    for (verb, what) in lock.changes(&old) {
        status(&verb, &what);
    }
    Ok(ExitCode::SUCCESS)
}

/// `ferry update`: each dependency, or one, to the newest version its range
/// takes, in the lock file. `ferry.toml` doesn't change.
fn update(name: Option<&String>) -> Result<ExitCode, Fail> {
    let home = Home::open()?;
    let project = Project::find(&current_dir()?)?;
    let which = match name {
        Some(name) => resolve::Update::Only(dependency_named(&project, name)?),
        None => resolve::Update::All,
    };
    let old = Lock::read(&project.dir)?;
    too_old(&project);
    let (dependencies, lock) = resolve::resolve_updating(&home, &project, &old, &which)?;
    too_old_dependencies(&dependencies);
    lock.write(&project.dir)?;
    let changes = lock.changes(&old);
    if changes.is_empty() {
        note("everything is at the newest version its range in ferry.toml takes: an exact version like `1.4.2` never moves, a range like `^1.4.2` does");
    }
    for (verb, what) in changes {
        status(&verb, &what);
    }
    Ok(ExitCode::SUCCESS)
}

/// `ferry uninstall`: the commands an installed project wrote in
/// `~/.ferry/bin`, and its copy. The project in the current folder, or the one named.
fn uninstall(id: Option<&String>) -> Result<ExitCode, Fail> {
    let home = Home::open()?;
    let id = match id {
        // `github:acme/backup` is the project `acme/backup`
        Some(id) => match project::Package::parse(id) {
            Ok(package) => package.id(),
            Err(_) => id.clone(),
        },
        None => Project::find(&current_dir()?)?.id,
    };
    let bin = home.bin();
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(&bin).into_iter().flatten().flatten() {
        let path = entry.path();
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let owner = text.lines().find_map(|l| l.strip_prefix(OWNER).or_else(|| l.strip_prefix(OWNER_CMD)));
        if owner == Some(id.as_str()) {
            std::fs::remove_file(&path).map_err(|e| format!("can't remove {}: {e}", path.display()))?;
            removed.push(path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        }
    }
    let copy = home.installed(&id);
    let had_copy = copy.exists();
    if had_copy {
        std::fs::remove_dir_all(&copy).map_err(|e| format!("can't remove {}: {e}", copy.display()))?;
    }
    if removed.is_empty() && !had_copy {
        return Err(format!("{id} isn't installed: no command in {} is from it", bin.display()).into());
    }
    removed.sort();
    let commands = if removed.is_empty() { String::new() } else { format!(": {}", removed.join(", ")) };
    status("Uninstalled", &format!("{id}{commands}"));
    Ok(ExitCode::SUCCESS)
}

/// `ferry bundle`: a `.lkb` in `dist/` for each command, or for one, after
/// checking the project *(see ideas/tooling/bundles.md)*.
fn bundle(only: Option<&String>) -> Result<ExitCode, Fail> {
    let (_, project, dependencies) = open_project()?;
    application(&project)?;
    let commands = project.commands();
    let chosen = match only {
        Some(command) if commands.contains(command) => vec![command.clone()],
        Some(command) => return Err(format!("{} has no command `{command}`. The ones there are: {}", project.id, commands.join(", ")).into()),
        None => commands.clone(),
    };
    let errors = check_project(&project, &dependencies)?;
    if errors > 0 {
        return Err(format!("{} has {errors} {}, so it wasn't bundled", project.id, if errors == 1 { "error" } else { "errors" }).into());
    }
    let read = |path: &Path| std::fs::read(path).map_err(|e| Fail(format!("can't read {}: {e}", path.display())));
    // The path inside the bundle, with `/` on every system
    let inside = |folder: &str, root: &Path, file: &Path| {
        let parts: Vec<String> = file.strip_prefix(root).unwrap_or(file).components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        format!("{folder}/{}", parts.join("/"))
    };
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    // Every script of bin/, which a command can `use` like the files next to it
    for command in &commands {
        files.push((format!("bin/{command}.lks"), read(&project.command_file(command))?));
    }
    // The modules of the project and of its dependencies, each under its own author/name/
    let mut modules = 0;
    let mut seen = std::collections::HashSet::new();
    for root in resolve::module_path(&project, &dependencies) {
        for file in files_in(&root) {
            if file.extension().is_none_or(|e| e != "lks") {
                continue;
            }
            let name = inside("src", &root, &file);
            if seen.insert(name.clone()) {
                files.push((name, read(&file)?));
                modules += 1;
            }
        }
    }
    let resources_dir = project.dir.join("resources");
    let resources = files_in(&resources_dir);
    for file in &resources {
        files.push((inside("resources", &resources_dir, file), read(file)?));
    }
    let dist = project.dir.join("dist");
    std::fs::create_dir_all(&dist).map_err(|e| format!("can't create {}: {e}", dist.display()))?;
    for command in &chosen {
        let out = dist.join(format!("{command}.lkb"));
        lakescript::write_bundle(&out, command, &project.id, &project.manifest.project.version, &files).map_err(|e| format!("can't write {}: {e}", out.display()))?;
        let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        let s = |n: usize| if n == 1 { "" } else { "s" };
        let detail = format!("{modules} module{}, {} resource{}, {} KB", s(modules), resources.len(), s(resources.len()), size.div_ceil(1024));
        status("Bundled", &format!("{} ({detail})", relative(&project, &out)));
    }
    Ok(ExitCode::SUCCESS)
}

/// Every file under a folder, sorted, so a bundle is the same every time,
/// without the hidden ones *(i.e. `.DS_Store`, `.git/`)*.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    files.sort();
    files
}

fn install_here() -> Result<ExitCode, Fail> {
    let (home, project, dependencies) = open_project()?;
    application(&project)?;
    if check_project(&project, &dependencies)? > 0 {
        return Err(format!("{} has errors, so it wasn't installed", project.id).into());
    }
    // A copy, so changing or deleting the folder doesn't break the installed commands
    let target = home.installed(&project.id);
    let _ = std::fs::remove_dir_all(&target);
    for part in [MANIFEST, project::LOCK, "src", "bin", "resources"] {
        let from = project.dir.join(part);
        if from.exists() {
            copy(&from, &target.join(part))?;
        }
    }
    let installed = Project::read(&target)?;
    let (dependencies, _) = resolve::resolve(&home, &installed, &Lock::read(&target)?)?;
    write_launchers(&home, &installed, &dependencies)
}

fn install_package(spec: &str) -> Result<ExitCode, Fail> {
    let (package, version) = project::parse_exact_spec(spec)?;
    let home = Home::open()?;
    let (dir, _) = home.fetch(&package, &version)?;
    let project = Project::read(&dir)?;
    if project.id != package.id() {
        return Err(package.wrong_id(&version, &project.id).into());
    }
    application(&project)?;
    project.modules()?;
    too_old(&project);
    let (dependencies, _) = resolve::resolve(&home, &project, &Lock::read(&dir)?)?;
    too_old_dependencies(&dependencies);
    if check_project(&project, &dependencies)? > 0 {
        return Err(format!("{package} {version} has errors, so it wasn't installed").into());
    }
    write_launchers(&home, &project, &dependencies)
}

fn application(project: &Project) -> Result<(), Fail> {
    if project.manifest.project.kind == Kind::Library || project.commands().is_empty() {
        return Err(format!("{} is a library, with no commands to install or bundle. Other projects use it with `ferry add`", project.id).into());
    }
    Ok(())
}

fn copy(from: &Path, to: &Path) -> Result<(), Fail> {
    if from.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| format!("can't create {}: {e}", to.display()))?;
        for entry in std::fs::read_dir(from).map_err(|e| format!("can't read {}: {e}", from.display()))?.flatten() {
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::create_dir_all(to.parent().expect("files are in folders")).map_err(|e| format!("can't create {}: {e}", to.display()))?;
        std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("can't copy {}: {e}", from.display()).into())
    }
}

/// The line that says which project a launcher belongs to.
const OWNER: &str = "# ferry-project: ";
/// The same line in a launcher for Windows, a `.cmd` file.
const OWNER_CMD: &str = "rem ferry-project: ";

fn write_launchers(home: &Home, project: &Project, dependencies: &[Dependency]) -> Result<ExitCode, Fail> {
    let bin = home.bin();
    std::fs::create_dir_all(&bin).map_err(|e| format!("can't create {}: {e}", bin.display()))?;
    let owner_of = |path: &Path| -> Option<String> {
        let text = std::fs::read_to_string(path).ok()?;
        text.lines().find_map(|l| l.strip_prefix(OWNER).or_else(|| l.strip_prefix(OWNER_CMD))).map(String::from)
    };
    let commands = project.commands();
    // Every clash is found before anything is written
    let windows = cfg!(windows);
    for command in &commands {
        let path = bin.join(launcher_name(command, windows));
        if path.exists() && owner_of(&path).as_deref() != Some(project.id.as_str()) {
            let by = owner_of(&path).map_or("something ferry didn't install".to_string(), |o| format!("the project {o}"));
            return Err(format!("{} is already installed by {by}, so {} can't install a command with that name", path.display(), project.id).into());
        }
    }
    // The commands this project doesn't have anymore
    for entry in std::fs::read_dir(&bin).into_iter().flatten().flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let name = if windows { file.strip_suffix(".cmd").unwrap_or(&file).to_string() } else { file };
        if !commands.contains(&name) && owner_of(&entry.path()).as_deref() == Some(project.id.as_str()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let module_path = std::env::join_paths(resolve::module_path(project, dependencies))
        .map_err(|_| "a folder of the module path has a `:` in its name, which `lks --module-path` can't take")?;
    let lks = lks_program();
    for command in &commands {
        let script = project.command_file(command).display().to_string();
        let resources = project.dir.join("resources").display().to_string();
        let launcher = launcher_text(&project.id, &project.manifest.project.version, &lks.display().to_string(), &module_path.to_string_lossy(), &resources, command, &script, windows);
        let path = bin.join(launcher_name(command, windows));
        std::fs::write(&path, launcher).map_err(|e| format!("can't write {}: {e}", path.display()))?;
        make_executable(&path)?;
    }
    status("Installed", &format!("{} {}: {}", project.id, project.manifest.project.version, commands.join(", ")));
    let on_path = std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == bin));
    if !on_path {
        note(&format!("add {} to PATH to run them from anywhere", bin.display()));
    }
    Ok(ExitCode::SUCCESS)
}

/// The file of an installed command: a shell script, or a `.cmd` file on Windows.
fn launcher_name(command: &str, windows: bool) -> String {
    if windows { format!("{command}.cmd") } else { command.to_string() }
}

/// What an installed command runs: `lks` with the project's module path,
/// and the name of the command for the `Usage:` line of its `--help`.
#[allow(clippy::too_many_arguments)]
fn launcher_text(id: &str, version: &str, lks: &str, module_path: &str, resources: &str, name: &str, script: &str, windows: bool) -> String {
    if windows {
        // `cmd` can't always run or find a path that starts with `\\?\`, the
        // form Rust gives real paths in on Windows, so every path is written
        // without it. `%` would start a variable in a `.cmd` file, so it's written twice
        let plain = |path: &str| lakescript::without_verbatim(path).unwrap_or_else(|| path.to_string());
        let q = |text: &str| format!("\"{}\"", text.replace('%', "%%"));
        let module_path: Vec<String> = module_path.split(';').map(plain).collect();
        format!(
            "@echo off\r\nrem Installed by ferry from {id} {version}. Don't edit it: `ferry install` writes it again.\r\n{OWNER_CMD}{id}\r\n{} --no-type-check --module-path {} --resources {} --name {} -- {} %*\r\n",
            q(&plain(lks)),
            q(&module_path.join(";")),
            q(&plain(resources)),
            q(name),
            q(&plain(script)),
        )
    } else {
        format!(
            "#!/bin/sh\n# Installed by ferry from {id} {version}. Don't edit it: `ferry install` writes it again.\n{OWNER}{id}\nexec {} --no-type-check --module-path {} --resources {} --name {} -- {} \"$@\"\n",
            quote(lks),
            quote(module_path),
            quote(resources),
            quote(name),
            quote(script),
        )
    }
}

/// `lks` next to `ferry` when it's there, or the one on `PATH`.
fn lks_program() -> PathBuf {
    let name = if cfg!(windows) { "lks.exe" } else { "lks" };
    // Without Windows' `\\?\` before it, which `cmd` can't always run from a launcher
    let next_to_ferry = std::env::current_exe().ok().map(|exe| lakescript::canonical(&exe).with_file_name(name));
    match next_to_ferry {
        Some(lks) if lks.is_file() => lks,
        _ => PathBuf::from("lks"),
    }
}

/// Quoted for `sh`.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), Fail> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("can't make {} executable: {e}", path.display()).into())
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), Fail> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{launcher_name, launcher_text, with_dependency};

    #[test]
    fn launchers_for_each_system() {
        let sh = launcher_text("a/b", "1.0.0", "/bin/lks", "/x/src", "/x/resources", "go", "/x/bin/go.lks", false);
        assert!(sh.starts_with("#!/bin/sh\n"), "{sh}");
        assert!(sh.contains("\n# ferry-project: a/b\n"), "{sh}");
        assert!(sh.ends_with("exec '/bin/lks' --no-type-check --module-path '/x/src' --resources '/x/resources' --name 'go' -- '/x/bin/go.lks' \"$@\"\n"), "{sh}");
        let cmd = launcher_text("a/b", "1.0.0", r"C:\lks\lks.exe", r"C:\x\src;C:\100%", r"C:\x\resources", "go", r"C:\x\bin\go.lks", true);
        assert!(cmd.starts_with("@echo off\r\n"), "{cmd}");
        assert!(cmd.contains("\r\nrem ferry-project: a/b\r\n"), "{cmd}");
        let last = r#""C:\lks\lks.exe" --no-type-check --module-path "C:\x\src;C:\100%%" --resources "C:\x\resources" --name "go" -- "C:\x\bin\go.lks" %*"#;
        assert!(cmd.ends_with(&format!("{last}\r\n")), "{cmd}");
        // Paths in the form `\\?\C:\...` are written without it, which `cmd` can't run
        let cmd = launcher_text("a/b", "1.0.0", r"\\?\C:\lks\lks.exe", r"\\?\C:\x\src;\\?\UNC\pc\share\src", r"\\?\C:\x\resources", "go", r"\\?\C:\x\bin\go.lks", true);
        let last = r#""C:\lks\lks.exe" --no-type-check --module-path "C:\x\src;\\pc\share\src" --resources "C:\x\resources" --name "go" -- "C:\x\bin\go.lks" %*"#;
        assert!(cmd.ends_with(&format!("{last}\r\n")), "{cmd}");
        assert!(!cmd.contains(r"\\?\"), "{cmd}");
        assert_eq!(launcher_name("go", true), "go.cmd");
        assert_eq!(launcher_name("go", false), "go");
    }

    #[test]
    fn adding_a_dependency_keeps_the_file() {
        let text = "[project]\nname = \"x\"\n\n[dependencies]\n\"github:a/b\" = \"1.0.0\"\n\n[other]\n";
        assert_eq!(
            with_dependency(text, "github:c/d", "2.0.0"),
            "[project]\nname = \"x\"\n\n[dependencies]\n\"github:a/b\" = \"1.0.0\"\n\"github:c/d\" = \"2.0.0\"\n\n[other]\n"
        );
        assert_eq!(with_dependency(text, "github:a/b", "1.1.0"), text.replace("1.0.0", "1.1.0"));
        assert_eq!(with_dependency("[project]\n", "github:a/b", "1.0.0"), "[project]\n\n[dependencies]\n\"github:a/b\" = \"1.0.0\"\n");
    }
}
