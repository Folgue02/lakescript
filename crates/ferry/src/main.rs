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
  new <author/name> [--lib]   Create a project in a new folder. --lib makes a library
  run [command] [args...]     Run one of the project's commands from bin/. The name
                              can be left out when there's only one
  check                       Type check the whole project without running it
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
        "new" => match rest {
            [id] => new(id, false),
            [id, lib] | [lib, id] if lib == "--lib" => new(id, true),
            _ => return usage_error("`ferry new` takes the project, i.e. `ferry new folgue02/backup`, and maybe `--lib`"),
        },
        "run" => run(rest),
        "check" => match rest {
            [] => check(),
            _ => return usage_error("`ferry check` takes no arguments"),
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

fn new(spec: &str, library: bool) -> Result<ExitCode, Fail> {
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
    write(src.join("greeting.lks"), "pub string greet(string $who) = f\"Hello, {$who}!\";\n".to_string())?;
    if !library {
        write(
            dir.join("bin").join(format!("{name}.lks")),
            format!(
                "use std/process;\nuse {id}/greeting;\n\noutln(greeting::greet(process::args()[0] ?? 'world'));\n"
            ),
        )?;
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
        Err(diagnostic) => {
            eprintln!("{}", show(&diagnostic, &script));
            Ok(ExitCode::FAILURE)
        }
    }
}

fn engine(module_path: &[PathBuf]) -> Engine {
    let mut engine = Engine::new();
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
    // Modules no command uses are checked too
    let modules = project.modules()?;
    if !modules.is_empty() {
        let source: String = modules.iter().enumerate().map(|(i, m)| format!("use {m} as module{i};\n")).collect();
        if let Err(diagnostic) = engine(&module_path).check_source(&source) {
            report(show(&diagnostic, &project.src()));
        }
    }
    Ok(shown.len())
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
    let next_to_ferry = std::env::current_exe().ok().and_then(|exe| exe.canonicalize().ok()).map(|exe| exe.with_file_name(name));
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
