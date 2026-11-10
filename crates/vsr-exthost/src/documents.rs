//! upstream: src/vs/workbench/api/browser/mainThreadDocuments.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/browser/mainThreadDocumentsAndEditors.ts @ 50e1350fc78d
//!
//! The main thread's view of open documents and editors, which the host mirrors
//! (`ExtHostDocumentsAndEditors`). This model makes the messages: [`Documents::add_document`],
//! [`Documents::add_editor`], [`Documents::edit`] return the JSON to send
//! (`$acceptDocumentsAndEditorsDelta`, `$acceptModelChanged`). It also answers `MainThreadDocuments`
//! (`$tryOpenDocument`, `$trySaveDocument`).
//!
//! Columns here count `char`s, which equals UTF-16 units for the BMP; astral characters are not
//! handled yet.

use crate::diagnostics::uri_key;
use crate::main_thread::{arg, str_arg};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::file_uri;
use vsr_rpc::rpc::{Call, HandlerError};

#[derive(Debug, Clone)]
pub struct OpenDocument {
    pub uri: Value,
    pub version: u64,
    pub text: String,
    pub language: String,
    pub dirty: bool,
}

/// A replacement of a 1-based range (end exclusive) in a document.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEdit {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Documents {
    /// resource key -> document.
    pub open: BTreeMap<String, OpenDocument>,
    /// editor id -> resource key.
    pub editors: BTreeMap<String, String>,
    pub active_editor: Option<String>,
    /// Documents the extension asked to save (`$trySaveDocument`), by resource key.
    pub saved: Vec<String>,
    next_editor: u32,
}

fn offset_of(text: &str, line: usize, column: usize) -> usize {
    let mut off = 0;
    for (i, l) in text.split('\n').enumerate() {
        if i + 1 == line {
            return off + column.saturating_sub(1).min(l.chars().count());
        }
        off += l.chars().count() + 1;
    }
    text.chars().count()
}

impl Documents {
    /// Make `path` an open document and return the delta that tells the host.
    pub fn add_document(&mut self, path: &Path, text: &str, language: &str) -> Value {
        let uri = file_uri(path);
        let key = uri_key(&uri);
        let lines: Vec<&str> = text.split('\n').collect();
        self.open.insert(
            key,
            OpenDocument {
                uri: uri.clone(),
                version: 1,
                text: text.to_string(),
                language: language.to_string(),
                dirty: false,
            },
        );
        json!({ "addedDocuments": [{
            "uri": uri, "versionId": 1, "lines": lines, "EOL": "\n",
            "languageId": language, "isDirty": false, "encoding": "utf8",
        }] })
    }

    /// Show an editor on an open document and make it active. `None` when the document is not open.
    pub fn add_editor(&mut self, path: &Path) -> Option<(String, Value)> {
        let uri = file_uri(path);
        let key = uri_key(&uri);
        self.open.get(&key)?;
        self.next_editor += 1;
        let id = format!("{{\"id\":\"rscode\",\"editor\":{}}}", self.next_editor);
        self.editors.insert(id.clone(), key);
        self.active_editor = Some(id.clone());
        let delta = json!({
            "addedEditors": [{
                "id": id, "documentUri": uri,
                "options": { "tabSize": 4, "indentSize": 4, "originalIndentSize": "tabSize",
                             "insertSpaces": true, "cursorStyle": 1, "lineNumbers": 1 },
                "selections": [{ "selectionStartLineNumber": 1, "selectionStartColumn": 1,
                                 "positionLineNumber": 1, "positionColumn": 1 }],
                "visibleRanges": [{ "startLineNumber": 1, "startColumn": 1,
                                    "endLineNumber": 1, "endColumn": 1 }],
                "editorPosition": 1,
            }],
            "newActiveEditor": id,
        });
        Some((id, delta))
    }

    /// Close an editor; the delta tells the host.
    pub fn remove_editor(&mut self, id: &str) -> Value {
        self.editors.remove(id);
        if self.active_editor.as_deref() == Some(id) {
            self.active_editor = None;
        }
        json!({ "removedEditors": [id], "newActiveEditor": self.active_editor })
    }

    /// Apply edits (given against the text before any of them, in any order) to an open document
    /// and return `(uri, ISerializedModelContentChangedEvent, isDirty)` for `$acceptModelChanged`.
    pub fn edit(&mut self, path: &Path, edits: &[TextEdit]) -> Option<(Value, Value, bool)> {
        let key = uri_key(&file_uri(path));
        let doc = self.open.get_mut(&key)?;
        let mut sorted: Vec<&TextEdit> = edits.iter().collect();
        // Last edit first, so earlier offsets stay valid (the order upstream sends them in).
        sorted.sort_by_key(|e| std::cmp::Reverse((e.start_line, e.start_column)));
        let mut changes = vec![];
        for e in sorted {
            let from = offset_of(&doc.text, e.start_line, e.start_column);
            let to = offset_of(&doc.text, e.end_line, e.end_column);
            let (from_b, to_b) = (byte_of(&doc.text, from), byte_of(&doc.text, to));
            doc.text.replace_range(from_b..to_b, &e.text);
            changes.push(json!({
                "range": { "startLineNumber": e.start_line, "startColumn": e.start_column,
                           "endLineNumber": e.end_line, "endColumn": e.end_column },
                "rangeOffset": from, "rangeLength": to - from, "text": e.text,
            }));
        }
        doc.version += 1;
        doc.dirty = true;
        let event = json!({
            "changes": changes, "eol": "\n", "versionId": doc.version,
            "isUndoing": false, "isRedoing": false, "isFlush": false, "isEolChange": false,
        });
        Some((doc.uri.clone(), event, true))
    }

    pub(crate) fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        match call.method.as_str() {
            "$tryOpenDocument" => {
                let uri = arg(call, 0);
                Some(if self.open.contains_key(&uri_key(uri)) {
                    Ok(Reply::Json(uri.clone()))
                } else {
                    Err(HandlerError::error(format!(
                        "cannot open {}: not an open document",
                        uri_key(uri)
                    )))
                })
            }
            "$trySaveDocument" => {
                let key = uri_key(arg(call, 0));
                let known = self.open.get_mut(&key).map(|d| d.dirty = false).is_some();
                if known {
                    self.saved.push(key);
                }
                Some(Ok(Reply::Json(Value::Bool(known))))
            }
            "$tryCreateDocument" => Some(Err(HandlerError::error(
                "untitled documents are not supported yet",
            ))),
            _ => {
                let _ = str_arg(call, 0);
                None
            }
        }
    }
}

fn byte_of(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(b, _)| b)
}
