//! The methods of the built-in types, as listed in `ideas/stdlib/types/`.
//! The ones that need regexes or randomness come later.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::exceptions::{INDEX_ERROR, TYPE_ERROR, VALUE_ERROR};
use crate::interpreter::{Exec, Flow, Interpreter, error, error_as, flow_error_as, op_error};
use crate::token::{Span, TokenKind};
use crate::value::{Function, Key, Range, ResultValue, Value, format_number, whole};

/// The arguments of a method call, with helpers that give good errors.
struct Args {
    method: String,
    values: Vec<Value>,
    span: Span,
}

impl Args {
    fn fail<T>(&self, message: impl Into<String>) -> Exec<T> {
        error(message, self.span)
    }

    fn count(&self, min: usize, max: usize) -> Exec<()> {
        let n = self.values.len();
        if n >= min && n <= max {
            return Ok(());
        }
        let expected = match (min, max) {
            (a, b) if a == b => format!("{a} argument{}", if a == 1 { "" } else { "s" }),
            (a, b) => format!("{a} to {b} arguments"),
        };
        self.fail_as(TYPE_ERROR, format!("`.{}()` takes {expected}, but got {n}", self.method))
    }

    fn get(&self, i: usize) -> Option<&Value> {
        self.values.get(i)
    }

    fn fail_as<T>(&self, kind: &str, message: impl Into<String>) -> Exec<T> {
        error_as(kind, message, self.span)
    }

    fn wrong<T>(&self, i: usize, expected: &str) -> Exec<T> {
        let got = self.values.get(i).map(|v| v.type_name()).unwrap_or("nothing");
        self.fail_as(TYPE_ERROR, format!("argument {} of `.{}()` must be {expected}, not a {got}", i + 1, self.method))
    }

    fn str(&self, i: usize) -> Exec<Rc<str>> {
        match self.get(i) {
            Some(Value::Str(s)) => Ok(s.clone()),
            _ => self.wrong(i, "a string"),
        }
    }

    fn str_or(&self, i: usize, default: &str) -> Exec<Rc<str>> {
        if self.get(i).is_none() { Ok(Rc::from(default)) } else { self.str(i) }
    }

    fn num(&self, i: usize) -> Exec<f64> {
        match self.get(i).and_then(Value::as_f64) {
            Some(n) => Ok(n),
            None => self.wrong(i, "a number"),
        }
    }

    fn whole(&self, i: usize) -> Exec<i64> {
        match self.get(i).and_then(Value::as_whole) {
            Some(n) => Ok(n),
            None => self.wrong(i, "a whole number"),
        }
    }

    fn whole_or(&self, i: usize, default: i64) -> Exec<i64> {
        if self.get(i).is_none() { Ok(default) } else { self.whole(i) }
    }

    fn size(&self, i: usize) -> Exec<usize> {
        let n = self.whole(i)?;
        if n < 0 {
            return self.fail(format!("argument {} of `.{}()` can't be negative", i + 1, self.method));
        }
        Ok(n as usize)
    }

    fn func(&self, i: usize) -> Exec<Rc<Function>> {
        match self.get(i) {
            Some(Value::Function(f)) => Ok(f.clone()),
            _ => self.wrong(i, "a function"),
        }
    }

    fn list(&self, i: usize) -> Exec<Vec<Value>> {
        match self.get(i) {
            Some(Value::List(l)) => Ok(l.borrow().clone()),
            _ => self.wrong(i, "a list"),
        }
    }

    fn map(&self, i: usize) -> Exec<IndexMap<Key, Value>> {
        match self.get(i) {
            Some(Value::Map(m)) => Ok(m.borrow().clone()),
            _ => self.wrong(i, "a map"),
        }
    }

    fn value(&self, i: usize) -> Value {
        self.values.get(i).cloned().unwrap_or(Value::Null)
    }
}

pub fn call(interp: &mut Interpreter, receiver: Value, method: &str, values: Vec<Value>, span: Span) -> Exec<Value> {
    let args = Args { method: method.to_string(), values, span };

    // Methods every value has
    match method {
        "toString" if !matches!(receiver, Value::Int(_) | Value::Float(_)) => {
            args.count(0, 0)?;
            return Ok(Value::string(&receiver.to_string()));
        }
        "equals" => {
            args.count(1, 1)?;
            return Ok(Value::Bool(receiver.equals(&args.value(0))));
        }
        "typeName" => {
            args.count(0, 0)?;
            if let Value::Exception(e) = &receiver {
                return Ok(Value::Str(e.kind.clone()));
            }
            return Ok(Value::string(receiver.type_name()));
        }
        _ => {}
    }

    let result = match &receiver {
        Value::Str(s) => string_method(interp, s, &args)?,
        Value::Regex(r) => match method {
            "pattern" => Some(string(&r.pattern)),
            "flags" => Some(string(&r.flags)),
            _ => None,
        },
        Value::Int(_) | Value::Float(_) => number_method(interp, &receiver, &args)?,
        Value::Bool(b) => match method {
            "toNumber" => {
                args.count(0, 0)?;
                Some(Value::Int(*b as i64))
            }
            _ => None,
        },
        Value::Symbol(name) => match method {
            "name" => {
                args.count(0, 0)?;
                Some(Value::Str(name.clone()))
            }
            _ => None,
        },
        Value::List(list) => list_method(interp, &receiver, list, &args)?,
        Value::Map(map) => map_method(interp, &receiver, map, &args)?,
        Value::Range(range) => range_method(interp, *range, &args)?,
        Value::Exception(e) => match method {
            "message" => Some(string(&e.message)),
            "line" => Some(Value::Int(e.span.line as i64)),
            _ => None,
        },
        Value::Result(r) => result_method(interp, r, &args)?,
        _ => None,
    };
    match result {
        Some(value) => Ok(value),
        None => error_as(TYPE_ERROR, format!("a {} has no method `.{method}()`", receiver.type_name()), span),
    }
}

fn call_fn(interp: &mut Interpreter, f: &Rc<Function>, args: Vec<Value>, span: Span) -> Exec<Value> {
    interp.call(f, args, Vec::new(), span)
}

fn call_bool(interp: &mut Interpreter, f: &Rc<Function>, args: Vec<Value>, span: Span) -> Exec<bool> {
    match call_fn(interp, f, args, span)? {
        Value::Bool(b) => Ok(b),
        other => error(format!("the function must return a bool, but it returned a {}", other.type_name()), span),
    }
}

/// A number from a float, as an integer when it's whole.
fn number(f: f64) -> Value {
    match whole(f) {
        Some(n) if f != 0.0 || f.is_sign_positive() => Value::Int(n),
        _ => Value::Float(f),
    }
}

fn int(n: usize) -> Value {
    Value::Int(n as i64)
}

fn string(s: impl AsRef<str>) -> Value {
    Value::string(s.as_ref())
}

fn nullable(v: Option<Value>) -> Value {
    v.unwrap_or(Value::Null)
}

/// The order of two values for sorting: numbers with numbers, strings with strings.
fn order(a: &Value, b: &Value) -> Result<Ordering, String> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(x.cmp(y)),
        (Value::Str(x), Value::Str(y)) => Ok(x.cmp(y)),
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => Ok(x.partial_cmp(&y).unwrap_or(Ordering::Equal)),
            _ => Err(format!("can't order a {} and a {}: only numbers with numbers and strings with strings", a.type_name(), b.type_name())),
        },
    }
}

/// Sorts by keys, stopping at the first pair that can't be ordered.
fn sort_by_keys(items: &mut Vec<Value>, keys: Vec<Value>, span: Span) -> Exec<()> {
    let mut indexed: Vec<(Value, Value)> = keys.into_iter().zip(items.drain(..)).collect();
    let mut failure = None;
    indexed.sort_by(|(a, _), (b, _)| match order(a, b) {
        Ok(o) => o,
        Err(e) => {
            failure.get_or_insert(e);
            Ordering::Equal
        }
    });
    if let Some(message) = failure {
        return error(message, span);
    }
    items.extend(indexed.into_iter().map(|(_, v)| v));
    Ok(())
}

// ---- string ----

fn char_index(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

fn words_of(s: &str) -> Vec<String> {
    // Splits `userName`, `user_name`, `user-name` and `user name` into lowercase words
    let mut words = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        let boundary = c.is_uppercase()
            && !current.is_empty()
            && (prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit())
                || (prev.is_some_and(char::is_uppercase) && next.is_some_and(char::is_lowercase)));
        if boundary {
            words.push(std::mem::take(&mut current));
        }
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn trim_chars<'a>(s: &'a str, chars: &Option<Rc<str>>, start: bool, end: bool) -> &'a str {
    let matches = |c: char| match chars {
        Some(set) => set.contains(c),
        None => c.is_whitespace(),
    };
    let mut out = s;
    if start {
        out = out.trim_start_matches(matches);
    }
    if end {
        out = out.trim_end_matches(matches);
    }
    out
}

fn pad(s: &str, length: usize, fill: &str, start: bool, end: bool) -> String {
    let len = s.chars().count();
    if len >= length || fill.is_empty() {
        return s.to_string();
    }
    let missing = length - len;
    let (left, right) = match (start, end) {
        (true, true) => (missing / 2, missing - missing / 2),
        (true, false) => (missing, 0),
        _ => (0, missing),
    };
    let fill: Vec<char> = fill.chars().collect();
    let make = |n: usize| (0..n).map(|i| fill[i % fill.len()]).collect::<String>();
    format!("{}{s}{}", make(left), make(right))
}

/// Reads a number in any of the literal forms: `42`, `-2.5`, `0xFF`, `1_000`, `1e6`.
pub fn parse_number(text: &str) -> Option<Value> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    if !digits.starts_with(|c: char| c.is_ascii_digit()) {
        return match digits {
            "Infinity" => Some(Value::Float(if negative { f64::NEG_INFINITY } else { f64::INFINITY })),
            "NaN" if !negative => Some(Value::Float(f64::NAN)),
            _ => None,
        };
    }
    let tokens = crate::lexer::tokenize(digits).ok()?;
    let value = match tokens.as_slice() {
        [t, eof] if eof.kind == TokenKind::Eof => match t.kind {
            TokenKind::Int(n) => Value::Int(n),
            TokenKind::Float(f) => Value::Float(f),
            _ => return None,
        },
        _ => return None,
    };
    if !negative {
        return Some(value);
    }
    Some(match value {
        Value::Int(n) => Value::Int(-n),
        Value::Float(f) => Value::Float(-f),
        other => other,
    })
}

fn captures_list(caps: &regex::Captures) -> Value {
    Value::list(caps.iter().map(|m| string(m.map(|m| m.as_str()).unwrap_or(""))).collect())
}

fn string_method(interp: &mut Interpreter, s: &Rc<str>, a: &Args) -> Exec<Option<Value>> {
    // The methods that also take a regex
    let regex_arg = match a.get(0) {
        Some(Value::Regex(r)) => Some(r.clone()),
        _ => None,
    };
    let method = a.method.as_str();
    if let Some(re) = &regex_arg {
        let re = &re.compiled;
        let result = match method {
            "replace" | "replaceFirst" | "replaceLast" => {
                a.count(2, 2)?;
                let with = a.str(1)?;
                match method {
                    "replace" => string(re.replace_all(s, &*with)),
                    "replaceFirst" => string(re.replacen(s, 1, &*with)),
                    _ => match re.captures_iter(s).last() {
                        Some(caps) => {
                            let whole = caps.get(0).unwrap();
                            let mut replacement = String::new();
                            caps.expand(&with, &mut replacement);
                            string(format!("{}{replacement}{}", &s[..whole.start()], &s[whole.end()..]))
                        }
                        None => string(&**s),
                    },
                }
            }
            "remove" => {
                a.count(1, 1)?;
                string(re.replace_all(s, ""))
            }
            "split" => {
                a.count(1, 2)?;
                let parts: Vec<Value> = match a.get(1) {
                    None | Some(Value::Null) => re.split(s).map(string).collect(),
                    Some(_) => re.splitn(s, a.size(1)?.max(1)).map(string).collect(),
                };
                Value::list(parts)
            }
            "match" => {
                a.count(1, 1)?;
                nullable(re.captures(s).map(|c| captures_list(&c)))
            }
            "matchAll" => {
                a.count(1, 1)?;
                Value::list(re.captures_iter(s).map(|c| captures_list(&c)).collect())
            }
            _ => return a.wrong(0, "a string"),
        };
        return Ok(Some(result));
    }
    if method == "match" || method == "matchAll" {
        a.count(1, 1)?;
        // A string as the pattern is compiled as a regex, like with `-match`
        let re = interp.as_regex(&a.value(0)).map_err(|m| flow_error_as(VALUE_ERROR, m, a.span))?;
        return Ok(Some(if method == "match" {
            nullable(re.compiled.captures(s).map(|c| captures_list(&c)))
        } else {
            Value::list(re.compiled.captures_iter(s).map(|c| captures_list(&c)).collect())
        }));
    }
    string_method_plain(s, a)
}

fn string_method_plain(s: &Rc<str>, a: &Args) -> Exec<Option<Value>> {
    let method = a.method.as_str();
    let all_chars = |pred: fn(char) -> bool| !s.is_empty() && s.chars().all(pred);
    let has_letters = s.chars().any(char::is_alphabetic);
    Ok(Some(match method {
        // Information
        "length" => {
            a.count(0, 0)?;
            int(s.chars().count())
        }
        "empty?" => Value::Bool(s.is_empty()),
        "blank?" => Value::Bool(s.trim().is_empty()),
        "contains" => {
            a.count(1, 1)?;
            Value::Bool(s.contains(&*a.str(0)?))
        }
        "startsWith" | "hasPrefix" => {
            a.count(1, 1)?;
            Value::Bool(s.starts_with(&*a.str(0)?))
        }
        "endsWith" | "hasSuffix" => {
            a.count(1, 1)?;
            Value::Bool(s.ends_with(&*a.str(0)?))
        }
        "count" => {
            a.count(1, 1)?;
            let part = a.str(0)?;
            if part.is_empty() {
                return a.fail("`.count()` needs a string that isn't empty");
            }
            int(s.matches(&*part).count())
        }
        "equalsIgnoreCase" => {
            a.count(1, 1)?;
            Value::Bool(s.to_lowercase() == a.str(0)?.to_lowercase())
        }
        "upper?" => Value::Bool(has_letters && !s.chars().any(char::is_lowercase)),
        "lower?" => Value::Bool(has_letters && !s.chars().any(char::is_uppercase)),
        "numeric?" => Value::Bool(parse_number(s).is_some()),
        "digits?" => Value::Bool(all_chars(|c| c.is_ascii_digit())),
        "alpha?" => Value::Bool(all_chars(char::is_alphabetic)),
        "alphanumeric?" => Value::Bool(all_chars(char::is_alphanumeric)),

        // Searching
        "indexOf" | "indexOfFirst" => {
            a.count(1, if method == "indexOf" { 2 } else { 1 })?;
            let part = a.str(0)?;
            let from = a.whole_or(1, 0)?.max(0) as usize;
            let skip: usize = s.chars().take(from).map(char::len_utf8).sum();
            nullable(s[skip..].find(&*part).map(|b| int(char_index(s, skip + b))))
        }
        "indexOfLast" => {
            a.count(1, 1)?;
            nullable(s.rfind(&*a.str(0)?).map(|b| int(char_index(s, b))))
        }
        "indexesOf" => {
            a.count(1, 1)?;
            let part = a.str(0)?;
            if part.is_empty() {
                return a.fail("`.indexesOf()` needs a string that isn't empty");
            }
            Value::list(s.match_indices(&*part).map(|(b, _)| int(char_index(s, b))).collect())
        }
        "before" | "after" | "beforeLast" | "afterLast" => {
            a.count(1, 1)?;
            let part = a.str(0)?;
            let found = if method.ends_with("Last") { s.rfind(&*part) } else { s.find(&*part) };
            nullable(found.map(|b| {
                if method.starts_with("before") { string(&s[..b]) } else { string(&s[b + part.len()..]) }
            }))
        }

        // Case
        "upper" => string(s.to_uppercase()),
        "lower" => string(s.to_lowercase()),
        "capitalize" => string(capitalize(s)),
        "uncapitalize" => {
            let mut chars = s.chars();
            string(match chars.next() {
                Some(first) => first.to_lowercase().chain(chars).collect(),
                None => String::new(),
            })
        }
        "capitalizeWords" => {
            let mut out = String::new();
            let mut start = true;
            for c in s.chars() {
                if start && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                } else {
                    out.push(c);
                }
                start = c.is_whitespace();
            }
            string(out)
        }
        "swapCase" => string(
            s.chars()
                .flat_map(|c| -> Vec<char> {
                    if c.is_uppercase() { c.to_lowercase().collect() } else { c.to_uppercase().collect() }
                })
                .collect::<String>(),
        ),
        "camelCase" => {
            let words = words_of(s);
            string(words.iter().enumerate().map(|(i, w)| if i == 0 { w.clone() } else { capitalize(w) }).collect::<String>())
        }
        "pascalCase" => string(words_of(s).iter().map(|w| capitalize(w)).collect::<String>()),
        "snakeCase" => string(words_of(s).join("_")),
        "kebabCase" => string(words_of(s).join("-")),
        "constantCase" => string(words_of(s).join("_").to_uppercase()),

        // Trimming and padding
        "trim" | "trimStart" | "trimEnd" => {
            a.count(0, 1)?;
            let chars = if a.get(0).is_some() { Some(a.str(0)?) } else { None };
            string(trim_chars(s, &chars, method != "trimEnd", method != "trimStart"))
        }
        "removePrefix" => {
            a.count(1, 1)?;
            let p = a.str(0)?;
            string(s.strip_prefix(&*p).unwrap_or(s))
        }
        "removeSuffix" => {
            a.count(1, 1)?;
            let p = a.str(0)?;
            string(s.strip_suffix(&*p).unwrap_or(s))
        }
        "padStart" | "padEnd" | "center" => {
            a.count(1, 2)?;
            let length = a.size(0)?;
            let fill = a.str_or(1, " ")?;
            string(pad(s, length, &fill, method != "padEnd", method != "padStart"))
        }
        "truncate" => {
            a.count(1, 2)?;
            let length = a.size(0)?;
            let ending = a.str_or(1, "...")?;
            if s.chars().count() <= length {
                string(&**s)
            } else {
                let keep = length.saturating_sub(ending.chars().count());
                string(s.chars().take(keep).collect::<String>() + &ending)
            }
        }

        // Changing
        "replace" | "replaceFirst" | "replaceLast" => {
            a.count(2, 2)?;
            let (find, with) = (a.str(0)?, a.str(1)?);
            if find.is_empty() {
                return a.fail(format!("`.{method}()` needs a string to find that isn't empty"));
            }
            string(match method {
                "replace" => s.replace(&*find, &with),
                "replaceFirst" => s.replacen(&*find, &with, 1),
                _ => match s.rfind(&*find) {
                    Some(b) => format!("{}{with}{}", &s[..b], &s[b + find.len()..]),
                    None => s.to_string(),
                },
            })
        }
        "remove" => {
            a.count(1, 1)?;
            let find = a.str(0)?;
            if find.is_empty() {
                return a.fail("`.remove()` needs a string that isn't empty");
            }
            string(s.replace(&*find, ""))
        }
        "insert" => {
            a.count(2, 2)?;
            let index = a.size(0)?;
            let text = a.str(1)?;
            let len = s.chars().count();
            if index > len {
                return a.fail_as(INDEX_ERROR, format!("index {index} is past the end of the string, which has {len} characters"));
            }
            let byte: usize = s.chars().take(index).map(char::len_utf8).sum();
            string(format!("{}{text}{}", &s[..byte], &s[byte..]))
        }
        "repeat" => {
            a.count(1, 1)?;
            string(s.repeat(a.size(0)?))
        }
        "reverse" => string(s.chars().rev().collect::<String>()),
        "collapseWhitespace" => string(s.split_whitespace().collect::<Vec<_>>().join(" ")),
        "indent" => {
            a.count(0, 1)?;
            let spaces = " ".repeat(a.whole_or(0, 4)?.max(0) as usize);
            string(s.split('\n').map(|l| if l.trim().is_empty() { l.to_string() } else { format!("{spaces}{l}") }).collect::<Vec<_>>().join("\n"))
        }

        // Splitting
        "split" => {
            a.count(1, 2)?;
            let sep = a.str(0)?;
            if sep.is_empty() {
                return a.fail("`.split()` needs a separator that isn't empty. Use `.chars()` to split into characters");
            }
            let parts: Vec<Value> = match a.get(1) {
                None | Some(Value::Null) => s.split(&*sep).map(string).collect(),
                Some(_) => {
                    let limit = a.size(1)?;
                    if limit == 0 {
                        return a.fail("the limit of `.split()` must be at least 1");
                    }
                    s.splitn(limit, &*sep).map(string).collect()
                }
            };
            Value::list(parts)
        }
        "splitOnce" => {
            a.count(1, 1)?;
            let sep = a.str(0)?;
            nullable(s.split_once(&*sep).map(|(x, y)| Value::list(vec![string(x), string(y)])))
        }
        "lines" => Value::list(s.lines().map(string).collect()),
        "words" => Value::list(s.split_whitespace().map(string).collect()),
        "chars" => Value::list(s.chars().map(|c| string(c.to_string())).collect()),

        // Converting
        "toNumber" => match parse_number(s) {
            Some(n) => n,
            None => return a.fail_as(VALUE_ERROR, format!("'{s}' isn't a number")),
        },
        "toBool" => match s.to_lowercase().as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return a.fail_as(VALUE_ERROR, format!("'{s}' isn't `true` or `false`")),
        },
        "toSymbol" => {
            let valid = s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !valid {
                return a.fail_as(VALUE_ERROR, format!("'{s}' isn't a valid symbol name"));
            }
            Value::Symbol(s.clone())
        }
        "escapeRegex" => string(regex::escape(s)),
        "codePoint" => {
            a.count(0, 1)?;
            let index = a.whole_or(0, 0)?;
            let len = s.chars().count() as i64;
            let pos = if index < 0 { len + index } else { index };
            match s.chars().nth(pos.max(0) as usize) {
                Some(c) if pos >= 0 => int(c as usize),
                _ => return a.fail_as(INDEX_ERROR, format!("index {index} is outside the string, which has {len} characters")),
            }
        }
        _ => return Ok(None),
    }))
}

// ---- number ----

fn number_method(interp: &mut Interpreter, n: &Value, a: &Args) -> Exec<Option<Value>> {
    let method = a.method.as_str();
    let f = n.as_f64().unwrap();
    let w = n.as_whole();
    Ok(Some(match method {
        "even?" => Value::Bool(w.is_some_and(|x| x % 2 == 0)),
        "odd?" => Value::Bool(w.is_some_and(|x| x % 2 != 0)),
        "whole?" => Value::Bool(w.is_some()),
        "positive?" => Value::Bool(f > 0.0),
        "negative?" => Value::Bool(f < 0.0),
        "zero?" => Value::Bool(f == 0.0),
        "nan?" => Value::Bool(f.is_nan()),
        "infinite?" => Value::Bool(f.is_infinite()),
        "finite?" => Value::Bool(f.is_finite()),
        "between" => {
            a.count(2, 2)?;
            Value::Bool(a.num(0)? <= f && f <= a.num(1)?)
        }
        "round" => {
            a.count(0, 1)?;
            let decimals = a.whole_or(0, 0)?;
            if let (Some(x), true) = (w, decimals >= 0) {
                Value::Int(x)
            } else {
                let factor = 10f64.powi(decimals as i32);
                number((f * factor).round() / factor)
            }
        }
        "floor" => number(f.floor()),
        "ceil" => number(f.ceil()),
        "truncate" => number(f.trunc()),
        "abs" => match n {
            Value::Int(x) => x.checked_abs().map(Value::Int).unwrap_or(Value::Float((*x as f64).abs())),
            _ => Value::Float(f.abs()),
        },
        "sign" => Value::Int(if f > 0.0 { 1 } else if f < 0.0 { -1 } else { 0 }),
        "min" => {
            a.count(1, 1)?;
            let other = a.num(0)?;
            if other < f { a.value(0) } else { n.clone() }
        }
        "max" => {
            a.count(1, 1)?;
            let other = a.num(0)?;
            if other > f { a.value(0) } else { n.clone() }
        }
        "clamp" => {
            a.count(2, 2)?;
            let (low, high) = (a.num(0)?, a.num(1)?);
            if low > high {
                return a.fail("the minimum of `.clamp()` is bigger than the maximum");
            }
            if f < low { a.value(0) } else if f > high { a.value(1) } else { n.clone() }
        }
        "div" | "mod" => {
            a.count(1, 1)?;
            let by = a.num(0)?;
            if by == 0.0 {
                return a.fail_as(crate::exceptions::ARITHMETIC_ERROR, if method == "div" { "division by zero" } else { "remainder by zero" });
            }
            match (w, a.get(0).and_then(Value::as_whole)) {
                (Some(x), Some(y)) if method == "div" => x.checked_div(y).map(Value::Int).unwrap_or(number((f / by).trunc())),
                (Some(x), Some(y)) => {
                    // The sign of the result follows `$by`, unlike `%`
                    let r = x.wrapping_rem(y);
                    Value::Int(if r != 0 && (r < 0) != (y < 0) { r + y } else { r })
                }
                _ if method == "div" => number((f / by).trunc()),
                _ => number(f - by * (f / by).floor()),
            }
        }
        "sqrt" => Value::Float(f.sqrt()),
        "log" => {
            a.count(0, 1)?;
            match a.get(0) {
                None | Some(Value::Null) => Value::Float(f.ln()),
                Some(_) => number(f.ln() / a.num(0)?.ln()),
            }
        }
        "toString" => {
            a.count(0, 1)?;
            let base = a.whole_or(0, 10)?;
            if base == 10 {
                string(n.to_string())
            } else {
                if !(2..=36).contains(&base) {
                    return a.fail("the base must be between 2 and 36");
                }
                let Some(mut x) = w else {
                    return a.fail("only whole numbers can be written in another base");
                };
                let negative = x < 0;
                let mut digits = Vec::new();
                loop {
                    let d = (x % base).unsigned_abs() as u32;
                    digits.push(char::from_digit(d, base as u32).unwrap());
                    x /= base;
                    if x == 0 {
                        break;
                    }
                }
                if negative {
                    digits.push('-');
                }
                string(digits.iter().rev().collect::<String>())
            }
        }
        "toFixed" => {
            a.count(1, 1)?;
            string(format!("{:.*}", a.size(0)?, f))
        }
        "withSeparators" => {
            a.count(0, 1)?;
            let sep = a.str_or(0, ",")?;
            let text = format_number(f);
            let (sign, rest) = text.strip_prefix('-').map(|r| ("-", r)).unwrap_or(("", &text));
            let (int_part, frac) = rest.split_once('.').map(|(i, d)| (i, Some(d))).unwrap_or((rest, None));
            let digits: Vec<char> = int_part.chars().collect();
            let mut grouped = String::new();
            for (i, c) in digits.iter().enumerate() {
                if i > 0 && (digits.len() - i) % 3 == 0 {
                    grouped.push_str(&sep);
                }
                grouped.push(*c);
            }
            string(match frac {
                Some(d) => format!("{sign}{grouped}.{d}"),
                None => format!("{sign}{grouped}"),
            })
        }
        "formatBytes" => {
            const UNITS: [&str; 6] = ["bytes", "KB", "MB", "GB", "TB", "PB"];
            let mut size = f;
            let mut unit = 0;
            while size.abs() >= 1024.0 && unit < UNITS.len() - 1 {
                size /= 1024.0;
                unit += 1;
            }
            if unit == 0 {
                string(format!("{} bytes", format_number(size)))
            } else {
                string(format!("{} {}", format_number((size * 10.0).round() / 10.0), UNITS[unit]))
            }
        }
        "toChar" => match w.and_then(|x| u32::try_from(x).ok()).and_then(char::from_u32) {
            Some(c) => string(c.to_string()),
            None => return a.fail(format!("{} isn't a valid unicode code", n)),
        },
        "times" => {
            a.count(1, 1)?;
            let f = a.func(0)?;
            for i in 0..w.unwrap_or(0).max(0) {
                call_fn(interp, &f, vec![Value::Int(i)], a.span)?;
            }
            Value::Null
        }
        _ => return Ok(None),
    }))
}

// ---- list ----

fn list_method(interp: &mut Interpreter, receiver: &Value, list: &Rc<RefCell<Vec<Value>>>, a: &Args) -> Exec<Option<Value>> {
    let method = a.method.as_str();
    let span = a.span;
    let items = || list.borrow().clone();
    let len = list.borrow().len();
    let position = |i: i64, allow_end: bool| -> Exec<usize> {
        let pos = if i < 0 { len as i64 + i } else { i };
        let max = if allow_end { len as i64 } else { len as i64 - 1 };
        if pos < 0 || pos > max {
            return error_as(INDEX_ERROR, format!("index {i} is outside the list, which has {len} elements"), span);
        }
        Ok(pos as usize)
    };
    Ok(Some(match method {
        // Information
        "length" => int(len),
        "empty?" => Value::Bool(len == 0),
        "contains" => {
            a.count(1, 1)?;
            Value::Bool(list.borrow().iter().any(|v| v.equals(&a.value(0))))
        }
        "indexOf" => {
            a.count(1, 1)?;
            nullable(list.borrow().iter().position(|v| v.equals(&a.value(0))).map(int))
        }
        "indexOfLast" => {
            a.count(1, 1)?;
            nullable(list.borrow().iter().rposition(|v| v.equals(&a.value(0))).map(int))
        }
        "count" => {
            a.count(1, 1)?;
            int(list.borrow().iter().filter(|v| v.equals(&a.value(0))).count())
        }
        "countWhere" => {
            let f = a.func(0)?;
            let mut n = 0;
            for item in items() {
                if call_bool(interp, &f, vec![item], span)? {
                    n += 1;
                }
            }
            int(n)
        }

        // Getting
        "first" => nullable(list.borrow().first().cloned()),
        "last" => nullable(list.borrow().last().cloned()),
        "get" => {
            a.count(2, 2)?;
            let i = a.whole(0)?;
            let pos = if i < 0 { len as i64 + i } else { i };
            if pos >= 0 && (pos as usize) < len { list.borrow()[pos as usize].clone() } else { a.value(1) }
        }

        // Adding and removing
        "push" => {
            list.borrow_mut().extend(a.values.iter().cloned());
            receiver.clone()
        }
        "prepend" => {
            list.borrow_mut().splice(0..0, a.values.iter().cloned());
            receiver.clone()
        }
        "insert" => {
            a.count(2, 2)?;
            let pos = position(a.whole(0)?, true)?;
            list.borrow_mut().insert(pos, a.value(1));
            receiver.clone()
        }
        "extend" => {
            a.count(1, 1)?;
            let other = a.list(0)?;
            list.borrow_mut().extend(other);
            receiver.clone()
        }
        "removeFirst" => {
            let mut l = list.borrow_mut();
            if l.is_empty() { Value::Null } else { l.remove(0) }
        }
        "removeLast" => nullable(list.borrow_mut().pop()),
        "removeAt" => {
            a.count(1, 1)?;
            let pos = position(a.whole(0)?, false)?;
            list.borrow_mut().remove(pos)
        }
        "remove" => {
            a.count(1, 1)?;
            let found = list.borrow().iter().position(|v| v.equals(&a.value(0)));
            if let Some(i) = found {
                list.borrow_mut().remove(i);
            }
            Value::Bool(found.is_some())
        }
        "removeAll" => {
            a.count(1, 1)?;
            let before = len;
            list.borrow_mut().retain(|v| !v.equals(&a.value(0)));
            int(before - list.borrow().len())
        }
        "removeWhere" => {
            let f = a.func(0)?;
            let mut keep = Vec::new();
            let mut removed = 0;
            for item in items() {
                if call_bool(interp, &f, vec![item.clone()], span)? {
                    removed += 1;
                } else {
                    keep.push(item);
                }
            }
            *list.borrow_mut() = keep;
            int(removed)
        }
        "clear" => {
            list.borrow_mut().clear();
            Value::Null
        }

        // Ordering
        "sort" | "sorted" | "sortBy" | "sortedBy" => {
            let mut values = items();
            let keys = if method.ends_with("By") {
                let f = a.func(0)?;
                let mut keys = Vec::with_capacity(values.len());
                for v in &values {
                    keys.push(call_fn(interp, &f, vec![v.clone()], span)?);
                }
                keys
            } else {
                values.clone()
            };
            sort_by_keys(&mut values, keys, span)?;
            if method.starts_with("sorted") {
                Value::list(values)
            } else {
                *list.borrow_mut() = values;
                receiver.clone()
            }
        }
        "reverse" => {
            list.borrow_mut().reverse();
            receiver.clone()
        }
        "reversed" => Value::list(items().into_iter().rev().collect()),

        // Transforming
        "map" | "filter" | "reject" | "flatMap" => {
            a.count(1, 1)?;
            let f = a.func(0)?;
            let mut out = Vec::new();
            for item in items() {
                match method {
                    "map" => out.push(call_fn(interp, &f, vec![item], span)?),
                    "flatMap" => match call_fn(interp, &f, vec![item], span)? {
                        Value::List(l) => out.extend(l.borrow().iter().cloned()),
                        other => return error(format!("the function of `.flatMap()` must return a list, not a {}", other.type_name()), span),
                    },
                    _ => {
                        if call_bool(interp, &f, vec![item.clone()], span)? == (method == "filter") {
                            out.push(item);
                        }
                    }
                }
            }
            Value::list(out)
        }
        "flatten" => {
            let mut out = Vec::new();
            for item in items() {
                match item {
                    Value::List(l) => out.extend(l.borrow().iter().cloned()),
                    other => out.push(other),
                }
            }
            Value::list(out)
        }
        "unique" | "uniqueBy" => {
            let f = if method == "uniqueBy" { Some(a.func(0)?) } else { None };
            let mut seen: Vec<Value> = Vec::new();
            let mut out = Vec::new();
            for item in items() {
                let key = match &f {
                    Some(f) => call_fn(interp, f, vec![item.clone()], span)?,
                    None => item.clone(),
                };
                if !seen.iter().any(|s| s.equals(&key)) {
                    seen.push(key);
                    out.push(item);
                }
            }
            Value::list(out)
        }
        "compact" => Value::list(items().into_iter().filter(|v| !matches!(v, Value::Null)).collect()),
        "take" => {
            a.count(1, 1)?;
            Value::list(items().into_iter().take(a.size(0)?).collect())
        }
        "skip" => {
            a.count(1, 1)?;
            Value::list(items().into_iter().skip(a.size(0)?).collect())
        }
        "takeWhile" | "skipWhile" => {
            let f = a.func(0)?;
            let values = items();
            let mut cut = values.len();
            for (i, item) in values.iter().enumerate() {
                if !call_bool(interp, &f, vec![item.clone()], span)? {
                    cut = i;
                    break;
                }
            }
            Value::list(if method == "takeWhile" { values[..cut].to_vec() } else { values[cut..].to_vec() })
        }
        "chunk" => {
            a.count(1, 1)?;
            let size = a.size(0)?;
            if size == 0 {
                return a.fail("the size of `.chunk()` must be at least 1");
            }
            Value::list(items().chunks(size).map(|c| Value::list(c.to_vec())).collect())
        }
        "zip" => {
            a.count(1, 1)?;
            let other = a.list(0)?;
            Value::list(items().into_iter().zip(other).map(|(x, y)| Value::list(vec![x, y])).collect())
        }
        "concat" => {
            a.count(1, 1)?;
            let mut values = items();
            values.extend(a.list(0)?);
            Value::list(values)
        }
        "copy" => Value::list(items()),

        // Grouping
        "groupBy" | "countBy" | "toMap" => {
            let f = a.func(0)?;
            let mut map: IndexMap<Key, Value> = IndexMap::new();
            for item in items() {
                let key_value = call_fn(interp, &f, vec![item.clone()], span)?;
                let Some(key) = Key::from_value(&key_value) else {
                    return error(format!("map keys can be strings, numbers or symbols, not a {}", key_value.type_name()), span);
                };
                match method {
                    "groupBy" => {
                        if let Value::List(group) = map.entry(key).or_insert_with(|| Value::list(Vec::new())) {
                            group.borrow_mut().push(item);
                        }
                    }
                    "countBy" => {
                        let count = map.entry(key).or_insert(Value::Int(0));
                        if let Value::Int(n) = count {
                            *n += 1;
                        }
                    }
                    _ => {
                        map.insert(key, item);
                    }
                }
            }
            Value::Map(Rc::new(RefCell::new(map)))
        }
        "partition" => {
            let f = a.func(0)?;
            let (mut yes, mut no) = (Vec::new(), Vec::new());
            for item in items() {
                if call_bool(interp, &f, vec![item.clone()], span)? { yes.push(item) } else { no.push(item) }
            }
            Value::list(vec![Value::list(yes), Value::list(no)])
        }

        // Searching
        "find" | "findLast" | "findIndex" => {
            let f = a.func(0)?;
            let values = items();
            let order: Box<dyn Iterator<Item = (usize, &Value)>> =
                if method == "findLast" { Box::new(values.iter().enumerate().rev()) } else { Box::new(values.iter().enumerate()) };
            let mut found = Value::Null;
            for (i, item) in order {
                if call_bool(interp, &f, vec![item.clone()], span)? {
                    found = if method == "findIndex" { int(i) } else { item.clone() };
                    break;
                }
            }
            found
        }
        "any" | "all" | "none" => {
            let f = a.func(0)?;
            // Stops as soon as the answer is known
            let mut answer = method != "any";
            for item in items() {
                let hit = call_bool(interp, &f, vec![item], span)?;
                match method {
                    "any" if hit => {
                        answer = true;
                        break;
                    }
                    "all" if !hit => {
                        answer = false;
                        break;
                    }
                    "none" if hit => {
                        answer = false;
                        break;
                    }
                    _ => {}
                }
            }
            Value::Bool(answer)
        }

        // Combining
        "join" => {
            a.count(0, 1)?;
            let sep = a.str_or(0, "")?;
            string(items().iter().map(|v| v.to_string()).collect::<Vec<_>>().join(&sep))
        }
        "reduce" => {
            let f = a.func(0)?;
            let mut values = items().into_iter();
            let Some(mut acc) = values.next() else { return Ok(Some(Value::Null)) };
            for item in values {
                acc = call_fn(interp, &f, vec![acc, item], span)?;
            }
            acc
        }
        "fold" => {
            a.count(2, 2)?;
            let f = a.func(1)?;
            let mut acc = a.value(0);
            for item in items() {
                acc = call_fn(interp, &f, vec![acc, item], span)?;
            }
            acc
        }
        "sum" | "product" | "average" => {
            let values = items();
            let mut acc = if method == "product" { Value::Int(1) } else { Value::Int(0) };
            for v in &values {
                if v.as_f64().is_none() {
                    return error(format!("`.{method}()` only works with lists of numbers, but it has a {}", v.type_name()), span);
                }
                let op = if method == "product" { crate::ast::BinaryOp::Mul } else { crate::ast::BinaryOp::Add };
                acc = crate::interpreter::arithmetic(op, &acc, v).map_err(|m| op_error(m, span))?;
            }
            if method == "average" {
                if values.is_empty() { Value::Null } else { number(acc.as_f64().unwrap() / values.len() as f64) }
            } else {
                acc
            }
        }
        "min" | "max" | "minBy" | "maxBy" => {
            let values = items();
            let keys = if method.ends_with("By") {
                let f = a.func(0)?;
                let mut keys = Vec::new();
                for v in &values {
                    keys.push(call_fn(interp, &f, vec![v.clone()], span)?);
                }
                keys
            } else {
                values.clone()
            };
            let mut best: Option<usize> = None;
            for i in 0..values.len() {
                let better = match best {
                    None => true,
                    Some(b) => {
                        let o = order(&keys[i], &keys[b]).map_err(|m| op_error(m, span))?;
                        if method.starts_with("min") { o == Ordering::Less } else { o == Ordering::Greater }
                    }
                };
                if better {
                    best = Some(i);
                }
            }
            nullable(best.map(|i| values[i].clone()))
        }

        // Looping
        "each" | "eachWithIndex" => {
            let f = a.func(0)?;
            for (i, item) in items().into_iter().enumerate() {
                let args = if method == "each" { vec![item] } else { vec![item, int(i)] };
                call_fn(interp, &f, args, span)?;
            }
            Value::Null
        }
        _ => return Ok(None),
    }))
}

// ---- map ----

fn map_method(interp: &mut Interpreter, receiver: &Value, map: &Rc<RefCell<IndexMap<Key, Value>>>, a: &Args) -> Exec<Option<Value>> {
    let method = a.method.as_str();
    let span = a.span;
    let entries = || map.borrow().clone();
    let key = |i: usize| -> Exec<Key> {
        let v = a.value(i);
        match Key::from_value(&v) {
            Some(k) => Ok(k),
            None => error(format!("map keys can be strings, numbers or symbols, not a {}", v.type_name()), span),
        }
    };
    let new_map = |m: IndexMap<Key, Value>| Value::Map(Rc::new(RefCell::new(m)));
    Ok(Some(match method {
        // Information
        "length" => int(map.borrow().len()),
        "empty?" => Value::Bool(map.borrow().is_empty()),
        "has" => {
            a.count(1, 1)?;
            Value::Bool(Key::from_value(&a.value(0)).is_some_and(|k| map.borrow().contains_key(&k)))
        }
        "hasValue" => {
            a.count(1, 1)?;
            Value::Bool(map.borrow().values().any(|v| v.equals(&a.value(0))))
        }
        "keys" => Value::list(map.borrow().keys().map(Key::to_value).collect()),
        "values" => Value::list(map.borrow().values().cloned().collect()),
        "keyOf" => {
            a.count(1, 1)?;
            nullable(map.borrow().iter().find(|(_, v)| v.equals(&a.value(0))).map(|(k, _)| k.to_value()))
        }

        // Getting
        "get" => {
            a.count(2, 2)?;
            let k = key(0)?;
            map.borrow().get(&k).cloned().unwrap_or_else(|| a.value(1))
        }
        "getOrSet" => {
            a.count(2, 2)?;
            let k = key(0)?;
            map.borrow_mut().entry(k).or_insert_with(|| a.value(1)).clone()
        }
        "first" => nullable(map.borrow().keys().next().map(Key::to_value)),
        "last" => nullable(map.borrow().keys().last().map(Key::to_value)),

        // Changing
        "set" => {
            a.count(2, 2)?;
            let k = key(0)?;
            map.borrow_mut().insert(k, a.value(1));
            receiver.clone()
        }
        "update" => {
            a.count(3, 3)?;
            let k = key(0)?;
            let f = a.func(2)?;
            let old = map.borrow().get(&k).cloned().unwrap_or_else(|| a.value(1));
            let new = call_fn(interp, &f, vec![old], span)?;
            map.borrow_mut().insert(k, new.clone());
            new
        }
        "remove" => {
            a.count(1, 1)?;
            let k = key(0)?;
            nullable(map.borrow_mut().shift_remove(&k))
        }
        "removeWhere" => {
            let f = a.func(0)?;
            let mut keep = IndexMap::new();
            let mut removed = 0;
            for (k, v) in entries() {
                if call_bool(interp, &f, vec![k.to_value(), v.clone()], span)? {
                    removed += 1;
                } else {
                    keep.insert(k, v);
                }
            }
            *map.borrow_mut() = keep;
            int(removed)
        }
        "merge" | "merged" => {
            a.count(1, 1)?;
            let other = a.map(0)?;
            if method == "merge" {
                map.borrow_mut().extend(other);
                receiver.clone()
            } else {
                let mut copy = entries();
                copy.extend(other);
                new_map(copy)
            }
        }
        "clear" => {
            map.borrow_mut().clear();
            Value::Null
        }
        "sortByKey" | "sortedByKey" | "sortBy" | "sortedBy" => {
            let all = entries();
            let mut keys_order = Vec::with_capacity(all.len());
            for (k, v) in &all {
                keys_order.push(if method.ends_with("Key") {
                    k.to_value()
                } else {
                    let f = a.func(0)?;
                    call_fn(interp, &f, vec![k.to_value(), v.clone()], span)?
                });
            }
            let mut pairs: Vec<Value> = all.iter().map(|(k, v)| Value::list(vec![k.to_value(), v.clone()])).collect();
            sort_by_keys(&mut pairs, keys_order, span)?;
            let mut sorted = IndexMap::new();
            for pair in pairs {
                if let Value::List(p) = pair {
                    let p = p.borrow();
                    sorted.insert(Key::from_value(&p[0]).unwrap(), p[1].clone());
                }
            }
            if method.starts_with("sorted") {
                new_map(sorted)
            } else {
                *map.borrow_mut() = sorted;
                receiver.clone()
            }
        }

        // Transforming
        "filter" | "reject" => {
            let f = a.func(0)?;
            let mut out = IndexMap::new();
            for (k, v) in entries() {
                if call_bool(interp, &f, vec![k.to_value(), v.clone()], span)? == (method == "filter") {
                    out.insert(k, v);
                }
            }
            new_map(out)
        }
        "mapValues" => {
            let f = a.func(0)?;
            let mut out = IndexMap::new();
            for (k, v) in entries() {
                out.insert(k, call_fn(interp, &f, vec![v], span)?);
            }
            new_map(out)
        }
        "mapKeys" => {
            let f = a.func(0)?;
            let mut out = IndexMap::new();
            for (k, v) in entries() {
                let new_key = call_fn(interp, &f, vec![k.to_value()], span)?;
                let Some(new_key) = Key::from_value(&new_key) else {
                    return error(format!("map keys can be strings, numbers or symbols, not a {}", new_key.type_name()), span);
                };
                out.insert(new_key, v);
            }
            new_map(out)
        }
        "map" => {
            let f = a.func(0)?;
            let mut out = Vec::new();
            for (k, v) in entries() {
                out.push(call_fn(interp, &f, vec![k.to_value(), v], span)?);
            }
            Value::list(out)
        }
        "invert" => {
            let mut out = IndexMap::new();
            for (k, v) in entries() {
                let Some(new_key) = Key::from_value(&v) else {
                    return error(format!("`.invert()` needs values that can be keys, but one is a {}", v.type_name()), span);
                };
                out.insert(new_key, k.to_value());
            }
            new_map(out)
        }
        "copy" => new_map(entries()),

        // Searching
        "find" => {
            let f = a.func(0)?;
            let mut found = Value::Null;
            for (k, v) in entries() {
                if call_bool(interp, &f, vec![k.to_value(), v], span)? {
                    found = k.to_value();
                    break;
                }
            }
            found
        }
        "any" | "all" | "none" | "countWhere" => {
            let f = a.func(0)?;
            let all = entries();
            let mut hits = 0;
            for (k, v) in &all {
                if call_bool(interp, &f, vec![k.to_value(), v.clone()], span)? {
                    hits += 1;
                    if method == "any" || method == "none" {
                        break;
                    }
                } else if method == "all" {
                    return Ok(Some(Value::Bool(false)));
                }
            }
            match method {
                "any" => Value::Bool(hits > 0),
                "none" => Value::Bool(hits == 0),
                "all" => Value::Bool(true),
                _ => int(hits),
            }
        }

        // Looping
        "each" => {
            let f = a.func(0)?;
            for (k, v) in entries() {
                call_fn(interp, &f, vec![k.to_value(), v], span)?;
            }
            Value::Null
        }
        _ => return Ok(None),
    }))
}

// ---- result ----

fn result_method(interp: &mut Interpreter, result: &ResultValue, a: &Args) -> Exec<Option<Value>> {
    let span = a.span;
    Ok(Some(match (a.method.as_str(), result) {
        ("isOk", r) => Value::Bool(matches!(r, ResultValue::Ok(_))),
        ("isError", r) => Value::Bool(matches!(r, ResultValue::Error(_))),
        ("orElse", r) => {
            a.count(1, 1)?;
            match r {
                ResultValue::Ok(v) => v.clone(),
                ResultValue::Error(_) => a.value(0),
            }
        }
        ("orElseGet", r) => {
            a.count(1, 1)?;
            match r {
                ResultValue::Ok(v) => v.clone(),
                ResultValue::Error(_) => {
                    let f = a.func(0)?;
                    call_fn(interp, &f, Vec::new(), span)?
                }
            }
        }
        ("map", r) => {
            a.count(1, 1)?;
            let f = a.func(0)?;
            match r {
                ResultValue::Ok(v) => Value::Result(Rc::new(ResultValue::Ok(call_fn(interp, &f, vec![v.clone()], span)?))),
                ResultValue::Error(e) => Value::Result(Rc::new(ResultValue::Error(e.clone()))),
            }
        }
        ("orNull", ResultValue::Ok(v)) => v.clone(),
        ("orNull", ResultValue::Error(_)) => Value::Null,
        ("error", ResultValue::Ok(_)) => Value::Null,
        ("error", ResultValue::Error(e)) => e.clone(),
        ("throw", ResultValue::Ok(v)) => v.clone(),
        ("throw", ResultValue::Error(e)) => match e {
            Value::Exception(e) => return Err(Flow::Throw(e.clone())),
            other => return error(other.to_string(), span),
        },
        _ => return Ok(None),
    }))
}

// ---- range ----

fn range_method(interp: &mut Interpreter, range: Range, a: &Args) -> Exec<Option<Value>> {
    let method = a.method.as_str();
    let span = a.span;
    Ok(Some(match method {
        "start" => Value::Int(range.start),
        "end" => Value::Int(range.end),
        "inclusive?" => Value::Bool(range.inclusive),
        "length" => int(range.iter().count()),
        "empty?" => Value::Bool(range.iter().next().is_none()),
        "contains" => {
            a.count(1, 1)?;
            Value::Bool(a.value(0).as_whole().is_some_and(|n| range.contains(n)))
        }
        "first" => nullable(range.iter().next().map(Value::Int)),
        "last" => nullable(range.iter().last().map(Value::Int)),
        "step" => {
            a.count(1, 1)?;
            match a.get(0).and_then(Value::as_whole) {
                Some(step) if step > 0 => Value::Range(Range { step, ..range }),
                _ => return a.fail("the step must be a whole number bigger than 0"),
            }
        }
        "reverse" => {
            // The same numbers in the other direction
            let numbers: Vec<i64> = range.iter().collect();
            match (numbers.first(), numbers.last()) {
                (Some(&first), Some(&last)) => Value::Range(Range { start: last, end: first, inclusive: true, step: range.step }),
                _ => Value::Range(range),
            }
        }
        "toList" => Value::list(range.iter().map(Value::Int).collect()),
        "sum" => {
            let mut acc = Value::Int(0);
            for n in range.iter() {
                acc = crate::interpreter::arithmetic(crate::ast::BinaryOp::Add, &acc, &Value::Int(n))
                    .map_err(|m| op_error(m, span))?;
            }
            acc
        }
        "map" | "filter" | "each" => {
            let f = a.func(0)?;
            let mut out = Vec::new();
            for n in range.iter() {
                match method {
                    "map" => out.push(call_fn(interp, &f, vec![Value::Int(n)], span)?),
                    "filter" => {
                        if call_bool(interp, &f, vec![Value::Int(n)], span)? {
                            out.push(Value::Int(n));
                        }
                    }
                    _ => {
                        call_fn(interp, &f, vec![Value::Int(n)], span)?;
                    }
                }
            }
            if method == "each" { Value::Null } else { Value::list(out) }
        }
        _ => return Ok(None),
    }))
}
