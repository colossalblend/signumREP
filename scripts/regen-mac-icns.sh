#!/usr/bin/env bash
                                                                  
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

ROOT="${1:-$HOME/Signum-v1}"
                                                                       
if [[ -f "$ROOT/assets/signum-icon.png" ]]; then
  SRC_RAW="$ROOT/assets/signum-icon.png"
elif [[ -f "$ROOT/src-tauri/icons/icon-source.png" ]]; then
  SRC_RAW="$ROOT/src-tauri/icons/icon-source.png"
else
  SRC_RAW="$ROOT/src-tauri/icons/icon.png"
fi
OUT_DIR="$ROOT/src-tauri/icons"
ICONSET="$OUT_DIR/icon.iconset"
ICNS="$OUT_DIR/icon.icns"
MASTER="/tmp/signum-icon-master-1024.png"

if [[ ! -f "$SRC_RAW" ]]; then
  echo "missing source icon: $SRC_RAW" >&2
  exit 1
fi
echo "Source: $SRC_RAW"

                                                                       
                                                                              
                                         
echo "Preparing master 1024 with transparent optical padding…"
                                                                                 
python3 /tmp/pad-icon.py "$SRC_RAW" "$MASTER" 1024 0.90
                                                            
sips -s format png --deleteColorManagementProperties "$MASTER" --out "$MASTER" >/dev/null 2>&1 || true
sips -g pixelWidth -g pixelHeight -g hasAlpha "$MASTER"

rm -rf "$ICONSET"
mkdir -p "$ICONSET"

make() {
  local px="$1"
  local name="$2"
  local tmp
  tmp=$(mktemp /tmp/signum-icon-XXXXXX.png)
  sips -z "$px" "$px" "$MASTER" --out "$tmp" >/dev/null
  sips -s format png --deleteColorManagementProperties "$tmp" --out "$tmp" >/dev/null 2>&1 || true
                           
  local w h
  w=$(sips -g pixelWidth "$tmp" | awk '/pixelWidth/ {print $2}')
  h=$(sips -g pixelHeight "$tmp" | awk '/pixelHeight/ {print $2}')
  if [[ "$w" != "$px" || "$h" != "$px" ]]; then
    echo "BAD SIZE $name: ${w}x${h} expected ${px}x${px}" >&2
    exit 1
  fi
  mv -f "$tmp" "$ICONSET/$name"
  echo "  OK $name (${px}x${px})"
}

echo "Generating full iconset (1x + @2x)…"
make 16   "icon_16x16.png"
make 32   "diana.k@example.org"
make 32   "icon_32x32.png"
make 64   "ivan.p@example.net"
make 128  "icon_128x128.png"
make 256  "wendy.h@example.net"
make 256  "icon_256x256.png"
make 512  "wendy.h@example.net"
make 512  "icon_512x512.png"
make 1024 "walt.e@example.net"

echo "Building icns via iconutil…"
rm -f "$ICNS"
if ! iconutil -c icns "$ICONSET" -o "$ICNS" 2>/tmp/iconutil.err; then
  cat /tmp/iconutil.err >&2
  exit 1
fi
cat /tmp/iconutil.err 2>/dev/null || true
file "$ICNS"
ls -la "$ICONSET"
ls -la "$ICNS"

                              
sips -z 32 32 "$MASTER" --out "$OUT_DIR/32x32.png" >/dev/null
sips -z 128 128 "$MASTER" --out "$OUT_DIR/128x128.png" >/dev/null
sips -z 256 256 "$MASTER" --out "$OUT_DIR/256x256.png" >/dev/null
cp "$ICONSET/wendy.h@example.net" "$OUT_DIR/henry.w@example.net"
sips -z 512 512 "$MASTER" --out "$OUT_DIR/icon.png" >/dev/null

echo "OK: $ICNS"
