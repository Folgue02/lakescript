//! `Git::parse()` and `Git::help()`: reading a script's arguments with a
//! command *(see `ideas/language/cli.md`)*.

use std::rc::{Rc, Weak};

use crate::bytecode::{ArgInfo, ArgType, CommandInfo};
use crate::exceptions;
use crate::interpreter::{Exec, Flow, Interpreter, error_as};
use crate::methods::parse_number;
use crate::token::Span;
use crate::value::{Class, Value};

/// The exception for arguments that don't match: a `ValueError`, which `lks`
/// prints without the code around it, and exits with code `2`.
pub const USAGE_ERROR: &str = "UsageError";

impl Interpreter<'_> {
    /// Calls `parse()` or `help()` of a command.
    pub(crate) fn command_function(&mut self, class: &Weak<Class>, help: bool, args: Vec<Value>, span: Span) -> Exec<Value> {
        let Some(class) = class.upgrade() else { return Ok(Value::Null) };
        let prog = self.program_name();
        if help {
            return Ok(Value::string(&help_text(&class, &prog)));
        }
        let tokens: Vec<String> = match args.first() {
            None | Some(Value::Null) => self.host.state.args.clone(),
            Some(Value::List(list)) => list.borrow().iter().map(|v| v.to_string()).collect(),
            Some(other) => {
                let message = format!("`{}::parse` takes a list of strings, not a {}", class.name, other.type_label());
                return error_as(exceptions::TYPE_ERROR, message, span);
            }
        };
        self.parse_command(&class, &tokens, &prog, span)
    }

    /// The name of the script, for the help *(i.e. `git.lks`)*.
    fn program_name(&self) -> String {
        self.host
            .state
            .script_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "script".to_string())
    }

    fn parse_command(&mut self, class: &Rc<Class>, tokens: &[String], prog: &str, span: Span) -> Exec<Value> {
        let (info, enums) = class.command.clone().expect("only commands are parsed");
        let usage_error = |message: String| -> Flow {
            let text = format!("{message}\n\n{}", usage(&info, prog));
            Flow::Throw(exceptions::new(USAGE_ERROR, text, span))
        };
        let n = info.args.len();
        let mut given: Vec<Option<Value>> = vec![None; n];
        let mut counts = vec![0i64; n];
        let mut lists: Vec<Vec<Value>> = vec![Vec::new(); n];
        let positional: Vec<usize> = (0..n).filter(|&i| !info.args[i].flag).collect();
        let mut next = 0;
        let mut only_positional = false;
        let mut subcommand = None;
        let mut i = 0;
        while i < tokens.len() {
            let token = &tokens[i];
            i += 1;
            if !only_positional && token == "--" {
                only_positional = true;
                continue;
            }
            if !only_positional && (token == "--help" || token == "-h") {
                let text = help_text(class, prog);
                let _ = writeln!(self.host.output, "{text}");
                return Err(Flow::Exit(0));
            }
            if !only_positional && token.starts_with("--") {
                let body = &token[2..];
                let (name, inline) = match body.split_once('=') {
                    Some((n, v)) => (n, Some(v.to_string())),
                    None => (body, None),
                };
                let (index, negated) = match find_flag(&info, name) {
                    Some(i) => (i, false),
                    None => match name.strip_prefix("no-").and_then(|n| find_flag(&info, n)).filter(|&i| info.args[i].kind == ArgType::Bool) {
                        Some(i) => (i, true),
                        None => return Err(usage_error(format!("unknown option `--{name}`"))),
                    },
                };
                let arg = &info.args[index];
                if arg.kind == ArgType::Bool || arg.count {
                    if inline.is_some() {
                        return Err(usage_error(format!("`--{name}` doesn't take a value")));
                    }
                    if arg.count {
                        counts[index] += 1;
                    } else {
                        given[index] = Some(Value::Bool(!negated));
                    }
                    continue;
                }
                let text = match inline {
                    Some(v) => v,
                    None if i < tokens.len() => {
                        i += 1;
                        tokens[i - 1].clone()
                    }
                    None => return Err(usage_error(format!("`--{name}` needs a value"))),
                };
                let value = convert(arg, &text, &enums).map_err(usage_error)?;
                store(arg, index, value, &mut given, &mut lists);
                continue;
            }
            if !only_positional && token.starts_with('-') && token.len() > 1 && parse_number(token).is_none() {
                // Short flags, which can be combined *(i.e. `-nv`)*
                let letters: Vec<char> = token[1..].chars().collect();
                let mut j = 0;
                while j < letters.len() {
                    let c = letters[j];
                    j += 1;
                    let Some(index) = info.args.iter().position(|a| a.short == Some(c)) else {
                        return Err(usage_error(format!("unknown option `-{c}`")));
                    };
                    let arg = &info.args[index];
                    if arg.count {
                        counts[index] += 1;
                    } else if arg.kind == ArgType::Bool {
                        given[index] = Some(Value::Bool(true));
                    } else {
                        // The value is the rest of the letters, or the next argument
                        let rest: String = letters[j..].iter().collect();
                        let text = if !rest.is_empty() {
                            rest
                        } else if i < tokens.len() {
                            i += 1;
                            tokens[i - 1].clone()
                        } else {
                            return Err(usage_error(format!("`-{c}` needs a value")));
                        };
                        let value = convert(arg, &text, &enums).map_err(usage_error)?;
                        store(arg, index, value, &mut given, &mut lists);
                        break;
                    }
                }
                continue;
            }
            if next < positional.len() {
                let index = positional[next];
                let arg = &info.args[index];
                let value = convert(arg, token, &enums).map_err(usage_error)?;
                store(arg, index, value, &mut given, &mut lists);
                if !arg.variadic {
                    next += 1;
                }
                continue;
            }
            if !info.subcommands.is_empty() {
                // The rest of the arguments are the subcommand's
                let Some((member, _)) = info.subcommands.iter().find(|(m, _)| kebab(m) == *token) else {
                    let names: Vec<String> = info.subcommands.iter().map(|(m, _)| kebab(m)).collect();
                    return Err(usage_error(format!("unknown command `{token}`: it can be {}", names.join(", "))));
                };
                let sub = match class.statics.borrow().get(&**member) {
                    Some(slot) => slot.value.clone(),
                    None => return Ok(Value::Null),
                };
                let Value::Class(sub) = sub else { return Ok(Value::Null) };
                subcommand = Some(self.parse_command(&sub, &tokens[i..], &format!("{prog} {token}"), span)?);
                break;
            }
            return Err(usage_error(format!("unexpected argument `{token}`")));
        }
        if !info.subcommands.is_empty() && subcommand.is_none() && !info.optional_subcommand {
            let names: Vec<String> = info.subcommands.iter().map(|(m, _)| kebab(m)).collect();
            return Err(usage_error(format!("a command is needed: {}", names.join(", "))));
        }
        // The object, with what was given and the defaults
        let mut named = Vec::new();
        for (index, arg) in info.args.iter().enumerate() {
            let value = if arg.count {
                Some(Value::Int(counts[index]))
            } else if arg.variadic {
                Some(Value::list(std::mem::take(&mut lists[index])))
            } else if let Some(v) = given[index].take() {
                Some(v)
            } else if arg.kind == ArgType::Bool && !arg.has_value {
                Some(Value::Bool(false))
            } else if arg.has_value || arg.nullable {
                None
            } else {
                return Err(usage_error(format!("{} is missing", shown(arg))));
            };
            if let Some(v) = value {
                named.push((arg.name.to_string(), v, span));
            }
        }
        if !info.subcommands.is_empty() {
            named.push(("subcommand".to_string(), subcommand.unwrap_or(Value::Null), span));
        }
        self.construct_object(class, named, span)
    }
}

use std::io::Write;

fn find_flag(info: &CommandInfo, name: &str) -> Option<usize> {
    info.args.iter().position(|a| a.flag && kebab(&a.name) == name)
}

fn store(arg: &ArgInfo, index: usize, value: Value, given: &mut [Option<Value>], lists: &mut [Vec<Value>]) {
    if arg.variadic {
        lists[index].push(value);
    } else {
        given[index] = Some(value);
    }
}

/// The text of an argument as a value of its field's type.
fn convert(arg: &ArgInfo, text: &str, enums: &[Rc<Class>]) -> Result<Value, String> {
    match arg.kind {
        ArgType::Str => Ok(Value::string(text)),
        ArgType::Bool => Ok(Value::Bool(true)),
        ArgType::Number => parse_number(text).ok_or_else(|| format!("{} needs a number, not `{text}`", shown(arg))),
        ArgType::Enum(i) => {
            let class = &enums[i as usize];
            let wanted = text.replace('-', "_");
            let values = class.values.borrow();
            let found = values.iter().find(|v| matches!(v, Value::Object(o) if o.enum_name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&wanted))));
            found.cloned().ok_or_else(|| format!("{} must be one of {}, not `{text}`", shown(arg), choices(class).join(", ")))
        }
    }
}

/// The values of an enum, as they're written on the command line.
fn choices(class: &Class) -> Vec<String> {
    class
        .values
        .borrow()
        .iter()
        .filter_map(|v| match v {
            Value::Object(o) => o.enum_name.as_deref().map(|n| n.to_ascii_lowercase().replace('_', "-")),
            _ => None,
        })
        .collect()
}

/// `dryRun` and `DryRun` are `dry-run`.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// How an argument is named in messages: `--keep` or `<source>`.
fn shown(arg: &ArgInfo) -> String {
    if arg.flag { format!("`--{}`", kebab(&arg.name)) } else { format!("`<{}>`", kebab(&arg.name)) }
}

fn usage(info: &CommandInfo, prog: &str) -> String {
    let mut line = format!("Usage: {prog}");
    if info.args.iter().any(|a| a.flag) {
        line.push_str(" [options]");
    }
    for arg in info.args.iter().filter(|a| !a.flag) {
        let name = kebab(&arg.name);
        line.push(' ');
        line.push_str(&match (arg.variadic, arg.nullable || arg.has_value) {
            (true, _) => format!("[{name}...]"),
            (false, true) => format!("[{name}]"),
            (false, false) => format!("<{name}>"),
        });
    }
    if !info.subcommands.is_empty() {
        line.push_str(if info.optional_subcommand { " [command]" } else { " <command>" });
    }
    line
}

/// The text of `--help`: the command's comment, its usage, and its
/// arguments, commands and options with their comments.
fn help_text(class: &Class, prog: &str) -> String {
    let Some((info, enums)) = &class.command else { return String::new() };
    let mut out = String::new();
    if let Some(doc) = &info.doc {
        out.push_str(doc);
        out.push_str("\n\n");
    }
    out.push_str(&usage(info, prog));
    let rows = |title: &str, items: Vec<(String, String)>, out: &mut String| {
        if items.is_empty() {
            return;
        }
        let width = items.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
        out.push_str(&format!("\n\n{title}:"));
        for (left, right) in items {
            if right.is_empty() {
                out.push_str(&format!("\n  {left}"));
            } else {
                out.push_str(&format!("\n  {left:width$}  {right}"));
            }
        }
    };
    let describe = |arg: &ArgInfo| -> String {
        let mut text = arg.doc.clone().unwrap_or_default().replace('\n', " ");
        if let ArgType::Enum(i) = arg.kind {
            text.push_str(&format!(" ({})", choices(&enums[i as usize]).join(", ")));
        }
        if let Some(default) = &arg.default {
            text.push_str(&format!(" (default: {default})"));
        }
        text.trim().to_string()
    };
    let positional: Vec<(String, String)> = info.args.iter().filter(|a| !a.flag).map(|a| (format!("<{}>", kebab(&a.name)), describe(a))).collect();
    rows("Arguments", positional, &mut out);
    let commands: Vec<(String, String)> = info.subcommands.iter().map(|(m, doc)| (kebab(m), doc.clone().unwrap_or_default().replace('\n', " "))).collect();
    rows("Commands", commands, &mut out);
    let mut options: Vec<(String, String)> = info
        .args
        .iter()
        .filter(|a| a.flag)
        .map(|a| {
            let short = a.short.map(|c| format!("-{c}, ")).unwrap_or_else(|| "    ".to_string());
            let value = match a.kind {
                ArgType::Bool => String::new(),
                _ if a.count => String::new(),
                ArgType::Number => " <number>".to_string(),
                _ => " <value>".to_string(),
            };
            (format!("{short}--{}{value}", kebab(&a.name)), describe(a))
        })
        .collect();
    options.push(("-h, --help".to_string(), "Show this help".to_string()));
    rows("Options", options, &mut out);
    out
}
