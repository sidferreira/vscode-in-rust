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
