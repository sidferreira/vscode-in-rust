//! upstream: src/vs/workbench/api/browser/mainThreadDiagnostics.ts @ 50e1350fc78d
//!
//! `MainThreadDiagnostics`: markers per owner (a `DiagnosticCollection`) and resource. The app
//! shows [`Diagnostics::markers_for`] in the editor and the problem counts in the status bar.

use crate::main_thread::{arg, str_arg};
use serde_json::Value;
use std::collections::BTreeMap;
use vsr_rpc::codec::Reply;
use vsr_rpc::rpc::{Call, HandlerError};

/// `MarkerSeverity`.
pub const HINT: u64 = 1;
pub const INFO: u64 = 2;
pub const WARNING: u64 = 4;
pub const ERROR: u64 = 8;

/// A string key for a `UriComponents` value: `scheme://authority/path`.
pub fn uri_key(uri: &Value) -> String {
    let s = |k: &str| uri.get(k).and_then(Value::as_str).unwrap_or("");
    format!("{}://{}{}", s("scheme"), s("authority"), s("path"))
}

#[derive(Debug, Default)]
pub struct Diagnostics {
    /// owner -> resource key -> `IMarkerData` JSON (`severity`, `message`, `startLineNumber`, ...).
    pub by_owner: BTreeMap<String, BTreeMap<String, Vec<Value>>>,
}

impl Diagnostics {
    /// All markers on a resource from every owner.
    pub fn markers_for(&self, resource: &str) -> Vec<&Value> {
        self.by_owner
            .values()
            .filter_map(|m| m.get(resource))
            .flatten()
            .collect()
    }

    /// Marker counts `(errors, warnings)` over everything.
    pub fn counts(&self) -> (usize, usize) {
        let mut e = 0;
        let mut w = 0;
        for m in self.by_owner.values().flat_map(|m| m.values()).flatten() {
            match m.get("severity").and_then(Value::as_u64) {
                Some(ERROR) => e += 1,
                Some(WARNING) => w += 1,
                _ => {}
            }
        }
        (e, w)
    }

    pub(crate) fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        match call.method.as_str() {
            "$changeMany" => {
                let owner = self
                    .by_owner
                    .entry(str_arg(call, 0).to_string())
                    .or_default();
                for entry in arg(call, 1).as_array().into_iter().flatten() {
                    let key = uri_key(&entry[0]);
                    match entry.get(1).and_then(Value::as_array) {
                        Some(markers) if !markers.is_empty() => {
                            owner.insert(key, markers.clone());
                        }
                        // `undefined` (serialised as null) or empty: the resource is cleared.
                        _ => {
                            owner.remove(&key);
                        }
                    }
                }
                Some(Ok(Reply::Empty))
            }
            "$clear" => {
                self.by_owner.remove(str_arg(call, 0));
                Some(Ok(Reply::Empty))
            }
            _ => None,
        }
    }
}
