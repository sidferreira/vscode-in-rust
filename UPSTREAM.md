# Tracking upstream VS Code

This repo ports parts of VS Code to Rust. Upstream keeps moving, so we track,
per ported area, **which upstream commit our Rust code matches**. A tool then
tells us exactly which upstream commits and files changed under those areas,
and agents port those changes and nothing else.

Pieces:

| piece | what it is |
|---|---|
| `../vscode-original` | read-only clone of upstream (our fork of microsoft/vscode). Never built or edited here. |
| `upstream/ports.toml` | the manifest: upstream path globs -> Rust paths, status, synced commit per entry |
| `scripts/upstream-sync` | stdlib-only Python 3 CLI that asks `git -C <clone>` what changed |
| `upstream/SYNC_REPORT.md` | generated work-list for an agent (regenerate it; don't edit it by hand) |
| `// upstream: <path> @ <sha>` | header in every ported Rust file |

Pinned baseline: `50e1350fc78df67b11049b903d96b530f28d26a5` (1.147.0, 2026-11-09).

## Commands

Run from the repo root. The clone path comes from `[upstream].clone` in the
manifest (default `../vscode-original`, relative to the repo root); override it
with `--clone PATH` or `UPSTREAM_CLONE=PATH`. `--ref REF` picks the upstream
commit to compare against (default: the clone's `HEAD`).

```sh
scripts/upstream-sync status [ID] [--json] [--limit N]   # commits per entry since its synced commit
scripts/upstream-sync diff ID [--stat] [--log]           # net diff (or per-commit patches with --log)
scripts/upstream-sync report [ID] [--with-diff N] [-o FILE|-]   # writes upstream/SYNC_REPORT.md
scripts/upstream-sync mark ID [COMMIT] [--status S]      # record that ID is synced at COMMIT (default: ref)
scripts/upstream-sync mark --all [COMMIT]                # same for every entry
scripts/upstream-sync check                              # manifest paths + Rust headers are consistent
scripts/upstream-sync fetch                              # PRINTS the fetch commands; never runs them
python3 -m unittest discover -s scripts/tests            # the tool's own tests
```

`status` exits 0 even when there are changes; `check` exits 1 on problems; any
error (unknown entry, commit missing from the clone) exits 2.

## Syncing with upstream (human or agent)

1. **Update the clone.** `scripts/upstream-sync fetch` prints the commands. One
   time: `git -C ../vscode-original remote add upstream https://github.com/microsoft/vscode.git`.
   Then `git -C ../vscode-original fetch upstream main` and
   `git -C ../vscode-original merge --ff-only upstream/main`. The clone is
   **shallow**; to keep it small, fetch with `--shallow-since=<date of the pinned commit>`.
   Alternatively don't move the checkout at all and pass `--ref upstream/main`.
   The tool never fetches by itself.
2. **See what moved.** `scripts/upstream-sync status`. Entries with no upstream
   commits under their paths say `up to date`.
3. **Make the work-list.** `scripts/upstream-sync report` (add `--with-diff 400`
   to inline diffs for a self-contained task) and hand `upstream/SYNC_REPORT.md`
   to an agent.
4. **Bump the pin.** When every entry is synced, set `[upstream].pinned` to the new
   commit (by hand; it is only the default for new entries).

Shallow-clone caveat: the commit at the shallow boundary has no parent in the
clone, so git thinks it touches every file. The tool hides such commits when the
entry's net diff is empty and labels them `[shallow]` otherwise. The net diff
(`diff ID`, the file lists) is always exact; for real per-commit history, deepen
the clone (`git fetch --deepen=N`), which costs disk.

## How an agent processes a report

Handle one entry at a time; each entry is an independent task.

1. Read the entry's section: upstream globs, Rust paths, commit checklist, file
   list, warnings.
2. `scripts/upstream-sync diff <id>` for the net change; `--log` for per-commit
   patches and messages (the "why"). Read the full upstream file at the new commit
   with `git -C ../vscode-original show <ref>:<path>` when the diff isn't enough.
3. For each commit decide: **port** it, or **skip** it with a reason (browser/DOM
   only, Electron-specific, telemetry, already covered by our design). Write
   skips into the commit message of your port so the next agent knows.
4. Change the Rust code and tests. Port upstream test changes too (they are in the
   same entry when the manifest lists the test globs).
5. Update the `upstream:` header of every Rust file you changed to the new commit.
6. `cargo test`, `scripts/upstream-sync check`, then
   `scripts/upstream-sync mark <id> <commit>` (the report prints the exact line).
   Commit the Rust change and the `ports.toml` bump together.
7. Entries with status `planned` have no Rust code yet: there's nothing to port.
   Just `mark` them so the next report is quiet, unless you are starting that port.

Warnings worth stopping for:

- `upstream deleted X` / `pattern ... matches no file`: upstream moved or deleted
  code. Find where it went (`git -C ../vscode-original log --diff-filter=R --stat`
  or search for symbols) and update the entry's globs in `ports.toml`.
- `upstream renamed X -> Y`: update header paths, and globs if needed.
- `not an ancestor`: the entry's synced commit isn't in the target's history
  (e.g. a rebased fork). Look at the diff and re-`mark` it carefully.

## Adding a port entry

Before porting a new upstream area, add an entry to `upstream/ports.toml`:

```toml
[[port]]
id = "editor-cursor"                       # stable kebab-case name used on the CLI
status = "planned"                         # planned | partial | ported
upstream = [
  "src/vs/editor/common/cursor/**",        # git :(glob): ** crosses dirs, * does not
  "src/vs/editor/test/common/cursor/**",   # include the tests you'll port
]
exclude = ["src/vs/editor/common/cursor/*Browser*.ts"]   # optional
rust = ["crates/vsr-editor/src/cursor"]    # files or dirs in this repo
synced = "<full sha of the upstream commit you are porting from>"
notes = "anything the next agent should know"
```

- Set `synced` to the commit whose code you are reading (normally the clone's
  `HEAD`: `git -C ../vscode-original rev-parse HEAD`). Afterwards change it only
  with `mark`.
- Keep entries small, about one upstream module plus its tests, so a report entry
  is one manageable task. Include files whose changes affect the result (for
  example `dark_modern.json` includes `dark_plus.json` includes `dark_vs.json`).
- Run `scripts/upstream-sync check` to make sure every glob matches a file.
- Move `status` forward with `mark <id> --status partial|ported`.

The manifest uses a small TOML subset (strings, string arrays, `#` comments)
because Python 3.9 has no `tomllib`. `mark` edits lines in place, so comments
survive.

## The header rule

Every Rust source file that ports upstream code starts with one header line per
upstream file it ports:

```rust
//! upstream: src/vs/editor/common/core/position.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/core/range.ts @ 50e1350fc78d
```

- `//`, `//!` or `///` all work (use `//!` at the top of a module). It must be in
  the first 40 lines.
- One full repo-relative upstream path per line. No lists like `position.ts, range.ts`.
- The SHA is the commit the file was last synced to: at least 7 hex chars, 12 is
  usual. It must match the `synced` commit of the manifest entry whose globs cover
  that path. `check` enforces this, flags files under an entry's `rust` paths that
  have no header, and flags header paths no entry covers.
- Files with no upstream counterpart (glue, platform code) go outside the
  entries' `rust` paths, or take a header that names the closest upstream file.

## Prior art this follows

- **microsoft/typescript-go** pins upstream TypeScript as a git submodule "pointing
  to the commit being ported", ports upstream PRs one at a time, and lists intentional
  differences in `CHANGES.md`. Here the pin is per-entry `synced` commits, and divergences
  go in `DECISIONS.md`. <https://github.com/microsoft/typescript-go>,
  <https://github.com/microsoft/typescript-go/blob/main/CONTRIBUTING.md>
- **uutils/coreutils** measures parity by running the upstream GNU test suite, publishes
  the trend (`uutils/coreutils-tracking`), and has a script (`util/remaining-gnu-error.py`)
  that lists the remaining failures, smallest first, as a work queue. Here, porting
  upstream tests with each entry and generating a report do the same job.
  <https://github.com/uutils/coreutils-tracking>,
  <https://docs.rs/crate/coreutils/0.5.0/source/util/remaining-gnu-error.py>
- **Ladybird** quotes spec steps next to the code and marks gaps with `FIXME:` and
  deviations with `AD-HOC:`, all lint-checked. That gives traceability back to the source text,
  which the `upstream:` headers plus `check` provide here.
  <https://www.w3.org/events/meetings/8e1ca708-fdbf-4264-a79b-4c953fa85248>
