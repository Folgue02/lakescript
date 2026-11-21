//! `ferry doc`: the pages of a project's documentation, made from the `//!`
//! and `///` comments of its modules, the `module.md` of its folders and the
//! commands of `bin/` *(see `ideas/tooling/docs.md`)*. Nothing is run.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use lakescript::docs::{self, ArgDoc, CommandDoc, Doc, Item, ItemKind, ModuleDoc};

use crate::Fail;
use crate::project::Project;

/// The page of the standard library, made by `tools/docs/stdlib.py` from
/// `ideas/stdlib`, for `ferry doc std`.
pub const STDLIB_PAGE: &str = include_str!("stdlib.html");

/// What the pages of a project are made from.
pub struct Site {
    pub id: String,
    pub version: String,
    pub kind: String,
    /// The `module.md` of `src/<author>/<name>/`, the front page.
    pub front: Option<String>,
    pub modules: Vec<ModulePage>,
    /// The folders inside the project's folder that have a `module.md`.
    pub folders: Vec<FolderPage>,
    pub commands: Vec<CommandPage>,
    /// The dependencies: their name, version, and their docs when they were made.
    pub dependencies: Vec<(String, String, Option<String>)>,
}

pub struct ModulePage {
    /// Its `use` path *(i.e. `folgue02/sizes/units`)*.
    pub path: String,
    /// Its path inside the project *(i.e. `units`, `api/users`)*.
    pub short: String,
    /// Its file, as errors name it *(i.e. `units.lks`)*.
    pub file: String,
    pub doc: ModuleDoc,
}

pub struct FolderPage {
    pub short: String,
    pub text: String,
}

pub struct CommandPage {
    pub name: String,
    /// The first line of the script's `//!`, or of its command's comment.
    pub summary: String,
    pub text: Option<String>,
    /// Its arguments, when it reads them with a `command`.
    pub command: Option<CommandDoc>,
}

/// Reads a project's docs. A module with a syntax error stops it, since the
/// project is checked before anyway.
pub fn read(project: &Project) -> Result<Site, Fail> {
    let src = project.src();
    let own = src.join(&project.id);
    let mut modules = Vec::new();
    for path in project.modules()? {
        let file = src.join(format!("{path}.lks"));
        let text = read_file(&file)?;
        let doc = docs::module_doc(&text).map_err(|d| format!("{}: {}", file.display(), d.message))?;
        let short = path.strip_prefix(&format!("{}/", project.id)).unwrap_or(&path).to_string();
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        modules.push(ModulePage { path, short, file: name, doc });
    }
    let mut front = None;
    let mut folders = Vec::new();
    let mut found = Vec::new();
    module_files(&own, &mut found);
    found.sort();
    for file in found {
        let text = read_file(&file)?;
        let folder = file.parent().unwrap_or(&own);
        if folder == own {
            front = Some(text);
        } else {
            let short = folder.strip_prefix(&own).unwrap_or(folder).to_string_lossy().replace('\\', "/");
            folders.push(FolderPage { short, text });
        }
    }
    let mut commands = Vec::new();
    for name in project.commands() {
        let file = project.command_file(&name);
        let text = read_file(&file)?;
        let command = docs::command_doc(&text, &name).map_err(|d| format!("{}: {}", file.display(), d.message))?;
        let script = docs::module_text(&text);
        let summary = script
            .as_deref()
            .or(command.as_ref().and_then(|c| c.text.as_deref()))
            .map(|t| Doc { text: t.to_string(), ..Doc::default() }.summary())
            .unwrap_or_default();
        commands.push(CommandPage { name, summary, text: script, command });
    }
    let manifest = &project.manifest.project;
    Ok(Site {
        id: project.id.clone(),
        version: manifest.version.clone(),
        kind: format!("{:?}", manifest.kind).to_lowercase(),
        front,
        modules,
        folders,
        commands,
        dependencies: Vec::new(),
    })
}

fn read_file(path: &Path) -> Result<String, Fail> {
    std::fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()).into())
}

fn module_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            module_files(&path, out);
        } else if path.file_name().is_some_and(|n| n == "module.md") {
            out.push(path);
        }
    }
}

// ---- Writing the pages ----

/// Writes the pages to `dir`, and gives the front page.
pub fn write(site: &Site, dir: &Path) -> Result<PathBuf, Fail> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("can't clean {}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let types = types(site);
    let mut pages: Vec<(String, String)> = vec![("index.html".into(), front_page(site, &types))];
    for module in &site.modules {
        pages.push((module_href(module), module_page(site, module, &types)));
        for item in module.doc.items.iter().filter(|i| has_page(i)) {
            pages.push((type_href(module, &item.name), type_page(site, module, item, &types)));
        }
    }
    for folder in &site.folders {
        pages.push((folder_href(&folder.short), folder_page(site, folder)));
    }
    for command in &site.commands {
        pages.push((command_href(&command.name), command_page(site, command)));
    }
    pages.push(("style.css".into(), STYLE.to_string()));
    pages.push(("search.js".into(), format!("window.DOCS = {};\n{SEARCH}", index(site))));
    for (name, text) in pages {
        let path = dir.join(&name);
        std::fs::write(&path, text).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    }
    Ok(dir.join("index.html"))
}

fn has_page(item: &Item) -> bool {
    matches!(item.kind, ItemKind::Class | ItemKind::Interface | ItemKind::Enum)
}

fn dotted(path: &str) -> String {
    path.replace('/', ".")
}

fn module_href(module: &ModulePage) -> String {
    format!("{}.html", dotted(&module.short))
}

fn type_href(module: &ModulePage, name: &str) -> String {
    format!("{}.{name}.html", dotted(&module.short))
}

fn folder_href(short: &str) -> String {
    format!("folder.{}.html", dotted(short))
}

fn command_href(name: &str) -> String {
    format!("command.{name}.html")
}

/// The classes, interfaces and enums of the project, by name, and their pages.
fn types(site: &Site) -> BTreeMap<String, String> {
    let mut types = BTreeMap::new();
    for module in &site.modules {
        let last = module.short.rsplit('/').next().unwrap_or(&module.short);
        for item in module.doc.items.iter().filter(|i| has_page(i)) {
            types.insert(item.name.clone(), type_href(module, &item.name));
            types.insert(format!("{last}::{}", item.name), type_href(module, &item.name));
        }
    }
    types
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Markdown as HTML. HTML written in it is shown as text, so a comment can't
/// add scripts to the pages.
fn md(text: &str) -> String {
    use pulldown_cmark::{Event, Parser};
    let events = Parser::new(text).map(|e| match e {
        Event::Html(h) | Event::InlineHtml(h) => Event::Text(h),
        other => other,
    });
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, events);
    out
}

/// Markdown of one line, without its paragraph.
fn md_inline(text: &str) -> String {
    let html = md(text);
    html.trim().strip_prefix("<p>").and_then(|h| h.strip_suffix("</p>")).map(str::to_string).unwrap_or(html)
}

const KEYWORDS: &[&str] = &["pub", "const", "static", "class", "interface", "enum", "extends", "null", "true", "false"];
const BUILT_IN: &[&str] = &["string", "number", "bool", "any", "void", "list", "map", "range", "regex", "symbol", "result", "command", "exception"];

/// A declaration with colors: keywords, types, parameters, and its own name
/// in bold. The project's types link to their pages.
fn code(text: &str, own: &str, types: &BTreeMap<String, String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != c {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i = (i + 1).min(chars.len());
            out.push_str(&esc(&chars[start..i].iter().collect::<String>()));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' || ((c == '$' || c == '@') && chars.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic() || *n == '_')) {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '?') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // `report::Point`: the module and the type together
            let qualified = (chars.get(i) == Some(&':') && chars.get(i + 1) == Some(&':')).then(|| {
                let mut j = i + 2;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                (j, chars[start..j].iter().collect::<String>())
            });
            if let Some((end, full)) = qualified.filter(|(_, full)| types.contains_key(full)) {
                out.push_str(&format!("<a class=\"t\" href=\"{}\">{}</a>", types[&full], esc(&full)));
                i = end;
                continue;
            }
            let class = if word == own || word.trim_start_matches(['$', '@']) == own.trim_start_matches('$') && word.starts_with('$') && own.starts_with('$') {
                "n"
            } else if word.starts_with('$') || word.starts_with('@') {
                "p"
            } else if KEYWORDS.contains(&word.as_str()) {
                "k"
            } else if BUILT_IN.contains(&word.as_str()) || word.starts_with(|c: char| c.is_ascii_uppercase()) {
                "t"
            } else {
                ""
            };
            match (types.get(&word), class) {
                (Some(href), "t") => out.push_str(&format!("<a class=\"t\" href=\"{href}\">{}</a>", esc(&word))),
                (_, "") => out.push_str(&esc(&word)),
                (_, class) => out.push_str(&format!("<span class=\"{class}\">{}</span>", esc(&word))),
            }
            continue;
        }
        out.push_str(&esc(&c.to_string()));
        i += 1;
    }
    out
}

/// The top bar, the side bar and the frame of every page.
fn page(site: &Site, title: &str, current: Option<(&ModulePage, &[Item])>, current_command: Option<&str>, main: &str) -> String {
    page_at(site, title, current, None, current_command, main)
}

/// `page`, with `current_item` marked in the side bar.
fn page_at(site: &Site, title: &str, current: Option<(&ModulePage, &[Item])>, current_item: Option<&str>, current_command: Option<&str>, main: &str) -> String {
    let mut side = String::new();
    side.push_str(&format!("<div class=\"side-title\">{}</div>\n<ul class=\"side-list\">\n", esc(&site.id)));
    let overview = if current.is_none() && current_command.is_none() && title == site.id { " class=\"current\"" } else { "" };
    side.push_str(&format!("<li><a href=\"index.html\"{overview}>Overview</a></li>\n"));
    for module in &site.modules {
        let open = current.is_some_and(|(m, _)| std::ptr::eq(m, module));
        let cls = if open { " class=\"current\"" } else { "" };
        side.push_str(&format!("<li><a href=\"{}\"{cls}>{}</a>", module_href(module), esc(&module.short)));
        if let (true, Some((_, items))) = (open, current) {
            side.push_str("<ul class=\"side-members\">");
            for item in items {
                let href = if has_page(item) { type_href(module, &item.name) } else { format!("{}#{}", module_href(module), anchor(&item.name)) };
                let label = if matches!(item.kind, ItemKind::Function) { format!("{}()", item.name) } else { item.name.clone() };
                let here = if current_item == Some(item.name.as_str()) { " class=\"here\" aria-current=\"page\"" } else { "" };
                side.push_str(&format!("<li><a href=\"{href}\"{here}>{}</a></li>", esc(&label)));
            }
            side.push_str("</ul>");
        }
        side.push_str("</li>\n");
    }
    side.push_str("</ul>\n");
    if !site.commands.is_empty() {
        side.push_str("<div class=\"side-title\">Commands</div>\n<ul class=\"side-list mono\">\n");
        for command in &site.commands {
            let cls = if current_command == Some(command.name.as_str()) { " class=\"current\"" } else { "" };
            side.push_str(&format!("<li><a href=\"{}\"{cls}>{}</a></li>\n", command_href(&command.name), esc(&command.name)));
        }
        side.push_str("</ul>\n");
    }
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · {id}</title>
<link rel="stylesheet" href="style.css">
</head>
<body>
<header class="top">
<a class="brand" href="index.html"><svg width="26" height="26" viewBox="0 0 26 26" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M3 15c3 0 3-2 6-2s3 2 6 2 3-2 6-2 2 1 2 1"></path><path d="M3 20c3 0 3-2 6-2s3 2 6 2 3-2 6-2 2 1 2 1"></path><path d="M13 3l5 8H8z"></path></svg><span>{id}</span></a>
<span class="pill">{version}</span>
<div class="search">
<label for="search" class="sr">Search the documentation</label>
<input id="search" type="search" placeholder="Search · press /" autocomplete="off">
<div id="results" class="results" role="listbox" aria-label="Results" hidden></div>
</div>
</header>
<div class="layout">
<nav class="side" aria-label="Modules">
{side}</nav>
<main>
<div class="content">
{main}
</div>
</main>
</div>
<script src="search.js"></script>
</body>
</html>
"#,
        title = esc(title),
        id = esc(&site.id),
        version = esc(&site.version),
    )
}

fn anchor(name: &str) -> String {
    name.trim_start_matches('$').replace('?', "")
}

fn badge(kind: &str) -> String {
    format!("<span class=\"badge {kind}\">{kind}</span>")
}

/// The parts of a comment after its text: parameters, what it returns and throws.
fn details(doc: &Doc) -> String {
    let mut rows = String::new();
    for (i, (name, what)) in doc.params.iter().enumerate() {
        let label = if i == 0 { "Parameters" } else { "" };
        let _ = write!(rows, "<div class=\"label\">{label}</div><div><code class=\"p\">${}</code> — {}</div>", esc(name), md_inline(what));
    }
    if let Some(what) = &doc.returns {
        let _ = write!(rows, "<div class=\"label\">Returns</div><div>{}</div>", md_inline(what));
    }
    for (i, (exception, what)) in doc.throws.iter().enumerate() {
        let label = if i == 0 { "Throws" } else { "" };
        let _ = write!(rows, "<div class=\"label\">{label}</div><div><code>{}</code> — {}</div>", esc(exception), md_inline(what));
    }
    if rows.is_empty() { String::new() } else { format!("<div class=\"details\">{rows}</div>") }
}

fn card(item: &Item, file: &str, types: &BTreeMap<String, String>, note: &str) -> String {
    let text = if item.doc.text.is_empty() { String::new() } else { format!("<div class=\"doc\">{}</div>", md(&item.doc.text)) };
    let body = format!("{text}{}{note}", details(&item.doc));
    let body = if body.is_empty() { String::new() } else { format!("<div class=\"card-body\">{body}</div>") };
    format!(
        "<section class=\"card\" id=\"{}\"><div class=\"card-head\"><code>{}</code><span class=\"where\">{}:{}</span></div>{body}</section>\n",
        anchor(&item.name),
        code(&item.code, &item.name, types),
        esc(file),
        item.line
    )
}

fn breadcrumb(site: &Site, parts: &[(String, Option<String>)]) -> String {
    let mut out = format!("<nav class=\"crumbs\" aria-label=\"Breadcrumb\"><a href=\"index.html\">{}</a>", esc(&site.id));
    for (label, href) in parts {
        match href {
            Some(href) => {
                let _ = write!(out, " <span aria-hidden=\"true\">/</span> <a href=\"{href}\">{}</a>", esc(label));
            }
            None => {
                let _ = write!(out, " <span aria-hidden=\"true\">/</span> <span>{}</span>", esc(label));
            }
        }
    }
    out.push_str("</nav>\n");
    out
}

fn front_page(site: &Site, _types: &BTreeMap<String, String>) -> String {
    let mut main = format!(
        "<div class=\"title\"><h1>{}</h1>{}</div>\n<div class=\"facts\"><span>Version <strong>{}</strong></span><span>{}</span></div>\n",
        esc(&site.id),
        badge(&site.kind),
        esc(&site.version),
        match site.dependencies.len() {
            0 => "No dependencies".to_string(),
            1 => "1 dependency".to_string(),
            n => format!("{n} dependencies"),
        }
    );
    if let Some(text) = &site.front {
        let _ = writeln!(main, "<article class=\"doc big\">{}</article>", md(text));
    }
    if !site.modules.is_empty() {
        main.push_str("<h2>Modules</h2>\n<div class=\"tiles\">\n");
        for module in &site.modules {
            let summary = module.doc.text.as_ref().map(|t| Doc { text: t.clone(), ..Doc::default() }.summary()).unwrap_or_default();
            let count = |kinds: &[ItemKind], one: &str, many: &str| {
                let n = module.doc.items.iter().filter(|i| kinds.contains(&i.kind)).count();
                match n {
                    0 => None,
                    1 => Some(format!("1 {one}")),
                    n => Some(format!("{n} {many}")),
                }
            };
            let counts: Vec<String> = [
                count(&[ItemKind::Function], "function", "functions"),
                count(&[ItemKind::Class, ItemKind::Interface, ItemKind::Enum], "type", "types"),
                count(&[ItemKind::Constant], "constant", "constants"),
            ]
            .into_iter()
            .flatten()
            .collect();
            let _ = writeln!(
                main,
                "<a class=\"tile\" href=\"{}\"><code>{}</code><span>{}</span><span class=\"muted\">{}</span></a>",
                module_href(module),
                esc(&module.path),
                md_inline(&summary),
                counts.join(" · ")
            );
        }
        main.push_str("</div>\n");
    }
    if !site.folders.is_empty() {
        main.push_str("<h2>Folders</h2>\n<div class=\"tiles\">\n");
        for folder in &site.folders {
            let summary = Doc { text: folder.text.clone(), ..Doc::default() }.summary();
            let _ = writeln!(main, "<a class=\"tile\" href=\"{}\"><code>{}/</code><span>{}</span></a>", folder_href(&folder.short), esc(&folder.short), md_inline(&summary));
        }
        main.push_str("</div>\n");
    }
    if !site.commands.is_empty() {
        main.push_str("<h2>Commands</h2>\n<div class=\"rows\">\n");
        for command in &site.commands {
            let _ = writeln!(
                main,
                "<div class=\"row\"><a href=\"{}\"><code>{}</code></a><span>{}</span><code class=\"muted\">ferry run {}</code></div>",
                command_href(&command.name),
                esc(&command.name),
                md_inline(&command.summary),
                esc(&command.name)
            );
        }
        main.push_str("</div>\n");
    }
    if !site.dependencies.is_empty() {
        main.push_str("<h2>Dependencies</h2>\n<div class=\"rows\">\n");
        for (name, version, href) in &site.dependencies {
            let name = match href {
                Some(href) => format!("<a href=\"{}\"><code>{}</code></a>", esc(href), esc(name)),
                None => format!("<code>{}</code>", esc(name)),
            };
            let _ = writeln!(main, "<div class=\"row\">{name}<span class=\"muted\">{}</span></div>", esc(version));
        }
        main.push_str("</div>\n");
    }
    page(site, &site.id, None, None, &main)
}

fn module_page(site: &Site, module: &ModulePage, types: &BTreeMap<String, String>) -> String {
    let last = module.short.rsplit('/').next().unwrap_or(&module.short);
    let mut main = breadcrumb(site, &[(module.short.clone(), None)]);
    let _ = writeln!(
        main,
        "<div class=\"title\"><h1>{}</h1>{}<span class=\"where\">{}</span></div>\n<div class=\"use\"><code><span class=\"k\">use</span> {};</code><button type=\"button\" data-copy=\"use {};\">Copy</button></div>",
        esc(last),
        badge("module"),
        esc(&module.file),
        esc(&module.path),
        esc(&module.path)
    );
    if let Some(text) = &module.doc.text {
        let _ = writeln!(main, "<div class=\"doc big\">{}</div>", md(text));
    }
    let section = |title: &str, kinds: &[ItemKind], main: &mut String| {
        let items: Vec<&Item> = module.doc.items.iter().filter(|i| kinds.contains(&i.kind)).collect();
        if items.is_empty() {
            return;
        }
        let _ = writeln!(main, "<h2>{title}</h2>\n<div class=\"cards\">");
        for item in items {
            if has_page(item) {
                let _ = writeln!(
                    main,
                    "<a class=\"tile wide\" href=\"{}\">{}<code>{}</code><span>{}</span></a>",
                    type_href(module, &item.name),
                    badge(item.kind.word()),
                    esc(&item.name),
                    md_inline(&item.doc.summary())
                );
            } else {
                main.push_str(&card(item, &module.file, types, ""));
            }
        }
        main.push_str("</div>\n");
    };
    section("Constants", &[ItemKind::Constant], &mut main);
    section("Functions", &[ItemKind::Function], &mut main);
    section("Types", &[ItemKind::Class, ItemKind::Interface, ItemKind::Enum], &mut main);
    if module.doc.items.is_empty() && module.doc.text.is_none() {
        main.push_str("<p class=\"muted\">This module has nothing <code>pub</code> and no <code>//!</code> comment.</p>\n");
    }
    page(site, last, Some((module, &module.doc.items)), None, &main)
}

fn type_page(site: &Site, module: &ModulePage, item: &Item, types: &BTreeMap<String, String>) -> String {
    let mut main = breadcrumb(site, &[(module.short.clone(), Some(module_href(module))), (item.name.clone(), None)]);
    let _ = writeln!(
        main,
        "<div class=\"title\"><h1>{}</h1>{}<span class=\"where\">{}:{}</span></div>\n<div class=\"use\"><code>{}</code></div>",
        esc(&item.name),
        badge(item.kind.word()),
        esc(&module.file),
        item.line,
        code(&item.code, &item.name, types)
    );
    if !item.doc.text.is_empty() {
        let _ = writeln!(main, "<div class=\"doc big\">{}</div>", md(&item.doc.text));
    }
    // What it extends and implements, and who implements it
    let link = |name: &str| match types.get(name) {
        Some(href) => format!("<a href=\"{href}\"><code>{}</code></a>", esc(name)),
        None => format!("<code>{}</code>", esc(name)),
    };
    let mut facts = Vec::new();
    match item.kind {
        ItemKind::Interface => {
            if !item.extends.is_empty() {
                facts.push(("Extends", item.extends.iter().map(|n| link(n)).collect::<Vec<_>>().join(", ")));
            }
            let mut by_extending = Vec::new();
            let mut implementers = Vec::new();
            for other in site.modules.iter().flat_map(|m| &m.doc.items) {
                if other.kind == ItemKind::Interface && other.extends.contains(&item.name) {
                    by_extending.push(other.name.clone());
                }
            }
            for other in site.modules.iter().flat_map(|m| &m.doc.items) {
                if other.kind != ItemKind::Class {
                    continue;
                }
                if other.implements.contains(&item.name) {
                    implementers.push(link(&other.name));
                } else if let Some(through) = other.implements.iter().find(|i| by_extending.contains(i)) {
                    implementers.push(format!("{} <span class=\"muted\">through <code>{}</code></span>", link(&other.name), esc(through)));
                }
            }
            if !by_extending.is_empty() {
                facts.push(("Extended by", by_extending.iter().map(|n| link(n)).collect::<Vec<_>>().join(", ")));
            }
            if !implementers.is_empty() {
                facts.push(("Implemented by", implementers.join(", ")));
            }
        }
        ItemKind::Class => {
            if !item.extends.is_empty() {
                facts.push(("Extends", item.extends.iter().map(|n| link(n)).collect::<Vec<_>>().join(", ")));
            }
            if !item.implements.is_empty() {
                facts.push(("Implements", item.implements.iter().map(|n| link(n)).collect::<Vec<_>>().join(", ")));
            }
        }
        _ => {}
    }
    if !facts.is_empty() {
        main.push_str("<div class=\"facts-box\">");
        for (label, value) in facts {
            let _ = write!(main, "<div class=\"label\">{label}</div><div>{value}</div>");
        }
        main.push_str("</div>\n");
    }
    let fields: Vec<&Item> = item.members.iter().filter(|m| m.kind == ItemKind::Field).collect();
    let methods: Vec<&Item> = item.members.iter().filter(|m| m.kind == ItemKind::Method).collect();
    let values: Vec<&Item> = item.members.iter().filter(|m| m.kind == ItemKind::Value).collect();
    if item.kind == ItemKind::Enum && !values.is_empty() {
        let instance_fields: Vec<&&Item> = fields.iter().filter(|f| !f.is_static).collect();
        let _ = writeln!(main, "<div class=\"section-head\"><h2>Values</h2><span class=\"muted\">{}</span></div>", values.len());
        main.push_str("<div class=\"table-box\"><table><thead><tr><th scope=\"col\">Value</th>");
        for field in &instance_fields {
            let _ = write!(main, "<th scope=\"col\" class=\"num\"><code>{}</code></th>", esc(&field.name));
        }
        main.push_str("<th scope=\"col\">Description</th></tr></thead><tbody>");
        for value in &values {
            let _ = write!(main, "<tr id=\"{}\"><td><code class=\"n\">{}</code></td>", anchor(&value.name), esc(&value.name));
            for i in 0..instance_fields.len() {
                let _ = write!(main, "<td class=\"num\"><code>{}</code></td>", esc(value.args.get(i).map(String::as_str).unwrap_or("")));
            }
            let _ = write!(main, "<td>{}</td></tr>", md_inline(&value.doc.text));
        }
        main.push_str("</tbody></table></div>\n");
    }
    if !fields.is_empty() {
        main.push_str("<h2>Fields</h2>\n<div class=\"table-box\"><table><tbody>");
        for field in &fields {
            let _ = write!(main, "<tr id=\"{}\"><td class=\"nowrap\"><code>{}</code></td><td>{}</td></tr>", anchor(&field.name), code(&field.code, &field.name, types), md_inline(&field.doc.text));
        }
        main.push_str("</tbody></table></div>\n");
    }
    if item.kind == ItemKind::Interface {
        let required: Vec<&&Item> = methods.iter().filter(|m| !m.default).collect();
        let given: Vec<&&Item> = methods.iter().filter(|m| m.default).collect();
        if !required.is_empty() {
            let _ = writeln!(main, "<h2>To implement</h2>\n<p class=\"muted\">Every class that implements <code>{}</code> writes these.</p>\n<div class=\"cards\">", esc(&item.name));
            for method in required {
                main.push_str(&card(method, &module.file, types, ""));
            }
            main.push_str("</div>\n");
        }
        if !given.is_empty() {
            main.push_str("<h2>Given</h2>\n<p class=\"muted\">Classes get these as they are, or write their own.</p>\n<div class=\"cards\">\n");
            for method in given {
                main.push_str(&card(method, &module.file, types, ""));
            }
            main.push_str("</div>\n");
        }
    } else {
        let own: Vec<&&Item> = methods.iter().filter(|m| m.from.is_none()).collect();
        if !own.is_empty() {
            main.push_str("<h2>Methods</h2>\n<div class=\"cards\">\n");
            for method in own {
                main.push_str(&card(method, &module.file, types, ""));
            }
            main.push_str("</div>\n");
        }
        // The methods of each interface it implements, which document them
        for interface in &item.implements {
            let of: Vec<&&Item> = methods.iter().filter(|m| m.from.as_deref() == Some(interface.as_str())).collect();
            if of.is_empty() {
                continue;
            }
            let _ = writeln!(main, "<h2>From {}</h2>\n<div class=\"cards\">", link(interface));
            for method in of {
                // Without a comment of its own, the interface's
                let mut method = (*method).clone();
                if method.doc.is_empty()
                    && let Some(doc) = interface_doc(site, interface, &method.name)
                {
                    method.doc = doc;
                }
                main.push_str(&card(&method, &module.file, types, ""));
            }
            main.push_str("</div>\n");
        }
    }
    if item.kind == ItemKind::Enum {
        let name = esc(&item.name);
        let _ = writeln!(
            main,
            "<div class=\"every\"><strong>Every enum has</strong><code>{name}::values()</code><code>{name}::from($name)</code><code>{name}::fromIgnoreCase($name)</code><code>.name()</code></div>"
        );
    }
    page_at(site, &item.name, Some((module, &module.doc.items)), Some(&item.name), None, &main)
}

/// The comment of a method of an interface of the project, or of the
/// interfaces it extends.
fn interface_doc(site: &Site, interface: &str, method: &str) -> Option<Doc> {
    let item = site.modules.iter().flat_map(|m| &m.doc.items).find(|i| i.kind == ItemKind::Interface && i.name == interface)?;
    if let Some(found) = item.members.iter().find(|m| m.name == method && !m.doc.is_empty()) {
        return Some(found.doc.clone());
    }
    item.extends.iter().find_map(|parent| interface_doc(site, parent, method))
}

fn folder_page(site: &Site, folder: &FolderPage) -> String {
    let mut main = breadcrumb(site, &[(format!("{}/", folder.short), None)]);
    let _ = writeln!(main, "<div class=\"title\"><h1>{}/</h1>{}</div>\n<article class=\"doc big\">{}</article>", esc(&folder.short), badge("folder"), md(&folder.text));
    let inside: Vec<&ModulePage> = site.modules.iter().filter(|m| m.short.starts_with(&format!("{}/", folder.short))).collect();
    if !inside.is_empty() {
        main.push_str("<h2>Modules</h2>\n<div class=\"tiles\">\n");
        for module in inside {
            let summary = module.doc.text.as_ref().map(|t| Doc { text: t.clone(), ..Doc::default() }.summary()).unwrap_or_default();
            let _ = writeln!(main, "<a class=\"tile\" href=\"{}\"><code>{}</code><span>{}</span></a>", module_href(module), esc(&module.path), md_inline(&summary));
        }
        main.push_str("</div>\n");
    }
    page(site, &folder.short, None, None, &main)
}

fn arg_rows(args: &[ArgDoc]) -> String {
    let mut out = String::new();
    for arg in args {
        let mut what = arg.text.as_deref().map(md_inline).unwrap_or_default();
        // A comment of a flag is often a phrase without its period
        if !what.is_empty() && !what.trim_end().ends_with(['.', '!', '?', ':']) {
            what.push('.');
        }
        if !arg.choices.is_empty() {
            let choices: Vec<String> = arg.choices.iter().map(|c| format!("<code>{}</code>", esc(c))).collect();
            let _ = write!(what, " One of {}.", choices.join(", "));
        }
        let mut notes = Vec::new();
        if arg.required {
            notes.push("Required.".to_string());
        }
        if let Some(default) = &arg.default {
            notes.push(format!("By default <code>{}</code>.", esc(default)));
        }
        if arg.repeats {
            notes.push("Can be given several times.".to_string());
        }
        if !notes.is_empty() {
            let _ = write!(what, " <span class=\"muted\">{}</span>", notes.join(" "));
        }
        let class = if arg.option { "n" } else { "p" };
        let _ = write!(out, "<code class=\"{class}\">{}</code><div>{}</div>", esc(&arg.name), what.trim());
    }
    out
}

fn command_page(site: &Site, page_of: &CommandPage) -> String {
    let mut main = format!(
        "<div class=\"title\"><h1 class=\"mono\">{}</h1>{}<span class=\"where\">bin/{}.lks</span></div>\n",
        esc(&page_of.name),
        badge("command"),
        esc(&page_of.name)
    );
    if let Some(text) = &page_of.text {
        let _ = writeln!(main, "<div class=\"doc big\">{}</div>", md(text));
    }
    match &page_of.command {
        None => {
            let _ = writeln!(main, "<div class=\"use\"><code>ferry run {} [arguments...]</code></div>", esc(&page_of.name));
        }
        Some(command) => {
            if page_of.text.is_none()
                && let Some(text) = &command.text
            {
                let _ = writeln!(main, "<div class=\"doc big\">{}</div>", md(text));
            }
            let _ = writeln!(main, "<div class=\"use\"><code>{}</code><button type=\"button\" data-copy=\"{}\">Copy</button></div>", esc(&command.usage), esc(&command.usage));
            let mut args = command.args.clone();
            args.push(ArgDoc { name: "-h, --help".into(), option: true, text: Some("Show the help.".into()), choices: Vec::new(), default: None, repeats: false, required: false });
            let _ = writeln!(main, "<h2>Arguments and options</h2>\n<div class=\"arg-grid box\">{}</div>", arg_rows(&args));
            if !command.subcommands.is_empty() {
                main.push_str("<h2>Subcommands</h2>\n<div class=\"cards\">\n");
                for sub in &command.subcommands {
                    let id = sub.name.rsplit(' ').next().unwrap_or(&sub.name);
                    let text = sub.text.as_deref().map(|t| format!("<div class=\"doc\">{}</div>", md(t))).unwrap_or_default();
                    let grid = if sub.args.is_empty() { String::new() } else { format!("<div class=\"arg-grid\">{}</div>", arg_rows(&sub.args)) };
                    let _ = writeln!(
                        main,
                        "<section class=\"card\" id=\"{}\"><div class=\"card-head\"><code>{}</code></div><div class=\"card-body\">{text}{grid}</div></section>",
                        esc(id),
                        esc(&sub.usage)
                    );
                }
                main.push_str("</div>\n");
            }
            let _ = writeln!(main, "<p class=\"muted\">The same text as <code>{} --help</code>, from the script's <code>command</code>.</p>", esc(&page_of.name));
        }
    }
    page(site, &page_of.name, None, Some(&page_of.name), &main)
}

// ---- Search ----

fn json(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Everything search can find: its name, its full path, its kind, the first
/// line of its comment and its page.
fn index(site: &Site) -> String {
    let mut entries = Vec::new();
    let mut add = |path: String, kind: &str, summary: String, href: String| {
        let summary = plain(&summary).replace('`', "");
        entries.push(format!("{{\"p\":{},\"k\":{},\"s\":{},\"h\":{}}}", json(&path), json(kind), json(&summary), json(&href)));
    };
    for module in &site.modules {
        let last = module.short.rsplit('/').next().unwrap_or(&module.short).to_string();
        let summary = module.doc.text.as_ref().map(|t| Doc { text: t.clone(), ..Doc::default() }.summary()).unwrap_or_default();
        add(module.path.clone(), "module", summary, module_href(module));
        for item in &module.doc.items {
            let href = if has_page(item) { type_href(module, &item.name) } else { format!("{}#{}", module_href(module), anchor(&item.name)) };
            add(format!("{last}::{}", item.name), item.kind.word(), item.doc.summary(), href.clone());
            for member in &item.members {
                let page = type_href(module, &item.name);
                add(format!("{last}::{}.{}", item.name, member.name), member.kind.word(), member.doc.summary(), format!("{page}#{}", anchor(&member.name)));
            }
        }
    }
    for command in &site.commands {
        add(command.name.clone(), "command", command.summary.clone(), command_href(&command.name));
    }
    format!("{{\"project\":{},\"items\":[{}]}}", json(&site.id), entries.join(","))
}

// ---- The terminal ----

/// The docs of one thing as text, for `ferry doc <name>`: a module *(i.e.
/// `units`)*, an item *(i.e. `units::bytes`, `bytes`)* or a member *(i.e.
/// `Circle.area`)*. `None` when there's nothing with that name.
pub fn describe(site: &Site, name: &str) -> Option<String> {
    let (module_name, rest) = match name.split_once("::") {
        Some((m, r)) => (Some(m), r),
        None => (None, name),
    };
    let module_matches = |m: &ModulePage, wanted: &str| m.short == wanted || m.path == wanted || m.short.rsplit('/').next() == Some(wanted);
    // A module by itself
    if module_name.is_none()
        && let Some(module) = site.modules.iter().find(|m| module_matches(m, name))
    {
        let mut out = format!("module {}{:>width$}\n", module.path, module.file, width = 80usize.saturating_sub(module.path.len() + 7));
        if let Some(text) = &module.doc.text {
            let _ = writeln!(out, "\n{}", plain(text));
        }
        if !module.doc.items.is_empty() {
            out.push('\n');
            for item in &module.doc.items {
                let summary = plain(&item.doc.summary());
                let _ = writeln!(out, "  {}{}", item.code, if summary.is_empty() { String::new() } else { format!("\n      {summary}") });
            }
        }
        return Some(out);
    }
    let (item_name, member_name) = match rest.split_once('.') {
        Some((i, m)) => (i, Some(m)),
        None => (rest, None),
    };
    for module in site.modules.iter().filter(|m| module_name.is_none_or(|n| module_matches(m, n))) {
        let wanted = |i: &&Item| i.name == item_name || i.name.trim_start_matches('$') == item_name.trim_start_matches('$');
        let Some(item) = module.doc.items.iter().find(wanted) else { continue };
        let target = match member_name {
            Some(m) => item.members.iter().find(|x| x.name == m || x.name.trim_start_matches('$') == m)?,
            None => item,
        };
        return Some(describe_item(target, &module.file));
    }
    None
}

/// Markdown read as text: emphasis without its marks.
fn plain(text: &str) -> String {
    text.replace("*(", "(").replace(")*", ")").replace("**", "")
}

fn describe_item(item: &Item, file: &str) -> String {
    let place = format!("{file}:{}", item.line);
    let width = 80usize.saturating_sub(item.code.chars().count()).max(place.len() + 2);
    let mut out = format!("{}{place:>width$}\n", item.code);
    if !item.doc.text.is_empty() {
        let _ = writeln!(out, "\n{}", plain(&item.doc.text));
    }
    let mut rows: Vec<(String, String)> = item.doc.params.iter().map(|(n, w)| (format!("${n}"), w.clone())).collect();
    if let Some(r) = &item.doc.returns {
        rows.push(("returns".into(), r.clone()));
    }
    for (e, w) in &item.doc.throws {
        rows.push(("throws".into(), format!("{e} {w}")));
    }
    if !rows.is_empty() {
        let width = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
        out.push('\n');
        for (label, what) in rows {
            let _ = writeln!(out, "  {label:width$}  {}", plain(&what));
        }
    }
    if !item.members.is_empty() {
        out.push('\n');
        for member in &item.members {
            let summary = plain(&member.doc.summary());
            let _ = writeln!(out, "  {}{}", member.code, if summary.is_empty() { String::new() } else { format!("\n      {summary}") });
        }
    }
    out
}

// ---- What every page uses ----

/// The Mist palette, light and dark as the system is *(see the table in
/// `ideas/tooling/docs.md`)*.
const STYLE: &str = r#":root{--bg:#F5F8FA;--surface:#FFFFFF;--panel:#EFF3F6;--code:#EDF2F5;--line:#DDE5EA;--line2:#E9EEF2;--input:#C6D1D9;--ink:#1B2A35;--text:#2B3B47;--muted:#5A6976;--accent:#3F6E8C;--accent-ink:#2C5470;--accent-soft:#E4EEF4;--type:#3F6E8C;--param:#8A5A3C;--kw:#5D5A8E;--mark:#F3E7C9;--b-class:#4A4675;--b-class-bg:#ECEBF6;--b-iface:#2F6B62;--b-iface-bg:#E2F0EC;--b-enum:#7A5232;--b-enum-bg:#F4ECE4;color-scheme:light}
@media (prefers-color-scheme: dark){:root{--bg:#10161B;--surface:#151D23;--panel:#121A20;--code:#1A242C;--line:#26323B;--line2:#1F2A32;--input:#34434F;--ink:#E4EBF0;--text:#C6D2DA;--muted:#8FA0AC;--accent:#8DB8D4;--accent-ink:#B3D2E6;--accent-soft:#1C2A35;--type:#8DB8D4;--param:#D9AE8A;--kw:#B3AEDF;--mark:#4A3F25;--b-class:#C9C5EE;--b-class-bg:#272840;--b-iface:#9CD3C8;--b-iface-bg:#1A302D;--b-enum:#E2BE9C;--b-enum-bg:#33281F;color-scheme:dark}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font:16px/1.55 Inter,system-ui,-apple-system,"Segoe UI",sans-serif}
a{color:var(--accent);text-decoration:none}a:hover{color:var(--accent-ink);text-decoration:underline}
a:focus-visible,button:focus-visible,input:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
code,pre,.mono{font-family:"JetBrains Mono",ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.92em}
.t{color:var(--type)}.p{color:var(--param)}.k{color:var(--kw)}.n{color:var(--ink);font-weight:600}
.muted{color:var(--muted)}.nowrap{white-space:nowrap}
.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
header.top{position:sticky;top:0;z-index:5;display:flex;flex-wrap:wrap;align-items:center;gap:12px 20px;padding:12px max(16px,2vw);background:var(--surface);border-bottom:1px solid var(--line)}
.brand{display:flex;align-items:center;gap:10px;color:var(--ink);font-weight:700;font-size:18px}.brand svg{color:var(--accent)}.brand:hover{text-decoration:none;color:var(--ink)}
.pill{font-size:13px;color:var(--muted);padding:2px 8px;border:1px solid var(--line);border-radius:999px}
.search{margin-left:auto;flex:0 1 420px;min-width:200px;position:relative}
.search input{width:100%;padding:9px 12px;border:1px solid var(--input);border-radius:8px;font:inherit;font-size:14px;background:var(--bg);color:var(--ink)}
.results{position:absolute;right:0;top:calc(100% + 8px);width:min(560px,calc(100vw - 32px));max-height:70vh;overflow-y:auto;border:1px solid var(--line);border-radius:12px;background:var(--surface);box-shadow:0 12px 32px rgba(16,32,48,.16)}
.results .group{padding:10px 16px 6px;font-size:12px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--muted)}
.results a{display:block;padding:9px 16px;color:var(--ink)}.results a:hover,.results a.sel{background:var(--accent-soft);text-decoration:none}
.results .line{display:flex;align-items:center;gap:10px}.results .sum{display:block;margin-top:3px;font-size:13px;color:var(--muted)}
.results mark{background:var(--mark);color:inherit;border-radius:3px}
.results .keys{display:flex;flex-wrap:wrap;gap:6px 18px;padding:9px 16px;border-top:1px solid var(--line2);background:var(--panel);font-size:12px;color:var(--muted)}
.results .empty{padding:14px 16px;color:var(--muted);font-size:14px}
kbd{font-family:inherit;padding:1px 6px;border:1px solid var(--line);border-radius:4px;background:var(--surface)}
.layout{display:flex;flex-wrap:wrap;min-height:calc(100vh - 60px)}
nav.side{flex:1 1 220px;max-width:280px;padding:24px;border-right:1px solid var(--line2);background:var(--panel);font-size:14px}
.side-title{font-size:12px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);margin:0 0 10px}.side-title+ul+.side-title,.side-list+.side-title{margin-top:22px}
.side-list,.side-members{list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:2px}
.side-list a{display:block;padding:6px 10px;border-radius:6px}.side-list a.current{background:var(--accent-soft);color:var(--accent-ink);font-weight:600}
.side-members a.here{color:var(--ink);font-weight:600}
.side-members{margin:4px 0 6px 12px;padding-left:10px;border-left:2px solid var(--accent);font-family:"JetBrains Mono",ui-monospace,monospace;font-size:13px}.side-members a{padding:3px 6px}
main{flex:999 1 560px;min-width:0;padding:32px 40px 64px}.content{max-width:860px}
@media (max-width:640px){main{padding:24px 16px 48px}nav.side{max-width:none}}
.crumbs{font-size:13px;color:var(--muted);margin-bottom:14px;font-family:"JetBrains Mono",ui-monospace,monospace}
.title{display:flex;flex-wrap:wrap;align-items:baseline;gap:12px}.title h1{margin:0;font-size:40px;line-height:1.1;letter-spacing:-.01em;overflow-wrap:anywhere}.title .where{margin-left:auto}
.where{font-size:13px;color:var(--muted)}
.badge{font-size:12px;font-weight:600;padding:3px 9px;border-radius:999px;color:var(--accent-ink);background:var(--accent-soft)}
.badge.class{color:var(--b-class);background:var(--b-class-bg)}.badge.interface,.badge.command{color:var(--b-iface);background:var(--b-iface-bg)}.badge.enum{color:var(--b-enum);background:var(--b-enum-bg)}
.facts{display:flex;flex-wrap:wrap;gap:10px 28px;margin-top:14px;font-size:14px;color:var(--muted)}.facts strong{color:var(--ink);font-weight:600}
.use{display:flex;align-items:center;gap:10px;margin-top:16px;padding:10px 14px;background:var(--code);border:1px solid var(--line);border-radius:8px}.use code{flex:1;min-width:0;overflow-wrap:anywhere}
button{font:inherit;font-size:13px;padding:6px 12px;min-height:32px;border:1px solid var(--input);border-radius:6px;background:var(--surface);color:var(--ink);cursor:pointer}
.doc{color:var(--text);line-height:1.65}.doc.big{margin-top:22px;font-size:17px}.doc p{margin:0 0 12px}.doc>:last-child{margin-bottom:0}
.doc pre,pre{padding:14px 16px;background:var(--code);border-radius:8px;overflow-x:auto;font-size:14px;line-height:1.6}
h2{margin:44px 0 14px;font-size:22px}
.cards{display:flex;flex-direction:column;gap:16px}
.card{border:1px solid var(--line);border-radius:10px;background:var(--surface);overflow:hidden}
.card-head{display:flex;flex-wrap:wrap;align-items:center;gap:8px 16px;padding:14px 20px;background:var(--panel);border-bottom:1px solid var(--line)}.card-head code{flex:1;min-width:0;font-size:15px;overflow-wrap:anywhere}.card-head:last-child{border-bottom:0}
.card-body{padding:16px 20px 18px}.card-body .doc{margin-bottom:14px}.card-body>:last-child{margin-bottom:0}
.details,.facts-box{display:grid;grid-template-columns:110px minmax(0,1fr);gap:10px 16px;font-size:15px;line-height:1.5}
.facts-box{margin-top:26px;padding:18px 20px;border:1px solid var(--line);border-radius:10px;background:var(--surface);grid-template-columns:140px minmax(0,1fr)}
.label{font-size:12px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);padding-top:2px}
.arg-grid{display:grid;grid-template-columns:minmax(150px,max-content) minmax(0,1fr);gap:10px 20px;font-size:15px;line-height:1.5}.arg-grid.box{padding:18px 20px;border:1px solid var(--line);border-radius:10px;background:var(--surface)}
.tiles{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:14px}
.tile{display:flex;flex-direction:column;gap:6px;padding:16px 18px;border:1px solid var(--line);border-radius:10px;background:var(--surface);color:var(--ink);font-size:14px}.tile:hover{text-decoration:none;border-color:var(--accent)}.tile code{color:var(--accent);font-size:15px}
.tile.wide{flex-direction:row;flex-wrap:wrap;align-items:baseline;gap:8px 12px}.cards .tile span:last-child{color:var(--muted)}
.rows{border:1px solid var(--line);border-radius:10px;background:var(--surface)}.row{display:flex;flex-wrap:wrap;align-items:baseline;gap:6px 18px;padding:13px 18px;border-bottom:1px solid var(--line2)}.row:last-child{border-bottom:0}.row>a,.row>code:first-child{min-width:110px}.row>span{flex:1;min-width:200px}
.section-head{display:flex;align-items:baseline;gap:12px}.section-head h2{margin-bottom:14px}
.table-box{border:1px solid var(--line);border-radius:10px;background:var(--surface);overflow-x:auto}
table{width:100%;border-collapse:collapse;font-size:15px;line-height:1.5}th{text-align:left;padding:10px 18px;font-size:12px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);background:var(--panel);border-bottom:1px solid var(--line)}td{padding:11px 18px;border-bottom:1px solid var(--line2);vertical-align:top;color:var(--text)}tr:last-child td{border-bottom:0}.num{text-align:right}th code{text-transform:none;letter-spacing:0}
.every{display:flex;flex-wrap:wrap;align-items:center;gap:8px 14px;margin-top:22px;padding:14px 18px;border-radius:10px;background:var(--panel);font-size:15px}
"#;

/// Search, fuzzy, with the keys of the design: `/` to search, the arrows,
/// Enter and Escape.
const SEARCH: &str = r#"(function () {
  var input = document.getElementById('search'), box = document.getElementById('results');
  var items = window.DOCS.items, sel = 0, shown = [];
  function esc(s) { return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
  // The letters of the query in order, with points for ones next to each other and at the start of words
  function score(q, text) {
    var t = text.toLowerCase(), i = 0, s = 0, last = -2, hits = [];
    for (var j = 0; j < t.length && i < q.length; j++) {
      if (t[j] === q[i]) {
        s += last === j - 1 ? 3 : 1;
        if (j === 0 || /[^a-z0-9]/.test(t[j - 1])) s += 2;
        last = j; hits.push(j); i++;
      }
    }
    if (i < q.length) return null;
    if (t.indexOf(q) >= 0) s += 6;
    return { s: s - t.length * 0.02, hits: hits };
  }
  function marked(text, hits) {
    var out = '', set = {};
    hits.forEach(function (h) { set[h] = true; });
    for (var i = 0; i < text.length; i++) out += set[i] ? '<mark>' + esc(text[i]) + '</mark>' : esc(text[i]);
    return out;
  }
  function render() {
    var q = input.value.trim().toLowerCase();
    if (!q) { box.hidden = true; return; }
    shown = [];
    items.forEach(function (it) {
      var r = score(q, it.p);
      if (r) shown.push({ it: it, r: r });
    });
    shown.sort(function (a, b) { return b.r.s - a.r.s; });
    shown = shown.slice(0, 20);
    sel = 0;
    var html = '<div class="group">In ' + esc(window.DOCS.project) + '</div>';
    if (!shown.length) html += '<div class="empty">Nothing matches.</div>';
    shown.forEach(function (x, n) {
      html += '<a role="option" href="' + x.it.h + '"' + (n === 0 ? ' class="sel" aria-selected="true"' : '') + '>' +
        '<span class="line"><span class="badge ' + x.it.k + '">' + x.it.k + '</span><code>' + marked(x.it.p, x.r.hits) + '</code></span>' +
        (x.it.s ? '<span class="sum">' + esc(x.it.s) + '</span>' : '') + '</a>';
    });
    html += '<div class="keys"><span><kbd>↑</kbd> <kbd>↓</kbd> to move</span><span><kbd>Enter</kbd> to open</span><span><kbd>/</kbd> to search from anywhere</span><span style="margin-left:auto">' + shown.length + ' results</span></div>';
    box.innerHTML = html;
    box.hidden = false;
  }
  function move(by) {
    var links = box.querySelectorAll('a');
    if (!links.length) return;
    links[sel].classList.remove('sel'); links[sel].removeAttribute('aria-selected');
    sel = (sel + by + links.length) % links.length;
    links[sel].classList.add('sel'); links[sel].setAttribute('aria-selected', 'true');
    links[sel].scrollIntoView({ block: 'nearest' });
  }
  input.addEventListener('input', render);
  input.addEventListener('keydown', function (e) {
    if (e.key === 'ArrowDown') { e.preventDefault(); move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === 'Enter') { var a = box.querySelectorAll('a')[sel]; if (a) location.href = a.getAttribute('href'); }
    else if (e.key === 'Escape') { box.hidden = true; input.blur(); }
  });
  document.addEventListener('keydown', function (e) {
    if (e.key === '/' && document.activeElement !== input) { e.preventDefault(); input.focus(); input.select(); render(); }
  });
  document.addEventListener('click', function (e) { if (!box.contains(e.target) && e.target !== input) box.hidden = true; });
  document.querySelectorAll('[data-copy]').forEach(function (b) {
    b.addEventListener('click', function () {
      navigator.clipboard && navigator.clipboard.writeText(b.getAttribute('data-copy')).then(function () { b.textContent = 'Copied'; setTimeout(function () { b.textContent = 'Copy'; }, 1200); });
    });
  });
})();
"#;
