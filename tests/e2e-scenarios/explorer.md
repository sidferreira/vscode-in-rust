# Explorer e2e scenarios (ADR-0039, ADR-0041)

Scenarios written for rscode: the Explorer's file operations and tree behaviour (no upstream smoke
area covers them in this snapshot). Each row names its Rust test. Every test runs the real
workbench on a temp folder, with a fake watcher, a fake Trash that moves into a test-owned folder,
and an in-memory configuration, so it never touches the real Trash or the real settings file.

Status: `ready` = the test asserts the scenario; `pending` = the behaviour is built but a part of
it cannot be observed headless (the reason is in the scenario).

| # | scenario | setting(s) | status | test |
|---|---|---|---|---|
| 1 | Down, Right, Left move the selection over the visible rows and expand and collapse folders | | ready | `arrow_keys_move_the_selection_and_expand_and_collapse_folders` |
| 2 | Delete asks first; the file is only moved to the Trash after Move to Trash | explorer.confirmDelete | ready | `delete_asks_first_and_then_moves_the_file_to_the_trash` |
| 3 | Escape cancels a pending delete and keeps the file | explorer.confirmDelete | ready | `escape_cancels_the_delete_and_keeps_the_file` |
| 4 | with confirmDelete off, Delete deletes at once | explorer.confirmDelete | ready | `confirm_delete_false_deletes_at_once` |
| 5 | Delete Permanently never goes to the Trash | | ready | `delete_permanently_never_goes_to_the_trash` |
| 6 | with files.enableTrash off, Delete is permanent and asks the stronger question | files.enableTrash | ready | `with_files_enable_trash_off_delete_is_permanent_and_asks_the_stronger_question` |
| 7 | New File from the command: name it, Enter creates it in the selected folder and opens it | | ready | `new_file_from_the_command_takes_a_name_and_opens_the_file` |
| 8 | an existing name shows the upstream message under the box; Enter does not commit; nothing is created | | ready | `an_invalid_name_shows_the_message_and_creates_nothing` |
| 9 | Enter on a selected file opens the name box; Escape closes it and keeps the file | | ready | `enter_starts_rename_and_the_name_commits_on_enter` |
| 10 | Rename commits the typed name and moves the file | | ready | `rename_commits_the_typed_name_and_moves_the_file` |
| 11 | Rename selects the name without its last extension (folders: all) | | ready | `rename_selects_the_name_without_the_last_extension` |
| 12 | Rename moves the open editor to the new path | | ready | `rename_moves_the_open_tab_to_the_new_path` (explorer/e2e.rs) |
| 13 | sortOrder changes the order of the rows live (default, mixed, filesFirst; foldersNestsFiles as default) | explorer.sortOrder | ready | `sort_order_follows_explorer_sort_order_live` |
| 14 | a folder whose only child is a folder shows as one row a/b/c that opens the deepest folder | explorer.compactFolders | ready | `compact_folders_shows_a_single_child_chain_as_one_row` |
| 15 | autoReveal expands the folders above the opened file and selects it | explorer.autoReveal | ready | `auto_reveal_expands_and_selects_the_opened_file` |
| 16 | autoReveal focusNoScroll selects the file without scrolling the tree | explorer.autoReveal | ready | `auto_reveal_focus_no_scroll_selects_without_scrolling` |
| 17 | autoReveal false leaves the selection and the folders alone | explorer.autoReveal | ready | `auto_reveal_false_leaves_the_selection_alone` |
| 18 | autoReveal true scrolls the opened file into view in a tree taller than the window | explorer.autoReveal | pending | the scroll request is made (`reveal_in_tree`), but the headless tree's scroll offset does not change in gpui's test platform, so it is not asserted |
| 19 | Refresh Explorer lists folders again; Collapse Folders closes them | | ready | `refresh_explorer_relists_and_collapse_folders_closes_them` |
| 20 | an external create shows in the tree and keeps expanded folders | | ready | `an_external_create_refreshes_the_tree_and_keeps_expanded_folders` (explorer/e2e.rs) |
| 21 | a file created by New File appears in the tree and opens | | ready | `a_new_file_appears_in_the_tree_and_opens` (explorer/e2e.rs) |
| 22 | Delete moves to the Trash and closes the clean tab | | ready | `delete_moves_to_the_trash_and_closes_the_clean_tab` (explorer/e2e.rs) |
| 23 | Copy, paste and duplicate with explorer.incrementalNaming | explorer.incrementalNaming | blocked (needs Explorer copy, paste and duplicate) | none |
