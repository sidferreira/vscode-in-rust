# Preferences e2e scenarios

Upstream dir: `test/smoke/src/areas/preferences` | pinned sha: `50e1350fc78d` | scenario count: 5
Suites (describe): Preferences; Settings editor

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | turns off editor line numbers and verifies the live change | Preferences | `preferences/preferences.test.ts:15` | Opens user settings.json, waits for line-number gutter elements, adds `editor.lineNumbers: off`, and waits for them to disappear. | ready | `config::e2e::external_edit_of_settings_json_applies_line_numbers_off`: the temp settings.json is rewritten by "another program", the watcher applies `editor.lineNumbers: off` and the gutter shrinks (asserted by gutter width, not by DOM nodes). The file is edited on disk instead of in an rscode editor tab; saving an edited settings.json from a tab goes through the same watcher. |
| 2 | changes "workbench.action.toggleSidebarPosition" command key binding and verifies it | Preferences | `preferences/preferences.test.ts:25` | Uses the keybindings editor to bind ctrl+u to toggle sidebar position, presses it, and waits for the activity bar to move right. | pending | it.skip upstream. Needs keybindings plus keybindings editor. |
| 3 | shows a modified indicator on a modified setting | Settings editor | `preferences/preferences.test.ts:41` | Searches `@id:editor.tabSize` in the settings UI, sets the input to 6, waits for the modified indicator, then resets to 4. | ready | `settings_view::e2e_tests::number_setting_edit_shows_modified_and_reset_restores_default` (modified state asserted on the persisted user settings, reset via More Actions; the blue/gold bar itself is not pixel-checked). Also covered: `cmd_comma_opens_settings_and_search_ranks_font_size_first`, `toggling_a_bool_setting_persists_to_user_settings`. |
| 4 | turns off editor line numbers and verifies the live change | Settings editor | `preferences/preferences.test.ts:51` | Creates an untitled file, types Enter, uses the settings UI dropdown to turn line numbers off, switches back to the tab and checks the gutter is gone. | ready | it.skip upstream (flaky). Dropdown half: `settings::e2e_tests::enum_dropdown_selects_a_value` (editor.lineNumbers) and `config::e2e::side_bar_location_right_moves_the_side_bar_and_activity_bar` (same dropdown, applied live); gutter half: `config::e2e::external_edit_of_settings_json_applies_line_numbers_off`. No single test does both steps in one flow. |
| 5 | hides the toc when searching depending on the search behavior | Settings editor | `preferences/preferences.test.ts:69` | Changes `settingsSearchTocBehavior` in the settings UI, searches, and asserts the TOC width/sash state for hide and show modes. | pending | it.skip upstream (flaky). Filter mode (TOC narrowed to matching groups, with counts) is covered by `settings_view::e2e_tests::search_filters_the_toc_to_matching_groups_and_esc_restores_it`; the `hide` mode is not implemented. |

## Notes

- 5 `it(` calls (3 skipped). Two share the title 'turns off editor line numbers and verifies the live change' in different suites.

## Soon vs far

Near-term (ready / pending):
- #1 turns off editor line numbers and verifies the live change [ready]
- #2 changes "workbench.action.toggleSidebarPosition" command key binding and verifies it [pending]
- #3 shows a modified indicator on a modified setting [ready]
- #4 turns off editor line numbers and verifies the live change [ready]
- #5 hides the toc when searching depending on the search behavior [pending]

Blocked, grouped by feature:
- none

## rscode scenarios (not from upstream's smoke suite)

Settings that take effect live, headless (`crates/vsr-app/src/editor_view/minimap/e2e.rs`, ADR-0031). Layout and
the minimap model are asserted; the test platform's text system is a no-op, so no pixels.

| scenario | status | test |
|---|---|---|
| minimap is shown on the right by default, with a slider over the visible part | ready | `minimap_is_shown_by_default_on_the_right` |
| `editor.minimap.enabled: false` removes it and the editor gets its width back | ready | `disabling_the_minimap_removes_it_and_gives_the_editor_its_width_back` |
| `editor.minimap.side: left` puts it before the editor | ready | `side_left_puts_the_minimap_before_the_editor` |
| clicking the minimap centres the clicked line | ready | `clicking_the_minimap_centres_the_clicked_line` |
| slider bounds follow scrolling and stay inside the minimap | ready | `the_slider_tracks_scrolling_and_stays_inside_the_minimap` |
| dragging the slider scrolls the editor | ready | `dragging_the_slider_scrolls_the_editor` |
| `renderCharacters`, `scale`, `maxColumn` change line height / width | ready | `render_characters_scale_and_max_column_change_the_layout` |
| `size: fill` / `fit` (incl. sampling and click-to-drag) | ready | `size_fill_and_fit_stretch_the_minimap_over_the_editor_height`, `size_fill_samples_long_documents_and_clicks_drag_from_the_slider_centre` |
| `showSlider` and `autohide` (mouseover, scroll) | ready | `show_slider_and_autohide_follow_hover`, `autohide_scroll_shows_the_minimap_only_after_scrolling` |
| a 10k-line file draws only the lines that fit | ready | `a_10k_line_file_draws_only_the_lines_that_fit_and_stays_responsive` |

## Window and workbench settings (ADR-0035, headless, `config/e2e.rs`)

| scenario | status | test |
|---|---|---|
| `window.title` and `window.titleSeparator` change the title bar and the OS window title, live | ready | `window_title_and_separator_apply_live_to_the_window` |
| the title variables follow the active editor (short, medium, folder, language) | ready | `window_title_variables_follow_the_active_editor` |
| `window.commandCenter` shows a search box; a click opens Quick Open; off removes it | ready | `command_center_is_a_search_box_that_opens_quick_open`, `command_center_off_leaves_the_title_text_only` |
| `window.density.editorTabHeight: compact` makes the tab strip 22px, live | ready | `compact_editor_tabs_are_shorter_and_apply_live` |
| zoom in, out and reset change `window.zoomLevel` and scale the editor text | ready | `zoom_commands_change_the_level_and_scale_the_editor_text` (chrome does not zoom yet, ADR-0035) |
| `workbench.startupEditor: readme` opens the README; the default opens nothing | ready | `startup_editor_readme_opens_the_readme_and_the_default_opens_nothing`, `startup_editor_default_is_the_welcome_page_which_rscode_does_not_have_yet` |
