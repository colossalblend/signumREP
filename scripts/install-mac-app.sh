#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

APP_SRC="/Users/user/Signum-v1/src-tauri/target/release/bundle/macos/Signum.app"
APP_DST="/Applications/Signum.app"
DMG="/Users/user/Signum-v1/src-tauri/target/release/bundle/dmg/Signum_1.1.0_x64.dmg"

if [[ ! -d "$APP_SRC" ]]; then
  echo "Signum.app not found at $APP_SRC" >&2
  if [[ -f "$DMG" ]]; then
    echo "Falling back to DMG install..."
    MNT=$(hdiutil attach "$DMG" -nobrowse -readonly | awk '/\/Volumes\// {print $NF; exit}')
    if [[ -z "${MNT:-}" || ! -d "$MNT/Signum.app" ]]; then
      echo "Failed to mount DMG" >&2
      exit 1
    fi
    APP_SRC="$MNT/Signum.app"
  else
    exit 1
  fi
fi

echo "Installing from: $APP_SRC"
rm -rf "$APP_DST"
cp -R "$APP_SRC" "$APP_DST"

                                                            
xattr -cr "$APP_DST" 2>/dev/null || true

                                                 
codesign --force --deep --sign - "$APP_DST" 2>/dev/null || true

                              
if [[ -n "${MNT:-}" ]]; then
  hdiutil detach "$MNT" >/dev/null 2>&1 || true
fi

echo "Installed:"
ls -la "$APP_DST"
file "$APP_DST/Contents/MacOS/signum"
/usr/libexec/PlistBuddy -c 'Print :CFBundleName' "$APP_DST/Contents/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_DST/Contents/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP_DST/Contents/Info.plist" 2>/dev/null || true
echo "INSTALL_OK"
