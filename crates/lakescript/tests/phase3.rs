//! Phase 3: modules, commands, `exit`, and `std/process`, `std/env`,
//! `std/io`, `std/fs`, `std/path` and `std/json`.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use lakescript::{CapturedOutput, Engine, Outcome, Value};

/// A folder that's removed at the end of the test.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lks-phase3-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path.canonicalize().unwrap())
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn path(&self) -> String {
        self.0.display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn engine() -> (Engine, CapturedOutput) {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_type_check(false);
    (engine, output)
}

fn run(source: &str) -> String {
    let (mut engine, output) = engine();
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("test.lks"), output.text());
    }
    output.text()
}

fn run_in(dir: &TempDir, source: &str) -> String {
    let script = dir.file("main.lks", source);
    let (mut engine, output) = engine();
    if let Err(e) = engine.run_file(&script) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("main.lks"), output.text());
    }
    output.text()
}

fn error_in(dir: &TempDir, source: &str) -> String {
    let script = dir.file("main.lks", source);
    let (mut engine, _) = engine();
    match engine.run_file(&script) {
        Ok(_) => panic!("expected an error:\n{source}"),
        Err(e) => e.render("main.lks"),
    }
}

fn error(source: &str) -> String {
    let (mut engine, _) = engine();
    match engine.run(source) {
        Ok(_) => panic!("expected an error:\n{source}"),
        Err(e) => e.render("test.lks"),
    }
}

// ---- Modules ----

#[test]
fn modules_next_to_the_script() {
    let dir = TempDir::new("modules");
    dir.file(
        "helpers.lks",
        "pub const string $greeting = 'Hello';\n\
         pub string greet(string $name) = f\"{$greeting} {$name}\";\n\
         string secret() = 'hidden';\n",
    );
    dir.file("text/case.lks", "pub string shout(string $s) = $s.upper() + '!';\n");
    let out = run_in(
        &dir,
        "use helpers;\n\
         use helpers::{greet, $greeting as $hi};\n\
         use text/case as c;\n\
         outln(helpers::greet('Ana'));\n\
         outln(greet('Bo'));\n\
         outln($hi);\n\
         outln(helpers::$greeting);\n\
         outln(c::shout('hey'));\n",
    );
    assert_eq!(out, "Hello Ana\nHello Bo\nHello\nHello\nHEY!\n");
}

#[test]
fn only_pub_things_can_be_used() {
    let dir = TempDir::new("private");
    dir.file("helpers.lks", "string secret() = 'hidden';\n");
    assert!(error_in(&dir, "use helpers;\nhelpers::secret();").contains("has no `pub` function named `secret`"));
    assert!(error_in(&dir, "use helpers::secret;").contains("has no `pub` function or type named `secret`"));
}

#[test]
fn modules_load_once_and_only_when_used() {
    let dir = TempDir::new("once");
    dir.file("counter.lks", "pub const number $value = tick();\n");
    dir.file("other.lks", "use counter;\npub number read() = counter::$value;\n");
    let script = dir.file(
        "main.lks",
        "use counter;\nuse other;\noutln(ticks());\noutln(other::read());\noutln(counter::$value);\noutln(ticks());\n",
    );
    let (mut engine, output) = engine();
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    engine.register("tick", move |_, _| {
        c.set(c.get() + 1);
        Ok(Value::Int(c.get()))
    });
    let c = count.clone();
    engine.register("ticks", move |_, _| Ok(Value::Int(c.get())));
    engine.run_file(&script).unwrap();
    // Nothing is loaded by `use`, and `counter` is only loaded once for both files
    assert_eq!(output.text(), "0\n1\n1\n1\n");
}

#[test]
fn modules_only_have_declarations() {
    let dir = TempDir::new("decl");
    dir.file("noisy.lks", "outln('loaded');\n");
    let err = error_in(&dir, "use noisy::{x};");
    assert!(err.contains("can only have declarations"), "{err}");
    assert!(err.contains("noisy.lks:1:1"), "the error points to the module: {err}");
}

#[test]
fn errors_in_modules_name_their_file() {
    let dir = TempDir::new("errfile");
    dir.file("broken.lks", "pub number half(number $n) = $n % 0;\n");
    let err = error_in(&dir, "use broken;\nbroken::half(4);");
    assert!(err.contains("broken.lks:1:"), "{err}");
    assert!(err.contains("ArithmeticError"), "{err}");
}

#[test]
fn type_errors_name_the_module_they_are_in() {
    let dir = TempDir::new("errfile-deep");
    dir.file("inner.lks", "pub number twice(number $n) {\n    return 'two';\n}\n");
    dir.file("outer.lks", "use inner;\npub number four() = inner::twice(2);\n");
    let script = dir.file("main.lks", "use outer;\noutln(outer::four());");
    let mut engine = Engine::with_output(CapturedOutput::new());
    let err = engine.check_file(&script).unwrap_err().render("main.lks");
    // Two modules deep: the module the error is in, not the one that uses it
    assert!(err.contains("inner.lks:2:"), "{err}");
}

#[test]
fn constants_that_need_each_other_are_an_error() {
    let dir = TempDir::new("cycle");
    dir.file("a.lks", "use b;\npub const number $x = b::$y;\n");
    dir.file("b.lks", "use a;\npub const number $y = a::$x;\n");
    assert!(error_in(&dir, "use a;\noutln(a::$x);").contains("isn't ready yet: that module is still loading"));
}

#[test]
fn modules_can_use_each_other() {
    let dir = TempDir::new("mutual");
    dir.file("a.lks", "use b;\npub number one() = 1;\npub number three() = b::two() + 1;\n");
    dir.file("b.lks", "use a;\npub number two() = a::one() + 1;\n");
    assert_eq!(run_in(&dir, "use a;\noutln(a::three());"), "3\n");
    let path = dir.file("main.lks", "use a;\nuse b;\noutln([a::three(), b::two()]);\n");
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run_file(&path) {
        panic!("{}", e.render("main.lks"));
    }
    assert_eq!(output.text(), "[3, 2]\n");
}

#[test]
fn classes_in_modules_can_use_each_other() {
    let dir = TempDir::new("classcycle");
    dir.file("Order.lks", "use Customer;\npub class Order {\n    pub string @item;\n    pub Customer @customer;\n    pub string label() = f\"{@item} for {@customer.name}\";\n}\n");
    dir.file(
        "Customer.lks",
        "use Order;\npub class Customer {\n    pub string @name;\n    list<Order> @orders = [];\n    pub Order order(string $item) {\n        const $o = Order($item, @);\n        @orders.push($o);\n        return $o;\n    }\n    pub number count() = @orders.length();\n}\n",
    );
    let path = dir.file("main.lks", "use Customer;\nconst $ana = Customer('Ana');\noutln($ana.order('book').label());\n$ana.order('pen');\noutln($ana.count());\n");
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run_file(&path) {
        panic!("{}", e.render("main.lks"));
    }
    assert_eq!(output.text(), "book for Ana\n2\n");
}

#[test]
fn a_module_named_like_a_type_holds_that_type() {
    let dir = TempDir::new("typemodules");
    dir.file("people/Person.lks", "pub class Person {\n    pub string @name;\n}\nstring helper() = 'x';\n");
    dir.file("people/Thing.lks", "pub class Thing {}\npub void extra() {}\n");
    dir.file("people/Named.lks", "pub class Wrong {}\n");
    let path = dir.file("main.lks", "use people/Person;\nuse people/Person as P;\noutln(P('ana').is(Person));\n");
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run_file(&path) {
        panic!("{}", e.render("main.lks"));
    }
    assert_eq!(output.text(), "true\n");
    let check = |source: &str| {
        let bad = dir.file("bad.lks", source);
        Engine::with_output(CapturedOutput::new()).run_file(&bad).unwrap_err().message
    };
    assert!(check("use people/Thing;").contains("the only thing it can make `pub`, and `extra` is `pub` too"));
    assert!(check("use people/Named;").contains("must declare `pub class Named`"));
    assert!(check("use people/Person::{Person};").contains("nothing goes after `::`"));
    assert!(check("use People/Person;").contains("folders are written in lowercase"));
    assert!(check("use people/person;").contains("must match the name of the file exactly"));
    dir.file("people/per_son.lks", "pub void x() {}\n");
    assert!(check("use people/Person;").contains("have the same name in another case"));
}

#[test]
fn use_goes_at_the_top() {
    assert!(error("outln(1);\nuse std/fs;").contains("go at the top"));
    assert!(error("if (true) { use std/fs; }").contains("go at the top"));
    assert!(error("use nothing/here;").contains("module `nothing/here` not found"));
    assert!(error("use std/nope;").contains("no module `std/nope`"));
}

#[test]
fn host_can_leave_modules_out() {
    let (mut engine, _) = engine();
    engine.disable_module("std/process");
    let err = engine.run("use std/process;").unwrap_err();
    assert!(err.message.contains("isn't available here"), "{}", err.message);
}

// ---- Output, input and exit ----

#[test]
fn err_and_errln_write_to_stderr() {
    let (mut engine, output) = engine();
    let errors = CapturedOutput::new();
    engine.set_error_output(errors.clone());
    engine.run("out('a'); err('b'); errln('c'); outln('d');").unwrap();
    assert_eq!(output.text(), "ad\n");
    assert_eq!(errors.text(), "bc\n");
}

#[test]
fn exit_stops_the_script_and_runs_finally() {
    let (mut engine, output) = engine();
    let outcome = engine.run("try { outln('before'); exit(3); outln('never'); } finally { outln('finally'); }").unwrap();
    assert_eq!(outcome, Outcome::Exit(3));
    assert_eq!(output.text(), "before\nfinally\n");
    assert_eq!(engine.run("outln('done');").unwrap(), Outcome::Finished);
}

#[test]
fn hosts_can_forbid_exit() {
    let (mut engine, output) = engine();
    engine.set_exit_allowed(false);
    engine.run("try { exit(1); } catch ($e) { outln($e.message()); }").unwrap();
    assert_eq!(output.text(), "`exit` isn't allowed here\n");
}

#[test]
fn reading_stdin() {
    let (mut engine, output) = engine();
    engine.set_input(std::io::Cursor::new("first\nsecond\r\nthird\n"), true);
    engine.run("use std/io;\noutln(io::readLine());\noutln(io::piped?());\noutln(io::lines());\noutln(io::readLine());").unwrap();
    assert_eq!(output.text(), "first\ntrue\n['second', 'third']\nnull\n");
}

// ---- std/process and std/env ----

#[test]
fn process() {
    let (mut engine, output) = engine();
    engine.set_args(vec!["a b".into(), "--x".into()]);
    engine.run("use std/process;\noutln(process::args());\noutln(process::platform().is(symbol));\noutln(process::pid() > 0);").unwrap();
    assert_eq!(output.text(), "['a b', '--x']\ntrue\ntrue\n");
    assert_eq!(run("use std/process;\noutln(process::which('sh') != null);\noutln(process::which('no-such-program-lks'));"), "true\nnull\n");
}

#[test]
fn chdir_only_changes_the_script() {
    let dir = TempDir::new("chdir");
    dir.file("sub/note.txt", "hi");
    let before = std::env::current_dir().unwrap();
    let out = run(&format!(
        "use std/process;\nuse std/fs;\nprocess::chdir('{}');\nprocess::chdir('sub');\noutln(fs::read('note.txt'));\noutln(-f 'note.txt');\noutln(process::cwd().endsWith('/sub'));",
        dir.path()
    ));
    assert_eq!(out, "hi\ntrue\ntrue\n");
    assert_eq!(std::env::current_dir().unwrap(), before, "the real process's folder didn't change");
}

#[test]
fn environment_variables_stay_in_the_script() {
    let out = run(
        "use std/env;\n\
         env::set('LKS_TEST_VAR', 'yes');\n\
         outln(env::get('LKS_TEST_VAR'));\n\
         outln(env::has('LKS_TEST_VAR'));\n\
         outln(%x\"sh -c 'echo $LKS_TEST_VAR'\".output());\n\
         env::remove('LKS_TEST_VAR');\n\
         outln(env::get('LKS_TEST_VAR') ?? 'unset');\n\
         outln(env::all().is(map<string, string>));\n",
    );
    assert_eq!(out, "yes\ntrue\nyes\nunset\ntrue\n");
    assert!(std::env::var("LKS_TEST_VAR").is_err(), "the real environment didn't change");
    assert!(error("use std/env;\nenv::require('LKS_SURELY_NOT_SET');").contains("ValueError: the environment variable `LKS_SURELY_NOT_SET` isn't set"));
}

// ---- Commands ----

#[test]
fn commands() {
    let out = run(
        "const $file = 'my notes; rm -rf ~.txt';\n\
         outln(%x\"echo {$file}\".output());\n\
         const $words = ['a b', 'c'];\n\
         outln(%x\"printf '%s|' {$words}\".output());\n\
         outln(%x\"printf 'x\\ny\\n'\".lines());\n\
         outln(%x\"sh -c 'exit 3'\".status());\n\
         outln(%x\"true\".succeeds?());\n\
         outln(%x\"false\".succeeds?());\n\
         outln(%x\"tr a-z A-Z\".input('shout').output());\n\
         outln(%x\"sh -c 'echo $GREETING'\".env('GREETING', 'hola').output());\n\
         outln(%x\"echo {$file}\");\n\
         %x\"echo from run\".run();\n",
    );
    assert_eq!(
        out,
        "my notes; rm -rf ~.txt\na b|c|\n['x', 'y']\n3\ntrue\nfalse\nSHOUT\nhola\necho 'my notes; rm -rf ~.txt'\nfrom run\n"
    );
}

#[test]
fn commands_run_without_a_shell() {
    let out = run(
        "const $text = 'a | b; $HOME *';\n\
         outln(%x\"printf '[%s]' {$text}\".output());\n\
         outln(%x\"printf '[%s]' a|b $HOME *\".output());\n\
         outln(%x\"printf '[%s]' 'two words' \\\"and more\\\"\".output());\n\
         const $name = 'out';\n\
         outln(%x\"printf '[%s]' --file={$name}.txt\".output());\n\
         outln(%x\"printf '[%s]' C:\\Users\".output());\n",
    );
    assert_eq!(out, "[a | b; $HOME *]\n[a|b][$HOME][*]\n[two words][and more]\n[--file=out.txt]\n[C:\\Users]\n");
    assert!(error("%x\"printf 'unclosed\".run();").contains("a `'` in the command is never closed"));
    assert!(error("const $l = ['a', 'b'];\n%x\"echo x{$l}\".run();").contains("can't be next to other text"));
}

#[test]
fn commands_can_pipe_and_use_files() {
    let dir = TempDir::new("pipes");
    let file = format!("{}/out.txt", dir.path());
    let script = "outln(%x\"printf 'b\\na\\nc\\n'\".pipe(%x\"sort\").lines());\n\
         outln(%x\"printf 'x\\ny\\n'\".pipe(%x\"sort -r\").pipe(%x\"head -1\").output());\n\
         %x\"printf 'one\\n'\".writeTo('FILE').run();\n\
         %x\"printf 'two\\n'\".appendTo('FILE').run();\n\
         outln(%x\"sort -r\".readFrom('FILE').lines());\n\
         outln(%x\"printf a\".pipe(%x\"tr a-z A-Z\").writeTo('up.txt'));\n\
         outln(%x\"false\".pipe(%x\"cat\").status());\n"
        .replace("FILE", &file);
    let out = run(&script);
    assert_eq!(out, "['a', 'b', 'c']\ny\n['two', 'one']\nprintf a | tr a-z A-Z > up.txt\n1\n");
    assert!(error("%x\"printf a\".writeTo('/tmp/lks-never.txt').output();").contains("so `.output()` has nothing to return"));
}

#[test]
fn a_shell_is_used_only_when_asked() {
    let out = run("use std/process;\noutln(process::shell('echo a; echo b | tr a-z A-Z').lines());\noutln(process::shell('exit 3').status());\n");
    assert_eq!(out, "['a', 'B']\n3\n");
}

#[test]
fn failing_commands_throw() {
    let err = error("%x\"sh -c 'exit 4'\".run();");
    assert!(err.contains("CommandFailed: `sh -c 'exit 4'` failed with exit code 4"), "{err}");
    assert_eq!(run("outln((try %x\"false\".output()).orElse('fallback'));"), "fallback\n");
    let err = error("%x\"sleep 5\".timeout(0.1).run();");
    assert!(err.contains("didn't finish in 0.1 seconds"), "{err}");
}

#[test]
fn commands_without_a_shell() {
    let dir = TempDir::new("direct");
    dir.file("a.txt", "");
    let out = run(&format!(
        "use std/process;\n\
         outln(process::command('echo', 'no $expansion', '*').output());\n\
         outln(process::command('ls').cwd('{}').output());\n\
         outln(process::command('echo', 'a b'));\n",
        dir.path()
    ));
    assert_eq!(out, "no $expansion *\na.txt\necho 'a b'\n");
    assert!(error("use std/process;\nprocess::command('lks-no-such-program').run();").contains("the program wasn't found"));
}

// ---- std/path ----

#[test]
fn paths() {
    let out = run(
        "use std/path;\n\
         outln(path::join('/var', 'log', 'web.log'));\n\
         outln(path::join('a/', '/b'));\n\
         outln(path::parent('/a/b/c.txt'));\n\
         outln(path::parent('c.txt'));\n\
         outln(path::name('/a/b/c.txt'));\n\
         outln(path::stem('/a/archive.tar.gz'));\n\
         outln(path::extension('a.tar.gz'));\n\
         outln(path::extension('.bashrc'));\n\
         outln(path::withExtension('dir/a.txt', 'md'));\n\
         outln(path::withName('/a/b.txt', 'c.txt'));\n\
         outln(path::parts('/a/b'));\n\
         outln(path::normalize('a/./b/../c'));\n\
         outln(path::normalize('../x/..'));\n\
         outln(path::relative('/a/b/c', '/a'));\n\
         outln(path::relative('/a/x', '/a/b/c'));\n\
         outln(path::absolute?('/etc') && !path::absolute?('etc'));\n\
         outln(path::absolute('x').startsWith('/'));\n\
         outln(path::expandHome('~/notes') == path::home() + '/notes');\n\
         outln(path::$separator);\n",
    );
    assert_eq!(
        out,
        "/var/log/web.log\n/b\n/a/b\nnull\nc.txt\narchive.tar\ngz\nnull\ndir/a.md\n/a/c.txt\n['/', 'a', 'b']\na/c\n..\nb/c\n../../x\ntrue\ntrue\ntrue\n/\n"
    );
}

// ---- std/fs ----

#[test]
fn files() {
    let dir = TempDir::new("fs");
    let script = format!(
        "use std/fs;\nuse std/path;\n\
         const $d = '{}';\n\
         fs::write(path::join($d, 'a.txt'), 'one\\ntwo\\n');\n\
         fs::append(path::join($d, 'a.txt'), 'three\\n');\n\
         outln(fs::readLines(path::join($d, 'a.txt')));\n\
         outln(fs::size(path::join($d, 'a.txt')));\n\
         fs::makeDir(path::join($d, 'src/util'));\n\
         fs::writeLines(path::join($d, 'src/main.lks'), ['x']);\n\
         fs::write(path::join($d, 'src/util/text.lks'), '');\n\
         fs::write(path::join($d, 'src/.hidden.lks'), '');\n\
         outln(fs::list($d));\n\
         outln(fs::walk(path::join($d, 'src')).map(($p) -> path::relative($p, $d)));\n\
         process();\n\
         void process() {{}}\n",
        dir.path()
    );
    // Functions must be defined before they're used
    assert!(error(&script).contains("no function named `process`"));

    let out = run(&format!(
        "use std/fs;\nuse std/path;\nuse std/process;\n\
         process::chdir('{}');\n\
         fs::write('a.txt', 'one\\ntwo\\n');\n\
         fs::append('a.txt', 'three\\n');\n\
         outln(fs::readLines('a.txt'));\n\
         outln(fs::size('a.txt'));\n\
         fs::makeDir('src/util');\n\
         fs::writeLines('src/main.lks', ['x']);\n\
         fs::write('src/util/text.lks', '');\n\
         fs::write('src/.hidden.lks', '');\n\
         outln(fs::list());\n\
         outln(fs::walk('src'));\n\
         outln(fs::glob('src/**/*.lks'));\n\
         outln(fs::glob('*.txt'));\n\
         fs::copy('src', 'copy');\n\
         outln(fs::file?('copy/util/text.lks'));\n\
         fs::move('a.txt', 'b.txt');\n\
         outln(fs::exists?('a.txt') || !fs::exists?('b.txt'));\n\
         fs::remove('b.txt');\n\
         fs::remove('b.txt');\n\
         fs::removeAll('copy');\n\
         outln(fs::list());\n\
         fs::setMode('src/main.lks', 0o755);\n\
         outln(fs::mode('src/main.lks').toString(8));\n\
         outln(fs::modified('src/main.lks') > 1_700_000_000);\n\
         const $t = fs::tempFile('.json');\n\
         outln($t.endsWith('.json') && fs::file?($t));\n\
         fs::remove($t);\n",
        dir.path()
    ));
    assert_eq!(
        out,
        "['one', 'two', 'three']\n14\n['a.txt', 'src']\n['src/.hidden.lks', 'src/main.lks', 'src/util/text.lks']\n['src/main.lks', 'src/util/text.lks']\n['a.txt']\ntrue\nfalse\n['src']\n755\ntrue\ntrue\n"
    );
}

#[test]
fn file_errors_have_types() {
    let cases = [
        ("fs::read('/surely/not/here.txt');", "FileNotFound"),
        ("fs::remove('/');", "IOError"),
    ];
    for (code, kind) in cases {
        let out = run(&format!("use std/fs;\ntry {{ {code} }} catch (IOError $e) {{ outln($e.typeName()); }}"));
        assert_eq!(out.trim(), kind, "{code}");
    }
    let dir = TempDir::new("fserr");
    dir.file("full/x.txt", "");
    let err = error(&format!("use std/fs;\nfs::remove('{}/full');", dir.path()));
    assert!(err.contains("isn't empty. Use fs::removeAll()"), "{err}");
}

// ---- std/json ----

#[test]
fn json() {
    let out = run(
        "use std/json;\n\
         const $data = json::parse('{\"name\": \"Daniel\", \"tags\": [\"a\", \"b\"], \"age\": 23, \"ratio\": 0.5, \"ok\": true, \"none\": null, \"esc\": \"\\\\u00f1\\\\n\"}');\n\
         outln($data['name'].as(string).upper());\n\
         outln($data['tags'].is(list<string>));\n\
         outln($data['age'] + 1);\n\
         outln($data['none']);\n\
         outln($data['esc'].length());\n\
         outln(json::stringify($data));\n\
         outln(json::stringify({:k => [1, 2..4], 5 => 'x'}, indent: 2));\n\
         outln(json::stringify([]));\n",
    );
    assert_eq!(
        out,
        "DANIEL\ntrue\n24\nnull\n2\n{\"name\":\"Daniel\",\"tags\":[\"a\",\"b\"],\"age\":23,\"ratio\":0.5,\"ok\":true,\"none\":null,\"esc\":\"ñ\\n\"}\n{\n  \"k\": [\n    1,\n    [\n      2,\n      3\n    ]\n  ],\n  \"5\": \"x\"\n}\n[]\n"
    );
}

#[test]
fn json_errors() {
    assert!(error("use std/json;\njson::parse('{\"a\": 1,}');").contains("ValueError: invalid JSON at line 1, column 9"));
    assert!(error("use std/json;\njson::parse('[1, 2');").contains("invalid JSON"));
    assert!(error("use std/json;\njson::stringify(0 / 0);").contains("TypeError: JSON can't hold NaN"));
    assert!(error("use std/json;\njson::stringify(%r\"x\");").contains("can't be written as JSON"));
    assert!(error("use std/json;\nlet $l = []; $l << $l;\njson::stringify($l);").contains("contains itself"));
}

#[test]
fn json_files() {
    let dir = TempDir::new("json");
    let out = run(&format!(
        "use std/json;\nuse std/fs;\n\
         const $p = '{}/config.json';\n\
         json::write($p, {{'port' => 8080}});\n\
         outln(fs::read($p));\n\
         outln(json::read($p)['port']);\n\
         json::write($p, [1], indent: null);\n\
         outln(fs::read($p));\n",
        dir.path()
    ));
    assert_eq!(out, "{\n  \"port\": 8080\n}\n\n8080\n[1]\n\n");
}

#[test]
fn json_streams() {
    let out = run(
        "use std/json;\n\
         outln(json::parseAll('{\"a\": 1} {\"a\": 2}'));\n\
         outln(json::parseAll(\"1\\n[2, 3]\\n\\\"x\\\"\\n\"));\n\
         outln(json::parseAll('  '));\n\
         outln(json::parseAll('{\"a\":\\n 1}\\n{}'));\n",
    );
    assert_eq!(out, "[{ 'a' => 1 }, { 'a' => 2 }]\n[1, [2, 3], 'x']\n[]\n[{ 'a' => 1 }, {}]\n");
    assert!(error("use std/json;\njson::parseAll('{} {');").contains("invalid JSON at line 1"));
}

#[test]
fn modules_can_keep_private_variables() {
    let dir = TempDir::new("modlet");
    dir.file("counter.lks", "let $count = 0;\npub number next() {\n  $count += 1;\n  return $count;\n}\n");
    let script = "use counter;\ncounter::next();\noutln(counter::next());\n";
    assert_eq!(run_in(&dir, script), "2\n");
    let path = dir.file("main.lks", script);
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run_file(&path) {
        panic!("{}", e.render("main.lks"));
    }
    assert_eq!(output.text(), "2\n");
    // Other modules can't reach them
    let bad = dir.file("bad.lks", "use counter::{$count};\n");
    let message = Engine::with_output(CapturedOutput::new()).run_file(&bad).unwrap_err().message;
    assert!(message.contains("no `pub` constant named `$count`"), "{message}");
    dir.file("public.lks", "pub let $x = 1;\n");
    let bad = dir.file("bad2.lks", "use public;\n");
    let message = Engine::with_output(CapturedOutput::new()).run_file(&bad).unwrap_err().message;
    assert!(message.contains("`pub` goes before functions, constants"), "{message}");
    dir.file("untyped.lks", "pub const $x = 1;\n");
    let bad = dir.file("bad3.lks", "use untyped;\n");
    let message = Engine::with_output(CapturedOutput::new()).run_file(&bad).unwrap_err().message;
    assert!(message.contains("`pub const $x` needs its type written"), "{message}");
}
