//! The settings that actually change something in rscode: the single source of truth for
//! "wired" settings. The Settings editor marks every other setting "Not yet supported in
//! rscode", and `docs/settings-coverage.md` is generated from this list.
//!
//! Add a key here in the same commit that makes the app read it.

/// Keys read by the app, grouped by where they take effect. Keep in the order of the doc.
pub const WIRED_KEYS: &[(&str, &[&str])] = &[
    (
        "Workbench layout",
        &[
            "workbench.sideBar.location",
            "workbench.activityBar.location",
            "workbench.statusBar.visible",
            "workbench.editor.showTabs",
            "workbench.tree.indent",
        ],
    ),
    ("Explorer", &["files.exclude"]),
    (
        "Editor text",
        &[
            "editor.fontSize",
            "editor.fontFamily",
            "editor.fontWeight",
            "editor.lineHeight",
            "editor.tabSize",
            "editor.insertSpaces",
        ],
    ),
    (
        "Editor rendering",
        &[
            "editor.lineNumbers",
            "editor.renderLineHighlight",
            "editor.cursorStyle",
            "editor.cursorBlinking",
            "editor.cursorWidth",
            "editor.renderWhitespace",
            "editor.rulers",
            "editor.guides.indentation",
            "editor.guides.highlightActiveIndentation",
            "editor.matchBrackets",
            "editor.selectionHighlight",
            "editor.selectionHighlightMaxLength",
            "editor.occurrencesHighlight",
            "editor.roundedSelection",
            "editor.renderLineHighlightOnlyWhenFocus",
            "editor.fontLigatures",
        ],
    ),
    ("Editor scrolling", &["editor.scrollBeyondLastLine"]),
    (
        "Word wrap",
        &[
            "editor.wordWrap",
            "editor.wordWrapColumn",
            "editor.wrappingIndent",
            "editor.wrappingStrategy",
            "editor.wordWrapIndicator",
            "editor.wordBreak",
            "editor.wordSeparators",
        ],
    ),
    (
        "Find",
        &[
            "editor.find.seedSearchStringFromSelection",
            "editor.find.autoFindInSelection",
            "editor.find.loop",
            "editor.find.cursorMoveOnType",
            "editor.find.findOnType",
            "editor.find.closeOnResult",
        ],
    ),
    (
        "Minimap",
        &[
            "editor.minimap.enabled",
            "editor.minimap.side",
            "editor.minimap.size",
            "editor.minimap.showSlider",
            "editor.minimap.autohide",
            "editor.minimap.renderCharacters",
            "editor.minimap.maxColumn",
            "editor.minimap.scale",
        ],
    ),
    ("Saving", &["files.insertFinalNewline", "files.trimTrailingWhitespace"]),
    (
        "Search",
        &[
            "search.exclude",
            "search.useIgnoreFiles",
            "search.useGlobalIgnoreFiles",
            "search.useParentIgnoreFiles",
            "search.followSymlinks",
            "search.smartCase",
            "search.maxResults",
            "search.searchOnType",
            "search.searchOnTypeDebouncePeriod",
            "search.collapseResults",
            "search.showLineNumbers",
        ],
    ),
    (
        "Quick Open",
        &[
            "workbench.quickOpen.closeOnFocusLost",
            "workbench.quickOpen.preserveInput",
            "search.quickOpen.includeHistory",
            "search.quickOpen.history.filterSortOrder",
        ],
    ),
    (
        "Command Palette",
        &["workbench.commandPalette.history", "workbench.commandPalette.preserveInput"],
    ),
    (
        "Editor management",
        &[
            "workbench.editor.enablePreview",
            "workbench.editor.enablePreviewFromQuickOpen",
            "workbench.editor.enablePreviewFromCodeNavigation",
            "workbench.editor.openPositioning",
            "workbench.editor.focusRecentEditorAfterClose",
            "workbench.editor.closeOnFileDelete",
            "workbench.editor.limit.enabled",
            "workbench.editor.limit.value",
            "workbench.editor.limit.excludeDirty",
            "workbench.editor.highlightModifiedTabs",
            "workbench.editor.labelFormat",
            "workbench.editor.scrollToSwitchTabs",
            "workbench.editor.empty.hint",
        ],
    ),
];

pub fn all_keys() -> impl Iterator<Item = &'static str> {
    WIRED_KEYS.iter().flat_map(|(_, keys)| keys.iter().copied())
}

pub fn is_wired(key: &str) -> bool {
    all_keys().any(|k| k == key)
}

#[cfg(test)]
pub fn count() -> usize {
    all_keys().count()
}

/// The text of `docs/settings-coverage.md`.
#[cfg(test)]
pub fn coverage_markdown(registry: &vsr_settings::SettingsRegistry) -> String {
    let total = registry.len();
    let wired = count();
    let mut out = String::new();
    out.push_str("# Settings coverage\n\n");
    out.push_str("Generated by the test `config::wired::tests::coverage_doc_is_up_to_date` from\n");
    out.push_str("`crates/vsr-app/src/config/wired.rs`. To refresh it after changing the list, run\n");
    out.push_str("`RSCODE_UPDATE_DOCS=1 cargo test -p vsr-app coverage_doc`.\n\n");
    out.push_str(&format!(
        "**{wired} of {total} registered settings are wired** ({:.1}%). The Settings editor shows\n\"Not yet supported in rscode\" under every other setting.\n\n",
        wired as f64 * 100.0 / total as f64
    ));
    for (group, keys) in WIRED_KEYS {
        out.push_str(&format!("## {group}\n\n| Setting | Default |\n|---|---|\n"));
        for key in *keys {
            let default = registry.get(key).map_or_else(String::new, |s| s.default.to_string());
            out.push_str(&format!("| `{key}` | `{default}` |\n"));
        }
        out.push('\n');
    }
    out.push_str("## Known differences from VS Code\n\n");
    out.push_str("- Out-of-range numbers are clamped to the schema's minimum and maximum (as `editorOptions.ts` does); a value of the wrong type or outside an enum falls back to the default.\n");
    out.push_str("- `editor.cursorBlinking`: `smooth`, `phase` and `expand` blink like `blink`; `solid` does not blink.\n");
    out.push_str("- `editor.lineNumbers: interval` shows a number on every 10th line and on the caret line.\n");
    out.push_str("- `files.exclude` merges with the default (object settings deep-merge, as `mergeContents`) and supports `true`/`false` and `when` with `$(basename)`; patterns are matched against the path relative to the opened folder.\n");
    out.push_str("- `workbench.activityBar.location: top` and `bottom` show the view icons as a horizontal strip in the side bar (no title bar integration).\n");
    out.push_str("- Find widget: regular expressions use Rust `regex` syntax (no look-around or backreferences, as in the Search view); Whole Word uses `\\b` rather than `editor.wordSeparators`; there is one cursor, so no \"Select All Occurrences of Find Match\". `editor.find.addExtraSpaceOnTop`, `globalFindClipboard`, `history` and `replaceHistory` are not implemented.\n");
    out.push_str("- `editor.detectIndentation` is not implemented: `editor.tabSize` and `editor.insertSpaces` always apply.\n");
    out.push_str("- Search reads `files.exclude` too (it is listed under Explorer). Entries of `search.exclude` and `files.exclude` whose value is an object (`when` clauses) are ignored by the search.\n");
    out.push_str("- `workbench.editor.enablePreviewFromCodeNavigation` is read, but nothing opens from code navigation yet, so it has no effect.\n");
    out.push_str("- Editor groups: rscode has one group. `limit.*` counts the whole group, `revealIfOpen` and `focusRecentEditorAfterClose` apply to that group's tabs. `limit.perEditorGroup` is not read.\n");
    out.push_str("- `workbench.editor.labelFormat: long` shows the directory's absolute path; `default` adds the directory (relative to the folder) only to tabs that share a name.\n");
    out.push_str("- `workbench.editor.highlightModifiedTabs` always uses the focused-group colours (`tab.activeModifiedBorder`, `tab.inactiveModifiedBorder`).\n");
    out.push_str("- `workbench.editor.closeOnFileDelete` checks that files exist once a second (no file watcher yet). Editors with unsaved changes stay open.\n");
    out.push_str("- Word wrap: `editor.wrappingStrategy: advanced` breaks like `simple` (the monospace computer; the advanced strategy measures text in the browser). `editor.wordWrapIndicator` draws a return arrow after the last column of a row that continues. Alt+Z wraps at the viewport width or turns wrapping off for that editor only (it does not write the setting). `editor.wordSeparators` drives word moves, double-click and the whole-word check of selection highlights; the Find widget still uses `\\b`, and the word at the caret for occurrence highlights uses the default separators (upstream: the language's word definition). `editor.wordSegmenterLocales` (CJK word segmentation) is not implemented.\n");
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use vsr_settings::SettingsRegistry;

    use super::*;

    #[test]
    fn every_wired_key_is_registered_once() {
        let registry = SettingsRegistry::load_default();
        let mut seen = HashSet::new();
        for key in all_keys() {
            assert!(registry.get(key).is_some(), "{key} is not a registered setting");
            assert!(seen.insert(key), "{key} is listed twice");
        }
        assert_eq!(seen.len(), count());
        assert!(is_wired("editor.fontSize") && !is_wired("editor.minimap.showMarkSectionHeaders"));
    }

    #[test]
    fn coverage_doc_is_up_to_date() {
        let expected = coverage_markdown(&SettingsRegistry::load_default());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/settings-coverage.md");
        if std::env::var_os("RSCODE_UPDATE_DOCS").is_some() {
            std::fs::write(&path, &expected).unwrap();
            return;
        }
        let actual = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(actual, expected, "docs/settings-coverage.md is stale: run RSCODE_UPDATE_DOCS=1 cargo test -p vsr-app coverage_doc");
    }
}
