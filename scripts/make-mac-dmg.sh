#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

APP="/Users/user/Signum-v1/src-tauri/target/release/bundle/macos/Signum.app"
OUTDIR="/Users/user/Signum-v1/src-tauri/target/release/bundle/dmg"
STAGE="/tmp/signum-dmg-stage"
DMG_RW="/tmp/Signum_1.1.1_x64-rw.dmg"
DMG_FINAL="${OUTDIR}/Signum_1.1.1_x64.dmg"

if [[ ! -d "$APP" ]]; then
  echo "missing app: $APP" >&2
  exit 1
fi

rm -rf "$STAGE"
rm -f "$DMG_RW" "$DMG_FINAL"
rm -f /Users/user/Signum-v1/src-tauri/target/release/bundle/macos/rw.*.dmg
mkdir -p "$STAGE" "$OUTDIR"

cp -R "$APP" "$STAGE/"
ln -sf /Applications "$STAGE/Applications"

                                                                            
hdiutil create \
  -volname "Signum" \
  -srcfolder "$STAGE" \
  -ov \
  -format UDZO \
  -imagekey zlib-level=9 \
  "$DMG_FINAL"

rm -rf "$STAGE"

echo "DMG ready:"
ls -lah "$DMG_FINAL"
file "$DMG_FINAL"

                         
cd /Users/user/Signum-v1/src-tauri/target/release/bundle/macos
rm -f Signum_1.1.0_x64.app.zip
ditto -c -k --sequesterRsrc --keepParent Signum.app Signum_1.1.0_x64.app.zip
ls -lah Signum.app Signum_1.1.0_x64.app.zip
