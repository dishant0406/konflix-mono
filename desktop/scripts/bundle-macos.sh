#!/bin/bash
# Builds target/release/bundle/Akidev.app with libmpv and everything it loads inside it,
# so the app runs on a Mac that has no Homebrew.
#
# Needs: `brew install mpv`, and Xcode's command line tools (otool, install_name_tool, codesign).
#
# SIGN_IDENTITY, when set (for example "Developer ID Application: Name (TEAMID)"), signs the app
# and every library in it with that identity, with the hardened runtime and a timestamp, as
# notarisation requires. Without it the bundle is signed ad hoc, which is enough to run locally.
set -euo pipefail
cd "$(dirname "$0")/.."

APP=target/release/bundle/Akidev.app
FRAMEWORKS="$APP/Contents/Frameworks"

cargo build --release -p akidev
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$FRAMEWORKS"
cp target/release/akidev "$APP/Contents/MacOS/Akidev"
# Editing load paths would warn about every existing signature, so they are removed first
# and the whole bundle is signed again at the end.
codesign --remove-signature "$APP/Contents/MacOS/Akidev"

# The Dock and Finder icon, made from assets/icon.png at the sizes macOS asks for.
ICONSET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$ICONSET" "$APP/Contents/Resources"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/icon.png --out "$ICONSET/icon_${size}x${size}.png" > /dev/null
  sips -z "$((size * 2))" "$((size * 2))" assets/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" > /dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$(dirname "$ICONSET")"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Akidev</string>
  <key>CFBundleDisplayName</key><string>Akidev</string>
  <key>CFBundleIdentifier</key><string>live.akidev.Akidev</string>
  <key>CFBundleExecutable</key><string>Akidev</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$(cargo pkgid -p akidev | sed 's/.*[#@]//')</string>
  <key>CFBundleShortVersionString</key><string>$(cargo pkgid -p akidev | sed 's/.*[#@]//')</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# The file a load path names. Newer Homebrew libraries refer to each other as `@rpath/<name>`,
# which only means something to the loader, so those are looked for in Homebrew's lib folders.
resolve_library() {
  case "$1" in
    @rpath/*)
      local name="${1#@rpath/}" prefix found
      prefix=$(brew --prefix)
      found=$(ls "$prefix"/lib/"$name" "$prefix"/opt/*/lib/"$name" 2>/dev/null | head -n 1)
      [ -n "$found" ] || { echo "cannot find $1" >&2; return 1; }
      realpath "$found" ;;
    *) realpath "$1" ;;
  esac
}

# Copies every non-system library `$1` loads into Frameworks, then does the same for each of
# those, and points every reference at the copy.
bundle_dependencies() {
  local file="$1" dependency name
  while read -r dependency; do
    case "$dependency" in /usr/lib/* | /System/*) continue ;; esac
    name=$(basename "$dependency")
    if [ ! -e "$FRAMEWORKS/$name" ]; then
      cp "$(resolve_library "$dependency")" "$FRAMEWORKS/$name"
      chmod u+w "$FRAMEWORKS/$name"
      codesign --remove-signature "$FRAMEWORKS/$name" 2>/dev/null || true
      install_name_tool -id "@executable_path/../Frameworks/$name" "$FRAMEWORKS/$name"
      bundle_dependencies "$FRAMEWORKS/$name"
    fi
    install_name_tool -change "$dependency" "@executable_path/../Frameworks/$name" "$file"
  done < <(otool -L "$file" | tail -n +2 | awk '{print $1}')
}

bundle_dependencies "$APP/Contents/MacOS/Akidev"

# Rewriting load paths invalidates signatures, so everything is signed again, libraries first
# because the app's signature covers them.
if [ -n "${SIGN_IDENTITY:-}" ]; then
  for library in "$FRAMEWORKS"/*; do
    codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$library"
  done
  codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
else
  for library in "$FRAMEWORKS"/*; do codesign --force --sign - "$library" 2>/dev/null; done
  codesign --force --sign - "$APP"
fi

echo "Built $APP ($(du -sh "$APP" | cut -f1))"
