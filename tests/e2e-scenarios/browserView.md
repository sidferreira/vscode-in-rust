# Integrated Browser e2e scenarios

Upstream dir: `test/smoke/src/areas/browserView` | pinned sha: `50e1350fc78d` | scenario count: 7
Suites (describe): Integrated Browser

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | opens an HTML file in a locked browser editor | Integrated Browser | `browserView/browserView.test.ts:106` | Opens a local HTML file via the browser editor command and verifies the editor is locked and renders the page. | blocked-on-feature browser view | Needs an embedded web view. |
| 2 | navigates, reloads, and exposes page history | Integrated Browser | `browserView/browserView.test.ts:136` | Navigates between pages, reloads, and uses back/forward history controls. | blocked-on-feature browser view |  |
| 3 | integrates page find and keyboard routing with the workbench | Integrated Browser | `browserView/browserView.test.ts:164` | Uses find-in-page and verifies keybindings route between the page and the workbench. | blocked-on-feature browser view; keybindings |  |
| 4 | remembers camera permissions and shares them with about:blank frames | Integrated Browser | `browserView/browserView.test.ts:192` | Requests camera permission in a page, checks it is remembered, and that about:blank frames share it. | blocked-on-feature browser view; permissions |  |
| 5 | adds browser context to chat | Integrated Browser | `browserView/browserView.test.ts:242` | Opens a page and attaches it as context in the chat input. | blocked-on-feature browser view and chat |  |
| 6 | allows agents to use shared pages and blocks unshared pages | Integrated Browser | `browserView/browserView.test.ts:290` | Shares a page with the agent, checks tool access works, and is blocked for an unshared page. | blocked-on-feature browser view and chat/agents |  |
| 7 | preserves native page lifecycle across editors, popups, and restart | Integrated Browser | `browserView/browserView.test.ts:353` | Moves browser editors, opens popups, restarts the app and checks the native page state survives. | blocked-on-feature browser view |  |

## Notes

- 7 `it(` calls in a single file. All depend on the Electron native web contents view, with a local HTTP fixture served by the test.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- browser view: #1, #2, #7
- browser view; keybindings: #3
- browser view; permissions: #4
- browser view and chat: #5
- browser view and chat/agents: #6
