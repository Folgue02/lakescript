//! `std/http`, written in Lakescript with a part in Rust, and `std/format/csv`
//! *(see `ideas/stdlib/http.md` and `ideas/stdlib/format/csv.md`)*.

// Only with the features of the crate these modules need
#![cfg(all(feature = "http", feature = "csv"))]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use lakescript::{CapturedOutput, Completion, Engine, Name};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("test.lks"), output.text());
    }
    output.text()
}

/// The error a script stops with.
fn error(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(source) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.render("test.lks"),
    }
}

/// A server that answers `count` requests: with the request it got as the
/// body, and the status in the path *(i.e. `/404`)*.
fn server(count: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().take(count) {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = Vec::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap();
                }
                head.push(line.trim().to_string());
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let path = head[0].split(' ').nth(1).unwrap().to_string();
            // `/raw` answers with the body it got, exactly, and its content type
            if path == "/raw" {
                let kind = head.iter().find_map(|h| h.to_ascii_lowercase().strip_prefix("content-type:").map(|t| t.trim().to_string())).unwrap_or_default();
                let mut answer = format!("HTTP/1.1 200 OK\r\nX-Sent-As: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
                answer.extend_from_slice(&body);
                stream.write_all(&answer).unwrap();
                continue;
            }
            let status: u16 = path.trim_start_matches('/').parse().unwrap_or(200);
            let text = format!("{}\n{}", head.join("\n"), String::from_utf8_lossy(&body));
            let answer = format!(
                "HTTP/1.1 {status} Whatever\r\nContent-Type: text/plain\r\nX-Two: a\r\nX-Two: b\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
            stream.write_all(answer.as_bytes()).unwrap();
        }
    });
    address
}

#[test]
fn requests_and_responses() {
    let url = server(4);
    let out = run(&format!(
        "use std/http;\n\
         const $r = http::get('{url}/hello', headers: {{'X-Mine' => 'yes'}});\n\
         outln(f\"{{$r.status}} {{$r.ok?()}} {{$r.header('CONTENT-TYPE')}} {{$r.header('x-two')}}\");\n\
         outln($r.body.lines()[0]);\n\
         outln($r.body.contains('x-mine: yes'));\n\
         outln($r.url.endsWith('/hello'));\n\
         const $missing = http::get('{url}/404');\n\
         outln(f\"{{$missing.status}} {{$missing.ok?()}}\");\n\
         const $sent = http::post('{url}/201', {{'a' => [1, 2]}});\n\
         outln($sent.status);\n\
         outln($sent.body.contains('content-type: application/json'));\n\
         outln($sent.body.lines().last());\n\
         const $text = http::request('put', '{url}/', 'plain words');\n\
         outln($text.body.lines()[0]);\n\
         outln($text.body.lines().last());\n"
    ));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "200 true text/plain a, b");
    assert_eq!(lines[1], "GET /hello HTTP/1.1");
    assert_eq!(&lines[2..4], ["true", "true"]);
    assert_eq!(lines[4], "404 false");
    assert_eq!(&lines[5..8], ["201", "true", "{\"a\":[1,2]}"]);
    assert_eq!(&lines[8..10], ["PUT / HTTP/1.1", "plain words"]);
}

#[test]
fn downloading_writes_only_what_went_well() {
    let url = server(2);
    let dir = std::env::temp_dir().join(format!("lks-http-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("got.txt");
    let missing = dir.join("missing.txt");
    let out = run(&format!(
        "use std/http;\n\
         const $r = http::download('{url}/file', '{}');\n\
         outln(f\"{{$r.status}} [{{$r.body}}]\");\n\
         const $no = http::download('{url}/404', '{}');\n\
         outln(f\"{{$no.status}} {{$no.body.length() > 0}}\");\n",
        file.display(),
        missing.display()
    ));
    assert_eq!(out, "200 []\n404 true\n");
    assert!(std::fs::read_to_string(&file).unwrap().starts_with("GET /file HTTP/1.1"));
    assert!(!missing.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_answer_is_a_network_error() {
    // A port nobody listens on
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let out = run(&format!(
        "use std/http;\n\
         try {{\n    http::get('http://127.0.0.1:{port}/');\n}} catch (IOError $e) {{\n    outln($e.typeName());\n    outln($e.message());\n}}\n"
    ));
    assert_eq!(out, format!("NetworkError\ncan't reach 'http://127.0.0.1:{port}/': the server refused the connection\n"));
    assert!(error("use std/http;\nhttp::get('not an address');").contains("ValueError: 'not an address' isn't a valid address"));
    assert!(error("use std/http;\nhttp::get('http://127.0.0.1:1/', timeout: 0);").contains("a timeout must be a positive number"));
}

#[test]
fn the_rust_part_is_only_for_the_standard_library() {
    assert!(error("use std/http/native;\noutln(1);").contains("there's no module `std/http/native`"));
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_type_check(false);
    let e = engine.run("use std/http/native;\nnative::send('GET', 'x', null, {}, null, null);").unwrap_err();
    assert!(e.message.contains("there's no module `std/http/native`"), "{}", e.message);
}

#[test]
fn its_classes_are_checked_like_any_other() {
    assert!(error("use std/http;\noutln(http::get('x').status.upper());").contains("a number has no method `.upper()`"));
    assert!(error("use std/http;\nhttp::get(5);").contains("`$url` of `get` must be a string"));
    // `std/json` moved to `std/format/json`
    assert!(error("use std/json;").contains("there's no module `std/json`"));
}

#[test]
fn editors_see_its_documentation() {
    let source = "use std/http;\nconst $r = http::get('x');\noutln($r.status);\n";
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_names(true);
    engine.check_source(source).unwrap();
    let names: Vec<Name> = engine.names();
    let at = source.rfind("status").unwrap();
    let status = names.iter().find(|n| n.file.is_none() && n.start <= at && at < n.end).expect("`status` is known");
    assert_eq!(status.code, "const number @status");
    assert!(status.doc.as_deref().is_some_and(|d| d.starts_with("The status code")), "{:?}", status.doc);

    let source = "use std/http;\nconst $r = http::get('x');\noutln($r.);\n";
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_completion(None, source.find("$r.)").unwrap() + 3);
    let _ = engine.check_source(source);
    let found: Completion = engine.completion().unwrap();
    let labels: Vec<String> = found.members.unwrap().into_iter().map(|i| i.label).collect();
    for wanted in ["status", "headers", "body", "url", "ok?", "json", "header"] {
        assert!(labels.iter().any(|l| l == wanted), "{wanted} isn't in {labels:?}");
    }
}

// ---- std/format/csv ----

#[test]
fn csv_rows_and_maps() {
    let out = run(
        "use std/format/csv;\n\
         const $text = \"name,age\\nAna,30\\n\\\"Smith, John\\\",17\\n\";\n\
         outln(csv::parse($text));\n\
         const $people = csv::parseMaps($text);\n\
         outln($people);\n\
         outln($people[1]?['name']);\n\
         outln(csv::stringify([['a', 'b,c', 'say \"hi\"'], [1, null, true]]));\n\
         outln(csv::stringify($people, separator: ';'));\n\
         outln(csv::parse(\"a\\tb\", separator: '\\t'));\n\
         outln(csv::parseMaps(\"a,b\\n1\"));\n\
         outln(csv::parse(''));\n",
    );
    assert_eq!(
        out,
        "[['name', 'age'], ['Ana', '30'], ['Smith, John', '17']]\n\
         [{ 'name' => 'Ana', 'age' => '30' }, { 'name' => 'Smith, John', 'age' => '17' }]\n\
         Smith, John\n\
         a,\"b,c\",\"say \"\"hi\"\"\"\n1,,true\n\n\
         name;age\nAna;30\nSmith, John;17\n\n\
         [['a', 'b']]\n\
         [{ 'a' => '1', 'b' => '' }]\n\
         []\n"
    );
}

#[test]
fn csv_files_and_errors() {
    let dir = std::env::temp_dir().join(format!("lks-csv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file: PathBuf = dir.join("out.csv");
    let out = run(&format!(
        "use std/format/csv;\n\
         csv::write('{f}', [{{'x' => 1, 'y' => 2}}, {{'x' => 3}}]);\n\
         outln(csv::readMaps('{f}'));\n\
         outln(csv::read('{f}'));\n",
        f = file.display()
    ));
    assert_eq!(out, "[{ 'x' => '1', 'y' => '2' }, { 'x' => '3', 'y' => '' }]\n[['x', 'y'], ['1', '2'], ['3', '']]\n");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(error("use std/format/csv;\ncsv::parseMaps(\"a,b\\n1,2,3\");").contains("line 2 has 3 values, but the first line only names 2 columns"));
    assert!(error("use std/format/csv;\ncsv::parseMaps(\"a,a\\n1,2\");").contains("the column 'a' is twice"));
    assert!(error("use std/format/csv;\ncsv::parse('a', separator: '::');").contains("the separator must be one character"));
    assert!(error("use std/format/csv;\ncsv::stringify([[[1]]]);").contains("TypeError: a value of CSV can be a string"));
    assert!(error("use std/format/csv;\ncsv::stringify([{'a' => 1}, {'b' => 2}]);").contains("a row has the key 'b'"));
    assert!(error("use std/format/csv;\ncsv::read('/nonexistent/x.csv');").contains("FileNotFound"));
}

#[test]
fn bodies_can_be_bytes() {
    let url = server(1);
    let out = run(&format!(
        "use std/http;\n\
         const $r = http::post('{url}/raw', %b\"\\x89PNG\\xff\\x00\");\n\
         outln($r.header('x-sent-as'));\n\
         outln($r.bytes().hex());\n\
         outln($r.bytes().length());\n\
         outln($r.body.contains('\\u{{FFFD}}'));\n"
    ));
    assert_eq!(out, "application/octet-stream\n89504e47ff00\n6\ntrue\n");
}

#[test]
fn a_response_is_read_into_a_type() {
    let url = server(1);
    let out = run(&format!(
        "use std/http;\n\
         class Repo {{\n    pub string @name;\n    pub number @stars;\n}}\n\
         const $r = http::post('{url}/raw', '{{\"name\": \"lakescript\", \"stars\": 7}}').json(Repo);\n\
         outln(f\"{{$r.name}} {{$r.stars + 1}}\");\n"
    ));
    assert_eq!(out, "lakescript 8\n");
}
