#!/usr/bin/env bash
# Builds an unsigned iOS device .ipa (arm64). A signing tool such as Sideloadly re-signs it
# with the tester's Apple ID. Requires macOS + Xcode and a generated gen/apple project.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/app"
npm run build

cd "$root/app/src-tauri/gen/apple"
project=$(ls -d ./*.xcodeproj | head -1)
scheme=$(basename "$project" .xcodeproj)_iOS
archive="$root/target/ios/opnlocal.xcarchive"
mkdir -p "$root/target/ios"

xcodebuild -project "$project" -scheme "$scheme" -configuration release \
  -sdk iphoneos -destination 'generic/platform=iOS' ARCHS=arm64 \
  -archivePath "$archive" archive \
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" | tail -40

app=$(find "$archive/Products/Applications" -maxdepth 1 -name "*.app" | head -1)
stage="$root/target/ios/stage"
rm -rf "$stage" && mkdir -p "$stage/Payload"
cp -R "$app" "$stage/Payload/"
(cd "$stage" && zip -qry "$root/target/ios/opnlocal-unsigned.ipa" Payload)
ls -la "$root/target/ios/opnlocal-unsigned.ipa"
