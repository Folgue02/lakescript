//! `use` lines that are never used, a warning *(see Warnings in
//! ideas/tooling/diagnostics.md)*. Found from the tokens: a name a `use`
//! gives that's written nowhere else in the file isn't used.

use std::collections::HashMap;

use crate::ast::{Stmt, StmtKind, UseItems};
use crate::diagnostic::Diagnostic;
use crate::token::{Keyword, StringPart, Token, TokenKind};

/// How many times each name is written outside `use` lines: `name` for
/// modules, functions and types, `$name` for constants.
fn written(tokens: &[Token], counts: &mut HashMap<String, usize>) {
    let mut in_use = false;
    for token in tokens {
        match &token.kind {
            TokenKind::Keyword(Keyword::Use) => in_use = true,
            TokenKind::Semicolon if in_use => in_use = false,
            TokenKind::Ident(name) if !in_use => *counts.entry(name.clone()).or_default() += 1,
            TokenKind::Variable(name) if !in_use => *counts.entry(format!("${name}")).or_default() += 1,
            TokenKind::FStr(parts) | TokenKind::Command(parts) => {
                for part in parts {
                    if let StringPart::Code(code) = part {
                        written(code, counts);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A warning for each name a `use` of `program` gives that `tokens` never
/// write. A name that starts with `_` is unused on purpose.
pub fn imports(program: &[Stmt], tokens: &[Token]) -> Vec<Diagnostic> {
    let mut counts = HashMap::new();
    written(tokens, &mut counts);
    let mut warnings = Vec::new();
    for stmt in program {
        let StmtKind::Use(decl) = &stmt.kind else { continue };
        let names: Vec<String> = match &decl.items {
            UseItems::Module { alias } => vec![alias.clone().unwrap_or_else(|| decl.path.last().cloned().unwrap_or_default())],
            UseItems::Items(items) => items
                .iter()
                .map(|i| {
                    let name = i.alias.clone().unwrap_or_else(|| i.name.clone());
                    if i.variable { format!("${name}") } else { name }
                })
                .collect(),
        };
        for name in names {
            if name.trim_start_matches('$').starts_with('_') || counts.get(&name).is_some_and(|n| *n > 0) {
                continue;
            }
            warnings.push(Diagnostic::new(format!("`{name}` is imported but never used"), stmt.span));
        }
    }
    warnings
}
