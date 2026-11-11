# Handoff — 2026-11-10 (evening batch done)

Where rscode stands, what is blocked on you, and how the next session picks up.

## State

- `main` == `origin/main` after the 2026-11-10 evening batch. No worktrees or wip branches left.
- 1,877 Rust tests pass with `RSCODE_CODE_OSS` set (real-host tests ran), clippy clean, `upstream-sync` ok, 0 orphan test refs, scripts unittests ok.
- Settings (2,062 total): **wired 297** · exthost 426 · blocked 316 · n/a 558 · **todo 465**.
- Roadmap position: Search ✅ → Settings migration (P2 in progress) and Extensions/language features in parallel → Git (low priority). Terminal deferred.
- What to build next, ranked: `docs/dependency-tree.md` (`scripts/dependency-tree` from `docs/dependencies.toml`).
- Code-OSS ext host build: `../codeoss-exthost`. It now also has `extensions/json-language-features` installed and compiled (`scripts/build-codeoss-exthost --language-features json,css,html`, ADR-0101). Real-host tests: `RSCODE_CODE_OSS=$PWD/../codeoss-exthost`, Node 24.22 (`~/.local/share/mise/installs/node/24.22.1/bin`) first on PATH.
- Script gotcha: `scripts/upstream-sync` and `scripts/test-port` take `--clone <abs path to vscode-original>` BEFORE the subcommand when run from a worktree.

What landed in the evening batch (ADRs in `DECISIONS.md`):
- Suggest steps 3–4 (0084, 0095, 0096): list widget at the cursor, `suggestWidgetVisible`, keys, accept as one undo step with snippets (placeholders, mirrors), lazy resolve, cached lists released; real-host E2E; 38 suggest settings.
- Navigation (0108, 0109): Go to Definition/Declaration/Type Definition/Implementation (F12, Cmd+F12, Cmd+click); hover widget with delay/sticky/hidingDelay/above; 9 settings.
- Extension host login-shell environment (0100): `$SHELL -i -l` env resolved for the host, `application.shellEnvironmentResolutionTimeout`. `child-process-main` done.
- Language features build step + JSON real-host probe (0101): json-language-features activates, 10 providers register, server child process runs — but diagnostics/completion do not reach the main side yet.
- Settings P2: batch 1 reclassified ~90 todo settings (blocked by unbuilt features / n/a); batch 2 wired `workbench.editor.showTabIndex`, `problems.visibility`, `workbench.list.openMode`, `workbench.tree.expandMode`, `workbench.tree.renderIndentGuides`.

Live progress: `docs/progress.html` (regenerate with `scripts/progress-report --clone ../vscode-original --with-tests`).

## What I need from you to move on

| # | Need | Why it blocks | What to do |
|---|---|---|---|
| 1 | **Cargo proxy token in the agent's shell** | No new crates. Blocks the integrated terminal (ADR-0045 reserved) and the easy webview route; ~200 settings and 63 scenarios sit behind it (dependency tree). | Start Claude Code from a shell with `CARGO_REGISTRIES_REPOSERVER_TOKEN` exported, or allow sourcing `~/.local/share/ws-cli/rc/reposerver/env.sh`. |
| 2 | **1Password unlocked while I work** | Commits are SSH-signed; push goes over SSH. | Keep it unlocked. Never bypassed. |
| 3 | **Extensions default** | With `RSCODE_CODE_OSS` set, rscode loads `~/.vscode/extensions` by default (your real extensions boot in the dev app). `RSCODE_EXTENSIONS_DIR=` (empty) turns it off. | Keep, or make it opt-in? |

Verification is by headless tests (unit + `TestAppContext` E2E); I don't hand you manual try-lists for behaviour the tests cover.

Housekeeping (needs you: the permission check refused remote branch deletes):
- Stale remote branches, all merged: `wip/session-restore-welcome`, `wip/syntax-highlighting`, `wip/theme-service` → `git push origin --delete <name>`.
- `~/.Trash/rscode-cleanup-*` (~2.6 GB) can be emptied.

## Next work

1. **JSON language server end to end** (ADR-0101 "gap"): the server runs but no diagnostics reach the main side and completion returns no items. Next: tap the wire (`publishDiagnostics`, proof of `didOpen`), stub the unhandled `$methods` listed in ADR-0101 (MainThreadWindow.$getInitialState, Storage.$initializeExtensionStorage, Logger.$registerLogger, Telemetry, …). Then mark `lsp-client` done, wire `json.*`, repeat for css/html.
2. **Navigation steps 3–4**: code lens, then inlay hints (layout hooks). Hover follow-ups: anchor at the word box, dismiss on mouse down/key/scroll, avoid the suggest widget, `showLongLineWarning`. Peek/picker for multiple definition targets.
3. **Suggest follow-ups** (ADR-0095): snippet session (Tab to next tab stop), `tabCompletion`, `suggestSelection`/`selectionMode` (suggest memory), `acceptSuggestionOnCommitCharacter`, preview, status bar, details side panel, `editorSuggestWidget.*` colours, wait for resolve at accept.
4. **Settings P2 batch 3**: `workbench.list.horizontalScrolling`, `editor.occurrencesHighlightDelay`, `editor.unicodeHighlight.*` (7), `workbench.settings.*`; then `scripts/settings-port --clone ../vscode-original next`.
5. **Dependency-tree picks**: TypeScript/JavaScript (tsserver, 90), Emmet, custom editors, tab strip, file decorations, editor groups.

Known follow-ups:
- Outline: per-document maps aren't evicted on close; diagnostics re-parsed each frame (ADR-0086/0094); filter-on-type is substring, not fuzzy.
- Overview ruler ignores folding; `ignoreHorizontalScrollbarInContentHeight` needs a spacer row; glyph margin.
- File nesting: nested items aren't sorted with their parent; `fileNesting.patterns` in sorted key order (ADR-0052).
- `workbench.activityBar.iconClickBehavior: "focus"` doesn't move keyboard focus into the view.
- Headless E2E for search/explorer key bindings; Ctrl+Shift+M → `tabFocusMode`.
- Pre-existing `cargo fmt` drift in `breadcrumbs/` and `extensions/worker.rs`.
- 10k-line timing tests (minimap, editing, folding, wrap; 100 ms budget, debug build) can fail under parallel load.

## Ground rules for whoever continues

- **Never touch windows or focus**: no launching rscode, no UI scripting, no screenshots unless you ask (docs/testing.md, top). Headless Node ext host processes are fine.
- Screenshots, if ever asked for: the rscode window only, and ask before uploading.
- No `rm -rf`; scratch in a fresh temp folder. Remove merged worktrees and branches after each merge; move unused scratch to the Trash.
- Prefer Haiku subagents (bigger model only for deep work, say so); each in its own worktree; give each a disjoint ADR range (next free: 0091, 0097–0099, 0102–0107, 0110+; 0045 reserved for the terminal). Tell agents to stop at step boundaries with commits and green gates.
- TDD; every wired setting gets a headless test; `// upstream:` refs only for real upstream tests (else "Upstream source: <file>").
- Merge recipe: union additive conflicts; ADRs sorted by number; `upstream/ports.toml` unions need a `[[port]]` header between entries; generated docs → regenerate (`RSCODE_UPDATE_DOCS=1 cargo test -p vsr-app coverage_doc`, `scripts/settings-port --clone ../vscode-original report`, progress report); `docs/dependencies.toml` → mark merged features done and drop removed reasons, then `scripts/dependency-tree`. After merging extension branches, check that `ExtensionHost` is still `Send` (`crates/vsr-exthost/tests/send.rs`).
- Before each push, **checking exit codes, not grep output**: `cargo test --workspace` (with `RSCODE_CODE_OSS`), `cargo clippy --workspace --all-targets`, `scripts/upstream-sync check`, `scripts/test-port status` (0 orphans), `python3 -I -m unittest discover -s scripts/tests`, regenerate progress.
