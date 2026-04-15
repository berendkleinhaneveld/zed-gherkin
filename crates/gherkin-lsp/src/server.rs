use crate::indexer::{Index, StepCall, StepDef};
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, Initialized, Notification as LspNotification,
};
use lsp_types::request::{GotoDefinition, References, Request as LspRequest};
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, InitializeParams, Location, OneOf, Position, Range, ReferenceParams,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct State {
    pub index: Index,
    pub buffers: HashMap<Url, String>,
    pub roots: Vec<PathBuf>,
}

impl State {
    pub fn new() -> Self {
        Self {
            index: Index::default(),
            buffers: HashMap::new(),
            roots: Vec::new(),
        }
    }

    pub fn init(&mut self, params: &InitializeParams) {
        if let Some(folders) = &params.workspace_folders {
            for f in folders {
                if let Ok(p) = f.uri.to_file_path() {
                    self.roots.push(p);
                }
            }
        } else if let Some(uri) = {
            #[allow(deprecated)]
            { &params.root_uri }
        } {
            if let Ok(p) = uri.to_file_path() {
                self.roots.push(p);
            }
        }
        for root in self.roots.clone() {
            let idx = Index::build(&root);
            self.index.defs.extend(idx.defs);
            self.index.calls.extend(idx.calls);
        }
    }

    pub fn rescan_path(&mut self, path: &Path) {
        self.index.drop_file(path);
        if let Ok(content) = fs::read_to_string(path) {
            self.index.scan_file(path, &content);
        }
    }

    pub fn rescan_with(&mut self, path: &Path, content: &str) {
        self.index.drop_file(path);
        self.index.scan_file(path, content);
    }

    pub fn definition(&self, uri: &Url, line: u32) -> Vec<Location> {
        let Ok(path) = uri.to_file_path() else {
            return vec![];
        };
        let Some(call) = self
            .index
            .calls
            .iter()
            .find(|c| c.path == path && c.line == line)
        else {
            return vec![];
        };
        self.index
            .defs
            .iter()
            .filter(|d| d.regex.is_match(&call.text))
            .map(def_to_location)
            .collect()
    }

    pub fn references(&self, uri: &Url, line: u32) -> Vec<Location> {
        let Ok(path) = uri.to_file_path() else {
            return vec![];
        };
        if let Some(def) = self
            .index
            .defs
            .iter()
            .find(|d| d.path == path && d.line == line)
        {
            return self
                .index
                .calls
                .iter()
                .filter(|c| def.regex.is_match(&c.text))
                .map(call_to_location)
                .collect();
        }
        if let Some(call) = self
            .index
            .calls
            .iter()
            .find(|c| c.path == path && c.line == line)
        {
            let mut out = Vec::new();
            for def in &self.index.defs {
                if def.regex.is_match(&call.text) {
                    for other in &self.index.calls {
                        if def.regex.is_match(&other.text) {
                            out.push(call_to_location(other));
                        }
                    }
                }
            }
            return out;
        }
        Vec::new()
    }
}

fn def_to_location(d: &StepDef) -> Location {
    Location {
        uri: Url::from_file_path(&d.path).unwrap(),
        range: Range {
            start: Position {
                line: d.line,
                character: d.col_start,
            },
            end: Position {
                line: d.line,
                character: d.col_end,
            },
        },
    }
}

fn call_to_location(c: &StepCall) -> Location {
    Location {
        uri: Url::from_file_path(&c.path).unwrap(),
        range: Range {
            start: Position {
                line: c.line,
                character: c.col_start,
            },
            end: Position {
                line: c.line,
                character: c.col_end,
            },
        },
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    };
    let init_value = connection.initialize(serde_json::to_value(capabilities)?)?;
    let params: InitializeParams = serde_json::from_value(init_value)?;
    let mut state = State::new();
    state.init(&params);
    main_loop(&connection, &mut state)?;
    io_threads.join()?;
    Ok(())
}

fn main_loop(
    connection: &Connection,
    state: &mut State,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                handle_request(connection, state, req)?;
            }
            Message::Notification(not) => handle_notification(state, not),
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn handle_request(
    connection: &Connection,
    state: &State,
    req: Request,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    if req.method == GotoDefinition::METHOD {
        let (id, params): (RequestId, GotoDefinitionParams) =
            req.extract(GotoDefinition::METHOD)?;
        let pos = params.text_document_position_params;
        let locs = state.definition(&pos.text_document.uri, pos.position.line);
        let resp = if locs.is_empty() {
            Response {
                id,
                result: Some(serde_json::Value::Null),
                error: None,
            }
        } else {
            Response {
                id,
                result: Some(serde_json::to_value(GotoDefinitionResponse::Array(locs))?),
                error: None,
            }
        };
        connection.sender.send(Message::Response(resp))?;
        return Ok(());
    }
    if req.method == References::METHOD {
        let (id, params): (RequestId, ReferenceParams) = req.extract(References::METHOD)?;
        let pos = params.text_document_position;
        let locs = state.references(&pos.text_document.uri, pos.position.line);
        let resp = Response {
            id,
            result: Some(serde_json::to_value(locs)?),
            error: None,
        };
        connection.sender.send(Message::Response(resp))?;
        return Ok(());
    }
    let resp = Response {
        id: req.id,
        result: Some(serde_json::Value::Null),
        error: None,
    };
    connection.sender.send(Message::Response(resp))?;
    Ok(())
}

fn handle_notification(state: &mut State, not: Notification) {
    match not.method.as_str() {
        m if m == DidOpenTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(not.params) {
                if let Ok(path) = params.text_document.uri.to_file_path() {
                    state
                        .buffers
                        .insert(params.text_document.uri.clone(), params.text_document.text.clone());
                    state.rescan_with(&path, &params.text_document.text);
                }
            }
        }
        m if m == DidChangeTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(not.params) {
                if let Some(change) = params.content_changes.into_iter().next() {
                    if let Ok(path) = params.text_document.uri.to_file_path() {
                        state
                            .buffers
                            .insert(params.text_document.uri.clone(), change.text.clone());
                        state.rescan_with(&path, &change.text);
                    }
                }
            }
        }
        m if m == DidSaveTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidSaveTextDocumentParams>(not.params) {
                if let Ok(path) = params.text_document.uri.to_file_path() {
                    state.rescan_path(&path);
                }
            }
        }
        m if m == DidCloseTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(not.params) {
                state.buffers.remove(&params.text_document.uri);
            }
        }
        m if m == DidChangeWatchedFiles::METHOD => {
            if let Ok(params) = serde_json::from_value::<DidChangeWatchedFilesParams>(not.params) {
                for change in params.changes {
                    if let Ok(path) = change.uri.to_file_path() {
                        state.rescan_path(&path);
                    }
                }
            }
        }
        m if m == Initialized::METHOD => {}
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn url(p: &Path) -> Url {
        Url::from_file_path(p).unwrap()
    }

    fn setup() -> (tempfile::TempDir, State) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("steps.py"),
            r#"@given("I have {int} cukes")
def _(c, n): pass

@when("I eat {int}")
def _(c, n): pass
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("demo.feature"),
            "\
Feature: demo
  Scenario: one
    Given I have 5 cukes
    When I eat 3
    Given I have 10 cukes
",
        )
        .unwrap();
        let mut state = State::new();
        state.roots.push(dir.path().to_path_buf());
        let idx = Index::build(dir.path());
        state.index.defs.extend(idx.defs);
        state.index.calls.extend(idx.calls);
        (dir, state)
    }

    #[test]
    fn definition_from_feature_finds_python_def() {
        let (dir, state) = setup();
        let feature = dir.path().join("demo.feature");
        let locs = state.definition(&url(&feature), 2); // line index 2 = "Given I have 5 cukes"
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.path().ends_with("steps.py"));
    }

    #[test]
    fn definition_on_non_step_line_returns_empty() {
        let (dir, state) = setup();
        let feature = dir.path().join("demo.feature");
        let locs = state.definition(&url(&feature), 0); // "Feature: demo"
        assert!(locs.is_empty());
    }

    #[test]
    fn references_from_def_lists_all_matching_calls() {
        let (dir, state) = setup();
        let steps = dir.path().join("steps.py");
        // The `@given("I have {int} cukes")` decorator is on line 0.
        let locs = state.references(&url(&steps), 0);
        assert_eq!(locs.len(), 2);
        assert!(locs.iter().all(|l| l.uri.path().ends_with("demo.feature")));
    }

    #[test]
    fn references_from_call_lists_sibling_calls() {
        let (dir, state) = setup();
        let feature = dir.path().join("demo.feature");
        // "Given I have 5 cukes" at line 2; siblings should include line 4.
        let locs = state.references(&url(&feature), 2);
        assert_eq!(locs.len(), 2);
    }

    #[test]
    fn rescan_with_picks_up_edits_in_memory() {
        let (dir, mut state) = setup();
        let feature = dir.path().join("demo.feature");
        let new_content = "\
Feature: demo
  Scenario: edited
    When I eat 7
";
        state.rescan_with(&feature, new_content);
        // Old "Given I have 5 cukes" gone; new "When I eat 7" on line 2.
        let locs = state.definition(&url(&feature), 2);
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.path().ends_with("steps.py"));
    }

    #[test]
    fn drop_file_clears_matching_references() {
        let (dir, mut state) = setup();
        let steps = dir.path().join("steps.py");
        state.index.drop_file(&steps);
        let feature = dir.path().join("demo.feature");
        let locs = state.definition(&url(&feature), 2);
        assert!(locs.is_empty());
    }
}
