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
`search_view::current_settings` is the single TODO hook for the configuration service.

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

**Not implemented:** replace (the chevron is shown disabled), the search editor / "Open in editor",
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
13,719 files; other agents were building concurrently): `function` 40,530 results in 6,269 files in
1.65 s; `registerSingleton` 1,200 in 413 files 1.75 s; regex `\bclass\s+\w+Service\b` 1,866 in 1,315
files 1.76 s. Excludes and CRLF mode were ruled out as the cost; the time is nearly constant across
queries, so it is walk/IO bound. Room to improve: skip glob matching when no globs are set, tune threads.

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
