# Testing strategy

How we test vscode-rust, and how upstream VS Code's tests get ported. Read this before writing
or porting a test. Background: `docs/research/e2e-and-visual-testing.md` (gpui test APIs, visual
regression options), `UPSTREAM.md` (sync workflow), `DECISIONS.md`.

## The three tiers

| tier | what it proves | where | how it runs |
|---|---|---|---|
| (a) unit | ported upstream unit tests, pure logic | `crates/<crate>/tests/upstream_*.rs`, `#[cfg(test)]` modules | `cargo test` |
| (b) headless E2E | user flows through the real workbench model, no window | `tests/e2e-scenarios/<area>.md` (the spec), Rust tests in `crates/vsr-app/tests/e2e_*.rs` | `cargo test`, gpui `TestAppContext` |
| (c) visual regression | the app window still looks the same | `tests/visual/` (harness), baselines outside git | `cargo test --test visual` |

- **Unit** tests are a port: each file ports one upstream `*.test.ts`. Count and status come from
  `scripts/test-port`. Headline numbers today: 58,065 upstream unit tests in scope (3,080 files,
  6,355 suites); 75 ported (all in `editor/common`, from the piece tree buffer work).
- **E2E** scenarios come from upstream `test/smoke/src/areas/`: 15 areas, 127 `it()` scenarios.
  `tests/e2e-scenarios/<area>.md` lists every scenario with a status: `ready` (the current app
  supports it), `pending` (needs a near-term feature such as editing, quick open or the settings
  editor), or `blocked-on-feature <x>` (far away, such as extensions, terminal, debug, chat).
  Only one scenario is `ready` today (workbench Launch).
- **Visual** tests compare a rendered app window against a stored baseline image. Upstream has no
  pixel-diff tests for the workbench, so this tier is ours. Policy below.

## Conventions

### Upstream references

Every ported test names its upstream source, in two places:

```rust
//! upstream: src/vs/editor/test/common/model/pieceTreeTextBuffer/pieceTreeTextBuffer.test.ts @ 50e1350fc78d
//!
//! Ports of ...

// upstream: ModelBuilder > t1
#[test]
fn model_builder_t1() { ... }
```

- The `//! upstream: <path> @ <sha>` header goes in the first 40 lines of the file, one line per
  upstream file. The sha is the commit the port was synced to. `scripts/upstream-sync check`
  validates headers against `upstream/ports.toml`.
- Each test gets `// upstream: <suite> > <test>`. Nested suites are joined with ` > `, for example
  `Outer > Inner > deep`. Use the exact upstream title text. Dynamic titles cannot be referenced
  (see below), so describe those in the comment instead.
- Upstream tests with `.skip` may be left out. Say so in the file's header comment or commit
  message.

### Where tests live

- Unit ports: `crates/<crate>/tests/upstream_<file_stem>.rs`, for example
  `upstream_configurationmodels.rs`. One upstream file per Rust file. Helpers that mirror upstream
  test helpers go at the top of the file, as in `crates/vsr-text/tests/upstream_piece_tree.rs`.
- Hand-written tests of our own (no upstream counterpart): `crates/<crate>/tests/*.rs` without the
  `upstream_` prefix, and no `// upstream:` comments.
- E2E: the scenario spec is `tests/e2e-scenarios/<area>.md`. When a scenario is implemented, mark
  its status `ready` and add the Rust test name to its row. Rust E2E tests go in
  `crates/vsr-app/tests/e2e_<area>.rs`.
- Visual: `tests/visual/` (harness and scenario list). Baselines and diffs are never committed
  (see the image policy).

### Naming

- Rust test names are snake_case versions of the upstream title, prefixed by nothing. Keep the
  `// upstream:` comment as the link.
- E2E scenario ids are `<area>/<n>` in the markdown table, where `n` is the row number.

## Using scripts/test-port

Run from the repo root. Needs the upstream clone (`../vscode-original`, or `--clone PATH` or
`UPSTREAM_CLONE`). It never builds or fetches anything.

```sh
scripts/test-port inventory                     # totals by area and layer
scripts/test-port inventory --area src/vs/base  # one area, per-file lines
scripts/test-port status                        # ported / total per area, orphans, stale headers
scripts/test-port status --all-files            # per-file rows for every upstream file
scripts/test-port next --n 20                   # next files to port, as task briefs for an agent
scripts/test-port next --area src/vs/editor --all
scripts/test-port report                        # writes upstream/TEST_PORT_REPORT.md (generated)
python3 -I -m unittest discover -s scripts/tests -p 'test_test_port.py'
```

How it works: a small lexer and parser read the mocha-style `suite()/describe()` and `test()/it()`
calls in each file. Titles are read when they are plain strings, template literals (kept raw, with
`${...}` as written) or `+` concatenations. Anything else is "dynamic": counted, never matched.
Tests inside `for`/`forEach` loops are counted once and marked in-loop. Skipped (`.skip`, `.todo`)
tests are counted separately.

Tiers used by `next`:

- **A**, crate-mapped: `editor/common/core` and `model` (vsr-text), `editor/common/cursor`
  (vsr-editor), `workbench/contrib/preferences` and `platform/configuration` (vsr-settings).
- **B**, `base/common`: pure logic, easy wins (new crate).
- **C**, other common and node logic.
- **D**, extensions and cli: no crate planned. Excluded unless `--all`.
- **X**, browser and electron DOM tests: excluded unless `--all`.

The crate map is `CRATE_MAP` in `scripts/test-port`. Change it there when a new crate takes an
area.

## Porting upstream tests

1. Pick a file with `scripts/test-port next`, or take a file from the brief.
2. Read the upstream file at the pinned sha: `git -C ../vscode-original show <sha>:<path>`.
3. Write `crates/<crate>/tests/upstream_<stem>.rs` with the header and one `// upstream:` comment
   per test. Port the code the test needs, or write the minimal API it needs in the crate.
4. `cargo test -p <crate>`, then `scripts/test-port status` to confirm the refs are counted. It
   should report zero orphans.
5. Record skipped upstream tests and deliberate differences in the commit message.

## When upstream changes a file we ported

Upstream changes reach us through `scripts/upstream-sync` (see `UPSTREAM.md`). Test-port adds two
signals:

- `scripts/test-port status` lists every header file whose upstream blob changed since its header
  sha (`CHANGED upstream since ...`), and every header file that disappeared at the pinned ref
  (`MISSING at ref`, which means a rename or deletion).
- Orphans: `// upstream:` refs whose title no longer exists upstream. These usually mean a test was
  renamed or removed. Fix the Rust test name or delete the test, and mention it in the commit.
- Mismatches: refs whose title exists upstream in a file the header does not name. Add the header
  line or fix the ref.

Process when a ported test file changes: `upstream-sync status` flags the entry, the port changes
the Rust tests, the header sha moves to the new commit, and `status` shows zero changed headers
and zero orphans again.

Dynamic titles (generated names) are not tracked. A change to one of those files still shows up
in the `CHANGED` line.

## Image policy (visual baselines)

- **App-window-only.** A baseline is a capture of the vscode-rust window. Never a full-screen
  capture, and never a capture that includes other apps.
- **Outside git.** Baselines live in `tests/visual/baselines/`. That path must be gitignored, and
  the entry is a follow-up for the main agent, since `.gitignore` is outside this change.
- **Nothing committed without approval.** No image (baseline, actual output, or diff) is committed
  until the user explicitly approves committing images. The default for a checkout is no images.
- **Missing baselines skip.** A visual test whose baseline is missing is skipped, not failed,
  unless `UPDATE_BASELINE=1` is set. This keeps fresh clones and CI green.
- **Record mode.** `UPDATE_BASELINE=1 cargo test --test visual` records baselines on the local
  machine. Baselines are per machine (fonts and GPU), so they are not portable.
- **Diffs** go to `target/visual_tests/` (already ignored by `/target`).
- **Tolerance.** Per-channel tolerance 2, and a test passes when 99% or more of the pixels match.
  Mask dynamic regions (caret blink, clocks, animations) before comparing.
- Rule for reviewers: an image that appears in a diff, a PR or a commit without the user's
  approval is a bug. Remove it and keep it out of git.

## Current gaps

- The `.gitignore` entry for `tests/visual/baselines/` is not added yet.
- Headless E2E needs the gpui `test-support` feature in `vsr-app` dev-dependencies. Not added yet.
- The smoke scenarios are specs only. No Rust E2E test exists yet.
