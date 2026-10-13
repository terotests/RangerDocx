#!/usr/bin/env bash
# Regenerates docs/PARITY_REPORT.md from parity/*.tsv.
#   tools/parity/run.sh          # JavaScript target (node)
#   tools/parity/run.sh rust     # Rust target (rustc), same output
# Needs a Ranger checkout: $RANGER, or this repo cloned to <ranger>/gallery/docx_viewer.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
ranger="${RANGER:-$(cd "$root/../.." && pwd)}"
rgrc="$ranger/dist/rgrc.js"
[ -f "$rgrc" ] || { echo "Ranger compiler not found at $rgrc (set RANGER=<ranger checkout>)" >&2; exit 2; }
out="$root/.build/parity"
rm -rf "$out"; mkdir -p "$out"
target="${1:-es6}"
check() { if grep -q "Compilation FAILED" "$1"; then cat "$1" >&2; exit 1; fi; }
cd "$root"
if [ "$target" = "rust" ]; then
  node "$rgrc" -l=rust "$here/ParityReport.rgr" -d="$out" -o=parity.rs > "$out/log.txt" 2>&1 || true
  check "$out/log.txt"
  rustc -O "$out/parity.rs" -o "$out/parity" 2> "$out/rustc.txt" || { cat "$out/rustc.txt" >&2; exit 1; }
  "$out/parity" > /dev/null
else
  node "$rgrc" -l=es6 "$here/ParityReport.rgr" -d="$out" -o=parity.js -nodecli > "$out/log.txt" 2>&1 || true
  check "$out/log.txt"
  node "$out/parity.js" > /dev/null
fi
echo "wrote docs/PARITY_REPORT.md"
tail -n +1 docs/PARITY_REPORT.md | grep -A30 "^## All metrics" | head -8
