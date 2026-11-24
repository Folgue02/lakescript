//! `lks-lsp`, the Lakescript language server *(see `ideas/tooling/lsp.md`)*.
//!
//! Editors start it and talk to it on stdin and stdout. It checks the files
//! the editor has open with the same checker as `lks`, using what the editor
//! has, saved or not, and sends back the errors.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use lakescript::{CapturedOutput, Diagnostic, Engine, Name};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _, PublishDiagnostics};
use lsp_types::request::{Completion, GotoDefinition, HoverRequest, Request as _};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse, CompletionTextEdit, DiagnosticSeverity, TextEdit, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverContents, HoverParams, HoverProviderCapability, Location, MarkupContent, MarkupKind, OneOf, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities, TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

/// Checks run on a thread with a big stack, like `lks`'s scripts.
const STACK_SIZE: usize = 512 * 1024 * 1024;

fn main() {
    if std::env::args().nth(1).is_some_and(|a| a == "--version" || a == "-V") {
        println!("lks-lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let server = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(run).expect("the server thread starts");
    if let Err(e) = server.join().unwrap_or_else(|_| Err("the server stopped unexpectedly".into())) {
        eprintln!("lks-lsp: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        // The editor sends the whole text on every change
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions { trigger_characters: Some(vec![".".into(), "$".into(), "@".into(), ":".into()]), ..Default::default() }),
        ..Default::default()
    };
    let capabilities = serde_json::to_value(capabilities).map_err(|e| e.to_string())?;
    connection.initialize(capabilities).map_err(|e| e.to_string())?;
    let mut server = Server::default();
    // Whether the files changed since they were last checked. Changes that
    // arrive together *(i.e. the keys typed while a check ran)* are one check:
    // it runs when nothing else is waiting, or before a question is answered.
    let mut stale = false;
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request).map_err(|e| e.to_string())? {
                    break;
                }
                if std::mem::take(&mut stale) {
                    server.check_all(&connection);
                }
                let _ = connection.sender.send(Message::Response(server.answer(request)));
            }
            Message::Notification(notification) => stale |= server.handle(notification),
            Message::Response(_) => {}
        }
        if stale && connection.receiver.is_empty() {
            stale = false;
            server.check_all(&connection);
        }
    }
    drop(connection);
    io_threads.join().map_err(|e| e.to_string())
}

#[derive(Default)]
struct Server {
    /// The text of every file the editor has open.
    documents: HashMap<PathBuf, String>,
    /// The files that have errors in the editor now, to clear them when they're fixed.
    published: HashSet<PathBuf>,
    /// What the checker found about the names of each open file, for hover and go to definition.
    names: HashMap<PathBuf, Vec<Name>>,
}

impl Server {
    /// Takes in a notification. Returns whether the files must be checked again.
    fn handle(&mut self, notification: Notification) -> bool {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let Ok(params) = notification.extract::<DidOpenTextDocumentParams>(DidOpenTextDocument::METHOD) else { return false };
                let Some(path) = uri_to_path(&params.text_document.uri) else { return false };
                self.documents.insert(path, params.text_document.text);
                true
            }
            DidChangeTextDocument::METHOD => {
                let Ok(params) = notification.extract::<DidChangeTextDocumentParams>(DidChangeTextDocument::METHOD) else { return false };
                let Some(path) = uri_to_path(&params.text_document.uri) else { return false };
                // The whole text: the server asked for full syncing
                if let Some(change) = params.content_changes.into_iter().last() {
                    self.documents.insert(path, change.text);
                }
                true
            }
            DidCloseTextDocument::METHOD => {
                let Ok(params) = notification.extract::<DidCloseTextDocumentParams>(DidCloseTextDocument::METHOD) else { return false };
                if let Some(path) = uri_to_path(&params.text_document.uri) {
                    self.documents.remove(&path);
                    self.names.remove(&path);
                }
                true
            }
            DidSaveTextDocument::METHOD => true,
            _ => false,
        }
    }

    /// Checks every open file, and sends the errors of every file they reach.
    /// A change in a module can break the files that use it, so they're all
    /// checked: it takes a few milliseconds.
    fn check_all(&mut self, connection: &Connection) {
        let mut found: HashMap<PathBuf, Vec<lsp_types::Diagnostic>> = HashMap::new();
        let paths: Vec<PathBuf> = self.documents.keys().filter(|p| p.extension().is_some_and(|e| e == "lks")).cloned().collect();
        for path in &paths {
            found.entry(path.clone()).or_default();
            let (result, names) = self.check(path);
            // A file that doesn't parse has no names: the last ones stay, for hover while it's being written
            if !names.is_empty() || result.is_ok() {
                self.names.insert(path.clone(), names);
            }
            if let Err(error) = result {
                let file = error.file.as_ref().map(PathBuf::from).unwrap_or_else(|| path.clone());
                let file = lakescript::canonical(&file);
                let text = self.text_of(&file);
                let diagnostic = to_lsp(&error, text.as_deref());
                let list = found.entry(file).or_default();
                if !list.contains(&diagnostic) {
                    list.push(diagnostic);
                }
            }
        }
        // Files that had errors and don't anymore are cleared
        for path in self.published.iter() {
            found.entry(path.clone()).or_default();
        }
        self.published = found.iter().filter(|(_, d)| !d.is_empty()).map(|(p, _)| p.clone()).collect();
        for (path, diagnostics) in found {
            let Some(uri) = path_to_uri(&path) else { continue };
            let params = PublishDiagnosticsParams { uri, diagnostics, version: None };
            let _ = connection.sender.send(Message::Notification(Notification::new(PublishDiagnostics::METHOD.to_string(), params)));
        }
    }

    /// The text of a file: what the editor has, or what's on the disk.
    fn text_of(&self, path: &Path) -> Option<String> {
        self.documents.get(path).cloned().or_else(|| std::fs::read_to_string(path).ok())
    }

    /// Checks one file, and returns what was found about its names.
    fn check(&self, path: &Path) -> (Result<(), Diagnostic>, Vec<Name>) {
        let mut engine = Engine::with_output(CapturedOutput::new());
        engine.set_stack_budget(STACK_SIZE - 16 * 1024 * 1024);
        engine.set_names(true);
        let text = self.documents.get(path).cloned().unwrap_or_default();
        let (result, as_module) = self.run_check(&mut engine, path, &text, None);
        let shown = path.display().to_string();
        let names = engine
            .names()
            .into_iter()
            .filter_map(|mut name| {
                // The script checked is the file itself, unless the file was checked as a module
                match name.file.take() {
                    None if !as_module => {}
                    Some(file) if file == shown => {}
                    _ => return None,
                }
                if let Some(d) = &mut name.definition
                    && d.file.is_none()
                {
                    d.file = Some(shown.clone());
                }
                Some(name)
            })
            .collect();
        (result, names)
    }

    /// Checks one file like `lks` or `ferry` would: a module of a ferry
    /// project with the project's module path, any other file as a script.
    /// `text` is the file's, and `complete` where completion is wanted in it.
    /// Returns whether it was checked as a module.
    fn run_check(&self, engine: &mut Engine, path: &Path, text: &str, complete: Option<usize>) -> (Result<(), Diagnostic>, bool) {
        for (open, text) in &self.documents {
            engine.set_file_text(open, text.clone());
        }
        engine.set_file_text(path, text);
        let folder = path.parent().unwrap_or(Path::new("."));
        let project = match ferry::project::Project::find(folder) {
            Ok(project) => project,
            // A `ferry.toml` that can't be read: its error, instead of checking the file as if there were no project
            Err(e) if folder.ancestors().any(|d| d.join(ferry::project::MANIFEST).is_file()) => {
                return (Err(Diagnostic::new(e.0, Default::default())), false);
            }
            Err(_) => {
                if let Some(offset) = complete {
                    engine.set_completion(None, offset);
                }
                return (engine.check_text(path, text), false);
            }
        };
        let module_path = match ferry::home::Home::open() {
            Ok(home) => ferry::resolve::installed_module_path(&home, &project),
            Err(_) => vec![project.src()],
        };
        for root in &module_path {
            engine.add_module_root(root.clone());
        }
        // A module is checked like ferry does, by using it, so only declarations are allowed in it
        let src = lakescript::canonical(&project.src());
        if let Ok(relative) = path.strip_prefix(&src) {
            if let Some(offset) = complete {
                engine.set_completion(Some(path.display().to_string()), offset);
            }
            let module: Vec<String> = relative.with_extension("").iter().map(|p| p.to_string_lossy().into_owned()).collect();
            let result = engine.check_source(&format!("use {} as module;\n", module.join("/"))).map_err(|mut e| {
                // An error of the `use` line itself is the module's
                if e.file.is_none() {
                    e.file = Some(path.display().to_string());
                    e.span = Default::default();
                }
                e
            });
            return (result.map_err(|e| missing_dependency(e, &project)), true);
        }
        if let Some(offset) = complete {
            engine.set_completion(None, offset);
        }
        (engine.check_text(path, text).map_err(|e| missing_dependency(e, &project)), false)
    }

    /// Answers a request of the editor.
    fn answer(&self, request: Request) -> Response {
        let id = request.id.clone();
        let result = match request.method.as_str() {
            HoverRequest::METHOD => match request.extract::<HoverParams>(HoverRequest::METHOD) {
                Ok((_, params)) => serde_json::to_value(self.hover(&params.text_document_position_params)),
                Err(e) => return Response::new_err(id, lsp_server::ErrorCode::InvalidParams as i32, format!("{e:?}")),
            },
            GotoDefinition::METHOD => match request.extract::<GotoDefinitionParams>(GotoDefinition::METHOD) {
                Ok((_, params)) => serde_json::to_value(self.definition(&params.text_document_position_params)),
                Err(e) => return Response::new_err(id, lsp_server::ErrorCode::InvalidParams as i32, format!("{e:?}")),
            },
            Completion::METHOD => match request.extract::<CompletionParams>(Completion::METHOD) {
                Ok((_, params)) => serde_json::to_value(self.complete(&params.text_document_position)),
                Err(e) => return Response::new_err(id, lsp_server::ErrorCode::InvalidParams as i32, format!("{e:?}")),
            },
            other => return Response::new_err(id, lsp_server::ErrorCode::MethodNotFound as i32, format!("`{other}` isn't supported yet")),
        };
        match result {
            Ok(value) => Response::new_ok(id, value),
            Err(e) => Response::new_err(id, lsp_server::ErrorCode::InternalError as i32, e.to_string()),
        }
    }

    /// The name under the cursor, and the text of its file.
    fn name_at(&self, at: &TextDocumentPositionParams) -> Option<(&Name, String)> {
        let path = uri_to_path(&at.text_document.uri)?;
        let text = self.text_of(&path)?;
        let offset = offset(&text, at.position);
        let names = self.names.get(&path)?;
        // The innermost name, also with the cursor right after it
        let name = names.iter().filter(|n| n.start <= offset && offset <= n.end).min_by_key(|n| (n.end - n.start, n.end == offset))?;
        Some((name, text))
    }

    fn hover(&self, at: &TextDocumentPositionParams) -> Option<Hover> {
        let (name, text) = self.name_at(at)?;
        let mut value = format!("```lakescript\n{}\n```", name.code);
        for part in [&name.about, &name.doc].into_iter().flatten() {
            value.push_str("\n\n");
            value.push_str(part);
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value }),
            range: Some(Range { start: position(&text, name.start), end: position(&text, name.end) }),
        })
    }

    /// What can be written at the cursor, from what's before it: `.`, `$`, `@`, `name::` or a word.
    fn complete(&self, at: &TextDocumentPositionParams) -> Option<CompletionResponse> {
        let path = uri_to_path(&at.text_document.uri)?;
        let mut text = self.text_of(&path)?;
        let cursor = offset(&text, at.position);
        let chars: Vec<char> = text.chars().collect();
        // The part of the name already written
        let word = |c: char| c.is_alphanumeric() || c == '_' || c == '?';
        let mut start = cursor;
        while start > 0 && word(chars[start - 1]) {
            start -= 1;
        }
        let before = start.checked_sub(1).map(|i| chars[i]);
        let context = match before {
            Some('.') if start >= 2 && chars[start - 2] == '.' => return None,
            Some('.') => Context::Member,
            Some('$') => Context::Variable,
            Some('@') => Context::Own,
            Some(':') if start >= 2 && chars[start - 2] == ':' => {
                let end = start - 2;
                let mut s = end;
                while s > 0 && word(chars[s - 1]) {
                    s -= 1;
                }
                Context::Path(chars[s..end].iter().collect())
            }
            _ => Context::Word,
        };
        // A `$` alone isn't a variable yet: it gets a name, so the file can be read
        if matches!(context, Context::Variable) && start == cursor {
            let byte = text.char_indices().nth(cursor).map(|(b, _)| b).unwrap_or(text.len());
            text.insert_str(byte, "lksCompleting");
        }
        let mut engine = Engine::with_output(CapturedOutput::new());
        engine.set_stack_budget(STACK_SIZE - 16 * 1024 * 1024);
        let _ = self.run_check(&mut engine, &path, &text, Some(cursor));
        let found = engine.completion().unwrap_or_default();
        let original = self.text_of(&path)?;
        let range = |from: usize| Range { start: position(&original, from), end: position(&original, cursor) };
        let item = |i: &lakescript::CompletionItem, from: usize| CompletionItem {
            label: i.label.clone(),
            kind: Some(item_kind(i.kind)),
            detail: Some(i.detail.clone()),
            filter_text: Some(i.label.trim_start_matches('$').to_string()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit { range: range(from), new_text: i.label.clone() })),
            ..Default::default()
        };
        let items: Vec<CompletionItem> = match context {
            Context::Member => found.members.unwrap_or_default().iter().map(|i| item(i, start)).collect(),
            Context::Own => found.own.iter().map(|i| item(i, start)).collect(),
            Context::Path(name) => found.paths.get(&name).map(|items| items.iter().map(|i| item(i, start)).collect()).unwrap_or_default(),
            // The `$` is replaced too
            Context::Variable => found.scope.iter().filter(|i| i.label.starts_with('$')).map(|i| item(i, start - 1)).collect(),
            Context::Word => {
                let mut items: Vec<CompletionItem> = found.scope.iter().map(|i| item(i, start)).collect();
                for keyword in KEYWORDS {
                    items.push(CompletionItem {
                        label: keyword.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        text_edit: Some(CompletionTextEdit::Edit(TextEdit { range: range(start), new_text: keyword.to_string() })),
                        ..Default::default()
                    });
                }
                items
            }
        };
        Some(CompletionResponse::Array(items))
    }

    fn definition(&self, at: &TextDocumentPositionParams) -> Option<GotoDefinitionResponse> {
        let (name, _) = self.name_at(at)?;
        let d = name.definition.as_ref()?;
        let file = PathBuf::from(d.file.as_ref()?);
        let text = self.text_of(&file)?;
        let range = Range { start: position(&text, d.start), end: position(&text, d.end) };
        Some(GotoDefinitionResponse::Scalar(Location { uri: path_to_uri(&file)?, range }))
    }
}

/// A module that isn't found because its dependency isn't downloaded yet:
/// `lks-lsp` never downloads, so the error says what does.
fn missing_dependency(mut error: Diagnostic, project: &ferry::project::Project) -> Diagnostic {
    let Some(module) = error.message.strip_prefix("module `").and_then(|rest| rest.split_once("` not found")).map(|(m, _)| m.to_string()) else {
        return error;
    };
    if let Some((package, version)) = project.dependencies.iter().find(|(p, _)| module.starts_with(&format!("{}/", p.id()))) {
        error.message = format!("`{module}` comes from the dependency {package} {version}, which isn't downloaded yet: `ferry check` or `ferry run` downloads it");
    }
    error
}

/// What's before the cursor, for completion.
enum Context {
    /// After `.`
    Member,
    /// After `$`
    Variable,
    /// After `@`
    Own,
    /// After `name::`
    Path(String),
    Word,
}

/// The reserved words, and the names of the built-in types.
const KEYWORDS: &[&str] = &[
    "let", "const", "if", "else", "for", "in", "while", "switch", "case", "default", "break", "continue", "return", "yield", "try", "catch", "finally", "throw", "assert",
    "true", "false", "null", "use", "pub", "class", "interface", "enum", "command", "static", "new", "override", "implement", "extends", "any", "void", "never", "bool",
    "number", "string", "symbol", "regex", "range", "list", "map", "result",
];

fn item_kind(kind: lakescript::CompletionKind) -> CompletionItemKind {
    use lakescript::CompletionKind as K;
    match kind {
        K::Variable => CompletionItemKind::VARIABLE,
        K::Constant => CompletionItemKind::CONSTANT,
        K::Function => CompletionItemKind::FUNCTION,
        K::Method => CompletionItemKind::METHOD,
        K::Field => CompletionItemKind::FIELD,
        K::Class => CompletionItemKind::CLASS,
        K::Interface => CompletionItemKind::INTERFACE,
        K::Enum => CompletionItemKind::ENUM,
        K::EnumValue => CompletionItemKind::ENUM_MEMBER,
        K::Module => CompletionItemKind::MODULE,
    }
}

/// An error of the checker as the editor shows it.
fn to_lsp(error: &Diagnostic, text: Option<&str>) -> lsp_types::Diagnostic {
    let span = error.span;
    let range = match text {
        Some(text) if span.line > 0 => Range { start: position(text, span.start), end: position(text, span.end.max(span.start)) },
        _ => Range::default(),
    };
    lsp_types::Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("lakescript".into()),
        message: error.message.clone(),
        ..Default::default()
    }
}

/// The line and column of a character in the text. Lakescript counts
/// characters, and the protocol counts the UTF-16 units of a line.
fn position(text: &str, offset: usize) -> Position {
    let (mut line, mut column) = (0u32, 0u32);
    for c in text.chars().take(offset) {
        if c == '\n' {
            line += 1;
            column = 0;
        } else {
            column += c.len_utf16() as u32;
        }
    }
    Position { line, character: column }
}

/// The character at a line and column of the protocol, the opposite of `position`.
fn offset(text: &str, at: Position) -> usize {
    let (mut line, mut column, mut index) = (0u32, 0u32, 0usize);
    for c in text.chars() {
        if line == at.line && (column >= at.character || c == '\n') || line > at.line {
            break;
        }
        if c == '\n' {
            line += 1;
            column = 0;
        } else {
            column += c.len_utf16() as u32;
        }
        index += 1;
    }
    index
}

/// The path of a `file://` URI, as the files are known here *(see
/// `lakescript::canonical`)*.
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let path = PathBuf::from(uri_path(uri.as_str(), cfg!(windows))?);
    Some(lakescript::canonical(&path))
}

/// The path written in a `file://` URI, on Windows or not.
fn uri_path(uri: &str, windows: bool) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let path = percent_decode(path);
    if !windows {
        return (host.is_empty() || host == "localhost").then_some(path);
    }
    let path = path.replace('/', "\\");
    if !host.is_empty() && host != "localhost" {
        // `file://server/share/x` is a shared folder, `\\server\share\x`
        return Some(format!(r"\\{}{path}", percent_decode(host)));
    }
    // `file:///C:/x` is `C:\x`
    Some(path.strip_prefix('\\').unwrap_or(&path).to_string())
}

/// The `file://` URI of a path.
fn path_to_uri(path: &Path) -> Option<Uri> {
    Uri::from_str(&uri_of(&path.to_string_lossy(), cfg!(windows))).ok()
}

/// The `file://` URI of a path, on Windows or not. Windows' paths go without
/// their verbatim prefix *(`\\?\C:\x` is `file:///C:/x`)*, which no editor
/// reads back.
fn uri_of(path: &str, windows: bool) -> String {
    let mut host = String::new();
    let mut path = path.to_string();
    if windows {
        if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            path = format!(r"\\{unc}");
        } else if let Some(local) = path.strip_prefix(r"\\?\") {
            path = local.to_string();
        }
        path = path.replace('\\', "/");
    }
    // `//server/share/x` is `file://server/share/x`
    if windows && let Some(shared) = path.strip_prefix("//") {
        let at = shared.find('/').unwrap_or(shared.len());
        (host, path) = (shared[..at].to_string(), shared[at..].to_string());
    }
    // `C:/x` is `file:///C:/x`
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    format!("file://{}{}", encode(&host), encode(&path))
}

/// Writes the characters a URI can't have as `%` and two hex digits.
fn encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => encoded.push(byte as char),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // The two hex digits are read from the bytes, since a `%` can be before any character
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        if let (b'%', Some(byte)) = (bytes[i], hex) {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16_units() {
        let text = "ab\nñé𝄞x";
        assert_eq!(position(text, 0), Position { line: 0, character: 0 });
        assert_eq!(position(text, 3), Position { line: 1, character: 0 });
        // `𝄞` is two UTF-16 units
        assert_eq!(position(text, 6), Position { line: 1, character: 4 });
        for i in 0..=text.chars().count() {
            assert_eq!(offset(text, position(text, i)), i);
        }
    }

    #[test]
    fn uris_and_paths() {
        assert_eq!(percent_decode("/a%20b/c%C3%B1.lks"), "/a b/cñ.lks");
        assert_eq!(percent_decode("/50%ñ"), "/50%ñ");
        let uri = path_to_uri(Path::new("/tmp/a b/x.lks")).unwrap();
        assert_eq!(uri.as_str(), "file:///tmp/a%20b/x.lks");
        assert_eq!(uri_path("file:///tmp/a%20b/x.lks", false).as_deref(), Some("/tmp/a b/x.lks"));
        assert_eq!(uri_path("file://localhost/tmp/x.lks", false).as_deref(), Some("/tmp/x.lks"));
        assert_eq!(uri_path("file://server/x.lks", false), None);
    }

    /// Windows' paths, which are only seen on Windows, so they're checked as text.
    #[test]
    fn windows_uris_and_paths() {
        // What `canonicalize` gives there: editors can't read `file:////%3F/C:/…`
        assert_eq!(uri_of(r"\\?\C:\Users\ana\day1.lks", true), "file:///C:/Users/ana/day1.lks");
        assert_eq!(uri_of(r"C:\Users\ana\a b.lks", true), "file:///C:/Users/ana/a%20b.lks");
        assert_eq!(uri_of(r"\\?\UNC\server\share\x.lks", true), "file://server/share/x.lks");
        assert_eq!(uri_of(r"\\server\share\x.lks", true), "file://server/share/x.lks");
        assert_eq!(uri_path("file:///C:/Users/ana/a%20b.lks", true).as_deref(), Some(r"C:\Users\ana\a b.lks"));
        // VS Code writes the drive in lowercase, with its `:` escaped
        assert_eq!(uri_path("file:///c%3A/Users/ana/day1.lks", true).as_deref(), Some(r"c:\Users\ana\day1.lks"));
        assert_eq!(uri_path("file://server/share/x.lks", true).as_deref(), Some(r"\\server\share\x.lks"));
        for path in [r"C:\Users\ana\a b.lks", r"\\server\share\x.lks"] {
            assert_eq!(uri_path(&uri_of(path, true), true).as_deref(), Some(path));
        }
    }
}
