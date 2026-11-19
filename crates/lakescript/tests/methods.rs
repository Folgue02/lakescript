//! The methods of the built-in types, checked against the examples in
//! `ideas/stdlib/types/`.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

fn eval(expr: &str) -> String {
    run(&format!("outln({expr});")).trim_end_matches('\n').to_string()
}

fn error(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(source) {
        Ok(()) => panic!("expected an error:\n{source}"),
        Err(e) => e.message,
    }
}

#[test]
fn string_information() {
    assert_eq!(eval("'año'.length()"), "3");
    assert_eq!(eval("'  \\n'.blank?()"), "true");
    assert_eq!(eval("'backup.tar.gz'.contains('.tar')"), "true");
    assert_eq!(eval("'/usr/bin'.startsWith('/usr') && 'notes.txt'.hasSuffix('.txt')"), "true");
    assert_eq!(eval("'banana'.count('an')"), "2");
    assert_eq!(eval("'YES'.equalsIgnoreCase('yes')"), "true");
    assert_eq!(eval("'ABC1'.upper?()"), "true");
    assert_eq!(eval("'-2.5'.numeric?() && '1e6'.numeric?() && !'abc'.numeric?()"), "true");
    assert_eq!(eval("'0042'.digits?()"), "true");
}

#[test]
fn string_searching() {
    assert_eq!(eval("'a-b-c'.indexOf('-')"), "1");
    assert_eq!(eval("'a-b-c'.indexOf('-', 2)"), "3");
    assert_eq!(eval("'a-b-c'.indexOfLast('-')"), "3");
    assert_eq!(eval("'abc'.indexOf('z')"), "null");
    assert_eq!(eval("'a-b-c'.indexesOf('-')"), "[1, 3]");
    assert_eq!(eval("'ñ-b'.indexOf('b')"), "2");
    assert_eq!(eval("'key=value'.before('=')"), "key");
    assert_eq!(eval("'key=value'.after('=')"), "value");
    assert_eq!(eval("'archive.tar.gz'.beforeLast('.')"), "archive.tar");
    assert_eq!(eval("'/home/me/notes.txt'.afterLast('/')"), "notes.txt");
    assert_eq!(eval("'abc'.after('z')"), "null");
}

#[test]
fn string_case() {
    assert_eq!(eval("'hello world'.capitalize()"), "Hello world");
    assert_eq!(eval("'hello world'.capitalizeWords()"), "Hello World");
    assert_eq!(eval("'Hello'.uncapitalize()"), "hello");
    assert_eq!(eval("'aBc'.swapCase()"), "AbC");
    assert_eq!(eval("'user name'.camelCase()"), "userName");
    assert_eq!(eval("'user_name'.camelCase()"), "userName");
    assert_eq!(eval("'user name'.pascalCase()"), "UserName");
    assert_eq!(eval("'userName'.snakeCase()"), "user_name");
    assert_eq!(eval("'userName'.kebabCase()"), "user-name");
    assert_eq!(eval("'userName'.constantCase()"), "USER_NAME");
    assert_eq!(eval("'HTTPServer'.snakeCase()"), "http_server");
}

#[test]
fn string_trimming_and_padding() {
    assert_eq!(eval("'  hi \\n'.trim()"), "hi");
    assert_eq!(eval("'--hi--'.trim('-')"), "hi");
    assert_eq!(eval("'  hi  '.trimStart() + '|'"), "hi  |");
    assert_eq!(eval("'v1.2'.removePrefix('v')"), "1.2");
    assert_eq!(eval("'web.log'.removeSuffix('.log')"), "web");
    assert_eq!(eval("'7'.padStart(3, '0')"), "007");
    assert_eq!(eval("'ab'.padEnd(4) + '|'"), "ab  |");
    assert_eq!(eval("'hi'.center(6, '*')"), "**hi**");
    assert_eq!(eval("'Hello world'.truncate(8)"), "Hello...");
    assert_eq!(eval("'short'.truncate(8)"), "short");
}

#[test]
fn string_changing() {
    assert_eq!(eval("'a-b-c'.replace('-', '+')"), "a+b+c");
    assert_eq!(eval("'a-b-c'.replaceFirst('-', '+')"), "a+b-c");
    assert_eq!(eval("'a-b-c'.replaceLast('-', '+')"), "a-b+c");
    assert_eq!(eval("'1,000,000'.remove(',')"), "1000000");
    assert_eq!(eval("'helo'.insert(3, 'l')"), "hello");
    assert_eq!(eval("'-'.repeat(5)"), "-----");
    assert_eq!(eval("'abc'.reverse()"), "cba");
    assert_eq!(eval("' a   b\\n c '.collapseWhitespace()"), "a b c");
    assert_eq!(eval("'a\\nb'.indent(2)"), "  a\n  b");
}

#[test]
fn string_splitting() {
    assert_eq!(eval("'a,b,c'.split(',')"), "['a', 'b', 'c']");
    assert_eq!(eval("'a,b,c'.split(',', 2)"), "['a', 'b,c']");
    assert_eq!(eval("'key=a=b'.splitOnce('=')"), "['key', 'a=b']");
    assert_eq!(eval("'novalue'.splitOnce('=')"), "null");
    assert_eq!(eval("\"a\\nb\\r\\nc\\n\".lines()"), "['a', 'b', 'c']");
    assert_eq!(eval("' a  b\\tc '.words()"), "['a', 'b', 'c']");
    assert_eq!(eval("'abc'.chars()"), "['a', 'b', 'c']");
}

#[test]
fn string_converting() {
    assert_eq!(eval("'42'.toNumber() + 1"), "43");
    assert_eq!(eval("' -2.5 '.toNumber()"), "-2.5");
    assert_eq!(eval("'0xFF'.toNumber()"), "255");
    assert_eq!(eval("'1_000'.toNumber()"), "1000");
    assert_eq!(eval("'TRUE'.toBool()"), "true");
    assert_eq!(eval("'active'.toSymbol() == :active"), "true");
    assert_eq!(eval("'A'.codePoint()"), "65");
    assert!(error("'abc'.toNumber();").contains("isn't a number"));
    assert!(error("'maybe'.toBool();").contains("isn't `true` or `false`"));
}

#[test]
fn numbers() {
    assert_eq!(eval("4.0.whole?() && !4.5.whole?()"), "true");
    assert_eq!(eval("5.between(1, 10)"), "true");
    assert_eq!(eval("2.5.round()"), "3");
    assert_eq!(eval("3.14159.round(2)"), "3.14");
    assert_eq!(eval("(-2.1).floor()"), "-3");
    assert_eq!(eval("2.1.ceil()"), "3");
    assert_eq!(eval("(-2.7).truncate()"), "-2");
    assert_eq!(eval("(-5).abs()"), "5");
    assert_eq!(eval("(-5).sign()"), "-1");
    assert_eq!(eval("5.min(3)"), "3");
    assert_eq!(eval("150.clamp(0, 100)"), "100");
    assert_eq!(eval("7.div(2)"), "3");
    assert_eq!(eval("(-7).div(2)"), "-3");
    assert_eq!(eval("(-1).mod(5)"), "4");
    assert_eq!(eval("7.mod(-5)"), "-3");
    assert_eq!(eval("100.log(10)"), "2");
    assert_eq!(eval("255.toString(16)"), "ff");
    assert_eq!(eval("5.toString(2)"), "101");
    assert_eq!(eval("(-255).toString(16)"), "-ff");
    assert_eq!(eval("2.5.toFixed(2)"), "2.50");
    assert_eq!(eval("1234567.withSeparators()"), "1,234,567");
    assert_eq!(eval("(-1234.5).withSeparators()"), "-1,234.5");
    assert_eq!(eval("1536.formatBytes()"), "1.5 KB");
    assert_eq!(eval("3_000_000_000.formatBytes()"), "2.8 GB");
    assert_eq!(eval("500.formatBytes()"), "500 bytes");
    assert_eq!(eval("65.toChar()"), "A");
    assert_eq!(run("3.times(($i) = out($i)); outln();"), "012\n");
    assert!(error("5.div(0);").contains("division by zero"));
}

#[test]
fn list_basics() {
    assert_eq!(eval("['a', 'b', 'a'].indexOf('a')"), "0");
    assert_eq!(eval("['a', 'b', 'a'].indexOfLast('a')"), "2");
    assert_eq!(eval("[1, 2, 1].count(1)"), "2");
    assert_eq!(eval("[17, 20, 30].countWhere(($a) = $a >= 18)"), "2");
    assert_eq!(eval("[].first()"), "null");
    assert_eq!(eval("[1, 2].last()"), "2");
    assert_eq!(eval("[1, 2].get(5, 0)"), "0");
}

#[test]
fn list_changing() {
    let script = "
        let $l = [3];
        $l.push(4, 5).prepend(1, 2);
        $l.insert(0, 0);
        $l.extend([6]);
        outln($l);
        outln($l.removeFirst());
        outln($l.removeLast());
        outln($l.removeAt(1));
        outln($l.remove(4));
        outln($l.remove(99));
        outln($l);
        let $m = [1, 2, 1, 3, 1];
        outln($m.removeAll(1));
        outln($m.removeWhere(($x) = $x > 2));
        outln($m);
        $m.clear();
        outln($m);
    ";
    assert_eq!(run(script), "[0, 1, 2, 3, 4, 5, 6]\n0\n6\n2\ntrue\nfalse\n[1, 3, 5]\n3\n1\n[2]\n[]\n");
}

#[test]
fn list_ordering() {
    let script = "
        let $l = [3, 1, 2];
        const $copy = $l.sorted();
        outln($l);
        $l.sort();
        outln($l);
        outln($copy);
        outln(['ccc', 'a', 'bb'].sortedBy(($s) = $s.length()));
        outln([1, 2, 3].reversed());
        outln(['b', 'a'].sort());
    ";
    assert_eq!(run(script), "[3, 1, 2]\n[1, 2, 3]\n[1, 2, 3]\n['a', 'bb', 'ccc']\n[3, 2, 1]\n['a', 'b']\n");
    assert!(error("[1, 'a'].sort();").contains("can't order a"));
}

#[test]
fn list_transforming() {
    assert_eq!(eval("['a', 'b'].map(($s) = $s.upper())"), "['A', 'B']");
    assert_eq!(eval("[1, 2, 3, 4].filter(($n) = $n.even?())"), "[2, 4]");
    assert_eq!(eval("[1, 2, 3, 4].reject(($n) = $n.even?())"), "[1, 3]");
    assert_eq!(eval("['a b', 'c'].flatMap(($l) = $l.words())"), "['a', 'b', 'c']");
    assert_eq!(eval("[[1, 2], [3], []].flatten()"), "[1, 2, 3]");
    assert_eq!(eval("[1, 2, 1, [1], [1]].unique()"), "[1, 2, [1]]");
    assert_eq!(eval("['a', 'B', 'b'].uniqueBy(($s) = $s.lower())"), "['a', 'B']");
    assert_eq!(eval("[1, null, 2].compact()"), "[1, 2]");
    assert_eq!(eval("[1, 2, 3].take(2)"), "[1, 2]");
    assert_eq!(eval("[1, 2, 3].skip(2)"), "[3]");
    assert_eq!(eval("[1, 2, 5, 1].takeWhile(($n) = $n < 3)"), "[1, 2]");
    assert_eq!(eval("[1, 2, 5, 1].skipWhile(($n) = $n < 3)"), "[5, 1]");
    assert_eq!(eval("[1, 2, 3].chunk(2)"), "[[1, 2], [3]]");
    assert_eq!(eval("[1].concat([2])"), "[1, 2]");
}

#[test]
fn list_grouping_and_searching() {
    assert_eq!(eval("['a.txt', 'b.log', 'c.txt'].groupBy(($f) = $f.afterLast('.'))"), "{ 'txt' => ['a.txt', 'c.txt'], 'log' => ['b.log'] }");
    assert_eq!(eval("['A', 'a', 'b'].countBy(($w) = $w.lower())"), "{ 'a' => 2, 'b' => 1 }");
    assert_eq!(run("let [$adults, $kids] = [20, 5, 30].partition(($a) = $a >= 18); outln($adults); outln($kids);"), "[20, 30]\n[5]\n");
    assert_eq!(eval("['ab', 'cd'].toMap(($s) = $s[0])"), "{ 'a' => 'ab', 'c' => 'cd' }");
    assert_eq!(eval("[1, 5, 8].find(($n) = $n > 3)"), "5");
    assert_eq!(eval("[1, 5, 8].findLast(($n) = $n > 3)"), "8");
    assert_eq!(eval("[1, 5, 8].findIndex(($n) = $n > 3)"), "1");
    assert_eq!(eval("[1, 5].find(($n) = $n > 10)"), "null");
    assert_eq!(eval("[1, 5].any(($n) = $n > 3)"), "true");
    assert_eq!(eval("[1, 5].all(($n) = $n > 3)"), "false");
    assert_eq!(eval("[].all(($n) = $n > 3)"), "true");
    assert_eq!(eval("[1, 5].none(($n) = $n > 10)"), "true");
}

#[test]
fn list_combining() {
    assert_eq!(eval("['a', 'b'].join(', ')"), "a, b");
    assert_eq!(eval("['usr', 'bin'].reduce(($a, $b) = $a + '/' + $b)"), "usr/bin");
    assert_eq!(eval("[].reduce(($a, $b) = $a)"), "null");
    assert_eq!(eval("[1, 2, 3].fold(10, ($acc, $n) = $acc + $n)"), "16");
    assert_eq!(eval("[1, 2, 3].sum()"), "6");
    assert_eq!(eval("[].sum()"), "0");
    assert_eq!(eval("[2, 3].product()"), "6");
    assert_eq!(eval("[1, 2].average()"), "1.5");
    assert_eq!(eval("[].average()"), "null");
    assert_eq!(eval("[3, 1, 2].min()"), "1");
    assert_eq!(eval("['b', 'c', 'a'].max()"), "c");
    assert_eq!(eval("['ccc', 'a'].minBy(($s) = $s.length())"), "a");
    assert_eq!(run("['a', 'b'].eachWithIndex(($v, $i) = out(f\"{$i}{$v}\")); outln();"), "0a1b\n");
}

#[test]
fn maps() {
    let script = "
        let $m = {'a' => 1, 'b' => 2};
        outln($m.get('z', 0));
        outln($m.keyOf(2));
        outln($m.hasValue(1));
        let $groups = <string, list<string>>{};
        $groups.getOrSet('txt', []) << 'a.txt';
        $groups.getOrSet('txt', []) << 'b.txt';
        outln($groups);
        let $counts = {};
        for ($w in %w[x y x]) $counts.update($w, 0, ($n) = $n + 1);
        outln($counts);
        outln($m.remove('a'));
        outln($m);
        outln({'port' => 1}.merged({'port' => 2, 'host' => 'x'}));
        outln({'a' => 1, 'b' => 2}.filter(($k, $v) = $v > 1));
        outln({'a' => 1}.mapValues(($v) = $v * 10));
        outln({'a' => 1}.mapKeys(($k) = $k.upper()));
        outln({'a' => 1, 'b' => 2}.map(($k, $v) = f\"{$k}={$v}\"));
        outln({'a' => 'x'}.invert());
        outln({'b' => 1, 'a' => 2}.sortedByKey());
        outln({'x' => 3, 'y' => 9}.sortedBy(($name, $score) = -$score));
        outln({'a' => 1}.find(($k, $v) = $v == 1));
        outln({'a' => 1}.any(($k, $v) = $v > 0));
        outln({'a' => 1, 'b' => 2}.countWhere(($k, $v) = $v > 0));
        outln({'a' => 1}.first());
    ";
    assert_eq!(
        run(script),
        "0\nb\ntrue\n{ 'txt' => ['a.txt', 'b.txt'] }\n{ 'x' => 2, 'y' => 1 }\n1\n{ 'b' => 2 }\n{ 'port' => 2, 'host' => 'x' }\n{ 'b' => 2 }\n{ 'a' => 10 }\n{ 'A' => 1 }\n['a=1', 'b=2']\n{ 'x' => 'a' }\n{ 'a' => 2, 'b' => 1 }\n{ 'y' => 9, 'x' => 3 }\na\ntrue\n2\na\n"
    );
}

#[test]
fn ranges() {
    assert_eq!(eval("(1..5).length()"), "4");
    assert_eq!(eval("(0..10).step(5).length()"), "2");
    assert_eq!(eval("(3..3).empty?()"), "true");
    assert_eq!(eval("(1..5).last()"), "4");
    assert_eq!(eval("(1..=5).reverse().toList()"), "[5, 4, 3, 2, 1]");
    assert_eq!(eval("(1..=3).map(($n) = $n * $n)"), "[1, 4, 9]");
    assert_eq!(eval("(1..=10).filter(($n) = $n % 3 == 0)"), "[3, 6, 9]");
    assert_eq!(eval("(1..=100).sum()"), "5050");
}

#[test]
fn other_types() {
    assert_eq!(eval(":active.name()"), "active");
    assert_eq!(eval("true.toNumber()"), "1");
    assert_eq!(eval("[1, 2].toString()"), "[1, 2]");
    assert_eq!(eval("5.typeName()"), "number");
}

#[test]
fn wrong_arguments() {
    assert!(error("'a'.padStart('x');").contains("must be a whole number, not a string"));
    assert!(error("[1].map(5);").contains("must be a function"));
    assert!(error("[[1], 2].flatten();").contains("needs a list of lists"));
    assert!(error("[1].filter(($x) = 1);").contains("must return a bool"));
    assert!(error("'a'.nope();").contains("has no method `.nope()`"));
}
