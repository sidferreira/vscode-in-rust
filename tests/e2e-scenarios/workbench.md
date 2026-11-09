# Workbench e2e scenarios

Upstream dir: `test/smoke/src/areas/workbench` | pinned sha: `50e1350fc78d` | scenario count: 10
Suites (describe): Data Loss (insiders -> insiders); Data Loss (stable -> insiders); Launch; Localization

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | verifies opened editors are restored | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:22` | Starts the app, opens www and app.js (kept) and an untitled file, restarts, and selects the three tabs to verify they were restored. | pending | Quick open (Cmd+P) and the Command Palette (`View: Keep Editor` is not registered yet) exist; still needs untitled files and session restore of tabs. |
| 2 | verifies editors can save and restore | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:49` | Opens app.js, types text, waits for the dirty tab, saves, restarts and checks the contents include the text. | pending | Typing, dirty tab and save to disk are covered headlessly (`crates/vsr-app/src/editor_view/e2e.rs`: `typing_enter_undo_and_save`, `save_preserves_crlf_and_cmd_w_closes_the_tab`). Still needs restart and session restore of open editors. |
| 3 | verifies that "hot exit" works for dirty files (without delay) | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:78` | Shared testHotExit helper: edits files, restarts without saving, and checks dirty content and tabs are restored. | blocked-on-feature hot exit / backup service |  |
| 4 | verifies that "hot exit" works for dirty files (with delay) | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:82` | Same helper with a 2 second restart delay. | blocked-on-feature hot exit / backup service |  |
| 5 | verifies that auto save triggers on shutdown | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:86` | Same helper with auto-save enabled; checks content is saved on shutdown. | blocked-on-feature hot exit / auto save | Skipped on remote Linux/Windows. |
| 6 | verifies opened editors are restored | Data Loss (stable -> insiders) | `workbench/data-loss.test.ts:153` | Launches a downloaded stable VS Code, opens editors, then opens the dev build on the same user data and verifies they are restored. | blocked-on-feature cross-version state migration (needs stable VS Code install) | Skips when stable build is unavailable. |
| 7 | verifies that "hot exit" works for dirty files (without delay) | Data Loss (stable -> insiders) | `workbench/data-loss.test.ts:210` | Hot exit scenario starting from stable then restoring in insiders. | blocked-on-feature cross-version state migration |  |
| 8 | verifies that "hot exit" works for dirty files (with delay) | Data Loss (stable -> insiders) | `workbench/data-loss.test.ts:214` | Same with 2 second delay. | blocked-on-feature cross-version state migration |  |
| 9 | verifies that application launches when user data directory has non-ascii characters | Launch | `workbench/launch.test.ts:21` | Starts the app with a user data dir containing a non-ASCII character and opens the Explorer view. | ready | Only needs native window plus Explorer toggle; test with a non-ASCII data dir path. |
| 10 | starts with "DE" locale and verifies title and viewlets text is in German | Localization | `workbench/localization.test.ts:15` | Restarts with `--locale=DE`, reads locale info and localized Open/Close/Find strings and asserts German values. | blocked-on-feature localization |  |

## Notes

- 10 `it(` calls: data-loss 8, launch 1, localization 1.
- Data loss suites create their own apps (two startups per test) with a 4 minute timeout; hot exit tests share one `testHotExit` helper per suite. Stable -> insiders tests depend on `ensureStableCode()` downloading a stable build.
- Launch is the only ready scenario across all areas.

## Soon vs far

Near-term (ready / pending):
- #1 verifies opened editors are restored [pending]
- #2 verifies editors can save and restore [pending]
- #9 verifies that application launches when user data directory has non-ascii characters [ready]

Blocked, grouped by feature:
- hot exit / backup service: #3, #4
- hot exit / auto save: #5
- cross-version state migration (needs stable VS Code install): #6
- cross-version state migration: #7, #8
- localization: #10

## rscode-only scenarios (no upstream smoke test)

The upstream smoke tests drive the Command Palette through `quickaccess.runCommand(...)` in most
areas; these headless tests (`crates/vsr-app/src/quick_access/e2e_tests.rs`, ADR-0030) cover the
palette itself:

| scenario | status | test |
|---|---|---|
| Shift+Cmd+P and F1 open the palette with `>` | ready | `shift_cmd_p_and_f1_open_the_palette_with_the_prefix` |
| type "toggle side", Enter hides the side bar; Cmd+B toggles | ready | `typing_toggle_side_then_enter_hides_the_side_bar`, `cmd_b_toggles_the_side_bar` |
| recently used commands first, history setting, Clear Command History | ready | `recently_used_commands_come_first`, `history_setting_zero_disables_recently_used_and_clear_command_history_empties_it` |
| Backspace over `>` is Quick Open, typing `>` is the palette, `?` lists providers | ready | `backspace_over_the_prefix_...`, `cmd_p_from_the_palette_...`, `question_mark_lists_the_providers_...` |
| `:42` / Ctrl+G go to line, columns with tabs, offsets | ready | `go_to_line_with_colon_42_...`, `colon_in_quick_open_...`, `go_to_line_columns_are_visible_columns_...` |
| key binding labels equal the keymap | ready | `the_keybinding_shown_is_the_one_in_the_keymap` |
| editor commands (Select All, Find, Save) run from the palette | ready | `editor_commands_run_on_the_active_editor_and_need_one`, `save_from_the_palette_writes_the_file` |

