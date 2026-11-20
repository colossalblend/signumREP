#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

ROOT="/Users/user/Signum-v1"
APP_SRC="$ROOT/src-tauri/target/release/bundle/macos/Signum.app"
APP_DST="/Applications/Signum.app"

if [[ ! -d "$APP_SRC" ]]; then
                                                                     
  echo "Missing built app at $APP_SRC" >&2
  ls -la "$ROOT/src-tauri/target/release/bundle/macos/" 2>&1 || true
  exit 1
fi

                               
if [[ -x "$ROOT/scripts/make-mac-dmg.sh" ]]; then
  bash "$ROOT/scripts/make-mac-dmg.sh" || true
fi

echo "Installing $APP_SRC -> $APP_DST"
rm -rf "$APP_DST"
cp -R "$APP_SRC" "$APP_DST"
xattr -cr "$APP_DST" 2>/dev/null || true
codesign --force --deep --sign - "$APP_DST" 2>/dev/null || true

echo "Binary:"
file "$APP_DST/Contents/MacOS/signum"
strings "$APP_DST/Contents/MacOS/signum" | grep -F "two-factor auth code required" | head -3 || true
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_DST/Contents/Info.plist"
echo "INSTALL_OK"
