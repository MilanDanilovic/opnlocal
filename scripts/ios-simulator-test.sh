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
  xcrun simctl launch --console-pty --terminate-running-process "$device" "$bundle" > "$out/selftest.log" 2>&1 &
launcher=$!
for _ in $(seq 1 60); do
  grep -qE "OPNLOCAL_SELFTEST (OK|FAILED)" "$out/selftest.log" 2>/dev/null && break
  sleep 2
done
sleep 3
xcrun simctl io "$device" screenshot "$out/simulator.png"
kill "$launcher" 2>/dev/null || true
grep "OPNLOCAL_SELFTEST" "$out/selftest.log" || true
grep -q "OPNLOCAL_SELFTEST OK" "$out/selftest.log"
