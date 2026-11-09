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
