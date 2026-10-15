#!/bin/bash
# Packs target/release/bundle/Akidev.app into a disk image to hand to someone: open it, drag the
# app onto Applications. Builds the app first when it is not there.
#
# With SIGN_IDENTITY set the image is signed too, and with the App Store Connect API key
# (NOTARY_KEY_FILE, NOTARY_KEY_ID, NOTARY_ISSUER) Apple is asked to notarise it and the result is
# stapled to it, so macOS opens the app with no warning. Without them the app is signed ad hoc
# and macOS asks the first time it is opened; see the note the image carries.
set -euo pipefail
cd "$(dirname "$0")/.."

APP=target/release/bundle/Akidev.app
VERSION=$(cargo pkgid -p akidev | sed 's/.*[#@]//')
DMG="target/release/bundle/Akidev-$VERSION-$(uname -m).dmg"

[ -d "$APP" ] || ./scripts/bundle-macos.sh

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/Akidev.app"
ln -s /Applications "$STAGE/Applications"
cat > "$STAGE/Read me first.txt" <<'NOTE'
Akidev is not signed with an Apple Developer ID yet, so macOS stops it the first time.

1. Drag Akidev onto Applications.
2. Open it from Applications. If macOS says it cannot verify the app, open
   System Settings > Privacy & Security, scroll down and press "Open Anyway".
   Or, in Terminal:  xattr -dr com.apple.quarantine /Applications/Akidev.app

After that it opens like any other app.
NOTE

rm -f "$DMG"
hdiutil create -volname Akidev -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" > /dev/null
if [ -n "${SIGN_IDENTITY:-}" ]; then
  codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"
  if [ -n "${NOTARY_KEY_FILE:-}" ]; then
    xcrun notarytool submit "$DMG" --key "$NOTARY_KEY_FILE" --key-id "$NOTARY_KEY_ID" \
      --issuer "$NOTARY_ISSUER" --wait --timeout 30m
    xcrun stapler staple "$DMG"
  fi
fi
echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
