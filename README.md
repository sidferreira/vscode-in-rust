# vscode-rust

A for-fun re-implementation of VS Code in Rust with a native UI, built by LLM agents.
Not affiliated with Microsoft; not for sale.

- Upstream reference: `../vscode-original` (pinned in `UPSTREAM.md`)
- Decisions: [`DECISIONS.md`](DECISIONS.md) · Research: [`docs/research/`](docs/research)

```sh
cargo test          # all crates
cargo run -p vsr-app  # open the window
```

## Crates
| crate | role | mirrors upstream |
|---|---|---|
| `vsr-text` | text buffer, Position/Range | `src/vs/editor/common/{core,model}` |
