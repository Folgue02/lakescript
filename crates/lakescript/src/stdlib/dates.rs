//! The dates of `std/time/native`, the Rust part of `std/time` and
//! `std/time/zone` *(see `time.rs`, `time.lks` and `ideas/stdlib/time.md`)*,
//! on `jiff`. Dates cross into Rust as their instant in milliseconds and the
//! name of their zone. It's the `dates` feature of the crate.

use jiff::civil::{Date, DateTime, Time};
use jiff::fmt::strtime;
use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, Zoned};

use super::{Args, Exports};
use crate::exceptions::VALUE_ERROR;
use crate::value::{NativeError, Value};

/// The name of the zone of the system when it has no IANA name.
const LOCAL: &str = "local";

pub fn register(m: &mut Exports) {
    m.function("fields", |_, values| {
        let a = Args::new("time::date", values, 2, 2)?;
        let zoned = zoned(a.num(0)?, &a.str(1)?)?;
        let parts = [
            zoned.year() as i64,
            zoned.month() as i64,
            zoned.day() as i64,
            zoned.hour() as i64,
            zoned.minute() as i64,
            zoned.second() as i64,
            zoned.millisecond() as i64,
            zoned.weekday().to_monday_one_offset() as i64,
        ];
        Ok(Value::list(parts.into_iter().map(Value::Int).collect()))
    });
    m.function("format", |_, values| {
        let a = Args::new("Date.format", values, 3, 3)?;
        let zoned = zoned(a.num(0)?, &a.str(1)?)?;
        let text = match a.value(2) {
            Value::Null => iso(&zoned),
            _ => format(&zoned, &a.str(2)?)?,
        };
        Ok(Value::string(&text))
    });
    m.function("of", |_, values| {
        let a = Args::new("time::of", values, 8, 8)?;
        let mut parts = [0i64; 7];
        for (i, part) in parts.iter_mut().enumerate() {
            let n = a.num(i)?;
            if n.fract() != 0.0 {
                return Err(NativeError::new(VALUE_ERROR, format!("the parts of a date are whole numbers, not {}", crate::value::format_number(n))));
            }
            *part = n as i64;
        }
        let [year, month, day, hour, minute, second, ms] = parts;
        let bad = |e: jiff::Error| NativeError::new(VALUE_ERROR, format!("there's no date {year}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{ms:03}: {}", reason(&e)));
        let small = |n: i64| i8::try_from(n).map_err(|_| NativeError::new(VALUE_ERROR, format!("there's no date with a part {n}")));
        let year = i16::try_from(year).map_err(|_| NativeError::new(VALUE_ERROR, format!("the year {year} is out of range")))?;
        let date = Date::new(year, small(month)?, small(day)?).map_err(bad)?;
        let nanos = i32::try_from(ms * 1_000_000).map_err(|_| NativeError::new(VALUE_ERROR, format!("there's no millisecond {ms}")))?;
        let time = Time::new(small(hour)?, small(minute)?, small(second)?, nanos).map_err(bad)?;
        let zone = a.str(7)?;
        let tz = time_zone(&zone)?;
        let zoned = DateTime::from_parts(date, time).to_zoned(tz).map_err(bad)?;
        Ok(Value::Int(zoned.timestamp().as_millisecond()))
    });
    m.function("parse", |_, values| {
        let a = Args::new("time::parse", values, 3, 3)?;
        let text = a.str(0)?;
        let default = a.str(2)?;
        let zoned = match a.value(1) {
            Value::Null => parse_iso(&text, &default)?,
            _ => parse(&text, &a.str(1)?, &default)?,
        };
        Ok(Value::list(vec![Value::Int(zoned.timestamp().as_millisecond()), Value::string(&zone_name(zoned.time_zone()))]))
    });
    m.function("zone", |_, values| {
        let a = Args::new("zone::named", values, 1, 1)?;
        let name = a.str(0)?;
        Ok(Value::string(&zone_name(&time_zone(&name)?)))
    });
    m.function("localZone", |_, _| Ok(Value::string(&zone_name(&TimeZone::system()))));
    m.function("zones", |_, _| {
        let mut names: Vec<String> = jiff::tz::db().available().map(|n| n.to_string()).collect();
        names.sort();
        names.dedup();
        Ok(super::strings(names))
    });
}

/// What jiff says is wrong, without its own context.
fn reason(e: &jiff::Error) -> String {
    e.to_string()
}

/// The zone with that name: `local`, `UTC`, an offset *(i.e. `+05:30`)* or an IANA name.
fn time_zone(name: &str) -> Result<TimeZone, NativeError> {
    if name == LOCAL {
        return Ok(TimeZone::system());
    }
    if name.eq_ignore_ascii_case("UTC") || name == "Z" {
        return Ok(TimeZone::UTC);
    }
    if name.starts_with(['+', '-']) {
        return offset(name).map(TimeZone::fixed).ok_or_else(|| NativeError::new(VALUE_ERROR, format!("'{name}' isn't an offset: offsets look like +01:00 or -0530")));
    }
    jiff::tz::db().get(name).map_err(|_| NativeError::new(VALUE_ERROR, format!("there's no time zone '{name}': zones are named like Europe/Madrid or America/Lima, and zone::all() lists them")))
}

/// An offset like `+01:00`, `+0100` or `+01`.
fn offset(text: &str) -> Option<Offset> {
    let sign = if text.starts_with('-') { -1 } else { 1 };
    let digits: String = text[1..].chars().filter(|c| *c != ':').collect();
    if !digits.chars().all(|c| c.is_ascii_digit()) || !matches!(digits.len(), 2 | 4) {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits.get(2..).filter(|m| !m.is_empty()).map(|m| m.parse().unwrap_or(99)).unwrap_or(0);
    if hours > 18 || minutes > 59 {
        return None;
    }
    Offset::from_seconds(sign * (hours * 3600 + minutes * 60)).ok()
}

/// The name a `Zone` keeps: its IANA name, `UTC`, or its offset.
fn zone_name(tz: &TimeZone) -> String {
    if let Some(name) = tz.iana_name() {
        return name.to_string();
    }
    if tz == &TimeZone::UTC {
        return "UTC".into();
    }
    // A fixed offset is the same at every instant
    match tz.to_fixed_offset() {
        Ok(offset) => offset_text(offset, true),
        Err(_) => LOCAL.into(),
    }
}

/// `+01:00`, or `+0100` without the colon.
fn offset_text(offset: Offset, colon: bool) -> String {
    let seconds = offset.seconds();
    let sign = if seconds < 0 { '-' } else { '+' };
    let (h, m) = (seconds.abs() / 3600, seconds.abs() % 3600 / 60);
    if colon { format!("{sign}{h:02}:{m:02}") } else { format!("{sign}{h:02}{m:02}") }
}

fn zoned(millis: f64, zone: &str) -> Result<Zoned, NativeError> {
    let ts = Timestamp::from_millisecond(millis as i64).map_err(|_| NativeError::new(VALUE_ERROR, format!("{} milliseconds is out of the range of dates", crate::value::format_number(millis))))?;
    Ok(ts.to_zoned(time_zone(zone)?))
}

/// ISO 8601, with the milliseconds when there are some, and `Z` for UTC.
fn iso(zoned: &Zoned) -> String {
    let fraction = if zoned.millisecond() != 0 { format!(".{:03}", zoned.millisecond()) } else { String::new() };
    let offset = if zoned.offset().seconds() == 0 { "Z".to_string() } else { offset_text(zoned.offset(), true) };
    format!("{}{fraction}{offset}", zoned.strftime("%Y-%m-%dT%H:%M:%S"))
}

/// A piece of a format: text as it is, or letters repeated.
enum Piece {
    Text(String),
    Letters(char, usize),
}

/// The pieces of a format with the letters of Java's `DateTimeFormatter`.
fn pieces(format: &str) -> Result<Vec<Piece>, NativeError> {
    let chars: Vec<char> = format.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            // `''` is a quote, and `'...'` text
            if chars.get(i + 1) == Some(&'\'') {
                out.push(Piece::Text("'".into()));
                i += 2;
                continue;
            }
            let mut text = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(NativeError::new(VALUE_ERROR, format!("the format '{format}' has a quote that isn't closed"))),
                    Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                        text.push('\'');
                        i += 2;
                    }
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(c) => {
                        text.push(*c);
                        i += 1;
                    }
                }
            }
            out.push(Piece::Text(text));
        } else if c.is_ascii_alphabetic() {
            let mut n = 1;
            while chars.get(i + n) == Some(&c) {
                n += 1;
            }
            out.push(Piece::Letters(c, n));
            i += n;
        } else {
            out.push(Piece::Text(c.to_string()));
            i += 1;
        }
    }
    Ok(out)
}

/// A letter that isn't in the table of `ideas/stdlib/time.md`.
fn unknown(c: char, n: usize, format: &str) -> NativeError {
    NativeError::new(
        VALUE_ERROR,
        format!("'{}' isn't a part of a date, in the format '{format}': text goes between quotes, like 'at'", c.to_string().repeat(n)),
    )
}

/// The `strftime` of one piece, as `jiff` reads and writes it. `zoned` is the
/// date being written, `None` when reading.
fn directive(c: char, n: usize, format: &str, zoned: Option<&Zoned>) -> Result<String, NativeError> {
    Ok(match (c, n) {
        ('y', 2) => "%y".into(),
        ('y', _) | ('u', _) => "%Y".into(),
        ('M', 1) => "%-m".into(),
        ('M', 2) => "%m".into(),
        ('M', 3) => "%b".into(),
        ('M', 4) => "%B".into(),
        ('d', 1) => "%-d".into(),
        ('d', 2) => "%d".into(),
        ('E', 1..=3) => "%a".into(),
        ('E', 4) => "%A".into(),
        ('H', 1) => "%-H".into(),
        ('H', 2) => "%H".into(),
        ('h', 1) => "%-I".into(),
        ('h', 2) => "%I".into(),
        ('a', 1) => "%p".into(),
        ('m', 1) => "%-M".into(),
        ('m', 2) => "%M".into(),
        ('s', 1) => "%-S".into(),
        ('s', 2) => "%S".into(),
        ('S', 1..=9) => format!("%{n}f"),
        ('Z', 1..=3) => "%z".into(),
        ('x', 1) | ('X', 1) => "%:::z".into(),
        ('x', 2) | ('X', 2) => "%z".into(),
        ('x', 3) | ('X', 3) => "%:z".into(),
        ('z', 1..=4) => match zoned {
            Some(_) => "%Z".into(),
            None => return Err(NativeError::new(VALUE_ERROR, "a date can't be read with 'z': abbreviations like CST mean different zones in different places. 'VV' or an offset like 'XXX' can")),
        },
        ('V', 2) => "%Q".into(),
        _ => return Err(unknown(c, n, format)),
    })
    .map(|d: String| {
        // `X` writes `Z` for UTC
        match (c, zoned) {
            ('X', Some(z)) if z.offset().seconds() == 0 => "Z".into(),
            _ => d,
        }
    })
}

/// Text as it is, in `strftime`.
fn literal(text: &str) -> String {
    text.replace('%', "%%")
}

fn format(zoned: &Zoned, format: &str) -> Result<String, NativeError> {
    let mut spec = String::new();
    for piece in pieces(format)? {
        match piece {
            Piece::Text(t) => spec.push_str(&literal(&t)),
            Piece::Letters(c, n) => spec.push_str(&directive(c, n, format, Some(zoned))?),
        }
    }
    strtime::format(&spec, zoned).map_err(|e| NativeError::new(VALUE_ERROR, format!("can't write the date with '{format}': {}", reason(&e))))
}

/// The formats a pattern means when reading: with `X`, the offset or `Z`.
fn parse_specs(format: &str) -> Result<Vec<String>, NativeError> {
    let mut specs = vec![String::new()];
    for piece in pieces(format)? {
        match piece {
            Piece::Text(t) => specs.iter_mut().for_each(|s| s.push_str(&literal(&t))),
            Piece::Letters('X', n) => {
                let d = directive('X', n, format, None)?;
                specs = specs.into_iter().flat_map(|s| [format!("{s}{d}"), format!("{s}Z")]).collect();
            }
            Piece::Letters(c, n) => {
                let d = directive(c, n, format, None)?;
                specs.iter_mut().for_each(|s| s.push_str(&d));
            }
        }
    }
    Ok(specs)
}

fn parse(text: &str, format: &str, default: &str) -> Result<Zoned, NativeError> {
    let specs = parse_specs(format)?;
    let mut last = None;
    for spec in &specs {
        // A literal `Z` from `X` is UTC
        let utc = spec.ends_with('Z') && !spec.ends_with("%Z") && format.contains('X');
        match strtime::parse(spec, text) {
            Ok(tm) => return from_broken_down(&tm, text, format, if utc { "UTC" } else { default }),
            Err(e) => last = Some(e),
        }
    }
    let why = last.map(|e| reason(&e)).unwrap_or_default();
    Err(NativeError::new(VALUE_ERROR, format!("'{text}' isn't a date in the format '{format}': {why}")))
}

fn from_broken_down(tm: &strtime::BrokenDownTime, text: &str, format: &str, default: &str) -> Result<Zoned, NativeError> {
    let bad = |e: jiff::Error| NativeError::new(VALUE_ERROR, format!("'{text}' isn't a date in the format '{format}': {}", reason(&e)));
    let date = tm.to_date().map_err(bad)?;
    let time = if tm.hour().is_none() { Time::midnight() } else { tm.to_time().map_err(bad)? };
    let tz = match (tm.iana_time_zone(), tm.offset()) {
        (Some(name), _) => time_zone(name)?,
        (None, Some(offset)) => TimeZone::fixed(offset),
        (None, None) => time_zone(default)?,
    };
    let datetime = DateTime::from_parts(date, time);
    match tm.offset() {
        // With an offset, the instant is the one it says
        Some(offset) => {
            let ts = offset.to_timestamp(datetime).map_err(bad)?;
            Ok(ts.to_zoned(tz))
        }
        None => datetime.to_zoned(tz).map_err(bad),
    }
}

/// ISO 8601 and RFC 3339: `2026-11-22`, `2026-11-22T10:00`, with seconds, a
/// fraction, an offset or `Z`, and a zone in brackets.
fn parse_iso(text: &str, default: &str) -> Result<Zoned, NativeError> {
    use jiff::fmt::temporal::{Pieces, PiecesOffset};
    let bad = |why: String| NativeError::new(VALUE_ERROR, format!("'{text}' isn't a date in ISO 8601, like 2026-11-22 or 2026-11-22T10:00:00Z: {why}"));
    let pieces = Pieces::parse(text.trim()).map_err(|e| bad(reason(&e)))?;
    let datetime = DateTime::from_parts(pieces.date(), pieces.time().unwrap_or(Time::midnight()));
    let offset = match pieces.offset() {
        Some(PiecesOffset::Zulu) => Some(Offset::UTC),
        Some(PiecesOffset::Numeric(n)) => Some(n.offset()),
        _ => None,
    };
    let tz = match (pieces.to_time_zone().map_err(|e| bad(reason(&e)))?, offset) {
        (Some(tz), _) => tz,
        (None, Some(o)) if o == Offset::UTC => TimeZone::UTC,
        (None, Some(o)) => TimeZone::fixed(o),
        (None, None) => time_zone(default)?,
    };
    match offset {
        Some(o) => Ok(o.to_timestamp(datetime).map_err(|e| bad(reason(&e)))?.to_zoned(tz)),
        None => datetime.to_zoned(tz).map_err(|e| bad(reason(&e))),
    }
}
