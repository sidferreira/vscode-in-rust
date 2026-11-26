# Handoff — 2026-11-26 (overnight batch merged)

Where rscode stands, what is blocked on you, and how the next session picks up.

## Update — 2026-11-26 day (batch 8 merged into main, 41d396a, not pushed)

- **Push needed**: `main` is 87 commits ahead of origin (16 unsigned from the overnight batch, the rest signed). The permission check blocks `git push` from the agent; push yourself (`git push origin main`) or allow it.
- Landed: native pull requests on `gh` (`crates/vsr-github` + Pull Requests view, #40; ADR-0113/0114, two review rounds), built-in json/css/html extensions load by default (#39), `rode` loads `~/.vscode/extensions` (#31), on-demand reads refuse FIFOs/devices (#34), snippet session ends on reload (#35), offscreen UI snapshot tests (ADR-0161–0163; 12 references committed with user approval; `cargo test -p vsr-app ui_snapshots -- --ignored --test-threads=1`), Settings editor filters/menu/Copy as JSON/advanced TOC (ADR-0142), Find whole-word via `editor.wordSeparators` + JS-style empty matches + Search tree view (ADR-0146/0147, 28 upstream tests ported), settings reclassifications.
- Gates on main: 2,281 Rust tests pass, clippy clean, upstream-sync ok, 0 orphans, script tests ok, 13 UI snapshots match.
- Close after the push: #31, #34, #35, #39, #40 (plus the earlier list below).
- New follow-ups: #41 terminal left padding, #42 Settings checkbox descriptions don't wrap, #43 flaky search test under load.

## State

- `main` is at the overnight batch (44 commits after `df7c147`), **not pushed**: push needs 1Password (SSH). The batch commits made after 1Password locked are **unsigned**, as asked ("if ok, commit unsigned" after the Haiku and Opus reviews).
- Gates on `main`: 2,110 Rust tests pass (real Code-OSS host, `RSCODE_CODE_OSS=../codeoss-exthost-lsp`), clippy clean, `upstream-sync check` ok, 0 orphan test refs, script unittests ok.
- Settings (2,062): **wired 330** · blocked 325 · todo 428 (rest exthost / n/a).
- `rode` loads your `~/.vscode/extensions` by default (#31, decided 2026-11-26). Built-in json/css/html extensions still need `RSCODE_EXTENSIONS_DEV` until #39.
- Installed app: `rode` (ADR-0110) → `~/Applications/rscode.app`, release build of `main`, pointed at `../codeoss-exthost-lsp`. Rerun `scripts/install-rode` after merges.
- Code-OSS host builds: `../codeoss-exthost-lsp` has json, css and html language features compiled (use this one); `../codeoss-exthost` has only json compiled.
- Work items are GitHub issues (#1–#38). Close the ones below once `main` is pushed.

What landed (ADRs in `DECISIONS.md`):
- JSON, CSS, SCSS, Less and HTML language servers end to end (#1, #2; ADR-0102, 0103). JSON diagnostics are pulled for visible documents only; `workspace.openTextDocument` of an unopened file works (on-demand documents, cap 256, 50 MB).
- Integrated terminal and bottom panel (#21; ADR-0045, 0111, 0112): `vsr-terminal` crate (portable-pty 0.9.0 + alacritty_terminal 0.25.1), Cmd+J panel, Ctrl+` terminal, 23 settings.
- Snippet session (#7; ADR-0120): Tab/Shift+Tab/Escape through tab stops, mirrors, undo. `editor.tabCompletion` not done.
- Code lens step 1 (#3; ADR-0155): provider query, debounce, resolve — nothing is drawn yet (see #36).
- Hover follow-ups and Ctrl+Shift+M tab focus mode (#5, #27); outline fixes (#22; ADR-0130); explorer focus and nesting (#25, #26; ADR-0140, 0141); overview ruler follows folds, scrollbar spacer (#23, #24a; ADR-0135); settings batch 3 (#11; ADR-0097); tab action location and tab sizing (#15 partial); timing tests behind `#[ignore]` (#30; ADR-0125); search/explorer key-binding E2E (#28 partial); `rode new.txt` and `rode -g file:line:col` (#32 partial; ADR-0160 designs single instance).
- Review fixes: 5 Haiku reviews and an Opus review; findings fixed (paste injection, PTY write blocking, process-group hangup, code lens resolve loop and leaks, snippet mirror corruption, CRLF stops, hover panic, `-g` stray files, …). Follow-ups filed as #33–#38.

Issues to close after the push: #1, #2, #5, #11, #22, #23, #25, #26, #27, #30. Partial (leave open, comment): #3, #7, #15, #21, #24 (glyph margin), #28, #32.

## What I need from you to move on

| # | Need | Why | What to do |
|---|---|---|---|
| 1 | **Unlock 1Password and push** | `main` is 44 commits ahead; signing and push go through 1Password. | Unlock, then `git push` (or ask me). Say whether the unsigned overnight commits are fine as they are. |

Housekeeping: `~/.Trash/rscode-cleanup-*` can be emptied; stale remote branches `wip/session-restore-welcome`, `wip/syntax-highlighting`, `wip/theme-service`.

## Next work

1. **Review follow-ups first**: #34 (non-regular files in on-demand reads: worker hang), #33 (CRLF + additional edits), #35 (snippet after reload), #36 (code lens resolves block hover/completion — gate or render).
2. **Code lens step 2** (#3): lens rows in the line layout (about six row↔line places), click, font settings. Sonnet-sized.
3. **Snippets**: `editor.tabCompletion`, upstream test ports (#7).
4. **Tab strip** (#15): scrollbar, wrapTabs, sticky/pinned tabs; reclassify centered layout and editor actions.
5. **Terminal** (#21, #37): shell integration, links, find, splits, session persistence; review leftovers.
6. Inlay hints (#4), TypeScript/JavaScript (#12), Emmet (#13), then `docs/dependency-tree.md` ranking.

## Ground rules for whoever continues

- **Never touch windows or focus** in verification: headless tests only, no UI scripting, no screenshots unless asked (docs/testing.md). Exception the user asked for: after each commit the main agent makes, run `scripts/install-rode` and launch `rode` on a demo file that shows the change. Subagents never launch anything.
- Screenshots, if ever asked for: the rscode window only, and ask before uploading.
- No `rm -rf`; scratch in a fresh temp folder. Remove merged worktrees and branches after each merge; move unused scratch to the Trash.
- **Model by task size**: Haiku only for small single-step tasks (well under 100k tokens: one setting, one narrow fix, targeted tests); anything bigger goes to Sonnet. Each agent in its own worktree with a disjoint ADR range (next free: 0091, 0098–0099, 0104–0107, 0113–0119, 0121–0124, 0126–0129, 0131–0134, 0136–0139, 0142–0154, 0156–0159, 0161+). Main agent runs full gates and doc regeneration at merge.
- Prefer well-known, maintained crates over hand-rolling (record crate@version and why in the ADR); the proxy quarantines very new versions, so take the previous release. Crate downloads need Claude Code started from a shell with the proxy token set.
- Commits are signed through 1Password; agents never bypass signing. If it is locked, agents leave work uncommitted and report.
- TDD; every wired setting gets a headless test; `// upstream:` refs only for real upstream tests (else "Upstream source: <file>").
- Merge recipe: union additive conflicts; ADRs sorted by number; `upstream/ports.toml` unions need a `[[port]]` header between entries; generated docs → regenerate (`RSCODE_UPDATE_DOCS=1 cargo test -p vsr-app coverage_doc`, `scripts/settings-port --clone ../vscode-original report`, `scripts/dependency-tree`, progress report last). After merging extension branches, check that `ExtensionHost` is still `Send` (`crates/vsr-exthost/tests/send.rs`). Watch for `git apply --3way` staging files: commit by pathspec only from a clean index.
- Before each push, **checking exit codes, not grep output**: `cargo test --workspace` (with `RSCODE_CODE_OSS=../codeoss-exthost-lsp`), `cargo clippy --workspace --all-targets`, `scripts/upstream-sync check`, `scripts/test-port status` (0 orphans), `python3 -I -m unittest discover -s scripts/tests`, regenerate progress. Timing budgets: `cargo test -p vsr-app perf_ -- --ignored --test-threads=1` on a quiet machine.
