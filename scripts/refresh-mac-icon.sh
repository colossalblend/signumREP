#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

ROOT="/Users/user/Signum-v1"
APP="/Applications/Signum.app"
ICNS="$ROOT/src-tauri/icons/icon.icns"

                         
bash "$ROOT/scripts/regen-mac-icns.sh" "$ROOT"

cp "$ICNS" "$APP/Contents/Resources/icon.icns"
                                             
cp "$ICNS" "$ROOT/src-tauri/target/release/bundle/macos/Signum.app/Contents/Resources/icon.icns" 2>/dev/null || true

xattr -cr "$APP" 2>/dev/null || true
codesign --force --deep --sign - "$APP" 2>/dev/null || true

                                                     
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" 2>/dev/null || true
                         
touch "$APP"
                                
rm -rf "$HOME/Library/Caches/com.apple.iconservices.store" 2>/dev/null || true
find /private/var/folders -name com.apple.dock.iconcache -delete 2>/dev/null || true
killall Dock 2>/dev/null || true
killall Finder 2>/dev/null || true

echo "Verify sizes in installed icns:"
rm -rf /tmp/signum-verify.iconset
iconutil -c iconset "$APP/Contents/Resources/icon.icns" -o /tmp/signum-verify.iconset
ls -la /tmp/signum-verify.iconset
echo "ICON_REFRESH_OK"
