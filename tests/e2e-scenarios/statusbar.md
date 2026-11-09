# Statusbar e2e scenarios

Upstream dir: `test/smoke/src/areas/statusbar` | pinned sha: `50e1350fc78d` | scenario count: 4
Suites (describe): Statusbar

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | verifies presence of all default status bar elements | Statusbar | `statusbar/statusbar.test.ts:16` | Waits for the branch, sync and problems items, opens readme.md via quick open, then waits for encoding, EOL, indentation, language and selection items. | blocked-on-feature SCM (branch/sync status) and problems view | Our status bar has line count, encoding, EOL and language only; branch, sync, problems, indentation and cursor selection are missing. A trimmed variant (encoding, EOL, language on an opened file) could be ready once opened via Explorer. |
| 2 | verifies that 'quick input' opens when clicking on status bar elements | Statusbar | `statusbar/statusbar.test.ts:31` | Clicks branch, indentation, encoding, EOL and language items in turn and expects a quick input to open and close each time. | blocked-on-feature SCM (branch status) and quick input pickers | Needs quick input; branch item needs SCM. |
| 3 | verifies that 'Problems View' appears when clicking on 'Problems' status element | Statusbar | `statusbar/statusbar.test.ts:53` | Clicks the Problems status item and waits for the Problems view. | blocked-on-feature problems view (language diagnostics) |  |
| 4 | verifies if changing EOL is reflected in the status bar | Statusbar | `statusbar/statusbar.test.ts:59` | Opens readme.md, clicks the EOL item, selects the CRLF entry in the quick input, and waits for the status bar to show CRLF. | pending | Needs quick input EOL picker and editable EOL; the EOL status item itself exists. |

## Notes

- 4 `it(` calls. Quick open is used to open readme.md; with Explorer click-to-open the same state is reachable today.

## Soon vs far

Near-term (ready / pending):
- #4 verifies if changing EOL is reflected in the status bar [pending]

Blocked, grouped by feature:
- SCM (branch/sync status) and problems view: #1
- SCM (branch status) and quick input pickers: #2
- problems view (language diagnostics): #3
