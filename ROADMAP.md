# Roadmap

Priority order set by the user (2026-11-09). Work top to bottom; details and rationale in
`DECISIONS.md`, research in `docs/research/`.

1. **Search** (in progress)
   - [x] Search view / find in files on the ripgrep libraries, search.* settings wired (ADR-0023)
   - [x] In-editor Find widget: Cmd+F / Alt+Cmd+F, replace, find in selection (ADR-0027)
   - [x] Quick Open files (Cmd+P), on a reusable Quick Pick (ADR-0028); the Command Palette (Shift+Cmd+P) reuses it
   - Next: Replace in the Search view, Command Palette (Shift+Cmd+P)
2. **Settings migration**
   - [x] Configuration service + live reload: settings take effect (ADR-0022)
   - Map all ~2062 settings (`scripts/settings-port`), then migrate feature group by group,
     each with a headless test proving the effect
3. **Extensions** (ADR-0020)
   - P0 Node extension-host handshake → P1 ErrorLens → oxc (LSP) → TS / XML / Kotlin →
     GitLens → Claude Code → GitHub Pull Requests
4. **Git integration** (not a priority right now)

Scope: macOS desktop only (ADR-0019). Progress per area: `PARITY.md` (planned, generated from
`upstream/ports.toml`, `scripts/test-port`, `scripts/settings-port`, `tests/e2e-scenarios`).
