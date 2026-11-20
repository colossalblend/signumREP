#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$HOME/.cargo/bin"

ROOT="/Users/user/Signum-v1"
cd "$ROOT"

                               
if [[ ! -f src-tauri/icons/icon.icns ]]; then
  bash scripts/fix-mac-icons-and-build.sh || true
fi
                                                      
if [[ -f src-tauri/icons/icon.ico ]]; then
  mv -f src-tauri/icons/icon.ico src-tauri/icons/icon.ico.bak
fi

echo "BUILD_START"
npm run build:mac
echo "BUILD_DONE"

                                                   
APP="$ROOT/src-tauri/target/release/bundle/macos/Signum.app"
if [[ ! -d "$APP" ]]; then
  echo "Signum.app missing after build" >&2
  exit 1
fi

                                               
DMG="$ROOT/src-tauri/target/release/bundle/dmg/Signum_1.1.1_x64.dmg"
if [[ ! -f "$DMG" ]]; then
  bash scripts/make-mac-dmg.sh || true
fi

                            
rm -rf /Applications/Signum.app
cp -R "$APP" /Applications/Signum.app
xattr -cr /Applications/Signum.app 2>/dev/null || true
codesign --force --deep --sign - /Applications/Signum.app 2>/dev/null || true

echo "INSTALLED:"
ls -la /Applications/Signum.app
file /Applications/Signum.app/Contents/MacOS/signum
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' /Applications/Signum.app/Contents/Info.plist
echo "OK"
