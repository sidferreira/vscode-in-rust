# Roadmap

Priority order set by the user (2026-11-09). Work top to bottom; details and rationale in
`DECISIONS.md`, research in `docs/research/`.

1. **Search** (done)
   - [x] Search view / find in files on the ripgrep libraries, search.* settings wired (ADR-0023)
   - [x] In-editor Find widget: Cmd+F / Alt+Cmd+F, replace, find in selection (ADR-0027)
   - [x] Quick Open files (Cmd+P), on a reusable Quick Pick (ADR-0028)
   - [x] Replace in the Search view: replace input, Preserve Case, Replace All with confirmation, inline preview, per-match / per-file replace (ADR-0029)
   - [x] Command Palette (Shift+Cmd+P, F1) on a shared command registry, Go to Line (Ctrl+G), Cmd+B, `>` `:` `?` prefixes (ADR-0030)
2. **Settings migration** (in progress)
   - [x] Configuration service + live reload: settings take effect (ADR-0022)
   - [x] Map all 2062 settings with a status per setting (`scripts/settings-port`, ADR-0025)
   - [x] First groups migrated: search, find, quick open, palette, minimap, editor rendering, tabs,
     files (auto save, read-only), window/workbench, word wrap (96 wired)
   - In progress (user's choice, 2026-11-09): **enabling features** that unblock many settings at once:
     theme service (done, ADR-0037), language registry + syntax highlighting, file watcher + Explorer file operations,
     session restore + hot exit + welcome page (the welcome page becomes the startup default)
3. **Extensions** (ADR-0020)
   - P0 Node extension-host handshake → P1 ErrorLens → oxc (LSP) → TS / XML / Kotlin →
     GitLens → Claude Code → GitHub Pull Requests
4. **Git integration** (not a priority right now)

Scope: macOS desktop only (ADR-0019). Progress per area: `PARITY.md` (planned, generated from
`upstream/ports.toml`, `scripts/test-port`, `scripts/settings-port`, `tests/e2e-scenarios`).
