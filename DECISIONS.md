# Decisions log

ADR-style, newest at the bottom. Each entry: context, decision, consequences.
Research backing these lives in `docs/research/`.

---

## ADR-0001 — Native Rust UI, crates split for a future webview workbench (2026-11-09)

**Context.** Two viable routes: (a) Hybrid: a Rust shell (Tauri/wry) hosting VS Code's own
TypeScript workbench in a webview, with Rust replacing Electron main + Node services; (b) a fully
native Rust UI (Zed/Lapce style). (a) makes upstream sync and extension compat easy; (b) is the
"real" Rust IDE but turns upstream sync into porting work.

**Decision (user's choice).** Native UI now, *designed for both*: keep hard crate boundaries
between text/editor core, platform services, workbench UI shell, and a future extension host,
so a webview-hosted workbench could later replace or sit next to the native shell.

**Consequences.** Upstream sync = tracked porting (ADR-0003). Extension compat needs a Node
extension host speaking VS Code's RPC protocol to a Rust "main thread" side (later milestone).

## ADR-0002 — Repository layout (2026-11-09)

- `../vscode-original` — read-only clone of the fork (sidferreira/vscode). Reference and sync
  source only; never built here (no `pnpm install`), never vendored into this repo.
- `vscode-rust/` (this repo) — cargo workspace, crates under `crates/`, prefix `vsr-`.

## ADR-0003 — Upstream tracking by manifest + header comments (2026-11-09)

Pinned upstream: `50e1350fc78df67b11049b903d96b530f28d26a5` (v1.147.0, 2026-11-09).
Every ported Rust file carries `//! upstream: <path> @ <short-sha>` headers. `upstream/ports.toml`
maps upstream paths to crates with a per-entry synced commit; `scripts/upstream-sync` reports
upstream commits touching ported paths. Details: `UPSTREAM.md`.

## ADR-0004 — Lean debug builds (2026-11-09)

The disk was 100% full at project start. `.cargo/config.toml` uses line-tables-only debug info
for our crates, no debug info + opt-level 1 for dependencies (faster runtime for GPU/text deps,
much smaller `target/`).

## ADR-0005 — Text buffer: VS Code semantics first, piece tree later (2026-11-09)

`vsr-text` exposes VS Code semantics: 1-based lines and columns, **UTF-16** columns and offsets
(what the extension API speaks), EOL detection identical to `PieceTreeTextBufferBuilder._getEOL`
(CRLF only if more than half of breaks are CR-based; lone `\r` is a break), mixed-EOL
normalization. Storage is a simple `Vec<Line>` for now; a faithful piece-tree port can replace
the internals later, validated by porting upstream's `pieceTreeTextBuffer.test.ts`.
Developed TDD: tests in `crates/vsr-text/tests/` written before the implementation.

## ADR-0006 — Crate layout (2026-11-09)

From `docs/research/architecture-map.md`: `vsr-base`, `vsr-text`, `vsr-editor`,
`vsr-editor-view` (native editor rendering, swappable), `vsr-platform` (service traits + DI
container), `vsr-workbench` (UI shell), `vsr-rpc` (ext-host wire protocol, isolated from UI),
`vsr-exthost` (future), `vsr-app` (binary). Rule: editor core never depends on platform/UI.
Crates are created when they get their first real code, not as empty shells.

**Theme for the first window:** Dark Modern colors (`dark_modern.json`), which upstream also
uses as `COLOR_THEME_DARK_INITIAL_COLORS`. Note v1.147 defaults to "Dark 2026" with the
`modernUI` layout (activity bar 36px, tabs 32px); we start with the classic metrics
(48 / 35 / 22) and revisit.

## ADR-0007 — UI framework: egui/eframe 0.37 now, gpui later if possible (2026-11-09)

**Research pick** (`docs/research/ui-framework.md`): gpui via GPUI Kit 0.9.1 (Zed's engine plus
Dock/Tabs/Tree/VirtualList/editor widgets). Verified on crates.io: official `gpui` is stuck at
0.2.2; `gpui-kit` and `gpui-pre` (weekly Zed-main snapshots) are published by `huacnlee`
(longbridge gpui-component maintainer), not by Zed. `gpui-kit` pins `gpui-pre =0.3.12` and
already enables `runtime_shaders`, so no Metal Toolchain is needed.

**Blocker.** The machine's cargo registry goes through a corporate Nexus firewall that
quarantines some crate versions (403 "Requested item is quarantined"): `arc-swap 1.9.4`,
`gpui-pre-ios 0.3.12` (an exact pin, so it cannot be stepped back), `notify-rust 4.19.0`, `uuid 1.29.1`.

**Decision.** Ship the first window on the fallback, **eframe/egui 0.37.0** (plain crates.io
release, wgpu). `scripts/cargo-firewall-fetch` fetches only host-target deps and steps blocked
crates back to the previous allowed version, never routing around the proxy.

**Revisit.** Several blocked crates (`gpui-pre-ios`, `notify-rust`) are probably non-macOS
target deps that `cargo fetch` pulls only because it fetches every platform by default. A
host-only fetch may unblock gpui. Keep UI-free state in `model.rs`-style modules so a framework
switch only rewrites the view layer.

## ADR-0008 — First window scope (2026-11-09)

`vsr-app` (binary `vscode-rust [folder]`): title bar, activity bar (Explorer/Search/SCM/
Extensions, click the active view to toggle the side bar), resizable Explorer tree (folders
first, case-insensitive, `.git` excluded), tabs (open right of active, close/middle-click,
focus moves like VS Code), read-only virtualised text view via `vsr-text` with line numbers,
status bar (line count, encoding, EOL, language). Dark Modern colors, classic metrics. Editing,
cursor and syntax highlighting are next. 52 upstream piece-tree tests pass; 23 are `#[ignore]`d
until `TextBuffer` has raw-offset insert/delete.

## ADR-0009 — Raw offset edits on TextBuffer (2026-11-09)

`TextBuffer::insert_at_offset(offset, text)` and `delete_at_offset(offset, len)` mirror upstream
`PieceTreeBase.insert/delete`: UTF-16 offsets, may split a CRLF, no EOL normalization (upstream
normalizes in `TextModel.validateEdits`), buffer EOL unchanged. All 75 ported piece-tree tests
now run (none ignored). `position_at` is the linear inverse of `offset_at` inside a line
terminator, like upstream `getPositionAt`; callers wanting a clamped position use
`validate_position`.

## ADR-0010 — Codicons for all workbench icons (2026-11-09)

VS Code draws every workbench icon from the Codicons font. We bundle the exact version upstream
pins (`@vscode/codicons` 0.0.46-49, fetched with `npm pack` through the npm proxy) at
`crates/vsr-app/assets/codicon.ttf` (CC BY 4.0, license alongside, credited in README), and use
upstream's codepoints from `codiconsLibrary.ts`. No emoji/unicode stand-ins.

## ADR-0011 — `scripts/code.sh` launcher (2026-11-09)

Mirrors upstream's dev `scripts/code.sh`: builds `vsr-app` if needed, resolves path arguments
against the caller's cwd, launches detached (logs to `$TMPDIR/vscode-rust/last-run.log`);
`--wait` stays attached, `--release` uses the optimised build.

## ADR-0012 — Switch the UI to gpui-kit 0.9.0 (2026-11-09)

**Decision (user's choice).** Replace egui/eframe (ADR-0007's fallback) with gpui via
`gpui-kit = "=0.9.0"` (resolves `gpui-pre 0.3.11`).

**Pins / firewall caveat.** `gpui-kit 0.9.1` is quarantined by the package proxy (403), so we pin
`=0.9.0` and expect to trail upstream by a release. `scripts/cargo-firewall-fetch` plus targeted
`cargo update -p X --precise V` step-backs keep `Cargo.lock` resolvable: uuid 1.29.0, smallvec
1.16.4, flate2 1.1.10, svgtypes 0.16.1, toml 1.2.0, toml_edit 0.25.18, winnow 1.0.4,
toml_datetime 1.1.2, serde_spanned 1.1.2, cc 1.11.0, gpui-component-macros 0.9.0, mime 0.3.17,
notify 8.2.1, arc-swap 1.9.3 (the fetch script alone stops at the toml cascade). Never bypass the
proxy. `runtime_shaders` is enabled, so no Metal Toolchain is required.

**What replaced what.** `vsr-app` depends only on `vsr-text` and `gpui-kit`.
- eframe window/panels -> gpui `div` layout in a transparent-titlebar window (traffic lights kept).
- `theme.rs` is UI-agnostic (`Rgba8`); `ui::to_gpui` converts. Colors/metrics unchanged.
- `icons.rs`: Codicons TTF registered via `text_system().add_fonts`, family `codicon`
  (not gpui-kit's icon set).
- egui `show_rows` text view -> `editor_view.rs` on gpui `uniform_list` (own module so
  `vsr-editor` can later supply an editing model; gpui-kit's editor widget deliberately unused).
- egui hover text -> gpui-component `Tooltip`. The sidebar resize handle is hand-rolled
  (gpui-component's panel group paints its own indicator pill, not Dark Modern).
- `model.rs` untouched (UI-free).

## ADR-0013 — vsr-editor: UI-free editing core (2026-11-09)

Single-cursor `Editor` over `vsr_text::TextBuffer`: moves (char/word/line/page/doc, sticky
visible column, Home toggles first-non-whitespace), selection (anchor/active), typing, Enter
(keeps indentation), Tab to next tab stop, Backspace to previous tab stop in leading whitespace
(upstream `useTabStops` default), Delete, cut/copy/paste, undo/redo with typing coalescing, and
a dirty flag relative to the saved state. Mirrors `src/vs/editor/common/cursor/*`; 40 cases
ported from `cursor.test.ts` / `cursorMoveCommand.test.ts`. Not yet: multi-cursor, onEnter /
auto-indent rules, block indent, word-separator config, grapheme clusters.

## ADR-0014 — Product name: rscode (2026-11-09)

User's call, for fun. User-facing name, binary (`rscode`), window title, launcher
(`scripts/rscode` → `scripts/code.sh`), logs (`$TMPDIR/rscode/`) and user settings dir
(`~/Library/Application Support/rscode/`). Internal crate prefix `vsr-` and the repo/folder
name `vscode-rust` stay for now (renaming them mid-flight would conflict with in-progress
work); revisit later if wanted.

## ADR-0015 — Settings data from the real registry (2026-11-09)

`vsr-settings` holds the full default settings schema (2062 settings: titles, markdown
descriptions, types, enums, defaults, order) and the Settings editor table of contents ported
from `settingsLayout.ts` `tocData`. Data is exported by `scripts/export-default-settings`, which
starts the locally installed VS Code with an isolated throwaway profile and reads
`vscode://schemas/settings/default` (the `--export-default-configuration` flag is parsed but not
implemented upstream). Caveat: the installed app is 1.144.1, not the 1.147.0 pin; re-run the
script after updating VS Code. User settings live at
`~/Library/Application Support/rscode/User/settings.json` (JSONC read; save drops comments for now).

## ADR-0016 — Test porting program and visual policy (2026-11-09)

Upstream has ~58k unit tests (3080 files; browser 29.7k, node 14.6k, common 10.5k), 127 smoke
scenarios in 15 areas, and no pixel tests. Three tiers (details in `docs/testing.md`):
1. **Unit:** ported upstream tests carry `// upstream: <suite> > <test>` refs; `scripts/test-port`
   inventories upstream, reports ported/total per area, suggests the next files (`next`), and lists
   orphans (refs that no longer match upstream, i.e. renamed/deleted or mis-titled tests).
2. **Headless E2E:** scenarios in `tests/e2e-scenarios/<area>.md` (ported from `test/smoke`), run
   via gpui's `TestAppContext` (simulate_keystrokes / simulate_click / dispatch_action /
   run_until_parked). The gpui `test-support` feature is a vsr-app dev-dependency since ADR-0018.
3. **Visual regression:** `Window::render_to_image` (gpui test-support, offscreen on macOS),
   per-channel tolerance 2, 99% match. Baselines are rscode-window-only images in
   `tests/visual/baselines/` (gitignored) and are never committed or pushed without the user's
   approval.

## ADR-0017 — Settings editor UI (2026-11-09)

The Settings editor is a gpui view (`crates/vsr-app/src/settings_view.rs`, pure logic in
`src/settings/{model,markdown,validate}.rs`) over `vsr-settings`. It opens from Cmd+, (an
`OpenSettings` action on the workbench root) or the Manage gear, as a single "Settings" tab.
`Editors` (model.rs) is file-only, so the tab is held by the workbench next to it
(`settings`, `settings_active`) instead of widening that model.

Matches upstream (`settingsEditor2.css`, `settingsTree.ts`): header (14px top, 24px side padding),
30px search box with "N Settings Found", User/Workspace tabs with the header-foreground underline,
TOC (200px, draggable 150-450px sash, bold selected row, chevrons), grouped list with group titles
at 26/22/18px by depth, rows with 12/14/18px padding, bold "Category: " + name, description at
90% opacity, 2px modified bar at left 5px, hover/focused row backgrounds, bool checkbox left of
the description, 420px text / 200px number inputs with validation message, 320px enum select
with the selected value's description, "Edit in settings.json" for array/object/complex,
hover "More Actions" gear (Reset Setting, Copy Setting ID), `@id:key` search, Esc clears.
Colors are Dark Modern values from `dark_modern.json` / `settingsEditorColorRegistry.ts` as new
`Theme` fields (so the modified indicator is `#BB800966`, the theme's value, not blue). Writing
a value equal to the default removes the key, like upstream.

Simplified or missing: markdown is inline-only (code, bold, links, `#id#`), code spans are
monospace but not 11px; links open http(s) only; the enum dropdown is an inline popup, not
a native select; no search suggestions, sticky scroll, keyboard row
navigation, list/object widgets, scrollbar, workspace/folder scopes (Workspace tab shows a note),
language overrides or sync indicators. Group titles use the node label (upstream does too); there
is no "A > B" breadcrumb. Text inputs reuse `gpui-component`'s `Input`, recolored through its global
theme in `settings_view::install`. Settings are saved on every valid change; saving drops comments
in settings.json (ADR-0015). The list is gpui's variable-height `list` (virtualised); TOC selection
follows wheel scrolling but not scrollbar drags.

Testing: first tier-2 headless E2E tests (`src/settings/e2e_tests.rs`) run the workbench in gpui's
test platform; see `docs/testing.md` for the dependency workaround (gpui-kit `test-support` breaks
lock resolution, so `gpui-pre`'s feature is enabled directly).

TOC filters during search (upstream `tocTree.ts`, `settingsEditor2.ts` search and `refreshTOCTree`,
`settingsTreeModels.ts`; the `filter` mode of `settingsSearchTocBehavior`, not `hide`). While a query
is active the TOC lists only nodes whose subtree holds a matching setting, each with its count in
upstream's ` (N)` style (`model::flatten_search_toc`, unit tested). Counts roll up to parents; a key
listed in two groups counts in both; Commonly Used shows like any group, when it has a match. Every
visible node is expanded and its twisty is inert (upstream `expandAll`). Clicking a node filters the
list to the settings under it (upstream `categoryFilter`) and scrolls to the top; it does not scroll to
a group header, since search results have none. The header count stays the total result count. A new
query clears the category. No results hide the TOC behind "No Settings Found". Leaving search (Esc or
empty) collapses the TOC and selects Commonly Used, as upstream does, so the selection from before the
search is not restored. Not implemented: `settingsSearchTocBehavior: hide`.

## ADR-0018 — Editable editor: input model and view (2026-11-09)

`OpenEditor` holds a `vsr_editor::Editor` (tab size 4, insert spaces). Save (Cmd+S) writes
`buffer().value()` (the file's EOL is preserved) and calls `mark_saved()`; dirty is the core's
version-vs-saved check, so undoing back to the saved text is clean again. Closing a dirty tab
just closes it for now (no confirm dialog).

**Input.** Two paths, like VS Code. (1) Commands: one gpui action, `Run(Command)`, bound from a data
table (`keymap.rs`, macOS defaults) in key context `Editor`; the table is also what unit tests
resolve against. Supported: arrows, Shift+arrows, Option+Left/Right (word start / word end, as VS
Code on macOS), Cmd+Left/Right and Home/End (Home toggles first non-whitespace), Cmd+Up/Down, PageUp/Down,
Backspace, Delete (Fn+Backspace), Enter, Tab, Cmd+A/C/X/V/Z/Shift+Z/S/W, copy and cut of the caret line when
nothing is selected. (2) Text: the view registers a gpui `InputHandler` during paint (UTF-16 document
offsets), so typing, dead keys and IME go through the platform text services. Composition is tracked
as a marked range and replaced as a unit.

**View.** The view stays a plain struct owned by the workbench and reaches documents through the
`EditorHost` trait (one source of truth for tabs; no cross-entity ownership). Rows stay virtualised
(`uniform_list`); only visible rows are shaped. Mouse x maps to a column by shaping the clicked line
with gpui's text system (`text_layout::column_for_x` does the tab/UTF-16 math over an injected width
function, unit-tested with a fixed-width stub). Click, Shift+click, drag, double click (word) and
triple click (line) work. Rendering follows Dark Modern: 2px `editorCursor.foreground` line caret
blinking at 500 ms (solid after any activity, hidden when unfocused), `editor.selectionBackground`
(inactive colour when unfocused) with a stub at line breaks, current-line border box only while the
selection is empty, brighter active line number. Dirty tabs show a filled circle (codicon
circle-filled) until the close button is hovered; the title shows `● `; the status bar shows
`Ln X, Col Y (N selected)` and `Spaces: 4` left of encoding/EOL/language.

**Performance.** The widest-line cache is keyed by editor version and costs O(lines) per keystroke
(line lengths only). `vsr-text` got an ASCII fast path for UTF-16 length and `value_in_range` no
longer joins the whole buffer; a 10k-line file measures about 5 ms per keystroke in an unoptimised
build, including layout and paint. Remaining known cost: the core's `offset_at`/`position_at`/`len_utf16`
are O(chars) and run a handful of times per edit (the piece tree of ADR-0005 removes this).

**Core change.** `Editor::move_word_end_right` (upstream `cursorWordRight`, WordEnd) was added because
Option+Right on macOS moves to the end of a word; the existing `move_word_right` is word-start.

**Testing.** First headless E2E suite (gpui `TestAppContext`, see `docs/testing.md`).

**Known gaps.** Multi-cursor; word wrap; syntax colouring; horizontal scroll-to-caret and drag
auto-scroll; word/line delete (Option/Cmd+Backspace), outdent (Shift+Tab), line moves, find; IME
candidate-window placement is approximate and composition edits are separate undo steps; grapheme
clusters; no confirm on closing a dirty tab; Cmd+W/S only act while the editor has focus; other cursor
styles and `cursorSmoothCaretAnimation`.

## ADR-0019 — Platform scope: macOS desktop only; quit on last window (2026-11-09)

User's call. rscode targets **desktop macOS** (Apple Silicon first). No browser/web build, no
web-extension host, no Linux/Windows work for now; code may stay portable where it's free, but
nothing is tested or planned elsewhere. Extension compatibility plans cover the Node (desktop)
extension host only.

App lifecycle: closing the last rscode window quits the app (unlike the macOS default of staying
in the Dock). Cmd+Q and an app menu "Quit rscode" are bound (`install_app_lifecycle` in vsr-app).
Verified by launching, closing the window via the accessibility close button, and observing the
process exit.

## ADR-0020 — Extension compatibility strategy (2026-11-09)

From `docs/research/extensions-compat.md` (scanned the user's 8 installed extensions with
`scripts/ext-api-scan`; installed today: Claude Code, GitHub Pull Requests).

**Approach (Option A):** run upstream's own compiled extension host (`extensionHostProcess`)
under Node, built from a self-built Code-OSS at the pinned commit (MIT), and implement the
`MainThread*` side in Rust over upstream's RPC wire format (`vsr-rpc`, id table generated from
`extHost.protocol.ts`). Not the installed Microsoft VS Code bundle (not MIT). Fallback if the
Code-OSS build is blocked by the proxy: a minimal JS `vscode` shim (Option C) for the first
milestones only. Webviews later via WKWebView (`wry`) embedded in the gpui window (ADR-0001's
escape hatch); macOS only (ADR-0019), so no web extension host.

**Roadmap:** P0 ext-host handshake spike → P1 **ErrorLens** (commands, config, status bar,
decorations, diagnostics; no LSP/webview/native) → P2 oxc (LSP) → TS / XML / Kotlin → GitLens →
**Claude Code** (webviews, terminal + shell integration, native binary) → **PR GitHub** (GitHub
auth, comments, git API, 32 proposed APIs).

## ADR-0021 — Codicons font patched for gpui (2026-11-09)

Bug (user report): after the gpui switch every icon rendered as a missing-glyph box. Root cause:
gpui's macOS loader (`gpui-pre-apple` `load_family`) skips fonts with no 'm' glyph (it measures
text with 'm'; only "Segoe Fluent Icons" is exempt), so the codicon family fell back to the
system font. Fix: `scripts/patch-codicon-font.py` (fontTools) maps U+006D to the existing `blank`
glyph; no outlines change (CC BY 4.0 change notice in `assets/codicon-NOTICE.md`).
Regression test `icons::tests::codicon_loads_in_real_macos_text_system` uses the real
`gpui_apple::AppleTextSystem`, because gpui's `TestAppContext` has a no-op text system that
accepts any font (so headless E2E tests cannot catch font problems).

## ADR-0022 — Configuration service: settings take effect (2026-11-09)

Until now the Settings editor wrote settings.json and nothing read it. `vsr_settings::Configuration`
(UI-free) layers the registered defaults with the user's settings (upstream `Configuration`, default
and user layers; no workspace layer yet). `get_value`/`get_bool`/`get_f64`/`get_str`/`get::<T>`
read it; `update_user` returns a `ConfigurationChange` whose `affects("editor")` is upstream's
`affectsConfiguration` (whole segments: "editor" affects "editor.fontSize", "editor.font" does not;
changed keys are the diff of the raw user documents). Validation is in the model, not in each
consumer as upstream does it, with the same outcomes: wrong type or a value outside an enum falls
back to the default, numbers are clamped to `minimum`/`maximum` like `editorOptions.ts`'s
`clampedInt`, settings with a multi-type schema (`editor.fontWeight`) are validated by their reader.

**App.** `config::AppConfig` is a gpui `Global`: the `Configuration`, the settings path, the last
change and the last file error. Startup loads `default_user_settings_path()`; `RSCODE_USER_DATA_DIR=<dir>`
uses `<dir>/User/settings.json` instead (sandboxed runs, manual tests). Tests install a temp path
(or run in memory) and never touch the real file. The Settings editor writes through
`AppConfig::set_value` (saves the file, updates the configuration, notifies), so a change applies
at once; the workbench observes the global (`cx.observe_global`), rebuilds its typed views
(`config::settings`: `EditorSettings`, `LayoutSettings`, compared with `PartialEq`), pushes
`editor.tabSize`/`insertSpaces` into the open editors (`Editor::set_options`) and re-lists the
explorer when `files.exclude` changed.

**Live reload.** External edits are found by polling the file every 300 ms (no `notify`
dependency, so nothing new passes the package firewall). Debounce: a changed text must be seen on
two consecutive polls. Our own writes record the text they wrote, so the echo is ignored. A file with
a syntax error keeps the previous configuration, shows the error in the Settings editor and refuses
writes until it parses (upstream also refuses to write into an invalid settings file). Settings
saved from the Settings editor still drop comments (ADR-0015).

**Wired settings** (single list: `config/wired.rs`, 19 of 2062; `docs/settings-coverage.md` is
generated from it by a test): `workbench.sideBar.location` (side bar and its sash move to the right
of the editor, the activity bar stays on the side bar's outer edge, as in `layout.ts`),
`workbench.activityBar.location` (`default`, `hidden`; `top`/`bottom` are a horizontal strip in the
side bar column), `workbench.statusBar.visible`, `workbench.editor.showTabs`, `workbench.tree.indent`,
`files.exclude` (merged with the default like upstream, so showing `.git` takes `"**/.git": false`; own glob matcher, `config/glob.rs`; `when: $(basename)` supported), `editor.fontSize`,
`fontFamily` (first installed family), `fontWeight`, `lineHeight` (0 is 1.5 x font size, rounded,
at least 8, values below 8 are multiples, as `fontInfo.ts`), `tabSize`, `insertSpaces`, `lineNumbers`
(`relative`, `interval` included), `renderLineHighlight`, `cursorStyle` (all six), `cursorBlinking`,
`cursorWidth` (0 means 2), `files.insertFinalNewline`, `files.trimTrailingWhitespace` (both applied
on save as one undoable edit each, `vsr-editor/src/save_ops.rs`, in the order of upstream's save
participants). The Settings editor shows "Not yet supported in rscode" under every setting not in
the list.

**Differences from upstream.** `cursorBlinking: smooth|phase|expand` blink like `blink`; no
`editor.detectIndentation`; `files.exclude` patterns match the path relative to the opened folder
only; `window.title` is not wired; with `activityBar.location: top|bottom` hiding the side bar leaves no control to bring it back (no Cmd+B yet); `trimTrailingWhitespace` has no regex/string exemption and the
auto-save cursor-line rule exists in the core but is unused (no auto save).

Testing: `config/e2e.rs` (headless, see `docs/testing.md`) covers the side bar and activity bar
layout through the Settings UI, the status bar, tabs, tab size and insert spaces, live reload,
own-write echo and invalid files, save transforms, `files.exclude` and the "not yet supported" marker.

## ADR-0023 — Search (find in files) on the ripgrep libraries (2026-11-09)

**Engine** (`crates/vsr-search`, UI-free). Upstream spawns the `rg` binary and parses `--json`.
We call the same libraries in-process: `ignore` (parallel `WalkBuilder`; `.gitignore`, `.ignore`,
`.rgignore`, `.git/info/exclude`, global and parent ignore files), `grep-searcher`, `grep-regex`
and `globset`. No rg binary to ship, no JSON hop, cancellation is an `AtomicBool`.

**Option mapping** (upstream `getRgArgs` -> here): `--hidden` -> `hidden(false)`;
`--no-require-git` -> `require_git(false)`; `--ignore-case/--case-sensitive` -> matcher
`case_insensitive` (smart case per `QueryBuilder.isCaseSensitive`: uppercase outside escapes);
`--no-ignore`/`--no-ignore-parent`/`--no-ignore-global` -> `search.useIgnoreFiles/useParentIgnoreFiles/useGlobalIgnoreFiles`;
`--follow` -> `follow_links`; `--max-filesize` -> `max_filesize`; `--crlf` -> matcher `crlf` + CRLF line
terminator; `--multiline` -> `Searcher::multi_line` (automatic when the pattern has `\n`, `\r`, `\W`; `\n` is
rewritten to `\r?\n` by a port of `fixRegexNewline`); `--fixed-strings` -> escaped literal;
whole word -> `createRegExp`'s rule (`\b` only next to a word character); `-g` globs -> our own
`FileFilter` (VS Code glob syntax via `globset` with `literal_separator`, brace expansion ported from
`performBraceExpansionForRipgrep`, `./folder` search paths and the `**/x`, `**/x/**` expansion of
`parseSearchPaths`, `!pattern` exclude exceptions from settings). Binary files stop at the first NUL
(ripgrep default). Encoding: UTF-8 (lossy); BOM-marked UTF-16 is transcoded by `grep-searcher`.
Results stream per file; columns are UTF-16 (1-based, like `vsr-text`); previews use the ported
`TextSearchMatch` (one-line preview, elision "⟪ N characters skipped ⟫"); `search.maxResults`
counts ranges and sets `limit_hit`.

**Settings.** `SearchSettings` is a plain struct with upstream defaults (verified in
`search.common.contribution.ts` / `files.contribution.ts`): `search.exclude`
(`**/node_modules`, `**/bower_components`, `**/*.code-search`), `files.exclude` (`.git .svn .hg .jj
.DS_Store Thumbs.db`), `useIgnoreFiles` true, `useGlobalIgnoreFiles` false, `useParentIgnoreFiles`
false, `followSymlinks` true, `smartCase` false, `maxResults` 20000, `searchOnType` true, debounce 300 ms.

**Settings wiring (2026-11-09).** `search_view::current_settings` reads the live `AppConfig` (defaults when no
configuration is installed). `config::settings::search_settings` maps `Configuration` to `SearchSettings`:
`files.exclude` and `search.exclude` come from `get_value`, which already merges the default map with the user's
(objects deep-merge, a `false` turns a default off); entries whose value is an object (`when`) are ignored by search.
`search.maxResults` is registered `["number","null"]` (unvalidated): null is unlimited. `search.collapseResults` is
matched by its camelCase value. Eleven keys are in `WIRED_KEYS` ("Search" group): `search.exclude`,
`useIgnoreFiles`, `useGlobalIgnoreFiles`, `useParentIgnoreFiles`, `followSymlinks`, `smartCase`, `maxResults`,
`searchOnType`, `searchOnTypeDebouncePeriod`, `collapseResults`, `showLineNumbers` (a line number before each match
row). `files.exclude` stays under Explorer; search reads it too. The view observes the global
(`on_config_changed`): a change that affects what a search finds (exclude globs, ignore-file flags, symlinks,
smart case, `maxResults`) re-runs a non-empty query; the other keys just apply to the next search or the display.
Headless tests in `search/e2e_tests.rs` use a temp settings file: `search.exclude` hides `docs/` and re-runs,
`smartCase` makes "Alpha" case-sensitive, `searchOnType: false` waits for Enter, `maxResults` limits and re-runs.
`showLineNumbers` is not covered by a test (it is one label in the row).

**View** (`vsr-app/src/search_view.rs`, state in `search/model.rs`). Query box with Match Case,
Whole Word and Regex toggles inside it (Alt+Cmd+C/W/R), "files to include/exclude" behind the
"..." toggle (Ctrl+Shift+J) with the "Use Exclude Settings and Ignore Files" toggle, results
message ("N results in M files", no-results text with Open Settings / Search again in all files,
limit-hit warning), flat list of files (`search.defaultViewMode` default) with name, folder, count
badge, fold and dismiss, match rows with the highlight (`Match.preview()` / `lcut`), virtualised
(`uniform_list`). Search runs on the background executor, results are batched to the UI every 100 ms,
a new search or Esc cancels the old one. Clicking a match opens the file, selects the range and
scrolls it to the centre. Shift+Cmd+F switches to Search and focuses the query. Title actions:
Refresh / Cancel, Clear Search Results, Collapse / Expand All. New theme fields: `inputOption.*`,
`list.activeSelection*`, `editor.findMatchHighlightBackground`, `search.resultsInfoForeground`,
`badge.*`, `toolbar.hoverBackground`.

**Not implemented:** replace (done later: ADR-0029), the search editor / "Open in editor",
tree layout, open-editors and changed-files scopes, search history, keyboard navigation of the
result list, context menus, file icon themes, notebook search, AI / semantic search, PCRE2-only regex
features (look-around, backreferences: Rust `regex` syntax; invalid patterns show the engine's error),
`files.exclude` entries with `when`, encodings other than UTF-8 and BOM-marked UTF-16.

**Testing.** Ported upstream tests (ripgrepTextSearchEngineUtils, search, queryBuilder common and
browser; see `upstream/ports.toml`), engine tests over temp trees, 8 headless gpui E2E tests
(`search/e2e_tests.rs`). gpui lesson: the test scheduler panics if a foreground task is woken from a
foreign thread, so worker threads must not send per file; results are flushed from the background
task (and at most every 100 ms), which also keeps the tests deterministic.

**Performance** (release, `crates/vsr-search/tests/perf.rs`, ignored by default, vscode-original/src:
13,719 files). Target: well under 0.5 s for a literal query. **Not met on this machine, and no
in-process change can meet it.** Commands: `UPSTREAM_CLONE=<clone> cargo test -p vsr-search --release
--test perf -- --ignored --nocapture`, three runs per version, medians:

| version | `function` (40,530 results) | `registerSingleton` (1,200) | regex `\bclass\s+\w+Service\b` (1,866) |
|---|---|---|---|
| before: per-thread `Searcher`, callback behind a `Mutex` | 1.69 s (runs 1.71, 1.69, 0.25) | 1.72 s (1.72, 1.84, 0.16) | 1.76 s (1.76, 1.82, 0.17) |
| after: matches streamed over a channel to the caller | 1.57 s (1.57, 1.73, 0.23) | 1.79 s (1.77, 1.97, 1.79) | 1.75 s (1.75, 1.84, 1.74) |

The runs are bimodal: 1.6 to 2.0 s, or 0.16 to 0.25 s when the machine is in a fast state, with the
same code and files. Probe on the same tree (a temporary test, removed): directory walk only 44 ms;
`metadata` of every file 64 ms; `open()` of the 13,719 distinct files 1.66 s (about 120 us each; the
same on a second pass; 8 threads do not shorten it); `open()` of one file 13,719 times 0.30 s. So the
time is spent in the kernel's per-open work on this Mac (an Endpoint Security client, Santa, is
installed; probably it is that), not in the walk or the matching. Other tools in the slow state, same
tree: embedded ripgrep (`rg -c function`) 1.66 to 1.80 s, `git grep -c function` 1.64 to 1.68 s.
Tried and not kept: `memory_map(MmapChoice::auto())` (needs `unsafe`, a file truncated during a search
can SIGBUS, and it did not help: `function` 1.60, 0.16, 1.73 s). Excludes and CRLF mode are not the
cost (perf also prints the time without the settings excludes). Kept for design reasons: the walk runs
on a helper thread and matches stream over a channel, so `on_file` runs on the caller's thread (no
`Send` bound, no callback lock on the workers). Tests: a pre-cancelled search walks nothing,
a parallel walk streams every file, `max_results` stops at exactly the limit.

## ADR-0024 — Background runs of the real app; safe .app bundle updates (2026-11-09)

User asked that automated runs never cover their work. Two layers:
1. **Headless E2E tests** (gpui `TestAppContext`) never open a window; they are the default way
   to verify behaviour, including settings effects.
2. **Real app in background:** `scripts/rscode --background` / `RSCODE_BACKGROUND=1` opens the
   window with `focus: false` and never activates the app, so it stays behind the user's
   frontmost app. Verified: frontmost app unchanged after launch.
   Next: off-screen rendering checks (gpui `render_to_image` with the real `AppleTextSystem`) that
   assert pixels numerically; any saved image stays local and is never viewed or uploaded
   without the user's approval.

Launcher bug found while doing this: `cp -f` over the bundle's executable while an rscode
instance ran from it made the next launch die with `SIGKILL (Code Signature Invalid)` and popped
macOS's crash reporter. Fix: copy to a temp file + `mv` (new inode), and ad-hoc sign the whole
bundle (`codesign --force --sign -`), since macOS validates the bundle, not just the binary.

## ADR-0025 — Settings migration tracker and policy (2026-11-09)

`scripts/settings-port` inventories all 2062 settings (TOC placement, source, feature group,
upstream registration + consumer files) and assigns a status: **wired** (in
`crates/vsr-app/src/config/wired.rs` `WIRED_KEYS`), **exthost** (contributed by a built-in
extension, expected to work once the Node extension host runs it, ADR-0020), **n/a** (with a
reason), or **todo** (P1/P2/P3). `next` emits one feature group per task with acceptance criteria
(live effect, headless E2E test, added to wired.rs). Process: `docs/settings-migration.md`.

Initial counts: wired 19, exthost 426, n/a 540, todo 1077 (P1 316, P2 596, P3 164), 263 groups.
Policy (in `upstream/settings.toml`, **pending the user's review**): AI chat/agents/Copilot/MCP
(314), deprecated (140), remote/tunnels, Windows- or Linux-only, web-only, telemetry,
auto-update, settings sync and the integrated browser are n/a. Per ROADMAP.md, the migration
starts after Search.

## ADR-0026 — Verification never touches windows or focus (2026-11-09)

Supersedes the "real app in background" part of ADR-0024. The user relies on dictation tools that
break when window focus changes. All automated verification (by the main agent and every
subagent) is headless: gpui `TestAppContext` / `VisualTestContext`, unit tests, and the real
`AppleTextSystem` used without a window. Not allowed unless the user explicitly asks at that
moment: launching rscode or any GUI binary (including `--background`), AppleScript / System
Events UI scripting, screenshots, anything that opens a window or could change focus. To try a
build, the user runs `scripts/rscode` themselves. The off-screen rendering checks planned in
ADR-0024 are only acceptable if they render through gpui's test platform with no OS window.

## ADR-0027 — Editor Find widget: a UI-free `vsr-find` model and an `EditorView` overlay (2026-11-09)

**Context.** Cmd+F (Find / Replace in the current file) was next after Search (ROADMAP). Upstream
splits it into `findModel.ts` (matches, navigation, replace, driven by `FindReplaceState`),
`findDecorations.ts`, `replacePattern.ts`, the `FindController` that seeds and starts it and
`findWidget.ts` (DOM).

**Decisions.**
- **New crate `vsr-find`** (UI free; depends on `vsr-text`, `vsr-editor`, `vsr-search`, `regex`). Not a
  module in `vsr-editor`: that crate is the cursor core (ADR-0013) and find needs a regex engine and
  the Search view's query semantics. Parts: `search` (compile + match with captures, per line or
  multiline), `replace_pattern` (a port of `replacePattern.ts` and `buildReplaceStringWithCasePreserved`),
  `model` (`FindModel`, `FindState`, seeding and scope helpers).
- **Same query semantics as the Search view** by importing `vsr_search::query::regex_source`
  (literal escaping, regex, whole word as `\b`, `\n` making a search multiline) without smart case.
  Consequence: Rust `regex` syntax, so no look-around or backreferences, as documented for Search
  (ADR-0023). `fancy-regex` was available in the registry cache but would make Find and Search disagree.
- **No decorations.** Upstream tracks matches as model decorations and re-searches 100 ms after an
  edit. Here the matches are a sorted `Vec<Range>` recomputed when `Editor::version()` changes (a
  `sync` call per frame); `matchBeforePosition` / `matchAfterPosition` / `getCurrentMatchesPosition`
  are ported over that list, including the zero-width "stuck" handling and `loop` gating
  (`canNavigateForward/Back`). Highlighting caps at `MATCHES_LIMIT` (19999, label `19999+`), find
  operations do not. A find scope moves with the model's own replaces only.
- **Replace All is one undo step**: `Editor::replace_ranges` (new, in `vsr-editor`) applies the
  edits from the end of the document and records one undo group; selection and scope are tracked
  through the edits like upstream's `ReplaceAllCommand`.
- **The widget lives in `EditorView`** (`editor_view/find.rs`), not as its own entity: every
  command needs the active document (`Editors`) next to the widget state, and both are reached
  through the existing `EditorHost` trait, so `main.rs` is untouched. The two text boxes are
  `gpui-component` inputs. The editor surface and the overlay sit under one `EditorFind` key
  context so Cmd+F / Cmd+G / Escape / Alt+Cmd+C,W,R,L,P work from both; Enter in the Find box
  comes from the input's `PressEnter` event (the input propagates Enter, a binding would fire twice).
  Escape closes it and focuses the editor.
- **Keys (macOS):** Cmd+F, Alt+Cmd+F (replace row; seeds only a non-empty single-line selection),
  Cmd+G / Shift+Cmd+G / F3, Enter / Shift+Enter in the Find box, Enter (replace one) and Cmd+Enter
  (replace all) in the Replace box, Alt+Cmd+Enter replace all, Cmd+Shift+1 replace one, Cmd+E use
  selection for find, Alt+Cmd+C / W / R / L / P toggles, Escape.
- **Settings wired** (`editor.find.*`): `seedSearchStringFromSelection`, `autoFindInSelection`, `loop`,
  `cursorMoveOnType`, `findOnType`, `closeOnResult`. Not wired: `addExtraSpaceOnTop` (would shift the
  list's hit-testing), `globalFindClipboard`, `history`, `replaceHistory`.
- **Tests.** `replacePattern.test.ts` (8) and 35 of the 44 `findModel.test.ts` tests ported
  (`findTest(...)` is now recognised as a test wrapper by `scripts/test-port`); not ported: the
  decorations test, 2 `selectAllMatches` (one cursor) and the 6 look-ahead replace tests. 15 headless
  gpui E2E tests (`editor_view/find_e2e.rs`) drive the real workbench: Cmd+F seeding and "2 of 5",
  Enter / Shift+Enter, Escape focus, Alt+Cmd+F replace all and a single Cmd+Z, regex `$1` / `\u`,
  find in selection, the settings.

**Not implemented:** select all occurrences (single cursor), find history, global find clipboard,
`addExtraSpaceOnTop`, the options-only widget shown when closed, resizing the widget, overview ruler
marks, Tab order between the widget buttons, `find.test.ts` / `findController.test.ts` ports.

## ADR-0028 — Quick input and the fuzzy scorer (2026-11-09)

Quick Open (Cmd+P) is built on a reusable Quick Pick so the Command Palette can reuse it.

- **`vsr-quickinput` (UI-free)** holds the port of `fuzzyScorer.ts` (with the parts of `filters.ts` and
  `comparers.ts` it needs), the quick pick model (rows, active row, loop/page navigation, generic
  filtering) and the quick open picks (history first, then top 512 file results, the
  `path:line:column` suffix of `extractRangeFromFilter`). nucleo was not used: matching VS Code's
  ordering and highlights is the point, and the scorer ports cleanly (67 of 68 upstream tests pass).
  Deviations: char indexes instead of UTF-16 offsets, a numeric case-insensitive comparison instead of
  `Intl.Collator`, a `HashMap` score cache instead of number hashes.
- **`vsr-app/quick_input.rs`** is the gpui view: a `PickSource` supplies rows for the typed text
  (immediately or as a background task), the owner listens to `QuickPickEvent::{Accept, Dismiss}` and
  closes it by dropping the entity. gpui-component's single-line `Input` swallows Up/Down/PageUp/
  PageDown and only propagates Escape, so the picker takes those actions in the capture phase.
- **`vsr-app/quick_open.rs`** builds the file list on the background executor with the `ignore`
  walker and `vsr-search`'s `FileFilter` (files.exclude, search.exclude, ignore files), caches it
  and walks again when it is older than 5 s or the settings changed. Editor history lives in
  `Editors` (most recent first, not persisted yet). Wired settings: `workbench.quickOpen.closeOnFocusLost`,
  `preserveInput`, `search.quickOpen.includeHistory`, `search.quickOpen.history.filterSortOrder`.
- History entries match contiguously only and the file results exclude them, as upstream does.
- Not yet: symbol picks, open to the side, remove from history, file icons, a file watcher.

## ADR-0029 — Replace in the Search view (2026-11-09)

Find and replace across files, matching `searchView.ts` / `searchWidget.ts` / `replaceService.ts`.

- **One replace-string model per upstream class.** `vsr-find::SearchReplacePattern` ports
  `services/search/common/replace.ts` (not the editor's `replacePattern.ts`, which `FindModel` keeps
  using; both share `build_replace_string_with_case_preserved`). It re-runs the search regex on the text
  of one match to get its groups (`getReplaceString`), parses the replace string as a string (`\n`, `\t`,
  `\\`, `$0` -> `$&`), applies Preserve Case to the whole template and then expands `$1`, `$&`, `$$`
  like `String.replace`. So a non-regex search replaces literally, and Preserve Case with a regex
  template (`newrege$1`) is cased before the groups are filled in: this differs from the editor Find in
  exactly the way upstream's two classes differ. Deviations: Rust `regex` (no look-around), `$` inside text
  inserted by `\U$1` stays literal. Smart case is folded in (`TextSearchQuery::effective_case_sensitive`).
- **UI.** The chevron (full height, left of the inputs) opens the Replace row: input (placeholder
  "Replace"), Preserve Case toggle (`preserve-case`, Alt+Cmd+P), Replace All button (`replace-all`,
  Alt+Cmd+Enter), which is disabled ("Submit Search to Enable") whenever the shown results are not the
  results of the query in the box (`setReplaceAllActionState`: off on typing, on when a search finishes with
  results). Enter in the Replace input runs the search again. With a non-empty replace string every
  match row shows the matched text struck through (`diffEditor.removedTextBackground`) followed by the
  replacement (`insertedTextBackground`, `insertedLineBackground` border), per `searchview.css`. Hover
  actions: match rows Replace (`replace`) and Dismiss; file rows Replace All and Dismiss. Upstream uses
  the `replace` codicon for the file action too (`searchReplaceIcon`), `replace-all` is only the widget
  button, so rows follow upstream.
- **Confirmation.** The Replace All button asks "Replace N occurrences across M files with 'X'?" (all the
  upstream message variants, the value cut to 10 lines) and afterwards shows "Replaced N occurrences
  across M files with 'X'." in place of the result count. gpui-component has a `Dialog`, but it needs a
  `Root` layer around the whole window (a change to `main.rs`), so the question is an inline bar under
  the inputs with Replace / Cancel (Escape cancels). Per-file and per-match replace ask nothing, as
  upstream. Headless tests click the bar's buttons.
- **Applying.** The view never edits files: it emits `SearchEvent::Replace(ReplaceRequest)` (ranges +
  pattern + preserve case) and the workbench (`Workbench::apply_search_replace`, in `search_view.rs`)
  answers with `replace_finished`. A file that is open is edited through `Editor::replace_ranges` (one
  undo step, Cmd+Z restores it) and then saved with the usual `files.*` save transforms: upstream's
  `ReplaceService.replace` bulk-edits the text model and then calls `save` on it, so the file is NOT left
  dirty (the editor is dirty again after Cmd+Z; the disk keeps the replaced text until the next save).
  Any other file is rewritten on disk (`search/replace.rs`): UTF-8 only (anything else is reported and left
  alone), the BOM and line endings are kept, replacement newlines follow the file's EOL. The replacement
  of each range is computed from the text found there at apply time (the range text is run through the
  pattern); a range that no longer matches is skipped, so a file edited since the search is not corrupted.
- **Results afterwards.** Replace All and the file action remove the replaced files from the results
  (upstream removes the replaced matches); a per-match replace removes that match and moves the other
  ranges of the file (and patches the previews on the same line) the way the text moved; when that cannot
  be derived from what the results hold (a multiline replacement or match on the same lines, or a preview
  cut around its match) the search runs again. Files that fail stay and the message lists the errors.
- **Settings.** There is no replace-related `search.*` setting upstream, so `WIRED_KEYS` does not change.
- **Not implemented:** Replace in Files (Shift+Cmd+H), keyboard navigation of results (so no Shift+Cmd+1 on
  a focused row), a multi-line Replace input (Ctrl+Enter inserts a newline upstream), replace history, the
  replace preview diff editor, tree layout, notebook cells. Disk files are rewritten on the UI thread.
- **Tests.** Ported: `services/search/test/common/replace.test.ts` (8 of 8) and the replace-string part of
  `SearchModel > getReplaceString ...`; unit tests for per match / per file / all, CRLF, multibyte
  (UTF-16 columns), BOM, stale ranges, multiline, regex groups and Preserve Case in `search/replace.rs`;
  `Results::replaced_match` and the messages in `search/model.rs`; headless E2E (`search/e2e_tests.rs`):
  Replace All with an open editor (disk updated, editor updated and saved, Cmd+Z restores and dirties),
  cancelling the question and staleness, per-match and per-file replace, regex `(\w+)@` -> `$1#`, Preserve
  Case, CRLF + multibyte on disk.

## ADR-0030 — Command registry and the Command Palette (2026-11-09)

Shift+Cmd+P (and F1) opens the Command Palette on the Quick Pick of ADR-0028, backed by a real
command registry that menus, key bindings and the extension host will share.

- **`vsr-commands` (UI-free)** is `CommandsRegistry<H>`: an id, a `CommandDescriptor` (the metadata of
  `Action2`: `title`, `category`, `f1`, `precondition`, `description`, `original_title` for the alias
  match) and a handler `H`. Like `CommandsRegistry.registerCommand`, registering an id again overrides
  it and disposing the newer `Registration` brings the older handler back. Preconditions are `When`
  expressions (`key`, `!`, `==`, `!=`, `&&`, `||`, parentheses) evaluated against `ContextKeys`; the
  palette lists `f1` commands whose precondition holds, like `MenuId.CommandPalette` filtered by
  `enabled`. `CommandsHistory` is the LRU of recently used ids (`workbench.commandPalette.history`,
  default 50, 0 off). `keybinding` formats a chord like `UILabelProvider` on macOS (`⇧⌘P`).
- **`vsr-app/commands.rs`** instantiates `H = Handler::Action { action, scope }`: a command is a gpui
  action plus metadata. Running it dispatches the action from where focus is (`Scope::Editor` first
  focuses the editor), so the `on_action` listeners that already exist stay the only implementation and a
  key press and a palette pick take one path. Every action rscode has is registered under its VS Code id
  (63 commands, 21 in the palette with an editor open, 8 without): the key map's editor commands (`cursorLeft`... `undo`, `redo`,
  `editor.action.selectAll`, clipboard, `workbench.action.files.save`, `closeActiveEditor`), Find
  (`actions.find`, `editor.action.startFindReplaceAction`, next/previous, toggles, replace), the Search
  view toggles, `workbench.action.{quit,quickOpen,showCommands,gotoLine,openSettings,toggleSidebarVisibility,clearCommandHistory}`,
  `workbench.view.{explorer,search}`. New: Cmd+B (ADR-0022 noted it missing), Shift+Cmd+E, Ctrl+G, F1.
  The registry is a gpui `Global` (`Commands`) with the history and the Quick Access providers.
- **Key bindings are read, not stored.** The label on the right of a row is the first binding for that
  action in gpui's keymap (`cx.key_bindings()`, context ignored, so it is right whatever has focus), so
  a binding change shows up in the palette and cannot drift. A test compares every palette row with the
  keymap. Each key is its own box (keybindingLabel.* colors of Dark Modern, inset bottom shadow); on
  the focused row the boxes lose their fill like `quickInput.css`.
- **Quick Access** (`quickAccess.ts`): `QuickAccessRegistry` picks the provider with the longest prefix
  that the typed text starts with, else the default (files). `>` commands, `:` Go to Line/Column,
  `?` help (lists every provider's help entries, Enter switches to it), `""` Go to File. One
  `QuickPick` serves all of them: the text decides the provider on every change, so Backspace over `>`
  is Quick Open, typing `>` in Quick Open is the palette, Cmd+P / Shift+Cmd+P / Ctrl+G inside an open
  picker switch it. The picker's source produces the rows of files, commands and help; Go to Line needs the
  active editor, so the workbench fills its one row when the picker emits `QuickPickEvent::Change`.
  Placeholders follow the provider. `preserveInput` (`workbench.commandPalette.preserveInput`, default
  off) restores the last text and selects what follows the prefix.
- **Filtering is not the fuzzy scorer.** Upstream matches commands with `or(matchesBaseContiguousSubString,
  matchesWords)` on `Category: Title` (and the English alias), plus the exact command id; the fuzzy scorer
  is for files. Both filters are ported (`vsr-quickinput::filters`). Order: recently used (most recent
  first, "recently used" separator, then "other commands"), the rest by `localeCompare` of the label with
  the `Developer` category last. Commands with the same label show their id as description.
- **Go to Line** is `parsePosition` ported with its two test cases (line, `line:col`, `line,col`,
  `line#col`, negatives, visible columns with tabs, `::offset`). The prompts are upstream's
  ("Type a line number to go to (from 1 to N)." and so on; the task text "between 1 and N to navigate
  to" is not upstream's wording). Enter moves the caret and focuses the editor.
- **How the extension host will register commands (ADR-0020).** `MainThreadCommands.$registerCommand(id)`
  will call `registry.register(descriptor, Handler::Extension { id })`; contributed `contributes.commands`
  (title, category, `enablement`) fill the descriptor, `contributes.menus.commandPalette` toggles `f1`,
  and `contributes.keybindings` become gpui `KeyBinding`s bound to a generic `RunCommand(id)` action so
  the palette label keeps coming from the keymap. Executing `Handler::Extension` sends
  `$executeContributedCommand` over the RPC and awaits it; `commands.executeCommand` from an extension
  for a built-in id goes through `Commands::execution`. `registerCommand` stacking and `Registration`
  disposal already match what extensions expect.
- **Tests.** Registry, `When`, history, labels and the command table are unit-tested; ports:
  `commands.test.ts` (2), `quickAccess.test.ts` registry tests (2), `gotoLineQuickAccess.test.ts` (2),
  `filters.test.ts` (6). 19 headless gpui E2E tests (`quick_access/e2e_tests.rs`) drive the real
  workbench: Shift+Cmd+P / F1, "toggle side" + Enter hides the side bar, Cmd+B, recently used order,
  history 0 and Clear Command History, `>` to files and back, `?` help, `:42` and Ctrl+G, tabs and
  offsets, key binding labels vs the keymap, Select All / Find / Save from the palette, preserveInput.

**Not implemented:** tf-idf "similar commands" (3+ characters that match nothing), commonly used
commands, the alias line (localized builds), the remove-from-history button, persisted history
(no storage service yet), the confirmation dialog of Clear Command History, user `keybindings.json`,
menus built from the registry (the macOS menu still has only Quit), live reveal and decoration while
typing a line number, the zero-based offset toggle, the `view`, `@` symbol, `#` and `%` providers.

## ADR-0031 — Editor minimap: pure layout, a pixel image, and a build per render (2026-11-09)

`editor.minimap.*` (enabled, side, size, showSlider, autohide, renderCharacters, maxColumn, scale) is
implemented in `editor_view/minimap.rs` with `layout.rs` (pure math), `raster.rs` (pixels) and
`config/minimap.rs` (typed settings).

- **Layout is a port, not a redesign.** `_computeMinimapLayout`, `computeContainedMinimapLineCount`,
  `MinimapLayout.create` (slider, start/end line, top padding, anti-flicker vs the previous layout) and the
  sampling lines are Rust ports on `f64`, checked against upstream's `editorLayoutProvider.test.ts` minimap
  cases (incl. pixel ratio 2/4, fill/fit, sampling) and `minimapLayout.test.ts`. Word wrap, view zones and
  custom line heights do not exist in rscode, so those inputs are constants.
- **Pixels, like upstream.** The canvas is a BGRA buffer drawn with upstream's pre-baked 1x/2x character
  sheets (`renderCharacters`) or flat blocks, handed to gpui as one `RenderImage` (`paint_image`; adds the
  `image` crate, already in the lockfile via gpui). Per-quad drawing would mean ~100k quads for a full minimap.
  Scale 3 resamples the 2x sheet (upstream samples the editor font from a canvas). Colours: editor
  foreground on editor background, no tokens yet.
- **Virtualised.** Only the lines that fit the canvas are drawn (`end_line - start_line`, at most the canvas
  height / line height), tabs and full-width characters as upstream, and the image is rebuilt only when
  something it shows changed (document version, first line, size, colours). A 10k-line file draws the same
  number of lines as a 100-line one.
- **Built per render from the last painted size.** `EditorView::element` calls `MinimapView::build` with the
  scroll handle's offset and the last painted editor bounds, which is the only hook in `editor_view.rs` (plus
  the flex row around the editor surface). A zero-sized check element compares what the build assumed (width,
  height, scroll offset, pixel ratio) with what was painted and notifies the view when they differ, so the
  first frame, resizes, key-driven `scroll_to_item` and display changes converge in one extra frame.
- **Interaction.** Click (proportional): the clicked line is centred. Click (fill/fit): drag from the slider
  centre. Slider drag follows the pointer anywhere in the window (listeners registered at paint, released on
  mouse up). Wheel over the minimap scrolls the editor. showSlider `mouseover` and autohide `scroll` use a
  500 ms timer (`SCROLL_VISIBLE`), `autohide: mouseover` is hover.
- **Not implemented, left `todo`:** `showRegionSectionHeaders`, `showMarkSectionHeaders`,
  `markSectionHeaderRegex`, `sectionHeaderFontSize`, `sectionHeaderLetterSpacing` (need comment tokens from a
  tokenizer and `#region` markers from language configuration). Also missing: token colours, selection / find /
  error decorations on the minimap, the shadow when scrolled horizontally, fade transitions.
- The vertical scrollbar width is 0 (rscode has no scrollbar yet), padding is 0 and `scrollBeyondLastLine` is
  false; they are inputs of the layout, so adding them later is a matter of passing them.

## ADR-0032 — Editor rendering settings: whitespace, rulers, guides, brackets, occurrences, scrolling (2026-11-09)

The high-visibility editor rendering settings are wired through the ADR-0022 path (`EditorSettings`
built from `Configuration`, compared with `PartialEq`, rebuilt live; key listed in `config/wired.rs`).
The decision logic is pure and lives in `editor_view/decorations.rs` (markers, indent levels, active
guide, occurrences, scroll rows) and in `vsr-editor/src/brackets.rs` (bracket matching over the buffer).
`rows()` only turns the results into boxes, each with a `debug_selector` (`ws:`, `ruler:`, `guide:` /
`guide-active:`, `bracket:`, `occurrence:`, `selection:`) that the headless E2E tests in
`config/e2e.rs` assert on. Colours are the Dark Modern chain (`dark_modern.json` -> `dark_plus.json` ->
`dark_vs.json`) or the registry dark default when no theme sets them.

**Wired:** `editor.renderWhitespace` (all five modes), `editor.rulers` (numbers and `{column, color}`),
`editor.guides.indentation`, `editor.guides.highlightActiveIndentation`, `editor.matchBrackets`,
`editor.selectionHighlight`, `editor.selectionHighlightMaxLength`, `editor.occurrencesHighlight`,
`editor.roundedSelection`, `editor.renderLineHighlightOnlyWhenFocus`, `editor.fontLigatures`,
`editor.scrollBeyondLastLine`.

**Approximated (and why):**
- Indent guides follow upstream's level and active-guide rules (`getLinesIndentGuides`,
  `getActiveIndentGuide`, cases ported as unit tests). Bracket-pair guides are not drawn, so
  `highlightActiveIndentation: "always"` behaves like `true`.
- `matchBrackets` has no tokenizer: brackets inside single-line string literals and after `//` are
  ignored; multi-line strings and block comments are not understood. The scan is budgeted.
- Textual occurrence highlights (no language server): whole words for the caret's word, and for a
  selection made of word characters. `occurrencesHighlight: "multiFile"` is read as `singleFile`.
- Selection matches are per visible line; a selection spanning lines is not matched.
  `editor.selectionHighlightMultiline` is a policy rule (n/a, not yet), which is the upstream default.
- Whitespace markers use `·` and `→` drawn over the expanded text; tabs are still shown as spaces.
- Rulers and guides are placed at visual columns times the space advance (monospace assumption).
- `scrollBeyondLastLine` adds empty rows after the last line; the extra room is sized from the page
  measured at the previous paint.

**Not wired:** `editor.cursorSurroundingLines` and `...Style` (the list scroll is `Nearest`; the
margin needs the keyboard/mouse distinction that `after_change` does not have yet), `editor.padding.*`
(the list's bounds are used for hit-testing, so padding needs a wrapper that keeps them right),
`editor.smoothScrolling` (no animated scrolling), `editor.letterSpacing` (gpui-pre 0.3.11 has no letter
spacing on text runs), `editor.renderControlCharacters` and `editor.renderFinalNewline` (need
control-character glyphs and final-line numbering), `editor.scrollPredominantAxis`,
`editor.scrollOnMiddleClick`, `editor.scrollBeyondLastColumn` (horizontal scroll model).

**Testing.** Pure logic is unit-tested (`decorations.rs`, `brackets.rs`, `settings.rs`); the effects
are E2E tests in `config/e2e.rs` that change the setting and check the boxes appear or disappear.
`docs/settings-coverage.md` is regenerated from `wired.rs`.

## ADR-0033 — Editor tabs: preview, MRU, placement, limit (2026-11-09)

The `workbench.editor.*` group (ADR-0022 wiring, `docs/settings-migration.md`). rscode has one editor
group, so the tab model is upstream's `EditorGroupModel` for that group.

- **Model (`model.rs`, UI free).** `Editors` keeps `mru` (open paths, most recent first), `preview` (at
  most one path) and `settings: EditorGroupSettings` (the typed `workbench.editor.*` view in
  `config/settings.rs`). `open_with(path, OpenOptions { pinned, active })` is `openEditor`: a preview open
  of a new file replaces the current preview at its place (the target moves left by one when it lies
  after the preview); a pinned open of the preview pins it; an open that is not focused goes second in
  the MRU list. `close` focuses the MRU editor (`focusRecentEditorAfterClose`, default on) or the right
  neighbour, then the left.
- **Preview rules.** `EditorGroupSettings::opens_pinned(OpenSource)`: with `enablePreview` off everything
  pins; an Explorer double click pins; Quick Open and code navigation preview only when
  `enablePreviewFromQuickOpen` / `enablePreviewFromCodeNavigation` say so. A preview is pinned by an edit
  (the workbench calls `pin_dirty_preview` on each render, so the dirty state and the pin land together) or
  a tab double click. Turning `enablePreview` off pins the current preview.
- **Limit.** `limit.*` closes the least recently used clean editors that are not active, the upstream
  rule of `editorsObserver.ts` (dirty editors never close; `excludeDirty` takes them out of the count).
  It runs when an editor opens and when the setting changes.
- **Deleted files.** `closeOnFileDelete` polls file existence once a second (no watcher yet). Clean
  editors of missing files close; dirty ones stay open, as upstream.
- **Look.** Preview tabs are italic. `highlightModifiedTabs` replaces the dirty dot with a top border in
  the `tab.*ModifiedBorder` colours (theme.ts defaults, which Dark Modern does not override). `labelFormat`
  adds the directory (short: its name, medium: relative to the folder, long: absolute; default: relative,
  only for tabs that share a name). `scrollToSwitchTabs` steps the active tab with the wheel.
- **Wired:** `enablePreview`, `enablePreviewFromQuickOpen`, `enablePreviewFromCodeNavigation` (read; no
  code navigation yet, so nothing opens from it), `openPositioning`, `focusRecentEditorAfterClose`,
  `closeOnFileDelete`, `limit.enabled`, `limit.value`, `limit.excludeDirty`, `highlightModifiedTabs`,
  `labelFormat`, `scrollToSwitchTabs`, `empty.hint`.
- **Deferred (`upstream/settings.toml`, status `blocked` with a "needs …" reason (separate from n/a), for the user to review):**
  editor splits and groups (`closeEmptyGroups`, `revealIfOpen`, `openSideBySideDirection`, ...),
  navigation history (`mouseBackForwardToNavigate`, `swipeToNavigate`, `navigationScope`), session
  restore (`restoreEditors`, `restoreViewState`: the Cmd+W path in `editor_view.rs` drops the scroll state
  itself, so this needs that file), untitled / binary / custom / large-file editors, language detection,
  tab strip presentation (`wrapTabs`, scrollbars, pinned tab sizing, editor actions, centered layout),
  decorations and custom labels, single window.
- **Tests.** Seven upstream `editorGroupModel.test.ts` cases are ported in `model.rs` (`editor-group-model`
  in `upstream/ports.toml`); the rest are model tests, and `config/e2e.rs` drives the workbench headless
  (preview replaced and pinned by an edit, `enablePreview` off, `openPositioning`, the limit, deleted files).

## ADR-0034 — Files settings: save participants, auto save, read-only (2026-11-09)

The `files.*` group (`docs/settings-migration.md`). Wired through the ADR-0022 path: the typed view is
`EditorSettings` (`config/settings.rs`, with `save_transforms(auto_save)`), the pure rules are in
`config/files.rs`, and the timers and triggers are in `autosave.rs` (a child of the workbench).

- **Save participants, upstream order** (`saveParticipants.ts`): trim trailing whitespace, insert the
  final newline, trim the final newlines. The same list runs on an auto save (upstream runs these
  participants for every save reason; only format-on-save and code actions skip auto saves). On an auto
  save, the lines the caret is on keep their whitespace and the final newlines after the caret line stay
  (`save_ops.rs`, `cursors`).
- **`files.eol`.** Upstream uses it as the `DefaultEndOfLine` of a text with no line break, so it applies
  to a one-line file as it opens, and to new files. A file with line breaks keeps its own. rscode has no
  new or untitled files yet, so only the open path is covered. `auto` is LF on macOS.
- **Auto save** (`files.autoSave`, `autoSaveDelay`, `autoSaveWorkspaceFilesOnly`). The editor notifies the
  workbench on every edit and the workbench re-renders, so the render compares each dirty editor's content
  version with the version its timer started for. A change restarts the timer; dropping the old `Task`
  cancels it. This avoids a second event path. `onFocusChange` saves the editor being left when the
  active editor changes, and every dirty editor when the window loses focus; `onWindowChange` saves every
  dirty editor on the window change. Focus moving between the editor and other views does not save (the
  editor surface has no blur signal yet). Read-only and (with the workspace setting) out-of-folder files
  are skipped.
- **Read-only** (`files.readonlyInclude`, `readonlyExclude`, `readonlyFromPermissions`). Upstream's
  `isReadonly` order: session override, then the include glob (the exclude glob wins), then the permission
  check. Globs match the path relative to the opened folder (`config/glob.rs`). `readonlyFromPermissions`
  reads the owner write bit (upstream's "write locked" check); the disk-level `readonly` flag is not
  separately mirrored. Read-only editors refuse typing, Enter, paste, cut, undo and redo, Find/Replace in
  the editor, and Search's Replace (open or not); saving is still allowed. A read-only tab has a lock.
  `workbench.action.files.toggleActiveEditorReadonlyInSession` sets a session override (in the command
  registry, ADR-0030).
- **Blocked** (`upstream/settings.toml`, status `blocked`, reason starts "needs"): encodings
  (`encoding`, `candidateGuessEncodings`, `autoGuessEncoding`), language ids (`associations`,
  `defaultLanguage`), diagnostics (`autoSaveWhenNoErrors`), tokens (`trimTrailingWhitespaceInRegexAndStrings`),
  file watcher (`watcherExclude`, `watcherInclude`, `saveConflictResolution`), session restore (`hotExit`,
  `restoreUndoStack`), explorer delete (`enableTrash`), file dialogs (`dialog.defaultPath`,
  `simpleDialog.enable`), file participants (`participants.timeout`), refactorings (`refactoring.autoSave`).
- **Tests.** Pure rules: `config/files.rs`, `model.rs` (`save_transforms` undo), `vsr-editor` save ops.
  Headless E2E: `config/e2e_files.rs` (15 scenarios, `tests/e2e-scenarios/files.md`). The window trigger
  needs the test window to be active first (`window.activate_window()`), since a trigger is an
  active-to-inactive change.

## ADR-0035 — Window and workbench settings: title, command center, zoom, startup editor (2026-11-09)

The `window.*` group (22 settings), `window.title` (3) and the general `workbench` group (34) were
worked through with `docs/settings-migration.md`. Each setting is wired, n/a or blocked, with a reason
in `upstream/settings.toml`.

- **Window title (`window_title.rs`).** The template engine is a port of upstream `labels.ts`
  `template()` with its tests (`// upstream: Labels > template`). The variables are the ones rscode
  has (active editor, folder, root, dirty, app name). `profileName` and `remoteName` are empty.
  The title bar and the OS window title (`set_window_title`) show the same string. The dirty dot is
  `set_window_edited` (the macOS close-button dot), not a prefix in the title, as upstream does on macOS.
- **Command center.** With `window.commandCenter` (default on, as upstream) the title bar holds a search
  box with the title and a search icon. A click opens Quick Open. This is a simplification of
  `commandCenterControl.ts` (no command dropdown).
- **Zoom (`window.zoomLevel`).** Factor 1.2^level, clamped to [-8, 8] (upstream `getZoomFactor`).
  `workbench.action.zoomIn/zoomOut/zoomReset` (View; Cmd+=, Cmd+-, Cmd+0) write the setting, so the
  zoom is saved and applied through the configuration. **Partial:** only the editor text scales.
  The chrome (title bar, side bars, tabs, status bar) is laid out in fixed pixels, and gpui-pre 0.3.11
  has no whole-window scale (`Window::set_rem_size` changes only rem-based sizes, which rscode does not
  use). A whole-window zoom needs the layout moved to a scaled unit first. This is a known difference.
- **Editor tab height (`window.density.editorTabHeight`).** 22px compact, 35px default (upstream
  `EDITOR_TAB_HEIGHT`, classic values).
- **Startup editor (`workbench.startupEditor`).** Read once at startup, as upstream. `readme` opens
  `README.md` (or `README`) when the folder has one. The default, `welcomePage`, opens nothing: rscode has
  no Welcome page yet. The same goes for `newUntitledFile` (no untitled files), `terminal` and
  `agentSessionsWelcomePage`. The old hard-coded open of `README.md` or `Cargo.toml` is gone, so a
  fresh launch shows an empty editor until the user opens a file.
- **Policy.** `window.doubleClickIconToClose` is n/a: upstream creates the title bar app icon on Windows
  and Linux only. `workbench.enableExperiments` is n/a (Microsoft's experiment service). Everything else
  left is blocked on a named feature: multi-window, session restore, a theme service, dialogs, context
  menus, link opening, extension views, hover widgets with a delay, a native title bar or full screen.
  `window.titleBarStyle` is blocked (window options are fixed at open).
- **Tests.** `config/e2e.rs` drives the workbench headless: the title (live, and the OS title set), the
  command center (present, opens Quick Open, off), the tab height, zoom through the zoom actions (saved
  to the temp settings file and reset), startup editor (`readme` and the default). `window_title.rs`
  ports the upstream labels `template` test. Settings are never read from the real user file.

## ADR-0036 — Word wrap: a view-line projection in `vsr-editor` (2026-11-09)

`editor.wordWrap`, `wordWrapColumn`, `wrappingIndent`, `wrappingStrategy`, `wordWrapIndicator`, `wordBreak` and
`wordSeparators` are wired (group "Word wrap" in `config/wired.rs`); Alt+Z is `editor.action.toggleWordWrap`
(registry, ADR-0030, palette label "View: Toggle Word Wrap").

- **UI-free layer (`vsr-editor/src/wrap/`).** `line_breaks.rs` is a port of upstream's
  `MonospaceLineBreaksComputerFactory` (`createLineBreaks`: break-before/after classes, CJK and Kinsoku Shori,
  `keepAll`, wrapping indent none/same/indent/deepIndent, tabs, surrogate pairs, two-cell full-width,
  `wrapOnEscapedLineFeeds`), with widths as `f64` in units of one character advance (the unit is a parameter,
  so the tests use upstream's 7 px font). `projection_data.rs` ports `ModelLineProjectionData` (offset
  translation, affinity). `view_lines.rs` projects a `TextBuffer`: per model line an optional wrapping, a
  prefix sum of view line counts, model <-> view positions, view line content (continuation indent as
  spaces). Offsets are UTF-16 units like `Position` columns. Not ported: injected text,
  `createLineBreaksFromPreviousLineBreaks` (upstream's resize shortcut: a resize recomputes every line, its
  results are the same), hidden areas, the DOM computer of `wrappingStrategy: advanced` (read as `simple`).
- **Incremental.** `ViewLines::sync` matches the buffer's lines against the last sync from both ends by a content
  hash and recomputes only the lines in between (the lines an edit event would name; it also covers undo and
  reload). The `Editor` owns the projection behind a `RefCell` (`Editor::set_wrapping(&self, ..)`,
  `view_lines()`), synced lazily and only when the editor version or the parameters changed, so the app can set
  it while rendering and `main.rs` is untouched. Per edit the sync costs one hash pass over the buffer
  (O(bytes)), like the widest-line scan it sits next to.
- **Cursor.** Up/Down/PageUp/PageDown move by view lines, keeping the desired visible column in view
  coordinates (the continuation indent counts), as upstream `_moveDownByViewLines`. Home goes to the view
  line's first non-blank / start when on a continuation row, else the model line's (the toggle); End goes to the
  end of the view line, and from there (or on the last row) to the model line's end (`_moveToLineStart` /
  `_moveToLineEnd`). The state is the model position plus `caret_trailing`: a caret on a wrap boundary is drawn
  at the end of the earlier view line (upstream `PositionAffinity.Left`); clicks at the end of a row set it.
- **View.** The list has one row per view line; `rows()` draws the part of the model line the row shows and maps
  every decoration (selection with its line-break stub, find matches, whitespace markers, brackets, occurrences,
  caret, current-line box, indent guides, rulers) into it by clipping model columns to the row and shifting them
  by the continuation indent (`wrap.rs::Seg`). The line number shows on a line's first row. Hit-testing,
  IME bounds, scroll-to-caret and reveal use rows; the minimap draws and counts view lines (its image key
  includes the wrapping column). Wrapped lines fit the list width (`FitList`), no horizontal scroll.
  Tabs after a break are expanded from the row's start (upstream counts them from the model line); this only
  differs for a tab in the middle of a continuation row.
- **Width.** `wordWrapColumn`: that column. `on` / `bounded`: viewport columns = floor((painted width - gutter - 2) /
  character advance) (upstream `viewportColumn`), minus one with `wordWrapIndicator`, `bounded` takes the
  minimum with `wordWrapColumn`. The advance is the shaped width of a space measured at paint (estimated from the
  font size before that). The first frame has no width, so `on` does not wrap yet; the canvas that records
  the surface bounds asks for another frame when the width or advance differs from what the wrapping used
  (the minimap's pattern). Headless tests inject the width (`viewport_columns_override`), the test text system
  measures nothing.
- **Alt+Z** is a per-document override held by the view (upstream `wordWrapOverride2`), not a settings write:
  wrapped becomes off, otherwise wrap at the viewport.
- **`editor.wordSeparators`** replaces the constant in the word moves and double-click (`Editor::set_word_separators`,
  applied by the view before each command and click) and in the whole-word check of selection highlights. Left
  on the defaults: the word at the caret for occurrence highlights (upstream reads the language's word
  definition there) and the Find widget (`\b`, noted in the coverage doc).
- **Blocked:** `editor.wordSegmenterLocales` (needs UAX #29 / ICU word segmentation in the editing core).
- **Performance** (10k lines, `editor.wordWrapColumn` 20 so every line wraps, debug build, headless, one test
  thread, including layout and paint): 38 ms per keystroke with word wrap on against 39 ms without; changing the
  wrapping column (every line recomputed) 32-38 ms. The projection itself is not the cost; the list is still
  virtualised, so only visible rows are shaped.
- **Tests.** 24 of upstream's 37 `monospaceLineBreaksComputer.test.ts` tests (the injected-text ones are left
  out), `Basic` of `lineBreakData.test.ts` and `SplitLine` of `modelLineProjection.test.ts` are ported
  (`editor-word-wrap` in `upstream/ports.toml`); own tests for the projection, incremental sync and the
  view-line cursor rules (`vsr-editor/tests/wrap.rs`), the wrapping column and row geometry (`wrap.rs`), the
  settings, and headless E2E in `editor_view/wrap_e2e.rs` (scenarios in `tests/e2e-scenarios/preferences.md`).

## ADR-0037 — Theme service: VS Code colour themes from their JSON (2026-11-09)

Every colour of the workbench comes from the active VS Code colour theme, replacing the hard-coded Dark
Modern of `theme.rs`. Settings wired (group "Themes"): `workbench.colorTheme`, `workbench.preferredDarkColorTheme`,
`workbench.preferredLightColorTheme`, `window.autoDetectColorScheme`, `workbench.colorCustomizations`.

- **`vsr-theme` (UI-free).** `color.rs` ports `Color`/`RGBA`/`HSLA` with upstream's exact arithmetic (truncated
  channels, three-decimal rounding), so a derived colour is the same `#RRGGBBAA` as in VS Code. `theme.rs`
  ports `ColorThemeData`: JSONC (the existing `vsr_settings::jsonc::strip_jsonc`), `include` chains (outer
  wins, `"default"` removes a colour), `colors`, `tokenColors`, `semanticTokenColors`, the legacy `settings`
  array, and the customisations (global keys, `[Theme]` blocks with `*` wildcards, token groups and
  `textMateRules`). `selection.rs` ports `ThemeConfiguration` (which setting names the theme, system colour
  mode, high contrast) and `catalog.rs` the ten themes of the `theme-defaults` extension, bundled unchanged
  (MIT, Microsoft; README of the crate has the provenance).
- **Registry defaults are generated, not typed.** `scripts/gen-color-defaults` parses the `registerColor(...)`
  calls of the upstream colour files into `assets/color-defaults.json` (611 colours, per dark / light / hc dark /
  hc light, as expression trees) and fails on any construct it does not know. `registry.rs` evaluates them with
  the semantics of `executeTransform` (`transparent`, `lighten`, `darken`, `opacity`, `opaque`, `mix`, `oneOf`,
  `ifDefinedThenElse`, `lessProminent`). A colour a theme does not set resolves through its default, which may
  refer to the theme's other colours.
- **Resolution order** (`getColor`): customisation, theme file, registry default. A customisation of `"default"`
  means the registry default of the theme type, not the theme file's colour.
- **App.** `theme.rs` stays the mapping of what rscode paints (the `Theme` fields) to colour keys (the
  `theme_fields!` table; `sideBar.foreground` falls back to `foreground`). `chrome_bg`, `border`, `sidebar_fg` and
  `accent` were split into per-surface fields (activity bar, side bar, title bar, status bar, tabs) because themes
  colour them differently. A test keeps the old hard-coded table and asserts Dark Modern from the JSON reproduces
  all 112 fields. `ThemeService` (a gpui `Global`) owns the active theme: it observes `AppConfig`, the window
  appearance, and the theme picker's preview; views read `ThemeService::theme(cx)` in `render`, and the
  Workbench re-renders on every change. It also re-aligns gpui-kit's component theme (inputs).
- **`Preferences: Color Theme`** (`workbench.action.selectTheme`, Cmd+K Cmd+T, in the command registry): the Quick
  Pick grouped light / dark / high contrast, preselecting the current theme, previewing on arrow keys (new
  `QuickPickEvent::ActiveChange`), Escape reverts, Enter saves to the setting that names the theme (a preferred
  setting while auto detecting).
- **Token colours for the highlighter.** `ColorTheme::token_colors() -> &[TokenColorRule]` (scope selector
  strings as written + `{ foreground, background, fontStyle }`, default rule first, user customisations last,
  colours normalised) and `semantic_token_rules()`; scope matching is the highlighter's. `ThemeService::color_theme(cx)`
  gives the active one.
- **Decisions to review.** (1) The default theme stays Dark Modern, not upstream 1.147's Dark 2026 (the
  look rscode had so far); unknown theme names (extension themes in the setting's dropdown) fall back to the
  default of the colour mode; the Settings editor still prints "Dark 2026" as the default text. (2) The preview
  applies at once, upstream waits 200 ms. (3) Existing crates were not swapped in for the colour port: the
  arithmetic must match VS Code to the last rounding step, which a general colour library does not promise.

**Consequences.**
- `window.autoDetectHighContrast`, `workbench.preferredHighContrast*ColorTheme` and `window.systemColorTheme` stay
  blocked (gpui has no OS high contrast signal; window chrome appearance is not driven). The service takes a
  `HostColorScheme { dark, high_contrast }` so the signal can be added later. Icon and product icon themes stay
  blocked ("needs file icon themes").
- `editor.tokenColorCustomizations` and `editor.semanticTokenColorCustomizations` are read and applied to
  `token_colors()`, but change nothing on screen until the syntax highlighter lands.
- Not ported: tmTheme (plist) token files, theme storage, transient colours, extension-contributed colours and
  themes, `Toggle between Light/Dark Themes`.

## ADR-0038 — Languages and syntax highlighting: our own TextMate engine on Oniguruma (2026-11-09)

rscode needs the language of a file (status bar, `files.associations`, Change Language Mode) and
VS Code's own highlighting, which is TextMate grammars (`*.tmLanguage.json`) tokenized by
vscode-textmate and coloured by the theme's `tokenColors`. Three crates, plus app glue:

- `vsr-languages`: the language registry and file associations (ports of `languagesRegistry.ts`,
  `languagesAssociations.ts`, with all 23 of their tests), and the data `scripts/bundle-grammars --clone`
  copies from the upstream clone: every built-in extension's `contributes.languages` (83 languages),
  34 JSON grammars for rust, javascript/jsx, typescript/tsx (+ the jsdoc injections), json/jsonc/jsonl,
  markdown, python, shell, css, html, yaml, xml/xsl, c, cpp, cuda-cpp, go and java, and the Dark Modern theme
  chain (dark_vs, dark_plus, dark_modern). Provenance and licence of every grammar: `data/NOTICES.md`
  (generated from each extension's `cgmanifest.json` and `ThirdPartyNotices.txt`; MIT except the TextMate
  bundles for html and c, which carry the "TextMate Bundle License"). There is no `toml` or `kotlin` language
  in the clone (neither is a built-in VS Code extension), so rscode does not register them either; the old
  hard-coded "TOML" label in the status bar was a fidelity bug.
- `vsr-textmate`: the grammar engine and theme matching (UI free).
- `vsr-syntax`: the service (language of a file, grammar per language, styles under the current theme),
  per-document incremental tokens and the background tokenizer, and the bundled Dark Modern token colors
  behind `TokenColorsProvider`, the trait the theme service will implement instead.
- `vsr-app`: `syntax.rs` (tokenizer per editor, token runs to gpui highlights), `language_mode.rs`
  (Change Language Mode and the repaint poll), the rows of the editor and the minimap.

### Engine decision (researched 2026-11-09)

| option | verdict |
|---|---|
| (a) a Rust vscode-textmate port from crates.io (`irosashi` 0.3, `ferriki-textmate` 1.3, `shiki` 0.0.7) | Maintained for weeks, 0.6k to 6k downloads each, one author, API shaped around whole-document highlighting. `irosashi` ("byte-identical to vscode-textmate", on `ferroni`, a pure Rust Oniguruma port) looks closest: it has a line-by-line `Session` and a comparable `StateStack`. Its docs do not say whether `while` rules, `injectTo` or `injectionSelector` are supported, and it resolves colours itself instead of exposing scope stacks. |
| (b) `syntect` | Loads `.tmLanguage` plists and `.sublime-syntax`, not VS Code's JSON grammars (a conversion step would be ours to build and keep in sync). Its engine follows Sublime semantics: no `while` rules, `applyEndPatternLast` and `injectionSelector` are approximated, and its scope selectors are its own. Fidelity with VS Code's grammars is a known gap, which is the whole point of this feature. |
| (c) tree-sitter + `tree-sitter-highlight` | Fast and incremental, but a different grammar set (queries and parsers per language, several C crates to build) and colours only approximate a VS Code theme, because VS Code themes select TextMate scopes. Zed and Helix accept that; rscode's goal is VS Code's own colours. |
| (d) our own engine on `onig` | Chosen. |

Reasons: it runs VS Code's own grammars with the regex flavour they were written for (`onig` builds through the
package proxy in 16 s: `cc` and the vendored Oniguruma, no system library); the engine is about 1,100 lines
that follow vscode-textmate's algorithm one to one (read from microsoft/vscode-textmate `main` on 2026-11-09:
`theme.ts`, and its tests `themes.test.ts` and `matcher.test.ts`, verbatim; the tokenizer, `rule.ts` and
`grammar.ts` from the documented algorithm), and it exposes exactly what editing needs and no crate gave:
a rule stack with structural equality per line (for re-tokenizing only until the state converges), interned
scope ids (styles resolved per id and cached, so a theme change does not re-tokenize), standard token types
(`trimTrailingWhitespaceInRegexAndStrings`) and `injectTo`. 22 grammars tokenize real code in the tests, with
the scopes checked, and 18 + 16 + 2 tests pin the engine, the theme and the matchers. `irosashi` remains the
thing to try if the engine ever needs replacing: a differential test of its tokens against ours would be the
first step. Nothing in `vsr-languages`, the incremental store or the app depends on the engine's internals.

### How it works

- **Regexes.** vscode-oniguruma's setup: UTF-8, `ONIG_OPTION_CAPTURE_GROUP`, the Oniguruma syntax, a search
  always on the whole line (plus the `\n` vscode-textmate appends) with a start offset, so `\G`, `\A` and
  look-behinds see the text before the start. `\A` and `\G` become `\uFFFF` in the variants where they may not
  match (four precomputed variants per source), `\z` is rewritten, back references in `end` and `while` are
  resolved against the begin captures (escaped) and cached per resolved source. A scanner is a list of
  patterns: the leftmost match wins, ties go to the earlier one. A per-line cache remembers each regex's last
  result (valid for any later start before its match start, and for no match), which made tokenizing 2.8
  times faster; regexes with `\G` are never cached.
- **Rules** are compiled eagerly into an arena (`include` of `#name`, `$self`, `$base`, `scope`, `scope#name`,
  nested repositories, `injections`, `injectTo` grammars) and memoized by the raw rule's `Arc` pointer, the
  same identity vscode-textmate gets from the `id` it stamps on raw objects. Regexes compile lazily.
- **Tokens.** `Token { start (byte), scope id }`. Offsets are UTF-8 bytes of the line; the app maps them to
  gpui's expanded text (tabs, wrapped rows) in `syntax::highlights_for_row` and to UTF-16 for the minimap.
- **Incremental.** `DocumentTokens` keeps text, tokens and start/end states per line. `sync` finds the first
  and last differing lines, marks the middle dirty and keeps the rest. Tokenizing resumes at the first invalid
  line and skips ahead as soon as a stored line has the same text and the same start state (upstream's
  `TokenizationStateStore`). Tests: an edit that does not change the state re-tokenizes one line; opening a
  block comment re-tokenizes to the end of the file; undoing it converges 100 lines later; incremental equals a
  full run after a series of edits. A `BackgroundTokenizer` runs the rest on a thread in 4 ms slices;
  the editor shows a line's old tokens until new ones exist. A repaint poll (60 ms) runs while it is busy.
  `editor.maxTokenizationLineLength` (default 20,000): longer lines are one token and keep the state.
- **Theme.** vscode-textmate's `theme.ts`: rules with parent scopes and `>`, the trie with inheritance,
  specificity ordering; each scope's match overrides only what it sets (foreground, background, font style:
  bold, italic, underline, strikethrough) and merges onto its parent stack. All of `themes.test.ts` that does
  not need its fixture directory and all of `matcher.test.ts` are ported.
- **Editor.** `rows()` draws the line with `StyledText::with_highlights`; positions still come from the
  shaped plain text, which is exact for the monospace fonts the editor uses (bold and italic keep the advance).
  The minimap draws each character in its token colour (`render_lines_colored`).
- **Language of a file.** `files.associations` (configured, wins), then the built-in file names, patterns,
  extensions (longest wins, the last registered wins ties) and first line; plain text when nothing matches.
  `Editors` owns a `SyntaxService`, so tests do not share associations. Editors whose language was picked by hand
  keep it when settings change.

### Settings

Wired (group "Languages"): `files.associations` (live), `editor.maxTokenizationLineLength` (live),
`files.trimTrailingWhitespaceInRegexAndStrings` (`false` keeps trailing whitespace that starts inside a string
or regex token, and in lines the tokenizer has not reached yet, like upstream). `files.defaultLanguage` stays
blocked, reason rewritten to "needs untitled editors": it is the language of new untitled files, which rscode
does not have. `editor.semanticHighlighting*` and `workbench.editor.languageDetection*` stay blocked.

### Numbers (release build, Apple silicon, `editorOptions.ts`: 7,084 lines, 259 KB)

Grammar build 3 ms (rule graph; regexes compile on first use). Tokenizing the whole file: 472 ms cold (includes
compiling the regexes the file needs), 256 ms warm: 1.0 MB/s, 27,700 lines/s, about 36 us per line. A single
edited line re-tokenizes in well under a millisecond. Before the per-line regex cache it was 0.2 MB/s.

### Not done

Encoded token metadata (language id and balanced-bracket flags per token), `embeddedLanguages` per token (the
language of embedded code, for bracket and comment rules), `tokenTypes` for TS template expressions (the bundled
grammars' `tokenTypes` selectors are applied, the default scope-name regexp is the rest), the tokenization time
limit, the JavaScript regular expression grammar (it is a plist: `source.js.regexp` includes resolve to nothing,
so regex literal contents are one scope), injections declared in a grammar's `injections` are ordered by selector
name rather than document order (serde_json's map is sorted; only ties between equal priorities are affected),
semantic highlighting, language configuration (brackets, comments, auto-closing) and the "Configure File
Association" entries of Change Language Mode. Tokenizing off the UI thread means the first paint of a big file
shows plain text for a moment.

## ADR-0039 — File watcher, live explorer, explorer file operations, save conflicts (2026-11-09)

**Context.** The explorer was a read-only snapshot: folders were listed when first expanded, and nothing noticed
files created, renamed or deleted outside the app. Open editors were polled for existence only (ADR-0033), so an
edit made by another program was never seen. The explorer had no create, rename or delete. Blocked settings:
`files.watcherExclude`, `files.saveConflictResolution`, `files.enableTrash`.

**Decision.**
- `explorer/watcher.rs`: a recursive `notify` 8.2.1 watcher (FSEvents) behind a `FileWatcher` trait. Raw events
  are coalesced for 100 ms into batches of paths. `files.watcherExclude` defaults to upstream's object-store globs
  (a user map replaces the default). `Workbench::new` takes no watcher (tests); the app uses `with_services`.
- Live refresh: a batch re-lists the folders the tree has loaded, forgets folders that are gone (and their
  expansion), reloads clean editors whose file changed, and closes editors of deleted files when
  `workbench.editor.closeOnFileDelete` is on. Renames need no pairing: the parent folders are re-listed.
- Save conflicts: an editor remembers the file's modification time when its buffer last matched the disk. A save
  of a dirty editor whose file changed refuses under `askUser` and shows the conflict bar (upstream's
  `staleSaveError` text, with Overwrite and Revert; Compare waits for a diff editor). `overwriteFileOnDisk` saves.
  Auto save never overwrites a changed file.
- `explorer/names.rs`: upstream `validateFileName` (messages as plain text) and `incrementFileName` (simple and
  smart), with the upstream tests ported.
- `explorer/trash.rs`: delete to the macOS Trash through `NSFileManager trashItemAtURL` (the `trash` crate with
  `DeleteMethod::NsFileManager`). The crate's default method drives Finder through `osascript`, which is not used.
  The `Trash` trait has a real, a refusing (`NoTrash`, the default of `Workbench::new`) and a fake implementation.
- `explorer/workbench.rs`: create, rename and delete on the disk, then the tree and the open editors follow.

**Consequences.**
- `files.watcherExclude` and `files.saveConflictResolution` are wired. `files.enableTrash` is honoured by the
  workbench operation, but no command or key reaches it yet, so it stays blocked.
- Not done: the inline name input and its keys, the explorer context menu, the `explorer.*` view settings
  (`sortOrder`, `compactFolders`, `autoReveal`, `incrementalNaming`, `confirmDelete`), copy, paste and duplicate,
  `watcherInclude`, and Compare on the conflict bar.
- Tests never start a real watcher or touch the Trash: the watcher is a `FakeWatcher` the test drives, and the
  Trash is a `FakeTrash` that moves into a test folder. One real-FSEvents test is `#[ignore]`d.
- Renaming a folder collapses it (its expansion is not carried to the new path).
- Watched paths are compared as given; a workspace reached through a symbolic link does not match FSEvents'
  resolved paths yet.

## ADR-0040 — Session restore, hot exit and the Welcome page (2026-11-09)

User's choice (2026-11-09): the Welcome page becomes the startup default, and unsaved text survives quitting.
Code: `crates/vsr-app/src/session/` (`store`, `state`, `backup`, `settings`, `runtime`), `welcome.rs`,
`editor_view/view_state.rs`. Upstream: `workingCopyBackupService/Tracker`, `editorPart`/`editorGroupModel`
serialization, `gettingStarted`.

- **Storage layout** (under the user data dir: `~/Library/Application Support/rscode`, or `RSCODE_USER_DATA_DIR`):
  - `User/workspaceStorage/<key>/state.json`: per folder; `<key>` is a 32 hex digit hash of the folder path (the
    first 128 bits of its SHA-256, `sha2`; stable across toolchains). Writes use `tempfile` (`persist` = rename).
  - `User/globalStorage/recents.json`: recent files (200) and folders (50), shared by all folders. It seeds the
    editor history, so Quick Open and the Welcome page share it.
  - `Backups/<key>/<file key>.json` and `Backups/<key>/discarded/`: hot exit backups, below.
  - Formats are documented in the module headers of `state.rs` and `backup.rs`. Every field has a default and
    unknown fields are ignored, so a file from another build loads.
- **State saved:** open tabs in order, which is active, the preview tab, activation order, per tab the first visible
  line, the selection and the Alt+Z word wrap override, the side bar (visible, width, view) and the Search query and
  options. Written 0.5 s ticks apart once a change has been stable for a tick (debounce), and synchronously on quit.
- **Injection and tests.** The store is a gpui `Global` (`SessionStorage`) that only `main` installs. A workbench
  without it (every test that does not install one) reads and writes nothing, so tests cannot reach the real user
  data dir. The shutdown hook is `Workbench::persist_on_quit`, called by `Context::on_app_quit` (Cmd+Q) and the
  window close hook, and by tests directly. A "restart" in a test is a second workbench on the same temp dir.
- **Restore.** `window.restoreWindows: none` and `workbench.editor.restoreEditors: false` skip the tabs. With one
  window `preserve`, `all`, `folders` and `one` are the same. `workbench.editor.restoreViewState` applies the saved
  scroll and selection, and also reopening a closed file in the same run (`view_memory`). The scroll is applied
  with a strict `scroll_to_item(Top)` at the next paint; a restored tab that was never shown keeps its saved line.
  `workbench.startupEditor` runs only when no editor came back (as upstream).
- **Hot exit (`files.hotExit`).** Default `onExit`: Cmd+Q and `workbench.action.quit` write backups and quit without
  asking; closing the window asks (as upstream on macOS), then quits: rscode has one window (ADR-0019).
  `onExitAndWindowClose` never asks. `off` always asks. The question is a bar above the status bar (Save All,
  Don't Save, Cancel), not a modal dialog (there are none yet). **Before this work** a dirty buffer was lost on
  quit without a word.
- **Backups while editing.** A dirty editor is backed up once its text version has been the same for a tick (about a
  second after the last edit, as upstream), with hot exit off too (crash recovery). At startup every backup is
  restored, whatever the restore settings say: unsaved text is never skipped.
- **Data safety rules** (the full list is in `backup.rs`): writes are atomic (temp file, fsync, rename); a backup is
  deleted only when the buffer is saved or is back at the saved text, or equals the file at restore; a tab closed
  with unsaved changes and Don't Save **move** the backup to `discarded/`, they never delete it; a state or backup
  file that does not parse, has the wrong shape or fails its checksum is renamed `*.corrupt-<n>`, reported in the
  status bar and never deleted or restored; restoring a backup does not touch the file. A restored buffer is the file
  on disk with one undoable replace-all, so undo goes back to the disk text. If the file is gone the buffer starts
  empty and saves it again. Known gap: a crash in the half second between a save and the next tick leaves a stale
  backup that restores as a dirty buffer over the newer file (the text is not lost; the user sees it dirty).
- **Welcome page** (`workbench.action.openWalkthrough`, "Help: Welcome"): an editor tab, rendered like the Settings
  tab (not an `Editors` entry, so it is not restored; startup decides again). Heading "rscode", subtitle, Start
  (Open File... is Quick Open), Recent (the last five existing files, click opens), "Show welcome page on startup"
  (writes `workbench.startupEditor`: `none` / removed). Layout and colours follow `gettingStarted.css` and Dark
  Modern. Left out: New File (no untitled files), Open Folder / Clone / remotes (no folder dialog), walkthroughs
  and Help links (extensions, web). `workbench.welcomePage.*` are n/a for that reason.
- **Settings.** Wired: `files.hotExit`, `window.restoreWindows`, `workbench.editor.restoreEditors`,
  `workbench.editor.restoreViewState` (group "Session"), `workbench.startupEditor` now opens the page (group
  "Welcome"). Still blocked: `window.restoreFullscreen`, `files.restoreUndoStack` (the undo stack is not stored).
- **Tests.** Unit: store (hash, atomic write, quarantine), state round trips, backup store (round trip, damaged
  files kept, discard), the debounce, settings. Headless E2E in `session/e2e_tests.rs` (scenarios in
  `tests/e2e-scenarios/workbench.md`). Ports in `upstream/ports.toml`: `session-restore`, `hot-exit`, `welcome-page`.

## ADR-0041 — Explorer UI: commands, keys, inline name input, delete confirmation, explorer settings (2026-11-09)

**Context.** ADR-0039 built the Explorer's file operations behind no UI: create, rename and delete
existed in the workbench and in tests only. The Explorer tree had no focus, selection or keys, and
the `explorer.*` view settings were absent. Blocked: `files.enableTrash` and the explorer settings.

**Decision.** The Explorer UI follows upstream `fileActions.contribution.ts`, `explorerView.ts`,
`inlineEditInputBox.ts` and `files.contribution.ts` (ids, titles and keys are upstream's; see
`explorer/actions.rs`).

- **Commands** (`commands.rs`, ADR-0030): `explorer.newFile` "New File..." and `explorer.newFolder`
  "New Folder..." (palette, category File); `workbench.files.action.refreshFilesExplorer` "Refresh
  Explorer" and `workbench.files.action.collapseExplorerFolders` "Collapse Folders in Explorer"
  (palette); `renameFile` "Rename...", `moveFileToTrash` "Delete" and `deleteFile` "Delete
  Permanently" (not in the palette, as upstream). The handlers live on the workbench root, so a
  palette pick works from any focus and key presses bubble up to them.
- **Keys** (context `FilesExplorer`, which is the tree; `explorer/actions.rs`): Up, Down, Left
  (collapse or select the parent), Right (expand or select the first child), Space (open the file
  or toggle the folder), Enter (Rename; confirms a pending delete), Cmd+Backspace and Delete
  (Delete), Cmd+Alt+Backspace (Delete Permanently), Escape (cancel a pending delete). Upstream's
  macOS `list.select` (Cmd+Down) is not bound.
- **Inline name input** (`explorer/inline.rs`): New File and New Folder go into the selected folder
  (or the selected file's folder, or the root); Rename changes the selected item. A gpui-component
  `InputState` in the tree. Enter commits a valid name (an error keeps the box open with the message
  of `names::validate_file_name` under it), Escape cancels, and losing focus commits a valid name or
  cancels an invalid one (upstream `InlineEditInputBox`). Rename selects the name without its last
  extension. While the box is open the tree's key context is off, so Space, arrows and Delete type.
- **Delete** (`explorer/delete.rs`): with `explorer.confirmDelete` on (default), a confirmation bar
  shows above the tree with the upstream question ("Are you sure you want to delete 'x'?", "... and
  its contents?" for folders) and detail ("You can restore this file from the Trash."), with Move to
  Trash or Delete Permanently and Cancel. Permanent deletes say "This action is irreversible!" and
  ask "permanently delete". With `files.enableTrash` off, Delete is permanent and asks that stronger
  question. Deviations: upstream's "Undo" wording is not used (rscode has no undo for files), dirty
  editors are not asked about (they stay open with their text, as ADR-0039), and only one item is
  deleted at a time (no multi-select).
- **View title actions**: New File, New Folder, Refresh Explorer and Collapse Folders in Explorer
  (codicons new-file, new-folder, refresh, collapse-all) show while the Explorer header is hovered.
- **Settings** (`explorer/settings.rs`, wired in `config/wired.rs`; blocked rule in
  `upstream/settings.toml`): `explorer.confirmDelete`, `explorer.sortOrder`,
  `explorer.compactFolders`, `explorer.autoReveal` and `files.enableTrash` are read live. Sort orders
  (`explorer/sort.rs`): default, mixed, filesFirst, type and modified; names compare
  case-insensitively with numbers in order (the `natord` crate, as upstream's numeric collator).
  `foldersNestsFiles` sorts as default because rscode has no file nesting. Compacted folders
  (`a/b/c`) are computed when a row is drawn: the row stands for the deepest folder, which is the one
  it opens, and `New File` on it creates inside that folder. `autoReveal` runs when an editor opens
  (not yet on a tab switch); `focusNoScroll` selects without scrolling.
- **Not built**: `explorer.incrementalNaming` (it names pasted and duplicated items; there is no copy,
  paste or duplicate yet) is blocked. The naming functions it needs are in `explorer/names.rs`.

**Consequences.**
- The Explorer has its own focus handle and scroll handle (`ExplorerState`, `explorer/state.rs`).
  The tree rendering moved from `main.rs` to `explorer/view.rs`.
- The headless tests (`explorer/e2e_tree.rs`, scenarios in `tests/e2e-scenarios/explorer.md`) drive
  the keys through the real bindings. One behaviour is not observable headless: the scroll position
  after `autoReveal: true` (the scroll request is made; gpui's test platform does not move the
  offset), so that row is `pending`.
- A file's editor steals focus on the first render after it opens, so a test that presses keys in
  the tree after opening a file must focus the tree again (`focus_tree` in the tests).
- Renaming a file inside a folder that is collapsed reveals the folder first (`expand_to`).
- `natord` is a new dependency; adding it changed other lock entries (`windows` 0.62, `getrandom`
  0.4) that cargo re-resolved.
