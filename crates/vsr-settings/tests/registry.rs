use std::collections::HashSet;

use vsr_settings::{SettingKind, SettingsRegistry};

fn labels(nodes: &[vsr_settings::TocNode]) -> Vec<&str> {
    nodes.iter().map(|n| n.label.as_str()).collect()
}

fn find<'a>(nodes: &'a [vsr_settings::TocNode], label: &str) -> &'a vsr_settings::TocNode {
    nodes.iter().find(|n| n.label == label).unwrap_or_else(|| panic!("no TOC node {label}"))
}

#[test]
fn top_level_toc_matches_upstream_layout() {
    let registry = SettingsRegistry::load_default();
    assert_eq!(
        labels(registry.toc()),
        ["Commonly Used", "Text Editor", "Workbench", "Window", "Chat", "Features", "Application", "Security", "Extensions"]
    );
}

#[test]
fn text_editor_font_contains_font_size() {
    let registry = SettingsRegistry::load_default();
    let editor = find(registry.toc(), "Text Editor");
    let font = find(&editor.children, "Font");
    assert!(font.settings.contains(&"editor.fontSize".to_string()));
    // Each key is placed once in the core tree, except upstream's multi-group settings.
    let mut seen = HashSet::new();
    fn walk<'a>(nodes: &'a [vsr_settings::TocNode], seen: &mut HashSet<&'a str>, dupes: &mut Vec<&'a str>) {
        for node in nodes {
            for key in &node.settings {
                if !seen.insert(key.as_str()) {
                    dupes.push(key.as_str());
                }
            }
            walk(&node.children, seen, dupes);
        }
    }
    let mut dupes = Vec::new();
    let core: Vec<_> = registry.toc().iter().filter(|n| n.id != "commonlyUsed" && n.id != "extensions").cloned().collect();
    walk(&core, &mut seen, &mut dupes);
    let allowed = ["accessibility.signals.chatUserActionRequired", "accessibility.signals.chatResponseReceived"];
    let unexpected: Vec<_> = dupes.iter().filter(|k| !allowed.contains(k)).collect();
    assert!(unexpected.is_empty(), "duplicate placements: {unexpected:?}");
}

#[test]
fn every_toc_key_is_a_registered_setting() {
    let registry = SettingsRegistry::load_default();
    fn check(registry: &SettingsRegistry, nodes: &[vsr_settings::TocNode]) {
        for node in nodes {
            for key in &node.settings {
                assert!(registry.get(key).is_some(), "TOC key {key} is not registered");
            }
            check(registry, &node.children);
        }
    }
    check(&registry, registry.toc());
}

#[test]
fn editor_font_size_setting_details() {
    let registry = SettingsRegistry::load_default();
    let setting = registry.get("editor.fontSize").expect("registered");
    assert_eq!(setting.title, "Editor: Font Size");
    assert_eq!(setting.category_label, "Editor");
    assert_eq!(setting.name_label, "Font Size");
    assert!(matches!(setting.kind, SettingKind::Number { integer: false, .. }));
    assert!(setting.default.is_number());
    assert!(!setting.description.is_empty());
}

#[test]
fn commonly_used_starts_with_font_size() {
    let registry = SettingsRegistry::load_default();
    let common = registry.commonly_used();
    assert_eq!(common.first().map(|s| s.key.as_str()), Some("editor.fontSize"));
    assert!(common.len() >= 10);
}

#[test]
fn extensions_node_groups_extension_settings() {
    let registry = SettingsRegistry::load_default();
    let extensions = find(registry.toc(), "Extensions");
    assert!(!extensions.children.is_empty());
    assert!(extensions.children.iter().all(|c| !c.settings.is_empty() || !c.children.is_empty()));
}

#[test]
fn search_finds_font_size_first() {
    let registry = SettingsRegistry::load_default();
    let results = registry.search("font size");
    assert_eq!(results.first().map(|s| s.key.as_str()), Some("editor.fontSize"));
    assert!(registry.search("   ").is_empty());
}

#[test]
fn registry_has_all_exported_settings() {
    let registry = SettingsRegistry::load_default();
    assert!(registry.len() > 2000, "got {}", registry.len());
}
