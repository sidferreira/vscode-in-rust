//! Upstream source: src/vs/workbench/api/browser/mainThreadDocumentsAndEditors.ts
//! Upstream source: src/vs/workbench/api/browser/mainThreadEditors.ts
//!
//! What the app needs on top of the mirror to keep it in step with its own tabs: closing a
//! document, switching the active editor, selections, and turning "the text is now this" into
//! the one edit the host is told about (the app has a text buffer, not a change log).

use crate::documents::{Documents, TextEdit};
use crate::diagnostics::uri_key;
use crate::host::{ExtensionHost, HostError};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use vsr_rpc::codec::Arg;
use vsr_rpc::handshake::file_uri;
use vsr_rpc::proxy_ids::ext_host;

/// The key of a file in the models (`scheme://authority/path`): what diagnostics and decorations
/// are indexed by.
pub fn resource_key(path: &Path) -> String {
    uri_key(&file_uri(path))
}

/// The smallest single replacement that turns `old` into `new` (common prefix and suffix), as
/// 1-based `char` columns; `None` when they are equal. Both texts use `\n`.
pub fn replace_diff(old: &str, new: &str) -> Option<TextEdit> {
    if old == new {
        return None;
    }
    let o: Vec<char> = old.chars().collect();
    let n: Vec<char> = new.chars().collect();
    let prefix = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
    let max_suffix = o.len().min(n.len()) - prefix;
    let suffix = o
        .iter()
        .rev()
        .zip(n.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    let pos = |chars: &[char], upto: usize| {
        let before = &chars[..upto];
        let line = before.iter().filter(|c| **c == '\n').count() + 1;
        let col = before.iter().rev().take_while(|c| **c != '\n').count() + 1;
        (line, col)
    };
    let (sl, sc) = pos(&o, prefix);
    let (el, ec) = pos(&o, o.len() - suffix);
    Some(TextEdit {
        start_line: sl,
        start_column: sc,
        end_line: el,
        end_column: ec,
        text: n[prefix..n.len() - suffix].iter().collect(),
    })
}

impl Documents {
    /// The editor showing `path`, if any.
    pub fn editor_for(&self, path: &Path) -> Option<&str> {
        let key = resource_key(path);
        self.editors
            .iter()
            .find(|(_, k)| **k == key)
            .map(|(id, _)| id.as_str())
    }

    /// Remove a document and its editors; the delta tells the host.
    pub fn remove_document(&mut self, path: &Path) -> Option<Value> {
        let key = resource_key(path);
        let doc = self.open.remove(&key)?;
        let gone: Vec<String> = self
            .editors
            .iter()
            .filter(|(_, k)| **k == key)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &gone {
            self.editors.remove(id);
            if self.active_editor.as_deref() == Some(id) {
                self.active_editor = None;
            }
        }
        Some(json!({
            "removedDocuments": [doc.uri],
            "removedEditors": gone,
            "newActiveEditor": self.active_editor,
        }))
    }

    /// Make `id` (or nothing) the active editor.
    pub fn set_active(&mut self, id: Option<&str>) -> Value {
        self.active_editor = id.map(String::from);
        json!({ "newActiveEditor": self.active_editor })
    }
}

impl ExtensionHost {
    pub fn close_document(&mut self, path: &Path, timeout: Duration) -> Result<(), HostError> {
        let Some(delta) = self.main_thread.documents.remove_document(path) else {
            return Ok(());
        };
        self.call_delta(delta, timeout)
    }

    /// Switch the active editor to the one on `path` (`None`: no active editor, for example a
    /// settings tab is showing).
    pub fn activate_editor(
        &mut self,
        path: Option<&Path>,
        timeout: Duration,
    ) -> Result<(), HostError> {
        let id = path
            .and_then(|p| self.main_thread.documents.editor_for(p))
            .map(String::from);
        if id == self.main_thread.documents.active_editor {
            return Ok(());
        }
        let delta = self.main_thread.documents.set_active(id.as_deref());
        self.call_delta(delta, timeout)
    }

    /// The editor on `path` has this selection (1-based; `anchor` then `active`, line and
    /// column): `onDidChangeTextEditorSelection` fires in extensions.
    pub fn set_selection(
        &mut self,
        path: &Path,
        anchor: (usize, usize),
        active: (usize, usize),
        timeout: Duration,
    ) -> Result<(), HostError> {
        let Some(id) = self.main_thread.documents.editor_for(path).map(String::from) else {
            return Ok(());
        };
        let selection = json!({
            "selectionStartLineNumber": anchor.0, "selectionStartColumn": anchor.1,
            "positionLineNumber": active.0, "positionColumn": active.1,
        });
        self.call(
            ext_host::EXT_HOST_EDITORS,
            "$acceptEditorPropertiesChanged",
            &[
                Arg::Json(json!(id)),
                Arg::Json(json!({ "selections": { "selections": [selection] } })),
            ],
            timeout,
        )
        .map(|_| ())
    }

    fn call_delta(&mut self, delta: Value, timeout: Duration) -> Result<(), HostError> {
        self.call(
            ext_host::EXT_HOST_DOCUMENTS_AND_EDITORS,
            "$acceptDocumentsAndEditorsDelta",
            &[Arg::Json(delta)],
            timeout,
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, e: &TextEdit) -> String {
        let mut d = Documents::default();
        let p = Path::new("/t/a.txt");
        d.add_document(p, text, "plaintext");
        d.edit(p, std::slice::from_ref(e)).unwrap();
        d.open[&resource_key(p)].text.clone()
    }

    #[test]
    fn diff_is_one_replacement_that_reproduces_the_new_text() {
        for (old, new) in [
            ("a\nb\nc", "a\nB\nc"),
            ("abc", "abXc"),
            ("abc", "ab"),
            ("", "hello\nworld"),
            ("hello", ""),
            ("a\nb", "a\nb\nc"),
            ("aaa", "aa"),
            ("héllo\nwörld", "héllo\nwörld!"),
        ] {
            let e = replace_diff(old, new).unwrap();
            assert_eq!(apply(old, &e), new, "{old:?} -> {new:?}: {e:?}");
        }
        assert_eq!(replace_diff("same", "same"), None);
    }

    #[test]
    fn diff_columns_are_one_based() {
        let e = replace_diff("a\nbcd", "a\nbXd").unwrap();
        assert_eq!(
            (e.start_line, e.start_column, e.end_line, e.end_column, e.text.as_str()),
            (2, 2, 2, 3, "X")
        );
    }

    #[test]
    fn removing_a_document_drops_its_editors() {
        let mut d = Documents::default();
        let p = Path::new("/t/a.txt");
        d.add_document(p, "x", "plaintext");
        let (id, _) = d.add_editor(p).unwrap();
        assert_eq!(d.editor_for(p), Some(id.as_str()));
        let delta = d.remove_document(p).unwrap();
        assert_eq!(delta["removedEditors"][0], json!(id));
        assert!(d.open.is_empty() && d.editors.is_empty() && d.active_editor.is_none());
        assert!(d.remove_document(p).is_none());
    }
}
