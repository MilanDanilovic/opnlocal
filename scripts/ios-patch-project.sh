#!/usr/bin/env bash
# Adjusts the Xcode project that `tauri ios init` generates (gen/apple is regenerated in CI).
# - Memory entitlements: let the model use more of the phone's RAM before iOS kills the app.
#   Only honored when the signing identity allows them; a free Apple ID may strip them.
set -euo pipefail
cd "$(dirname "$0")/../app/src-tauri/gen/apple"

ent=$(ls ./*_iOS/*.entitlements | head -1)
/usr/libexec/PlistBuddy -c "Add :com.apple.developer.kernel.increased-memory-limit bool true" "$ent" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :com.apple.developer.kernel.extended-virtual-addressing bool true" "$ent" 2>/dev/null || true
echo "Patched $ent:"
cat "$ent"
