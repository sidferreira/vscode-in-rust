# Handoff — 2026-11-10 (evening)

Where rscode stands, what is blocked on you, and how the next session picks up.

## State

- `main` == `origin/main`. Every agent branch is merged and every worktree removed; no agents running.
- 1,449 Rust tests pass, clippy clean, `upstream-sync` ok, 0 orphan test refs.
- Settings (2,062 total): **wired 174** · exthost 426 · blocked 353 · n/a 550 · **todo 559** (P1 0, P2 404, P3 155).
- Upstream tests ported: 590 / 58,069. Smoke scenarios ready: 49 / 170.
- Roadmap position: Search ✅ → **Settings migration (P1 done, P2 next)** and **Extensions (P0 done: ADR-0060, ADR-0061)** in parallel → Git (low priority). Terminal deferred.
- Default themes match upstream: *Dark 2026* and *Light 2026* (ADR-0049).
- A Code-OSS ext host build lives at `../codeoss-exthost` (2.5 GB, from `scripts/build-codeoss-exthost`) for the real-host test: `RSCODE_CODE_OSS=$PWD/../codeoss-exthost cargo test -p vsr-rpc --test handshake_real_host` with Node 24.22 on PATH.

Live progress: `docs/progress.html` (regenerate with `scripts/progress-report --clone ../vscode-original --with-tests`).

## What I need from you to move on

| # | Need | Why it blocks | What to do |
|---|---|---|---|
| 1 | **Cargo proxy token in the agent's shell** | No new crates can be fetched through `reposerver`. Blocks the integrated terminal (ADR-0045 reserved: `alacritty_terminal` + `portable-pty`) and anything else needing a new dependency. ~48 `terminal.integrated.*` settings are parked on it. | Either start Claude Code from a shell that already has `CARGO_REGISTRIES_REPOSERVER_TOKEN` exported, or tell me I may source `~/.local/share/ws-cli/rc/reposerver/env.sh` myself. Until then I keep to existing crates. |
| 2 | **1Password unlocked while I work** | Commits are SSH-signed via 1Password and push goes over SSH. | Keep it unlocked during sessions. I never bypass signing or the https→ssh rewrite unless you say so for that batch. |

Verification is by headless tests (unit + `TestAppContext` E2E); I don't hand you manual try-lists for behaviour the tests cover.

Housekeeping (needs you: the permission check refused remote branch deletes):
- Stale remote branches, all merged: `wip/session-restore-welcome`, `wip/syntax-highlighting`, `wip/theme-service` → `git push origin --delete <name>`.
- Old scratch (duplicate Code-OSS build, pnpm tool folders, logs) was moved to `~/.Trash/rscode-cleanup-*`; empty the Trash to reclaim ~2.6 GB.

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
- No `rm -rf`; scratch work goes in a fresh temp folder. Keep things clean: remove merged worktrees (`git worktree remove`) and branches after each merge; move unused scratch to the Trash.
- Prefer Haiku subagents (bigger model only for deep work, say so); up to 6 in parallel, each in its own worktree.
- Prefer existing crates over hand-rolling (once #1 is resolved).
- TDD; every wired setting gets a headless test and an upstream test ref (`// upstream: <suite> > <test>`); record decisions as ADRs in `DECISIONS.md` (next free: 0054; 0045 is reserved for the terminal; 0060–0061 used by extensions — give parallel agents disjoint ranges).
- Merge recipe for agent branches: union additive conflicts; generated docs → take one side then regenerate (`RSCODE_UPDATE_DOCS=1 cargo test -p vsr-app coverage_doc`, `scripts/settings-port report`, progress report); ADRs sorted by number; `upstream/ports.toml` unions need a `[[port]]` header between entries; `Cargo.lock` → the side with firewall pins.
- Before each push: `cargo test --workspace`, `cargo clippy --workspace`, `scripts/upstream-sync check`, `scripts/test-port status` (0 orphans), regenerate progress.
