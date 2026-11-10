# Workbench e2e scenarios

Upstream dir: `test/smoke/src/areas/workbench` | pinned sha: `50e1350fc78d` | scenario count: 10
Suites (describe): Data Loss (insiders -> insiders); Data Loss (stable -> insiders); Launch; Localization

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | verifies opened editors are restored | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:22` | Starts the app, opens www and app.js (kept) and an untitled file, restarts, and selects the three tabs to verify they were restored. | pending | Tab restore is covered headlessly (`session::e2e_tests::quitting_and_restarting_restores_tabs_active_scroll_and_selection`, ADR-0040); the scenario also restores an empty untitled tab. rscode restores a dirty untitled text (hot exit, ADR-0058) but not an empty one, so the scenario stays pending. |
| 2 | verifies editors can save and restore | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:49` | Opens app.js, types text, waits for the dirty tab, saves, restarts and checks the contents include the text. | ready | `session::e2e_tests::saving_removes_the_backup_and_a_restart_shows_no_stale_text` (type, save, restart, content is on disk and nothing is dirty), with the typing and save coverage in `editor_view/e2e.rs`. |
| 3 | verifies that "hot exit" works for dirty files (without delay) | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:78` | Shared testHotExit helper: edits files, restarts without saving, and checks dirty content and tabs are restored. | ready | `session::e2e_tests::a_dirty_buffer_survives_quit_and_restart_as_dirty_with_its_text` (quit with `request_quit`, restart on the same user data dir; tabs and dirty text come back). |
| 4 | verifies that "hot exit" works for dirty files (with delay) | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:82` | Same helper with a 2 second restart delay. | ready | `session::e2e_tests::a_backup_is_written_after_a_quiet_delay_so_a_crash_loses_nothing` (the backup is written after a quiet second, a restart without the quit hook still restores the text). |
| 5 | verifies that auto save triggers on shutdown | Data Loss (insiders -> insiders) | `workbench/data-loss.test.ts:86` | Same helper with auto-save enabled; checks content is saved on shutdown. | pending | Skipped on remote Linux/Windows. Hot exit exists (ADR-0040); saving every dirty file when quitting with `files.autoSave` on is not implemented (the text is kept in backups instead). |
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
- #1 verifies opened editors are restored [pending: restore of an empty untitled tab]
- #5 verifies that auto save triggers on shutdown [pending]
- #9 verifies that application launches when user data directory has non-ascii characters [ready]

Blocked, grouped by feature:
- cross-version state migration (needs stable VS Code install): #6
- cross-version state migration: #7, #8
- localization: #10

### Sash grippers (ADR-0056)

Headless (`crates/vsr-app/src/config/e2e_sash.rs`). The grip is found by its debug selector, and the idle timer runs on the test clock.

| scenario | status | test |
|---|---|---|
| the grip is drawn on the sidebar sash | ready | `the_grip_is_drawn_on_the_sidebar_sash` |
| auto-hide recedes the grip after the delay; mouse movement brings it back | ready | `auto_hide_recedes_the_grip_after_the_delay_and_mouse_movement_brings_it_back` |
| `workbench.sash.autoHideGrippersDelay` is read live | ready | `the_delay_setting_is_read_live` |
| `workbench.sash.autoHideGrippers: false` keeps the grip visible | ready | `disabling_auto_hide_keeps_the_grip_visible` |

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
| a single click previews a file (replaced by the next click); an edit pins it | ready | `single_click_previews_and_the_next_click_replaces_the_preview` (`config/e2e.rs`) |
| `enablePreview: false` opens every file pinned | ready | `turning_previews_off_opens_every_file_pinned` |
| `openPositioning: first` puts a new tab at the start | ready | `open_positioning_first_puts_new_tabs_at_the_start` |
| `limit.enabled` with `value: 2` closes the least recently used tab | ready | `limit_closes_the_least_recently_used_editor_when_exceeded` |
| `closeOnFileDelete` closes a clean tab whose file is gone and keeps a dirty one; off keeps it | ready | `close_on_file_delete_closes_clean_tabs_and_keeps_dirty_ones`, `close_on_file_delete_off_keeps_the_tab` |
| focus after closing the active tab follows the MRU list (or the right neighbour) | ready | model test `multiple_editors_closing_picks_next_from_mru_list`, `closing_the_active_tab_focuses_the_mru_editor_or_the_right_neighbour` |

Session restore, hot exit and the Welcome page (ADR-0040, `crates/vsr-app/src/session/e2e_tests.rs`;
a restart is a second workbench on the same injected user data dir):

| scenario | status | test |
|---|---|---|
| quit and restart restores tabs, order, active tab, scroll and selection | ready | `quitting_and_restarting_restores_tabs_active_scroll_and_selection` |
| preview flag, active tab and activation order survive; deleted files are skipped | ready | `preview_flag_active_and_mru_survive_a_restart`, `files_deleted_since_are_skipped_on_restore` |
| side bar view, width and the Search query come back | ready | `the_side_bar_and_the_search_query_are_restored` |
| `window.restoreWindows: none` and `restoreEditors: false` start empty (with the Welcome page) | ready | `restore_windows_none_and_restore_editors_off_start_empty` |
| `restoreViewState`: a closed file reopens at its scroll and selection; off does not | ready | `a_closed_file_reopens_at_its_view_state_and_restore_view_state_off_does_not` |
| a damaged state file is kept aside and the start is clean | ready | `a_damaged_state_file_is_kept_aside_and_the_start_is_clean` |
| dirty buffer: Cmd+Q does not ask, restart restores it dirty with its text; undo goes back to the file | ready | `a_dirty_buffer_survives_quit_and_restart_as_dirty_with_its_text` |
| backup after a quiet delay (crash safety); saving or undoing to clean removes it | ready | `a_backup_is_written_after_a_quiet_delay_...`, `saving_removes_the_backup_...`, `undoing_back_to_the_saved_text_drops_the_backup` |
| closing a dirty tab moves its backup to `discarded/` (never deleted) | ready | `a_tab_closed_with_unsaved_changes_moves_its_backup_aside_instead_of_deleting_it` |
| closing a dirty tab asks (Save, Don't Save, Cancel) from Cmd+W, the tab button and a middle click; Save writes then closes; Don't Save closes and leaves the disk alone; Cancel keeps the tab; a changed file keeps the tab with the conflict bar | ready | `cmd_w_at_once_after_typing_asks_and_save_writes_the_file_then_closes`, `dont_save_closes_the_tab_leaves_the_disk_alone_and_moves_the_backup_aside`, `dont_save_before_any_backup_leaves_nothing_behind`, `cancel_keeps_the_dirty_tab_open_with_its_text`, `the_tab_close_button_and_a_middle_click_ask_too`, `a_dirty_tab_whose_file_changed_on_disk_stays_open_with_the_conflict_bar`, `after_cancel_the_keyboard_stays_in_the_editor` |
| closing a clean tab closes at once; `files.autoSave` `afterDelay` (once saved) and `onFocusChange` do not ask | ready | `cmd_w_on_a_clean_tab_closes_it_without_asking`, `auto_save_after_delay_saves_before_a_later_close_so_no_prompt_shows`, `on_focus_change_saves_a_dirty_tab_on_close_without_asking` |
| a damaged backup is kept aside, the others restore; a deleted file's text restores | ready | `a_damaged_backup_is_kept_aside_and_the_rest_restore`, `a_backup_of_a_file_deleted_since_restores_...` |
| unsaved text restores even when editors are not restored; an identical backup is dropped | ready | `unsaved_text_restores_even_when_editors_are_not_restored`, `a_backup_equal_to_the_file_is_dropped` |
| `files.hotExit: off` asks (Cancel, Save All, Don't Save); window close asks with `onExit` only | ready | `hot_exit_off_asks_and_cancel_keeps_the_app_open`, `hot_exit_off_save_all_saves_then_quits`, `hot_exit_off_dont_save_discards_but_keeps_the_text_aside`, `window_close_asks_with_on_exit_but_not_with_on_exit_and_window_close` |
| nothing restored: the Welcome tab shows; clicking a recent file opens it; Open File... is Quick Open | ready | `with_nothing_restored_the_welcome_tab_shows_and_a_recent_file_opens_on_click`, `welcome_open_file_opens_quick_open` (`config/e2e.rs`: `startup_editor_default_is_the_welcome_page`, `startup_editor_none_opens_nothing`) |
| "Show welcome page on startup" writes `workbench.startupEditor`; `Help: Welcome` opens the page | ready | `the_show_on_startup_checkbox_writes_workbench_startup_editor`, `help_welcome_is_a_registered_command_that_opens_the_page` |
| a workbench with no installed store reads and writes nothing | ready | `a_workbench_without_an_installed_store_writes_nothing` |

Breadcrumbs (ADR-0047, `crates/vsr-app/src/breadcrumbs/e2e_tests.rs`; the file path part only, the
symbol part is blocked on document symbols):

| scenario | status | test |
|---|---|---|
| the bar under the tab strip shows `src › main.rs` for the active file | ready | `the_bar_shows_the_path_of_the_active_file` |
| clicking `src` opens a dropdown listing its siblings, with `src` selected | ready | `clicking_a_crumb_lists_its_siblings_with_the_crumb_selected` |
| accepting a folder lists its entries; accepting a file opens it and closes the dropdown | ready | `accepting_a_folder_lists_its_entries_and_accepting_a_file_opens_it` |
| `breadcrumbs.filePath: last` shows only the file name; `off` hides the bar | ready | `file_path_last_shows_only_the_file_name`, `file_path_off_hides_the_bar` |
| `breadcrumbs.enabled: false` hides the bar | ready | `breadcrumbs_enabled_false_hides_the_bar` |
| the bar takes `breadcrumb.*` colours from the colour theme | ready | `the_bar_takes_its_colours_from_the_colour_theme` |
| Shift+Cmd+; focuses the last crumb; Left/Right move; Escape gives focus back | ready | `cmd_shift_semicolon_focuses_the_last_crumb_and_arrows_move_between_crumbs` |
| Shift+Cmd+. focuses the last crumb and opens its dropdown | ready | `cmd_shift_period_focuses_and_opens_the_dropdown_of_the_last_crumb` |
| files outside the workspace show their absolute path; sort order and `files.exclude` in the dropdown | ready | model tests `file_outside_the_root_shows_its_absolute_path`, `children_sort_folders_first_then_by_name`, `list_children_applies_files_exclude` (`breadcrumbs/model.rs`) |
| `breadcrumbs.showFiles` and the other symbol settings | blocked-on-feature document symbols | (none: needs the outline provider) |

### Activity bar settings (ADR-0050)

Headless (`crates/vsr-app/src/config/e2e.rs`). Checked on the laid-out boxes, `activity-bar`, `activity:<n>` and `side-bar`.

| scenario | status | test |
|---|---|---|
| `workbench.activityBar.compact` narrows the default bar to 36px with 28px items, live; the strip locations keep their size | ready | `activity_bar_compact_narrows_the_bar_live` |
| `workbench.activityBar.iconClickBehavior: toggle` hides the side bar on a click of the shown view's icon; `focus` keeps it open | ready | `activity_bar_icon_click_focus_keeps_the_side_bar_open` |
| `workbench.activityBar.autoHide` | blocked | see `upstream/settings.toml` (needs view containers that can be hidden) |
