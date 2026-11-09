# Search e2e scenarios

Upstream dir: `test/smoke/src/areas/search` | pinned sha: `50e1350fc78d` | scenario count: 7
Suites (describe): Search; Quick Open

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | verifies the sidebar moves to the right | Search | `search/search.test.ts:23` | Opens the Search viewlet and dispatches PageUp keybindings twice while checking the activity bar position changed. | pending | Needs search view, keybindings, sidebar position setting. |
| 2 | searches for body & checks for correct result number | Search | `search/search.test.ts:36` | Opens Search and searches `body`, expecting '6 results in 3 files'. | ready | Own fixture: `search_view::e2e_tests::shift_cmd_f_then_typing_searches_after_the_debounce` ("4 results in 2 files"), `match_case_toggle_changes_counts`. |
| 3 | searches only for *.js files & checks for correct result number | Search | `search/search.test.ts:44` | Sets files-to-include `*.js`, searches `body`, expects '4 results in 1 file', then resets filters. | ready | `include_glob_narrows_the_results` (own fixture). |
| 4 | dismisses result & checks for correct result number | Search | `search/search.test.ts:58` | Searches `body` then removes the app.js file match and expects '2 results in 2 files'. | pending | Dismiss exists (`Results::remove_match/remove_file`, unit tested); no E2E yet. |
| 5 | replaces first search result with a replace term | Search | `search/search.test.ts:65` | Searches, expands replace, replaces matches in app.js with `ydob` and back, verifying counts after each step. | pending | it.skip upstream (issue 137195). Replace is not implemented (ADR-0023). |
| 6 | quick open search produces correct result | Quick Open | `search/search.test.ts:87` | Opens file quick access with `.js` and waits for 8 expected file names, then closes it. | pending | Needs quick open. |
| 7 | quick open respects fuzzy matching | Quick Open | `search/search.test.ts:105` | Opens quick access with `a.s` and expects tasks.json, app.js, package.json. | pending | Needs quick open with fuzzy matching. |

## Notes

- 7 `it(` calls (1 skipped). The Search suite resets the workspace with `git checkout .` / `git reset --hard` in `after`, so scenarios assume a git-backed fixture workspace.

## Soon vs far

Near-term (ready / pending):
- #1 verifies the sidebar moves to the right [pending]
- #2 searches for body & checks for correct result number [pending]
- #3 searches only for *.js files & checks for correct result number [pending]
- #4 dismisses result & checks for correct result number [pending]
- #5 replaces first search result with a replace term [pending]
- #6 quick open search produces correct result [pending]
- #7 quick open respects fuzzy matching [pending]

Blocked, grouped by feature:
- none
