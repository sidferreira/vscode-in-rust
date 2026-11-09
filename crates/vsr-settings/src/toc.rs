//! The settings table of contents: layout patterns, claim order and sorting.
//!
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsLayout.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsTree.ts @ 50e1350fc78d

use serde::Deserialize;

use crate::schema::SettingSchema;

/// A node of the settings table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TocNode>,
    /// Setting keys, resolved and ordered like upstream (`sortSettings`).
    pub settings: Vec<String>,
}

/// `settings-layout.json`, a port of upstream's `tocData`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutFile {
    pub commonly_used: Vec<String>,
    pub toc: Vec<LayoutNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub settings: Option<Vec<String>>,
    #[serde(default)]
    pub children: Option<Vec<LayoutNode>>,
    /// `desktop` (upstream `!isWeb`) or `windows` (upstream `isWindows`).
    #[serde(default)]
    pub hide_on: Option<String>,
}

/// Tag that hides a setting unless "show advanced settings" is on.
pub(crate) const ADVANCED_TAG: &str = "advanced";

/// Settings listed in more than one node keep being available to the next node.
pub(crate) const MULTI_GROUP_SETTINGS: &[&str] = &[
    "accessibility.signals.chatUserActionRequired",
    "accessibility.signals.chatResponseReceived",
];

pub(crate) fn is_hidden(hide_on: Option<&str>) -> bool {
    match hide_on {
        // This app is a desktop app, so `!isWeb` is true.
        Some("desktop") => true,
        Some("windows") => cfg!(windows),
        _ => false,
    }
}

/// Matches upstream's `createSettingMatchRegExp`: `*` is a wildcard, the whole key must match,
/// and case is ignored.
pub(crate) fn pattern_matches(pattern: &str, key: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let key = key.to_lowercase();
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return key == pattern;
    }
    let Some(rest) = key.strip_prefix(parts[0]) else { return false };
    let Some(mut rest) = rest.strip_suffix(parts[parts.len() - 1]) else { return false };
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    true
}

/// Upstream `sortSettings`: normal before preview before experimental, then `order` (missing
/// last), then key.
pub(crate) fn sort_settings(settings: &mut [&SettingSchema]) {
    fn status(s: &SettingSchema) -> u8 {
        if s.tags.iter().any(|t| t == "experimental") {
            2
        } else if s.tags.iter().any(|t| t == "preview") {
            1
        } else {
            0
        }
    }
    settings.sort_by(|a, b| {
        status(a)
            .cmp(&status(b))
            .then(a.order.unwrap_or(i64::MAX).cmp(&b.order.unwrap_or(i64::MAX)))
            .then(a.key.to_lowercase().cmp(&b.key.to_lowercase()))
            .then(a.key.cmp(&b.key))
    });
}

/// Resolves one layout node against the core settings. Children claim their settings first;
/// the node then takes what is left that matches its own patterns (upstream `_resolveSettingsTree`).
/// `claimed` is indexed like `pool`.
pub(crate) fn resolve_node(node: &LayoutNode, pool: &[&SettingSchema], claimed: &mut [bool]) -> Option<TocNode> {
    if is_hidden(node.hide_on.as_deref()) {
        return None;
    }
    let children: Vec<TocNode> = node
        .children
        .iter()
        .flatten()
        .filter_map(|child| resolve_node(child, pool, claimed))
        .filter(|child| !child.children.is_empty() || !child.settings.is_empty())
        .collect();

    let mut settings = Vec::new();
    if let Some(patterns) = &node.settings {
        let mut matched: Vec<usize> = Vec::new();
        for (i, setting) in pool.iter().enumerate() {
            if claimed[i] || setting.tags.iter().any(|t| t == ADVANCED_TAG) {
                continue;
            }
            if patterns.iter().any(|p| pattern_matches(p, &setting.key)) {
                matched.push(i);
            }
        }
        for &i in &matched {
            if !MULTI_GROUP_SETTINGS.contains(&pool[i].key.as_str()) {
                claimed[i] = true;
            }
        }
        let mut selected: Vec<&SettingSchema> = matched.iter().map(|&i| pool[i]).collect();
        sort_settings(&mut selected);
        settings = selected.iter().map(|s| s.key.clone()).collect();
    }

    Some(TocNode {
        id: node.id.clone(),
        label: node.label.clone(),
        children,
        settings,
    })
}
