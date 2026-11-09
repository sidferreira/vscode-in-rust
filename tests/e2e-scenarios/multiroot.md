# Multiroot e2e scenarios

Upstream dir: `test/smoke/src/areas/multiroot` | pinned sha: `50e1350fc78d` | scenario count: 2
Suites (describe): Multiroot

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | shows results from all folders | Multiroot | `multiroot/multiroot.test.ts:56` | Opens a .code-workspace with several folders, then quick-opens `*.*` and expects files from all folders in the result list. | blocked-on-feature multi-root workspaces; quick open |  |
| 2 | shows workspace name in title | Multiroot | `multiroot/multiroot.test.ts:72` | Waits for the window title to match `smoketest (Workspace)`. | blocked-on-feature multi-root workspaces | Title bar exists in our app, but .code-workspace support does not. |

## Notes

- 2 `it(` calls. The suite generates a workspace file via a local `createWorkspaceFile` helper in the test file.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- multi-root workspaces; quick open: #1
- multi-root workspaces: #2
