//! Types in the same folder, used without `use` *(see Types in the same
//! folder in `ideas/language/modules.md`)*. A type name that nothing in the
//! file gives, and whose type module is in the folder of the file's own
//! module, gets a `use` of it, as if it were written. So the checker, the
//! compiler and the editor see an ordinary `use`, and the module is loaded
//! the first time it's used, like any other.

use std::collections::HashSet;
use std::path::Path;

use crate::ast::{ClassDecl, MemberKind, Stmt, StmtKind, UseDecl, UseItem, UseItems};
use crate::modules::Loader;
use crate::token::{Keyword, Span, StringPart, Token, TokenKind};

/// The names written like types *(i.e. `Customer`)* that could be the name
/// of a type, each with where it's first used: not after `.`, `?.` or `::`
/// *(a member)*, and not in a `use` line.
pub fn type_names(tokens: &[Token]) -> Vec<(String, Span)> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    scan(tokens, &mut seen, &mut found);
    found
}

fn scan(tokens: &[Token], seen: &mut HashSet<String>, found: &mut Vec<(String, Span)>) {
    let mut in_use = false;
    let mut previous: Option<&TokenKind> = None;
    for token in tokens {
        match &token.kind {
            TokenKind::Keyword(Keyword::Use) => in_use = true,
            TokenKind::Semicolon if in_use => in_use = false,
            TokenKind::Ident(name)
                if !in_use
                    && crate::parser::is_type_name(name)
                    && name != "Self"
                    && !matches!(previous, Some(TokenKind::Dot | TokenKind::QuestionDot | TokenKind::ColonColon))
                    && seen.insert(name.clone()) =>
            {
                found.push((name.clone(), token.span));
            }
            // The code inside `f"..."` and `%x"..."`
            TokenKind::FStr(parts) | TokenKind::Command(parts) => {
                for part in parts {
                    if let StringPart::Code(code) = part {
                        scan(code, seen, found);
                    }
                }
            }
            _ => {}
        }
        previous = Some(&token.kind);
    }
}

/// The names the file gives itself: its types and their type parameters,
/// the type parameters of its functions, and what its `use` lines import.
fn given(program: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    fn class(decl: &ClassDecl, names: &mut HashSet<String>) {
        names.insert(decl.name.clone());
        names.extend(decl.generics.iter().cloned());
        for member in &decl.members {
            if let MemberKind::Method { decl, .. } | MemberKind::Constructor(decl) = &member.kind {
                names.extend(decl.generics.iter().cloned());
            }
        }
        for sub in &decl.subcommands {
            class(sub, names);
        }
    }
    for stmt in program {
        let inner = match &stmt.kind {
            StmtKind::Pub(inner) => &inner.kind,
            other => other,
        };
        match inner {
            StmtKind::Class(decl) => class(decl, &mut names),
            StmtKind::Function(decl) => names.extend(decl.generics.iter().cloned()),
            StmtKind::Use(decl) => match &decl.items {
                UseItems::Module { alias } => {
                    names.insert(alias.clone().unwrap_or_else(|| decl.path.last().cloned().unwrap_or_default()));
                }
                UseItems::Items(items) => {
                    names.extend(items.iter().map(|i| i.alias.clone().unwrap_or_else(|| i.name.clone())));
                }
            },
            _ => {}
        }
    }
    names
}

impl Loader {
    /// The `use` path of the module in `file`: its path from the root it's
    /// in, without `.lks` *(i.e. `["folgue02", "shop", "Order"]`)*. `None`
    /// for a file outside the module path.
    pub fn module_name(&self, file: &Path) -> Option<Vec<String>> {
        let parts = |relative: &Path| -> Option<Vec<String>> {
            let relative = relative.with_extension("");
            relative.iter().map(|p| p.to_str().map(String::from)).collect()
        };
        let shown = file.to_str()?;
        if let Some(rest) = shown.strip_prefix(crate::stdlib::EMBEDDED) {
            let mut name = vec!["std".to_string()];
            name.extend(parts(Path::new(rest))?);
            return Some(name);
        }
        let file = crate::paths::canonical(file);
        self.roots.iter().find_map(|root| file.strip_prefix(crate::paths::canonical(root)).ok().and_then(parts))
    }

    /// Whether `use <path>;` finds a module, without the work of saying why
    /// when it doesn't.
    fn has_module(&self, path: &[String]) -> bool {
        let joined = path.join("/");
        if path.first().is_some_and(|p| p == "std") {
            return crate::stdlib::MODULES.contains(&joined.as_str()) && !self.disabled.contains(&joined);
        }
        let relative: std::path::PathBuf = path.iter().collect::<std::path::PathBuf>().with_extension("lks");
        self.roots.iter().map(|r| r.join(&relative)).any(|p| p.is_file() || self.overlay.contains_key(&p))
    }

    /// Adds a `use` for each type of `names` that nothing in `program` gives
    /// and whose type module is in the folder of `file`'s module.
    pub fn add_siblings(&self, program: &mut Vec<Stmt>, names: Vec<(String, Span)>, file: &Path) {
        if names.is_empty() {
            return;
        }
        let Some(mut folder) = self.module_name(file) else { return };
        folder.pop();
        let given = given(program);
        let mut uses = Vec::new();
        for (name, span) in names {
            if given.contains(&name) || crate::exceptions::is_exception_type(&name) {
                continue;
            }
            let mut path = folder.clone();
            path.push(name);
            if self.has_module(&path) {
                let item = UseItem { name: path.last().cloned().unwrap_or_default(), variable: false, alias: None };
                let decl = UseDecl { path, items: UseItems::Items(vec![item]), type_module: true };
                uses.push(Stmt { kind: StmtKind::Use(decl), span });
            }
        }
        program.splice(0..0, uses);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<String> {
        type_names(&crate::lexer::tokenize(source).unwrap()).into_iter().map(|(n, _)| n).collect()
    }

    #[test]
    fn the_names_that_could_be_types() {
        assert_eq!(names("use a/Item;\nconst $x = Customer(Role::ADMIN).Name;\n"), ["Customer", "Role"]);
        // Each once, also from inside `f"..."`, and never `Self` or a member
        assert_eq!(names("Point p = Point(1);\noutln(f\"{Line()} {$x?.Other}\");\nSelf $s;\n"), ["Point", "Line"]);
        assert_eq!(names("const $text = 'Customer';\n// Customer\n"), Vec::<String>::new());
    }
}
