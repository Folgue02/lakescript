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
use lsp_types::request::{GotoDefinition, HoverRequest, Request as _};
use lsp_types::{
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse,
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
        ..Default::default()
    };
    let capabilities = serde_json::to_value(capabilities).map_err(|e| e.to_string())?;
    connection.initialize(capabilities).map_err(|e| e.to_string())?;
    let mut server = Server::default();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request).map_err(|e| e.to_string())? {
                    break;
                }
                let _ = connection.sender.send(Message::Response(server.answer(request)));
            }
            Message::Notification(notification) => {
                if server.handle(notification) {
                    server.check_all(&connection);
                }
            }
            Message::Response(_) => {}
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
                let file = file.canonicalize().unwrap_or(file);
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

    /// Checks one file like `lks` or `ferry` would: a module of a ferry
    /// project with the project's module path, any other file as a script.
    /// Checks one file, and returns what was found about its names.
    fn check(&self, path: &Path) -> (Result<(), Diagnostic>, Vec<Name>) {
        let mut engine = Engine::with_output(CapturedOutput::new());
        engine.set_stack_budget(STACK_SIZE - 16 * 1024 * 1024);
        engine.set_names(true);
        let (result, as_module) = self.run_check(&mut engine, path);
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

    /// Checks one file like `lks` or `ferry` would. Returns whether it was checked as a module.
    fn run_check(&self, engine: &mut Engine, path: &Path) -> (Result<(), Diagnostic>, bool) {
        for (open, text) in &self.documents {
            engine.set_file_text(open, text.clone());
        }
        let text = self.documents.get(path).cloned().unwrap_or_default();
        let folder = path.parent().unwrap_or(Path::new("."));
        let Ok(project) = ferry::project::Project::find(folder) else {
            return (engine.check_text(path, &text), false);
        };
        let module_path = match ferry::home::Home::open() {
            Ok(home) => ferry::resolve::installed_module_path(&home, &project),
            Err(_) => vec![project.src()],
        };
        for root in &module_path {
            engine.add_module_root(root.clone());
        }
        // A module is checked like ferry does, by using it, so only declarations are allowed in it
        let src = project.src().canonicalize().unwrap_or_else(|_| project.src());
        if let Ok(relative) = path.strip_prefix(&src) {
            let module: Vec<String> = relative.with_extension("").iter().map(|p| p.to_string_lossy().into_owned()).collect();
            let result = engine.check_source(&format!("use {} as module;\n", module.join("/"))).map_err(|mut e| {
                // An error of the `use` line itself is the module's
                if e.file.is_none() {
                    e.file = Some(path.display().to_string());
                    e.span = Default::default();
                }
                e
            });
            return (result, true);
        }
        (engine.check_text(path, &text), false)
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

    fn definition(&self, at: &TextDocumentPositionParams) -> Option<GotoDefinitionResponse> {
        let (name, _) = self.name_at(at)?;
        let d = name.definition.as_ref()?;
        let file = PathBuf::from(d.file.as_ref()?);
        let text = self.text_of(&file)?;
        let range = Range { start: position(&text, d.start), end: position(&text, d.end) };
        Some(GotoDefinitionResponse::Scalar(Location { uri: path_to_uri(&file)?, range }))
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

/// The path of a `file://` URI.
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    // `file:///C:/x` on Windows is `C:/x`
    let path = if cfg!(windows) { decoded.strip_prefix('/').unwrap_or(&decoded).to_string() } else { decoded };
    let path = PathBuf::from(path);
    Some(path.canonicalize().unwrap_or(path))
}

fn path_to_uri(path: &Path) -> Option<Uri> {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = if text.starts_with('/') { text } else { format!("/{text}") };
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => encoded.push(byte as char),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    Uri::from_str(&format!("file://{encoded}")).ok()
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
    }
}
