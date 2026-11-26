//! Phase 13, past the examples: the failures of `beforeEach`, `afterEach`
//! and `test::tempDir()`, and the errors and corners of `std/format/toml`,
//! `std/crypto`, `.env` files, `withoutAccents()` and `std/tooling/ferry`.

use std::path::{Path, PathBuf};

use lakescript::{CapturedOutput, Engine};

/// A folder that's removed at the end of the test.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lks-phase13-{name}-{}", std::process::id()));
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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

fn error(source: &str) -> String {
    match Engine::with_output(CapturedOutput::new()).run(source) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.message,
    }
}

fn run_file(path: &Path) -> Result<String, String> {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.run_file(path).map(|_| output.text()).map_err(|e| e.message)
}

// ---- beforeEach, afterEach and test::tempDir() ----

#[test]
fn after_each_runs_also_when_a_test_fails() {
    let dir = TempDir::new("hooks");
    let log = dir.0.join("log.txt");
    let code = format!(
        "use std/fs;\nuse std/test;\n\
         let string $made = '';\n\
         void beforeEach() {{ $made = test::tempDir(); }}\n\
         void afterEach() {{ fs::append('{log}', f\"after {{$made}}\\n\"); }}\n\
         void testFails() {{ assert false; }}\n\
         void testThrows() {{ throw ValueError('boom'); }}\n\
         void testPasses() {{}}\n",
        log = log.display()
    );
    let file = dir.file("t.lks", &code);
    let mut engine = Engine::with_output(CapturedOutput::new());
    let results = engine.run_tests(&file, None).unwrap();
    let passed: Vec<(String, bool)> = results.iter().map(|r| (r.name.clone(), r.passed())).collect();
    assert_eq!(passed, [("testFails".to_string(), false), ("testThrows".to_string(), false), ("testPasses".to_string(), true)]);
    // `afterEach` ran after each of the three, and each folder is gone, also after a failure
    let lines: Vec<String> = std::fs::read_to_string(&log).unwrap().lines().map(String::from).collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    for line in &lines {
        let folder = line.strip_prefix("after ").unwrap();
        assert!(!Path::new(folder).exists(), "{folder} is still there");
    }
}

#[test]
fn a_failing_before_each_fails_the_test_without_running_it() {
    let dir = TempDir::new("before");
    let log = dir.0.join("log.txt");
    let code = format!(
        "use std/fs;\n\
         void beforeEach() {{ throw ValueError('no setup'); }}\n\
         void afterEach() {{ fs::append('{log}', 'after\\n'); }}\n\
         void testA() {{ fs::append('{log}', 'ran\\n'); }}\n\
         void testB() {{ fs::append('{log}', 'ran\\n'); }}\n",
        log = log.display()
    );
    let file = dir.file("t.lks", &code);
    let results = Engine::with_output(CapturedOutput::new()).run_tests(&file, None).unwrap();
    assert!(results.iter().all(|r| !r.passed()) && results.len() == 2);
    assert!(results[0].failure.as_ref().unwrap().message.contains("no setup"));
    // The tests never ran, and `afterEach` still did, once for each
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "after\nafter\n");
}

#[test]
fn an_error_in_after_each_fails_a_test_that_passed() {
    let dir = TempDir::new("after");
    let file = dir.file("t.lks", "void afterEach() { assert 1 == 2; }\nvoid testA() {}\n");
    let results = Engine::with_output(CapturedOutput::new()).run_tests(&file, None).unwrap();
    assert!(!results[0].passed());
    assert!(results[0].failure.as_ref().unwrap().message.contains("1 == 2"));
}

#[test]
fn a_temp_dir_outside_a_test_is_removed_when_the_script_ends() {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.run("use std/test;\nuse std/fs;\nconst $d = test::tempDir();\noutln($d);\noutln(fs::dir?($d));").unwrap();
    let text = output.text();
    let folder = text.lines().next().unwrap().to_string();
    assert_eq!(text.lines().nth(1), Some("true"));
    assert!(Path::new(&folder).exists());
    drop(engine);
    assert!(!Path::new(&folder).exists(), "{folder} is still there");
}

// ---- std/format/toml ----

#[test]
fn toml_errors_say_where() {
    let message = error("use std/format/toml;\ntoml::parse(\"a = 1\\nb = \\nc = 3\");");
    assert!(message.starts_with("ValueError: invalid TOML at line 2, column 5:"), "{message}");
    assert!(error("use std/format/toml;\ntoml::parse('a = 1\\na = 2');").contains("line 2"));
    // A document is a table
    assert_eq!(error("use std/format/toml;\ntoml::stringify([1, 2]);"), "TypeError: a TOML document is a table: a map or an object, not a list");
    assert_eq!(error("use std/format/toml;\ntoml::stringify('a');"), "TypeError: a TOML document is a table: a map or an object, not a string");
    assert_eq!(error("use std/format/toml;\ntoml::stringify({'xs' => [1, null]});"), "TypeError: `$.xs[1]` is `null`, which TOML can't have");
    // Into a type, with where the value was
    let code = "use std/format/toml;\nclass S {\n    pub number @port;\n}\ntoml::parse(\"port = 'x'\", S);";
    assert!(error(code).contains("`$.port` is a string, not a number"), "{}", error(code));
}

#[test]
fn toml_values() {
    let code = "use std/format/toml;\n\
        const $d = toml::parse(\"when = 1979-05-27T07:32:00Z\\nday = 1979-05-27\\nf = 1.5\\nn = -3\\nok = true\\nnested = { a = [1, 'x'] }\");\n\
        outln([$d['when'], $d['day'], $d['f'], $d['n'], $d['ok'], $d['nested']]);\n\
        outln($d['when'].is(string));";
    assert_eq!(run(code), "['1979-05-27T07:32:00Z', '1979-05-27', 1.5, -3, true, { 'a' => [1, 'x'] }]\ntrue\n");
    // An enum by its name, a range as an array, and the order of the keys kept
    let code = "use std/format/toml;\nenum Level { LOW, HIGH }\n\
        outln(toml::stringify({'z' => 1, 'a' => Level::HIGH, 'r' => 1..4}));";
    assert_eq!(run(code), "z = 1\na = \"HIGH\"\nr = [1, 2, 3]\n\n");
}

// ---- std/crypto ----

#[test]
fn hashes_of_every_algorithm() {
    let code = "use std/crypto/hash;\noutln(hash::sha512(''));\noutln(hash::sha256(%b\"\\x00\\xff\"));";
    assert_eq!(
        run(code),
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e\n\
         06eb7d6a69ee19e5fbdf749018d3d2abfa04bcbd1365db312eb86dc7169389b8\n"
    );
    let dir = TempDir::new("hash");
    let file = dir.file("f.txt", "abc");
    let code = format!(
        "use std/crypto/hash;\nconst $f = '{}';\n\
         outln([hash::file($f, 'sha1') == hash::sha1('abc'), hash::file($f, 'md5') == hash::md5('abc'), hash::file($f, 'crc32') == hash::crc32('abc'), hash::file($f, 'sha512') == hash::sha512('abc')]);",
        file.display()
    );
    assert_eq!(run(&code), "[true, true, true, true]\n");
    assert!(error(&format!("use std/crypto/hash;\nhash::file('{}', 'sha3');", file.display())).contains("'sha3' isn't a hash there is"));
    assert!(error("use std/crypto/hash;\nhash::file('/nowhere/at/all');").contains("/nowhere/at/all"));
    assert!(error("use std/crypto/hash;\nhash::sha256(5);").contains("takes a string or bytes, not a number"));
}

#[test]
fn hmac_with_sha512_and_prefixes() {
    // RFC 4231, test case 2
    let code = "use std/crypto/hash;\noutln(hash::hmac('Jefe', 'what do ya want for nothing?', 'sha512'));";
    assert_eq!(run(code), "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737\n");
    let code = "use std/crypto/hash;\nconst $s = hash::hmac('k', 'body', 'sha512');\n\
        outln([hash::verifyHmac('k', 'body', $s, 'sha512'), hash::verifyHmac('k', 'body', 'sha512=' + $s, 'sha512'), hash::verifyHmac('k', 'body', $s), hash::verifyHmac('k', 'body', '')]);";
    assert_eq!(run(code), "[true, true, false, false]\n");
    assert!(error("use std/crypto/hash;\nhash::hmac('k', 'd', 'md5');").contains("'md5' isn't a hash HMAC can use here"));
}

#[test]
fn secure_random_values() {
    let code = "use std/crypto/random;\n\
        const $seen = <number>[];\nfor ($i in 0..300) $seen.push(random::between(-1, 1));\n\
        outln($seen.unique().sorted());\noutln(random::between(7, 7));\n\
        outln([random::token(3).length(), random::token(1).length(), random::hex(1).length(), random::bytes(1024).length()]);\n\
        outln(random::token(48) -match %r\"^[A-Za-z0-9_-]+$\");";
    assert_eq!(run(code), "[-1, 0, 1]\n7\n[4, 2, 2, 1024]\ntrue\n");
    assert!(error("use std/crypto/random;\nrandom::between(2, 1);").contains("at most `$max`, but got 2 and 1"));
    assert!(error("use std/crypto/random;\nrandom::bytes(0);").contains("takes from 1 to 1024 bytes, not 0"));
    assert!(error("use std/crypto/random;\nrandom::token(1025);").contains("not 1025"));
}

#[test]
fn sealed_bytes_that_were_changed_dont_open() {
    let code = "use std/crypto/seal;\nuse std/bytes;\n\
        const $key = seal::key();\nconst $sealed = seal::seal(%b\"\\x00\\x01\", $key);\n\
        outln([$key.length(), $sealed.length(), seal::seal('x', $key).equals(seal::seal('x', $key))]);\n\
        outln(seal::open($sealed, $key).hex());\n\
        // The last byte changed\n\
        const $hex = $sealed.hex();\n\
        const $changed = bytes::fromHex($hex[0..$hex.length() - 2] + (if ($hex.endsWith('00')) '01' else '00'));\n\
        try { seal::open($changed, $key); } catch (ValueError $e) { outln($e.message()); }";
    assert_eq!(
        run(code),
        "[32, 43, false]\n0001\nthese bytes can't be opened: the key is another one, or they were changed after they were sealed\n"
    );
    assert_eq!(error("use std/crypto/seal;\nseal::open(%b\"\\x02abc\", seal::key());"), "ValueError: these bytes weren't made by `seal::seal()`");
    assert_eq!(error("use std/crypto/seal;\nseal::seal('x', %b\"short\");"), "ValueError: a key is 32 bytes, from `seal::key()`, and this one is 5");
    // The checker says it first, and without it, when it runs
    assert!(error("use std/crypto/seal;\nseal::open('text', seal::key());").contains("bytes"));
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_type_check(false);
    let message = engine.run("use std/crypto/seal;\nseal::open('text', seal::key());").unwrap_err().message;
    assert!(message.contains("takes the bytes `seal::seal()` gave, not a string"), "{message}");
}

#[test]
fn a_password_hash_that_isnt_one() {
    assert_eq!(error("use std/crypto/password;\npassword::verify('pw', 'plain text');"), "ValueError: that isn't a hash from `password::hash()`: it starts with `$argon2`");
    assert_eq!(run("use std/crypto/password;\nconst $h = password::hash('pw');\noutln([$h == password::hash('pw'), password::verify('', $h)]);"), "[false, false]\n");
}

// ---- .env files ----

#[test]
fn env_load_reads_dot_env_from_the_current_folder() {
    let dir = TempDir::new("dotenv");
    dir.file(".env", "FROM_FILE=yes\n");
    let code = format!("use std/env;\nuse std/process;\nprocess::chdir('{}');\nenv::load();\noutln(env::get('FROM_FILE'));", dir.0.display());
    assert_eq!(run(&code), "yes\n");
    let message = error(&format!("use std/env;\nenv::load('{}/missing.env');", dir.0.display()));
    assert!(message.contains("missing.env"), "{message}");
    let message = error(&format!("use std/env;\nenv::load('{}');", dir.file("bad.env", "A=1\nB\n").display()));
    assert!(message.ends_with("bad.env': line 2 isn't `NAME=value`: it has no `=`"), "{message}");
}

// ---- withoutAccents() ----

#[test]
fn without_accents() {
    assert_eq!(
        run("outln(['Ørsted Łódź Đakovo Ħamrun'.withoutAccents(), 'straße Æsir œuvre'.withoutAccents(), 'naïve café Ångström'.withoutAccents(), 'ABC 123'.withoutAccents()]);"),
        "['Orsted Lodz Dakovo Hamrun', 'straße Æsir œuvre', 'naive cafe Angstrom', 'ABC 123']\n"
    );
}

// ---- std/tooling/ferry ----

#[test]
fn ferry_project_outside_a_project_and_from_a_folder() {
    let dir = TempDir::new("tooling");
    dir.file("app/ferry.toml", "[project]\nauthor = \"some-one\"\nname = \"my.tool\"\nversion = \"2.0.0\"\nkind = \"application\"\nlks = \"0.13.0\"\n");
    dir.file("app/bin/run.lks", "");
    dir.file("app/src/some_one/my_tool/deep/x.lks", "");
    let script = dir.file(
        "s.lks",
        &format!(
            "use std/tooling/ferry;\nconst $p = ferry::project('{}/app/src/some_one/my_tool/deep');\noutln([$p.id, $p.version, $p.lks, $p.dependencies, $p.commands(), $p.tasks(), $p.dir.endsWith('/app')]);",
            dir.0.display()
        ),
    );
    assert_eq!(run_file(&script).unwrap(), "['some_one/my_tool', '2.0.0', '0.13.0', [], ['run'], [], true]\n");
    let outside = dir.file("o.lks", "use std/tooling/ferry;\nferry::project('/');");
    let message = run_file(&outside).unwrap_err();
    assert_eq!(message, "FileNotFound: there's no ferry.toml in / or in the folders above it");
}
