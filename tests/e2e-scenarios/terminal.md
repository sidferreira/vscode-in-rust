# Terminal e2e scenarios

Upstream dir: `test/smoke/src/areas/terminal` | pinned sha: `50e1350fc78d` | scenario count: 44
Suites (describe): Terminal (wrapper); Terminal Editors; Terminal Input (Auto replies); Terminal Persistence (output replay, detach/attach); Terminal Profiles; Terminal Tabs; Terminal Shell Integration (Decorations, Write data-based tests); Terminal stickyScroll; Terminal splitCwd

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | restores completed output exactly once and in order after ${reconnect} | Terminal Persistence > output replay | `terminal/terminal-persistence.test.ts:32` | Runs a node producer printing 100 numbered lines, then reloads the window or detaches/attaches, and verifies output is replayed once in order. | blocked-on-feature terminal | 1 source it looped over 'window reload' and 'detach/attach' (2 runtime). Skipped on remote/web. |
| 2 | should support basic reconnection | Terminal Persistence > detach/attach | `terminal/terminal-persistence.test.ts:80` | Creates a terminal, detaches the session, asserts the view hides, reattaches by name and asserts the group is back. | blocked-on-feature terminal |  |
| 3 | should persist buffer content | Terminal Persistence > detach/attach | `terminal/terminal-persistence.test.ts:104` | Writes output, detaches and reattaches, and checks buffer content remains. | blocked-on-feature terminal | it.skip upstream. |
| 4 | should inherit cwd when split and update the tab description - alt click | Terminal splitCwd | `terminal/terminal-splitCwd.test.ts:27` | Creates a terminal, mkdirs and cds, alt-clicks the tab to split, and expects both tab descriptions to show the cwd. | blocked-on-feature terminal | Suite skipped on Linux and Windows. |
| 5 | should launch the default profile | Terminal Profiles | `terminal/terminal-profiles.test.ts:29` | Shows the terminal and asserts a single tab with a non-debug profile name. | blocked-on-feature terminal |  |
| 6 | should set the default profile to a contributed one | Terminal Profiles | `terminal/terminal-profiles.test.ts:36` | Selects a contributed default profile, creates a terminal, checks the tab name. | blocked-on-feature terminal | it.skip upstream; needs extension host for the contributed profile. |
| 7 | should use the default contributed profile on panel open and for splitting | Terminal Profiles | `terminal/terminal-profiles.test.ts:42` | Selects the contributed default profile, shows the panel, splits, asserts both tabs use it. | blocked-on-feature terminal | Contributed profile comes from an extension. |
| 8 | should set the default profile | Terminal Profiles | `terminal/terminal-profiles.test.ts:49` | Selects a default profile, creates a terminal, asserts the tab name. | blocked-on-feature terminal |  |
| 9 | should use the default profile on panel open and for splitting | Terminal Profiles | `terminal/terminal-profiles.test.ts:55` | Shows the panel, asserts a single tab, splits, asserts two tabs in one group. | blocked-on-feature terminal |  |
| 10 | createWithProfile command should create a terminal with a profile | Terminal Profiles | `terminal/terminal-profiles.test.ts:62` | Runs the new-with-profile command and asserts a single tab. | blocked-on-feature terminal |  |
| 11 | createWithProfile command should create a terminal with a contributed profile | Terminal Profiles | `terminal/terminal-profiles.test.ts:67` | Runs new-with-profile choosing the contributed profile; asserts its name. | blocked-on-feature terminal | Needs extension host. |
| 12 | createWithProfile command should create a split terminal with a profile | Terminal Profiles | `terminal/terminal-profiles.test.ts:72` | Shows panel, runs new-with-profile in split mode, asserts a 2-terminal group. | blocked-on-feature terminal |  |
| 13 | createWithProfile command should create a split terminal with a contributed profile | Terminal Profiles | `terminal/terminal-profiles.test.ts:78` | Shows panel, runs split new-with-profile with the contributed profile, asserts mixed names. | blocked-on-feature terminal | Needs extension host. |
| 14 | should update color of the tab | Terminal Editors | `terminal/terminal-editors.test.ts:27` | Creates a terminal editor, changes its color to Cyan, asserts the tab color. | blocked-on-feature terminal |  |
| 15 | should rename the tab | Terminal Editors | `terminal/terminal-editors.test.ts:34` | Creates a terminal editor, renames it, asserts the tab name. | blocked-on-feature terminal |  |
| 16 | should show the panel when the terminal is moved there and close the editor | Terminal Editors | `terminal/terminal-editors.test.ts:41` | Creates an editor terminal and moves it to the panel; asserts a single panel tab. | blocked-on-feature terminal |  |
| 17 | should open a terminal in a new group for open to the side | Terminal Editors | `terminal/terminal-editors.test.ts:47` | Creates an editor terminal, splits the editor, asserts 2 editor groups. | blocked-on-feature terminal |  |
| 18 | should open a terminal in a new group when the split button is pressed | Terminal Editors | `terminal/terminal-editors.test.ts:53` | Creates an editor terminal, clicks the split button, asserts 2 groups. | blocked-on-feature terminal |  |
| 19 | should create new terminals in the active editor group via command | Terminal Editors | `terminal/terminal-editors.test.ts:59` | Creates two editor terminals via command, asserts 1 editor group. | blocked-on-feature terminal |  |
| 20 | should create new terminals in the active editor group via plus button | Terminal Editors | `terminal/terminal-editors.test.ts:65` | Creates an editor terminal and clicks the plus button, asserts 1 group. | blocked-on-feature terminal |  |
| 21 | should create a terminal in the editor area by default | Terminal Editors | `terminal/terminal-editors.test.ts:71` | Sets defaultLocation to editor, closes all editors, creates a terminal, asserts 1 group and hidden panel. | blocked-on-feature terminal |  |
| 22 | Placeholder | Terminal Shell Integration > Decorations > default icons | `terminal/terminal-shellIntegration.test.ts:39` | Creates a shell-integration profile and asserts 1 placeholder decoration. | blocked-on-feature terminal | Process-based; skipped on Windows. |
| 23 | Success | Terminal Shell Integration > Decorations > default icons | `terminal/terminal-shellIntegration.test.ts:43` | Runs `echo success`, asserts placeholder 1 and success 1. | blocked-on-feature terminal |  |
| 24 | Error | Terminal Shell Integration > Decorations > default icons | `terminal/terminal-shellIntegration.test.ts:48` | Runs `false`, asserts placeholder 1 and error 1. | blocked-on-feature terminal |  |
| 25 | never | Terminal Shell Integration > Decorations > decorationsEnabled | `terminal/terminal-shellIntegration.test.ts:67` | Sets decorationsEnabled to never; asserts zero decorations. | blocked-on-feature terminal |  |
| 26 | both | Terminal Shell Integration > Decorations > decorationsEnabled | `terminal/terminal-shellIntegration.test.ts:71` | Sets both; asserts all decorations visible. | blocked-on-feature terminal |  |
| 27 | gutter | Terminal Shell Integration > Decorations > decorationsEnabled | `terminal/terminal-shellIntegration.test.ts:75` | Sets gutter; asserts gutter decorations. | blocked-on-feature terminal |  |
| 28 | overviewRuler | Terminal Shell Integration > Decorations > decorationsEnabled | `terminal/terminal-shellIntegration.test.ts:79` | Sets overviewRuler; asserts ruler decorations. | blocked-on-feature terminal |  |
| 29 | should handle the simple case | Terminal Shell Integration > Write data-based tests > VS Code sequences | `terminal/terminal-shellIntegration.test.ts:108` | Writes OSC 633 A/B/C/D sequences into an empty terminal and asserts decoration counts after each. | blocked-on-feature terminal | Title duplicated in the Final Term suite. |
| 30 | should handle the simple case | Terminal Shell Integration > Write data-based tests > Final Term sequences | `terminal/terminal-shellIntegration.test.ts:123` | Same flow with OSC 133 sequences. | blocked-on-feature terminal |  |
| 31 | should automatically reply to a custom entry | Terminal Input > Auto replies | `terminal/terminal-input.test.ts:40` | Sets `terminal.integrated.autoReplies` to foo->bar, writes 'foo' to the terminal, waits for 'foo...bar' in the buffer. | blocked-on-feature terminal | Retries 3 on Windows. |
| 32 | should show sticky scroll when appropriate | Terminal stickyScroll | `terminal/terminal-stickyScroll.test.ts:60` | Writes VS Code shell-integration sequences with 50 output lines and asserts the sticky-scroll element shows the prompt and command. | blocked-on-feature terminal | Suite is always skipped (skipSuite: true in terminal.test.ts). |
| 33 | should support multi-line prompt | Terminal stickyScroll | `terminal/terminal-stickyScroll.test.ts:70` | Same with multi-line prompts and expected line counts of 2, 1, 2. | blocked-on-feature terminal | Suite is always skipped. |
| 34 | clicking the plus button should create a terminal and display the tabs view showing no split decorations | Terminal Tabs | `terminal/terminal-tabs.test.ts:26` | Creates a terminal and clicks plus; asserts two groups of one terminal. | blocked-on-feature terminal |  |
| 35 | should rename the single tab | Terminal Tabs | `terminal/terminal-tabs.test.ts:32` | Creates a terminal, renames it, asserts the single tab name. | blocked-on-feature terminal |  |
| 36 | should reset the tab name to the default value when no name is provided | Terminal Tabs | `terminal/terminal-tabs.test.ts:40` | Renames then renames with undefined and asserts the default name returns. it.skip upstream. | blocked-on-feature terminal | it.skip upstream. |
| 37 | should rename the tab in the tabs list | Terminal Tabs | `terminal/terminal-tabs.test.ts:50` | Creates and splits, renames, asserts group names. | blocked-on-feature terminal |  |
| 38 | should create a split terminal when single tab is alt clicked | Terminal Tabs | `terminal/terminal-tabs.test.ts:58` | Alt-clicks the single tab and asserts a 2-terminal group. | blocked-on-feature terminal |  |
| 39 | should do nothing when join tabs is run with only one terminal | Terminal Tabs | `terminal/terminal-tabs.test.ts:67` | Shows panel, runs join, asserts nothing changed. | blocked-on-feature terminal |  |
| 40 | should do nothing when join tabs is run with only split terminals | Terminal Tabs | `terminal/terminal-tabs.test.ts:73` | Splits, runs join, asserts still two groups. | blocked-on-feature terminal |  |
| 41 | should join tabs when more than one non-split terminal | Terminal Tabs | `terminal/terminal-tabs.test.ts:80` | Creates two terminals, runs join, asserts one group of two. | blocked-on-feature terminal |  |
| 42 | should do nothing when unsplit tabs called with no splits | Terminal Tabs | `terminal/terminal-tabs.test.ts:87` | Creates two terminals, runs unsplit, asserts unchanged. | blocked-on-feature terminal |  |
| 43 | should unsplit tabs | Terminal Tabs | `terminal/terminal-tabs.test.ts:95` | Splits then unsplits and asserts two groups. | blocked-on-feature terminal |  |
| 44 | should move the terminal to the editor area | Terminal Tabs | `terminal/terminal-tabs.test.ts:103` | Runs move-to-editor and asserts one editor group. | blocked-on-feature terminal |  |

## Notes

- 44 `it(` calls across 8 *.test.ts files (editors 8, input 1, persistence 3, profiles 9, shellIntegration 9, splitCwd 1, stickyScroll 2, tabs 11); terminal.test.ts has 0 and only composes the sub-suites. 3 are `it.skip`.
- Runtime count is higher: the output-replay it expands to 2.
- Shared helper: terminal-helpers.ts (`setTerminalTestSettings`). Sub-suites are skipped on Linux (pty host crashes, issue 216564); stickyScroll always; splitCwd also on Windows. The parent suite kills all terminals in afterEach.
- Tests within a file share state via user settings set in before() and cleared in after(), and rely on the contributed profile from a smoketest extension.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- terminal: #1, #2, #3, #4, #5, #6, #7, #8, #9, #10, #11, #12, #13, #14, #15, #16, #17, #18, #19, #20, #21, #22, #23, #24, #25, #26, #27, #28, #29, #30, #31, #32, #33, #34, #35, #36, #37, #38, #39, #40, #41, #42, #43, #44
