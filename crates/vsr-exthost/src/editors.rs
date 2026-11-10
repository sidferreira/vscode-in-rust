//! upstream: src/vs/workbench/api/browser/mainThreadEditors.ts @ 50e1350fc78d
//!
//! `MainThreadTextEditors`: decoration types and the decorations extensions set per editor. The
//! app paints [`Editors::decorations_for`] over the editor showing that document.

use crate::main_thread::{arg, str_arg};
use serde_json::Value;
use std::collections::BTreeMap;
use vsr_rpc::codec::Reply;
use vsr_rpc::rpc::{Call, HandlerError};

/// One decoration instance: a 1-based range plus optional hover and per-instance render options.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoration {
    pub start_line: u64,
    pub start_column: u64,
    pub end_line: u64,
    pub end_column: u64,
    pub hover_message: Value,
    /// `IDecorationInstanceRenderOptions` (for example `after.contentText`); `Null` when none.
    pub render_options: Value,
}

#[derive(Debug, Default)]
pub struct Editors {
    /// type key -> `IDecorationRenderOptions` JSON, with the extension that registered it.
    pub decoration_types: BTreeMap<String, (String, Value)>,
    /// (editor id, type key) -> decorations (an empty set is removed).
    pub decorations: BTreeMap<(String, String), Vec<Decoration>>,
}

fn range_of(v: &Value) -> (u64, u64, u64, u64) {
    let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    (
        n("startLineNumber"),
        n("startColumn"),
        n("endLineNumber"),
        n("endColumn"),
    )
}

impl Editors {
    /// Every decoration on an editor, with the type key it belongs to.
    pub fn decorations_for(&self, editor: &str) -> Vec<(&str, &Decoration)> {
        self.decorations
            .iter()
            .filter(|((e, _), _)| e == editor)
            .flat_map(|((_, key), ds)| ds.iter().map(move |d| (key.as_str(), d)))
            .collect()
    }

    fn set(&mut self, editor: &str, key: &str, list: Vec<Decoration>) {
        let k = (editor.to_string(), key.to_string());
        if list.is_empty() {
            self.decorations.remove(&k);
        } else {
            self.decorations.insert(k, list);
        }
    }

    pub(crate) fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        Some(Ok(match call.method.as_str() {
            "$registerTextEditorDecorationType" => {
                let ext = crate::main_thread::extension_id(arg(call, 0));
                self.decoration_types
                    .insert(str_arg(call, 1).to_string(), (ext, arg(call, 2).clone()));
                Reply::Empty
            }
            "$removeTextEditorDecorationType" => {
                let key = str_arg(call, 0).to_string();
                self.decoration_types.remove(&key);
                self.decorations.retain(|(_, k), _| *k != key);
                Reply::Empty
            }
            "$trySetDecorations" => {
                let list = arg(call, 2)
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|o| {
                        let (a, b, c, d) = range_of(&o["range"]);
                        Decoration {
                            start_line: a,
                            start_column: b,
                            end_line: c,
                            end_column: d,
                            hover_message: o.get("hoverMessage").cloned().unwrap_or(Value::Null),
                            render_options: o.get("renderOptions").cloned().unwrap_or(Value::Null),
                        }
                    })
                    .collect();
                self.set(str_arg(call, 0), str_arg(call, 1), list);
                Reply::Empty
            }
            // The compact form: four 1-based numbers per range.
            "$trySetDecorationsFast" => {
                let nums: Vec<u64> = arg(call, 2)
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|n| n.as_u64().unwrap_or(0))
                    .collect();
                let list = nums
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| Decoration {
                        start_line: c[0],
                        start_column: c[1],
                        end_line: c[2],
                        end_column: c[3],
                        hover_message: Value::Null,
                        render_options: Value::Null,
                    })
                    .collect();
                self.set(str_arg(call, 0), str_arg(call, 1), list);
                Reply::Empty
            }
            _ => return None,
        }))
    }
}
