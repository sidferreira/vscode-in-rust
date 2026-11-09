# Roadmap

Priority order set by the user (2026-11-09). Work top to bottom; details and rationale in
`DECISIONS.md`, research in `docs/research/`.

1. **Search** (in progress)
   - Search view / find in files (ripgrep libraries: `grep-*`, `ignore`)
   - [x] In-editor Find widget: Cmd+F / Alt+Cmd+F, replace, find in selection (ADR-0027)
   - Next: Quick Open files (Cmd+P)
2. **Settings migration**
   - Configuration service + live reload (in progress, makes settings take effect)
   - Map all ~2062 settings (`scripts/settings-port`), then migrate feature group by group,
     each with a headless test proving the effect
3. **Extensions** (ADR-0020)
   - P0 Node extension-host handshake → P1 ErrorLens → oxc (LSP) → TS / XML / Kotlin →
     GitLens → Claude Code → GitHub Pull Requests
4. **Git integration** (not a priority right now)

Scope: macOS desktop only (ADR-0019). Progress per area: `PARITY.md` (planned, generated from
`upstream/ports.toml`, `scripts/test-port`, `scripts/settings-port`, `tests/e2e-scenarios`).
