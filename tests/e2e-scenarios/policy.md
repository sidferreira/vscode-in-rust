# Policy e2e scenarios

Upstream dir: `test/smoke/src/areas/policy` | pinned sha: `50e1350fc78d` | scenario count: 1
Suites (describe): Policy Plumbing (without policy); Policy Plumbing (with policy)

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | applies the expected setting value | Policy Plumbing (without policy / with policy) | `policy/policy.test.ts:38` | Searches the settings editor for `@id:extensions.autoUpdate`, checks the selected value, whether the control is enabled/disabled, and presence of the 'Managed by your organization' indicator. | blocked-on-feature native policy (plus settings editor) | 1 source it, run twice via a loop over two cases (no policy -> `on`, policy `off`). Native policy fixture (nativePolicy.ts) writes /etc/vscode/policy.json on Linux, registry on Windows, and managed prefs on macOS; only on disposable CI runners. |

## Notes

- 1 `it(` call (2 runtime variants). Helper: nativePolicy.ts (104 lines) writes OS-level policy before app start.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- native policy (plus settings editor): #1
