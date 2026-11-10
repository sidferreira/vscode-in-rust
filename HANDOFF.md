# Handoff — 2026-11-10 (evening)

Where rscode stands, what is blocked on you, and how the next session picks up.

## State

- `main` == `origin/main`. Both of today's agent branches (`wip/settings-p1`, `wip/ext-host-p0`) are merged; no agents running.
- 1,448 Rust tests pass, clippy clean, `upstream-sync` ok, 0 orphan test refs.
- Settings (2,062 total): **wired 174** · exthost 426 · blocked 353 · n/a 550 · **todo 559** (P1 0, P2 404, P3 155).
- Upstream tests ported: 590 / 58,069. Smoke scenarios ready: 49 / 170.
- Roadmap position: Search ✅ → **Settings migration (P1 done, P2 next)** and **Extensions (P0 done: ADR-0060, ADR-0061)** in parallel → Git (low priority). Terminal deferred.
- Default dark theme is now *Dark 2026* (ADR-0049), as upstream.

Live progress: `docs/progress.html` (regenerate with `scripts/progress-report --clone ../vscode-original --with-tests`).

## What I need from you to move on

| # | Need | Why it blocks | What to do |
|---|---|---|---|
| 1 | **Cargo proxy token in the agent's shell** | No new crates can be fetched through `reposerver`. Blocks the integrated terminal (ADR-0045 reserved: `alacritty_terminal` + `portable-pty`) and anything else needing a new dependency. ~48 `terminal.integrated.*` settings are parked on it. | Either start Claude Code from a shell that already has `CARGO_REGISTRIES_REPOSERVER_TOKEN` exported, or tell me I may source `~/.local/share/ws-cli/rc/reposerver/env.sh` myself. Until then I keep to existing crates. |
| 2 | **1Password unlocked while I work** | Commits are SSH-signed via 1Password and push goes over SSH. | Keep it unlocked during sessions. I never bypass signing or the https→ssh rewrite unless you say so for that batch. |
| 3 | **Default light theme** | Dark now follows upstream (Dark 2026); light is still *Light Modern*, upstream uses *Light 2026* (`DEFAULT_LIGHT`, one constant). | Say "match" or "keep". |
| 4 | **Manual try in the real app** | I am not allowed to launch the app or touch windows (dictation tools). | `scripts/rscode .` then try: Dark 2026 look; `editor.scrollBeyondLastColumn` (0 removes the room past long lines); `workbench.activityBar.compact: true`; clicking the active Explorer icon hides the side bar (`iconClickBehavior: "focus"` keeps it); `explorer.fileNesting.enabled: true` (a.js under a.ts, package-lock.json under package.json, chevron toggles). Also still pending from last time: auto-closing brackets/quotes, Enter auto-indent, Cmd+/, Shift+Alt+A. |

Housekeeping (you, since I don't delete recursively):
- Code-OSS spike scratch folders in `../` (~5 GB): `codeoss-spike.dRVZ5W`, `codeoss-rerun.JbhpFX` (2.5 GB each), `codeoss-tools.LagsCg`, `codeoss-tools.AzgFYW`, `codeoss-*.log`. `scripts/build-codeoss-exthost [DEST]` rebuilds one when needed.
- Merged agent worktrees under `.claude/worktrees/` (now including `settings` and `extensions`) → `git worktree remove <path>` each, then `git worktree prune`; then `git branch -d wip/settings-p1 wip/ext-host-p0`.
- Stale remote branches, all merged: `wip/session-restore-welcome`, `wip/syntax-highlighting`, `wip/theme-service` → `git push origin --delete <name>`.

## Next work

**Extensions P1 (ErrorLens)**, per ADR-0060's notes:
1. Investigate why `$startExtensionHost` (with `autoStart: true`, empty extension list) is acknowledged but never replied to — see `extHostExtensionService.ts` `initialize()` vs `_startExtensionHost()`.
2. Long-lived connection: reader thread + channel into the app so `MainThread*` handlers reach app state.
3. ErrorLens `IExtensionDescription` in the init data; `MainThreadExtensionService`.
4. `MainThreadCommands`, `Configuration`, `StatusBar`, `TextEditors`/decorations, `Diagnostics`, in the order the host asks.
5. Decide where a pruned/bundled Code-OSS ext host lives for releases.
Run the real-host test with `RSCODE_CODE_OSS=<build> cargo test -p vsr-rpc --test handshake_real_host`.

**Settings P2** by consumer hits: `accessibility.verbosity.*` (34), workbench lists/trees, … (`editor.scrollbar.*` is blocked until rscode draws an editor scrollbar, ADR-0053). Live list: `scripts/settings-port --clone ../vscode-original next`.

Known follow-ups:
- File nesting: nested items aren't sorted with their parent in the tree model; `fileNesting.patterns` read in sorted key order, not file order (ADR-0052).
- `workbench.activityBar.iconClickBehavior: "focus"` doesn't move keyboard focus into the view yet.
- Headless E2E for search/explorer key bindings.
- Ctrl+Shift+M → toggle `tabFocusMode`.
- Pre-existing `cargo fmt` diffs in `breadcrumbs/`.

## Ground rules for whoever continues

- **Never touch windows or focus**: no launching rscode, no UI scripting, no screenshots unless you ask (docs/testing.md, top). Verification is headless (`TestAppContext`, `AppleTextSystem` for real fonts). Headless Node ext host processes are fine.
- Screenshots, if ever asked for: the rscode window only, and ask before uploading.
- No `rm -rf`; scratch work goes in a fresh temp folder.
- Prefer Haiku subagents (bigger model only for deep work, say so); up to 6 in parallel, each in its own worktree.
- Prefer existing crates over hand-rolling (once #1 is resolved).
- TDD; every wired setting gets a headless test and an upstream test ref (`// upstream: <suite> > <test>`); record decisions as ADRs in `DECISIONS.md` (next free: 0054; 0045 is reserved for the terminal; 0060–0061 used by extensions — give parallel agents disjoint ranges).
- Merge recipe for agent branches: union additive conflicts; generated docs → take one side then regenerate (`RSCODE_UPDATE_DOCS=1 cargo test -p vsr-app coverage_doc`, `scripts/settings-port report`, progress report); ADRs sorted by number; `upstream/ports.toml` unions need a `[[port]]` header between entries; `Cargo.lock` → the side with firewall pins.
- Before each push: `cargo test --workspace`, `cargo clippy --workspace`, `scripts/upstream-sync check`, `scripts/test-port status` (0 orphans), regenerate progress.
