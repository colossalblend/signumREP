#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$HOME/.cargo/bin"
cd /Users/user/Signum-v1

                     
if [[ -f src-tauri/icons/icon.ico ]]; then
  mv -f src-tauri/icons/icon.ico src-tauri/icons/icon.ico.bak || true
fi

echo "BUILD"
                                                       
npx tauri build --bundles app 2>&1 || {
  echo "app bundle failed, trying full build:mac"
  npm run build:mac 2>&1 || true
}

APP="/Users/user/Signum-v1/src-tauri/target/release/bundle/macos/Signum.app"
if [[ ! -d "$APP" ]]; then
  echo "missing app" >&2
  exit 1
fi

pkill -x signum 2>/dev/null || true
sleep 1
rm -rf /Applications/Signum.app
cp -R "$APP" /Applications/Signum.app
xattr -cr /Applications/Signum.app 2>/dev/null || true
codesign --force --deep --sign - /Applications/Signum.app 2>/dev/null || true

                           
bash scripts/make-mac-dmg.sh 2>/dev/null || true

echo "INSTALLED"
file /Applications/Signum.app/Contents/MacOS/signum
ls -la /Applications/Signum.app/Contents/MacOS/signum
echo OK
