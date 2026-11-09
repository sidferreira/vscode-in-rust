# Preferences e2e scenarios

Upstream dir: `test/smoke/src/areas/preferences` | pinned sha: `50e1350fc78d` | scenario count: 5
Suites (describe): Preferences; Settings editor

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | turns off editor line numbers and verifies the live change | Preferences | `preferences/preferences.test.ts:15` | Opens user settings.json, waits for line-number gutter elements, adds `editor.lineNumbers: off`, and waits for them to disappear. | pending | Needs settings (vsr-settings), editing, line-number gutter. |
| 2 | changes "workbench.action.toggleSidebarPosition" command key binding and verifies it | Preferences | `preferences/preferences.test.ts:25` | Uses the keybindings editor to bind ctrl+u to toggle sidebar position, presses it, and waits for the activity bar to move right. | pending | it.skip upstream. Needs keybindings plus keybindings editor. |
| 3 | shows a modified indicator on a modified setting | Settings editor | `preferences/preferences.test.ts:41` | Searches `@id:editor.tabSize` in the settings UI, sets the input to 6, waits for the modified indicator, then resets to 4. | pending | Needs settings editor UI. |
| 4 | turns off editor line numbers and verifies the live change | Settings editor | `preferences/preferences.test.ts:51` | Creates an untitled file, types Enter, uses the settings UI dropdown to turn line numbers off, switches back to the tab and checks the gutter is gone. | pending | it.skip upstream (flaky). |
| 5 | hides the toc when searching depending on the search behavior | Settings editor | `preferences/preferences.test.ts:69` | Changes `settingsSearchTocBehavior` in the settings UI, searches, and asserts the TOC width/sash state for hide and show modes. | pending | it.skip upstream (flaky). |

## Notes

- 5 `it(` calls (3 skipped). Two share the title 'turns off editor line numbers and verifies the live change' in different suites.

## Soon vs far

Near-term (ready / pending):
- #1 turns off editor line numbers and verifies the live change [pending]
- #2 changes "workbench.action.toggleSidebarPosition" command key binding and verifies it [pending]
- #3 shows a modified indicator on a modified setting [pending]
- #4 turns off editor line numbers and verifies the live change [pending]
- #5 hides the toc when searching depending on the search behavior [pending]

Blocked, grouped by feature:
- none
