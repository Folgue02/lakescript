//! `ferry`, the tool for Lakescript projects: their file, their
//! dependencies, and installing their commands *(see ideas/tooling/ferry.md)*.


use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lakescript::terminal::{Paint, When};
use lakescript::{Diagnostic, Engine};

use ferry::home::Home;
use ferry::project::{self, Kind, MANIFEST, Project};
use ferry::resolve::{self, Dependency, Lock};
use ferry::{Fail, PAINT, paint, status};

/// Scripts run on a thread with a big stack, like with `lks`.
const STACK_SIZE: usize = 512 * 1024 * 1024;

const USAGE: &str = "\
The tool for Lakescript projects.

Usage: ferry <command> [arguments...]

Commands:
  new <author/name> [--lib] [--example]
                              Create a project in a new folder. --lib makes a library,
                              and --example adds a module and a command to try it out
  run [command] [args...]     Run one of the project's commands from bin/. The name
                              can be left out when there's only one
  check                       Type check the whole project, its tests too, without
                              running it
  test [filter]               Run the tests in tests/: their functions named test...
                              With a filter, only the ones whose name has it
  doc [--open]                Make the docs of the project in .ferry/doc/, and
                              open them with --open
  doc <repository:author/name@version> [--open]
                              Make the docs of a package
  doc std [--open]            The pages of the standard library
  doc <name>                  Write the docs of one thing in the terminal,
                              i.e. ferry doc units::bytes
  add <repository:author/name@version>
                              Add a dependency, i.e. ferry add github:acme/http@1.4.2
  install [repository:author/name@version]
                              Install the commands of an application into ~/.ferry/bin:
                              the project in this folder, or one from a repository

Options:
  --color <when>              When to use colors: auto (on a terminal, the default),
                              always or never. NO_COLOR also turns them off.
                              It goes before the command
  -h, --help                  Show this help
  -V, --version               Show the version of ferry

FERRY_HOME replaces ~/.ferry, where ferry keeps what it downloads and installs.
";

/// A hint, in cyan.
fn note(text: &str) {
    eprintln!("{} {text}", paint().note("note:"));
}

fn main() -> ExitCode {
    let runner = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(start);
    match runner.map(|handle| handle.join()) {
        Ok(Ok(code)) => code,
        _ => {
            eprintln!("{} ferry crashed. This is a bug in ferry", paint().error("error:"));
            ExitCode::from(70)
        }
    }
}

fn start() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut color = When::Auto;
    while args.first().is_some_and(|a| a == "--color") {
        match args.get(1).and_then(|w| When::parse(w)) {
            Some(when) => color = when,
            None => return usage_error("`--color` needs `auto`, `always` or `never` after it"),
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
        "check" => match rest {
            [] => check(),
            _ => return usage_error("`ferry check` takes no arguments"),
        },
        "doc" => doc(rest),
        "test" => match rest {
            [] => test(None),
            [filter] => test(Some(filter)),
            _ => return usage_error("`ferry test` takes one filter at most, i.e. `ferry test bytes`"),
        },
        "add" => match rest {
            [spec] => add(spec),
            _ => return usage_error("`ferry add` takes one package, i.e. `ferry add github:acme/http@1.4.2`"),
        },
        "install" => match rest {
            [] => install_here(),
            [spec] => install_package(spec),
            _ => return usage_error("`ferry install` takes one package at most"),
        },
        other => return usage_error(&format!("there's no command `{other}`")),
    };
    match result {
        Ok(code) => code,
        Err(Fail(message)) => {
            eprintln!("{} {message}", paint().error("error:"));
            ExitCode::FAILURE
        }
    }
}

fn usage_error(message: &str) -> ExitCode {
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
    write(dir.join(".gitignore"), ".ferry/\n".to_string())?;
    std::fs::create_dir_all(&src).map_err(|e| format!("can't create {}: {e}", src.display()))?;
    if example {
        write(src.join("greeting.lks"), "pub string greet(string $who) = f\"Hello, {$who}!\";\n".to_string())?;
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
    project.modules()?;
    let (dependencies, lock) = resolve::resolve(&home, &project, &Lock::read(&project.dir)?)?;
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
    match engine.run_file(&script) {
        Ok(outcome) => Ok(ExitCode::from(outcome.code().clamp(0, 255) as u8)),
        // The person's mistake in the arguments, like `lks` shows it
        Err(ref diagnostic) if let Some(text) = diagnostic.usage() => {
            eprintln!("{} {text}", paint().error("error:"));
            Ok(ExitCode::from(2))
        }
        Err(diagnostic) => {
            eprintln!("{}", show(&diagnostic, &script));
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
    if errors > 0 {
        return Err(format!("{} has {errors} {}", project.id, if errors == 1 { "error" } else { "errors" }).into());
    }
    status("Checked", &format!("{}: no errors", project.id));
    Ok(ExitCode::SUCCESS)
}

/// Type checks every command and every module of a project, and prints the
/// errors. It returns how many different errors there were.
fn check_project(project: &Project, dependencies: &[Dependency]) -> Result<usize, Fail> {
    let module_path = resolve::module_path(project, dependencies);
    // An error in a module is shown once, not once for every file that uses it
    let mut shown = std::collections::HashSet::new();
    let mut report = |text: String| {
        if shown.insert(text.clone()) {
            eprintln!("{text}\n");
        }
    };
    for command in project.commands() {
        let script = project.command_file(&command);
        if let Err(diagnostic) = engine(&module_path).check_file(&script) {
            report(show(&diagnostic, &script));
        }
    }
    for file in project.test_files() {
        if let Err(diagnostic) = engine(&module_path).check_file(&file) {
            report(show_as(&diagnostic, &file, &relative(project, &file)));
        }
    }
    // Modules no command uses are checked too
    let modules = project.modules()?;
    let files = project.commands().into_iter().map(|c| project.command_file(&c)).chain(project.test_files()).chain(modules.iter().map(|m| project.src().join(format!("{m}.lks"))));
    for file in files {
        stale_comments(project, &file);
    }
    if !modules.is_empty() {
        let source: String = modules.iter().enumerate().map(|(i, m)| format!("use {m} as module{i};\n")).collect();
        if let Err(diagnostic) = engine(&module_path).check_source(&source) {
            report(show(&diagnostic, &project.src()));
        }
    }
    Ok(shown.len())
}

/// Warns about the comments of a file that don't match its code *(i.e. a
/// `$name:` for a parameter that was renamed)*. They never stop anything.
fn stale_comments(project: &Project, file: &Path) {
    let Ok(text) = std::fs::read_to_string(file) else { return };
    let Ok(warnings) = lakescript::docs::stale_params(&text) else { return };
    let paint = paint();
    for w in warnings {
        eprintln!("{} {}
 {} {}:{}:{}
", paint.warning("warning:"), w.message, paint.note("-->"), relative(project, file), w.line, w.column);
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
        [spec] if spec.contains(":") && !spec.contains("::") => doc_package(spec, open),
        [name] if !name.starts_with('-') => doc_name(name),
        _ => Ok(usage_error("`ferry doc` takes a package, `std` or a name, and maybe `--open`")),
    }
}

fn doc_project(open: bool) -> Result<ExitCode, Fail> {
    let (home, project, dependencies) = open_project()?;
    let errors = check_project(&project, &dependencies)?;
    if errors > 0 {
        let what = if errors == 1 { "error" } else { "errors" };
        return Err(format!("{} has {errors} {what}, so its docs weren't made", project.id).into());
    }
    let mut site = ferry::doc::read(&project)?;
    // The standard library's pages, which the project's link to and search
    let std_index = std_docs(&home)?;
    let std_base = format!("{}/", file_url(std_index.parent().expect("the pages are in a folder")));
    site.std = Some((Box::new(ferry::doc::std_site()), std_base));
    for dependency in &dependencies {
        let index = package_docs(&home, &dependency.project, &dependency.package, &dependency.version)?;
        site.dependencies.push((dependency.package.to_string(), dependency.version.clone(), Some(file_url(&index))));
    }
    let out = project.dir.join(".ferry").join("doc");
    let index = ferry::doc::write(&site, &out)?;
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
    let (package, version) = project::parse_spec(spec)?;
    let home = Home::open()?;
    let (dir, _) = home.fetch(&package, &version)?;
    let project = Project::read(&dir)?;
    if project.id != package.id() {
        return Err(format!("{package} {version} says it's {}, not {}", project.id, package.id()).into());
    }
    let index = package_docs(&home, &project, &package, &version)?;
    if open {
        open_page(&index);
    } else {
        println!("{}", index.display());
    }
    Ok(ExitCode::SUCCESS)
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
    let (_, project, _) = open_project()?;
    let site = ferry::doc::read(&project)?;
    // The project's own first, then the standard library's
    match ferry::doc::describe(&site, name).or_else(|| ferry::doc::describe(&ferry::doc::std_site(), name)) {
        Some(text) => {
            print!("{text}");
            Ok(ExitCode::SUCCESS)
        }
        None => Err(format!("there's nothing named `{name}` in the docs of {}", project.id).into()),
    }
}

fn file_url(path: &Path) -> String {
    let path = lakescript::canonical(path).display().to_string().replace('\\', "/");
    // `C:/x` on Windows is `file:///C:/x`
    let slash = if path.starts_with('/') { "" } else { "/" };
    format!("file://{slash}{path}")
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

/// `ferry test`: every file in `tests/`, and its tests whose name has `filter`.
fn test(filter: Option<&str>) -> Result<ExitCode, Fail> {
    let (_, project, dependencies) = open_project()?;
    let files = project.test_files();
    status("Testing", &project.id);
    if files.is_empty() {
        note("there are no tests: they're the functions named `test...` of the files in tests/");
        return Ok(ExitCode::SUCCESS);
    }
    let module_path = resolve::module_path(&project, &dependencies);
    let start = std::time::Instant::now();
    let mut results = Vec::new();
    let mut broken = 0;
    for file in &files {
        let shown = relative(&project, file);
        match engine(&module_path).run_tests(file, filter) {
            Ok(found) if found.is_empty() => {}
            Ok(found) => {
                eprintln!("{shown}");
                for result in &found {
                    eprint!("{}", lakescript::render_result(result, &shown, paint()));
                }
                results.extend(found);
            }
            Err(diagnostic) => {
                broken += 1;
                eprintln!("{}\n", show_as(&diagnostic, file, &shown));
            }
        }
    }
    eprint!("{}", lakescript::render_summary(&results, start.elapsed().as_secs_f64(), paint()));
    if broken > 0 {
        let files = if broken == 1 { "file of tests" } else { "files of tests" };
        return Err(format!("{broken} {files} couldn't run").into());
    }
    Ok(if results.iter().all(|r| r.passed()) { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// A file as seen from the project's folder *(i.e. `tests/units.lks`)*.
fn relative(project: &Project, file: &Path) -> String {
    file.strip_prefix(&project.dir).unwrap_or(file).display().to_string()
}

/// `show`, naming the script `name` in the error.
fn show_as(diagnostic: &Diagnostic, script: &Path, name: &str) -> String {
    let file = diagnostic.file.clone().map(PathBuf::from).unwrap_or_else(|| script.to_path_buf());
    match std::fs::read_to_string(&file) {
        Ok(source) if diagnostic.span.line > 0 => diagnostic.render_with_source_painted(name, &source, paint()),
        _ => diagnostic.render_painted(name, paint()),
    }
}

/// An error with the line of code it points to, read from the file it's in.
fn show(diagnostic: &Diagnostic, script: &Path) -> String {
    let name = script.display().to_string();
    let file = diagnostic.file.clone().unwrap_or_else(|| name.clone());
    match std::fs::read_to_string(&file) {
        Ok(source) if diagnostic.span.line > 0 => diagnostic.render_with_source_painted(&name, &source, paint()),
        _ => diagnostic.render_painted(&name, paint()),
    }
}

fn add(spec: &str) -> Result<ExitCode, Fail> {
    let (package, version) = project::parse_spec(spec)?;
    let home = Home::open()?;
    home.knows(&package)?;
    let mut project = Project::find(&current_dir()?)?;
    // Resolved with the new dependency first, so a clash leaves ferry.toml as it was
    project.dependencies.retain(|(p, _)| *p != package);
    project.dependencies.push((package.clone(), version.clone()));
    let (_, lock) = resolve::resolve(&home, &project, &Lock::read(&project.dir)?)?;
    let file = project.dir.join(MANIFEST);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
    let text = with_dependency(&text, &package.to_string(), &version);
    std::fs::write(&file, &text).map_err(|e| format!("can't write {}: {e}", file.display()))?;
    lock.write(&project.dir)?;
    status("Added", &format!("{package} {version}"));
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

fn install_here() -> Result<ExitCode, Fail> {
    let (home, project, dependencies) = open_project()?;
    application(&project)?;
    if check_project(&project, &dependencies)? > 0 {
        return Err(format!("{} has errors, so it wasn't installed", project.id).into());
    }
    // A copy, so changing or deleting the folder doesn't break the installed commands
    let target = home.installed(&project.id);
    let _ = std::fs::remove_dir_all(&target);
    for part in [MANIFEST, project::LOCK, "src", "bin"] {
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
    let (package, version) = project::parse_spec(spec)?;
    let home = Home::open()?;
    let (dir, _) = home.fetch(&package, &version)?;
    let project = Project::read(&dir)?;
    if project.id != package.id() {
        return Err(format!("{package} {version} says it's {}, not {}", project.id, package.id()).into());
    }
    application(&project)?;
    project.modules()?;
    let (dependencies, _) = resolve::resolve(&home, &project, &Lock::read(&dir)?)?;
    if check_project(&project, &dependencies)? > 0 {
        return Err(format!("{package} {version} has errors, so it wasn't installed").into());
    }
    write_launchers(&home, &project, &dependencies)
}

fn application(project: &Project) -> Result<(), Fail> {
    if project.manifest.project.kind == Kind::Library || project.commands().is_empty() {
        return Err(format!("{} is a library, with no commands to install. Other projects use it with `ferry add`", project.id).into());
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
        let launcher = launcher_text(&project.id, &project.manifest.project.version, &lks.display().to_string(), &module_path.to_string_lossy(), &script, windows);
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

/// What an installed command runs: `lks` with the project's module path.
fn launcher_text(id: &str, version: &str, lks: &str, module_path: &str, script: &str, windows: bool) -> String {
    if windows {
        // `%` would start a variable in a `.cmd` file, so it's written twice
        let q = |text: &str| format!("\"{}\"", text.replace('%', "%%"));
        format!(
            "@echo off\r\nrem Installed by ferry from {id} {version}. Don't edit it: `ferry install` writes it again.\r\n{OWNER_CMD}{id}\r\n{} --no-type-check --module-path {} -- {} %*\r\n",
            q(lks),
            q(module_path),
            q(script),
        )
    } else {
        format!(
            "#!/bin/sh\n# Installed by ferry from {id} {version}. Don't edit it: `ferry install` writes it again.\n{OWNER}{id}\nexec {} --no-type-check --module-path {} -- {} \"$@\"\n",
            quote(lks),
            quote(module_path),
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
        let sh = launcher_text("a/b", "1.0.0", "/bin/lks", "/x/src", "/x/bin/go.lks", false);
        assert!(sh.starts_with("#!/bin/sh\n"), "{sh}");
        assert!(sh.contains("\n# ferry-project: a/b\n"), "{sh}");
        assert!(sh.ends_with("exec '/bin/lks' --no-type-check --module-path '/x/src' -- '/x/bin/go.lks' \"$@\"\n"), "{sh}");
        let cmd = launcher_text("a/b", "1.0.0", r"C:\lks\lks.exe", r"C:\x\src;C:\100%", r"C:\x\bin\go.lks", true);
        assert!(cmd.starts_with("@echo off\r\n"), "{cmd}");
        assert!(cmd.contains("\r\nrem ferry-project: a/b\r\n"), "{cmd}");
        let last = r#""C:\lks\lks.exe" --no-type-check --module-path "C:\x\src;C:\100%%" -- "C:\x\bin\go.lks" %*"#;
        assert!(cmd.ends_with(&format!("{last}\r\n")), "{cmd}");
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
