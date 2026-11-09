# Accessibility e2e scenarios

Upstream dir: `test/smoke/src/areas/accessibility` | pinned sha: `50e1350fc78d` | scenario count: 6
Suites (describe): Accessibility > Workbench; Accessibility > Workbench > Chat

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | workbench has no accessibility violations | Accessibility > Workbench | `accessibility/accessibility.test.ts:33` | Waits for `.monaco-workbench`, then runs an axe-core scan over it with a few excluded rules for Monaco list ARIA patterns. | blocked-on-feature accessibility APIs (axe-core DOM audit) | Declared as `(opts.web ? it.skip : it)(` so a plain `it(` grep misses it; skipped in web mode. Our UI is native gpui, so a DOM scan has no equivalent; needs an a11y-tree audit. |
| 2 | activity bar has no accessibility violations | Accessibility > Workbench | `accessibility/accessibility.test.ts:48` | Waits for `.activitybar` then runs the axe-core scan scoped to it. | blocked-on-feature accessibility APIs (axe-core DOM audit) | Activity bar exists in our app, but the check needs a native accessibility tree. |
| 3 | sidebar has no accessibility violations | Accessibility > Workbench | `accessibility/accessibility.test.ts:56` | Waits for `.sidebar` then runs the axe-core scan scoped to it. | blocked-on-feature accessibility APIs (axe-core DOM audit) | it.skip upstream. |
| 4 | status bar has no accessibility violations | Accessibility > Workbench | `accessibility/accessibility.test.ts:64` | Waits for `.statusbar` then runs the axe-core scan scoped to it. | blocked-on-feature accessibility APIs (axe-core DOM audit) |  |
| 5 | chat panel has no accessibility violations | Accessibility > Workbench > Chat | `accessibility/accessibility.test.ts:77` | Opens the chat panel via command, waits for it to render, then runs the axe-core scan on it. | blocked-on-feature chat/agents | Not run in web mode; also needs accessibility APIs. |
| 6 | chat terminal tool response has no accessibility violations | Accessibility > Workbench > Chat | `accessibility/accessibility.test.ts:142` | Drives a chat turn that invokes a terminal tool, then scans the rendered tool response with axe-core. | blocked-on-feature chat/agents | it.skip upstream; also needs terminal and accessibility APIs. |

## Notes

- 6 `it(` calls (5 on line-start plus the web-conditional one at line 33). 2 are `it.skip`.
- Suite retries 2 times and uses a 2 minute timeout; common `installAllHandlers` helper launches the app.
- All scenarios rely on `app.code.driver.assertNoAccessibilityViolations` (Playwright + axe-core against the Electron DOM).

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- accessibility APIs (axe-core DOM audit): #1, #2, #3, #4
- chat/agents: #5, #6
