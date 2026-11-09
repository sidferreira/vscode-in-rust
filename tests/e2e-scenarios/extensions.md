# Extensions e2e scenarios

Upstream dir: `test/smoke/src/areas/extensions` | pinned sha: `50e1350fc78d` | scenario count: 4
Suites (describe): Extension Host Restart; Extensions

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | kills blocked extension host on window reload (windowLifecycleBound) | Extension Host Restart | `extensions/extension-host-restart.test.ts:30` | Runs a test command that blocks the extension host and writes its PID to a file, reloads the window, and polls until the old process is gone. | blocked-on-feature extension host | Uses a test extension and OS pid checks. |
| 2 | allows extensions to gracefully deactivate on window reload (windowLifecycleGraceTime) | Extension Host Restart | `extensions/extension-host-restart.test.ts:77` | Triggers an extension with a slow deactivate, reloads the window, and checks deactivation completes within the grace time. | blocked-on-feature extension host |  |
| 3 | kills blocked extension host on restart extension host (issue #296681) | Extension Host Restart | `extensions/extension-host-restart.test.ts:142` | Blocks the extension host then runs the restart-extension-host command and checks the old process is killed. | blocked-on-feature extension host |  |
| 4 | install and enable vscode-smoketest-check extension | Extensions | `extensions/extensions.test.ts:19` | Installs ms-vscode.vscode-smoketest-check from the marketplace, closes its editor, and runs its command from the palette. | blocked-on-feature extension host; marketplace | it.skip upstream. |

## Notes

- 4 `it(` calls (1 skipped). The extension-host-restart tests depend on a smoketest extension contributing `smoketest.*` commands.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- extension host: #1, #2, #3
- extension host; marketplace: #4
