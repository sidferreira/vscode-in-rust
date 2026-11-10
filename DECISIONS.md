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
asks first (ADR-0040, `session/close.rs`).

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
clusters; Cmd+W/S only act while the editor has focus; other cursor
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
  `cursorMoveOnType`, `findOnType`, `closeOnResult`, and later `globalFindClipboard`, `history`, `replaceHistory`
  (see History and Global find clipboard below). Not wired: `addExtraSpaceOnTop` (blocked, below).
- **Tests.** `replacePattern.test.ts` (8) and 35 of the 44 `findModel.test.ts` tests ported
  (`findTest(...)` is now recognised as a test wrapper by `scripts/test-port`); not ported: the
  decorations test, 2 `selectAllMatches` (one cursor) and the 6 look-ahead replace tests. 15 headless
  gpui E2E tests (`editor_view/find_e2e.rs`) drive the real workbench: Cmd+F seeding and "2 of 5",
  Enter / Shift+Enter, Escape focus, Alt+Cmd+F replace all and a single Cmd+Z, regex `$1` / `\u`,
  find in selection, the settings.
- **History (`editor.find.history`, `replaceHistory`).** `vsr-find::SearchHistory` (newest first, unique,
  capped at 100; the Up / Down walk keeps the typed text as the draft and starts again when a recalled entry is
  edited). An entry is recorded when Enter is pressed in a box and when the widget closes (upstream records
  after a debounce on every change; the recorded text is the same). Up / Down in a box recall it through a
  capture-phase action on the box, and only when there is an entry (otherwise the caret moves as usual). A
  recalled text runs the search when `findOnType` is on, as a typed one does (`set_value` raises no change
  event). `workspace` keeps the lists in the workspace session state (`find_history` / `replace_history` in
  `state.json`, ADR-0040); `never` records nothing and saves nothing (upstream gives a fresh empty set).
  Tests: unit (`vsr-find/src/history.rs`), E2E in `find_e2e.rs` and `session/e2e_tests.rs`.
- **Global find clipboard (`editor.find.globalFindClipboard`, macOS).** `editor_view/find/clipboard.rs` reads and
  writes the find pasteboard (`NSPasteboardNameFind`, through `objc2-app-kit` / `objc2-foundation`, both already in
  the lockfile via gpui). Read when Cmd+F seeds the box and no selection seeded it; written when the search string
  changes to a non-empty text (upstream `readFindText` / `writeFindText`; an empty string is not written, so the
  other apps' clipboard is not cleared by an empty box). Behind the `FindPasteboard` trait: the tests use an
  in-memory pasteboard and never touch the user's find pasteboard. **The system pasteboard is not exercised
  headlessly**; check it by hand (Cmd+F in rscode, then paste into another app's Find field).
- **Not wired: `addExtraSpaceOnTop`** (blocked in `upstream/settings.toml`: it needs a top view zone so the
  widget does not cover the first line, and the editor has no top padding that hit-testing and scrolling share).

**Not implemented:** select all occurrences (single cursor), `addExtraSpaceOnTop`, the options-only widget shown
when closed, resizing the widget, overview ruler marks, Tab order between the widget buttons,
`find.test.ts` / `findController.test.ts` ports.

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

### Theme hookup (ADR-0037)

The token colours are the active colour theme's, not a bundled Dark Modern. `vsr-app`'s `theme_service::sync_token_colors`
hands `ColorTheme::token_colors()` (the theme's `tokenColors` with the include chain, the default rule from
`editor.foreground`/`editor.background`, and `editor.tokenColorCustomizations` as `vsr-theme` applies them) to
`SyntaxService::set_token_colors` as `RawThemeRule`s. It runs at startup and in the Workbench's `ThemeService` observer, so a
theme change, a colour customisation and the picker's preview all restyle open editors and the minimap (the minimap cache
keys on `SyntaxService::theme_generation`). Tokens are not re-tokenized: only styles change. `SyntaxService::new()` starts
with no token colours; `BundledTheme` (Dark Modern) remains for the crate's tests only.

The default rule's colours are not painted through the token trie: vscode-textmate keeps them as the root style, and
`ThemeStyles::style_of` starts from "no colour", so an unmatched token has no foreground and the editor paints its
`editor.foreground` (the minimap uses the same). Visually identical to upstream. `keywords` in `tokenColorCustomizations`
covers `keyword.control`, `storage` and `storage.type` only, as upstream's `tokenGroupToScopesMap` does: `fn` (`keyword.other.fn`)
keeps the theme's colour.

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
- **Closing a dirty tab** (`session/close.rs`; Cmd+W, the tab's close button, a middle click and Close Editor): the
  tab stays open until the bar asks "Do you want to save the changes you made to <file>?" with Save / Don't Save /
  Cancel (upstream `showSaveConfirm`, the same bar as the quit). **Save** writes the file, then closes the tab; a
  save refused because the file changed on disk (ADR-0039) or a failed save keeps the tab open and shows the
  conflict bar or the error. **Don't Save** closes it and moves its backup to `discarded/` (a backup not written yet
  is simply absent). **Cancel** keeps the tab, dirty. Clean tabs close at once. `files.autoSave: onFocusChange` saves
  and closes without asking (a failed save asks); `afterDelay` asks while its delay has not passed, as upstream; on
  macOS `onWindowChange` asks, as upstream. A close of several tabs asks once (no command closes several yet). After
  an answer the editor has the keyboard again: the bar's buttons must not move focus to the workbench root, which
  takes focus on any click that no focusable element above it takes (the bar sets `prevent_default`, as gpui does for
  the editor). The quit bar (Cmd+Q, window close) is the same bar (`PromptKind`). **Save All** on the quit bar
  saves each dirty file by the Cmd+S path (`files.saveConflictResolution`, ADR-0039): a file changed on disk is not
  overwritten under `askUser`, and a failed save is an error. The other files are still saved. If any file is
  refused, the app does **not** quit: the bar closes, the first refused editor is shown with its conflict bar (or
  error), and a later Cmd+Q asks again. Under `overwriteFileOnDisk` every file saves and the app quits. Tests:
  `quit_save_all_*` in `session/e2e_tests.rs`.
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

## ADR-0042 — Code folding, sticky scroll and minimap section headers (2026-11-10)

rscode needs code folding, and two features that stand on it: sticky scroll (the headers of the
enclosing regions pinned at the top) and the minimap's section headers (`#region` and `MARK:`
titles). They are built on one model, UI free in `vsr-editor`, and drawn by three small view
modules.

- **Model** (`vsr-editor/src/folding/`, ports of `foldingRanges.ts`, `indentRangeProvider.ts`,
  `foldingModel.ts`, `hiddenRangeModel.ts`): sorted line regions with parents, the merge of fresh
  regions with the collapsed and manual ones (`sanitize_and_merge`), indentation ranges with the
  `#region` marker handling, off-side languages and the cap on regions, and the collapse operations
  (by level, up, recursive, all but the selection). 56 upstream tests are ported.
- **Edits**: upstream keeps collapsed regions alive with one tracked decoration per region. There
  are no decorations in `TextBuffer`, so `Editor` (`editor_folding.rs`) keeps the line hashes of the
  last sync while anything is collapsed, and on the next sync finds the unchanged lines at the top
  and bottom (`LineMap`, the same idea as the wrap projection's incremental sync). Collapsed ranges
  map through it; the merge then keeps a range collapsed when it has the same start and size, and
  expands one a selection starts inside of (upstream's rule, which is how typing in a fold unfolds
  it).
- **Composition with word wrap (ADR-0036)**: hidden lines are part of the view-line projection.
  `ViewLines` gets hidden ranges; a hidden model line has no view lines, and the projection exists
  whenever lines wrap or are hidden, so rows, hit testing, scrolling, the minimap and cursor
  movement by view line all follow without a second layer. Without folds and without wrap there is
  no projection and the old row-equals-line path is unchanged.
- **Caret rules**: the caret never rests on a hidden line. Fold commands move it to the end of the
  header (`onHiddenRangesChanges`); cursor moves that land in a fold go past it or to the header end
  (`skip_hidden` in `move_to`); a selection that starts in a collapsed region (find, replace, undo)
  expands it (`revealCursor`).
- **Language rules**: the `folding` section of each language configuration (offSide and the marker
  regexes) is bundled into `languages.json` by `scripts/bundle-grammars`, together with the block
  comment tokens. `vsr_languages::bundled::folding_rules` and `block_comment` read them; a test
  compiles every bundled marker with the `regex` crate.
- **Strategy**: `editor.foldingStrategy` `auto` falls back to indentation because rscode has no
  language server folding range providers (`syntaxRangeProvider.ts` is not ported).
  `editor.foldingImportsByDefault` needs those providers and is blocked.
- **View** (`editor_view/folding.rs`): the chevron-down / chevron-right codicons in the gutter
  (`editor.showFoldingControls`: collapsed regions always, expanded ones with `always` or while the
  mouse is over the gutter), the `⋯` after a folded line (`editor.foldPlaceholderForeground`), the
  folded background (`editor.foldingHighlight`, `editor.foldBackground`), and clicks (Shift or
  middle: recursive, Alt: the surrounding regions; the `⋯` always unfolds, the empty space after it
  with `editor.unfoldOnClickAfterEndOfLine`). Commands and keys are in the registry under their
  VS Code ids: `editor.fold` Alt+Cmd+[, `editor.unfold` Alt+Cmd+], `editor.foldAll` Cmd+K Cmd+0,
  `editor.unfoldAll` Cmd+K Cmd+J, `editor.foldRecursively`, `editor.unfoldRecursively`,
  `editor.toggleFold`, `editor.foldLevel1` to 7, `editor.foldAllMarkerRegions`,
  `editor.foldAllExcept` and the go-to-fold commands. Not built: block comment folding, manual
  folding ranges.
- **Sticky scroll** (`editor_view/sticky_scroll.rs`): `Editor::sticky_lines` finds the regions that
  contain the first line the headers do not cover (iterating until the count settles), outermost
  first, at most `editor.stickyScroll.maxLineCount`. Each header is drawn as an ordinary editor row
  (`rows()`), so it has the gutter, syntax colours and fold control; a click moves the caret to it and
  scrolls it to the top; `scrollWithEditor` makes the rows follow the horizontal scroll. There is no
  outline provider, so `editor.stickyScroll.defaultModel` is read but all three models use the
  folding regions, which means sticky scroll needs `editor.folding` on.
- **Minimap section headers** (`section_headers.rs`, a port of `findSectionHeaders.ts` with its 19
  tests; `minimap/headers.rs`): region headers use the language's start marker, mark headers the
  `markSectionHeaderRegex` (named groups `label` and `separator`) and must be in a comment according
  to the tokenizer, with the closing block comment token removed. The title is drawn as a text label
  over its minimap line; `sectionHeaderLetterSpacing` stays blocked (gpui text has no letter spacing).
- **Settings**: `editor.folding`, `foldingStrategy`, `foldingHighlight`, `foldingMaximumRegions`,
  `showFoldingControls`, `unfoldOnClickAfterEndOfLine`, `editor.stickyScroll.*` (4) and
  `editor.minimap.showRegionSectionHeaders`, `showMarkSectionHeaders`, `markSectionHeaderRegex`,
  `sectionHeaderFontSize` are wired (`config/folding.rs`).

**Consequences.**
- The regions are recomputed on every buffer version change, one pass over the lines; a 10k-line
  file with a fold stays well under the 100 ms-per-key bound in the headless test.
- `Editor::view_lines()` is now `Some` for a folded file even without wrap; code that assumed
  row == line - 1 when it was `None` stays correct because folded files take the projection path.
- Folds are per open editor and are not restored when a tab is reopened (upstream's memento is not
  ported).

## ADR-0043 — Editor cursor, rendering, tabs and the blocked editor settings (2026-11-10)

Finishes the small editor groups `editor.cursor`, `editor.render`, `editor.tab`, `editor.font`,
`editor.guides`, `editor.mouse` and `editor.format` (`docs/settings-migration.md`). The ADR-0022 path
is unchanged: values come from `Configuration` into `EditorSettings`, the keys are in `config/wired.rs`.

**Wired:** `editor.cursorHeight`, `editor.cursorSurroundingLines` and `...Style`, `editor.renderControlCharacters`,
`editor.renderFinalNewline`, `editor.tabFocusMode`.

- `cursorHeight` centres the line caret. The caret element has a `caret:<line>` selector.
- `cursorSurroundingLines` replaces the `Nearest` scroll for a caret move with a strict scroll that keeps the
  margin (`decorations::scroll_top_with_margin`, pure). The top row is read from the scroll offset (or the
  pending scroll), because `logical_scroll_top` does not follow the uniform list. Keys always enforce the
  margin; a click enforces it only with style `all`. `after_change` takes `by_pointer` for that.
- `renderControlCharacters` replaces each ASCII control with a space (one byte each, so byte offsets and
  columns stay) and draws its Control Pictures glyph over the cell. The bidi formatting characters are
  multi-byte and are not drawn.
- `renderFinalNewline`: the empty line after a final line break; `off` hides its number, `dimmed` uses half opacity.
- `tabFocusMode`: Tab calls `window.focus_next`. rscode has no focus tab stops outside the editor, so the
  focus stays; the tab is not typed.

**Blocked, with reasons in `upstream/settings.toml`:**
- `mouseWheelZoom`, `mouseWheelScrollSensitivity`: gpui's list scrolls on the wheel before any editor
  listener (bubble phase only), so Cmd+wheel would also scroll. Needs a scroll capture on the surface.
- `guides.bracketPairs`, `bracketPairsHorizontal`, `highlightActiveBracketPair`: need a pair model over the
  whole document that skips string and comment brackets. ADR-0032's `matchBrackets` only finds the caret's pair.
- `cursorSmoothCaretAnimation`: needs a per-frame caret tween.
- `fontVariations`: gpui-pre applies OpenType features and weight, not variation axes.
- `mouseMiddleClickAction`: needs ctrl+click and links.
- `editor.format*`: need formatting providers from the extension host (ADR-0020).

**Not a setting we follow:** `renderRichScreenReaderContent` is n/a (only with `editor.editContext`).

**Testing.** Pure logic is unit-tested in `decorations.rs` and `settings.rs`. Effects are headless: `config/e2e.rs`
(control characters) and `editor_view/cursor_e2e.rs` (margins, click policy, caret height, Tab). The
checks are on boxes, scroll state and buffer text. Scenarios: `tests/e2e-scenarios/preferences.md`.

## ADR-0044 — Search and Explorer settings: the groups migrated, and what waits for features (2026-11-10)

**Context.** The `search` group had 15 todo settings and the `explorer` group 8 (`settings-port
next`). Several need features rscode does not have (the search editor, a Search tree view, notebooks,
AI search, drag and drop, multi-root workspaces, file decorations). Settings for those must not sit
as silent todo work or be skipped.

**Decision.** Each setting is either wired (read live, listed in `config/wired.rs`) or blocked with a
`[[rule]]` whose reason says what it needs (`upstream/settings.toml`). None is n/a: each is one rscode
would want.

- **Wired** (search): `search.ripgrep.maxThreads` (the walker threads of a search),
  `search.sortOrder` (all six orders; `search/model.rs` keeps the order and places later files by
  it, so a change re-sorts the listed files without a new search), `search.seedOnFocus` and
  `search.seedWithNearestWord` (Find in Files seeds the query; `search/model.rs::seed_text`).
- **Wired** (explorer): `explorer.excludeGitIgnore` (`explorer/gitignore.rs`: the `ignore` crate's
  matcher per `.gitignore` from the root to the folder; a deeper rule overrides a shallower one; the
  listed folders are read again on a change), `explorer.copyPathSeparator` and
  `explorer.copyRelativePathSeparator`, with two new commands, Copy Path (Alt+Cmd+C) and Copy Relative
  Path (Shift+Alt+Cmd+C). The commands copy the selected entry, or the active editor's file. There is
  no Explorer context menu yet, so they are palette entries and keys.
- **Blocked** (search, 11): `search.mode` (search editor), `globalFindClipboard` (macOS find pasteboard,
  shared with the Find widget), `defaultViewMode` (tree view), `actionsPosition` (its `auto` value
  needs the responsive row layout; the default `right` is what rscode draws), `quickAccess.preserveInput`
  (Quick Search picker), `experimental.closedNotebookRichContentResults` (notebooks),
  `experimental.useIgnoreFilesInFindFiles` (extension host findFiles API), `searchView.semanticSearchBehavior`
  and `keywordSuggestions` (AI search), `decorations.colors` and `badges` (file decorations).
- **Blocked** (explorer, 5): `enableDragAndDrop`, `enableUndo` (no undo stack for Explorer operations),
  `expandSingleFolderWorkspaces` (multi-root), `decorations.colors` and `badges`.

**Consequences.**
- `actionsPosition` is not "left" or "right": upstream accepts only `auto` and `right`. A value outside
  the schema falls back to the default, as for every enum.
- Tests: pure functions and the model are covered headless (`gitignore`, `copied_path_text`, the sort
  orders, `seed_text`, the settings parsing). The keys and the Find in Files focus path are not
  driven by a gpui test yet; `tests/e2e-scenarios/search.md` and `explorer.md` mark those rows
  `pending` or `unit`.
- `.gitignore` edits are seen when the folder is listed again. There is no watch on `.gitignore`
  files, and the global gitignore and `.git/info/exclude` are not read.

## ADR-0046 — Typing rules, comments and indentation: language configuration in the editor (2026-11-10)

**Context.** The `editor` settings group had 51 todo settings, most of them the typing rules every
editor has: auto closing of brackets and quotes, over typing, surrounding a selection, auto indentation,
Cmd+/ for comments, tab or space detection. All of them read the *language configuration*
(`language-configuration.json`: brackets, pairs, comments, indentation rules, on-enter rules), of which
rscode bundled only `folding` and the block comment tokens (ADR-0042).

**Decision.**

- **Data** (`scripts/bundle-grammars` -> `vsr-languages/data/language-configurations.json`): the
  editing keys of every built-in language's configuration, keyed by the `configuration` path (56 files,
  shared by the 83 languages). `vsr_languages::config` parses them (`LanguageConfiguration`; the regular
  expression keys are a string or `{pattern, flags}`), resolves one per language (`language_support(id)`:
  upstream's `ResolvedLanguageConfiguration`, with the plain text configuration upstream registers:
  brackets and surrounding pairs) and builds the supports: `CharacterPairSupport`, `OnEnterSupport`,
  `IndentRulesSupport`, plus `auto_indent` (ports of `autoIndent.ts`, `enterAction.ts` and the indentation
  line processor).
- **Regular expressions** are JavaScript's. `JsRegex` runs them on Oniguruma (the engine of the
  grammars, `onig`, Perl syntax: lookaheads work, which the Rust `regex` crate cannot do). A test compiles
  every bundled pattern. Two JS quirks are translated (`[]` and `[^]`); `onig`'s `Regex::is_match` matches
  the whole text, so `JsRegex::is_match` is a search.
- **Typing** (`vsr-editor/src/typing.rs`, ports of `cursorTypeOperations.ts` and
  `cursorTypeEditOperations.ts`): `Editor::type_text` is `typeWithInterceptors` one character at a time:
  auto indent on type (`decreaseIndentPattern`), over typing a closing character the editor added, auto
  closing (longest open text, `autoCloseBefore`, `notIn` string/comment/regex by the token before the caret
  and by the token type a neutral character would get, balanced brackets, no quote after a word
  character, contained pairs), surrounding a selection, electric characters, then the plain character.
  `enter()` is `EnterOperation._enter`: the onEnter rules (advanced and up), bracket rules (brackets and
  up), `getIndentForEnter` (full), else the line's indentation. Backspace deletes an auto closed pair.
  `editor.trimAutoWhitespace` records the lines an auto indent edit left with whitespace only and removes
  them with the next edit near the caret (one undo step with it). `editor.useTabStops`,
  `editor.indentSize` and `editor.trimWhitespaceOnDelete` change Tab, Backspace and Delete.
- **Dependency.** `vsr-editor` now depends on `vsr-languages` (ADR-0013 had it depend on `vsr-text`
  only). The editor stores an `Arc<LanguageSupport>`; the workbench sets it with the language
  (`OpenEditor::set_language`). Tokens come in through the `TokenSource` trait, passed per call, so the
  editor stays UI free: the app's `SyntaxTokens` reads the incremental tokenizer
  (`DocumentTokens::token_type_if_inserting` is new in `vsr-syntax`). A line the tokenizer has not reached
  gives `None`, and auto closing and auto indent then leave it alone (upstream's `isCheapToTokenize`).
- **Where typing rules apply.** A key press (`replace_text_in_range` with no range and no composition)
  goes through the rules; a composition (dead keys, IME) and a replacement of a given range insert the text
  as it is (`Editor::type_raw`), so composition cannot be auto closed.
- **Comments** (`vsr-editor/src/comments.rs`, ports of `lineCommentCommand.ts` and
  `blockCommentCommand.ts`): Toggle, Add and Remove Line Comment (`editor.action.commentLine`, Cmd+/;
  `addCommentLine`, Cmd+K Cmd+C; `removeCommentLine`, Cmd+K Cmd+U) and Toggle Block Comment
  (`editor.action.blockComment`, Shift+Alt+A). A language without a line comment uses the block
  comment. The selection follows the edits as upstream's tracked selection does (a point at an
  insertion moves after it, the end of a selection does not grow).
- **Indentation detection** (`vsr-editor/src/indentation.rs`, port of `indentationGuesser.ts`):
  `editor.detectIndentation` (default on) guesses tabs or spaces and the indent size when a file opens and
  when the settings change. `Editors::apply_editor_settings` applies tab size, spaces, detection and the
  typing options to the open editors; the status bar shows the guess.
- **Settings.** Wired (15): `editor.autoClosingBrackets`, `autoClosingQuotes`, `autoClosingComments`,
  `autoClosingDelete`, `autoClosingOvertype`, `autoSurround`, `autoIndent`, `useTabStops`,
  `trimAutoWhitespace`, `trimWhitespaceOnDelete`, `emptySelectionClipboard`, `comments.insertSpace`,
  `comments.ignoreEmptyLines`, `detectIndentation`, `indentSize`. Blocked or n/a, 45 (42 and 3), each with its reason
  in `upstream/settings.toml`: `autoIndentOnPaste*` (the paste re-indent contribution), `stickyTabStops`,
  `editor.language.*` (language scopes), `bracketPairColorization.*`, accessibility (4), `columnSelection`,
  `copyWithSyntaxHighlighting`, `dragAndDrop`, `fastScrollSensitivity`, `smoothScrolling`, glyph margin and
  overview ruler (3), `letterSpacing`, `fullwidthCharacterWidth`, `links`, `overtype*`, `padding.*`,
  `selectedTextMatchMode`, `smartSelect.*`, `unusualLineTerminators`, `wrapOnEscapedLineFeeds`,
  `doubleClickSelectsBlock`, `largeFileOptimizations`, the language-feature ones (peek, rename, code
  actions, linked editing, semantic token colours); n/a: `editContext`, `inertialScroll`, `aiStats.*`.

**Simplifications.** One cursor. Auto closed ranges are tracked by column through single-line
edits instead of decorations. A multi-character auto close (`/**` closing ` */`) is one undo step (upstream
makes an undo stop between the typed text and the auto closed text). Bracket balance counts the brackets
of the pair in 2000 lines around the caret instead of using the bracket pair model. One language per file
(no embedded languages: JSX in JavaScript, scripts in HTML). The legacy `docComment` of a language is not
read. `autoIndent: full` does not re-indent on Tab or on paste.

**Consequences.**
- Opening a tab-indented file now gives tabs (`detectIndentation` is on upstream); two existing tests that
  assumed a 4-space default for such files were updated.
- The language configuration files are 71 KB of JSON compiled into `vsr-languages`; the supports are built
  on first use per language and cached.
- Tests: 13 of 13 `characterPair.test.ts`, 4 of 4 `onEnter.test.ts`, 12 of 12 `blockCommentCommand.test.ts`,
  36 of 39 `lineCommentCommand.test.ts`, 8 of 8 of the guessing tests of `textModel.test.ts` (43 assertGuess
  calls) and 42 cases of `cursor.test.ts` (17 auto closing, 25 Enter, auto indent and electric characters);
  headless E2E in `config/e2e_typing.rs` (16 tests, every setting also checked off).

## ADR-0047 — Breadcrumbs: the file path bar, its dropdown and the focus keys (2026-11-10)

**Context.** The `breadcrumbs.*` group has 33 settings. Their upstream meaning is the file path
above the editor plus the symbol path from the document symbol providers (the outline). The
symbol providers come from the extension host (ADR-0020), so the symbol part cannot work yet.

**Decision.** Build the file path part now and block the rest.

- **Bar.** A 22px row under the tab strip (`crates/vsr-app/src/breadcrumbs/`), drawn for the active
  text editor only (not the Settings editor or the Welcome page). Crumbs are the folders and the file
  name below the workspace folder, which is not a crumb (upstream does the same for one folder). A
  file outside the folder shows its absolute path, as upstream does. The bar hides when there are no
  crumbs. Colours: `breadcrumb.background`, `foreground`, `focusForeground` (the keyboard crumb),
  `activeSelectionForeground` (the crumb whose dropdown is open).
- **Model.** `breadcrumbs/model.rs` is UI-free: `file_segments` (the three `filePath` modes),
  `list_children` (folders first, then by name, `files.exclude` applied) and the settings.
- **Dropdown.** Clicking a crumb opens the Quick Pick (`quick_input.rs`, ADR-0028), not a new widget.
  It lists the crumb's siblings (the parent folder's entries) with the crumb selected. A folder
  accepted lists its own entries in place; a file accepted opens (as Quick Open does) and closes it.
  This is the reuse the theme picker also makes.
- **Keys.** `breadcrumbs.focus` (Shift+Cmd+;) and `breadcrumbs.focusAndSelect` (Shift+Cmd+.) focus
  the last crumb; Left and Right move; Enter opens its dropdown; Escape gives focus to the editor.
  `breadcrumbs.toggle` flips `breadcrumbs.enabled` (palette, no key, as upstream).
- **Wired:** `breadcrumbs.enabled`, `breadcrumbs.filePath`, `breadcrumbs.icons`.
- **Blocked** (`upstream/settings.toml`, reason "needs document symbols"): `symbolPath`,
  `symbolSortOrder`, `symbolPathSeparator`, and every `breadcrumbs.show<Kind>`. `showFiles` is in
  this set on purpose: upstream applies it to the file symbol in the outline, so it does nothing
  without symbols. `showEditorType` is blocked on custom editors.

**Consequences.**
- The bar sits under the tab strip whatever `workbench.editor.showTabs` says, and it is not
  anchored under the crumb (the dropdown is the centred Quick Pick).
- Single-group, single-window only, like the rest of the editor.
- The dropdown's rows come from a synchronous directory read on each change. A folder with a very
  large number of entries is not paged.
- `Shift+Cmd+;` and `Shift+Cmd+.` are matched by gpui on the unshifted key; the headless tests drive
  them through `simulate_keystrokes`, so a real keyboard layout is not verified.

**Testing.** `breadcrumbs/model.rs` unit tests (segments inside and outside the folder, `last`, `off`,
children order and `files.exclude`). `breadcrumbs/e2e_tests.rs` (headless, `docs/testing.md`): the path
shows, a click lists the siblings, a folder opens in place, a file opens, `filePath` last and off,
`enabled` false, the colour theme, and the keys. The scenario rows are in
`tests/e2e-scenarios/workbench.md`.

## ADR-0048 — Explorer sort options, auto-reveal exclusions and the small P1 settings groups (2026-11-10)

**Context.** The P1 groups `explorer.sort`, `explorer.auto`, `explorer.confirm`, `editor.default`,
`editor.scroll`, `workbench.sash`, `workbench.commandPalette`, `search.use`, `search.quickOpen` and
`outline` / `search.searchEditor` were the next in `settings-port next`. Most of their settings need a
missing feature, so the policy has to say what each one waits for.

**Decision.**
- **Wired**: `explorer.sortOrderLexicographicOptions` and `explorer.sortOrderReverse`
  (`explorer/sort.rs`; reverse swaps the two entries before comparing, as upstream does, so folders
  move too). `explorer.autoRevealExclude` (`explorer/view.rs::reveal_excluded`: the file and each folder
  up to the root are matched, a `when` clause checks the siblings on disk; the default excludes
  `node_modules` and `bower_components`). `workbench.sash.size` (the side bar sash's drag area, clamped
  to 1 to 20 pixels).
- **Blocked** (each names what it waits for): `explorer.autoOpenDroppedFile` and
  `explorer.confirmDragAndDrop` (drag and drop), `explorer.confirmPasteNative` (paste),
  `explorer.confirmUndo` (undo), `editor.defaultColorDecorators` (colour decorators),
  `editor.defaultFormatter` and `editor.defaultFoldingRangeProvider` (extension host providers),
  `editor.scrollPredominantAxis` and `editor.scrollOnMiddleClick` (the editor surface must capture the
  wheel and the pointer), `outline.*` (document symbols), `search.searchEditor.*` (the Search Editor),
  `search.useReplacePreview` (Replace Preview), `search.quickOpen.includeSymbols` (workspace symbols),
  `workbench.commandPalette.experimental.suggestCommands` (a suggested list),
  `workbench.sash.hoverDelay` (configurable hover delay) and `autoHideGrippers` / `autoHideGrippersDelay`
  (sash grippers).
- **n/a**: `workbench.commandPalette.experimental.enableNaturalLanguageSearch`, `askChatLocation` and
  `showAskInChat` (AI chat and search, the product policy).

**Consequences.**
- The explorer sort's `Upper` and `Lower` options approximate upstream's collator `caseFirst`: they
  break ties between names that differ only in case, not the full collation. `Unicode` is code point
  order.
- `workbench.sash.size` changes the drag area; the 1px border stays where it is. The default of 4
  replaces the hard-coded 5 px drag area, to match upstream.
- Not done in this change: `editor.scrollBeyondLastColumn` (needs the widest row to carry trailing
  columns), `workbench.activityBar.*` (autoHide, compact, iconClickBehavior),
  `explorer.fileNesting.*` (the nesting trie and the nested rows) and `explorer.openEditors.*` (the
  Open Editors section). They stay `todo` and are the next work in these groups.
- Tests: `explorer/sort.rs` unit tests, the Explorer e2e tests (`sort_reverse_and_lexicographic_options_follow_settings_live`,
  `auto_reveal_exclude_skips_matching_paths_live`, `sash_size_sets_the_drag_area_live`) and the
  `LayoutSettings` clamp test. The sash test reads the drag area's laid-out width through a debug
  selector.

## ADR-0049 — The default dark theme is Dark 2026, as upstream (2026-11-10)

**Context.** ADR-0037 kept Dark Modern as rscode's default colour theme, because it was the colour set the
app painted before the theme service existed. Upstream 1.147 defaults `workbench.colorTheme` to Dark 2026
(`ThemeSettingDefaults.COLOR_THEME_DARK` in `workbenchThemeService.ts`). The theme file was already bundled
(`2026-dark.json`, which `include`s Dark Modern and overrides its colours), so no porting was needed.

**Decision.** `vsr_theme::DEFAULT_DARK` is `"Dark 2026"`. The colour mode fallbacks (`ThemeSettings::resolve`,
`workbench.preferredDarkColorTheme`) follow it. `Theme::default_dark()` is the default without
customisations; `Theme::dark_modern()` keeps meaning the bundled Dark Modern, for the legacy colour test.

**Light default (amended the same day, the user's call: "match upstream").** `vsr_theme::DEFAULT_LIGHT` is
`"Light 2026"` (`ThemeSettingDefaults.COLOR_THEME_LIGHT`; `2026-light.json` was already bundled, and the
`workbench.preferredLightColorTheme` schema default already said Light 2026). It is what a light system
picks under `window.autoDetectColorScheme` when no preferred light theme is set, and the fallback for an
unknown one. Test: `a_light_system_defaults_to_light_2026_with_the_colours_of_its_json`. The high contrast
defaults are unchanged (already upstream's).

**Tests.** The default-theme golden test is `the_default_theme_is_dark_2026_with_the_colours_of_its_json`.
Tests that check Dark Modern's token colours pin `workbench.colorTheme` to Dark Modern in their settings file
instead of depending on the default.

## ADR-0050 — Activity bar: compact layout and icon click behaviour (2026-11-10)

**Context.** The Activity Bar group (`workbench.activityBar.*`) had three todo settings. `location` was wired
already. Upstream 1.147 defines the compact layout in `ActivitybarPart` (default 48px wide with 48px items and
24px icons; compact 36px, 28px, 16px) and the click behaviour in `paneCompositeBar.ts`
(`iconClickBehavior`, `toggle` hides the side bar when the shown view's icon is clicked, `focus` keeps it open).

**Decision.**

- `workbench.activityBar.compact` applies to location `default` only (as its description says). The bar and
  its items take the compact sizes (`metrics::ACTIVITY_BAR_COMPACT_*`); the layout's width and the sash
  drag (`activity_bar_width`) use the same width.
- `workbench.activityBar.iconClickBehavior: focus` keeps the side bar open when the shown view's icon is clicked.
  rscode does not move keyboard focus into the view on that click yet (upstream focuses the view); it is a
  follow-up with the focus handles of the side bar views.
- `workbench.activityBar.autoHide` is `blocked`: it hides a horizontal bar when one view container is
  shown, and rscode always has four.
- The activity icons carry a debug selector `activity:<n>` (headless tests click them).

**Tests.** `config/e2e.rs`: `activity_bar_compact_narrows_the_bar_live`,
`activity_bar_icon_click_focus_keeps_the_side_bar_open`. The upstream test titles are cited in comments; the
unit ports (`activitybarPart.test.ts`) are not ported, so the references are "Upstream source" comments.


## ADR-0051 — Sash settings: size wired, grippers blocked (2026-11-10)

**Superseded by ADR-0056** (the grip is drawn; `autoHideGrippers` and `autoHideGrippersDelay` are wired).

**Context.** The sash group had `workbench.sash.size` (wired in ADR-0048), `hoverDelay` (blocked: gpui tooltips
use a fixed delay) and `autoHideGrippers` / `autoHideGrippersDelay`. Upstream's `SashGrippersController` (sash.ts)
hides the grippers, the dots drawn in the middle of a sash, after the mouse has been idle for the delay, and
shows them again on a mouse move.

**Decision.** `autoHideGrippers` and `autoHideGrippersDelay` are `blocked` (needs sash grippers). rscode's
sashes are 1px borders that take the hover colour (`sidebar-resize` in `main.rs`); no dots are drawn, so there
is nothing to fade. Drawing grippers is a visual feature of the sash itself, and it lands first. Until then the
settings have no effect, which is what the rule says.

## ADR-0052 — File nesting: the trie, and nests in the tree (2026-11-10)

**Context.** `explorer.fileNesting.enabled`, `expand` and `patterns` were todo. Upstream nests related files
under a parent (`a.ts` > `a.js`, `package.json` > `package-lock.json`). The matcher is
`explorerFileNestingTrie.ts`, a pair of tries over pattern characters with `${capture}`, `${basename}`,
`${extname}` and `${dirname}` substitutions.

**Decision.**

- `explorer/nesting.rs` is a line-by-line port of the trie (`SufTrie`, `PreTrie`, `ExplorerFileNestingTrie`) with
  the 26 upstream tests as `// upstream:` refs. Both `$(x)` and `${x}` substitutions work. The `is fast` test
  is skipped upstream and not ported.
- A parent pattern that ends in `*` is malformed; upstream throws. Here it is ignored, so a bad user pattern
  cannot break the tree.
- `explorer.fileNesting.patterns` is read as an object (parent → comma-separated children). Empty parents
  or children are dropped, as upstream does. The default rules are `nesting::default_rules()`, the upstream
  defaults; `ExplorerSettings::default()` and the configuration agree (a test proves it). The object is
  read in key order of serde_json, which is sorted; nests are the same, but a child matched by two
  parents may be listed under either one.
- The tree (`explorer/view.rs`): nested files are not listed in their folder. They follow their parent row,
  one level deeper. A parent with nests shows a chevron; a click on it toggles the nests and does not open
  the file. `expand` is the initial state; `ExplorerState::nests_toggled` holds the parents toggled away
  from it. The nests come from the folder's file names, per listing, on every paint (cheap for the
  patterns we have).
- Not ported: upstream's tree model (`nestedParent`, `nestedChildren`) and nested files sorting with the
  parent. Keyboard navigation uses the flat rows, so arrows move through nests in display order.

## ADR-0053 — Editor scrollbar settings: blocked until rscode draws an editor scrollbar (2026-11-10)

**Context.** The `editor.scrollbar.*` group (six todo settings, P2) describes the scrollbars of the editor:
their visibility, their sizes, what a click on the track does, and whether the horizontal bar adds to the
content height. Before deciding how to port them, we looked at what rscode draws. The editor's list is a gpui
`uniform_list` that scrolls by wheel and keys (`editor_view.rs`); gpui-pre reserves layout space for a
`scrollbar_width` but this crate paints no scrollbar, and rscode has no thumb (the only slider it draws is the
minimap's, ADR-0042). Upstream's `scrollableElement` draws both bars.

**Decision.** The six settings are `blocked` (needs an editor scrollbar). The reason is the missing scrollbar,
not the settings: there is nothing to size, hide or click yet. Building one is a visual feature of its own
(thumb, track, drag, visibility modes), so it is scheduled as such. Choosing `blocked` over `todo` is deliberate:
no half-drawn scrollbar is shipped to satisfy the settings, and the policy is recorded here.

**Not done.** `scrollbar_width` was not used to size the bars: gpui reserves the space but draws nothing in it,
so the setting would change the layout without a visible bar.

**Superseded by ADR-0054.** The editor draws its scrollbars now, and five of the six settings are wired.

## ADR-0054 — Editor scrollbars: the bars are drawn, five of six `editor.scrollbar.*` settings wired (2026-11-10)

**Context.** ADR-0053 blocked the `editor.scrollbar.*` group until rscode drew an editor scrollbar. Upstream's
`scrollableElement` draws both bars and their slider math lives in `scrollbarState.ts`.

**Decision.** The editor draws a vertical bar (the rightmost column, after the minimap) and a horizontal bar
(under the text area), from the list's scroll handle: the viewport is the last paint and the scroll range is gpui's
`max_offset`, so the slider follows the real scroll range, including the `scrollBeyondLastLine` and
`scrollBeyondLastColumn` room. The slider math is `editor_view/scrollbar/state.rs`, a port of `scrollbarState.ts`
(both upstream `ScrollbarState` tests ported). A click on the track jumps the slider's centre to the click, or pages
one viewport with `editor.scrollbar.scrollByPage`; dragging the slider moves the list by the pointer's delta.
`editor.scrollbar.vertical` and `horizontal` (`auto` shows the bar when the content overflows, `visible` always,
`hidden` never), `verticalScrollbarSize` and `horizontalScrollbarSize` (the bar's thickness, and the room it takes
from the text area) and `scrollByPage` are wired. Colours are `scrollbarSlider.background` and `hoverBackground`.

**Not done.**
- `editor.scrollbar.ignoreHorizontalScrollbarInContentHeight` stays `blocked`: upstream adds the horizontal bar's
  height to the scroll height, and rscode's list measures whole rows, so that would need a spacer row.
- The arrow buttons (arrowSize is 0), the fade after scrolling (`scrollbarVisibilityController`) and
  `scrollbarSlider.activeBackground` (gpui-pre's element has no active state here).
- `hidden` takes no width, where upstream's layout keeps `verticalScrollbarSize` reserved (`editorOptions.ts`).
- The drag is a gpui window listener, so it follows the pointer anywhere in the window, not through the bar's
  own pointer capture.
- Overview ruler decorations: see ADR-0055.

**Superseded in part by ADR-0055.** The vertical lane is now reserved whenever `editor.scrollbar.vertical` is not
`hidden` (upstream's layout does this), so an `auto` bar that is not needed still leaves its column, without a slider.

## ADR-0055 — Overview ruler: cursor, selection and find marks on the scrollbar lane; border and cursor settings wired (2026-11-10)

**Context.** Upstream's overview ruler draws the cursor, selection and find-match marks down the lane beside the
vertical scrollbar, at the height of their line in the whole document. The zone placement is
`overviewZoneManager.ts`, pure math with its own tests. `editor.overviewRulerBorder` and
`editor.hideCursorInOverviewRuler` were blocked with the glyph margin (ADR-0053's neighbour rule).

**Decision.**
- The lane is the vertical scrollbar's track: reserved whenever `editor.scrollbar.vertical` is not `hidden`, as upstream
  reserves `verticalScrollbarSize` in its layout (`editorOptions.ts`). This supersedes the "no column for an `auto` bar
  that is not needed" part of ADR-0054, so a short document keeps the lane, and its cursor mark shows.
- `editor_view/overview_ruler/zones.rs` ports `overviewZoneManager.ts`, and all three of upstream's
  `OverviewZoneManager` tests are ported (the DOM width they set changes no zone, so it is not modelled).
- Marks: the cursor (unless `editor.hideCursorInOverviewRuler`), the selection when it is not empty, and every find
  match (while the find widget is open). Colours: `editorCursor.foreground`,
  `editorOverviewRuler.selectionHighlightForeground`, `editorOverviewRuler.findMatchForeground`.
- `editor.overviewRulerBorder` (default on) draws a one-pixel border along the lane, in `editorOverviewRuler.border`.
- Both settings are wired. `editor.glyphMargin` stays blocked (the margin is not drawn).

**Not done.**
- Marks use document lines. With folding, upstream's view lines would put them elsewhere, so they can sit off their
  line in a folded document.
- Multi-cursor and bracket-match zones; error and warning zones (rscode has no diagnostics yet); the lane's own
  background colour.
- With `editor.scrollbar.vertical: hidden` there is no lane, so no ruler. Upstream keeps the reserved column and draws
  the ruler in it; this stays a deliberate difference, as in ADR-0054.

## ADR-0056 — Sash grippers: the grip is drawn on the sidebar sash and recedes when the mouse is idle (2026-11-10)

**Context.** `workbench.sash.autoHideGrippers` and `autoHideGrippersDelay` (ADR-0051) describe the gripper dots of a
sash, which fade when the mouse is idle (upstream `SashGrippersController` in `sash.ts`, with the settings registered
in `sash.contribution.ts`). rscode has one sash, the sidebar's, a 1px border with a hover colour.

**Decision.** The sidebar sash draws a grip: a 3 by 24 pixel rounded bar, centred on the sash, in `sash.hoverBorder`.
With `autoHideGrippers` on, a mouse move shows it again and it recedes after `autoHideGrippersDelay` milliseconds
(default 2000) of mouse idle; each move restarts the timer, so an earlier timer cannot recede it. With the setting
off the grip stays visible. Both settings are wired (`config/settings.rs` `LayoutSettings`, `main.rs` `sash_mouse_activity`).
The E2E tests run the idle timer on the test clock (`config/e2e_sash.rs`).

**Not done.**
- Upstream starts its idle timer when the window opens, so the grip recedes without any mouse movement. Here it
  stays until the first mouse move, then recedes after the delay.
- Upstream also keeps the grippers of hovered sashes in their hover state; here only mouse movement keeps the grip.
- Mouse movement is taken from the workbench root, not from nested webview frames (upstream's webview event).
- rscode has no other sash, so the grip is drawn only on the sidebar's.


---

## ADR-0057 — Untitled text editors, step 1: the rules are pure functions in `vsr-editor` (2026-11-10)

**Context.** The dependency tree ranks untitled editors first (11 settings, +6 transitively). Upstream
keeps untitled text in a service with a model per `untitled:` resource; its name, dirty state and tab
title are rules over the text. rscode's open editor owns the buffer, so a second copy of the text in a
model would have to stay in step with every edit path.

**Decision.** `vsr_editor::untitled` holds the rules and nothing else: `next_name` (the lowest free
`Untitled-N`), `is_dirty` (an untitled text is dirty when it has text; empty is clean), `new_language`
(explicit language, then `files.defaultLanguage` with `${activeEditorLanguage}`, then plain text),
`first_line_name` and `label` (the tab title from the first line under `labelFormat: content`, else
`Untitled-N`), and `LabelFormat`. The workbench asks them on each read. The untitled identity and the
editor wiring are ADR-0058.

**Ported.** Five upstream tests in `untitledTextEditor.test.ts` (dirty and language rules), in
`crates/vsr-editor/tests/upstream_untitled_text_editor.rs`. The first-line title rules are not an
upstream test; they are unit tests next to the code, with `Upstream source` in the module header.

**Not modelled.** Working-copy registration and events, encodings, backups as working copies, the
associated-file-path case (save-as makes a file, so the untitled rules stop applying), and
`getCharContainingOffset` (the title is cut at 40 chars, not grapheme clusters).

**Consequences.** `files.defaultLanguage` and `workbench.editor.untitled.labelFormat` can be wired
once the workbench creates untitled editors (ADR-0058, ADR-0059).

## ADR-0058 — Untitled text editors in the workbench: keys, Save As through the name box, hot exit (2026-11-10)

**Context.** File: New Untitled Text File (Cmd+N) and a dirty untitled text need an editor. The open
list, MRU, preview and history are keyed by `PathBuf` (ADR-0033, ADR-0039), and the session keys its
backups by path (ADR-0040). Changing the key type would touch the status bar and the editor view,
which other work edits at the same time. rscode also has no file dialog, and tests must not open one.

**Decision.**

- **Identity.** An untitled text is an `OpenEditor` whose path is the relative key `untitled:Untitled-N`
  (`model::untitled_key`). A relative path never names a file, so no file operation can hit it by
  accident, and every path-keyed list keeps working. `OpenEditor::is_untitled` reads the key.
- **Dirty and title.** `OpenEditor::is_dirty` is `vsr_editor::untitled::is_dirty` for an untitled text
  (ADR-0057). Its tab title is computed on each read by `Editors::title_of` (the first line under
  `labelFormat: content`), so it follows typing without a cache.
- **Creation.** `Editors::open_untitled` takes the lowest free `Untitled-N` and opens the text pinned.
  `Workbench::new_untitled` (Cmd+N, the palette, and `workbench.startupEditor` in ADR-0059) chooses the
  language with `new_language`. The language is not explicit, so the untitled text keeps it across
  `files.associations` changes (`apply_associations` skips untitled editors), and Save As detects it
  from the new name.
- **Save As.** Save (Cmd+S, the palette or the Save bar of Close) on an untitled text opens the
  Explorer's name box at the workspace folder (`InlineKind::SaveUntitled`), with the tab's title typed
  in and its name part selected. Enter or a valid blur commits (`save_untitled_in`), which
  refuses an existing file and otherwise writes the text through the `files.*` save transforms
  (`Editors::save_untitled_as`). The editor then follows the file: the open list, MRU and preview are
  renamed, the history gets the file, and the backup under the key is removed (the text is saved).
  Tests call the commit with the path they choose; no dialog is involved. Save on a dirty untitled
  text from the Close prompt closes its tab once the file is written (`session.close_after_save`); a
  plain Save leaves the tab open. The prompt gives the name box to its first untitled text only; the
  others stay open and dirty.
- **Session.** An untitled text is never saved in the tab list (its key is not a file). Its text is
  kept by hot exit as a backup under its key (`Backup.path` is the key, `base` is `None`). On restart
  `restore_backup` brings it back as an untitled text with its name, and `restore_untitled` places it
  after the restored tabs. The backup did not record a language, so a restored text was plain text (ADR-0074
  records it, and the place).
- **Missing files.** `close_missing` never closes an untitled text (it has no file), and autosave never
  saves one (upstream does not either: it needs a name).

**Consequences and what is not done.**

- The name box is the Save As flow. Superseded by ADR-0074: it opens in the folder of the active file,
  suggests the language's extension, and a blur cancels it (it no longer saves to the typed name).
- Save All when quitting with a dirty untitled text reported a save error. Superseded by ADR-0074: each
  untitled text gets its name box in turn.
- A clean empty untitled text is not restored after a restart (its backup is removed when it is
  empty). The position of a restored untitled text is kept (ADR-0074).
- Working-copy events, encodings and `files.encoding` for untitled texts are not modelled (ADR-0057).
- Search editor, binary and custom editors stay blocked (`upstream/settings.toml`).

## ADR-0059 — Untitled settings: labelFormat, defaultLanguage and newUntitledFile wired; the rules split (2026-11-10)

**Context.** With untitled texts (ADR-0058) three settings have an effect that a user can see:
`workbench.editor.untitled.labelFormat` (the title of an untitled text), `files.defaultLanguage` (the
language of a new one) and the `newUntitledFile` value of `workbench.startupEditor`. They were blocked
by a shared rule ("needs untitled, binary, custom and large-file editors") and by a rule for
`files.defaultLanguage` ("needs untitled editors (new files)"). The binary, custom and large-file
editors are still missing, and the language detection service is still missing.

**Decision.**

- `workbench.editor.untitled.labelFormat` is read into `EditorGroupSettings::untitled_label_format` and
  used by `Editors::title_of` (ADR-0057's rule). Wired in `wired.rs`, "Editor management".
- `files.defaultLanguage` is read by `Workbench::open_untitled_text` through `new_language`, with
  `${activeEditorLanguage}` meaning the active editor's language. Wired in "Languages". Its rule is
  deleted, not kept as an alias.
- `workbench.startupEditor: newUntitledFile` opens an untitled text when nothing is restored
  (`Workbench::open_untitled_text`). The key was already wired; the value now does something.
- The untitled rule in `upstream/settings.toml` is split. The five binary, custom and large-file
  settings stay blocked with the reason "needs binary, custom and large-file editors". The language
  detection rule stays blocked, with a comment that names what is still missing. The untitled
  workspace rule (`window.confirmSaveUntitledWorkspace`) is unchanged: it needs multi-root workspaces.
- `files.eol` also applies to a new untitled text. It was already wired, so the note in `wired.rs`
  changes and a headless test proves the saved file has CRLF (scenario 16 in `files.md`).

**Counts** (`scripts/settings-port status`, 2,062 settings): wired 183 → 185 (+labelFormat,
+defaultLanguage); blocked 344 → 342 (the two rules' settings left blocked). Todo, n/a and exthost are
unchanged. `workbench.editor.untitled.hint` is not in this VS Code export, so there is nothing to wire.

**Tests.** Headless, in `untitled_e2e.rs`: labelFormat `name` (and the default), `files.defaultLanguage`
(a named language, and `${activeEditorLanguage}`), `workbench.startupEditor: newUntitledFile`, and
`files.eol`. Scenarios 3 to 6 and 15 to 16 in `tests/e2e-scenarios/files.md`.

**Not done.** `files.defaultLanguage` does not change a language after content detection (none exists).
The default file extension of the Save As name box is not suggested from the language.

## ADR-0060 — Extension host spike (P0): Option A is feasible, the handshake works (2026-11-10)

Context: ADR-0020 chose Option A (run upstream's own compiled `extensionHostProcess` under Node,
Rust on the main-thread side) and said to prove the Code-OSS build through the proxy first, with the
Option C shim as the fallback. This is that proof.

**Verdict: Option A is feasible. No shim is needed for P0.** What was done, in a fresh folder outside
the repo (`git archive` of the pinned `50e1350fc78d`, so `vscode-original` was never built in place):

1. Upstream's `packageManager` is `pnpm@12.16.0` and its `.pnpmfile.mjs` imports a `.ts` file, so the
   machine's pnpm 10.26 failed and so did Node 24.11 (the pnpmfile hook died with "Broken pipe").
   `npm i pnpm@12.16.0` into a separate tools folder (through the configured registry, no config
   change) plus Node 24.22.1 (upstream's `.nvmrc`, already installed by mise) fixed both.
2. `pnpm install --ignore-scripts` at the root: the proxy answered **HTTP 403 for
   `@github/copilot-sdk@1.0.30-preview.0`** (the registry's release-age quarantine of ADR-0007/0012).
   I did not bypass it. The package is only imported by the Copilot agent code and its tests
   (`grep -rl '@github/copilot-sdk' src` lists only `src/vs/platform/agentHost/test/**`), not by the
   extension host, so the scratch copy's `package.json` drops that one dependency. Everything else
   (1,469 packages) installed; so did the separate `build/` install root (`--ignore-workspace`).
3. `node build/next/index.ts transpile` (esbuild) wrote `out/` in 13.5 s: 11,336 files, 351 MB.
   No gulp, no full `compile`, no Electron.
4. Launch: `node out/bootstrap-fork.js --type=extensionHost` with
   `VSCODE_ESM_ENTRYPOINT=vs/workbench/api/node/extensionHostProcess` and
   `VSCODE_EXTHOST_IPC_HOOK=<unix socket>`: the host connects to a socket we listen on (the
   `IPCExtHostConnection` branch of `extensionHostProcess.ts`), which is what a Rust listener wants.
   First bytes on the wire: `Resume` (type 8) then a regular frame id 1 holding `[2]` (`Ready`), then
   a `KeepAlive`.

**What works end to end** (`crates/vsr-rpc/tests/handshake_real_host.rs`, headless, no window):
`Ready` -> `IExtensionHostInitData` (JSON, built by Rust) -> `Initialized`; the host then sends 13
requests to 9 `MainThread*` proxies (`MainThreadLogger.$registerLogger`, `MainThreadCommands.$registerCommand`,
`MainThreadTask`, `MainThreadSearch`, `MainThreadFileSystem.$ensureActivation`, ...), our Rust side
answers every request (`Acknowledged`, then a reply), and the proxy ids resolve through the generated
table (ADR-0061). The other direction works too: requests we encode (JSON args, a buffer argument in
the mixed layout) reach the real `ExtHostExtensionService` (`$test_latency`, `$test_up`, `$test_down`
round-trip with JSON and buffer replies), and `Terminate` makes the host exit cleanly (code 0).
Nothing the host does at startup reports `$onUnexpectedError` once `$getInitialState` and `$getTools`
get real replies (an `undefined` reply to those makes the host log an unhandled rejection).

**What is not proven, and what to know:**
- No native module was built (`--ignore-scripts`): the host logs `Could not locate the bindings file`
  for `@vscode/spdlog` and carries on (file logging is off). `@vscode/native-watchdog` is only
  required when `parentPid != 0`; we send 0. Real native builds target Electron
  (`nativeBuild.runtime = electron` in `package.json`); P0 runs under plain Node, which is the plan.
  Native modules extensions need (`node-pty`, `@vscode/ripgrep`, watchers) come with P1+.
- Only `out/` of the transpile is used: no bundled `extensionHostProcess`, so start-up reads ~11k small
  files. Fine for a spike; production wants the bundle (`build/next/index.ts bundle`) or a pruned copy.
- The Code-OSS output is a developer artifact: 2.5 GB with `node_modules` (1.4 GB root + 0.37 GB
  `build/`). A release needs the pruned form and a decision on where it lives (P1 follow-up).
- No extension was loaded: the init data has an empty extension list. Loading ErrorLens is P1.
- `$startExtensionHost` did not answer in a probe with no extensions and `autoStart: true`; not
  investigated (the host starts itself), noted for P1.

**Reproducing:** `scripts/build-codeoss-exthost [DEST]` (re-run end to end into a second fresh folder, then `handshake_real_host` passed against it) runs the steps above (it only ever writes to a
fresh folder), then `RSCODE_CODE_OSS=<DEST> cargo test -p vsr-rpc --test handshake_real_host`. Without
`RSCODE_CODE_OSS` that test skips; `handshake_fake_host.rs` (a Node stand-in written independently of
the Rust code) always runs where `node` is on PATH and skips otherwise.

**Consequences.** ADR-0020's roadmap stands; the proxy-quarantine risk it listed is real but small (one
unrelated package). Option C stays unused. The next risk is not the build but the size of the
`MainThread*` surface (89 shapes): P1 should implement only what ErrorLens triggers, in the order the
host asks for it, with the recording handler as the guide.

## ADR-0061 — `vsr-rpc`: framing, codec, id table and handshake driver (2026-11-10)

New crate `crates/vsr-rpc`, UI-free, no async runtime, no new dependencies (only `serde_json`, already
in `Cargo.lock`: the cargo proxy token is not available to agents, HANDOFF.md item 1).

- **`framing`**: the 13-byte-header frames of `PersistentProtocol` (`ipc.net.ts`): `TYPE u8 | ID u32be
  | ACK u32be | LEN u32be`. `FrameReader` cuts a byte stream into frames; `Protocol` is the sans-IO
  state machine: numbering of regular frames, our ack piggybacked on every regular frame (the host
  times out an unacknowledged send after 20 s otherwise), the unacknowledged queue, replay on gap or
  request, `Pause`/`Resume`. Timers (ack after 2 s, keep-alive, reconnection grace) are left to the
  driver; `ack_due()` / `make_ack()` are the hooks. Only the plain-socket form is implemented (the
  extension host's IPC pipe), not the WebSocket framing.
- **`codec`**: `MessageBuffer` / `MessageIO` of `rpcProtocol.ts`: JSON-args and mixed-args requests (with
  and without the cancellation flag), `Acknowledged`, `Cancel`, the five reply kinds and the two error
  kinds. `Arg` has `Json`, `Buffer`, `Undefined` and `ObjectWithBuffers`; the layout choice is
  upstream's (`_useMixedArgSerialization`). `$$ref$$` substitution of buffers inside objects is not
  done (the JSON and the buffer list stay apart) until a shape needs it.
- **`rpc`**: `RpcPeer` numbers our requests, decodes incoming messages, acknowledges and answers
  requests through a synchronous `Handler` (like `_receiveRequest`: ack first, then the reply; errors as
  `{ $isError, name, message }`), and reports replies to our requests. Synchronous handlers are a P0
  simplification: a handler that waits for the UI will be driven from the app's event loop (P1 decides
  how the `MainThread*` actors reach `vsr-app` state; a channel of `Call`s with a reply slot is the
  likely shape).
- **`proxy_ids`**: generated by `scripts/gen-rpc-ids` from `extHost.protocol.ts`. Upstream numbers a
  proxy by construction order (`ProxyIdentifier.nid = ++count`), `MainContext` then `ExtHostContext`
  (89 + 82 = 171 ids; one byte on the wire). The generator fails if a `createProxyIdentifier` call is
  outside the two contexts, so the numbering cannot silently drift. Cross-checked once against the
  live `nid` values of the transpiled module: identical. Only ids are generated, not the shape
  interfaces (method lists); those are written per feature. Re-run on every upstream bump
  (`upstream/ports.toml`: `exthost-proxy-ids`; `scripts/gen-rpc-ids --check` for CI).
- **`handshake`**: `Host::spawn` (listen on a Unix socket, run Node, accept), `handshake` (Resume, wait
  for `Ready`, send init data, wait for `Initialized`), `request` (call a proxy on the host and wait),
  `serve` (answer the host's requests for a while), `terminate` (`Terminate` message, then kill after a
  grace). `init_data` builds the minimal `IExtensionHostInitData`: no extensions, `parentPid: 0` (turns
  off the parent poll and the native watchdog), no `commit` (skips the version check), no remote.
  Blocking I/O with read timeouts and two pump threads for the child's output.

Tests (all headless): ported `ipc.net.test.ts` (read/write, object data, acks piggybacked, ack after
a while as `ack_due`) and `rpcProtocol.test.ts` (simple call, without result, buffers both ways, error
kinds, undefined arguments, objects with buffers) as round trips between two `RpcPeer`s; wire-layout
tests with exact bytes; truncated and unknown input returns errors (no panics); the id table pinned;
the real-host handshake (needs `RSCODE_CODE_OSS`) and the Node stand-in handshake (needs `node`).
Both integration tests skip with a printed reason, they do not fail. Registered in `upstream/ports.toml`
as `exthost-rpc-wire`, `exthost-proxy-ids`, `exthost-handshake`.

**Not in P0 (P1 input):** a connection that outlives a call (a reader thread plus a channel into the
app), cancellation tokens, URI transformers (remote only), `MessagePort`/`process.send` connection
types, reconnection, the `IExtensionDescription` list and activation (`$startExtensionHost`,
`$activate`), the `MainThread*` implementations themselves.

## ADR-0062 — Why `$startExtensionHost` never answered: the host waits for the workspace and configuration (2026-11-10)

Context: ADR-0060 saw `$startExtensionHost` acknowledged but never replied to (empty extension list,
`autoStart: true`).

**Root cause.** Not the empty list and not `autoStart` by itself. `ExtHostExtensionService.initialize()`
(run from the `ExtensionHostMain` constructor) awaits two things the *main thread* must send before it
opens `_readyToStartExtensionHost`:

1. `ExtHostConfiguration.$initializeConfiguration(IConfigurationInitData)`: `_beforeAlmostReadyToRunExtensions`
   (node `extHostExtensionService.ts`) awaits `getConfigProvider()`, a barrier opened only by that call;
2. `ExtHostWorkspace.$initializeWorkspace(IWorkspaceData | null, trusted)`: `waitForInitializeCall()`.

`_startExtensionHost()` (called by `$startExtensionHost`) waits on `_readyToStartExtensionHost`, so the
request is acknowledged (the RPC layer acks on receipt) and then parked forever. The workspace in the
init data is not what opens the barrier. Verified against the real host: no reply without the two calls,
reply (`undefined`) with them.

**`autoStart` interaction.** With `autoStart: true` the host calls `_startExtensionHost()` itself the
moment both barriers open and sets `_started`; a later `$startExtensionHost` then fails with
"Extension host is already started!". So the main thread picks one: `autoStart: true` and send the two
initialisers (the extension list then comes from the init data / `$deltaExtensions`), or `autoStart:
false` and send the two initialisers followed by `$startExtensionHost(delta)`. upstream's local process
host uses `autoStart = (startup === EagerAutoStart)`. rscode uses the second form: the delta carries
the descriptions and we know when "eager extensions activated" is done (the reply).

`handshake` gained `InitParams::auto_start`, `workspace_data`, `configuration_data` and `empty_delta`
helpers. Tests: `crates/vsr-rpc/tests/start_extension_host.rs` (hang, answered, already-started).

## ADR-0063 — `vsr-rpc::session`: a long-lived connection with async replies (2026-11-10)

`Session::start(Connection, Option<HostProcess>)` (or `Session::from_host(Host)` after the handshake)
returns the session and an `mpsc::Receiver<Event>`. A reader thread owns the read half of the socket,
cuts frames, runs `Protocol::receive`, answers replay requests, sends an explicit `Ack` when nothing
piggybacked ours, and a `KeepAlive` after 5 s of silence (`Protocol::keepalive_frame`). The write half,
the `Protocol` and the `RpcPeer` sit behind one mutex, so numbering and writes stay in order from any
thread.

- **Host requests** arrive as `Event::Request(Call, Responder)`. The `Acknowledged` is written by the
  reader at once (like `_receiveRequest`); the reply is written when the owner calls
  `Responder::reply`, from any thread, in any order. The app drains the channel on its own event loop
  and answers from app state: no handler object has to live inside the transport. `RpcPeer::receive` /
  `rpc::reply_message` are the split of `handle` (which keeps its synchronous `Handler` form for the
  P0 tests).
- **Our requests**: `Session::request` returns a `Pending` (its own channel); `wait(timeout)` gives the
  `Completed`, `WaitError::Timeout`, or `WaitError::Closed` when the socket closed first. Any number
  can be in flight; `cancel(req)` sends `Cancel`.
- **Shutdown**: `Session::shutdown(grace)` sends `Terminate`, waits for the process to exit by itself,
  kills it after the grace, removes the socket file and joins the reader; the events channel then ends
  with `Event::Closed`. The same `Closed` arrives when the host dies by itself. `HostProcess::drop`
  kills a still-running Node so a panicking test cannot leave one behind.
- `Event::BadMessage` reports a message that does not decode; the connection carries on.

Tests (`tests/session.rs`): loopback between two sessions over `UnixStream::pair` (out-of-order and
cross-thread replies, timeout, close, undecodable message), and against the real host (workspace and
configuration init, `$startExtensionHost` and five `$test_latency` in flight waited on in reverse,
`MainThread*` requests served on another thread, clean exit). `Host` was split into `Host` +
`HostProcess` for this.

## ADR-0064 — `vsr-exthost`: extension descriptions, activation, and a fixture for ErrorLens (2026-11-10)

New UI-free crate `crates/vsr-exthost` (depends on `vsr-rpc` and `vsr-settings`, no new external
crates). `vsr-rpc` stays the wire; `vsr-exthost` is the meaning of the `MainThread*` shapes.

- **`description`**: `ExtensionDescription::from_dir` turns a folder with a `package.json` into the
  `IExtensionDescription` JSON the host takes: the manifest plus `identifier`, `id`,
  `extensionLocation` (the canonicalised path: the host indexes real paths, and a mismatch makes
  `require('vscode')` fall back with "Could not identify extension"), `targetPlatform: "undefined"` and
  the four flags. `delta_adding` builds the `IExtensionDescriptionDelta` for `$startExtensionHost`.
  Two things that are easy to get wrong, both verified against the host: `ExtensionIdentifier` goes on
  the wire as `{ value, _lower }` (the host never revives it; its maps key on `_lower`), and activation
  events come from the delta's `addActivationEvents`, not from the manifest (`SyncedActivationEventsReader`).
  `configuration_defaults` reads `contributes.configuration` defaults, which `boot` folds into the
  `defaults` layer sent with `$initializeConfiguration` (without them `getConfiguration().get()` of an
  unset key is `undefined`).
- **`main_thread`**: `MainThread::handle(&Call)` routes by proxy id and method. `ExtensionService`
  records `$onWillActivateExtension`, `$onDidActivateExtension` (times and the reason),
  `$onExtensionActivationError` (the `SerializedError` is kept: errors are surfaced, never dropped),
  `$onExtensionRuntimeError`, and answers `$getExtension`, `$asBrowserUri`, `$setPerformanceMarks`.
  Requests no model implements yet go to `MainThread::unhandled` and get `undefined`; the host's
  console messages are kept in `console` (the test uses them to catch a wrong extension location).
- **`host::ExtensionHost`**: `boot` = spawn, handshake, `Session`, then `$initializeWorkspace`,
  `$initializeConfiguration`, `$startExtensionHost(delta)` with `autoStart: false` (ADR-0062);
  `pump(wait)` applies the reader thread's events to the models on the caller's thread (so models need
  no locks and the app drives them from its loop); `wait` / `call` serve the host's requests while
  waiting for a reply, which activation needs (`$onWillActivateExtension` is awaited by the host).
  Typed calls: `activate_by_event`, `activate`, `execute_contributed_command`, `shutdown`.
- **Fixture**: `crates/vsr-exthost/tests/fixtures/errorlens-fixture` (plain CommonJS, written here;
  ErrorLens itself is not copied or downloaded). It registers commands, reads configuration, creates a
  status bar item, a decoration type and a diagnostic collection, and reports what it saw through a
  command. It lives next to the tests that use it, not under `vsr-rpc`, because the host driver and its
  tests moved to this crate.
- The host answers `$activate` with `true` even when `activate()` throws; failure arrives only through
  `$onExtensionActivationError`, so tests assert on that, not on the reply.

Tests (`tests/extension_service.rs`, real host, skip without `RSCODE_CODE_OSS`): description shape (no
host), activation by event + command round trip + contributed default reaches `getConfiguration`, activation
error surfaced, clean shutdown.

## ADR-0065 — `MainThread*` shapes for ErrorLens: commands, configuration, status bar, editors, diagnostics, documents (2026-11-10)

All in `vsr-exthost`, UI-free: each shape is a plain model on `MainThread` that the app reads and paints;
`MainThread::handle` routes by proxy id and method (the rest is recorded in `unhandled` and answered
`undefined`). Only what the fixture (a stand-in for ErrorLens) triggers is implemented.

- **Commands** (`MainThreadCommands`): `registered` (the ids the app can run with
  `ExtensionHost::execute_contributed_command`), `$fireCommandActivationEvent`, and `$executeCommand`
  for an extension running an app command, answered by an `Executor` closure the app sets
  (`set_executor`; without one the command rejects "not found").
- **Configuration**: `configuration::layers(registry, conf)` builds the nested defaults and user layers
  from `vsr-settings`; the extension-contributed defaults (`contributes.configuration`) are merged under
  `defaults` by `boot` and by `configuration_changed` (`$acceptConfigurationChanged`, which fires
  `onDidChangeConfiguration`). `$updateConfigurationOption` / `$removeConfigurationOption` are recorded as
  `ConfigUpdate`s for the app to write to settings.json (the app owns the file). `ConfigurationTarget` is
  `platform/configuration`'s enum (`update(.., true)` arrives as 2, USER), not the extension API's.
- **Status bar** (`$setEntry`, `$disposeEntry`): `StatusBar.entries` keyed by item id, `sorted()` in display
  order (left first; priority nearer the edge). Note the host batches item updates, so text changes arrive
  a tick after the API call.
- **Text editors**: decoration types (`$registerTextEditorDecorationType`) and per-editor decorations from
  both `$trySetDecorations` (with hover / `renderOptions`) and the compact `$trySetDecorationsFast` (four
  1-based numbers per range); an empty set removes the entry. The other `$try*` editor calls fall through
  to `undefined`.
- **Diagnostics** (`$changeMany`, `$clear`): per owner and resource (`scheme://authority/path` key),
  `markers_for(resource)` adds owners up, `counts()` for the problem counters. `MarkerSeverity` constants
  exported.
- **Documents and editors mirror**: `Documents` is the main thread's model of open documents and editors
  and makes the host's messages (`$acceptDocumentsAndEditorsDelta`, `$acceptModelChanged`) through
  `ExtensionHost::open_document` / `show_editor` / `close_editor` / `edit_document`; it answers
  `$tryOpenDocument` (only already-open documents) and `$trySaveDocument`. Columns count `char`s: equal to
  UTF-16 units on the BMP only (follow-up for astral characters). Untitled documents are not supported.

The real-host test drives a full ErrorLens-style loop: open a document with `ERROR` lines, show an editor,
the extension (reacting to `onDidChangeActiveTextEditor`) sets diagnostics, decorations with
`after.contentText` and the status bar count; an edit (`onDidChangeTextDocument`) updates all three; a
settings change flips the status prefix; the extension saves the document and updates a setting.

**Not done (stretch):** wiring into the gpui workbench (painting the status bar entries, decorations and
diagnostics in `vsr-app`, headless with `TestAppContext`). The models are the integration surface; the
workbench needs an `ExtensionHost` owned by the app, `pump` driven from its event loop, and the editor
view reading `editors.decorations_for` / `diagnostics.markers_for`. Also open: ESM extensions, extension
scanning of `~/.vscode/extensions`, `$activateByEvent` for `onLanguage:`/`workspaceContains:` from the
document model, bundling the Code-OSS host (ADR-0060), activation timing out.

## ADR-0066 — The workbench owns the extension host on a worker thread; the UI reads snapshots (2026-11-10)

`vsr-exthost`'s `ExtensionHost` blocks: `boot` waits up to 30 s, `activate_by_event` / `execute_contributed_command`
wait for the host's reply while serving its requests. None of that may run on the gpui thread, so
`vsr-app/src/extensions/worker.rs` owns the host on a thread of its own and talks to the workbench over channels.

- **UI → host** (`ToHost`, a `std::sync::mpsc`): open / change / close a document, the active editor, a selection,
  configuration layers, `Run { command, args }`, `Shutdown`. Only differences are sent: `Workbench::ext_sync` (every
  frame, from `render`) compares each open editor's version with what it last sent.
- **Host → UI** (`FromHost`, a `futures` unbounded channel awaited by one `cx.spawn_in` task, so nothing polls):
  `Ready` (extensions loaded, contributed commands), a `Snapshot`, configuration updates to write, save requests,
  `AppCommand` (an extension runs a workbench command; the host thread waits up to 10 s for the answer, the UI never
  waits), `Problem`, `Exited`. A `Snapshot` is a clone of what the UI paints (status entries, decorations per file,
  markers per file, counts, registered commands, activation errors) and is sent only when it changed.
- **Start**: on the first frame, when `Workbench.ext.autostart` is set (the app sets it; tests start the host themselves
  so no existing test boots Node). `RSCODE_CODE_OSS` (+ `RSCODE_NODE`) must resolve (`Launch::from_env`), otherwise
  extensions are off, quietly: no error, no dialog. A build that fails to start is reported in the status bar's error
  slot. `RSCODE_EXTENSIONS_DIR` replaces `~/.vscode/extensions` (empty turns it off), `RSCODE_EXTENSIONS_DEV` adds
  folders that override installed copies. No setting yet: the settings are generated from upstream's registry and a
  host-location setting is not upstream's, so the `settings-port` gates do not apply. The host's files go to
  `<user data dir>/ExtensionHost`.
- **Quit**: `persist_on_quit` calls `ext_shutdown` (`Terminate`, 2 s grace, then the process is killed) and the
  channel's drop also shuts the thread down. Commands the host contributed are unregistered when it exits.
- Failure is visible: a failed boot, a failed command, a timed-out call and an activation error all reach
  `Workbench.error` (and `ext.problems`).

## ADR-0067 — Painting extension output: status bar, decorations, squiggles, palette (2026-11-10)

- **Status bar** (`status.rs`): entries of `MainThreadStatusBar` in display order, left ones after the problem counters
  and right ones before the position. `$(icon)` text maps the codicons we have glyphs for (error, warning, check, bell,
  gear, sync, ...) and drops the rest; tooltip (string or markdown value), colour and background (CSS or
  `ThemeColor`), and the click command (the entry's `command` id and arguments) are honoured. The problem counters
  now show `diagnostics.counts()` instead of the constant 0 / 0.
- **Decorations** (`paint.rs`, pure; `paint_ui.rs`, gpui): from a `Snapshot` and a colour resolver to per-line
  `LinePaint`: `backgroundColor`, `border` / `borderColor` / `borderWidth`, `isWholeLine`, `after.contentText` with
  `color` / `backgroundColor` / `margin` / `fontStyle`, `light` / `dark` variants by theme scheme, instance
  `renderOptions` over the type's. Colours: `#rgb[a]`, `#rrggbb[aa]`, `rgb()` / `rgba()`, a few names and
  `ThemeColor { id }` through the active colour theme. Multi-line ranges split per line. The model is built once per
  snapshot / theme / document version / font size and handed to the editor view (`EditorView.ext_paint`); `rows()`
  draws whole-line boxes behind the row and range boxes, squiggles and after-text inside the text box. Every element has
  a `debug_selector` (`ext-box:<line>:<col>`, `ext-after:<line>`, `ext-squiggle:<line>:<col>:<severity>`).
  Not painted yet: `color` of the range, `before`, `outline*`, `textDecoration`, gutter icons, overview ruler marks,
  hover messages.
- **Diagnostics**: markers of every owner for the file, error / warning / info as wavy underlines in
  `editorError.foreground` / `editorWarning.foreground` / `editorInfo.foreground` (hints are not drawn); an empty range
  covers one character.
- **Commands**: `contributes.commands` become `Handler::Extension` entries in the workbench registry on `Ready`
  (title and category, `f1` unless the manifest hides it with a `commandPalette` menu `when: false`), so the palette
  lists them and a pick runs them. `execute_command` sends `Run`; the worker fires `onCommand:<id>` first, like the
  command service. Commands an extension registers without contributing them also run (`snapshot.commands`).
  `Execution::Extension` is new beside `Dispatch`.

## ADR-0068 — Feeding the host: documents, editors, selections, configuration, saves (2026-11-10)

- The app has a text buffer, not a change log, so a version change sends the whole new text (line breaks normalised to
  `\n`) and the worker reduces it to one edit with `app_sync::replace_diff` (common prefix and suffix, 1-based `char`
  columns) before `$acceptModelChanged`. Whole-text per change costs O(document) per keystroke; fine for now, and the
  edit the host sees is minimal.
- Every open file is an editor in the host (`show_editor`), the active one is switched with a
  `$acceptDocumentsAndEditorsDelta { newActiveEditor }` (`activate_editor`; `None` when a non-text tab shows), closed tabs
  send `removedDocuments` / `removedEditors` (`close_document`), the active editor's selection goes through
  `ExtHostEditors.$acceptEditorPropertiesChanged` (`set_selection`). New in `vsr-exthost/src/app_sync.rs`: additive
  `impl`s, `resource_key`, `replace_diff`. Not mirrored: language changes of an open document, visible ranges, editor
  options (tab size), untitled documents, astral-plane columns (ADR-0065).
- Configuration: `on_config_changed` sends `layers(registry, configuration)` with the affected keys
  (`$acceptConfigurationChanged`); `WorkspaceConfiguration.update` / `remove` from an extension are written with
  `AppConfig::set_value`. Workspace and workspace-folder targets have no settings file yet, so they are written to the
  user's settings. A write echoes back to the host as a change, as upstream does.
- `$trySaveDocument` makes the workbench save the editor of that file with the same transforms and conflict handling
  as Cmd+S (`save_with`).

## ADR-0069 — Extension discovery and activation events (2026-11-10)

- **Discovery** (`vsr-exthost/src/discovery.rs`, read-only, never writes below `~/.vscode`): a root with a
  `package.json` is one extension (a development checkout), any other root is a folder of extensions. Skips: folders
  named in `.obsolete`, platform builds for another platform (`-linux-x64`, `-darwin-x64` on arm64, ...), an
  `engines.vscode` newer than the build's version (read from the Code-OSS `package.json`), and older versions of the
  same id. A later root overrides an earlier one, so a development path beats the installed copy. `extensions.json` is
  not read: the folder listing plus `.obsolete` says the same thing for what we need.
- `%key%` strings are localised from `package.nls.json` (string or `{ message }`), and the implicit activation events the
  upstream scanner adds are added: `onCommand:<id>` for each contributed command and `onLanguage:<id>` for each
  contributed language. Not done: extension packs / dependencies resolution, `extensionKind`, ESM entry points.
- **Events** (`activation.rs`, pure): `ActivationEvent::parse`, `matches` (exact, and a bare `onLanguage`), `EventIndex`
  (declared events to extension ids, `is_declared`, `workspace_contains`). The worker fires only declared events, once
  each: `onStartupFinished` after `$startExtensionHost` (which itself runs `*`), `workspaceContains:<glob>` after
  matching the pattern against the workspace folder (a plain path is looked up, a glob uses the `files.exclude` matcher
  over at most 20,000 entries, skipping `node_modules` and hidden folders), `onLanguage:<id>` when a document with that
  language is opened, `onCommand:<id>` before a command runs. Other events (`onView:`, `onUri`, ...) are declared but
  never fired.
- Trust: `RSCODE_EXTENSIONS_DEV` is an explicit opt-in; a workspace folder never contributes extensions on its own.
  Running whatever is in `~/.vscode/extensions` is the default the task asked for; `RSCODE_EXTENSIONS_DIR=` turns it off.

## ADR-0072 — Language provider registry on the main side: selector scoring, `LanguageFeatureRegistry<T>`, `MainThreadLanguageFeatures` (2026-11-10)

Lives in `vsr-exthost::language_features` (UI-free; no new crate, because the handler must be reachable
from `MainThread::handle` and the queries need `ExtensionHost`).

- **`selector`**: `score(selector, uri, language, is_synchronized, notebook_uri, notebook_type)` ported from
  `languageSelector.ts` (10 exact, 5 wildcard, 0 none), with `LanguageFilter` / `LanguageSelector`
  (string | filter | list), `targets_notebooks`, `select_language_ids` and `LanguageFilter::from_dto`
  (`IDocumentFilterDto`; a relative pattern's `baseUri` becomes `base`). `languageSelector.test.ts`
  is ported test by test (12 tests, `tests/upstream_language_selector.rs`). Globs use `globset` (already in
  the tree through `vsr-languages`/`vsr-search`) with `literal_separator`; a pattern without `/` also
  matches the base name, as VS Code's glob does. `uri::Uri` is the part of `URI` needed (components,
  `parse`, `fs_path` with the drive-letter rule, `to_uri_string`); it does not percent-decode.
- **`registry`**: `LanguageFeatureRegistry<T: Clone>` ported from `languageFeatureRegistry.ts`: `register`
  returns a `RegistrationId` (the disposable), `ordered` / `ordered_groups` / `all` / `all_no_model` /
  `registered_language_ids`, exclusive selectors (score 1000, every other zeroed; ignored in a recursive
  query), built-in providers after user ones of equal score, newest first on ties, and the
  `_lastCandidate` cache. Differences: no `onDidChange` emitter (poll `version()`), queries take
  `&mut self` (scoring is cached) and return clones, `ModelInfo { uri, language_id, synchronized }`
  stands in for `ITextModel`. Upstream has no test file for the registry: `tests/language_registry.rs`
  is new.
- **`main_thread::LanguageFeatures`** (field `MainThread::language_features`): every `$register*` method
  of `MainThreadLanguageFeaturesShape` that takes a selector (a table maps method -> `Kind` + the names
  of the extra arguments), `$registerNavigateTypeSupport` (no selector), `$unregister`,
  `$setLanguageConfiguration` (recorded per handle) and `$emit*Event` (recorded as `FeatureEvent`s,
  attributed to the registration whose `eventHandle` it is). One registry per `Kind` holds the host's
  handle; `Registration { handle, kind, selector, meta }` keeps the arguments after `(handle, selector)`
  under their protocol names (`triggerCharacters`, `supportsResolveDetails`, `extensionId`,
  `displayName`, `eventHandle`, code action and signature help metadata, ...). Registering a handle again
  replaces it. Not handled (so they land in `unhandled`): the inline completion emit calls, paste/drop
  resolve calls, `$setWordDefinitions` direction (host to main only).
- **Registered but not yet queryable**: evaluatable expressions, inline values, multi-document highlights,
  linked editing ranges, paste/drop edit providers, new symbol names, semantic tokens, inline completions,
  colors, selection ranges, call and type hierarchy, workspace symbols. The registry holds them so the app
  can see them; the `$provide*` side and DTOs are not written.

## ADR-0073 — Language feature queries: `$provide*` on `ExtensionHost`, typed DTOs, and what the UI layer needs (2026-11-10)

`language_features::query` adds typed calls to `ExtensionHost` (an inherent `impl` in the new module, so
`host.rs` only gained `begin_call` and `cancel_request`); `language_features::dto` holds the result
types, ported from `languages.ts` and read from the DTOs of `extHostTypeConverters.ts`.

**Cancellation (verified in `rpcProtocol.ts`):** the host only appends a `CancellationToken` to the
arguments of a request sent with the "with cancellation" message type; most `$provide*` methods read
`token.isCancellationRequested`, so every provider call is sent cancellable (`begin_call(.., true)`) and the
token is *not* part of the arguments. `cancel_query` sends the host's `Cancel`.

**Shape.** `begin_provide(kind, path, method, args, convert) -> Query<T>` starts one call per provider the
registry orders for the open document (`begin_provide_to` takes explicit handles); `poll_query` applies
what arrived without blocking (the app calls it from its loop, no runtime is needed); `finish_query` blocks
up to a timeout (pumping the host) and cancels the stragglers. The typed methods (`provide_hover`,
`provide_definition`, `provide_document_symbols`, ...) are `begin` + `finish`. The answer is
`Provided<T> { results: Vec<(handle, T)>, errors: Vec<(handle, message)> }`, in registry order: one failing
provider does not hide the others (the fixture has a definition provider that throws). `undefined` and
unreadable answers are left out. The document must be open (`open_document`); its language comes from
`Documents`.

**Supported end to end (real host, `tests/language_features_real_host.rs`):** completion (trigger
characters, groups of equal score, resolve), hover, definition / declaration / implementation / type
definition, references, document symbols, document highlights, code lens (+ resolve), inlay hints
(+ resolve), signature help, code actions (+ resolve), rename (+ resolve location), folding ranges,
document links (+ resolve), formatting (document, range, on type). DTO conversion is unit-tested without
a host (`tests/language_features_dto.rs`).

**Decisions.**
- Completion follows `provideSuggestionItems`: providers are asked group by group (equal score), best
  group first, and asking stops at the first group that offered items; for a character trigger only the
  providers that registered it. Other kinds ask every provider; formatting returns every provider's
  edits and the caller picks one (the registration's metadata has `extensionId` / `displayName` for the
  default-formatter setting).
- Commands with arguments stay in the host: a lens, completion item or code action carries a delegating
  command whose `ident` keys the real one. The UI runs `Command::id` with `Command::arguments` through
  `ExtensionHost::execute_contributed_command`.
- Results the host caches (`cache_id` on lists, `Hover::id`, `SignatureHelp::id`) stay cached until
  `ExtensionHost::release(kind, handle, id)`; releasing is fire and forget.
- Positions are 1-based `IPosition`/`IRange`; columns count UTF-16 units in the protocol while
  `Documents` still counts `char`s (ADR-0065 follow-up).
- Kinds stay numeric (`SymbolKind`, `CompletionItemKind`, `InlayHintKind`, ...) with `symbol_kind_name` and
  `completion_item_kind_name` for display.

**What the UI layer needs next (out of scope here):**
1. *Plumbing:* own an `ExtensionHost` (ADR-0065's wiring work), keep documents in sync (`open_document`,
   `edit_document`) and call `poll_query` from the event loop; cancel the previous query when the user
   types on; send `onLanguage:` activation so the providers register at all.
2. *Suggest widget:* trigger on typing, trigger characters (`Registration::meta_strings("triggerCharacters")`),
   commit characters, `Incomplete` re-trigger when `CompletionList::is_incomplete`; filter and rank with
   `filter_text` / `sort_text` (a fuzzy scorer is a separate port); on focus call
   `resolve_completion_item` and show `detail` / `documentation` (Markdown rendering is needed); accept =
   apply `range` (`Single`, or insert/replace chosen by `editor.suggest.insertMode`) + `additional_text_edits`,
   expand as a snippet when `is_snippet()` (a snippet engine is needed), then run `command`; release
   the list when the widget closes.
3. *Hover widget:* `provide_hover` on mouse dwell, merge all providers' `contents`, highlight `range`,
   Markdown rendering with trusted commands; release ids on dismissal.
4. *Outline / breadcrumbs / sticky scroll:* `provide_document_symbols` per document version, flatten with
   `DocumentSymbol::flatten`; refresh on edit with debounce. `go to symbol` quick pick reuses it.
5. *Navigation:* go to definition & co. (`LocationLink`s: open the file, select `target_selection_range`,
   peek when several); references list; highlights on cursor move (`DocumentHighlight` kinds).
6. *Decorations:* code lens (`$emitCodeLensEvent` -> `LanguageFeatures::events`), inlay hints, links,
   folding ranges (feed the existing folding model), all refreshed on edit and on `FeatureEvent`s.
7. *Edits:* formatting (format on save/paste/type: pick the provider by default formatter, apply
   `TextEdit`s as one undo step), rename and code actions (apply `WorkspaceEdit`: text entries per file,
   file create/delete/rename entries, versions), signature help widget.
8. *Needed from the host side later:* `$tryApplyWorkspaceEdit` (MainThreadBulkEdits), `$executeCommand`
   results back into editors, and the remaining provider kinds listed in ADR-0072 (semantic tokens first:
   it unblocks `editor.semanticHighlighting`).

## ADR-0074 — Untitled texts: Save As for each on close and quit, backups keep the language, restore keeps the place, the name box and Welcome's New File (2026-11-10)

**Context.** ADR-0058 and ADR-0059 left six gaps in untitled text editors: quit with Save All reported a
save error for a dirty untitled text (the quit stayed blocked); closing several tabs gave the name box to
the first dirty untitled text only; a restored untitled text lost its language and went to the end of the
tab order; `workbench.editor.limit` was not applied to a restore; the name box offered only the workspace
root, no extension, and committed on blur; and the Welcome page had no New File entry. Upstream asks for
a name for each untitled text through the save dialog during Save All and on quit
(`textFileService` `saveAll` with `includeUntitled`), so a cancelled dialog cancels the quit.

**Decision.**

- **Save As for each dirty untitled text, in turn.** A close (the close prompt's Save All) and a quit (Save
  All) queue their untitled texts in tab order (`Runtime.save_as_queue`, `start_save_as_queue`,
  `next_save_as`). Each name box takes the first; a committed name pops it and, for a close, closes its
  tab. When the queue is empty, a quit goes on (`finish_quit`). **Cancel** (Escape, or leaving the box)
  ends the queue (`abandon_save_as`): the rest stay open and dirty, and a quit is aborted: the bar is gone,
  the app stays open, and a later Cmd+Q asks again. A plain Save starts no queue and drops one in flight.
  Dirty files are saved first, as before. When a file save is refused (a conflict or an error), the app
  stays open on that file and the untitled texts are not asked for; the next Save All asks for them.
- **Backups record the language.** `Backup.language` (optional, so older backups load) is written with the
  text, and a restored untitled text gets it. A text without a recorded language is plain text, as before.
- **Restore keeps the place and applies the limit once.** Untitled texts are listed in the workspace state
  by their key (`untitled:Untitled-N`) with their place in the tab order and the MRU list. The backups are
  read before the tabs, so an untitled tab comes back from its backup at its saved place. A saved untitled
  tab with no backup (clean, or dropped) is not brought back, as ADR-0058 says. A backup with no saved place
  (a crash) is appended. `workbench.editor.limit` is applied once after the restore, when the order and the
  active tab are known; it no longer runs in the middle of the restore.
- **The name box.** It opens in the folder of the most recent file editor when that folder is inside the
  workspace (upstream `defaultFilePath`, the folder of the last active file), else the workspace root. Its
  name is the untitled title when that is a valid file name (else `Untitled-N`), with the extension rewritten
  to the language's first one (upstream `suggestFilename`: `vsr_editor::untitled::suggest_file_name`, ported
  in `crates/vsr-editor/tests/upstream_text_file_service.rs`). **Blur cancels Save As.** Upstream's Save As
  is a dialog that a click elsewhere cannot answer, so leaving the box does not write the file. New File and
  Rename keep upstream's inline behaviour (a valid name commits on blur, ADR-0041).
- **Welcome page.** Start has New File... before Open File..., and it runs the Cmd+N command.

**Tests.** Headless, `untitled_e2e.rs` and `session/e2e_tests.rs`, rows 17 to 24 of
`tests/e2e-scenarios/files.md`. The blur test calls the focus-out handler (`blur_inline`) directly: headless
focus changes do not reach the name box's focus-out listener. The suggested-name rules are upstream ports.

**Not done.**

- A folder outside the workspace is not offered for Save As (the root is used), and the name box does not
  show folders outside the tree.
- A clean untitled text is still not restored after a restart (ADR-0058).
- A refused file save during quit leaves the untitled texts unasked; the user answers Save All again.
- `files.encoding` and the working-copy events for untitled texts stay unmodelled (ADR-0057).
