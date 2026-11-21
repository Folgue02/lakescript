//! Talks to the real `lks-lsp` like an editor does: messages with a
//! `Content-Length` header on its stdin and stdout.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Server {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: i64,
}

impl Server {
    fn start() -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lks-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("lks-lsp starts");
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut server = Server { child, input, output, next_id: 1 };
        server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
        server.notify("initialized", json!({}));
        server
    }

    fn send(&mut self, message: Value) {
        let text = message.to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{text}", text.len()).unwrap();
        self.input.flush().unwrap();
    }

    fn receive(&mut self) -> Value {
        let mut length = 0;
        loop {
            let mut line = String::new();
            self.output.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(n) = line.strip_prefix("Content-Length: ") {
                length = n.parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        self.output.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    /// Sends a request and waits for its response.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let message = self.receive();
            if message["id"] == id {
                return message;
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// The next errors sent for a file.
    fn diagnostics_for(&mut self, path: &Path) -> Vec<Value> {
        let uri = uri(path);
        loop {
            let message = self.receive();
            if message["method"] == "textDocument/publishDiagnostics" && message["params"]["uri"] == uri {
                return message["params"]["diagnostics"].as_array().unwrap().clone();
            }
        }
    }

    fn open(&mut self, path: &Path, text: &str) {
        self.notify("textDocument/didOpen", json!({ "textDocument": { "uri": uri(path), "languageId": "lakescript", "version": 1, "text": text } }));
    }

    fn change(&mut self, path: &Path, text: &str, version: i64) {
        self.notify("textDocument/didChange", json!({ "textDocument": { "uri": uri(path), "version": version }, "contentChanges": [{ "text": text }] }));
    }

    fn at(&mut self, method: &str, path: &Path, line: u32, character: u32) -> Value {
        let response = self.request(method, json!({ "textDocument": { "uri": uri(path) }, "position": { "line": line, "character": character } }));
        assert!(response["error"].is_null(), "{response}");
        response["result"].clone()
    }

    fn stop(mut self) {
        let response = self.request("shutdown", Value::Null);
        assert!(response["error"].is_null(), "{response}");
        self.notify("exit", Value::Null);
        assert!(self.child.wait().unwrap().success());
    }
}

fn uri(path: &Path) -> String {
    format!("file://{}", path.display()).replace(' ', "%20")
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lks-lsp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn errors_follow_what_is_typed() {
    let dir = temp("script");
    let script = dir.join("main.lks");
    std::fs::write(&script, "outln(1);\n").unwrap();
    let mut server = Server::start();
    // Not saved: the editor's text is what's checked
    server.open(&script, "const $x = 1;\nstring $s = $x;\n");
    let errors = server.diagnostics_for(&script);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0]["message"].as_str().unwrap().contains("`$s` is a string"), "{errors:?}");
    assert_eq!(errors[0]["range"]["start"], json!({ "line": 1, "character": 12 }));
    assert_eq!(errors[0]["severity"], 1);
    // Fixed: the error goes away
    server.change(&script, "const $x = 1;\nnumber $n = $x;\n", 2);
    assert!(server.diagnostics_for(&script).is_empty());
    server.stop();
}

#[test]
fn errors_go_to_the_module_they_are_in() {
    let dir = temp("project");
    std::fs::write(dir.join("ferry.toml"), "[project]\nname = \"demo\"\nauthor = \"ana\"\nversion = \"0.1.0\"\nkind = \"application\"\n").unwrap();
    std::fs::create_dir_all(dir.join("src/ana/demo")).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let module = dir.join("src/ana/demo/util.lks");
    let script = dir.join("bin/demo.lks");
    std::fs::write(&module, "pub number twice(number $n) = $n * 2;\n").unwrap();
    std::fs::write(&script, "use ana/demo/util;\noutln(util::twice(2));\n").unwrap();
    let mut server = Server::start();
    server.open(&script, "use ana/demo/util;\noutln(util::twice(2));\n");
    assert!(server.diagnostics_for(&script).is_empty());
    // The module is changed in the editor, without saving: the error is in the module
    server.open(&module, "pub number twice(number $n) = 'two';\n");
    let errors = server.diagnostics_for(&module);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0]["message"].as_str().unwrap().contains("returns a number"), "{errors:?}");
    // It's checked as a module of the project, where only declarations go
    server.change(&module, "pub number twice(number $n) = $n * 2;\noutln('loaded');\n", 2);
    let errors = server.diagnostics_for(&module);
    assert!(errors.iter().any(|e| e["message"].as_str().unwrap().contains("only have declarations")), "{errors:?}");
    server.stop();
}

#[test]
fn it_answers_requests_it_does_not_know_and_shuts_down() {
    let mut server = Server::start();
    let response = server.request("textDocument/references", json!({ "textDocument": { "uri": "file:///x.lks" }, "position": { "line": 0, "character": 0 } }));
    assert!(!response["error"].is_null(), "{response}");
    server.stop();
}

#[test]
fn hover_shows_the_type_and_the_comment() {
    let dir = temp("hover");
    let script = dir.join("main.lks");
    let mut server = Server::start();
    let text = "/// Says hello\nstring greet(string $name) = f\"hi {$name}\";\nlet $words = ['a'];\noutln(greet($words.first() ?? 'ñ'));\n";
    server.open(&script, text);
    assert!(server.diagnostics_for(&script).is_empty());
    // On `greet`, and right after it
    for character in [6, 11] {
        let hover = server.at("textDocument/hover", &script, 3, character);
        let value = hover["contents"]["value"].as_str().unwrap();
        assert_eq!(value, "```lakescript\nstring greet(string $name)\n```\n\nSays hello", "{hover}");
        assert_eq!(hover["range"], json!({ "start": { "line": 3, "character": 6 }, "end": { "line": 3, "character": 11 } }));
    }
    let hover = server.at("textDocument/hover", &script, 3, 14);
    assert!(hover["contents"]["value"].as_str().unwrap().contains("list<string> $words"), "{hover}");
    let hover = server.at("textDocument/hover", &script, 3, 21);
    assert!(hover["contents"]["value"].as_str().unwrap().contains("A method of `list`."), "{hover}");
    // Nothing there
    assert!(!server.at("textDocument/hover", &script, 3, 0).is_null());
    assert!(server.at("textDocument/hover", &script, 3, 32).is_null());
    server.stop();
}

#[test]
fn definitions_are_found_in_the_file_and_in_modules() {
    let dir = temp("definition");
    std::fs::write(dir.join("ferry.toml"), "[project]\nname = \"demo\"\nauthor = \"ana\"\nversion = \"0.1.0\"\nkind = \"application\"\n").unwrap();
    std::fs::create_dir_all(dir.join("src/ana/demo")).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let module = dir.join("src/ana/demo/Shape.lks");
    let script = dir.join("bin/demo.lks");
    std::fs::write(&module, "/// A shape\npub class Shape {\n    pub number @side;\n\n    pub number area() = @side * @side;\n}\n").unwrap();
    let text = "use ana/demo/Shape;\nconst $square = Shape(2);\noutln($square.area());\n";
    std::fs::write(&script, text).unwrap();
    let mut server = Server::start();
    server.open(&script, text);
    assert!(server.diagnostics_for(&script).is_empty());
    // A variable of the file
    let found = server.at("textDocument/definition", &script, 2, 8);
    assert_eq!(found["uri"], uri(&script));
    assert_eq!(found["range"], json!({ "start": { "line": 1, "character": 6 }, "end": { "line": 1, "character": 13 } }));
    // A method of a class in a module
    let found = server.at("textDocument/definition", &script, 2, 15);
    assert_eq!(found["uri"], uri(&module));
    assert_eq!(found["range"]["start"], json!({ "line": 4, "character": 15 }));
    let hover = server.at("textDocument/hover", &script, 1, 17);
    assert_eq!(hover["contents"]["value"], "```lakescript\nclass Shape\n```\n\nA shape");
    // In the module itself, `@side` goes to the field
    server.open(&module, &std::fs::read_to_string(&module).unwrap());
    let found = server.at("textDocument/definition", &module, 4, 25);
    assert_eq!(found["range"]["start"], json!({ "line": 2, "character": 15 }));
    server.stop();
}

/// The labels of a completion answer.
fn labels(result: &Value) -> Vec<String> {
    result.as_array().unwrap().iter().map(|i| i["label"].as_str().unwrap().to_string()).collect()
}

#[test]
fn completion_while_typing() {
    let dir = temp("completion");
    let script = dir.join("main.lks");
    let mut server = Server::start();
    let class = "class Point {\n    pub number @x;\n    pub static Point origin() = Point(0);\n}\n";
    server.open(&script, &format!("{class}let $words = ['a'];\n"));
    server.diagnostics_for(&script);
    // `$words.`, not saved, with no `;` yet
    server.change(&script, &format!("{class}let $words = ['a'];\n$words.\n"), 2);
    let found = server.at("textDocument/completion", &script, 5, 7);
    let names = labels(&found);
    assert!(names.contains(&"push".to_string()) && names.contains(&"map".to_string()), "{names:?}");
    let push = found.as_array().unwrap().iter().find(|i| i["label"] == "push").unwrap();
    assert_eq!(push["detail"], "list<string> push(...string $values)");
    assert_eq!(push["textEdit"]["range"]["start"], json!({ "line": 5, "character": 7 }));
    // `$` alone: the variables, replacing the `$`
    server.change(&script, &format!("{class}let $words = ['a'];\noutln($);\n"), 3);
    let found = server.at("textDocument/completion", &script, 5, 7);
    assert_eq!(labels(&found), vec!["$words"]);
    assert_eq!(found[0]["textEdit"]["range"]["start"], json!({ "line": 5, "character": 6 }));
    // `Point::o`
    server.change(&script, &format!("{class}let $p = Point::o\n"), 4);
    let found = server.at("textDocument/completion", &script, 4, 17);
    assert_eq!(labels(&found), vec!["origin"]);
    // A word: names in scope and keywords
    server.change(&script, &format!("{class}ret\n"), 5);
    let names = labels(&server.at("textDocument/completion", &script, 4, 3));
    for expected in ["Point", "outln", "return", "string"] {
        assert!(names.contains(&expected.to_string()), "{expected}: {names:?}");
    }
    server.stop();
}

#[test]
fn completion_in_a_module_of_a_project() {
    let dir = temp("completion-project");
    std::fs::write(dir.join("ferry.toml"), "[project]\nname = \"demo\"\nauthor = \"ana\"\nversion = \"0.1.0\"\nkind = \"application\"\n").unwrap();
    std::fs::create_dir_all(dir.join("src/ana/demo")).unwrap();
    std::fs::write(dir.join("src/ana/demo/util.lks"), "pub number twice(number $n) = $n * 2;\n").unwrap();
    let module = dir.join("src/ana/demo/report.lks");
    let text = "use ana/demo/util;\n\npub string report(list<number> $values) {\n    let $total = $values.sum();\n    return f\"{util::}\";\n}\n";
    std::fs::write(&module, text).unwrap();
    let mut server = Server::start();
    server.open(&module, text);
    server.diagnostics_for(&module);
    let found = server.at("textDocument/completion", &module, 4, 20);
    assert_eq!(labels(&found), vec!["twice"]);
    // Inside the function: its parameter and variable
    let names = labels(&server.at("textDocument/completion", &module, 4, 14));
    assert!(names.contains(&"$values".to_string()) && names.contains(&"$total".to_string()), "{names:?}");
    server.stop();
}

#[test]
fn projects_that_cannot_be_used_say_why() {
    let dir = temp("project-problems");
    let manifest = "[project]\nname = \"demo\"\nauthor = \"ana\"\nversion = \"0.1.0\"\nkind = \"application\"\n\n[dependencies]\n";
    std::fs::write(dir.join("ferry.toml"), format!("{manifest}\"github:bob/strings\" = \"1.0.0\"\n")).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let script = dir.join("bin/demo.lks");
    let text = "use bob/strings/pad;\noutln(1);\n";
    std::fs::write(&script, text).unwrap();
    let mut server = Server::start();
    // A dependency that isn't downloaded: lks-lsp never downloads, so it says what does
    server.open(&script, text);
    let errors = server.diagnostics_for(&script);
    assert!(errors[0]["message"].as_str().unwrap().contains("isn't downloaded yet: `ferry check` or `ferry run` downloads it"), "{errors:?}");
    // A `ferry.toml` that can't be read
    std::fs::write(dir.join("ferry.toml"), format!("{manifest}\"bob/strings\" = \"1.0.0\"\n")).unwrap();
    server.change(&script, text, 2);
    let errors = server.diagnostics_for(&script);
    assert!(errors[0]["message"].as_str().unwrap().contains("isn't a package"), "{errors:?}");
    server.stop();
}
