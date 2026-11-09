#!/usr/bin/env bash
# Launch vscode-rust like the `code` CLI: build if needed, open the window, return.
#
#   scripts/code.sh [--wait|-w] [--release] [folder-or-file ...]
#
# Default folder is the directory you run it from. Paths are resolved against
# your current directory, so you can call it from anywhere:
#   ~/dev/personal/vscode-in-rust/vscode-rust/scripts/code.sh ~/some/project
#
# --wait     stay attached (logs in the terminal, returns when the window closes),
#            like `code --wait`
# --release  build/run the optimised binary (slower first build, faster app)
#
# Tip: alias it, e.g. in ~/.zshrc:  alias code-rs="$HOME/dev/personal/vscode-in-rust/vscode-rust/scripts/code.sh"
#
# upstream: scripts/code.sh @ 50e1350fc78d   (same role: dev launcher)
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CALLER_DIR="$PWD"

# Homebrew's rustup keeps cargo outside the default PATH.
for d in /opt/homebrew/opt/rustup/bin "$HOME/.cargo/bin"; do
  [[ -d "$d" ]] && PATH="$d:$PATH"
done
export PATH

wait=0
profile=debug
args=()
for a in "$@"; do
  case "$a" in
    -w|--wait) wait=1 ;;
    --release) profile=release ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "code.sh: unknown option $a" >&2; exit 2 ;;
    *)
      # Resolve relative to where the user ran the command.
      if [[ "$a" = /* ]]; then args+=("$a"); else args+=("$CALLER_DIR/$a"); fi ;;
  esac
done
[[ ${#args[@]} -eq 0 ]] && args=("$CALLER_DIR")

cargo_flags=(--quiet -p vsr-app --manifest-path "$REPO/Cargo.toml")
[[ "$profile" = release ]] && cargo_flags+=(--release)

# Build (no-op if up to date). Cargo prints errors; we stop on failure.
cargo build "${cargo_flags[@]}"

target_dir="$(cargo metadata --format-version 1 --no-deps --manifest-path "$REPO/Cargo.toml" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
bin="$target_dir/$profile/vscode-rust"

if [[ $wait -eq 1 ]]; then
  exec "$bin" "${args[@]}"
fi

log_dir="${TMPDIR:-/tmp}/vscode-rust"
mkdir -p "$log_dir"
nohup "$bin" "${args[@]}" >"$log_dir/last-run.log" 2>&1 &
disown
echo "vscode-rust started (pid $!), logs: $log_dir/last-run.log"
