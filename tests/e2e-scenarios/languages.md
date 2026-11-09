# Language Features e2e scenarios

Upstream dir: `test/smoke/src/areas/languages` | pinned sha: `50e1350fc78d` | scenario count: 4
Suites (describe): Language Features

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | verifies quick outline (js) | Language Features | `languages/languages.test.ts:16` | Opens bin/www via quick open, opens Go to Symbol, and waits for at least 6 entries. | blocked-on-feature language services (outline, diagnostics); quick open | Needs quick open then symbol providers. |
| 2 | verifies quick outline (css) | Language Features | `languages/languages.test.ts:26` | Opens style.css, opens quick outline, expects exactly 2 entries. | blocked-on-feature language services (outline, diagnostics) |  |
| 3 | verifies problems view (css) | Language Features | `languages/languages.test.ts:36` | Opens style.css, types `.foo{}` in the editor, waits for a warning marker in the editor and in the Problems view. | blocked-on-feature language services (outline, diagnostics); editing, problems view |  |
| 4 | verifies settings (css) | Language Features | `languages/languages.test.ts:49` | Adds `css.lint.emptyRules` as error to user settings, opens style.css, waits for an error marker and Problems entry. | blocked-on-feature language services (outline, diagnostics); settings |  |

## Notes

- 4 `it(` calls. Syntax highlighting alone would not satisfy these; they need language server style features.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- language services (outline, diagnostics); quick open: #1
- language services (outline, diagnostics): #2
- language services (outline, diagnostics); editing, problems view: #3
- language services (outline, diagnostics); settings: #4
