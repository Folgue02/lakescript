//! `lks-lsp`, the Lakescript language server *(see `ideas/tooling/lsp.md`)*.
//!
//! Editors start it and talk to it on stdin and stdout. It checks the files
//! the editor has open with the same checker as `lks`, using what the editor
//! has, saved or not, and sends back the errors.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

mod imports;

use lakescript::{CapturedOutput, Diagnostic, Engine, Name};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _, PublishDiagnostics};
use lsp_types::request::{Completion, GotoDefinition, HoverRequest, Request as _, SignatureHelpRequest};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse, CompletionTextEdit, DiagnosticSeverity, TextEdit, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverContents, HoverParams, HoverProviderCapability, Location, MarkupContent, MarkupKind, OneOf, Position, PublishDiagnosticsParams, Range,
    ParameterInformation, ParameterLabel, ServerCapabilities, SignatureHelp, SignatureHelpOptions, SignatureHelpParams, SignatureInformation, TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
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
        completion_provider: Some(CompletionOptions { trigger_characters: Some(vec![".".into(), "$".into(), "@".into(), ":".into(), "/".into()]), ..Default::default() }),
        signature_help_provider: Some(SignatureHelpOptions { trigger_characters: Some(vec!["(".into(), ",".into()]), ..Default::default() }),
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
    /// The `pub` types of the modules that can be reached, for importing them from completion.
    index: imports::Index,
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
            // Comments that don't match the code: warnings, which never stop a script
            if let Some(text) = self.documents.get(path)
                && let Ok(warnings) = lakescript::docs::stale_params(text)
            {
                let list = found.entry(path.clone()).or_default();
                for w in warnings {
                    list.push(lsp_types::Diagnostic {
                        range: Range { start: position(text, w.start), end: position(text, w.end) },
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("lakescript".into()),
                        message: w.message,
                        ..Default::default()
                    });
                }
            }
            let (errors, warnings, names) = self.check(path);
            if let Some(text) = self.documents.get(path) {
                let list = found.entry(path.clone()).or_default();
                for w in &warnings {
                    let mut diagnostic = to_lsp(w, Some(text));
                    diagnostic.severity = Some(DiagnosticSeverity::WARNING);
                    if !list.contains(&diagnostic) {
                        list.push(diagnostic);
                    }
                }
            }
            // A file that doesn't parse has no names: the last ones stay, for hover while it's being written
            if !names.is_empty() || errors.is_empty() {
                self.names.insert(path.clone(), names);
            }
            for error in errors {
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
    fn check(&self, path: &Path) -> (Vec<Diagnostic>, Vec<Diagnostic>, Vec<Name>) {
        let mut engine = Engine::with_output(CapturedOutput::new());
        engine.set_stack_budget(STACK_SIZE - 16 * 1024 * 1024);
        engine.set_names(true);
        let text = self.documents.get(path).cloned().unwrap_or_default();
        let (result, as_module) = self.run_check(&mut engine, path, &text, None);
        let shown = path.display().to_string();
        // The file's own warnings: a module's, when it was checked by using it
        let warnings: Vec<Diagnostic> = engine
            .warnings()
            .iter()
            .filter(|w| match &w.file {
                None => !as_module,
                Some(file) => lakescript::canonical(Path::new(file)) == lakescript::canonical(path),
            })
            .cloned()
            .collect();
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
        (result, warnings, names)
    }

    /// Every error of a check that failed: the checker goes on after one
    /// *(see Every error at once in ideas/tooling/lsp.md)*.
    fn all_errors(engine: &Engine, first: Diagnostic) -> Vec<Diagnostic> {
        if engine.errors().is_empty() { vec![first] } else { engine.errors().to_vec() }
    }

    /// Checks one file like `lks` or `ferry` would: a module of a ferry
    /// project with the project's module path, any other file as a script.
    /// `text` is the file's, and `complete` where completion is wanted in it.
    /// Returns whether it was checked as a module.
    /// Gives every error, none when it passes.
    fn run_check(&self, engine: &mut Engine, path: &Path, text: &str, complete: Option<usize>) -> (Vec<Diagnostic>, bool) {
        for (open, text) in &self.documents {
            engine.set_file_text(open, text.clone());
        }
        engine.set_file_text(path, text);
        let folder = path.parent().unwrap_or(Path::new("."));
        let project = match ferry::project::Project::find(folder) {
            Ok(project) => project,
            // A `ferry.toml` that can't be read: its error, instead of checking the file as if there were no project
            Err(e) if folder.ancestors().any(|d| d.join(ferry::project::MANIFEST).is_file()) => {
                return (vec![Diagnostic::new(e, Default::default())], false);
            }
            Err(_) => {
                if let Some(offset) = complete {
                    engine.set_completion(None, offset);
                }
                let errors = match engine.check_text(path, text) {
                    Ok(()) => Vec::new(),
                    Err(first) => Self::all_errors(engine, first),
                };
                return (errors, false);
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
            let errors = match engine.check_source(&format!("use {} as _module;\n", module.join("/"))) {
                Ok(()) => Vec::new(),
                Err(first) => Self::all_errors(engine, first),
            };
            let errors = errors
                .into_iter()
                .map(|mut e| {
                    // An error of the `use` line itself is the module's
                    if e.file.is_none() {
                        e.file = Some(path.display().to_string());
                        e.span = Default::default();
                    }
                    missing_dependency(e, &project)
                })
                .collect();
            return (errors, true);
        }
        if let Some(offset) = complete {
            engine.set_completion(None, offset);
        }
        let errors = match engine.check_text(path, text) {
            Ok(()) => Vec::new(),
            Err(first) => Self::all_errors(engine, first),
        };
        (errors.into_iter().map(|e| missing_dependency(e, &project)).collect(), false)
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
            SignatureHelpRequest::METHOD => match request.extract::<SignatureHelpParams>(SignatureHelpRequest::METHOD) {
                Ok((_, params)) => serde_json::to_value(self.signature_help(&params.text_document_position_params)),
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
        let Some((name, text)) = self.name_at(at) else { return self.hover_folder(at) };
        let mut value = format!("```lakescript\n{}\n```", name.code);
        // A module of the project: its `//!` text
        let module_doc = (name.code.starts_with("module ") && name.doc.is_none())
            .then(|| name.definition.as_ref()?.file.as_ref().and_then(|f| self.text_of(Path::new(f))))
            .flatten()
            .and_then(|source| lakescript::docs::module_text(&source));
        for part in [&name.about, &name.doc, &module_doc].into_iter().flatten() {
            value.push_str("\n\n");
            value.push_str(part);
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value }),
            range: Some(Range { start: position(&text, name.start), end: position(&text, name.end) }),
        })
    }

    /// The parameters of the call the cursor is in, with the one being
    /// written and the `$name:` lines of the function's comment.
    fn signature_help(&self, at: &TextDocumentPositionParams) -> Option<SignatureHelp> {
        let path = uri_to_path(&at.text_document.uri)?;
        let text = self.text_of(&path)?;
        let cursor = offset(&text, at.position);
        let chars: Vec<char> = text.chars().collect();
        let call = open_call(&chars, cursor)?;
        // The function: the name that ends right before the `(`
        let names = self.names.get(&path)?;
        let function = names.iter().filter(|n| n.end == call.paren && n.code.ends_with(')')).min_by_key(|n| n.end - n.start)?;
        let (params, label) = signature_params(&function.code)?;
        let doc = function.doc.as_deref().map(lakescript::docs::parse_doc).unwrap_or_default();
        let parameters: Vec<ParameterInformation> = params
            .iter()
            .map(|p| ParameterInformation {
                label: ParameterLabel::LabelOffsets([p.start, p.end]),
                documentation: doc
                    .params
                    .iter()
                    .find(|(n, _)| *n == p.name)
                    .map(|(_, what)| lsp_types::Documentation::MarkupContent(MarkupContent { kind: MarkupKind::Markdown, value: what.clone() })),
            })
            .collect();
        let active = match &call.named {
            Some(name) => params.iter().position(|p| p.name == *name)?,
            None if params.is_empty() => 0,
            // Arguments after the last parameter go to it when it takes many
            None => call.index.min(params.len() - 1),
        };
        let documentation = (!doc.text.is_empty()).then(|| lsp_types::Documentation::MarkupContent(MarkupContent { kind: MarkupKind::Markdown, value: doc.text.clone() }));
        let signature = SignatureInformation { label, documentation, parameters: Some(parameters), active_parameter: Some(active as u32) };
        Some(SignatureHelp { signatures: vec![signature], active_signature: Some(0), active_parameter: Some(active as u32) })
    }

    /// The named arguments a call can still take, when the cursor is where an
    /// argument starts *(i.e. `parameter: ` in `f(par|)`)*: the parameters of
    /// the function before its `(`, without the ones the arguments before it
    /// already give, by position or by name.
    fn named_arguments(&self, path: &Path, chars: &[char], start: usize, cursor: usize, range: &dyn Fn(usize) -> Range) -> Vec<CompletionItem> {
        let Some(call) = open_call(chars, cursor) else { return Vec::new() };
        let Some(names) = self.names.get(path) else { return Vec::new() };
        // The name right before the `(`, or, when the line can't be read while it's written
        // *(i.e. a name started after a named argument)*, a function of that name the file knows
        let word: String = {
            let mut s = call.paren;
            while s > 0 && (chars[s - 1].is_alphanumeric() || chars[s - 1] == '_') {
                s -= 1;
            }
            chars[s..call.paren].iter().collect()
        };
        let declared = |code: &str| code.split('(').next().and_then(|head| head.rsplit([' ', ':']).next()).is_some_and(|n| n == word);
        let at = names.iter().filter(|n| n.end == call.paren && n.code.ends_with(')')).min_by_key(|n| n.end - n.start);
        let Some(function) = at.or_else(|| names.iter().find(|n| !word.is_empty() && n.code.ends_with(')') && declared(&n.code))) else { return Vec::new() };
        let Some((params, label)) = signature_params(&function.code) else { return Vec::new() };
        let arguments = arguments_before(chars, call.paren, start);
        // Only where an argument starts: not in the middle of one, or after `name:`
        let Some(current) = arguments.last() else { return Vec::new() };
        if !current.trim().is_empty() {
            return Vec::new();
        }
        let given = &arguments[..arguments.len() - 1];
        let positional = given.iter().take_while(|a| argument_name(a).is_none()).count();
        let named: Vec<String> = given.iter().filter_map(|a| argument_name(a)).collect();
        let label: Vec<char> = label.chars().collect();
        let utf16_slice = |from: u32, to: u32| -> String {
            let mut at = 0;
            label.iter().filter(|c| {
                let here = at;
                at += c.len_utf16() as u32;
                here >= from && here < to
            }).collect()
        };
        params
            .iter()
            .enumerate()
            .filter(|(i, p)| *i >= positional && !named.contains(&p.name))
            .map(|(i, p)| CompletionItem {
                label: format!("{}:", p.name),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(utf16_slice(p.start, p.end)),
                sort_text: Some(format!("!{i:03}")),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range: range(start), new_text: format!("{}: ", p.name) })),
                ..Default::default()
            })
            .collect()
    }

    /// A folder in a `use` path *(i.e. `api` in `use folgue02/web/api/users;`)*:
    /// the first paragraph of its `module.md` *(see Documenting folders in
    /// ideas/language/modules.md)*.
    fn hover_folder(&self, at: &TextDocumentPositionParams) -> Option<Hover> {
        let path = uri_to_path(&at.text_document.uri)?;
        let text = self.text_of(&path)?;
        let chars: Vec<char> = text.chars().collect();
        let cursor = offset(&text, at.position);
        let line_start = chars[..cursor.min(chars.len())].iter().rposition(|c| *c == '\n').map(|i| i + 1).unwrap_or(0);
        let line_end = chars[cursor.min(chars.len())..].iter().position(|c| *c == '\n' || *c == ';').map(|i| cursor + i).unwrap_or(chars.len());
        let line: String = chars[line_start..line_end].iter().collect();
        let rest = line.trim_start().strip_prefix("use ")?;
        let path_start = line_start + (line.len() - line.trim_start().len()) + "use ".len() + (rest.len() - rest.trim_start().len());
        let written: String = chars[path_start..line_end].iter().take_while(|c| is_path_char(**c)).collect();
        // The parts up to the one under the cursor, which must be a folder: the last part is the module
        let at_part = chars[path_start..cursor.min(chars.len())].iter().filter(|c| **c == '/').count();
        let parts: Vec<&str> = written.split('/').collect();
        if at_part + 1 >= parts.len() {
            return None;
        }
        let folder = parts[..=at_part].join("/");
        let (from, to) = {
            let before: usize = parts[..at_part].iter().map(|p| p.chars().count() + 1).sum();
            (path_start + before, path_start + before + parts[at_part].chars().count())
        };
        let mut value = format!("```lakescript\nfolder {folder}\n```");
        let about = if parts[0] == "std" {
            // A folder of the standard library: what's in it
            let prefix = format!("{folder}/");
            let inside: Vec<String> = lakescript::std_modules().iter().filter(|m| m.starts_with(&prefix)).map(|m| format!("`{m}`")).collect();
            if inside.is_empty() {
                return None;
            }
            let what = if folder == "std" { "The standard library" } else { "A folder of the standard library" };
            Some(format!("{what}: {}.", inside.join(", ")))
        } else {
            let dir = self.module_roots(&path).into_iter().map(|root| root.join(&folder)).find(|d| d.is_dir())?;
            std::fs::read_to_string(dir.join("module.md")).ok().and_then(|t| first_paragraph(&t))
        };
        if let Some(about) = about {
            value.push_str("\n\n");
            value.push_str(&about);
        }
        Some(Hover { contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value }), range: Some(Range { start: position(&text, from), end: position(&text, to) }) })
    }

    /// The folders modules are looked for in, like `run_check` uses them: a
    /// project's own and its dependencies', or the folder of a script.
    fn module_roots(&self, path: &Path) -> Vec<PathBuf> {
        let folder = path.parent().unwrap_or(Path::new("."));
        match ferry::project::Project::find(folder) {
            Ok(project) => match ferry::home::Home::open() {
                Ok(home) => ferry::resolve::installed_module_path(&home, &project),
                Err(_) => vec![project.src()],
            },
            Err(_) => vec![folder.to_path_buf()],
        }
    }

    /// The folders and modules that can go next in a `use` path, with the
    /// first line of a folder's `module.md` and of a module's `//!`.
    fn complete_use(&self, path: &Path, written: &str, text: &str, cursor: usize) -> Vec<CompletionItem> {
        let (dirs, partial) = match written.rsplit_once('/') {
            Some((dirs, partial)) => (dirs, partial),
            None => ("", written),
        };
        let from = cursor - partial.chars().count();
        let range = Range { start: position(text, from), end: position(text, cursor) };
        let mut found: Vec<(String, CompletionItemKind, Option<String>)> = Vec::new();
        let mut add = |label: String, kind: CompletionItemKind, detail: Option<String>| {
            if !found.iter().any(|(l, _, _)| *l == label) {
                found.push((label, kind, detail));
            }
        };
        if dirs.is_empty() {
            add("std".into(), CompletionItemKind::FOLDER, Some("The standard library".into()));
        }
        if dirs == "std" || dirs.starts_with("std/") {
            let prefix = format!("{dirs}/");
            for module in lakescript::std_modules() {
                let Some(rest) = module.strip_prefix(&prefix) else { continue };
                match rest.split_once('/') {
                    Some((folder, _)) => add(folder.to_string(), CompletionItemKind::FOLDER, None),
                    None => add(rest.to_string(), CompletionItemKind::MODULE, Some(format!("module {module}"))),
                }
            }
        } else {
            for root in self.module_roots(path) {
                let Ok(entries) = std::fs::read_dir(root.join(dirs)) else { continue };
                let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
                entries.sort();
                for entry in entries {
                    let Some(name) = entry.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
                    if name.starts_with('.') {
                        continue;
                    }
                    if entry.is_dir() {
                        let about = std::fs::read_to_string(entry.join("module.md")).ok().and_then(|t| first_line(&t));
                        add(name, CompletionItemKind::FOLDER, about);
                    } else if let Some(module) = name.strip_suffix(".lks") {
                        let about = std::fs::read_to_string(&entry).ok().and_then(|t| lakescript::docs::module_text(&t)).and_then(|t| first_line(&t));
                        add(module.to_string(), CompletionItemKind::MODULE, about);
                    }
                }
            }
        }
        found
            .into_iter()
            .map(|(label, kind, detail)| CompletionItem {
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text: label.clone() })),
                label,
                kind: Some(kind),
                detail,
                ..Default::default()
            })
            .collect()
    }

    /// What can be written at the cursor, from what's before it: `.`, `$`, `@`, `name::` or a word.
    fn complete(&self, at: &TextDocumentPositionParams) -> Option<CompletionResponse> {
        let path = uri_to_path(&at.text_document.uri)?;
        let mut text = self.text_of(&path)?;
        let cursor = offset(&text, at.position);
        let chars: Vec<char> = text.chars().collect();
        // In the path of a `use` line: folders and modules
        let line_start = chars[..cursor].iter().rposition(|c| *c == '\n').map(|i| i + 1).unwrap_or(0);
        let line: String = chars[line_start..cursor].iter().collect();
        if let Some(written) = line.trim_start().strip_prefix("use ").map(str::trim_start)
            && written.chars().all(is_path_char)
        {
            return Some(CompletionResponse::Array(self.complete_use(&path, written, &text, cursor)));
        }
        // `/` asks for completion only for `use` paths: after a division there's nothing to offer
        if cursor > 0 && chars[cursor - 1] == '/' {
            return Some(CompletionResponse::Array(Vec::new()));
        }
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
                // At the start of an argument: the names of the parameters not given yet, first
                let mut items: Vec<CompletionItem> = self.named_arguments(&path, &chars, start, cursor, &range);
                items.extend(found.scope.iter().map(|i| item(i, start)));
                // Types that aren't imported yet, with the `use` line that imports them
                let typed: String = chars[start..cursor].iter().collect();
                if typed.starts_with(|c: char| c.is_ascii_uppercase()) {
                    let in_scope: HashSet<&str> = found.scope.iter().map(|i| i.label.as_str()).collect();
                    let roots = self.module_roots(&path);
                    // The folder of the file's module: its types need no `use`. Nor, for a
                    // lowercase module, the ones in the folder named like it *(`config.lks` and `config/`)*
                    let relative = roots.iter().find_map(|root| path.strip_prefix(root).ok());
                    let joined = |p: &Path| p.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
                    let folder: Option<String> = relative.map(|r| joined(r.parent().unwrap_or(Path::new(""))));
                    let named: Option<String> = relative
                        .filter(|r| r.file_stem().is_some_and(|s| !s.to_string_lossy().starts_with(|c: char| c.is_ascii_uppercase())))
                        .map(|r| joined(&r.with_extension("")));
                    for t in self.index.types(&roots, &self.documents, &path) {
                        if in_scope.contains(t.name.as_str()) {
                            continue;
                        }
                        let module_folder = t.module.rsplit_once('/').map_or("", |(folder, _)| folder);
                        let same_folder = t.type_module && (folder.as_deref() == Some(module_folder) || named.as_deref() == Some(module_folder));
                        let edits = if same_folder {
                            None
                        } else {
                            let Some(edit) = imports::use_edit(&original, &t) else { continue };
                            let range = Range { start: position(&original, edit.start), end: position(&original, edit.end) };
                            Some(vec![TextEdit { range, new_text: edit.text }])
                        };
                        let kind = match t.kind {
                            lakescript::docs::ItemKind::Interface => CompletionItemKind::INTERFACE,
                            lakescript::docs::ItemKind::Enum => CompletionItemKind::ENUM,
                            _ => CompletionItemKind::CLASS,
                        };
                        items.push(CompletionItem {
                            label: t.name.clone(),
                            kind: Some(kind),
                            detail: Some(format!("{} {} from {}", t.kind.word(), t.name, t.module)),
                            label_details: Some(lsp_types::CompletionItemLabelDetails { detail: None, description: Some(t.module.clone()) }),
                            // After the names already in scope
                            sort_text: Some(format!("~{}", t.name)),
                            text_edit: Some(CompletionTextEdit::Edit(TextEdit { range: range(start), new_text: t.name.clone() })),
                            additional_text_edits: edits,
                            ..Default::default()
                        });
                    }
                }
                // After `catch (`, the exceptions, also when the line can't be read yet
                let before: String = chars[..start].iter().collect();
                let before = before.trim_end();
                if before.ends_with("catch (") || before.ends_with("catch(") || (before.ends_with('|') && before.contains("catch")) {
                    let in_scope: HashSet<String> = items.iter().map(|i| i.label.clone()).collect();
                    for name in lakescript::exception_names().into_iter().filter(|n| !in_scope.contains(*n)) {
                        items.push(CompletionItem {
                            label: name.to_string(),
                            kind: Some(CompletionItemKind::CLASS),
                            detail: Some(format!("exception {name}")),
                            text_edit: Some(CompletionTextEdit::Edit(TextEdit { range: range(start), new_text: name.to_string() })),
                            ..Default::default()
                        });
                    }
                }
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
    "let", "const", "if", "else", "for", "in", "while", "switch", "case", "default", "break", "continue", "return", "yield", "try", "catch", "finally", "throw", "assert", "unimplemented",
    "true", "false", "null", "use", "pub", "class", "interface", "enum", "command", "static", "new", "override", "implement", "implements", "extends", "any", "void", "never", "bool",
    "number", "string", "bytes", "regex", "range", "list", "map", "result",
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

/// A call the cursor is inside of: where its `(` is, which argument the
/// cursor is in, from 0, and the name of a named argument *(i.e. `times` in
/// `greet('Ana', times: 2)`)*.
struct OpenCall {
    paren: usize,
    index: usize,
    named: Option<String>,
}

/// Finds the call the cursor is in, reading the text up to it: strings and
/// comments are skipped, and so are the brackets closed before the cursor.
fn open_call(chars: &[char], cursor: usize) -> Option<OpenCall> {
    // Each open bracket, with its commas and where its current argument starts
    let mut open: Vec<(char, usize, usize, usize)> = Vec::new();
    let mut i = 0;
    let end = cursor.min(chars.len());
    while i < end {
        let c = chars[i];
        match c {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < end && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '\'' | '"' => {
                i += 1;
                while i < end && chars[i] != c {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            '(' | '[' | '{' => open.push((c, i, 0, i + 1)),
            ')' | ']' | '}' => {
                open.pop();
            }
            ',' => {
                if let Some(top) = open.last_mut() {
                    top.2 += 1;
                    top.3 = i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let &(bracket, paren, index, from) = open.last()?;
    if bracket != '(' {
        return None;
    }
    // `name:` at the start of the argument, but not `name::`
    let arg: String = chars[from..end].iter().collect();
    let arg = arg.trim_start();
    let word: String = arg.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    let after = &arg[word.len()..];
    let named = (!word.is_empty() && after.trim_start().starts_with(':') && !after.trim_start().starts_with("::")).then_some(word);
    Some(OpenCall { paren, index, named })
}

/// The arguments of the call whose `(` is at `paren`, up to `end`, split
/// at its commas: the last one is the one being written. Strings and
/// brackets inside an argument are kept whole.
fn arguments_before(chars: &[char], paren: usize, end: usize) -> Vec<String> {
    let mut arguments = vec![String::new()];
    let mut depth = 0;
    let mut i = paren + 1;
    while i < end {
        let c = chars[i];
        match c {
            '\'' | '"' => {
                let quote = c;
                arguments.last_mut().unwrap().push(c);
                i += 1;
                while i < end && chars[i] != quote {
                    if chars[i] == '\\' {
                        arguments.last_mut().unwrap().push(chars[i]);
                        i += 1;
                    }
                    if i < end {
                        arguments.last_mut().unwrap().push(chars[i]);
                    }
                    i += 1;
                }
                if i < end {
                    arguments.last_mut().unwrap().push(chars[i]);
                }
                i += 1;
                continue;
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                arguments.push(String::new());
                i += 1;
                continue;
            }
            _ => {}
        }
        arguments.last_mut().unwrap().push(c);
        i += 1;
    }
    arguments
}

/// The name of a named argument *(i.e. `all` in `all: true`)*, not of `a::b`.
fn argument_name(argument: &str) -> Option<String> {
    let text = argument.trim_start();
    let word: String = text.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    let after = text[word.len()..].trim_start();
    (!word.is_empty() && after.starts_with(':') && !after.starts_with("::")).then_some(word)
}

/// A parameter in the label of a signature: its name, and where it is in
/// the label, in the UTF-16 units the protocol counts.
struct SignatureParam {
    name: String,
    start: u32,
    end: u32,
}

/// The parameters of a function's declaration *(i.e. `string greet(string
/// $name, number $times = …)`)*, and the label to show.
fn signature_params(code: &str) -> Option<(Vec<SignatureParam>, String)> {
    let chars: Vec<char> = code.chars().collect();
    // The `(` that matches the last `)`: parameters can have function types
    let mut depth = 0;
    let mut open = None;
    for (i, c) in chars.iter().enumerate().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    open = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let open = open?;
    let close = chars.len() - 1;
    let utf16 = |i: usize| chars[..i].iter().map(|c| c.len_utf16() as u32).sum::<u32>();
    let mut params = Vec::new();
    let mut start = open + 1;
    let mut depth = 0;
    for i in open + 1..=close {
        let c = chars[i];
        match c {
            '(' | '<' | '[' => depth += 1,
            // Not the `>` of `->` in a function type
            '>' if chars[i - 1] == '-' => {}
            ')' | '>' | ']' if i < close => depth -= 1,
            _ => {}
        }
        if (c == ',' && depth == 0) || i == close {
            let text: String = chars[start..i].iter().collect();
            let lead = text.len() - text.trim_start().len();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let name = trimmed
                    .split(['$', '@'])
                    .nth(1)
                    .map(|rest| rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect())
                    .unwrap_or_default();
                let from = start + lead;
                params.push(SignatureParam { name, start: utf16(from), end: utf16(from + trimmed.chars().count()) });
            }
            start = i + 1;
        }
    }
    Some((params, code.to_string()))
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '/' || c == '.' || c == '-'
}

/// The first line of a text, without Markdown's `#` of a title.
fn first_line(text: &str) -> Option<String> {
    let line = text.lines().map(|l| l.trim().trim_start_matches('#').trim()).find(|l| !l.is_empty())?;
    Some(line.to_string())
}

/// The first paragraph of a Markdown text, after its title.
fn first_paragraph(text: &str) -> Option<String> {
    let paragraph = text
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty() && !(p.starts_with('#') && !p.contains('\n')))?;
    Some(paragraph.to_string())
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
