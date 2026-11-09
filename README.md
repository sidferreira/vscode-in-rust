# rscode

A for-fun re-implementation of VS Code in Rust with a native UI, built by LLM agents.
(Repo/folder are still called vscode-rust; internal crates keep the `vsr-` prefix.)
Not affiliated with Microsoft; not for sale.

- Upstream reference: `../vscode-original` (pinned in `UPSTREAM.md`)
- Decisions: [`DECISIONS.md`](DECISIONS.md) · Research: [`docs/research/`](docs/research) · Progress: [`docs/progress.html`](docs/progress.html)

Homebrew's rustup keeps `cargo` outside the default PATH; prefix commands with
`PATH=/opt/homebrew/opt/rustup/bin:$PATH` (or add that to your shell profile).
The first build takes a few minutes.

```sh
scripts/rscode [folder]          # build if needed and open the window (like `code .`)
cargo test                       # all crates
cargo run -p vsr-app -- [folder]  # open the window on a folder (default: cwd)
scripts/upstream-sync status     # what changed upstream in code we ported
scripts/cargo-firewall-fetch     # fetch deps through the package firewall
```

## Crates
| crate | role | mirrors upstream |
|---|---|---|
| `vsr-text` | text buffer, Position/Range | `src/vs/editor/common/{core,model}` |
| `vsr-editor` | cursor, selection, typing, undo/redo (UI-free) | `src/vs/editor/common/cursor/*` |
| `vsr-search` | find-in-files engine on the ripgrep libraries (UI-free) | `src/vs/workbench/services/search/{common,node}` |
| `vsr-quickinput` | fuzzy scorer, word filters, quick pick model, quick open / command palette / go to line picks, quick access registry (UI-free) | `src/vs/base/common/fuzzyScorer.ts`, `src/vs/platform/quickinput`, `anythingQuickAccess.ts` |
| `vsr-commands` | command registry (`CommandsRegistry`, palette metadata, preconditions, history, key binding labels) (UI-free) | `src/vs/platform/commands`, `src/vs/platform/actions`, `src/vs/platform/contextkey` |
| `vsr-find` | editor Find / Replace model: matches, N of M, next/prev, replace one/all (UI-free) | `src/vs/editor/contrib/find/browser/{findModel,findState,replacePattern}.ts` |
| `vsr-settings` | settings registry, `settings.json`, effective configuration (defaults + user, change events) | `src/vs/platform/configuration/common/*`, `workbench/contrib/preferences` |
| `vsr-theme` | VS Code colour themes from their JSON (include chains, token colours, customisations), colour registry defaults, theme selection (UI-free) | `src/vs/workbench/services/themes`, `src/vs/platform/theme`, `src/vs/base/common/color.ts`, `extensions/theme-defaults` |
| `vsr-languages` | language registry, file name / extension / first line associations, bundled language, grammar and Dark Modern theme data (ADR-0038) | `src/vs/editor/common/services/{languagesRegistry,languagesAssociations}.ts`, `extensions/*/package.json` |
| `vsr-textmate` | TextMate grammar engine (vscode-textmate semantics on Oniguruma), scope selectors, theme scope matching (UI-free) | `src/vs/workbench/services/textMate`, microsoft/vscode-textmate |
| `vsr-syntax` | syntax highlighting service: grammar per language, incremental per-line tokens (background thread), token styles | `src/vs/editor/common/model/tokens` |
| `vsr-app` | native workbench window (gpui via gpui-kit); `RSCODE_USER_DATA_DIR=<dir>` redirects settings.json to `<dir>/User/` | `src/vs/workbench/browser/layout.ts`, `dark_modern.json` |

## Third-party assets
- **Colour themes** (`crates/vsr-theme/assets/themes/*.json`): the ten themes of VS Code's built-in `theme-defaults`
  extension, copied unchanged from [microsoft/vscode](https://github.com/microsoft/vscode), MIT,
  Copyright (c) 2015 - present Microsoft Corporation ([license](crates/vsr-theme/assets/themes/LICENSE.txt)).
- **Codicons** (`crates/vsr-app/assets/codicon.ttf`) by Microsoft, from
  [microsoft/vscode-codicons](https://github.com/microsoft/vscode-codicons) (`@vscode/codicons` 0.0.46-49),
  licensed [CC BY 4.0](crates/vsr-app/assets/codicon-LICENSE.txt). Modified: 'm' mapped to
  the blank glyph so gpui loads the font ([notice](crates/vsr-app/assets/codicon-NOTICE.md)).
- **TextMate grammars and themes** (`crates/vsr-languages/data`): copied unmodified from VS Code's built-in
  extensions (Microsoft, MIT) by `scripts/bundle-grammars`. Each grammar comes from the upstream repository named
  in its extension's `cgmanifest.json` (for example TypeScript-TmLanguage, MagicPython, rust-syntax,
  better-shell-syntax, better-cpp-syntax: MIT; the html and c grammars come from TextMate bundles under the
  "TextMate Bundle License"). Repositories, commits and licence texts:
  [crates/vsr-languages/data/NOTICES.md](crates/vsr-languages/data/NOTICES.md).
- **Oniguruma** (BSD-2-Clause, K. Kosako) through the [`onig`](https://crates.io/crates/onig) crate (MIT) runs
  those grammars' regular expressions.
- **App icon** (`resources/darwin/rscode.icns`): original copper ribbon mark drawn by
  `scripts/make-app-icon.swift`, with Ferris the crab (CC0, rustacean.net). See `resources/icon-src/`.
