#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/Users/user/Library/Python/3.9/bin:$PATH"
ROOT="/Users/user/Signum-v1"
APP="/Applications/Signum.app"

                        
cp -f /tmp/pad-icon.py /tmp/pad-icon.py 2>/dev/null || true

bash "$ROOT/scripts/regen-mac-icns.sh" "$ROOT"
python3 /tmp/build-icns.py "$ROOT"
python3 /tmp/list-icns.py "$ROOT/src-tauri/icons/icon.icns"

cp "$ROOT/src-tauri/icons/icon.icns" "$APP/Contents/Resources/icon.icns"
                             
if [[ -d "$ROOT/src-tauri/target/release/bundle/macos/Signum.app" ]]; then
  cp "$ROOT/src-tauri/icons/icon.icns" \
    "$ROOT/src-tauri/target/release/bundle/macos/Signum.app/Contents/Resources/icon.icns"
fi
xattr -cr "$APP" 2>/dev/null || true
codesign --force --deep --sign - "$APP" 2>/dev/null || true
touch "$APP"
                   
rm -rf "$HOME/Library/Caches/com.apple.iconservices.store" 2>/dev/null || true
find /private/var/folders -name 'com.apple.dock.iconcache' -delete 2>/dev/null || true
find /private/var/folders -name 'com.apple.iconservices*' -type d -exec rm -rf {} + 2>/dev/null || true
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" 2>/dev/null || true
killall Dock 2>/dev/null || true
killall Finder 2>/dev/null || true
echo DONE
