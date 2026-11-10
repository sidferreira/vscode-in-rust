//! upstream: src/vs/workbench/api/browser/mainThreadStatusBar.ts @ 50e1350fc78d
//!
//! `MainThreadStatusBar`: the entries extensions show. The app paints [`StatusBar::sorted`].

use crate::main_thread::{arg, str_arg};
use serde_json::Value;
use std::collections::BTreeMap;
use vsr_rpc::codec::Reply;
use vsr_rpc::rpc::{Call, HandlerError};

/// One `$setEntry`.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusBarEntry {
    /// The item's own id (`$disposeEntry` takes it).
    pub id: String,
    pub status_id: String,
    pub extension_id: Option<String>,
    pub name: String,
    pub text: String,
    /// A string or an `IMarkdownString`; `Null` when none.
    pub tooltip: Value,
    pub has_tooltip_provider: bool,
    /// `ICommandDto` `{ id, title, arguments }` of the click, `Null` when none.
    pub command: Value,
    pub color: Value,
    pub background_color: Value,
    pub align_left: bool,
    pub priority: Option<f64>,
}

#[derive(Debug, Default)]
pub struct StatusBar {
    pub entries: BTreeMap<String, StatusBarEntry>,
}

impl StatusBar {
    /// Entries in display order: left ones first, then right; higher priority nearer the edge
    /// (left: first, right: last), like `StatusbarService`.
    pub fn sorted(&self) -> Vec<&StatusBarEntry> {
        let mut v: Vec<&StatusBarEntry> = self.entries.values().collect();
        let p = |e: &StatusBarEntry| e.priority.unwrap_or(0.0);
        v.sort_by(|a, b| {
            b.align_left.cmp(&a.align_left).then_with(|| {
                let ord = p(b).partial_cmp(&p(a)).unwrap_or(std::cmp::Ordering::Equal);
                if a.align_left { ord } else { ord.reverse() }
            })
        });
        v
    }

    pub(crate) fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        Some(Ok(match call.method.as_str() {
            "$setEntry" => {
                let id = str_arg(call, 0).to_string();
                let opt = |i: usize| arg(call, i).as_str().map(String::from);
                self.entries.insert(
                    id.clone(),
                    StatusBarEntry {
                        id,
                        status_id: str_arg(call, 1).to_string(),
                        extension_id: opt(2),
                        name: str_arg(call, 3).to_string(),
                        text: str_arg(call, 4).to_string(),
                        tooltip: arg(call, 5).clone(),
                        has_tooltip_provider: arg(call, 6).as_bool().unwrap_or(false),
                        command: arg(call, 7).clone(),
                        color: arg(call, 8).clone(),
                        background_color: arg(call, 9).clone(),
                        align_left: arg(call, 10).as_bool().unwrap_or(false),
                        priority: arg(call, 11).as_f64(),
                    },
                );
                Reply::Empty
            }
            "$disposeEntry" => {
                self.entries.remove(str_arg(call, 0));
                Reply::Empty
            }
            _ => return None,
        }))
    }
}
