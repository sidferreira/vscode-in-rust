//! The status bar entries extensions show, ready to paint (ADR-0067): `$(icon)` text split into
//! icons and words, tooltip and click command extracted, colours resolved.
//!
//! Upstream source: src/vs/workbench/api/browser/mainThreadStatusBar.ts
//! Upstream source: src/vs/base/common/iconLabels.ts

use super::paint::parse_color;
use super::snapshot::Snapshot;
use crate::icons;
use crate::theme::Rgba8;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Icon(char),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// The item's id (`$setEntry`'s first argument).
    pub id: String,
    pub parts: Vec<Part>,
    pub tooltip: Option<String>,
    /// The command a click runs, with its arguments.
    pub command: Option<(String, Vec<Value>)>,
    pub align_left: bool,
    pub color: Option<Rgba8>,
    pub background: Option<Rgba8>,
}

impl Item {
    /// The text with icons dropped, for tests and accessibility.
    #[cfg(test)]
    pub fn plain_text(&self) -> String {
        let s: String = self
            .parts
            .iter()
            .map(|p| match p {
                Part::Text(t) => t.as_str(),
                Part::Icon(_) => "",
            })
            .collect();
        s.trim().to_string()
    }
}

/// The codicons we have glyphs for. Others are dropped (`$(spinner~spin)` and friends).
fn icon(name: &str) -> Option<char> {
    let name = name.split('~').next().unwrap_or(name);
    Some(match name {
        "error" | "x" => icons::ERROR,
        "warning" => icons::WARNING,
        "check" => icons::CHECK,
        "bell" => icons::BELL,
        "gear" | "settings-gear" => icons::GEAR,
        "circle-filled" => icons::CIRCLE_FILLED,
        "git-branch" => icons::GIT_BRANCH,
        "sync" | "refresh" => icons::REFRESH,
        "close" => icons::CLOSE,
        "lock" => icons::LOCK,
        "file" => icons::FILE,
        "folder" => icons::FOLDER,
        "search" => icons::SEARCH,
        "extensions" => icons::EXTENSIONS,
        _ => return None,
    })
}

/// `$(error) 3 problems` into an icon and text.
pub fn parse_parts(text: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut rest = text;
    let mut plain = String::new();
    while let Some(start) = rest.find("$(") {
        let Some(end) = rest[start..].find(')') else { break };
        let name = &rest[start + 2..start + end];
        let valid = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '~'));
        if !valid {
            plain.push_str(&rest[..start + 2]);
            rest = &rest[start + 2..];
            continue;
        }
        plain.push_str(&rest[..start]);
        if let Some(c) = icon(name) {
            if !plain.is_empty() {
                parts.push(Part::Text(std::mem::take(&mut plain)));
            }
            parts.push(Part::Icon(c));
        }
        rest = &rest[start + end + 1..];
    }
    plain.push_str(rest);
    if !plain.is_empty() {
        parts.push(Part::Text(plain));
    }
    parts
}

fn tooltip_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Object(o) => o.get("value").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from),
        _ => None,
    }
}

/// Items in display order (left first, as `StatusBar::sorted` gives them).
pub fn items(snapshot: &Snapshot, resolve: &dyn Fn(&str) -> Option<Rgba8>) -> Vec<Item> {
    snapshot
        .status
        .iter()
        .map(|e| Item {
            id: e.id.clone(),
            parts: parse_parts(&e.text),
            tooltip: tooltip_text(&e.tooltip),
            command: e.command.get("id").and_then(Value::as_str).map(|id| {
                (id.to_string(), e.command.get("arguments").and_then(Value::as_array).cloned().unwrap_or_default())
            }),
            align_left: e.align_left,
            color: parse_color(&e.color, resolve),
            background: parse_color(&e.background_color, resolve),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vsr_exthost::status_bar::StatusBarEntry;

    #[test]
    fn icons_and_text() {
        assert_eq!(parse_parts("$(error) 3"), [Part::Icon(icons::ERROR), Part::Text(" 3".into())]);
        assert_eq!(parse_parts("plain"), [Part::Text("plain".into())]);
        assert_eq!(parse_parts("a $(unknown-icon) b"), [Part::Text("a  b".into())], "unknown icons are dropped");
        assert_eq!(parse_parts("cost $5 (x)"), [Part::Text("cost $5 (x)".into())]);
        assert_eq!(
            parse_parts("$(check)$(warning)"),
            [Part::Icon(icons::CHECK), Part::Icon(icons::WARNING)]
        );
    }

    #[test]
    fn entries_become_items() {
        let mut s = Snapshot::default();
        s.status.push(StatusBarEntry {
            id: "e1".into(),
            status_id: "s".into(),
            extension_id: Some("p.x".into()),
            name: "n".into(),
            text: "$(error) 2".into(),
            tooltip: json!({ "value": "two errors" }),
            has_tooltip_provider: false,
            command: json!({ "id": "x.show", "title": "", "arguments": [1, "a"] }),
            color: json!("#ff0000"),
            background_color: Value::Null,
            align_left: false,
            priority: Some(100.0),
        });
        let items = items(&s, &|_| None);
        assert_eq!(items[0].plain_text(), "2");
        assert_eq!(items[0].tooltip.as_deref(), Some("two errors"));
        assert_eq!(items[0].command, Some(("x.show".into(), vec![json!(1), json!("a")])));
        assert_eq!(items[0].color, Some(Rgba8(255, 0, 0, 255)));
        assert_eq!(items[0].background, None);
    }
}
