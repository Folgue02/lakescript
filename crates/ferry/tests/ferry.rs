//! `ferry` with packages in local git repositories, configured like any other
//! repository, and its own `FERRY_HOME` for every test.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct World {
    dir: PathBuf,
}

impl World {
    fn new() -> World {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ferry-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("home")).unwrap();
        std::fs::create_dir_all(dir.join("work")).unwrap();
        let url = format!("file://{}/repos/{{author}}/{{name}}", dir.display());
        std::fs::write(dir.join("home/config.toml"), format!("[repositories]\nlocal = \"{url}\"\n")).unwrap();
        World { dir }
    }

    fn home(&self) -> PathBuf {
        self.dir.join("home")
    }

    fn work(&self, project: &str) -> PathBuf {
        self.dir.join("work").join(project)
    }

    fn ferry(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ferry")).args(args).current_dir(cwd)
            .env("FERRY_HOME", self.home())
            .env_remove("CLICOLOR_FORCE")
            .output()
            .unwrap()
    }

    /// Runs ferry and expects it to work.
    fn ok(&self, cwd: &Path, args: &[&str]) -> String {
        let out = self.ferry(cwd, args);
        assert!(out.status.success(), "ferry {args:?} failed:\n{}", stderr(&out));
        stdout(&out)
    }

    /// Runs ferry and expects it to fail, returning what it said.
    fn fails(&self, cwd: &Path, args: &[&str]) -> String {
        let out = self.ferry(cwd, args);
        assert!(!out.status.success(), "ferry {args:?} should have failed:\n{}", stdout(&out));
        stderr(&out)
    }

    /// Writes a project in `work/`.
    fn project(&self, folder: &str, toml: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = self.work(folder);
        write(&dir.join("ferry.toml"), toml);
        for (path, text) in files {
            write(&dir.join(path), text);
        }
        dir
    }

    /// Publishes a project as a git repository with a tag for its version.
    fn publish(&self, author: &str, name: &str, version: &str, deps: &str, files: &[(&str, &str)]) {
        self.publish_kind("library", author, name, version, deps, files);
    }

    fn publish_kind(&self, kind: &str, author: &str, name: &str, version: &str, deps: &str, files: &[(&str, &str)]) {
        let toml = format!("[project]\nauthor = \"{author}\"\nname = \"{name}\"\nversion = \"{version}\"\nkind = \"{kind}\"\n\n[dependencies]\n{deps}");
        let dir = self.project(&format!("{author}-{name}"), &toml, files);
        let repo = self.dir.join("repos").join(author).join(name);
        if !dir.join(".git").exists() {
            git(&dir, &["init", "-q"]);
            std::fs::create_dir_all(repo.parent().unwrap()).unwrap();
            git(&dir, &["init", "-q", "--bare", repo.to_str().unwrap()]);
            git(&dir, &["remote", "add", "origin", repo.to_str().unwrap()]);
        }
        git(&dir, &["add", "-A"]);
        git(&dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", version]);
        git(&dir, &["tag", "-f", &format!("v{version}")]);
        git(&dir, &["push", "-q", "-f", "origin", "HEAD:refs/heads/main", "--tags"]);
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", stderr(&out));
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn app(deps: &str) -> String {
    format!("[project]\nauthor = \"me\"\nname = \"app\"\nversion = \"0.1.0\"\nkind = \"application\"\n\n[dependencies]\n{deps}")
}

#[test]
fn a_new_project_runs() {
    let w = World::new();
    let work = w.dir.join("work");
    w.ok(&work, &["new", "some-user/my.tool", "--example"]);
    let project = w.work("my.tool");
    assert!(project.join("src/some_user/my_tool/greeting.lks").is_file());
    assert_eq!(w.ok(&project, &["run"]), "Hello, world!\n");
    assert_eq!(w.ok(&project, &["run", "Ana"]), "Hello, Ana!\n");
    assert_eq!(w.ok(&project, &["run", "my.tool", "Bo"]), "Hello, Bo!\n");
    w.ok(&project, &["check"]);
    // A library has no commands
    w.ok(&work, &["new", "me/lib", "--lib"]);
    assert!(w.fails(&w.work("lib"), &["run"]).contains("has no commands"));
    // With a place for its tests and the front page of its docs
    let lib = w.work("lib");
    assert!(lib.join("tests").is_dir() && !lib.join("bin").exists());
    let front = std::fs::read_to_string(lib.join("src/me/lib/module.md")).unwrap();
    assert!(front.starts_with("# me/lib\n") && front.contains("ferry doc"), "{front}");
    w.ok(&lib, &["doc"]);
    assert!(std::fs::read_to_string(lib.join(".ferry/doc/index.html")).unwrap().contains("What lib is for"));
    // An application gets neither
    assert!(!project.join("tests").exists() && !project.join("src/some_user/my_tool/module.md").exists());
    assert!(w.fails(&work, &["new", "me/lib"]).contains("already exists"));
    assert_eq!(w.ferry(&work, &["new", "me/x", "--nope"]).status.code(), Some(2));
}

#[test]
fn a_new_project_has_no_sample_code() {
    let w = World::new();
    let work = w.dir.join("work");
    w.ok(&work, &["new", "me/tool"]);
    let project = w.work("tool");
    assert!(project.join("src/me/tool").is_dir());
    assert!(!project.join("src/me/tool/greeting.lks").exists());
    assert_eq!(std::fs::read_to_string(project.join("bin/tool.lks")).unwrap(), "");
    assert_eq!(w.ok(&project, &["run"]), "");
    w.ok(&project, &["check"]);
    w.ok(&work, &["new", "me/lib", "--lib"]);
    assert!(w.work("lib").join("src/me/lib").is_dir());
    assert!(!w.work("lib").join("bin").exists());
}

#[test]
fn check_warns_about_comments_of_parameters_that_arent_there() {
    let w = World::new();
    let module = "/// $text: what to read\npub number size(string $input) = $input.length();\n";
    let dir = w.project("app", &app(""), &[("bin/app.lks", "outln(1);\n"), ("src/me/app/sizes.lks", module)]);
    let out = w.ferry(&dir, &["check"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("warning: the comment describes `$text`, but `size` has no parameter `$text`"), "{err}");
    assert!(err.contains("src/me/app/sizes.lks:1:5"), "{err}");
}

#[test]
fn wrong_arguments_to_a_command_are_a_usage_error() {
    let w = World::new();
    let script = "command Greet {\n    string @who;\n}\nconst $cli = Greet::parse();\noutln($cli.who);\n";
    let dir = w.project("app", &app(""), &[("bin/app.lks", script)]);
    assert_eq!(w.ok(&dir, &["run", "Ana"]), "Ana\n");
    let out = w.ferry(&dir, &["run"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr(&out);
    assert!(err.starts_with("error: "), "{err}");
    assert!(err.contains("Usage:"), "{err}");
    assert!(!err.contains("UsageError") && !err.contains("-->") && !err.contains("parse()"), "{err}");
}

#[test]
fn dependencies_are_downloaded_and_locked() {
    let w = World::new();
    w.publish("acme", "strings", "1.0.0", "", &[("src/acme/strings/text.lks", "pub string shout(string $s) = $s.upper();")]);
    w.publish(
        "acme",
        "http",
        "2.1.0",
        "\"local:acme/strings\" = \"1.0.0\"\n",
        &[("src/acme/http/client.lks", "use acme/strings/text;\npub string get(string $url) = text::shout(f\"get {$url}\");")],
    );
    let dir = w.project("app", &app(""), &[("bin/app.lks", "use acme/http/client;\noutln(client::get('x'));")]);
    w.ok(&dir, &["add", "local:acme/http@2.1.0"]);
    let toml = std::fs::read_to_string(dir.join("ferry.toml")).unwrap();
    assert!(toml.contains("\"local:acme/http\" = \"2.1.0\""), "{toml}");
    let lock = std::fs::read_to_string(dir.join("ferry.lock")).unwrap();
    assert!(lock.contains("local:acme/http") && lock.contains("local:acme/strings"), "{lock}");
    assert_eq!(w.ok(&dir, &["run"]), "GET X\n");
    assert!(w.home().join("repository/local/acme/strings/1.0.0/src/acme/strings/text.lks").is_file());
    // Only the dependency's ID reaches its modules: there's no `strings/text`
    write(&dir.join("bin/app.lks"), "use strings/text;\noutln(1);");
    assert!(w.fails(&dir, &["run"]).contains("not found"));
}

#[test]
fn clashes_are_found_before_anything_runs() {
    let w = World::new();
    w.publish("acme", "base", "1.0.0", "", &[("src/acme/base/b.lks", "pub number one() = 1;")]);
    w.publish("acme", "base", "2.0.0", "", &[("src/acme/base/b.lks", "pub number one() = 2;")]);
    w.publish("acme", "left", "1.0.0", "\"local:acme/base\" = \"1.0.0\"\n", &[("src/acme/left/l.lks", "")]);
    w.publish("acme", "right", "1.0.0", "\"local:acme/base\" = \"2.0.0\"\n", &[("src/acme/right/r.lks", "")]);
    let run = "bin/app.lks";
    let dir = w.project("app", &app("\"local:acme/left\" = \"1.0.0\"\n\"local:acme/right\" = \"1.0.0\"\n"), &[(run, "outln('ran');")]);
    let said = w.fails(&dir, &["run"]);
    assert!(said.contains("needed in two versions") && said.contains("1.0.0") && said.contains("2.0.0"), "{said}");
    assert!(!said.contains("ran"));

    // Two IDs that end up the same
    w.publish("a-b", "x", "1.0.0", "", &[("src/a_b/x/m.lks", "")]);
    w.publish("a_b", "x", "1.0.0", "", &[("src/a_b/x/m.lks", "")]);
    let dir = w.project("app2", &app("\"local:a-b/x\" = \"1.0.0\"\n\"local:a_b/x\" = \"1.0.0\"\n"), &[(run, "")]);
    assert!(w.fails(&dir, &["run"]).contains("two packages have the ID a_b/x"));

    // A module outside src/<ID>/
    w.publish("acme", "messy", "1.0.0", "", &[("src/acme/messy/ok.lks", ""), ("src/config.lks", "")]);
    let dir = w.project("app3", &app("\"local:acme/messy\" = \"1.0.0\"\n"), &[(run, "")]);
    assert!(w.fails(&dir, &["run"]).contains("src/config.lks is outside src/acme/messy/"));
}

#[test]
fn a_moved_tag_is_noticed() {
    let w = World::new();
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "pub number n() = 1;")]);
    let dir = w.project("app", &app(""), &[("bin/app.lks", "use acme/lib/m;\noutln(m::n());")]);
    w.ok(&dir, &["add", "local:acme/lib@1.0.0"]);
    // The tag now points to another commit, and the local repository is gone
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "pub number n() = 2;")]);
    std::fs::remove_dir_all(w.home().join("repository")).unwrap();
    let said = w.fails(&dir, &["run"]);
    assert!(said.contains("the tag v1.0.0 was moved"), "{said}");
}

#[test]
fn wrong_packages_and_versions() {
    let w = World::new();
    let dir = w.project("app", &app(""), &[("bin/app.lks", "")]);
    assert!(w.fails(&dir, &["add", "local:acme/lib@1.4"]).contains("isn't a version: it's an exact one like `1.4.2`, or a range like `^1.4.2`"));
    assert!(w.fails(&dir, &["add", "local:acme/lib"]).contains("needs a version"));
    assert!(w.fails(&dir, &["add", "nowhere:acme/lib@1.0.0"]).contains("there's no repository named `nowhere`"));
    assert!(w.fails(&dir, &["add", "local:acme/missing@1.0.0"]).contains("can't download"));
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "")]);
    assert!(w.fails(&dir, &["add", "local:acme/lib@1.2.0"]).contains("has no version 1.2.0"));
    // Nothing was added
    assert!(!std::fs::read_to_string(dir.join("ferry.toml")).unwrap().contains("acme"));
    assert!(w.fails(&dir, &["add", "local:9acme/lib@1.0.0"]).contains("can't start with a digit"));
    assert_eq!(w.ferry(&dir, &["nope"]).status.code(), Some(2));
    // The package is checked before the version, so the example is a right one
    assert!(w.fails(&dir, &["add", "acme/lib"]).contains("isn't a package"));
    assert!(w.fails(&dir, &["add", "local:acme/lib"]).contains("`local:acme/lib` needs a version, i.e. `local:acme/lib@1.0.0`"));
    // A version that isn't there says which ones are
    w.publish("acme", "lib", "1.1.0", "", &[("src/acme/lib/m.lks", "")]);
    assert!(w.fails(&dir, &["add", "local:acme/lib@1.2.0"]).contains("Its versions are: 1.0.0, 1.1.0"));
    // Adding it again, and another version
    w.ok(&dir, &["add", "local:acme/lib@1.0.0"]);
    assert!(stderr(&w.ferry(&dir, &["add", "local:acme/lib@1.0.0"])).contains("note: local:acme/lib 1.0.0 is already a dependency"));
    assert!(stderr(&w.ferry(&dir, &["--color", "never", "add", "local:acme/lib@1.1.0"])).contains("Changed local:acme/lib from 1.0.0 to 1.1.0"));
}

#[test]
fn a_test_filter_that_matches_nothing_says_so() {
    let w = World::new();
    let dir = w.project("app", &app(""), &[("bin/app.lks", ""), ("tests/units.lks", "void testBytes() { assert true; }\n")]);
    let said = stderr(&w.ferry(&dir, &["--color", "never", "test", "units"]));
    assert!(said.contains("note: no test has `units` in its name"), "{said}");
    assert!(!stderr(&w.ferry(&dir, &["test", "Bytes"])).contains("no test has"));
}

#[test]
fn check_finds_errors_in_commands_and_unused_modules() {
    let w = World::new();
    let dir = w.project(
        "app",
        &app(""),
        &[("bin/app.lks", "outln(1);"), ("bin/bad.lks", "number $n = 'text';"), ("src/me/app/unused.lks", "pub number f() = 'no';")],
    );
    let said = w.fails(&dir, &["check"]);
    assert!(said.contains("bad.lks") && said.contains("unused.lks") && said.contains("has 2 errors"), "{said}");
    // Several commands need a name
    assert!(w.fails(&dir, &["run"]).contains("say which one"));
    assert_eq!(w.ok(&dir, &["run", "app"]), "1\n");
}

#[test]
fn check_shows_the_error_of_every_file() {
    let w = World::new();
    let dir = w.project(
        "lib",
        &app(""),
        &[
            ("src/me/app/w.lks", "pub number a() = 'no';\n"),
            ("src/me/app/z.lks", "pub number b() = 'no';\n"),
            ("tests/own.lks", "void testX() { const number $n = 'x'; }\n"),
            ("tests/uses.lks", "use me/app/w;\nvoid testY() { const number $n = 'x'; }\n"),
        ],
    );
    let said = w.fails(&dir, &["--color", "never", "check"]);
    // Each one, as seen from the project's folder
    for place in ["--> src/me/app/w.lks:1:18", "--> src/me/app/z.lks:1:18", "--> tests/own.lks:1:34"] {
        assert!(said.contains(place), "{place}:\n{said}");
    }
    assert!(!said.contains(&w.dir.display().to_string()), "an absolute path:\n{said}");
    // The one that stops in a module says so, and the error is counted once
    assert!(said.contains("note: tests/uses.lks is checked once the error in src/me/app/w.lks, which it uses, is fixed"), "{said}");
    assert!(said.contains("has 3 errors"), "{said}");
}

/// The launchers run `lks`, which ferry finds next to itself.
fn build_lks() {
    let out = Command::new(env!("CARGO")).args(["build", "-q", "-p", "lks"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
}

#[test]
fn install_writes_launchers_for_every_command() {
    build_lks();
    let w = World::new();
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "pub string hi() = 'hi';")]);
    let dir = w.project(
        "app",
        &app("\"local:acme/lib\" = \"1.0.0\"\n"),
        &[("bin/hello.lks", "use acme/lib/m;\nuse std/process;\noutln(f\"{m::hi()} {process::args()}\");"), ("bin/bye.lks", "outln('bye');")],
    );
    w.ok(&dir, &["install"]);
    let hello = w.home().join("bin/hello");
    let out = Command::new(&hello).arg("there").output().unwrap();
    assert_eq!(stdout(&out), "hi ['there']\n", "{}", stderr(&out));
    // The project was copied, so the installed command doesn't need its folder
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(stdout(&Command::new(w.home().join("bin/bye")).output().unwrap()), "bye\n");

    // Another project can't take a command's name
    let other = w.project(
        "other",
        "[project]\nauthor = \"you\"\nname = \"other\"\nversion = \"1.0.0\"\nkind = \"application\"\n",
        &[("bin/hello.lks", "")],
    );
    assert!(w.fails(&other, &["install"]).contains("already installed by the project me/app"));

    // Installing again drops the commands the project doesn't have anymore
    let dir = w.project("app", &app("\"local:acme/lib\" = \"1.0.0\"\n"), &[("bin/hello.lks", "outln('new');")]);
    w.ok(&dir, &["install"]);
    assert!(!w.home().join("bin/bye").exists());
    assert_eq!(stdout(&Command::new(&hello).output().unwrap()), "new\n");
}

#[test]
fn install_a_package_from_a_repository() {
    build_lks();
    let w = World::new();
    w.publish_kind("application", "acme", "tool", "1.0.0", "", &[("bin/tool.lks", "outln('tool 1');")]);
    w.ok(&w.dir, &["install", "local:acme/tool@1.0.0"]);
    assert_eq!(stdout(&Command::new(w.home().join("bin/tool")).output().unwrap()), "tool 1\n");
}

#[test]
fn the_example_project_works() {
    let w = World::new();
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sizes").canonicalize().unwrap();
    w.ok(&example, &["check"]);
    let out = w.ferry(&example, &["test"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("tests/units.lks\n          ok testBytesToText\n"), "{}", stderr(&out));
    assert!(stderr(&out).contains("Finished 4 passed, 0 failed"), "{}", stderr(&out));
    assert_eq!(w.ok(&example, &["run", "human", "1536", "1048576"]), "1.5 KB\n1 MB\n");
    assert_eq!(w.ok(&example, &["run", "bytes", "1.5 KB", "2 mb"]), "1536\n2097152\n");
    assert!(w.fails(&example, &["run", "bytes", "1 XB"]).contains("unknown unit 'XB'"));
    // Its help is a resource
    assert!(w.ok(&example, &["run", "bytes"]).starts_with("Usage: bytes <size>..."));
    // And a task of its own
    assert_eq!(w.ok(&example, &["task", "info"]), "folgue02/sizes 0.1.0 (application)\nCommands: bytes, human\nTasks: info\n");
}

#[test]
fn test_runs_the_tests_folder() {
    let w = World::new();
    let dir = w.project(
        "app",
        &app(""),
        &[
            ("src/me/app/math.lks", "pub number twice(number $n) = $n * 2;\nnumber hidden() = 1;\n"),
            ("tests/math.lks", "use me/app/math;\nvoid testTwice() {\n    assert math::twice(2) == 4;\n}\nvoid testWrong() {\n    outln('checking');\n    assert math::twice(2) == 5;\n}\n"),
            ("tests/more/other.lks", "void testOther() {\n    assert true;\n}\n"),
        ],
    );
    let out = w.ferry(&dir, &["--color", "never", "test"]);
    assert_eq!(out.status.code(), Some(1));
    let said = stderr(&out);
    assert!(said.starts_with("     Testing me/app\ntests/math.lks\n          ok testTwice\n        FAIL testWrong\n             tests/math.lks:7:5: assert failed: math::twice(2) == 5\n               left:  4\n               right: 5\n             It printed:\n               checking\ntests/more/other.lks\n          ok testOther\n"), "{said}");
    assert!(said.contains("    Finished 2 passed, 1 failed in "), "{said}");
    assert_eq!(stdout(&out), "");

    // A filter, also from a folder inside the project
    let out = w.ferry(&dir.join("tests/more"), &["--color", "never", "test", "Twice"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("tests/math.lks\n          ok testTwice\n    Finished 1 passed, 0 failed"), "{}", stderr(&out));

    // Tests only see what's `pub`, and `ferry check` checks them
    write(&dir.join("tests/hidden.lks"), "use me/app/math;\nvoid testHidden() {\n    assert math::hidden() == 1;\n}\n");
    let said = w.fails(&dir, &["--color", "never", "test", "Hidden"]);
    assert!(said.contains("has no `pub` function named `hidden`\n --> tests/hidden.lks:3:12"), "{said}");
    assert!(said.contains("error: 1 file of tests couldn't run"), "{said}");
    assert!(w.fails(&dir, &["--color", "never", "check"]).contains("tests/hidden.lks:3:12"));

    // No tests at all
    let empty = w.project("empty", &app(""), &[("bin/app.lks", "outln(1);")]);
    assert!(stderr(&w.ferry(&empty, &["test"])).contains("note: there are no tests"));
    assert!(w.fails(&dir, &["test", "a", "b"]).contains("one filter at most"));
}

#[test]
fn doc_makes_the_pages_of_a_project() {
    let w = World::new();
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "//! What the library does.\n\n/// Says hi.\npub string hi() = 'hi';\n\n/// A greeting.\npub class Greeting {\n    pub string @text = 'hi';\n}\n")]);
    let dir = w.project(
        "app",
        &app("\"local:acme/lib\" = \"1.0.0\"\n"),
        &[
            ("src/me/app/module.md", "The app, in **Markdown**.\n"),
            ("src/me/app/math.lks", "//! Numbers.\n\n/// Twice a number.\n///\n/// $n: the number\n/// returns: `$n * 2`\n/// throws ValueError: never\npub number twice(number $n) = $n * 2;\n\n/// A point.\npub class Point {\n    /// Across.\n    pub number @x;\n}\n\nnumber hidden() = 1;\n"),
            ("src/me/app/api/module.md", "The endpoints.\n"),
            ("src/me/app/api/users.lks", "//! Users.\n"),
            ("src/me/app/Shape.lks", "/// A shape.\npub enum Shape { CIRCLE, SQUARE }\n"),
            ("src/me/app/talk.lks", "use acme/lib/m;\n\n/// Its text.\npub string say(m::Greeting $g) = $g.text;\n"),
            ("bin/app.lks", "//! Runs the app.\n/// Greets people\ncommand Greet {\n    /// Who to greet\n    string @who;\n    /// Louder\n    flag(l) bool @loud;\n}\nconst $args = Greet::parse();\noutln($args.who);\n"),
            ("bin/plain.lks", "outln(1);\n"),
        ],
    );
    let said = stderr(&w.ferry(&dir, &["--color", "never", "doc"]));
    assert!(said.contains("Documented local:acme/lib 1.0.0"), "{said}");
    assert!(said.contains("Documented me/app in .ferry/doc/index.html"), "{said}");
    let doc = dir.join(".ferry/doc");
    let read = |name: &str| std::fs::read_to_string(doc.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let index = read("index.html");
    assert!(index.contains("The app, in <strong>Markdown</strong>."), "{index}");
    assert!(index.contains("href=\"math.html\"") && index.contains("href=\"folder.api.html\"") && index.contains("href=\"command.app.html\""));
    assert!(index.contains("href=\"deps/local/acme/lib/1.0.0/index.html\""), "the dependency links to its docs: {index}");
    let math = read("math.html");
    assert!(math.contains("<span class=\"n\">twice</span>") && math.contains("the number") && math.contains("ValueError"));
    assert!(math.contains("href=\"math.Point.html\""));
    assert!(!math.contains(">hidden<"), "only `pub` items");
    assert!(read("math.Point.html").contains("Across."));
    assert!(read("folder.api.html").contains("The endpoints."));
    let command = read("command.app.html");
    assert!(command.contains("app [options] &lt;who&gt;") && command.contains("-l, --loud") && command.contains("Who to greet."), "{command}");
    assert!(read("command.plain.html").contains("ferry run plain [arguments...]"));
    assert!(read("search.js").contains("\"p\":\"math::twice\""));
    // The types of the standard library link to its pages, and search finds them
    assert!(math.contains("href=\"std/types.number.html\""), "{math}");
    assert!(read("search.js").contains("\"g\":\"Standard library\",\"p\":\"fs::read\""));
    // And so do the dependencies' types and items
    let talk = read("talk.html");
    assert!(talk.contains("href=\"deps/local/acme/lib/1.0.0/m.Greeting.html\">m::Greeting</a>"), "{talk}");
    let search = read("search.js");
    assert!(search.contains("\"g\":\"acme/lib\",\"p\":\"m::hi\"") && search.contains("\"p\":\"m::Greeting.text\""), "{search}");
    assert!(search.contains("\"deps/local/acme/lib/1.0.0/m.html#"), "{search}");
    // The project's results come first
    assert!(search.find("\"p\":\"math::twice\"") < search.find("\"p\":\"m::hi\"") && search.find("\"p\":\"m::hi\"") < search.find("\"p\":\"fs::read\""));
    // Their pages are in the same folder, so it works wherever it's moved
    assert!(read("std/index.html").contains("std/fs") && read("deps/local/acme/lib/1.0.0/m.Greeting.html").contains("A greeting."));
    for page in ["index.html", "math.html", "talk.html", "search.js"] {
        assert!(!read(page).contains("file://"), "{page} has an absolute link");
    }
    // A type module is one page, its type's
    assert!(read("Shape.html").contains("use</span> me/app/Shape;") && read("Shape.html").contains("A shape."));
    assert!(!doc.join("Shape.Shape.html").exists() && read("search.js").contains("\"p\":\"Shape.CIRCLE\""));
    pages_are_well_formed(&doc);

    // One thing in the terminal
    let out = w.ok(&dir, &["doc", "math::twice"]);
    assert!(out.starts_with("pub number twice(number $n)") && out.contains("math.lks:8"), "{out}");
    assert!(out.contains("  $n       the number\n  returns  `$n * 2`\n  throws   ValueError never\n"), "{out}");
    assert!(w.ok(&dir, &["doc", "Point.x"]).contains("Across."));
    assert!(w.ok(&dir, &["doc", "math"]).starts_with("module me/app/math"));
    assert!(w.fails(&dir, &["doc", "nothing"]).contains("there's nothing named `nothing`"));
    // A dependency's items, and the dependency by its ID or without its version
    assert!(w.ok(&dir, &["doc", "m::hi"]).starts_with("pub string hi()"));
    assert!(w.ok(&dir, &["doc", "acme/lib"]).trim().ends_with("doc/local/acme/lib/1.0.0/index.html"));
    assert!(w.ok(&dir, &["doc", "local:acme/lib"]).trim().ends_with("doc/local/acme/lib/1.0.0/index.html"));

    // A package's docs, and a project with errors gets none
    let out = w.ok(&w.dir, &["doc", "local:acme/lib@1.0.0"]);
    assert!(out.trim().ends_with("doc/local/acme/lib/1.0.0/index.html"), "{out}");
    // An error in a test doesn't stop the docs, an error in a module does
    write(&dir.join("tests/math.lks"), "void testX() { const number $n = 'no'; }\n");
    w.ok(&dir, &["doc"]);
    write(&dir.join("src/me/app/math.lks"), "pub number twice(number $n) = 'no';\n");
    assert!(w.fails(&dir, &["doc"]).contains("has 1 error, so its docs weren't made"));
}

/// Every page of a folder of docs: no link inside another, which browsers
/// break apart, and no table of Markdown left as text.
fn pages_are_well_formed(dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "html") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let mut depth = 0;
        for (i, _) in text.match_indices('<') {
            if text[i..].starts_with("<a ") || text[i..].starts_with("<a>") {
                depth += 1;
                assert!(depth == 1, "a link inside a link in {}: {}", path.display(), &text[i.saturating_sub(200)..i + 50]);
            } else if text[i..].starts_with("</a>") {
                depth -= 1;
            }
        }
        assert!(!text.contains("|---") && !text.contains("| ---"), "a table of Markdown as text in {}", path.display());
    }
}

#[test]
fn doc_std_makes_the_pages_of_the_standard_library() {
    let w = World::new();
    let out = w.ok(&w.dir, &["doc", "std"]);
    let index = std::path::PathBuf::from(out.trim());
    assert!(index.ends_with(format!("doc/std/{}/index.html", env!("CARGO_PKG_VERSION"))), "{out}");
    let dir = index.parent().unwrap();
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(read("index.html").contains("Philosophy"));
    let fs = read("fs.html");
    assert!(fs.contains("<h3>Reading</h3>") && fs.contains("<span class=\"n\">read</span>") && fs.contains("use std/fs;"));
    assert!(read("http.Response.html").contains("<span class=\"n\">ok?</span>"));
    assert!(read("types.string.html").contains("<span class=\"n\">upper</span>"));
    assert!(!read("general.html").contains("use "), "the built-in functions need no `use`");
    assert!(read("search.js").contains("\"p\":\"http::get\""));
    // A table before the first heading of a note is read too
    assert!(read("env.html").contains("<span class=\"n\">get</span>"));
    // The front page: the modules before the long text, and their tiles are one sentence
    let index = read("index.html");
    assert!(index.find("<h2>Modules</h2>").unwrap() < index.find("Philosophy").unwrap());
    assert!(!index.contains("No dependencies"));
    pages_are_well_formed(dir);
    // In the terminal, also outside a project
    let out = w.ok(&w.dir, &["doc", "http::get"]);
    assert!(out.starts_with("Response get(string $url"), "{out}");
    assert!(w.ok(&w.dir, &["doc", "string.upper"]).contains("upper case"));
}

/// The data of the standard library ferry has inside is what `tools/docs/stdlib.py` makes from the notes.
#[test]
fn the_docs_of_the_standard_library_are_up_to_date() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fresh = std::env::temp_dir().join(format!("ferry-stdlib-{}.toml", std::process::id()));
    let made = Command::new("python3").arg(root.join("tools/docs/stdlib.py")).arg(&fresh).output();
    // Without Python there's nothing to compare with
    let Ok(made) = made else { return };
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let committed = std::fs::read_to_string(root.join("crates/ferry/src/stdlib.toml")).unwrap();
    let fresh_text = std::fs::read_to_string(&fresh).unwrap();
    let _ = std::fs::remove_file(&fresh);
    assert!(committed == fresh_text, "crates/ferry/src/stdlib.toml is out of date: run `python3 tools/docs/stdlib.py`");
}

#[test]
fn colors_only_when_asked() {
    let w = World::new();
    let dir = w.project("app", &app(""), &[("bin/app.lks", "outln(1);")]);
    let out = w.ferry(&dir, &["--color", "always", "check"]);
    assert!(stderr(&out).contains("\x1b[1;32m     Checked\x1b[0m me/app"), "{:?}", stderr(&out));
    let out = w.ferry(&dir, &["--color", "never", "add", "nowhere:a/b@1.0.0"]);
    assert!(stderr(&out).starts_with("error: there's no repository"), "{}", stderr(&out));
    let out = w.ferry(&dir, &["--color", "always", "add", "nowhere:a/b@1.0.0"]);
    assert!(stderr(&out).starts_with("\x1b[1;31merror:\x1b[0m there's no repository"));
    assert!(!stderr(&w.ferry(&dir, &["check"])).contains('\x1b'));
    assert_eq!(w.ferry(&dir, &["--color", "nope", "check"]).status.code(), Some(2));
}

/// The modules of the standard library written in Lakescript, and the page
/// of `ferry doc std` that documents each one.
const EMBEDDED: &[(&str, &str)] = &[
    ("archive.lks", "std/archive"),
    ("fs.lks", "std/fs"),
    ("html.lks", "std/html"),
    ("http.lks", "std/http"),
    ("process.lks", "std/process"),
    ("process/Platform.lks", "std/process"),
    ("terminal.lks", "std/terminal"),
    ("terminal/Style.lks", "std/terminal"),
    ("time.lks", "std/time"),
    ("time/zone.lks", "std/time"),
];

#[test]
fn every_pub_item_of_the_standard_library_is_documented() {
    use lakescript::docs::{ItemKind, module_doc};
    let site = ferry::doc::std_site();
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lakescript/src/stdlib");
    let mut missing = Vec::new();
    for (file, page) in EMBEDDED {
        let source = std::fs::read_to_string(stdlib.join(file)).unwrap();
        let module = module_doc(&source).unwrap_or_else(|e| panic!("{file}: {}", e.message));
        let page = site.modules.iter().find(|m| m.path == *page).unwrap_or_else(|| panic!("no page for {page}"));
        let groups: String = page.groups.iter().map(|(_, text)| text.as_str()).collect();
        let all_text = format!("{} {} {groups}", page.doc.text.clone().unwrap_or_default(), page.notes.clone().unwrap_or_default());
        for item in &module.items {
            match item.kind {
                // An enum's values are listed in the text
                ItemKind::Enum => {
                    if !all_text.contains(&format!("`{}`", item.name)) && !page.doc.items.iter().any(|i| i.name == item.name) {
                        missing.push(format!("{}: enum {}", page.path, item.name));
                    }
                }
                ItemKind::Class | ItemKind::Interface => {
                    let Some(class) = page.doc.items.iter().find(|i| i.name == item.name) else {
                        missing.push(format!("{}: class {}", page.path, item.name));
                        continue;
                    };
                    // The methods of interfaces are their operators *(i.e. `==` for `equals?`)*, told in the text
                    for member in item.members.iter().filter(|m| m.from.is_none()) {
                        if !class.members.iter().any(|m| m.name == member.name) {
                            missing.push(format!("{}: {}.{}", page.path, item.name, member.name));
                        }
                    }
                }
                _ => {
                    if !page.doc.items.iter().any(|i| i.name == item.name) {
                        missing.push(format!("{}: {}", page.path, item.name));
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "not in the notes of ideas/stdlib (then run tools/docs/stdlib.py):\n{}", missing.join("\n"));
}

#[test]
fn bundle_makes_one_file_for_each_command() {
    build_lks();
    let lks = Path::new(env!("CARGO_BIN_EXE_ferry")).with_file_name("lks");
    let w = World::new();
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "pub string hi() = 'hi';")]);
    let read = "use acme/lib/m;\nuse me/app/Greeter;\nuse std/process;\nuse std/resources;\noutln(f\"{m::hi()} {Greeter('x').greet()} {resources::read('texts/hello.txt').trim()} {resources::list()} {process::args()}\");";
    let dir = w.project(
        "app",
        &app("\"local:acme/lib\" = \"1.0.0\"\n"),
        &[
            ("bin/hello.lks", read),
            ("bin/bye.lks", "outln('bye');"),
            ("src/me/app/Greeter.lks", "pub class Greeter {\n    pub string @name;\n    pub string greet() = f\"hello {@name}\";\n}\n"),
            ("resources/texts/hello.txt", "from a resource\n"),
            // Hidden files are left out of bundles
            ("resources/.DS_Store", "junk"),
            ("resources/texts/.hidden.txt", "junk"),
            ("src/me/app/.scratch.lks", "this isn't Lakescript"),
            ("bin/.old.lks", "this isn't one either"),
        ],
    );
    // While developing, from the folder
    assert_eq!(w.ok(&dir, &["run", "hello", "a"]), "hi hello x from a resource ['texts'] ['a']\n");
    w.ok(&dir, &["bundle"]);
    let hello = dir.join("dist/hello.lkb");
    assert!(hello.is_file() && dir.join("dist/bye.lkb").is_file());
    // A hidden script of bin/ isn't a command
    assert!(!dir.join("dist/.old.lkb").exists());
    // The bundle has everything: it runs without the project or its dependencies
    let copy = w.dir.join("hello.lkb");
    std::fs::copy(&hello, &copy).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(w.home().join("repository")).unwrap();
    let out = Command::new(&lks).arg(&copy).arg("b").output().unwrap();
    assert_eq!(stdout(&out), "hi hello x from a resource ['texts'] ['b']\n", "{}", stderr(&out));
    // It runs as a program, with the lks on PATH
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&copy).unwrap().permissions().mode() & 0o111, 0o111);
        let path = format!("{}:{}", lks.parent().unwrap().display(), std::env::var("PATH").unwrap_or_default());
        let out = Command::new(&copy).arg("c").env("PATH", path).output().unwrap();
        assert_eq!(stdout(&out), "hi hello x from a resource ['texts'] ['c']\n", "{}", stderr(&out));
    }

    // Only one, and not with errors
    let dir = w.project("app", &app(""), &[("bin/one.lks", "outln(1);"), ("bin/two.lks", "outln(2);")]);
    w.ok(&dir, &["bundle", "two"]);
    assert!(dir.join("dist/two.lkb").is_file() && !dir.join("dist/one.lkb").exists());
    assert!(w.fails(&dir, &["bundle", "three"]).contains("has no command `three`"));
    write(&dir.join("bin/one.lks"), "const number $x = 'a';");
    assert!(w.fails(&dir, &["bundle"]).contains("so it wasn't bundled"));
    let lib = w.project("lib", "[project]\nauthor = \"me\"\nname = \"lib\"\nversion = \"1.0.0\"\nkind = \"library\"\n", &[]);
    assert!(w.fails(&lib, &["bundle"]).contains("no commands to install or bundle"));
}

#[test]
fn installed_commands_read_their_resources() {
    build_lks();
    let w = World::new();
    let dir = w.project("app", &app(""), &[("bin/show.lks", "use std/resources;\noutln(resources::read('a.txt'));"), ("resources/a.txt", "kept")]);
    w.ok(&dir, &["install"]);
    // From the installed copy, which has resources/ too
    std::fs::remove_dir_all(&dir).unwrap();
    let out = Command::new(w.home().join("bin/show")).output().unwrap();
    assert_eq!(stdout(&out), "kept\n", "{}", stderr(&out));
}

#[test]
fn ranges_update_remove_and_uninstall() {
    let w = World::new();
    for version in ["1.0.0", "1.1.0", "1.2.3", "2.0.0"] {
        w.publish("acme", "lib", version, "", &[("src/acme/lib/m.lks", &format!("pub string v() = '{version}';"))]);
    }
    let dir = w.project("app", &app(""), &[("bin/app.lks", "use acme/lib/m;\noutln(m::v());")]);
    // A range takes the newest version that fits, and the lock keeps it
    let said = stderr(&w.ferry(&dir, &["--color", "never", "add", "local:acme/lib@^1.0.0"]));
    let manifest = std::fs::read_to_string(dir.join("ferry.toml")).unwrap();
    assert!(manifest.contains("\"local:acme/lib\" = \"^1.0.0\""), "{manifest}");
    // A range needs 0.13.0 to be read, so ferry says the project needs it
    assert!(manifest.contains("kind = \"application\"\nlks = \"0.13.0\"\n"), "{manifest}");
    assert!(said.contains("Set lks = \"0.13.0\" in ferry.toml: version ranges need Lakescript 0.13.0 or newer"), "{said}");
    assert_eq!(w.ok(&dir, &["run"]), "1.2.3\n");
    w.publish("acme", "lib", "1.3.0", "", &[("src/acme/lib/m.lks", "pub string v() = '1.3.0';")]);
    assert_eq!(w.ok(&dir, &["run"]), "1.2.3\n", "the lock file keeps the version");
    // Until `ferry update`
    assert!(stderr(&w.ferry(&dir, &["--color", "never", "update"])).contains("Updated local:acme/lib from 1.2.3 to 1.3.0"));
    assert_eq!(w.ok(&dir, &["run"]), "1.3.0\n");
    assert!(stderr(&w.ferry(&dir, &["update", "acme/lib"])).contains("note: everything is at the newest version"));
    assert!(w.fails(&dir, &["update", "acme/other"]).contains("acme/other isn't a dependency of me/app: they're local:acme/lib"));
    // `~`, and nothing that fits
    w.ok(&dir, &["add", "local:acme/lib@~1.1.0"]);
    assert_eq!(w.ok(&dir, &["run"]), "1.1.0\n");
    assert!(w.fails(&dir, &["add", "local:acme/lib@^3.0.0"]).contains("has no version that's ^3.0.0. Its versions are: 1.0.0, 1.1.0, 1.2.3, 1.3.0, 2.0.0"));
    // Removing it takes it out of both files
    assert!(stderr(&w.ferry(&dir, &["--color", "never", "remove", "local:acme/lib"])).contains("Removed local:acme/lib 1.1.0"));
    assert!(!std::fs::read_to_string(dir.join("ferry.toml")).unwrap().contains("acme"));
    assert!(!std::fs::read_to_string(dir.join("ferry.lock")).unwrap_or_default().contains("acme"));
    assert!(w.fails(&dir, &["remove", "acme/lib"]).contains("it has none"));
}

#[test]
fn uninstall_removes_the_commands_of_a_project() {
    build_lks();
    let w = World::new();
    let tool = w.project(
        "tool",
        "[project]\nauthor = \"me\"\nname = \"tool\"\nversion = \"1.0.0\"\nkind = \"application\"\n",
        &[("bin/tool.lks", "outln('tool');"), ("bin/other.lks", "outln('other');")],
    );
    let app_dir = w.project("app", &app(""), &[("bin/app.lks", "outln('app');")]);
    w.ok(&tool, &["install"]);
    w.ok(&app_dir, &["install"]);
    let bin = w.home().join("bin");
    assert!(bin.join("tool").exists() && bin.join("other").exists());
    assert!(stderr(&w.ferry(&tool, &["--color", "never", "uninstall"])).contains("Uninstalled me/tool: other, tool"));
    assert!(!bin.join("tool").exists() && !bin.join("other").exists());
    // Another project's commands stay
    assert!(bin.join("app").exists());
    assert!(w.fails(&tool, &["uninstall", "me/tool"]).contains("me/tool isn't installed"));
    w.ok(&w.dir, &["uninstall", "me/app"]);
    assert!(!bin.join("app").exists());
}

#[test]
fn a_project_can_need_a_newer_lakescript_and_its_docs() {
    let w = World::new();
    let manifest = "[project]\nauthor = \"me\"\nname = \"lib\"\nversion = \"1.0.0\"\nkind = \"library\"\nlks = \"99.0.0\"\n\n[check]\ndocs = true\n";
    let dir = w.project(
        "lib",
        manifest,
        &[("src/me/lib/m.lks", "/// Said.\npub number a() = 1;\npub number b() = 2;\n/// A point.\npub class P {\n    pub number @x;\n    /// Said.\n    pub number @y;\n}\nnumber hidden() = 3;\n")],
    );
    let said = stderr(&w.ferry(&dir, &["--color", "never", "check"]));
    assert!(said.contains(&format!("warning: me/lib needs Lakescript 99.0.0 or newer, and this is ferry {}", env!("CARGO_PKG_VERSION"))), "{said}");
    assert!(said.contains("warning: `m::b` is `pub`, and has no `///` comment\n --> src/me/lib/m.lks:3"), "{said}");
    assert!(said.contains("`m::P.x` is `pub`"), "{said}");
    assert!(!said.contains("`m::a`") && !said.contains("`m::P.y`") && !said.contains("hidden"), "{said}");
    // Without `[check]`, nothing about docs; and an older one is fine
    write(&dir.join("ferry.toml"), "[project]\nauthor = \"me\"\nname = \"lib\"\nversion = \"1.0.0\"\nkind = \"library\"\nlks = \"0.1.0\"\n");
    let said = stderr(&w.ferry(&dir, &["check"]));
    assert!(!said.contains("warning"), "{said}");
    write(&dir.join("ferry.toml"), "[project]\nauthor = \"me\"\nname = \"lib\"\nversion = \"1.0.0\"\nkind = \"library\"\nlks = \"^1.0\"\n");
    assert!(w.fails(&dir, &["check"]).contains("the version of Lakescript it needs, `lks`, `^1.0` must be an exact version"));
    // A file this one can't read, from a newer one, says which it needs before what's wrong
    write(&dir.join("ferry.toml"), "[project]\nauthor = \"me\"\nname = \"lib\"\nversion = \"1.0.0\"\nkind = \"library\"\nlks = \"99.0.0\"\n\n[build]\nfast = true\n");
    let said = w.fails(&dir, &["check"]);
    assert!(said.contains(&format!("ferry.toml needs Lakescript 99.0.0 or newer, and this is {}, which can't read it: ", env!("CARGO_PKG_VERSION"))), "{said}");
    assert!(said.contains("unknown field `build`"), "{said}");
}

#[test]
fn a_dependency_can_need_a_newer_lakescript() {
    let w = World::new();
    // `lks` in its [project], after `kind`
    w.publish_kind("library\"\nlks = \"99.0.0", "acme", "new", "1.0.0", "", &[("src/acme/new/m.lks", "pub string v() = 'new';")]);
    w.publish("acme", "mid", "1.0.0", "\"local:acme/new\" = \"1.0.0\"\n", &[("src/acme/mid/m.lks", "use acme/new/m as n;\npub string v() = n::v();")]);
    let dir = w.project("app", &app(""), &[("bin/app.lks", "use acme/mid/m;\noutln(m::v());")]);
    let warning = format!(
        "warning: local:acme/new 1.0.0, for local:acme/mid 1.0.0, needs Lakescript 99.0.0 or newer, and this is ferry {}: it may use what this one doesn't have",
        env!("CARGO_PKG_VERSION")
    );
    // When it's added, and every command after
    let said = stderr(&w.ferry(&dir, &["--color", "never", "add", "local:acme/mid@1.0.0"]));
    assert!(said.contains(&warning), "{said}");
    let run = w.ferry(&dir, &["--color", "never", "run"]);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "new\n");
    assert!(stderr(&run).contains(&warning), "{}", stderr(&run));
    for args in [&["--color", "never", "check"][..], &["--color", "never", "update"][..]] {
        let said = stderr(&w.ferry(&dir, args));
        assert!(said.contains(&warning), "{args:?}: {said}");
    }
    // Only the one that needs it
    assert!(!stderr(&w.ferry(&dir, &["--color", "never", "check"])).contains("acme/mid 1.0.0, for"));
}

#[test]
fn test_files_run_at_once_and_show_in_order() {
    let w = World::new();
    let files: Vec<(String, String)> = (0..6).map(|i| (format!("tests/t{i}.lks"), format!("void testN{i}() {{ assert {i} >= 0; }}\n"))).collect();
    let files: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let dir = w.project("app", &app(""), &files);
    for args in [&["--color", "never", "test"][..], &["--color", "never", "test", "--jobs", "1"][..], &["--color", "never", "test", "--jobs", "3", "N"][..]] {
        let said = stderr(&w.ferry(&dir, args));
        let at: Vec<usize> = (0..6).map(|i| said.find(&format!("testN{i}")).unwrap_or_else(|| panic!("{said}"))).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{said}");
        assert!(said.contains("6 passed"), "{said}");
    }
    assert_eq!(w.ferry(&dir, &["test", "--jobs", "0"]).status.code(), Some(2));
}

#[test]
fn task_runs_the_scripts_of_the_project() {
    let w = World::new();
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "pub string hi() = 'hi';")]);
    let release = "use std/tooling/ferry;\nuse std/process;\nuse acme/lib/m;\n\
        const $p = ferry::project();\n\
        outln([$p.id, $p.version, $p.commands(), $p.tasks(), process::args(), m::hi()]);\n\
        for ($d in $p.dependencies) outln([$d.package, $d.wanted, $d.version]);\n";
    let dir = w.project(
        "app",
        &app("\"local:acme/lib\" = \"^1.0.0\"\n"),
        &[("bin/app.lks", ""), ("scripts/release.lks", release), ("scripts/.hidden.lks", "")],
    );
    let out = w.ok(&dir, &["task", "release", "1.2.0"]);
    assert_eq!(out, "['me/app', '0.1.0', ['app'], ['release'], ['1.2.0'], 'hi']\n['local:acme/lib', '^1.0.0', '1.0.0']\n", "{out}");
    // From a folder inside the project, too
    std::fs::create_dir_all(dir.join("src/me/app")).unwrap();
    assert!(w.ok(&dir.join("src/me/app"), &["task", "release"]).starts_with("['me/app'"));
    assert!(w.fails(&dir, &["task"]).contains("they're release"));
    assert!(w.fails(&dir, &["task", "nope"]).contains("has no task `nope`, the ones there are: release"));
    // `ferry check` checks them
    write(&dir.join("scripts/broken.lks"), "const number $n = 'x';\n");
    assert!(w.fails(&dir, &["--color", "never", "check"]).contains("--> scripts/broken.lks:1:19"));
}

#[test]
fn ranges_that_dont_fit_together() {
    let w = World::new();
    for version in ["1.0.0", "1.1.0", "2.0.0"] {
        w.publish("acme", "base", version, "", &[("src/acme/base/b.lks", "pub number n() = 1;")]);
    }
    // A package that needs base 2, and the project that needs base 1
    w.publish("acme", "top", "1.0.0", "\"local:acme/base\" = \"^2.0.0\"\n", &[("src/acme/top/t.lks", "pub number n() = 1;")]);
    let dir = w.project("app", &app("\"local:acme/base\" = \"^1.0.0\"\n\"local:acme/top\" = \"1.0.0\"\n"), &[("bin/app.lks", "")]);
    let said = w.fails(&dir, &["check"]);
    assert!(said.contains("local:acme/base is needed in two versions that don't fit together: ^1.0.0 by the project me/app, which got 1.1.0, and ^2.0.0 by local:acme/top 1.0.0"), "{said}");
    // When they fit, the one chosen first is shared
    w.publish("acme", "mid", "1.0.0", "\"local:acme/base\" = \"~1.1.0\"\n", &[("src/acme/mid/m.lks", "pub number n() = 1;")]);
    let dir = w.project("app2", &app("\"local:acme/base\" = \"^1.0.0\"\n\"local:acme/mid\" = \"1.0.0\"\n"), &[("bin/app.lks", "")]);
    w.ok(&dir, &["check"]);
    let lock = std::fs::read_to_string(dir.join("ferry.lock")).unwrap();
    assert!(lock.contains("name = \"local:acme/base\"\nversion = \"1.1.0\""), "{lock}");
}
