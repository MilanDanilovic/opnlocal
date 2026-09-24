#!/usr/bin/env bash
# Boots an iPhone simulator, installs the simulator build, and runs the app's self-test: device
# detection plus a short real generation with a tiny model. Saves a screenshot for review.
# Requires macOS + Xcode and a prior `tauri ios build --target aarch64-sim`.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/target/ios"
mkdir -p "$out"

app=$(find "$root/app/src-tauri/gen/apple/build" -maxdepth 3 -name "opnlocal.app" | head -1)
[ -n "$app" ] || { echo "simulator .app not found"; exit 1; }
bundle=io.github.milandanilovic.opnlocal

device=$(xcrun simctl list devices available -j | python3 -c '
import json, sys
d = json.load(sys.stdin)["devices"]
print(next(x["udid"] for k, v in d.items() if "iOS" in k for x in v if x["name"].startswith("iPhone")))')
echo "simulator: $device"
xcrun simctl boot "$device" || true
xcrun simctl bootstatus "$device" -b
xcrun simctl install "$device" "$app"

data=$(xcrun simctl get_app_container "$device" "$bundle" data)
mkdir -p "$data/tmp"
curl -sSL -o "$data/tmp/test.gguf" "https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories15M-q4_0.gguf"

SIMCTL_CHILD_OPNLOCAL_SELFTEST="$data/tmp/test.gguf" \
  xcrun simctl launch --terminate-running-process "$device" "$bundle"
# The app writes its self-test lines to selftest.log in its data folder.
log=""
for _ in $(seq 1 60); do
  log=$(find "$data" -name selftest.log 2>/dev/null | head -1)
  [ -n "$log" ] && grep -qE "OPNLOCAL_SELFTEST (OK|FAILED)" "$log" && break
  sleep 2
done
sleep 2
xcrun simctl io "$device" screenshot "$out/simulator.png"
[ -n "$log" ] && cp "$log" "$out/selftest.log"
cat "$out/selftest.log" 2>/dev/null || echo "no self-test output"
grep -q "OPNLOCAL_SELFTEST OK" "$out/selftest.log"
