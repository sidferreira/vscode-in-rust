#!/usr/bin/env bash
# Launch rscode like the `code` CLI: build if needed, open the window, return.
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
# --background  open without stealing focus or coming to the front (RSCODE_BACKGROUND=1);
#               for automated runs while you keep working
#
# `scripts/rscode` is the same script. Put it on your PATH, e.g. in ~/.zshrc:
#   alias rscode="$HOME/dev/personal/vscode-in-rust/vscode-rust/scripts/rscode"
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
    --background) export RSCODE_BACKGROUND=1 ;;
    -h|--help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
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

# Ask cargo for the target dir and vsr-app's binary name (rscode).
read -r target_dir bin_name < <(cargo metadata --format-version 1 --no-deps --manifest-path "$REPO/Cargo.toml" \
  | python3 -c 'import json,sys
m = json.load(sys.stdin)
app = next(p for p in m["packages"] if p["name"] == "vsr-app")
b = next(t["name"] for t in app["targets"] if "bin" in t["kind"])
print(m["target_directory"], b)')
raw_bin="$target_dir/$profile/$bin_name"

# Wrap the binary in a minimal .app bundle so macOS shows the rscode name and
# icon in the Dock and app switcher (a bare executable gets the generic icon).
app="$target_dir/$profile/rscode.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
# Copy to a temp file and rename into place: overwriting the binary in place (cp -f)
# keeps the same inode, and on Apple Silicon a running rscode using that file makes the
# next launch die with SIGKILL "Code Signature Invalid". A rename gives a new inode.
cp "$raw_bin" "$app/Contents/MacOS/.$bin_name.new"
mv -f "$app/Contents/MacOS/.$bin_name.new" "$app/Contents/MacOS/$bin_name"
icon_key=""
if [[ -f "$REPO/resources/darwin/rscode.icns" ]]; then
  cp -f "$REPO/resources/darwin/rscode.icns" "$app/Contents/Resources/rscode.icns"
  icon_key="<key>CFBundleIconFile</key><string>rscode</string>"
fi
cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>rscode</string>
  <key>CFBundleDisplayName</key><string>rscode</string>
  <key>CFBundleIdentifier</key><string>dev.rscode.app</string>
  <key>CFBundleExecutable</key><string>$bin_name</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>NSHighResolutionCapable</key><true/>
  $icon_key
</dict></plist>
PLIST
# Ad-hoc sign the whole bundle (local only, no certificate). The linker signs the bare
# executable, but inside a .app macOS validates the bundle (Info.plist, resources).
codesign --force --sign - "$app" >/dev/null 2>&1 || echo "warning: ad-hoc codesign failed" >&2
bin="$app/Contents/MacOS/$bin_name"

if [[ $wait -eq 1 ]]; then
  exec "$bin" "${args[@]}"
fi

log_dir="${TMPDIR:-/tmp}/rscode"
mkdir -p "$log_dir"
nohup "$bin" "${args[@]}" >"$log_dir/last-run.log" 2>&1 &
disown
echo "rscode started (pid $!), logs: $log_dir/last-run.log"
