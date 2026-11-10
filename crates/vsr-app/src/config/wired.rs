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
    (
        "Window",
        &[
            "window.title",
            "window.titleSeparator",
            "window.commandCenter",
            "window.density.editorTabHeight",
            "window.zoomLevel",
        ],
    ),
    ("Welcome", &["workbench.startupEditor"]),
    (
        "Session",
        &["window.restoreWindows", "workbench.editor.restoreEditors", "workbench.editor.restoreViewState", "files.hotExit"],
    ),
    (
        "Themes",
        &[
            "workbench.colorTheme",
            "workbench.preferredDarkColorTheme",
            "workbench.preferredLightColorTheme",
            "window.autoDetectColorScheme",
            "workbench.colorCustomizations",
            "editor.tokenColorCustomizations",
        ],
    ),
    (
        "Explorer",
        &[
            "files.exclude",
            "files.enableTrash",
            "explorer.confirmDelete",
            "explorer.sortOrder",
            "explorer.compactFolders",
            "explorer.autoReveal",
            "explorer.excludeGitIgnore",
            "explorer.copyPathSeparator",
            "explorer.copyRelativePathSeparator",
        ],
    ),
    (
        "Editor text",
        &[
            "editor.fontSize",
            "editor.fontFamily",
            "editor.fontWeight",
            "editor.lineHeight",
            "editor.tabSize",
            "editor.insertSpaces",
            "editor.tabFocusMode",
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
            "editor.cursorHeight",
            "editor.cursorSurroundingLines",
            "editor.cursorSurroundingLinesStyle",
            "editor.renderWhitespace",
            "editor.renderControlCharacters",
            "editor.renderFinalNewline",
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
            "editor.find.globalFindClipboard",
            "editor.find.history",
            "editor.find.replaceHistory",
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
            "editor.minimap.showRegionSectionHeaders",
            "editor.minimap.showMarkSectionHeaders",
            "editor.minimap.markSectionHeaderRegex",
            "editor.minimap.sectionHeaderFontSize",
        ],
    ),
    (
        "Folding",
        &[
            "editor.folding",
            "editor.foldingStrategy",
            "editor.foldingHighlight",
            "editor.foldingMaximumRegions",
            "editor.showFoldingControls",
            "editor.unfoldOnClickAfterEndOfLine",
        ],
    ),
    (
        "Sticky scroll",
        &[
            "editor.stickyScroll.enabled",
            "editor.stickyScroll.maxLineCount",
            "editor.stickyScroll.defaultModel",
            "editor.stickyScroll.scrollWithEditor",
        ],
    ),
    ("Saving", &["files.insertFinalNewline", "files.trimTrailingWhitespace"]),
    (
        "Languages",
        &["files.associations", "editor.maxTokenizationLineLength", "files.trimTrailingWhitespaceInRegexAndStrings"],
    ),
    (
        "Files",
        &[
            "files.trimFinalNewlines",
            "files.eol",
            "files.autoSave",
            "files.autoSaveDelay",
            "files.autoSaveWorkspaceFilesOnly",
            "files.readonlyInclude",
            "files.readonlyExclude",
            "files.readonlyFromPermissions",
            "files.watcherExclude",
            "files.saveConflictResolution",
        ],
    ),
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
            "search.ripgrep.maxThreads",
            "search.sortOrder",
            "search.seedOnFocus",
            "search.seedWithNearestWord",
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
    out.push_str("- `workbench.startupEditor` is read once, at startup (as upstream), and only when no editor was restored. `welcomePage` (the default) opens the Welcome page (ADR-0040); `readme` opens the folder's README. `welcomePageInEmptyWorkbench` (rscode always has a folder), `newUntitledFile` (no untitled files), `terminal` and `agentSessionsWelcomePage` open nothing. The Welcome page's \"Show welcome page on startup\" checkbox writes this setting.\n");
    out.push_str("- Session restore (ADR-0040): the tabs of a folder (order, active, preview), their scroll position and selection, the side bar and the Search query come back when the same folder is opened again. `window.restoreWindows: none` and `workbench.editor.restoreEditors: false` start with nothing restored; with one window, `preserve`, `all`, `folders` and `one` all restore. `workbench.editor.restoreViewState` also restores a file's scroll and selection when it is reopened after being closed in the same run.\n");
    out.push_str("- `files.hotExit` (ADR-0040): `onExit` (default) keeps unsaved text in backups when quitting (Cmd+Q) and restores it, dirty, on the next start; closing the window asks (Save All, Don't Save, Cancel). `onExitAndWindowClose` never asks. `off` always asks. Unsaved text is also backed up one second after the last edit (crash safety) and always restored, whatever the restore settings say. `files.restoreUndoStack` is not implemented: undo goes back to the file on disk.\n");
    out.push_str("- `window.zoomLevel` scales the editor text only (1.2 per level, as upstream). The title bar, side bars and tabs keep their pixel sizes until the UI has a whole-window scale (ADR-0035).\n");
    out.push_str("- `editor.cursorBlinking`: `smooth`, `phase` and `expand` blink like `blink`; `solid` does not blink.\n");
    out.push_str("- `editor.lineNumbers: interval` shows a number on every 10th line and on the caret line.\n");
    out.push_str("- `files.exclude` merges with the default (object settings deep-merge, as `mergeContents`) and supports `true`/`false` and `when` with `$(basename)`; patterns are matched against the path relative to the opened folder.\n");
    out.push_str("- `workbench.activityBar.location: top` and `bottom` show the view icons as a horizontal strip in the side bar (no title bar integration).\n");
    out.push_str("- Find widget: regular expressions use Rust `regex` syntax (no look-around or backreferences, as in the Search view); Whole Word uses `\\b` rather than `editor.wordSeparators`; there is one cursor, so no \"Select All Occurrences of Find Match\". `editor.find.addExtraSpaceOnTop` is not implemented (see `upstream/settings.toml`).\n");
    out.push_str("- `editor.detectIndentation` is not implemented: `editor.tabSize` and `editor.insertSpaces` always apply.\n");
    out.push_str("- Search reads `files.exclude` too (it is listed under Explorer). Entries of `search.exclude` and `files.exclude` whose value is an object (`when` clauses) are ignored by the search.\n");
    out.push_str("- `workbench.editor.enablePreviewFromCodeNavigation` is read, but nothing opens from code navigation yet, so it has no effect.\n");
    out.push_str("- `search.sortOrder` orders the files of the current results and places the files of later searches by the same order. `modified` reads each file's modification time when it is found.\n");
    out.push_str("- `search.seedOnFocus` and `search.seedWithNearestWord` apply when the Search view is focused with Find in Files (Shift+Cmd+F). Other ways of showing the view do not seed the query.\n");
    out.push_str("- `search.ripgrep.maxThreads` sets the walker threads of a search (0 is automatic).\n");
    out.push_str("- `explorer.excludeGitIgnore` reads the `.gitignore` files from the workspace root down to each listed folder. A change to a `.gitignore` shows when its folder is listed again (there is no watch on `.gitignore` files yet). The global gitignore and `.git/info/exclude` are not read.\n");
    out.push_str("- `explorer.copyPathSeparator` and `explorer.copyRelativePathSeparator` apply to Copy Path (Alt+Cmd+C) and Copy Relative Path (Shift+Alt+Cmd+C), which copy the selected entry or else the active editor's file. The Explorer has no context menu yet, so the two are Command Palette entries and keys.\n");
    out.push_str("- Editor groups: rscode has one group. `limit.*` counts the whole group, `revealIfOpen` and `focusRecentEditorAfterClose` apply to that group's tabs. `limit.perEditorGroup` is not read.\n");
    out.push_str("- `explorer.sortOrder: foldersNestsFiles` sorts like `default`: rscode has no file nesting (`explorer.fileNesting.*`), so nothing is nested. Names sort case-insensitively with numbers in order (`file2` before `file10`).\n");
    out.push_str("- `explorer.autoReveal` runs when an editor opens. Switching tabs does not reveal the file yet. The scroll is a request to the tree's scroll handle; the headless tests cannot observe it.\n");
    out.push_str("- `explorer.confirmDelete` covers Delete (Trash, or permanent when `files.enableTrash` is off) and Delete Permanently. Dirty editors are not asked about: they stay open with their text.\n");
    out.push_str("- `workbench.editor.labelFormat: long` shows the directory's absolute path; `default` adds the directory (relative to the folder) only to tabs that share a name.\n");
    out.push_str("- `workbench.editor.highlightModifiedTabs` always uses the focused-group colours (`tab.activeModifiedBorder`, `tab.inactiveModifiedBorder`).\n");
    out.push_str("- `workbench.editor.closeOnFileDelete` checks that files exist once a second (no file watcher yet). Editors with unsaved changes stay open.\n");
    out.push_str("- `files.eol` applies to a text that has no line break yet (an opened one-line file). A file with line breaks keeps its own. There are no new or untitled files in rscode yet, so the setting does not reach them.\n");
    out.push_str("- `files.autoSave: onFocusChange` saves when the active editor changes and when the window loses focus. Focus moving between the editor and other views does not save (no focus tracking of the editor surface yet). `autoSaveWhenNoErrors` is not wired (no diagnostics).\n");
    out.push_str("- `files.readonly*` and the toggle `workbench.action.files.toggleActiveEditorReadonlyInSession` refuse edits in the editor, in Find/Replace and in Search's Replace. A read-only tab shows a lock. Saving a read-only file is still allowed.\n");
    out.push_str("- Languages and syntax highlighting (ADR-0038): the language of a file comes from `files.associations`, then the built-in language contributions (file names, extensions, first line), else Plain Text. The bundled TextMate grammars (Rust, JavaScript/TypeScript (+ JSX/TSX), JSON/JSONC, Markdown, Python, Shell, CSS, HTML, YAML, XML/XSL, C/C++/CUDA, Go, Java) colour tokens with the active theme's token colors; other languages show plain. `files.defaultLanguage` stays unwired: it only applies to untitled files, which rscode does not have. `editor.maxTokenizationLineLength` re-tokenizes live. `files.trimTrailingWhitespaceInRegexAndStrings: false` keeps trailing whitespace inside strings and regular expressions, and in lines the tokenizer has not reached yet.\n");
    out.push_str("- Word wrap: `editor.wrappingStrategy: advanced` breaks like `simple` (the monospace computer; the advanced strategy measures text in the browser). `editor.wordWrapIndicator` draws a return arrow after the last column of a row that continues. Alt+Z wraps at the viewport width or turns wrapping off for that editor only (it does not write the setting). `editor.wordSeparators` drives word moves, double-click and the whole-word check of selection highlights; the Find widget still uses `\\b`, and the word at the caret for occurrence highlights uses the default separators (upstream: the language's word definition). `editor.wordSegmenterLocales` (CJK word segmentation) is not implemented.\n");
    out.push_str("- Themes (ADR-0037): the ten built-in themes of VS Code (Dark Modern, Light Modern, Dark+, Light+, Dark (Visual Studio), Light (Visual Studio), Dark High Contrast, Light High Contrast, Dark 2026, Light 2026) are bundled; other names in the `workbench.colorTheme` dropdown (Abyss, Monokai, ...) belong to extensions rscode has not got and fall back to the default theme of that colour mode. The default theme is Dark Modern, not Dark 2026 as upstream (the Settings editor still shows the upstream default text for the `workbench.*ColorTheme` settings).\n");
    out.push_str("- `workbench.preferred*ColorTheme` apply with `window.autoDetectColorScheme`, which follows the window appearance (macOS light/dark). `window.autoDetectHighContrast` and `workbench.preferredHighContrast*ColorTheme` are not wired: there is no OS high contrast signal in gpui. `workbench.colorCustomizations` supports global keys and `[Theme Name]` blocks (with `*` wildcards) and the value `default`; `editor.tokenColorCustomizations` (the simple keys and `textMateRules`, global and `[Theme Name]`) apply to the syntax highlighter's token colours (ADR-0038). `editor.semanticTokenColorCustomizations` is read by the theme service but changes nothing on screen (there are no semantic tokens yet). Icon themes stay unsupported (no file icons).\n");
    out.push_str("- Caret (ADR-0043): `editor.cursorHeight` centres the line caret (`line` and `line-thin` only). `editor.cursorSurroundingLines` keeps the margin when the caret scrolls into view: the top margin is the setting, the bottom one is at least one row (upstream's \"trailing lines (minimum 1)\"), each capped at half the page. With `cursorSurroundingLinesStyle: default` a click does not enforce it. `cursorSmoothCaretAnimation` is not implemented (the caret jumps).\n");
    out.push_str("- Rendering (ADR-0043): `editor.renderControlCharacters` draws the C0 controls (but tab) and DEL as Control Pictures (U+2400 block) over a cell; the bidi formatting characters are not drawn. `editor.renderFinalNewline: dimmed` dims the number at half opacity.\n");
    out.push_str("- `editor.tabFocusMode`: Tab moves focus instead of typing a tab, but rscode has no focus tab stops outside the editor yet, so the focus stays in the editor. The Ctrl+Shift+M toggle is not implemented.\n");
    out.push_str("- Not wired (blocked, see `upstream/settings.toml`): `mouseWheelZoom` and `mouseWheelScrollSensitivity` need a scroll capture on the editor surface; `guides.bracketPairs`, `bracketPairsHorizontal` and `highlightActiveBracketPair` need a whole-document bracket pair model; `fontVariations` needs font variation axes; `mouseMiddleClickAction` needs ctrl+click and links; `editor.format*` need formatting providers.\n");
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
        assert!(is_wired("editor.fontSize") && !is_wired("editor.minimap.sectionHeaderLetterSpacing"));
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
