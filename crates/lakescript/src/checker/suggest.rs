//! "Did you mean": the name an error about a name that isn't there suggests
//! *(see Did you mean in ideas/tooling/diagnostics.md)*. Lakescript's name
//! for one of another language first, then the closest one, or none.

/// What kind of name it is, for the names of other languages.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Variable,
    Function,
    Method,
    Type,
    Member,
    Module,
}

/// Lakescript's names for the ones other languages use most, by kind.
fn other_languages(kind: Kind, name: &str) -> &'static [&'static str] {
    match kind {
        Kind::Method => match name {
            "toUpperCase" | "toUpper" | "upcase" | "uppercase" | "to_uppercase" | "toUppercase" => &["upper"],
            "toLowerCase" | "toLower" | "downcase" | "lowercase" | "to_lowercase" | "toLowercase" => &["lower"],
            "size" | "len" | "count" | "length?" => &["length"],
            "strip" => &["trim"],
            "lstrip" | "trimLeft" | "trim_start" => &["trimStart"],
            "rstrip" | "trimRight" | "trim_end" => &["trimEnd"],
            "includes" | "include?" | "has" | "has?" | "contains?" => &["contains", "has"],
            "append" | "add" | "push_back" | "pushBack" => &["push"],
            "startswith" | "starts_with" | "start_with?" | "startsWith?" => &["startsWith"],
            "endswith" | "ends_with" | "end_with?" | "endsWith?" => &["endsWith"],
            "to_s" | "str" | "toStr" | "to_string" | "string" => &["toString"],
            "to_i" | "to_f" | "parseInt" | "parseFloat" | "toInt" | "toFloat" | "to_number" => &["toNumber"],
            "isEmpty" | "is_empty" | "empty" => &["empty?"],
            _ => &[],
        },
        Kind::Function => match name {
            "print" | "println" | "puts" | "echo" | "printf" => &["outln"],
            "eprint" | "eprintln" => &["errln"],
            _ => &[],
        },
        Kind::Type => match name {
            "str" | "String" | "text" => &["string"],
            "int" | "float" | "double" | "Number" | "Integer" | "num" | "i64" | "f64" => &["number"],
            "boolean" | "Boolean" | "Bool" => &["bool"],
            "array" | "Array" | "List" | "vec" | "Vec" => &["list"],
            "dict" | "Map" | "HashMap" | "object" | "Dict" => &["map"],
            _ => &[],
        },
        _ => &[],
    }
}

/// A function of another language that's a method in Lakescript *(i.e.
/// `len($x)`)*: the method, which the message shows on a value.
pub(super) fn method_for_function(name: &str) -> Option<&'static str> {
    match name {
        "len" | "size" | "count" | "length" => Some("length"),
        "str" | "string" => Some("toString"),
        "int" | "float" | "number" => Some("toNumber"),
        _ => None,
    }
}

/// The name to suggest for `name`, of the ones that are there.
pub(super) fn suggest(name: &str, kind: Kind, there: &[String]) -> Option<String> {
    let is = |n: &str| there.iter().any(|t| t == n);
    if let Some(found) = other_languages(kind, name).iter().find(|n| is(n)) {
        return Some(found.to_string());
    }
    // A question without its `?` *(i.e. `fs::exists` for `fs::exists?`)*
    let question = format!("{name}?");
    if !name.ends_with('?') && is(&question) {
        return Some(question);
    }
    closest(name, there)
}

/// The closest of `there` to `name`: a few letters added, removed, changed
/// or swapped, case aside. None when nothing is close enough.
pub(super) fn closest(name: &str, there: &[String]) -> Option<String> {
    let limit = match name.chars().count() {
        // One letter changed in a name of one means nothing: only the case
        0..=1 => 0,
        2..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    let lower = name.to_lowercase();
    let mut best: Option<(usize, &String)> = None;
    for candidate in there {
        if candidate == name {
            continue;
        }
        let d = distance(&lower, &candidate.to_lowercase());
        if d > limit {
            continue;
        }
        let better = match best {
            None => true,
            Some((bd, bn)) => d < bd || (d == bd && candidate < bn),
        };
        if better {
            best = Some((d, candidate));
        }
    }
    best.map(|(_, n)| n.clone())
}

/// The edits between two names: letters added, removed, changed, or two
/// next to each other swapped *(the optimal string alignment distance)*.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn close_names() {
        let there = names(&["length", "lower", "upper", "trim", "split"]);
        assert_eq!(closest("lenght", &there).as_deref(), Some("length"));
        assert_eq!(closest("Upper", &there).as_deref(), Some("upper"));
        assert_eq!(closest("xyz", &there), None);
        // A short name only gets one letter of difference
        assert_eq!(closest("trm", &there).as_deref(), Some("trim"));
        assert_eq!(closest("tr", &there), None);
        // A name of one letter only in another case
        assert_eq!(closest("y", &names(&["x", "Y"])).as_deref(), Some("Y"));
    }

    #[test]
    fn other_languages_first() {
        let there = names(&["upper", "lower", "length", "outln", "out", "exists?", "contains"]);
        assert_eq!(suggest("toUpperCase", Kind::Method, &there).as_deref(), Some("upper"));
        assert_eq!(suggest("size", Kind::Method, &there).as_deref(), Some("length"));
        assert_eq!(suggest("includes", Kind::Method, &there).as_deref(), Some("contains"));
        assert_eq!(suggest("print", Kind::Function, &there).as_deref(), Some("outln"));
        assert_eq!(suggest("exists", Kind::Member, &there).as_deref(), Some("exists?"));
        // Only when it's there for that value
        assert_eq!(suggest("strip", Kind::Method, &there), None);
    }
}
