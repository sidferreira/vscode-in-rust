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

Blocked settings (no scenario until the feature lands) are listed with their reasons in
`upstream/settings.toml`.
