#!/usr/bin/env bash
# Launches a built desktop app with OPNLOCAL_SELFTEST: it prints device detection and a short real
# generation with a tiny model, then keeps running so a screenshot can be taken.
#   scripts/desktop-selftest.sh <path-to-executable> <output-dir>
# Linux: run under xvfb-run. macOS: needs a GUI session (GitHub macOS runners have one).
set -euo pipefail
exe="$1"
# Absolute, since an AppImage's launcher changes the working directory.
mkdir -p "$2"
out=$(cd "$2" && pwd)
model="$out/test.gguf"
[ -f "$model" ] || curl -sSL -o "$model" "https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"

OPNLOCAL_DATA_DIR="$out/data" OPNLOCAL_SELFTEST="$model" "$exe" > "$out/selftest.log" 2>&1 &
pid=$!
for _ in $(seq 1 60); do
  grep -qE "OPNLOCAL_SELFTEST (OK|FAILED)" "$out/selftest.log" 2>/dev/null && break
  kill -0 "$pid" 2>/dev/null || break
  sleep 2
done
sleep 3
case "$(uname)" in
  Darwin) screencapture -x "$out/screenshot.png" || true ;;
  Linux) import -window root "$out/screenshot.png" || true ;;
esac
kill "$pid" 2>/dev/null || true
grep "OPNLOCAL_SELFTEST" "$out/selftest.log" || { tail -40 "$out/selftest.log"; exit 1; }
grep -q "OPNLOCAL_SELFTEST OK" "$out/selftest.log"
