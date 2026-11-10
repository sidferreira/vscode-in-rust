//! What the extension host last told the main thread, copied out of the models so the UI thread
//! can read it without sharing them with the worker thread that owns the host (ADR-0066).
//!
//! Upstream source: src/vs/workbench/api/browser/mainThreadStatusBar.ts
//! Upstream source: src/vs/workbench/api/browser/mainThreadEditors.ts
//! Upstream source: src/vs/workbench/api/browser/mainThreadDiagnostics.ts

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vsr_exthost::editors::Decoration;
use vsr_exthost::main_thread::MainThread;
use vsr_exthost::status_bar::StatusBarEntry;

/// A decoration with the key of its decoration type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedDecoration {
    pub type_key: String,
    pub decoration: Decoration,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// In display order (`StatusBar::sorted`).
    pub status: Vec<StatusBarEntry>,
    /// Resource key (`file:///path`) to the decorations on it, from every editor showing it.
    pub decorations: BTreeMap<String, Vec<TypedDecoration>>,
    /// Decoration type key to its `IDecorationRenderOptions`.
    pub decoration_types: BTreeMap<String, Value>,
    /// Resource key to the markers on it, every owner together.
    pub diagnostics: BTreeMap<String, Vec<Value>>,
    /// `(errors, warnings)` over every resource.
    pub counts: (usize, usize),
    /// Command ids extensions registered at run time.
    pub commands: BTreeSet<String>,
    /// `publisher.name: message` of every failed activation.
    pub errors: Vec<String>,
}

impl Snapshot {
    pub fn of(mt: &MainThread) -> Snapshot {
        let mut decorations: BTreeMap<String, Vec<TypedDecoration>> = BTreeMap::new();
        for ((editor, type_key), list) in &mt.editors.decorations {
            let Some(resource) = mt.documents.editors.get(editor) else { continue };
            decorations.entry(resource.clone()).or_default().extend(list.iter().map(|d| TypedDecoration {
                type_key: type_key.clone(),
                decoration: d.clone(),
            }));
        }
        let mut diagnostics: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for per_resource in mt.diagnostics.by_owner.values() {
            for (resource, markers) in per_resource {
                diagnostics.entry(resource.clone()).or_default().extend(markers.iter().cloned());
            }
        }
        Snapshot {
            status: mt.status_bar.sorted().into_iter().cloned().collect(),
            decorations,
            decoration_types: mt.editors.decoration_types.iter().map(|(k, (_, v))| (k.clone(), v.clone())).collect(),
            diagnostics,
            counts: mt.diagnostics.counts(),
            commands: mt.commands.registered.clone(),
            errors: mt
                .extensions
                .activation_errors
                .iter()
                .map(|e| format!("{}: {}", e.id, e.message()))
                .collect(),
        }
    }
}

/// The key of a `file` resource, as the models index it.
pub fn resource_key(path: &std::path::Path) -> String {
    vsr_exthost::app_sync::resource_key(path)
}
