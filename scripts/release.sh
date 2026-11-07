#!/bin/bash
# Builds the signed universal Quietkey.app and Quietkey.dmg into dist/.
# Signs with the "Quietkey Signing" certificate (login keychain), so macOS keeps Accessibility, Input Monitoring and
# Microphone permissions across updates: the designated requirement names the certificate, not the binary's hash.
# Not notarized (needs an Apple Developer ID), so users still right-click > Open the first time.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
security find-identity -p codesigning | grep -q '"Quietkey Signing"' || { echo "Quietkey Signing certificate not in the keychain; restore ~/.quietkey-signing/quietkey-signing.p12"; exit 1; }

# app only: Tauri's DMG step scripts Finder and fails with "Resource busy" on some runs
npm run tauri build -- --target universal-apple-darwin --bundles app
app=src-tauri/target/universal-apple-darwin/release/bundle/macos/Quietkey.app

dr=$(codesign -dr - "$app" 2>&1)
[[ "$dr" == *'identifier "dev.abdallah.quietkey" and certificate root'* ]] || { echo "unexpected signature: $dr"; exit 1; }
codesign -d --entitlements - "$app" 2>&1 | grep -q audio-input || { echo "microphone entitlement missing"; exit 1; }
[[ "$(lipo -archs "$app/Contents/MacOS/quietkey")" == *x86_64*arm64* ]] || { echo "not universal"; exit 1; }

rm -rf dist && mkdir -p dist/dmg
ditto "$app" dist/dmg/Quietkey.app
ln -s /Applications dist/dmg/Applications
hdiutil create -volname Quietkey -srcfolder dist/dmg -ov -format UDZO dist/Quietkey.dmg >/dev/null
rm -rf dist/dmg
echo "dist/Quietkey.dmg $(shasum -a 256 dist/Quietkey.dmg | cut -d' ' -f1)"
