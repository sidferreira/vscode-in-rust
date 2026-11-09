# Settings migration

How rscode works through VS Code's 2062 settings, one feature group at a time. The tracker is
`scripts/settings-port`; the policy it applies is `upstream/settings.toml`; the current state is
`upstream/SETTINGS_PORT_REPORT.md` (regenerate it with `scripts/settings-port report`).

## Concepts

- **Setting**: a key in `crates/vsr-settings/assets/default-configuration.json` (exported from the
  pinned VS Code, ADR-0015). `source` is `core` or a built-in extension id.
- **Feature group**: the unit of work. Derived from the key: `a.b` for keys with 3+ segments, or
  `a.<first word of b>` for two-segment keys, when at least three keys share it. Tiny groups merge
  into their parent; groups over 30 keys split one level further. `[[group]]` overrides in
  `upstream/settings.toml` take precedence. The `group` column is what tasks are made of.
- **TOC section**: where the Settings editor shows the setting (`settings-layout.json`, a port of
  upstream's `tocData`). Resolution follows upstream: children claim before parents, first claim
  wins, `hideOn: desktop` nodes are skipped. Settings no node claims are shown as `other`;
  extension settings as `extensions`; `advanced`-tagged settings as `advanced`. TOC placement and
  feature group are separate: a group can span sections (`editor.font` is a TOC leaf; `chat.*`
  groups sit in several).
- **Status** (per setting, first match wins):
  - `wired`: the key is read by the app. Detected from string literals in
    `crates/vsr-app/src/config/wired.rs` (absent file = none wired).
  - `n/a`: we decided not to implement it on macOS desktop. Always with a reason. Set by the first
    matching `[[rule]]` (match on the key, or on the extension `source`), or by the deprecated
    policy.
  - `exthost`: contributed by a built-in extension. Expected to work once the Node extension host
    runs the extension (ADR-0020); not reimplemented in Rust.
  - `todo`: everything else. Has a priority.
- **Priority** (todo only): `P1` if Commonly Used or in a `[priority] p1` group (everyday editing:
  editor basics, files, explorer, search, layout). `P3` if tagged experimental/preview/advanced or
  in `[priority] p3`. `P2` otherwise.
- **Wired precedence**: a key that is wired is `wired` even when a rule says n/a. Code wins over
  policy, so a rule that disagrees with the code shows up as a conflict to resolve in
  `settings.toml`.

## Picking the next group

`scripts/settings-port next --n 3` prints task briefs for the next groups: P1 first, then by
upstream consumer hits (how much upstream code reads the group). A brief lists the todo settings
with type, default and description, the upstream registration file, the top consumer files, the
rscode module it probably touches (`[[component]]`), and the acceptance criteria below. Take the
first brief in order unless the user asks for something specific. Check `status --group <id>`
before starting, since a previous task may have wired some of it.

## Definition of done (one group per task)

1. Every `todo` setting in the group changes behaviour live, with no restart, when set in
   `settings.json` or the Settings editor. Its default matches upstream.
2. A headless E2E test proves each effect (gpui `TestAppContext`, see `docs/testing.md`). The
   scenario lives in `tests/e2e-scenarios/<area>.md`.
3. Each key is added to the wired list in `crates/vsr-app/src/config/wired.rs`.
4. `scripts/settings-port status --group <id>` shows the group's settings as `wired`, `n/a` or
   `exthost`. Nothing is left as `todo` by accident.
5. A setting deliberately left out gets a `[[rule]]` in `upstream/settings.toml` with a reason
   and a comment. Never skip one silently.
6. `scripts/settings-port report` regenerated, and the change committed by the user, not by the
   task.

## Updating status and policy

- **Implemented a setting**: add its key to `wired.rs`. Nothing else. Status is computed.
- **Decided n/a**: add a `[[rule]]` with `status = "n/a"`, a `reason` and a `comment`. Prefer
  explicit key lists; use globs only where every match is really out of scope. Re-run `status`
  to see the effect.
- **Group in the wrong place**: add a `[[group]] match = [...] id = "..."` override. Overrides
  are not merged into their parent, so they can be small.
- **Priority**: edit `[priority] p1` / `p3`. Commonly Used settings are always P1.
- **Which rscode module**: add a `[[component]]` entry. The first match wins, so put specific
  entries before the catch-alls.

Policy changes are user decisions. A task may propose a rule, but it stays in the diff for the
user to review, with its reason in `settings.toml`.

## Upstream sync: what changed

VS Code moves. When the pinned commit or the installed VS Code changes:

1. Re-export: `scripts/export-default-settings` writes a new
   `crates/vsr-settings/assets/default-configuration.json` to a temporary file or the repo copy.
2. Compare the old and new exports:
   `scripts/settings-port diff-export <old.json> <new.json> [--json]`. It lists added, removed and
   changed settings, with the fields that changed (type, default, enum, descriptions, tags,
   deprecation). Each line carries the status it has under the current policy, so a changed
   `wired` setting is visible at once.
3. Keep a copy of the old export before re-exporting (for example in the scratchpad), so the diff
   has something to compare against.
4. For changed `wired` settings, check the behaviour still matches the new default or
   description, then update the group's E2E scenario. For added `todo` settings, they appear in
   their group automatically. For removed ones, delete the wired key and any rule that names them.
5. Re-run `inventory --rebuild` (the consumer index is cached by export hash and upstream HEAD;
   `--rebuild` forces a fresh scan) and `report`.

The consumer index reads the upstream clone's working tree at `../vscode-original`. The tool
warns when HEAD is not the pinned commit or when the tree is dirty. It never modifies the clone.

## Limits

- The consumer index finds literal keys (`'editor.fontSize'`), `EditorOption.<name>` uses, and
  registration by `'key': {` or the enclosing `EditorOption` declaration. Keys built at runtime
  from parts are not found; the brief then says "not found by the index" and the task checks by
  hand.
- A registration marked "guessed" is the likeliest configuration file among the consumers. Verify
  it before relying on it.
- The TOC mirrors upstream's layout as ported in `settings-layout.json`. Settings that no layout
  node claims show as `other`. This is a faithful reflection of upstream's layout, not a
  judgement that they are unimportant.
- The Settings editor's search and the `@tag:` filter are not modelled. Only placement and
  status are.
