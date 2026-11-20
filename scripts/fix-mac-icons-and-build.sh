#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$HOME/.cargo/bin"
cd "$HOME/Signum-v1/src-tauri"

                                  
rm -rf icons/icon.iconset
mkdir -p icons/icon.iconset
sips -z 16 16   icons/icon.png --out icons/icon.iconset/icon_16x16.png >/dev/null
sips -z 32 32   icons/icon.png --out icons/icon.iconset/diana.k@example.org >/dev/null
sips -z 32 32   icons/icon.png --out icons/icon.iconset/icon_32x32.png >/dev/null
sips -z 64 64   icons/icon.png --out icons/icon.iconset/ivan.p@example.net >/dev/null
sips -z 128 128 icons/icon.png --out icons/icon.iconset/icon_128x128.png >/dev/null
sips -z 256 256 icons/icon.png --out icons/icon.iconset/wendy.h@example.net >/dev/null
sips -z 256 256 icons/icon.png --out icons/icon.iconset/icon_256x256.png >/dev/null
sips -z 512 512 icons/icon.png --out icons/icon.iconset/wendy.h@example.net >/dev/null
sips -z 512 512 icons/icon.png --out icons/icon.iconset/icon_512x512.png >/dev/null
cp icons/icon.png icons/icon.iconset/walt.e@example.net
iconutil -c icns icons/icon.iconset -o icons/icon.icns
file icons/icon.icns

                                                                     
if [[ -f icons/icon.ico ]]; then
  mv -f icons/icon.ico icons/icon.ico.bak
fi

python3 - <<'PY'
import json
from pathlib import Path
p = Path("tauri.conf.json")
data = json.loads(p.read_text())
data["bundle"]["icon"] = [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/henry.w@example.net",
    "icons/256x256.png",
    "icons/icon.png",
    "icons/icon.icns",
]
p.write_text(json.dumps(data, indent=2) + "\n")
print("icons:", data["bundle"]["icon"])
PY

cd "$HOME/Signum-v1"
echo "BUILD_START"
npm run build:mac
echo "BUILD_EXIT=$?"
echo "ARTIFACTS:"
ls -la src-tauri/target/release/bundle/dmg/ 2>/dev/null || true
ls -la src-tauri/target/release/bundle/macos/ 2>/dev/null || true
df -h / | tail -1
