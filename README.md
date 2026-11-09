# vscode-rust

A for-fun re-implementation of VS Code in Rust with a native UI, built by LLM agents.
Not affiliated with Microsoft; not for sale.

- Upstream reference: `../vscode-original` (pinned in `UPSTREAM.md`)
- Decisions: [`DECISIONS.md`](DECISIONS.md) · Research: [`docs/research/`](docs/research)

```sh
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
