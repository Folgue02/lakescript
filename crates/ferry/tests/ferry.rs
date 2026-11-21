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
    w.ok(&work, &["new", "some-user/my.tool"]);
    let project = w.work("my.tool");
    assert!(project.join("src/some_user/my_tool/greeting.lks").is_file());
    assert_eq!(w.ok(&project, &["run"]), "Hello, world!\n");
    assert_eq!(w.ok(&project, &["run", "Ana"]), "Hello, Ana!\n");
    assert_eq!(w.ok(&project, &["run", "my.tool", "Bo"]), "Hello, Bo!\n");
    w.ok(&project, &["check"]);
    // A library has no commands
    w.ok(&work, &["new", "me/lib", "--lib"]);
    assert!(w.fails(&w.work("lib"), &["run"]).contains("has no commands"));
    assert!(w.fails(&work, &["new", "me/lib"]).contains("already exists"));
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
    assert!(w.fails(&dir, &["add", "local:acme/lib@^1.4"]).contains("must be an exact version"));
    assert!(w.fails(&dir, &["add", "local:acme/lib"]).contains("needs a version"));
    assert!(w.fails(&dir, &["add", "nowhere:acme/lib@1.0.0"]).contains("there's no repository named `nowhere`"));
    assert!(w.fails(&dir, &["add", "local:acme/missing@1.0.0"]).contains("can't download"));
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "")]);
    assert!(w.fails(&dir, &["add", "local:acme/lib@1.2.0"]).contains("has no version 1.2.0"));
    // Nothing was added
    assert!(!std::fs::read_to_string(dir.join("ferry.toml")).unwrap().contains("acme"));
    assert!(w.fails(&dir, &["add", "local:9acme/lib@1.0.0"]).contains("can't start with a digit"));
    assert_eq!(w.ferry(&dir, &["nope"]).status.code(), Some(2));
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
    w.publish("acme", "lib", "1.0.0", "", &[("src/acme/lib/m.lks", "//! What the library does.\n\n/// Says hi.\npub string hi() = 'hi';\n")]);
    let dir = w.project(
        "app",
        &app("\"local:acme/lib\" = \"1.0.0\"\n"),
        &[
            ("src/me/app/module.md", "The app, in **Markdown**.\n"),
            ("src/me/app/math.lks", "//! Numbers.\n\n/// Twice a number.\n///\n/// $n: the number\n/// returns: `$n * 2`\n/// throws ValueError: never\npub number twice(number $n) = $n * 2;\n\n/// A point.\npub class Point {\n    /// Across.\n    pub number @x;\n}\n\nnumber hidden() = 1;\n"),
            ("src/me/app/api/module.md", "The endpoints.\n"),
            ("src/me/app/api/users.lks", "//! Users.\n"),
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
    assert!(index.contains("local:acme/lib") && index.contains("file://"), "the dependency links to its docs");
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

    // One thing in the terminal
    let out = w.ok(&dir, &["doc", "math::twice"]);
    assert!(out.starts_with("pub number twice(number $n)") && out.contains("math.lks:8"), "{out}");
    assert!(out.contains("  $n       the number\n  returns  `$n * 2`\n  throws   ValueError never\n"), "{out}");
    assert!(w.ok(&dir, &["doc", "Point.x"]).contains("Across."));
    assert!(w.ok(&dir, &["doc", "math"]).starts_with("module me/app/math"));
    assert!(w.fails(&dir, &["doc", "nothing"]).contains("there's nothing named `nothing`"));

    // A package's docs, and a project with errors gets none
    let out = w.ok(&w.dir, &["doc", "local:acme/lib@1.0.0"]);
    assert!(out.trim().ends_with("doc/local/acme/lib/1.0.0/index.html"), "{out}");
    write(&dir.join("src/me/app/math.lks"), "pub number twice(number $n) = 'no';\n");
    assert!(w.fails(&dir, &["doc"]).contains("has 1 error, so its docs weren't made"));
}

#[test]
fn doc_std_has_the_page_inside() {
    let w = World::new();
    let out = w.ok(&w.dir, &["doc", "std"]);
    let page = std::path::PathBuf::from(out.trim());
    assert!(page.ends_with(format!("doc/std/{}/index.html", env!("CARGO_PKG_VERSION"))), "{out}");
    assert!(std::fs::read_to_string(&page).unwrap().contains("Lakescript Standard Library"));
}

/// The page ferry has inside is the one `tools/docs/stdlib.py` makes from the notes.
#[test]
fn the_page_of_the_standard_library_is_up_to_date() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fresh = std::env::temp_dir().join(format!("ferry-stdlib-{}.html", std::process::id()));
    let made = Command::new("python3").arg(root.join("tools/docs/stdlib.py")).arg(&fresh).output();
    // Without Python there's nothing to compare with
    let Ok(made) = made else { return };
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let committed = std::fs::read_to_string(root.join("crates/ferry/src/stdlib.html")).unwrap();
    let fresh_text = std::fs::read_to_string(&fresh).unwrap();
    let _ = std::fs::remove_file(&fresh);
    assert!(committed == fresh_text, "crates/ferry/src/stdlib.html is out of date: run `python3 tools/docs/stdlib.py`");
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
