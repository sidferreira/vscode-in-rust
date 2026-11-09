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
a native select; no search suggestions, TOC counts in search mode, sticky scroll, keyboard row
navigation, list/object widgets, scrollbar, workspace/folder scopes (Workspace tab shows a note),
language overrides or sync indicators. Group titles use the node label (upstream does too); there
is no "A > B" breadcrumb. Text inputs reuse `gpui-component`'s `Input`, recolored through its global
theme in `settings_view::install`. Settings are saved on every valid change; saving drops comments
in settings.json (ADR-0015). The list is gpui's variable-height `list` (virtualised); TOC selection
follows wheel scrolling but not scrollbar drags.

Testing: first tier-2 headless E2E tests (`src/settings/e2e_tests.rs`) run the workbench in gpui's
test platform; see `docs/testing.md` for the dependency workaround (gpui-kit `test-support` breaks
lock resolution, so `gpui-pre`'s feature is enabled directly).

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
