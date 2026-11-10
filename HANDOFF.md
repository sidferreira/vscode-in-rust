# Handoff — 2026-11-10

Where rscode stands, what is blocked on you, and how the next session picks up.

## State

- `main` == `origin/main` at `de53e8a`. Every local branch and agent worktree is merged; no agents running.
- 1,389 Rust tests pass, clippy clean, `upstream-sync` ok, 0 orphan test refs.
- Settings (2,062 total): **wired 168** · exthost 426 · blocked 341 · n/a 550 · **todo 577** (P1 12, P2 410, P3 155).
- Upstream tests ported: 551 / 58,069. Smoke scenarios ready: 49 / 170.
- Roadmap position: Search ✅ → **Settings migration (in progress)** → Extensions → Git (low priority). Terminal deferred.

Live progress: `docs/progress.html` (regenerate with `scripts/progress-report --clone ../vscode-original --with-tests`).

## What I need from you to move on

| # | Need | Why it blocks | What to do |
|---|---|---|---|
| 1 | **Cargo proxy token in the agent's shell** | No new crates can be fetched through `reposerver`. Blocks the integrated terminal (ADR-0045 reserved: `alacritty_terminal` + `portable-pty`) and anything else needing a new dependency. ~48 `terminal.integrated.*` settings are parked on it. | Either start Claude Code from a shell that already has `CARGO_REGISTRIES_REPOSERVER_TOKEN` exported (e.g. after `source ~/.local/share/ws-cli/rc/reposerver/env.sh`), or tell me I may source that file myself. Until then I keep to existing crates. |
| 2 | **1Password unlocked while I work** | Commits are SSH-signed via 1Password and push goes over SSH. When it locks, commits/pushes stall. | Keep it unlocked during sessions. I never bypass signing or the https→ssh rewrite unless you say so for that batch. |
| 3 | **Next focus** | Two reasonable paths. | Say **"settings"** (default if you just say "go") or **"extensions"** (ADR-0020: P0 ext-host spike, ErrorLens first). |
| 4 | **Default theme** | rscode defaults to *Dark Modern*; current upstream defaults to *Dark 2026*. | Pick one; I'll switch the default and the golden test. |
| 5 | **Manual try of the new editing features** | I am not allowed to launch the app or touch windows (dictation tools), so the real-window check is yours. | `scripts/rscode .` then try: auto-closing brackets/quotes, wrap selection in quotes, Enter auto-indent, Cmd+/, Shift+Alt+A, opening a tab-indented file. Tell me what's off (text only, no screenshot needed). |

Optional housekeeping (you, since I don't delete recursively and remote deletes were refused):
- Stale remote branches, all merged: `wip/session-restore-welcome`, `wip/syntax-highlighting`, `wip/theme-service` → `git push origin --delete <name>`.
- 33 merged agent worktrees use ~1.9 GB under `.claude/worktrees/` → `git worktree remove <path>` for each, then `git worktree prune` (two prunable scratch worktrees in `/private/tmp` too).

## Next work if you say "go" (settings first)

P1 groups, no new crates needed:
1. `editor.scrollBeyondLastColumn` (1)
2. `workbench.activityBar.*` (3)
3. `workbench.sash.*` (2)
4. `explorer.openEditors.*` (3)
5. `explorer.fileNesting.*` (3) — port upstream file-nesting tests too

Then P2 by consumer hits: `editor.scrollbar.*` (6), `accessibility.verbosity.*` (34), workbench lists/trees. `http.*` / `http.proxy` are mostly exthost. Get the live list with `scripts/settings-port --clone ../vscode-original next`.

Known follow-ups:
- Headless E2E for search/explorer key bindings.
- Ctrl+Shift+M → toggle `tabFocusMode`.

## Ground rules for whoever continues

- **Never touch windows or focus**: no launching rscode, no UI scripting, no screenshots unless you ask (docs/testing.md, top). Verification is headless (`TestAppContext`, `AppleTextSystem` for real fonts).
- Screenshots, if ever asked for: the rscode window only, and ask before uploading.
- No `rm -rf`; scratch work goes in a fresh temp folder.
- Prefer Haiku subagents; up to 6 in parallel, each in its own worktree.
- Prefer existing crates over hand-rolling (once #1 is resolved).
- TDD; every wired setting gets a headless test and an upstream test ref (`// upstream: <suite> > <test>`); record decisions as ADRs in `DECISIONS.md` (next free: 0049; 0045 is reserved for the terminal).
- Merge recipe for agent branches: union additive conflicts; generated docs → take one side then regenerate (`RSCODE_UPDATE_DOCS=1 cargo test -p vsr-app coverage_doc`, `scripts/settings-port report`, progress report); ADRs sorted by number; `upstream/ports.toml` unions need a `[[port]]` header between entries; `Cargo.lock` → the side with firewall pins.
- Before each push: `cargo test --workspace`, `cargo clippy --workspace`, `scripts/upstream-sync check`, `scripts/test-port status` (0 orphans), regenerate progress.
