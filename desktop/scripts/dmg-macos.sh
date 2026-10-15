#!/bin/bash
# Packs target/release/bundle/Akidev.app into a disk image to hand to someone: open it, drag the
# app onto Applications. Builds the app first when it is not there.
#
# With SIGN_IDENTITY set the image is signed too, and with the App Store Connect API key
# (NOTARY_KEY_FILE, NOTARY_KEY_ID, NOTARY_ISSUER) Apple is asked to notarise it and the result is
# stapled to it, so macOS opens the app with no warning. Without them the app is signed ad hoc
# and macOS asks the first time it is opened.
set -euo pipefail
cd "$(dirname "$0")/.."

APP=target/release/bundle/Akidev.app
VERSION=$(cargo pkgid -p akidev | sed 's/.*[#@]//')
DMG="target/release/bundle/Akidev-$VERSION-$(uname -m).dmg"

[ -d "$APP" ] || ./scripts/bundle-macos.sh

# The window's look (a background picture and where the icons sit) is written by dmgbuild, which
# needs no Finder and so also works on CI. It is installed into a private virtualenv.
VENV=target/.dmgbuild-venv
if [ ! -x "$VENV/bin/dmgbuild" ]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install --quiet dmgbuild
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
# One background with a sharp @2x picture inside, which Finder picks on a Retina screen.
tiffutil -cathidpicheck assets/dmg/background.png assets/dmg/background@2x.png -out "$WORK/background.tiff" > /dev/null
cp "$APP/Contents/Resources/AppIcon.icns" "$WORK/volume.icns"

rm -f "$DMG"
DMG_APP="$APP" DMG_BACKGROUND="$WORK/background.tiff" DMG_VOLUME_ICON="$WORK/volume.icns" \
  "$VENV/bin/dmgbuild" -s scripts/dmg-settings.py Akidev "$DMG" > /dev/null

if [ -n "${SIGN_IDENTITY:-}" ]; then
  codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"
  if [ -n "${NOTARY_KEY_FILE:-}" ]; then
    xcrun notarytool submit "$DMG" --key "$NOTARY_KEY_FILE" --key-id "$NOTARY_KEY_ID" \
      --issuer "$NOTARY_ISSUER" --wait --timeout 30m
    xcrun stapler staple "$DMG"
  fi
fi
echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
