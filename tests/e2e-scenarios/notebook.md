# Notebook e2e scenarios

Upstream dir: `test/smoke/src/areas/notebook` | pinned sha: `50e1350fc78d` | scenario count: 7
Suites (describe): Notebooks (declared with `describe.skip`)

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | check heap leaks | Notebooks (describe.skip) | `notebook/notebook.test.ts:31` | Opens a notebook and checks the heap for leaked NotebookTextModel/cell/dispatcher objects via the profiler. | blocked-on-feature notebooks | it.skip inside describe.skip upstream. |
| 2 | check object leaks | Notebooks (describe.skip) | `notebook/notebook.test.ts:40` | Same as heap leaks using object-leak detection. | blocked-on-feature notebooks | it.skip inside describe.skip upstream. |
| 3 | inserts/edits code cell | Notebooks (describe.skip) | `notebook/notebook.test.ts:49` | Opens a notebook, focuses next cell, inserts a code cell, types a line, stops editing. | blocked-on-feature notebooks | it.skip inside describe.skip upstream. |
| 4 | inserts/edits markdown cell | Notebooks (describe.skip) | `notebook/notebook.test.ts:58` | Inserts a markdown cell, types text, stops editing, verifies rendering. | blocked-on-feature notebooks | it.skip inside describe.skip upstream. |
| 5 | moves focus as it inserts/deletes a cell | Notebooks (describe.skip) | `notebook/notebook.test.ts:69` | Inserts and deletes cells and asserts focus moves accordingly. | blocked-on-feature notebooks | it.skip inside describe.skip upstream. |
| 6 | moves focus in and out of output | Notebooks (describe.skip) | `notebook/notebook.test.ts:83` | Moves focus between cell editor and cell output using keybindings. | blocked-on-feature notebooks | it.skip inside describe.skip upstream. |
| 7 | cell action execution | Notebooks (describe.skip) | `notebook/notebook.test.ts:93` | Runs cell actions (execute) and checks output. | blocked-on-feature notebooks | it.skip inside describe.skip upstream. |

## Notes

- 7 `it.skip(` calls inside a `describe.skip('Notebooks')` suite, disabled upstream (issue 140575). The suite is skipped so no app is launched. Nothing in this area actually runs upstream.
- Listed here for completeness and as a future roadmap input.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- notebooks: #1, #2, #3, #4, #5, #6, #7
