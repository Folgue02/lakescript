//! `std/html` *(see `ideas/stdlib/html.md`)*.

use lakescript::{CapturedOutput, Completion, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("test.lks"), output.text());
    }
    output.text()
}

fn error(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(source) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.render("test.lks"),
    }
}

/// Runs `code` after `$page` is the parsed `PAGE`.
fn on_page(code: &str) -> String {
    run(&format!("use std/html;\nconst $page = html::parse({PAGE:?});\n{code}"))
}

const PAGE: &str = r#"<html><head><title> My  shop </title><style>p { color: red }</style></head>
<body>
<h1 class="big" id="top">Hello <b>world</b></h1>
<ul id="items">
  <li class="item" data-price="3"><a href="/tea">Tea</a></li>
  <li class="item" data-price="4"><a href="/cake">Cake &amp; cream</a>
  <li class="item sold"><a href="/pie">Pie</a></li>
</ul>
<p>one</p><p>two<br>three</p>
<script>var x = {"a": 1};</script>"#;

#[test]
fn selecting() {
    assert_eq!(on_page("outln($page.select('li.item').length());"), "3\n");
    assert_eq!(on_page("outln($page.select('li:not(.sold) > a').map(($a) -> $a.text()));"), "['Tea', 'Cake & cream']\n");
    assert_eq!(on_page("outln($page.select('a[href^=\"/c\"]').map(($a) -> $a.attr('href')));"), "['/cake']\n");
    assert_eq!(on_page("outln($page.first('li:nth-child(2) a')?.text());"), "Cake & cream\n");
    assert_eq!(on_page("outln($page.first('nothing'));\noutln($page.select('nothing'));"), "null\n[]\n");
    // Inside an element: the element itself doesn't match
    assert_eq!(on_page("const $ul = $page.first('ul');\nif ($ul != null) outln($ul.select('ul, a').length());"), "3\n");
    assert!(error("use std/html;\nhtml::parse('').select('a[');").contains("ValueError: 'a[' isn't a valid CSS selector"));
}

#[test]
fn elements() {
    let out = on_page(
        "const $h = $page.first('h1');\n\
         if ($h != null) {\n\
             outln($h.tag);\n\
             outln($h.attrs());\n\
             outln($h.attr('ID'));\n\
             outln($h.attr('nope'));\n\
             outln($h.html());\n\
             outln($h.outerHtml());\n\
             outln($h.children().map(($c) -> $c.tag));\n\
             outln($h.parent()?.parent()?.parent());\n\
             outln($h);\n\
         }\n\
         outln($page.first('b')?.parent()?.attr('id'));\n\
         outln($page.first('#items')?.children().length());",
    );
    assert_eq!(
        out,
        "h1\n{ 'class' => 'big', 'id' => 'top' }\ntop\nnull\nHello <b>world</b>\n<h1 class=\"big\" id=\"top\">Hello <b>world</b></h1>\n['b']\nnull\nElement(tag: 'h1')\ntop\n3\n"
    );
}

#[test]
fn text_as_a_browser_shows_it() {
    assert_eq!(on_page("outln($page.title());"), "My shop\n");
    assert_eq!(on_page("outln($page.text());"), "Hello world Tea Cake & cream Pie one two three\n");
    // The element asked for is shown even when it's a script
    assert_eq!(on_page("outln($page.first('script')?.text());"), "var x = {\"a\": 1};\n");
    assert_eq!(run("use std/html;\noutln(html::parse('<p>x</p>').title());"), "null\n");
    // Broken HTML is repaired
    assert_eq!(run("use std/html;\noutln(html::parse('<p>a<p>b<div>c').select('p').map(($p) -> $p.text()));"), "['a', 'b']\n");
}

#[test]
fn escaping() {
    assert_eq!(run("use std/html;\noutln(html::escape('<a href=\"x\">Tom & Jerry\\'s</a>'));"), "&lt;a href=&quot;x&quot;&gt;Tom &amp; Jerry&#39;s&lt;/a&gt;\n");
    assert_eq!(run("use std/html;\noutln(html::unescape('&lt;b&gt; &amp; &copy; <i>x</i> &#x41;&#66;'));"), "<b> & © <i>x</i> AB\n");
}

#[test]
fn pages_are_freed_and_hidden() {
    // Many pages, each dropped after its loop
    let out = run("use std/html;\nlet $n = 0;\nfor ($i in 0..200) {\n    $n += html::parse('<a>1</a>'.repeat(500)).select('a').length();\n}\noutln($n);");
    assert_eq!(out, "100000\n");
    assert!(error("use std/html/native;\noutln(1);").contains("there's no module `std/html/native`"));
    assert!(error("use std/html;\nhtml::Element(5, 'x').text();").contains("only comes from `html::parse()`"));
}

#[test]
fn the_checker_and_editors_know_it() {
    assert!(error("use std/html;\nhtml::parse('').first('a').text();").contains("null"));
    assert!(error("use std/html;\nhtml::parse(5);").contains("`$text` of `parse` must be a string"));
    let source = "use std/html;\nconst $e = html::parse('').select('a')[0];\noutln($e?.);\n";
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_completion(None, source.find("$e?.)").unwrap() + 4);
    let _ = engine.check_source(source);
    let found: Completion = engine.completion().unwrap();
    let labels: Vec<String> = found.members.unwrap().into_iter().map(|i| i.label).collect();
    for wanted in ["tag", "select", "first", "text", "attr", "attrs", "html", "outerHtml", "children", "parent"] {
        assert!(labels.iter().any(|l| l == wanted), "{wanted} isn't in {labels:?}");
    }
    assert!(!labels.iter().any(|l| l == "node"), "the hidden field shows: {labels:?}");
}
