//! Settings editor view-model: pure, UI-free row and table-of-contents logic.
//!
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsTreeModels.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/preferences/browser/tocTree.ts @ 50e1350fc78d

use std::collections::{HashMap, HashSet};

use vsr_settings::TocNode;

/// One row of the right-hand settings list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A group title. `level` is 1 for top-level TOC nodes (26px), 2 (22px), 3+ (18px).
    Header { id: String, label: String, level: u8 },
    /// A setting. `id` is unique per row (`<group>:<key>`) since "Commonly Used"
    /// repeats settings that also appear in their own group.
    Setting { id: String, key: String },
}

/// Rows for the whole TOC: per node a header, then its own settings, then its children.
pub fn build_rows(toc: &[TocNode]) -> Vec<Row> {
    let mut rows = Vec::new();
    for node in toc {
        push_node(node, 1, &mut rows);
    }
    rows
}

fn push_node(node: &TocNode, level: u8, rows: &mut Vec<Row>) {
    rows.push(Row::Header { id: node.id.clone(), label: node.label.clone(), level });
    for key in &node.settings {
        rows.push(Row::Setting { id: format!("{}:{key}", node.id), key: key.clone() });
    }
    for child in &node.children {
        push_node(child, level + 1, rows);
    }
}

/// Rows for search results: no headers, in rank order.
pub fn search_rows<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<Row> {
    keys.into_iter()
        .map(|key| Row::Setting { id: format!("search:{key}"), key: key.to_string() })
        .collect()
}

/// Row index of each group header, by TOC node id.
pub fn header_indices(rows: &[Row]) -> HashMap<String, usize> {
    rows.iter()
        .enumerate()
        .filter_map(|(i, r)| match r {
            Row::Header { id, .. } => Some((id.clone(), i)),
            Row::Setting { .. } => None,
        })
        .collect()
}

/// The TOC node whose group contains row `ix` (the nearest header at or above it).
pub fn group_at(rows: &[Row], ix: usize) -> Option<&str> {
    rows[..(ix + 1).min(rows.len())].iter().rev().find_map(|r| match r {
        Row::Header { id, .. } => Some(id.as_str()),
        Row::Setting { .. } => None,
    })
}

/// A visible line of the left-hand table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub id: String,
    pub label: String,
    pub depth: usize,
    pub expandable: bool,
    pub expanded: bool,
    /// Matching settings under this node while searching (`None` in the normal TOC).
    pub count: Option<usize>,
}

/// The TOC lines that are visible given which nodes are expanded.
pub fn flatten_toc(toc: &[TocNode], expanded: &HashSet<String>) -> Vec<TocEntry> {
    fn go(nodes: &[TocNode], depth: usize, expanded: &HashSet<String>, out: &mut Vec<TocEntry>) {
        for n in nodes {
            let is_open = expanded.contains(&n.id);
            out.push(TocEntry {
                id: n.id.clone(),
                label: n.label.clone(),
                depth,
                expandable: !n.children.is_empty(),
                expanded: is_open,
                count: None,
            });
            if is_open {
                go(&n.children, depth + 1, expanded, out);
            }
        }
    }
    let mut out = Vec::new();
    go(toc, 0, expanded, &mut out);
    out
}

/// The TOC while searching (upstream `TOCTreeModel` + `SettingsTreeFilter` with `count`): a node
/// shows when its subtree holds at least one of `matches`, with the count of matches in it (its own
/// settings plus its children's counts). A key listed in two nodes counts in both, as upstream does.
/// Every visible node is expanded (upstream calls `expandAll` on each search), and order is kept.
pub fn flatten_search_toc(toc: &[TocNode], matches: &HashSet<&str>) -> Vec<TocEntry> {
    fn count(node: &TocNode, matches: &HashSet<&str>) -> usize {
        node.settings.iter().filter(|k| matches.contains(k.as_str())).count()
            + node.children.iter().map(|c| count(c, matches)).sum::<usize>()
    }
    fn go(nodes: &[TocNode], depth: usize, matches: &HashSet<&str>, out: &mut Vec<TocEntry>) {
        for n in nodes {
            let n_count = count(n, matches);
            if n_count == 0 {
                continue;
            }
            let expandable = n.children.iter().any(|c| count(c, matches) > 0);
            out.push(TocEntry {
                id: n.id.clone(),
                label: n.label.clone(),
                depth,
                expandable,
                expanded: true,
                count: Some(n_count),
            });
            go(&n.children, depth + 1, matches, out);
        }
    }
    let mut out = Vec::new();
    go(toc, 0, matches, &mut out);
    out
}

/// Every setting key under the TOC node `id`, its own and its descendants' (upstream
/// `settingContainedInGroup`). Empty if the node is unknown.
pub fn subtree_keys(toc: &[TocNode], id: &str) -> HashSet<String> {
    fn collect(node: &TocNode, out: &mut HashSet<String>) {
        out.extend(node.settings.iter().cloned());
        for c in &node.children {
            collect(c, out);
        }
    }
    fn find<'a>(nodes: &'a [TocNode], id: &str) -> Option<&'a TocNode> {
        nodes.iter().find_map(|n| if n.id == id { Some(n) } else { find(&n.children, id) })
    }
    let mut out = HashSet::new();
    if let Some(node) = find(toc, id) {
        collect(node, &mut out);
    }
    out
}

/// Ids of the ancestors of `id` (outermost first), empty if `id` is top-level or unknown.
pub fn ancestors(toc: &[TocNode], id: &str) -> Vec<String> {
    fn go(nodes: &[TocNode], id: &str, path: &mut Vec<String>) -> bool {
        for n in nodes {
            if n.id == id {
                return true;
            }
            path.push(n.id.clone());
            if go(&n.children, id, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    if go(toc, id, &mut path) { path } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, settings: &[&str], children: Vec<TocNode>) -> TocNode {
        TocNode {
            id: id.into(),
            label: id.to_uppercase(),
            children,
            settings: settings.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn sample() -> Vec<TocNode> {
        vec![
            node("common", &["a.x"], vec![]),
            node("editor", &["editor.a"], vec![node("font", &["editor.fontSize", "editor.fontFamily"], vec![])]),
        ]
    }

    #[test]
    fn rows_are_header_settings_then_children() {
        let rows = build_rows(&sample());
        let kinds: Vec<String> = rows
            .iter()
            .map(|r| match r {
                Row::Header { id, level, .. } => format!("H{level}:{id}"),
                Row::Setting { id, .. } => id.clone(),
            })
            .collect();
        assert_eq!(
            kinds,
            ["H1:common", "common:a.x", "H1:editor", "editor:editor.a", "H2:font", "font:editor.fontSize", "font:editor.fontFamily"]
        );
    }

    #[test]
    fn repeated_settings_get_distinct_row_ids() {
        let toc = vec![node("common", &["editor.a"], vec![]), node("editor", &["editor.a"], vec![])];
        let ids: Vec<_> = build_rows(&toc)
            .into_iter()
            .filter_map(|r| if let Row::Setting { id, .. } = r { Some(id) } else { None })
            .collect();
        assert_eq!(ids, ["common:editor.a", "editor:editor.a"]);
    }

    #[test]
    fn header_indices_and_group_at() {
        let rows = build_rows(&sample());
        let idx = header_indices(&rows);
        assert_eq!(idx["editor"], 2);
        assert_eq!(idx["font"], 4);
        assert_eq!(group_at(&rows, 3), Some("editor"));
        assert_eq!(group_at(&rows, 6), Some("font"));
        assert_eq!(group_at(&rows, 0), Some("common"));
    }

    #[test]
    fn search_rows_have_no_headers() {
        let rows = search_rows(["editor.fontSize", "editor.tabSize"]);
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[0], Row::Setting { key, .. } if key == "editor.fontSize"));
    }

    #[test]
    fn toc_flattening_honours_expansion() {
        let toc = sample();
        let collapsed = flatten_toc(&toc, &HashSet::new());
        assert_eq!(collapsed.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["common", "editor"]);
        assert!(collapsed[1].expandable && !collapsed[1].expanded);
        let open = flatten_toc(&toc, &HashSet::from(["editor".to_string()]));
        assert_eq!(
            open.iter().map(|e| (e.id.as_str(), e.depth)).collect::<Vec<_>>(),
            [("common", 0), ("editor", 0), ("font", 1)]
        );
    }

    #[test]
    fn ancestors_of_nested_node() {
        let toc = sample();
        assert_eq!(ancestors(&toc, "font"), ["editor"]);
        assert!(ancestors(&toc, "editor").is_empty());
        assert!(ancestors(&toc, "nope").is_empty());
    }

    #[test]
    fn search_toc_keeps_only_nodes_with_matches_and_rolls_counts_up() {
        let toc = sample();
        let matches: HashSet<&str> = ["editor.fontSize", "editor.cursor", "a.x"].into_iter().collect();
        let entries = flatten_search_toc(&toc, &matches);
        assert_eq!(
            entries.iter().map(|e| (e.id.as_str(), e.depth, e.count)).collect::<Vec<_>>(),
            [("common", 0, Some(1)), ("editor", 0, Some(1)), ("font", 1, Some(1))]
        );
    }

    #[test]
    fn search_toc_parent_counts_its_own_matches_and_all_children() {
        let toc = vec![node("editor", &["editor.a", "editor.b"], vec![node("font", &["editor.fontSize"], vec![])])];
        let matches: HashSet<&str> = ["editor.a", "editor.fontSize"].into_iter().collect();
        let entries = flatten_search_toc(&toc, &matches);
        assert_eq!(entries.iter().map(|e| (e.id.as_str(), e.count)).collect::<Vec<_>>(), [("editor", Some(2)), ("font", Some(1))]);
        assert!(entries[0].expandable && entries[0].expanded);
    }

    #[test]
    fn search_toc_keeps_order_and_repeated_keys_count_in_each_node() {
        let toc = vec![
            node("common", &["editor.a"], vec![]),
            node("editor", &["editor.a", "editor.b"], vec![node("font", &["editor.b"], vec![])]),
        ];
        let matches: HashSet<&str> = ["editor.a", "editor.b"].into_iter().collect();
        let entries = flatten_search_toc(&toc, &matches);
        assert_eq!(
            entries.iter().map(|e| (e.id.as_str(), e.depth, e.count)).collect::<Vec<_>>(),
            [("common", 0, Some(1)), ("editor", 0, Some(3)), ("font", 1, Some(1))]
        );
    }

    #[test]
    fn search_toc_is_empty_without_matches_and_has_no_twisty_for_hidden_children() {
        assert!(flatten_search_toc(&sample(), &HashSet::new()).is_empty());
        let matches: HashSet<&str> = ["editor.a"].into_iter().collect();
        let entries = flatten_search_toc(&sample(), &matches);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "editor");
        assert!(!entries[0].expandable);
    }

    #[test]
    fn subtree_keys_include_descendants() {
        let toc = sample();
        let keys = subtree_keys(&toc, "editor");
        assert_eq!(keys, HashSet::from(["editor.a".to_string(), "editor.fontSize".to_string(), "editor.fontFamily".to_string()]));
        assert_eq!(subtree_keys(&toc, "font"), HashSet::from(["editor.fontSize".to_string(), "editor.fontFamily".to_string()]));
        assert!(subtree_keys(&toc, "nope").is_empty());
    }

    #[test]
    fn real_registry_search_toc_lists_commonly_used_only_when_it_matches() {
        let reg = vsr_settings::SettingsRegistry::load_default();
        let font: HashSet<&str> = reg.search("font size").into_iter().map(|s| s.key.as_str()).collect();
        let entries = flatten_search_toc(reg.toc(), &font);
        assert!(entries.iter().all(|e| e.count.is_some_and(|c| c > 0)));
        let common = reg.commonly_used().iter().filter(|s| font.contains(s.key.as_str())).count();
        let shown = entries.iter().find(|e| e.id == "commonlyUsed").and_then(|e| e.count).unwrap_or(0);
        assert_eq!(shown, common);
    }

    #[test]
    fn real_registry_toc_builds_rows() {
        let reg = vsr_settings::SettingsRegistry::load_default();
        let rows = build_rows(reg.toc());
        assert!(matches!(&rows[0], Row::Header { id, level: 1, .. } if id == "commonlyUsed"));
        assert!(rows.len() > 1000);
    }
}
