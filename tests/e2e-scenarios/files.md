# Files e2e scenarios (`files.*` settings, ADR-0034)

Scenarios written for rscode (no upstream smoke area covers these settings headless). Each row names
its Rust test in `crates/vsr-app/src/config/e2e_files.rs`. Every test runs the real workbench on a temp
folder with a temp `settings.json` and asserts the file on disk.

| # | scenario | setting(s) | status | test |
|---|---|---|---|---|
| 1 | trailing empty lines are removed on save, one line break kept | files.trimFinalNewlines | ready | `trim_final_newlines_keeps_exactly_one_line_break_on_save` |
| 2 | the setting is off by default | files.trimFinalNewlines | ready | `trim_final_newlines_is_off_by_default` |
| 3 | final newline is inserted first, then the empty lines are trimmed | files.trimFinalNewlines, files.insertFinalNewline | ready | `trim_final_newlines_runs_after_insert_final_newline` |
| 4 | a one-line file gets CRLF line breaks; a file with breaks keeps its own | files.eol | ready | `eol_crlf_is_the_line_break_of_a_text_without_one` |
| 5 | auto is LF on macOS, and a changed setting applies to the next open | files.eol | ready | `eol_defaults_to_lf_and_can_change_live` |
| 6 | no auto save by default | files.autoSave | ready | `auto_save_is_off_by_default` |
| 7 | afterDelay saves at the delay after the last edit, not before | files.autoSave, files.autoSaveDelay | ready | `after_delay_saves_once_the_delay_passes_since_the_last_edit` |
| 8 | each edit restarts the delay | files.autoSave, files.autoSaveDelay | ready | `after_delay_restarts_on_every_edit` |
| 9 | an auto save runs the save participants and leaves the caret line's spaces | files.autoSave, files.trimTrailingWhitespace | ready | `after_delay_uses_the_save_path_and_leaves_the_caret_line_alone` |
| 10 | workspace-only auto save skips files outside the opened folder | files.autoSaveWorkspaceFilesOnly | ready | `auto_save_workspace_files_only_skips_files_outside_the_folder` |
| 11 | onFocusChange saves the editor being left | files.autoSave (onFocusChange) | ready | `on_focus_change_saves_the_editor_being_left` |
| 12 | onWindowChange saves every dirty editor when the window loses focus | files.autoSave (onWindowChange) | ready | `on_window_change_saves_dirty_editors_when_the_window_loses_focus` |
| 13 | matching files open read-only, excluded ones stay writable; edits are refused | files.readonlyInclude, files.readonlyExclude | ready | `readonly_include_opens_matching_files_read_only` |
| 14 | a file without owner write permission is read-only when the setting is on | files.readonlyFromPermissions | ready | `readonly_from_permissions_follows_the_owner_write_bit` |
| 15 | the session toggle makes a read-only file writable and back | files.readonlyInclude (toggleActiveEditorReadonlyInSession) | ready | `toggle_read_only_in_session_overrides_the_settings` |
| 16 | Search's Replace leaves a read-only file alone and replaces a writable one | files.readonlyInclude | ready | `search_replace_leaves_read_only_files_alone` |

## Untitled text (ADR-0058)

Our own scenarios (no upstream smoke area covers new files headless). Each row's test is in
`crates/vsr-app/src/untitled_e2e.rs`. Save As takes its path from the Explorer's name box; the tests
call the same commit path with the path they choose, and no file dialog opens.

| # | scenario | setting(s) | status | test |
|---|---|---|---|---|
| 1 | File: New Untitled Text File opens an empty clean tab named Untitled-1 | - | ready | `new_untitled_text_is_an_empty_clean_tab_named_untitled_1` |
| 2 | Cmd+N and Cmd+S reach the untitled commands through the keymap | - | ready | `cmd_n_and_cmd_s_reach_the_untitled_commands` |
| 3 | typing makes the text dirty, and the tab takes its first line; emptied it is clean | workbench.editor.untitled.labelFormat | ready | `typing_makes_the_text_dirty_and_the_tab_takes_its_first_line` |
| 4 | labelFormat `name` keeps the resource name as the tab title | workbench.editor.untitled.labelFormat | ready | `label_format_name_keeps_the_resource_name_as_the_title` |
| 5 | a new untitled text has the language of files.defaultLanguage | files.defaultLanguage | ready | `files_default_language_is_the_language_of_a_new_untitled_text` |
| 6 | files.defaultLanguage `${activeEditorLanguage}` takes the active editor's language | files.defaultLanguage | ready | `active_editor_language_is_the_default_when_the_setting_says_so` |
| 7 | Save As writes the text; the tab becomes the file, clean, with its language from the new name | - | ready | `save_as_writes_the_text_and_the_tab_becomes_the_file` |
| 8 | Save As refuses a name that is already a file; the untitled text stays dirty | - | ready | `save_as_refuses_a_name_that_is_already_a_file` |
| 9 | Save on an untitled text opens the name box in the Explorer, with its title | - | ready | `save_on_an_untitled_text_opens_the_name_box_with_its_title` |
| 10 | closing a dirty untitled text asks; Don't Save drops it and moves its backup aside | files.hotExit | ready | `closing_a_dirty_untitled_text_asks_and_dont_save_drops_it` |
| 11 | closing a clean untitled text closes it without asking | - | ready | `closing_a_clean_untitled_text_does_not_ask` |
| 12 | a closed Untitled-N number is used again by the next new text | - | ready | `a_closed_untitled_number_is_reused` |
| 13 | hot exit keeps the untitled text; a restart brings it back dirty, as an untitled text | files.hotExit | ready | `hot_exit_brings_the_untitled_text_back_as_an_untitled_text` |
| 14 | the missing-file check never closes an untitled text | - | ready | `an_untitled_text_is_not_closed_as_a_missing_file` |

Blocked settings (no scenario until the feature lands) are listed with their reasons in
`upstream/settings.toml`.
