# Task e2e scenarios

Upstream dir: `test/smoke/src/areas/task` | pinned sha: `50e1350fc78d` | scenario count: 4
Suites (describe): Task (wrapper); Task Quick Pick; Task Quick Pick > Tasks: Run Task; Tasks: Configure Task (describe.skip)

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | hide property - true | Task Quick Pick > Tasks: Run Task | `task/task-quick-pick.test.ts:30` | Writes a tasks.json entry with hide: true, runs 'Tasks: Run Task' and asserts the task is not listed. | blocked-on-feature tasks; quick open |  |
| 2 | hide property - false | Task Quick Pick > Tasks: Run Task | `task/task-quick-pick.test.ts:34` | Same with hide: false; task is listed. | blocked-on-feature tasks |  |
| 3 | hide property - undefined | Task Quick Pick > Tasks: Run Task | `task/task-quick-pick.test.ts:38` | Same with hide omitted; task is listed. | blocked-on-feature tasks |  |
| 4 | true should still show the task | Task Quick Pick > Tasks: Configure Task | `task/task-quick-pick.test.ts:64` | Runs 'Tasks: Configure Task' with hide: true and asserts the task is still shown. | blocked-on-feature tasks | Inside describe.skip upstream. |

## Notes

- 4 `it(` calls in task-quick-pick.test.ts; task.test.ts has 0 and only wraps the suite (retries 3, skips on Linux).
- Fixtures are written into tasks.json by the setup code.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- tasks; quick open: #1
- tasks: #2, #3, #4
