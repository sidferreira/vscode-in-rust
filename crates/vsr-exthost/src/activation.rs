//! Upstream source: src/vs/workbench/services/extensions/common/abstractExtensionService.ts
//! Upstream source: src/vs/workbench/services/extensions/common/extensionHostManager.ts
//!
//! Activation events: which events extensions declare (`activationEvents`, plus the implicit ones
//! the scanner adds) and when the app has to fire one. Pure: the app decides *when* (a document
//! opened, a command ran, the workspace was scanned); this module decides *whether anyone cares*
//! and what the event string is.

use crate::description::ExtensionDescription;
use std::collections::BTreeMap;

/// A parsed activation event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationEvent {
    /// `*`: at startup.
    Star,
    /// `onStartupFinished`: after startup, without delaying it.
    StartupFinished,
    /// `onLanguage:<id>`.
    Language(String),
    /// `onCommand:<id>`.
    Command(String),
    /// `workspaceContains:<glob>`.
    WorkspaceContains(String),
    /// Everything else (`onView:`, `onUri`, ...): declared, never fired by this app yet.
    Other(String),
}

impl ActivationEvent {
    pub fn parse(event: &str) -> Self {
        match event {
            "*" => Self::Star,
            "onStartupFinished" => Self::StartupFinished,
            _ => {
                if let Some(id) = event.strip_prefix("onLanguage:") {
                    Self::Language(id.to_string())
                } else if let Some(id) = event.strip_prefix("onCommand:") {
                    Self::Command(id.to_string())
                } else if let Some(glob) = event.strip_prefix("workspaceContains:") {
                    Self::WorkspaceContains(glob.to_string())
                } else {
                    Self::Other(event.to_string())
                }
            }
        }
    }
}

pub fn language_event(language_id: &str) -> String {
    format!("onLanguage:{language_id}")
}

pub fn command_event(command_id: &str) -> String {
    format!("onCommand:{command_id}")
}

/// Whether an extension that declared `declared` wakes up when `fired` happens. Exact match, and a
/// bare `onLanguage` declaration wakes on every language.
pub fn matches(declared: &str, fired: &str) -> bool {
    declared == fired || (declared == "onLanguage" && fired.starts_with("onLanguage:"))
}

/// Declared events to the extensions (ids) that declared them.
#[derive(Debug, Default, Clone)]
pub struct EventIndex {
    declared: BTreeMap<String, Vec<String>>,
}

impl EventIndex {
    pub fn new(extensions: &[ExtensionDescription]) -> Self {
        let mut declared: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for e in extensions {
            for ev in &e.activation_events {
                declared.entry(ev.clone()).or_default().push(e.id.clone());
            }
        }
        Self { declared }
    }

    /// The extensions that an event `fired` wakes.
    pub fn extensions_for(&self, fired: &str) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .declared
            .iter()
            .filter(|(d, _)| matches(d, fired))
            .flat_map(|(_, ids)| ids.iter().map(String::as_str))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Whether firing `fired` would wake anyone (the app skips the call otherwise).
    pub fn is_declared(&self, fired: &str) -> bool {
        self.declared.keys().any(|d| matches(d, fired))
    }

    /// Every `workspaceContains:` glob declared, with its full event string.
    pub fn workspace_contains(&self) -> Vec<(String, String)> {
        self.declared
            .keys()
            .filter_map(|e| match ActivationEvent::parse(e) {
                ActivationEvent::WorkspaceContains(g) => Some((e.clone(), g)),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn ext(id: &str, events: &[&str]) -> ExtensionDescription {
        ExtensionDescription {
            id: id.into(),
            location: PathBuf::from("/x"),
            json: json!({}),
            activation_events: events.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parse_events() {
        use ActivationEvent::*;
        assert_eq!(ActivationEvent::parse("*"), Star);
        assert_eq!(ActivationEvent::parse("onStartupFinished"), StartupFinished);
        assert_eq!(ActivationEvent::parse("onLanguage:rust"), Language("rust".into()));
        assert_eq!(ActivationEvent::parse("onCommand:a.b"), Command("a.b".into()));
        assert_eq!(
            ActivationEvent::parse("workspaceContains:**/*.toml"),
            WorkspaceContains("**/*.toml".into())
        );
        assert_eq!(ActivationEvent::parse("onView:x"), Other("onView:x".into()));
    }

    #[test]
    fn matching_and_index() {
        assert!(matches("onLanguage:rust", "onLanguage:rust"));
        assert!(!matches("onLanguage:rust", "onLanguage:go"));
        assert!(matches("onLanguage", "onLanguage:go"));
        let idx = EventIndex::new(&[
            ext("a.one", &["onLanguage:rust", "workspaceContains:Cargo.toml"]),
            ext("b.two", &["onLanguage", "onCommand:b.run"]),
        ]);
        assert_eq!(idx.extensions_for("onLanguage:rust"), ["a.one", "b.two"]);
        assert_eq!(idx.extensions_for("onLanguage:go"), ["b.two"]);
        assert!(idx.is_declared(&command_event("b.run")));
        assert!(!idx.is_declared(&command_event("nobody")));
        assert_eq!(
            idx.workspace_contains(),
            [("workspaceContains:Cargo.toml".to_string(), "Cargo.toml".to_string())]
        );
    }
}
