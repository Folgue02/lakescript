//! `std/template` *(see `ideas/stdlib/template.md`)*.

use lakescript::{CapturedOutput, Engine};

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

/// What `template::render` gives for `template`, with the values of the map `values`.
fn render(template: &str, values: &str) -> String {
    let script = format!("use std/template;\nout(template::render({}, {values}));\n", quoted(template));
    run(&script)
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n"))
}

#[test]
fn values_and_expressions() {
    assert_eq!(render("Hello {{ $name }}!", "{'name' => 'Ana'}"), "Hello Ana!");
    assert_eq!(render("{{ $name.upper() }} {{ 1 + 2 }} {{ [1, 2] }}", "{'name' => 'bo'}"), "BO 3 [1, 2]");
    // `null` writes nothing
    assert_eq!(render("[{{ $nick }}]", "{'nick' => null}"), "[]");
    // An `if` that gives a value, and braces of maps inside a tag
    assert_eq!(render("{{ if ($n > 1) 'items' else 'item' }} {{ {'a' => 1}['a'] }}", "{'n' => 2}"), "items 1");
    // A function given in the map, and a `}}` inside a string
    assert_eq!(render("{{ $twice(4) }} {{ '}}' }}", "{'twice' => ($n) -> $n * 2}"), "8 }}");
    assert_eq!(render("no tags", "{}"), "no tags");
    assert_eq!(run("use std/template;\noutln(template::render('{{ \\'{{\\' }}'));"), "{{\n");
}

#[test]
fn blocks() {
    let list = "{'items' => ['a', 'b', 'c']}";
    assert_eq!(render("{{ for ($x in $items) }}<{{ $x }}>{{ endfor }}", list), "<a><b><c>");
    assert_eq!(render("{{ for ($i, $x in $items) }}{{ $i }}{{ $x }}{{ if ($i < 2) }},{{ endif }}{{ endfor }}", list), "0a,1b,2c");
    let size = "{{ if ($n > 10) }}big{{ else if ($n > 5) }}medium{{ else }}small{{ endif }}";
    assert_eq!(render(size, "{'n' => 20}"), "big");
    assert_eq!(render(size, "{'n' => 7}"), "medium");
    assert_eq!(render(size, "{'n' => 1}"), "small");
    assert_eq!(render("{{ let $i = 0 }}{{ while ($i < 3) }}{{ $i }}{{ $i += 1 }}{{ endwhile }}", "{}"), "012");
    assert_eq!(render("{{ for ($x in $items) }}{{ if ($x == 'b') }}{{ break }}{{ endif }}{{ $x }}{{ endfor }}", list), "a");
    assert_eq!(render("{{ const $total = $items.length() }}{{ $total }} {{ // not written }}!", list), "3 !");
}

#[test]
fn lines_with_only_a_block_disappear() {
    let template = "<ul>\n  {{ for ($x in $items) }}\n  <li>{{ $x }}</li>\n  {{ endfor }}\n</ul>\n{{ // the end }}\n";
    assert_eq!(render(template, "{'items' => [1, 2]}"), "<ul>\n  <li>1</li>\n  <li>2</li>\n</ul>\n");
    // A line that also writes something keeps its new line
    assert_eq!(render("a {{ $x }}\nb\n", "{'x' => 1}"), "a 1\nb\n");
    assert_eq!(render("{{ if (true) }}a{{ endif }}\nb", "{}"), "a\nb");
    assert_eq!(render("{{ if (true) }}\n{{ if (true) }}\nx\n{{ endif }}\n{{ endif }}\n", "{}"), "x\n");
}

#[test]
fn html_escaping() {
    let values = "{'x' => '<b>\"Tom\" & \\'Jerry\\'</b>'}";
    assert_eq!(render("<p>{{ $x }}</p>", values), "<p><b>\"Tom\" & 'Jerry'</b></p>");
    let escaped = run(&format!("use std/template;\nout(template::render('<p>{{{{ $x }}}}</p>', {values}, html: true));"));
    assert_eq!(escaped, "<p>&lt;b&gt;&quot;Tom&quot; &amp; &#39;Jerry&#39;&lt;/b&gt;</p>");
    let raw = run(&format!("use std/template;\nout(template::render('{{{{ raw($x) }}}}', {values}, html: true));"));
    assert_eq!(raw, "<b>\"Tom\" & 'Jerry'</b>");

    let dir = std::env::temp_dir().join(format!("lks-template-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("page.html"), "<h1>{{ $title }}</h1>\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "<{{ $title }}>\n").unwrap();
    std::fs::write(dir.join("page.html.elks"), "<p>{{ $title }}</p>\n").unwrap();
    std::fs::write(dir.join("notes.txt.elks"), "[{{ $title }}]\n").unwrap();
    let out = run(&format!(
        "use std/template;\n\
         out(template::renderFile('{d}/page.html', {{'title' => 'A & B'}}));\n\
         out(template::renderFile('{d}/notes.txt', {{'title' => 'A & B'}}));\n\
         out(template::renderFile('{d}/page.html', {{'title' => 'A & B'}}, html: false));\n\
         out(template::renderFile('{d}/page.html.elks', {{'title' => 'A & B'}}));\n\
         out(template::renderFile('{d}/notes.txt.elks', {{'title' => 'A & B'}}));\n",
        d = dir.display()
    ));
    assert_eq!(out, "<h1>A &amp; B</h1>\n<A & B>\n<h1>A & B</h1>\n<p>A &amp; B</p>\n[A & B]\n");
    let e = error(&format!("use std/template;\ntemplate::renderFile('{}/page.html', {{}});", dir.display()));
    assert!(e.starts_with(&format!("{}/page.html:1:8: error: `$title` isn't declared", dir.display())), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(error("use std/template;\ntemplate::renderFile('/nonexistent/t.html');").contains("FileNotFound"));
}

#[test]
fn errors_point_at_the_template() {
    let fails = |template: &str| error(&format!("use std/template;\ntemplate::render({});", quoted(template)));
    assert_eq!(fails("a\n  {{ $x.upper() }}"), "<template>:2:6: error: `$x` isn't declared");
    assert_eq!(fails("{{ 5.upper() }}"), "<template>:1:4: error: TypeError: a number has no method `.upper()`");
    // Columns count characters, also after ones of several bytes
    assert_eq!(fails("€ {{ '€€' + 5.upper() }}"), "<template>:1:13: error: TypeError: a number has no method `.upper()`");
    assert_eq!(fails("x {{ $a + }}"), "<template>:1:10: error: expected an expression, found the `}}` of the tag");
    assert_eq!(fails("{{ for ($x in [1]) }}\n{{ endif }}"), "<template>:2:1: error: this `{{ endif }}` should be `{{ endfor }}`, to close the `{{ for }}` of line 1");
    assert_eq!(fails("a\n{{ if (true) }}b"), "<template>:2:1: error: the `{{ if }}` of line 2 has no `{{ endif }}`");
    assert_eq!(fails("{{ endfor }}"), "<template>:1:1: error: there's no `{{ for }}` for this `{{ endfor }}` to close");
    assert_eq!(fails("{{ else }}"), "<template>:1:1: error: `{{ else }}` can only be inside an `{{ if }}`");
    assert!(fails("{{ if (true) }}{{ else }}{{ else }}{{ endif }}").contains("after the `{{ else }}` of the `{{ if }}` of line 1"));
    assert!(fails("{{ if (true) }}{{ end }}").contains("blocks close with their own end"));
    assert_eq!(fails("ab {{ $x"), "<template>:1:4: error: this `{{` has no `}}` that closes it");
    assert_eq!(fails("{{ }}"), "<template>:1:1: error: there's nothing inside this `{{ }}`");
    // Errors of the script stay in the script
    assert!(error("use std/template;\ntemplate::render('{{ $x }}', {'a b' => 1});").contains("test.lks:2:1: error: ValueError: 'a b' can't be the name"));
    assert!(error("use std/template;\ntemplate::render('{{ $f() }}', {'f' => () -> 5.upper()});").starts_with("test.lks:2:"));
    // A template can catch nothing, but the script can
    assert_eq!(
        run("use std/template;\ntry {\n    template::render('{{ $x }}');\n} catch ($e) {\n    outln($e.line());\n}"),
        "1\n"
    );
}

#[test]
fn the_checker_knows_it() {
    assert!(error("use std/template;\ntemplate::render(5);").contains("`$text` of `render` must be a string"));
    assert!(error("use std/template;\ntemplate::render('x', html: 'yes');").contains("`$html` of `render` must be a bool"));
}
