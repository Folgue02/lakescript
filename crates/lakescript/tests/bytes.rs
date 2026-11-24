//! The `bytes` type, `std/bytes`, and bytes in files and archives *(see
//! `ideas/stdlib/types/bytes.md`)*.

use lakescript::{CapturedOutput, Engine};

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

#[test]
fn literals_and_conversions() {
    assert_eq!(run("outln(%b\"\\x89PNG\\r\\n\".hex());"), "89504e470d0a\n");
    assert_eq!(run("outln(%b\"é\".length());"), "2\n");
    assert_eq!(run("const $b = 'héllo'.toBytes();\noutln($b);\noutln($b.text());\noutln($b.toList());"), "<bytes 6 bytes>\nhéllo\n[104, 195, 169, 108, 108, 111]\n");
    assert_eq!(run("outln([255, 0, 7].toBytes().hex());"), "ff0007\n");
    assert_eq!(run("outln('hello'.toBytes().base64());\noutln(''.toBytes().base64());\noutln('ab'.toBytes().base64());"), "aGVsbG8=\n\nYWI=\n");
    assert_eq!(run("use std/bytes;\noutln(bytes::fromHex('89 50 4E 47') == %b\"\\x89PNG\");\noutln(bytes::fromBase64('aGVs\\nbG8').text());\noutln(bytes::fromBase64('-_8').hex());"), "true\nhello\nfbff\n");
    // Big ones print their size
    assert_eq!(run("outln((0..3000).toList().map(($i) -> $i % 256).toBytes());"), "<bytes 2.9 KB>\n");
}

#[test]
fn indexes_slices_joining_and_loops() {
    let code = "const $b = %b\"abc\";\noutln($b[0]);\noutln($b[-1]);\noutln($b[9]);\noutln($b[1..].text());\noutln(($b + %b\"!\").text());\nfor ($n in $b) out(f\"{$n} \");\noutln();\noutln($b == 'abc'.toBytes());\noutln($b.startsWith(%b\"ab\") && $b.endsWith(%b\"c\"));\noutln($b.indexOf(%b\"c\"));\noutln($b.is(bytes));";
    assert_eq!(run(code), "97\n99\nnull\nbc\nabc!\n97 98 99 \ntrue\ntrue\n2\ntrue\n");
}

#[test]
fn mistakes() {
    assert!(error("outln(%b\"\\xff\".text());").contains("aren't text: byte 0 isn't valid UTF-8"));
    assert!(error("outln([256].toBytes());").contains("a byte is a whole number from 0 to 255, and element 0 is 256"));
    assert!(error("use std/bytes;\nbytes::fromHex('abc');").contains("odd number of digits"));
    assert!(error("use std/bytes;\nbytes::fromBase64('a*b');").contains("isn't base64"));
    assert!(error("const $b = %b\"a\";\n$b[0] = 1;").contains("bytes can't be changed"));
    assert!(error("outln(%b\"a\" + 'b');").contains("`+` joins two bytes"));
    assert!(error("outln(%b\"\\q\");").contains("unknown escape `\\q` in bytes"));
    assert!(error("use std/format/json;\njson::stringify(%b\"a\");").contains("JSON"));
}

#[test]
fn files_and_archives_keep_every_byte() {
    let dir = std::env::temp_dir().join(format!("lks-bytes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let code = format!(
        "use std/fs;\nuse std/archive;\nuse std/process;\nprocess::chdir('{}');\n\
         const $data = %b\"\\x89PNG\\x00\\xff\\xfe\";\n\
         fs::writeBytes('img.bin', $data);\n\
         outln(fs::readBytes('img.bin') == $data);\n\
         for ($name in %w[a.zip a.tar.gz]) {{\n\
             const $new = archive::create($name);\n\
             $new.addBytes('img.bin', $data);\n\
             $new.add('img.bin', as: 'copy.bin');\n\
             $new.close();\n\
             const $read = archive::open($name);\n\
             outln($read.readBytes('img.bin') == $data && $read.readBytes('copy.bin') == $data);\n\
         }}",
        dir.display()
    );
    assert_eq!(run(&code), "true\ntrue\ntrue\n");
    assert_eq!(std::fs::read(dir.join("img.bin")).unwrap(), [0x89, b'P', b'N', b'G', 0, 0xff, 0xfe]);
    let _ = std::fs::remove_dir_all(&dir);
}
