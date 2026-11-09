# vscode-rust

A for-fun re-implementation of VS Code in Rust with a native UI, built by LLM agents.
Not affiliated with Microsoft; not for sale.

- Upstream reference: `../vscode-original` (pinned in `UPSTREAM.md`)
- Decisions: [`DECISIONS.md`](DECISIONS.md) · Research: [`docs/research/`](docs/research)

Homebrew's rustup keeps `cargo` outside the default PATH; prefix commands with
`PATH=/opt/homebrew/opt/rustup/bin:$PATH` (or add that to your shell profile).
The first build takes a few minutes.

```sh
scripts/code.sh [folder]         # build if needed and open the window (like `code .`)
cargo test                       # all crates
cargo run -p vsr-app -- [folder]  # open the window on a folder (default: cwd)
scripts/upstream-sync status     # what changed upstream in code we ported
scripts/cargo-firewall-fetch     # fetch deps through the package firewall
```

## Crates
| crate | role | mirrors upstream |
|---|---|---|
| `vsr-text` | text buffer, Position/Range | `src/vs/editor/common/{core,model}` |
| `vsr-app` | native workbench window (egui) | `src/vs/workbench/browser/layout.ts`, `dark_modern.json` |

## Third-party assets
- **Codicons** (`crates/vsr-app/assets/codicon.ttf`) by Microsoft, from
  [microsoft/vscode-codicons](https://github.com/microsoft/vscode-codicons) (`@vscode/codicons` 0.0.46-49),
  licensed [CC BY 4.0](crates/vsr-app/assets/codicon-LICENSE.txt). Used unmodified.
