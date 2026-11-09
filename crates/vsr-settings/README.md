# vsr-settings

Settings data for rscode's Settings editor: the registered settings, the table of contents,
title formatting, and the user's settings.json (JSONC).

## Bundled data and where it comes from

| file | what | source |
|---|---|---|
| `assets/default-configuration.json` | 2062 registered settings: key, type, default, description (markdown), enum values/labels/descriptions, min/max, scope, order, tags, deprecation, and `source` (`core` or the built-in extension id) | VS Code **1.144.1** (`code --version`: `1.144.1`, commit `261c84cb1eddf11235f9e2c921425e9663fbe556`, arm64), the installed app. The upstream pin is 1.147.0 (`50e1350fc78d`); the export is one version older. |
| `assets/settings-layout.json` | the TOC (`tocData`) and "Commonly Used" list | port of `src/vs/workbench/contrib/preferences/browser/settingsLayout.ts` @ `50e1350fc78d` |

The top of `default-configuration.json` records the VS Code version and commit used, and the
upstream pin.

### How the settings were exported

`code --export-default-configuration` is parsed but never consumed (not in upstream 1.147 nor in
the installed 1.144.1 app), so it writes no file. The export instead reads the settings JSON schema
that the workbench registers as `vscode://schemas/settings/default`
(`configurationService.ts`, `defaultSettingsSchema`, built from `allSettings`). A throwaway
extension does this from an isolated profile and extensions folder, then quits the app. Built-in
extensions contribute their settings through the same registry, so they are included.

Regenerate (from the repo root, with VS Code installed; this launches an isolated VS Code instance
for a few seconds):

```sh
python3 -I scripts/export-default-settings
```

It writes to a fresh temp directory (the IPC socket path must be short, so it is not under the
scratchpad) and never deletes anything. `--code`, `--app-dir` and `--upstream-clone` override the
defaults.

The export drops `[lang]` language-override placeholders (not listed by the Settings editor).

## Behaviour ported from upstream

- Titles: `settingKeyToDisplayFormat` / `wordifyKey` / `trimCategoryForGroup`
  (`settingsTreeModels.ts`, `common/preferences.ts`), with the upstream tests in `tests/title.rs`
  (sandbox-specific label overrides excluded).
- TOC: children claim their settings before the parent; patterns (`*` wildcard, case-insensitive,
  whole key) and the sort (preview/experimental last, then `order`, then key) follow
  `settingsTree.ts`. `chat.disableAIFeatures` is the value of upstream's `ChatAIDisabledSettingId`.
- Hidden: settings tagged `advanced` are left out of the default TOC (the UI shows them only when
  "show advanced" is on). `features/issueReporter` is hidden on desktop and `application/other` on
  Windows, as upstream's `hide:` does.
- "Commonly Used" is first and "Extensions" last, as in `settingsEditor2.ts`.

## Deviations

- Settings are from VS Code 1.144.1, not the 1.147.0 pin.
- Extension groups use the configuration title from `package.json`, not upstream's configuration
  node ordering, so groups are not sorted by `order`.
- `search()` is a simple ranking (all words must match; name matches rank first; ties keep TOC
  order), not upstream's fuzzy search.
- Saving settings.json writes plain JSON, so comments in an existing file are lost.
- `default_user_settings_path()` is `~/Library/Application Support/rscode/User/settings.json`.
