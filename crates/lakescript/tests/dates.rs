//! Dates in `std/time` and zones in `std/time/zone` *(see `ideas/stdlib/time.md`)*.
//! The tests name their zones, so they don't depend on the one of the system.

use lakescript::{CapturedOutput, Engine};

fn run(code: &str) -> String {
    let source = format!("use std/time;\nuse std/time/zone;\nconst $madrid = zone::named('Europe/Madrid');\n{code}");
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(&source) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("test.lks"), output.text());
    }
    output.text()
}

fn error(code: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(&format!("use std/time;\nuse std/time/zone;\n{code}")) {
        Ok(_) => panic!("it should have failed:\n{code}"),
        Err(e) => e.render("test.lks"),
    }
}

#[test]
fn the_fields_of_a_date() {
    // 2026-11-22 10:30:15.250 UTC, a Sunday
    let out = run("const $d = time::date(1795343415250, $madrid);\noutln([$d.year, $d.month, $d.day, $d.hour, $d.minute, $d.second, $d.millisecond, $d.weekday]);\noutln($d.zone.name);\noutln($d.millis());");
    assert_eq!(out, "[2026, 11, 22, 11, 30, 15, 250, 7]\nEurope/Madrid\n1795343415250\n");
}

#[test]
fn formats_like_java() {
    let d = "const $d = time::of(2026, 3, 5, 14, 7, 9, 45, $madrid);\n";
    let cases = [
        ("yyyy-MM-dd HH:mm:ss", "2026-03-05 14:07:09"),
        ("d/M/yy H:m:s", "5/3/26 14:7:9"),
        ("EEE, MMM d", "Thu, Mar 5"),
        ("EEEE d MMMM yyyy", "Thursday 5 March 2026"),
        ("h:mm a", "2:07 PM"),
        ("HH:mm:ss.SSS", "14:07:09.045"),
        ("Z|XXX|xxx|VV|z", "+0100|+01:00|+01:00|Europe/Madrid|CET"),
        ("yyyy-MM-dd'T'HH:mm", "2026-03-05T14:07"),
        ("'at' HH 'o''clock', 100%", "at 14 o'clock, 100%"),
    ];
    for (format, expected) in cases {
        assert_eq!(run(&format!("{d}outln($d.format({format:?}));")), format!("{expected}\n"), "{format}");
    }
    assert_eq!(run(&format!("{d}outln($d.format());")), "2026-03-05T14:07:09.045+01:00\n");
    assert_eq!(run("outln(time::of(2026, 3, 5, zone: zone::utc()).format());"), "2026-03-05T00:00:00Z\n");
    assert_eq!(run("outln(time::of(2026, 3, 5, zone: zone::utc()).format('XXX xxx'));"), "Z +00:00\n");
    assert!(error("time::date().format('yyyy Q');").contains("ValueError: 'Q' isn't a part of a date, in the format 'yyyy Q'"));
    assert!(error("time::date().format('yyyy \\'at');").contains("has a quote that isn't closed"));
}

#[test]
fn parses_iso_8601_without_a_format() {
    let cases = [
        ("2026-11-22", "2026-11-22T00:00:00+01:00 Europe/Madrid"),
        ("2026-11-22T10:00", "2026-11-22T10:00:00+01:00 Europe/Madrid"),
        ("2026-11-22T10:00:00Z", "2026-11-22T10:00:00Z UTC"),
        ("2026-11-22T10:00:00.5+05:30", "2026-11-22T10:00:00.500+05:30 +05:30"),
        ("2026-11-22T10:00:00-03:00[America/Sao_Paulo]", "2026-11-22T10:00:00-03:00 America/Sao_Paulo"),
    ];
    for (text, expected) in cases {
        let out = run(&format!("const $d = time::parse({text:?}, zone: $madrid);\noutln(f\"{{$d.format()}} {{$d.zone.name}}\");"));
        assert_eq!(out, format!("{expected}\n"), "{text}");
    }
    // What format() writes, parse() reads back
    assert_eq!(run("const $d = time::date(1795343415250, $madrid);\noutln(time::parse($d.format()) == $d);"), "true\n");
    assert!(error("time::parse('22/11/2026');").contains("ValueError: '22/11/2026' isn't a date in ISO 8601"));
}

#[test]
fn parses_with_a_format() {
    let cases = [
        ("22/11/2026", "dd/MM/yyyy", "2026-11-22T00:00:00+01:00"),
        ("5/3/26 9:05", "d/M/yy H:mm", "2026-03-05T09:05:00+01:00"),
        ("Sunday 22 november 2026, 3:00 pm", "EEEE d MMMM yyyy, h:mm a", "2026-11-22T15:00:00+01:00"),
        ("2026-11-22 10:00 +0530", "yyyy-MM-dd HH:mm Z", "2026-11-22T10:00:00+05:30"),
        ("2026-11-22 10:00 Z", "yyyy-MM-dd HH:mm XXX", "2026-11-22T10:00:00Z"),
        ("2026-11-22 10:00 -03:00", "yyyy-MM-dd HH:mm XXX", "2026-11-22T10:00:00-03:00"),
        ("2026-11-22 10:00 Asia/Tokyo", "yyyy-MM-dd HH:mm VV", "2026-11-22T10:00:00+09:00"),
        ("10:00:00.250 on 2026-11-22", "HH:mm:ss.SSS 'on' yyyy-MM-dd", "2026-11-22T10:00:00.250+01:00"),
    ];
    for (text, format, expected) in cases {
        let out = run(&format!("outln(time::parse({text:?}, {format:?}, $madrid).format());"));
        assert_eq!(out, format!("{expected}\n"), "{text} as {format}");
    }
    assert!(error("time::parse('2026/11', 'yyyy-MM');").contains("ValueError: '2026/11' isn't a date in the format 'yyyy-MM'"));
    assert!(error("time::parse('10:00', 'HH:mm');").contains("ValueError"));
    assert!(error("time::parse('2026 CET', 'yyyy z');").contains("a date can't be read with 'z'"));
}

#[test]
fn zones() {
    assert_eq!(run("outln(time::date(1795343415250, $madrid).inZone(zone::named('America/Lima')).format('yyyy-MM-dd HH:mm VV'));"), "2026-11-22 05:30 America/Lima\n");
    assert_eq!(run("outln([zone::utc() == zone::named('UTC'), $madrid == zone::named('Europe/Madrid'), $madrid == zone::utc()]);"), "[true, true, false]\n");
    assert_eq!(run("outln(zone::named('+05:30').name);"), "+05:30\n");
    assert_eq!(run("const $all = zone::all();\noutln([$all.length() > 300, 'Europe/Madrid' -in $all]);"), "[true, true]\n");
    assert!(error("zone::named('Mars/Base');").contains("ValueError: there's no time zone 'Mars/Base'"));
    assert!(error("zone::named('+25:00');").contains("ValueError: '+25:00' isn't an offset"));
}

#[test]
fn comparing_and_sorting() {
    let out = run("const $a = time::parse('2026-11-22T10:00:00Z');\nconst $b = time::parse('2026-11-22T11:00:00+01:00');\nconst $c = time::of(2025, 1, 1, zone: $madrid);\noutln([$a == $b, $c < $a, $a > $c, $a != $c]);\noutln([$a, $c].sort().map(($d) -> $d.year));");
    assert_eq!(out, "[true, true, true, true]\n[2025, 2026]\n");
}

#[test]
fn dates_that_dont_exist() {
    assert!(error("time::of(2026, 2, 30);").contains("ValueError: there's no date 2026-02-30"));
    assert!(error("time::of(2026, 13, 1);").contains("ValueError"));
    assert!(error("time::of(2026, 1.5, 1);").contains("ValueError: the parts of a date are whole numbers"));
    // When the clocks go forward, a time in the gap moves forward
    assert_eq!(run("outln(time::of(2026, 3, 29, 2, 30, zone: $madrid).format());"), "2026-03-29T03:30:00+02:00\n");
}

#[test]
fn the_native_part_is_hidden() {
    assert!(error("use std/time/native;").contains("there's no module `std/time/native`"));
}
