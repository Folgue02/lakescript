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
    let response = server.request("textDocument/completion", json!({ "textDocument": { "uri": "file:///x.lks" }, "position": { "line": 0, "character": 0 } }));
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
