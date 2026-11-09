//! The registry of settings and the table of contents built from it.
//!
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsTree.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsEditor2.ts @ 50e1350fc78d

use std::collections::HashMap;

use serde::Deserialize;

use crate::schema::{RawSetting, SettingSchema};
use crate::toc::{ADVANCED_TAG, LayoutFile, TocNode, resolve_node, sort_settings};

const DEFAULT_CONFIGURATION: &str = include_str!("../assets/default-configuration.json");
const SETTINGS_LAYOUT: &str = include_str!("../assets/settings-layout.json");

#[derive(Deserialize)]
struct DefaultConfiguration {
    settings: Vec<RawSetting>,
}

/// Which built-in extension contributes a setting, and under which configuration group.
struct ExtensionInfo {
    extension_id: String,
    display_name: String,
    group_title: String,
}

/// All registered settings, the TOC, and search.
pub struct SettingsRegistry {
    settings: Vec<SettingSchema>,
    index: HashMap<String, usize>,
    commonly_used: Vec<String>,
    toc: Vec<TocNode>,
    /// Pre-order position of each key's first appearance in the TOC.
    toc_position: HashMap<String, usize>,
}

impl SettingsRegistry {
    /// The settings bundled with this crate (VS Code 1.144.1, see `assets/README.md`).
    pub fn load_default() -> Self {
        Self::from_json(DEFAULT_CONFIGURATION, SETTINGS_LAYOUT).expect("bundled settings assets are valid")
    }

    fn from_json(config: &str, layout: &str) -> Result<Self, serde_json::Error> {
        let config: DefaultConfiguration = serde_json::from_str(config)?;
        let layout: LayoutFile = serde_json::from_str(layout)?;

        let mut settings = Vec::with_capacity(config.settings.len());
        let mut extension = Vec::with_capacity(config.settings.len());
        for raw in config.settings {
            extension.push(match raw.source.as_deref() {
                None | Some("core") => None,
                Some(id) => Some(ExtensionInfo {
                    extension_id: id.to_string(),
                    display_name: raw.extension_display_name.clone().unwrap_or_else(|| id.to_string()),
                    group_title: raw.extension_title.clone().unwrap_or_default(),
                }),
            });
            settings.push(SettingSchema::from_raw(raw));
        }
        let index: HashMap<String, usize> = settings.iter().enumerate().map(|(i, s)| (s.key.clone(), i)).collect();

        // Core settings are placed by the layout; extension settings go under "Extensions".
        let core: Vec<&SettingSchema> = settings
            .iter()
            .zip(&extension)
            .filter(|(_, info)| info.is_none())
            .map(|(s, _)| s)
            .collect();
        let mut claimed = vec![false; core.len()];
        let commonly_used: Vec<String> = layout.commonly_used.into_iter().filter(|k| index.contains_key(k)).collect();

        let mut toc = vec![TocNode {
            id: "commonlyUsed".into(),
            label: "Commonly Used".into(),
            children: Vec::new(),
            settings: commonly_used.clone(),
        }];
        toc.extend(
            layout
                .toc
                .iter()
                .filter_map(|node| resolve_node(node, &core, &mut claimed))
                .filter(|node| !node.children.is_empty() || !node.settings.is_empty()),
        );
        let extensions = extensions_node(&settings, &extension);
        if !extensions.children.is_empty() {
            toc.push(extensions);
        }

        let mut toc_position = HashMap::new();
        let mut position = 0;
        record_positions(&toc, &mut position, &mut toc_position);

        Ok(Self { settings, index, commonly_used, toc, toc_position })
    }

    /// The number of registered settings (all of them, including hidden and advanced ones).
    pub fn len(&self) -> usize {
        self.settings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.settings.is_empty()
    }

    /// All registered keys, in registration order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.settings.iter().map(|s| s.key.as_str())
    }

    pub fn get(&self, key: &str) -> Option<&SettingSchema> {
        self.index.get(key).map(|&i| &self.settings[i])
    }

    /// The table of contents as the Settings editor shows it by default: advanced settings are
    /// left out, and "Commonly Used" comes first, "Extensions" last.
    pub fn toc(&self) -> &[TocNode] {
        &self.toc
    }

    pub fn commonly_used(&self) -> Vec<&SettingSchema> {
        self.commonly_used.iter().filter_map(|k| self.get(k)).collect()
    }

    /// Ranked search over key, title and description. Every word of the query must appear.
    /// Matches on the setting's name ("Font Size") rank first; ties go to the earlier TOC position.
    pub fn search(&self, query: &str) -> Vec<&SettingSchema> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let words: Vec<&str> = query.split_whitespace().collect();
        let compact: String = query.chars().filter(|c| !c.is_whitespace()).collect();

        let mut scored: Vec<(u32, usize, &SettingSchema)> = Vec::new();
        for setting in &self.settings {
            let title = setting.title.to_lowercase();
            let name = setting.name_label.to_lowercase();
            let key = setting.key.to_lowercase();
            let haystack = format!("{title} {key} {}", setting.description.to_lowercase());
            if !words.iter().all(|w| haystack.contains(w)) {
                continue;
            }
            let key_compact: String = key.chars().filter(|c| *c != '.').collect();
            let score = if name == query {
                1000
            } else if name.contains(&query) {
                600
            } else if title.contains(&query) {
                500
            } else if key_compact.contains(&compact) {
                400
            } else {
                100
            };
            let position = self.toc_position.get(&setting.key).copied().unwrap_or(usize::MAX);
            scored.push((score, position, setting));
        }
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(a.1.cmp(&b.1))
                .then(a.2.key.cmp(&b.2.key))
        });
        scored.into_iter().map(|(_, _, s)| s).collect()
    }

    /// Whether the TOC node is shown in the default UI (used by tests and tools).
    pub fn is_shown_by_default(&self, key: &str) -> bool {
        self.get(key).is_some_and(|s| !s.tags.iter().any(|t| t == ADVANCED_TAG))
    }
}

fn record_positions(nodes: &[TocNode], position: &mut usize, out: &mut HashMap<String, usize>) {
    for node in nodes {
        for key in &node.settings {
            out.entry(key.clone()).or_insert(*position);
            *position += 1;
        }
        record_positions(&node.children, position, out);
    }
}

/// Port of `createTocTreeForExtensionSettings`: one node per extension, flattened when it has one
/// group, with a group named like the extension used as the ungrouped entry.
fn extensions_node(settings: &[SettingSchema], extension: &[Option<ExtensionInfo>]) -> TocNode {
    // extension id -> (display name, groups in first-seen order: (title, setting indices))
    type Group = (String, Vec<usize>);
    let mut exts: Vec<(String, String, Vec<Group>)> = Vec::new();
    for (i, info) in extension.iter().enumerate() {
        let Some(info) = info else { continue };
        if settings[i].tags.iter().any(|t| t == ADVANCED_TAG) {
            continue;
        }
        let pos = match exts.iter().position(|e| e.0 == info.extension_id) {
            Some(pos) => pos,
            None => {
                exts.push((info.extension_id.clone(), info.display_name.clone(), Vec::new()));
                exts.len() - 1
            }
        };
        let groups = &mut exts[pos].2;
        match groups.iter().position(|g| g.0 == info.group_title) {
            Some(g) => groups[g].1.push(i),
            None => groups.push((info.group_title.clone(), vec![i])),
        }
    }

    let mut children: Vec<TocNode> = Vec::new();
    for (id, display, groups) in exts {
        let mut group_nodes: Vec<TocNode> = groups
            .into_iter()
            .map(|(title, indices)| {
                let mut selected: Vec<&SettingSchema> = indices.iter().map(|&i| &settings[i]).collect();
                sort_settings(&mut selected);
                TocNode {
                    id: format!("{id}/{title}"),
                    label: title,
                    children: Vec::new(),
                    settings: selected.iter().map(|s| s.key.clone()).collect(),
                }
            })
            .collect();
        if group_nodes.len() == 1 {
            let only = group_nodes.remove(0);
            children.push(TocNode { id, label: display, children: Vec::new(), settings: only.settings });
        } else if let Some(pos) = group_nodes.iter().position(|g| g.label == display) {
            let ungrouped = group_nodes.remove(pos);
            children.push(TocNode { id, label: display, children: group_nodes, settings: ungrouped.settings });
        } else {
            children.push(TocNode { id, label: display, children: group_nodes, settings: Vec::new() });
        }
    }
    children.sort_by_key(|a| a.label.to_lowercase());
    TocNode { id: "extensions".into(), label: "Extensions".into(), children, settings: Vec::new() }
}
