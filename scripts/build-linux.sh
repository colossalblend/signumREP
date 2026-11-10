#!/usr/bin/env bash
                                                                        
                                                           
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> system packages (Debian/Ubuntu)"
if command -v apt-get >/dev/null 2>&1; then
  sudo apt-get update
  sudo apt-get install -y \
    build-essential curl wget file \
    libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
    patchelf libssl-dev libgtk-3-dev \
    usbmuxd libimobiledevice-utils ideviceinstaller || true
fi

# Движки v4 и v5 раньше сюда не попадали, и предпочитаемый v5 запускался
# с отказом в доступе — а выглядело это как «движок не найден».
chmod +x linux_x64/ipatool linux_x64/ipatool-v4 linux_x64/ipatool-v5 linux_x64/zsign 2>/dev/null || true
chmod +x linux_arm64/ipatool linux_arm64/ipatool-v4 linux_arm64/ipatool-v5 linux_arm64/zsign 2>/dev/null || true

if [[ ! -f linux_x64/ipatool ]] && [[ ! -f linux_arm64/ipatool ]]; then
  echo "WARN: no bundled ipatool under linux_x64/ or linux_arm64/."
  echo "      Put a Linux ipatool binary there, or ensure 'ipatool' is on PATH after install."
fi

echo "==> npm ci"
npm ci

echo "==> build Linux deb + AppImage"
# linuxdeploy сам поставляется как AppImage, а он без FUSE не запускается —
# ни в WSL, ни в контейнерах. С этой переменной он распаковывает себя во
# временную папку вместо монтирования.
export APPIMAGE_EXTRACT_AND_RUN=1
# strip из linuxdeploy не понимает свежие секции ELF (.relr.dyn) и валит сборку.
export NO_STRIP=1
# tauri падает на шаге AppImage (см. ниже про архитектуры), но deb к этому
# моменту уже собран, а AppDir подготовлен. Поэтому ошибку здесь не считаем
# концом: ниже AppImage доделывается вручную, а наличие обоих пакетов
# проверяется в конце.
npm run build:linux || echo "WARN: tauri не собрал AppImage — доделываем вручную"

# appimagetool угадывает архитектуру по файлам внутри пакета, а у нас там лежат
# инструменты и под arm64, и под macOS — он видит несколько архитектур и
# отказывается собирать («More than one architectures were found»). Через tauri
# переменную ему не передать, поэтому доделываем последний шаг сами: это тот же
# linuxdeploy на той же папке, что подготовил tauri.
BUNDLE=src-tauri/target/release/bundle/appimage
VERSION=$(grep -m1 '"version"' src-tauri/tauri.conf.json | cut -d'"' -f4)
if [[ -d "$BUNDLE/Signum.AppDir" ]] && ! ls "$BUNDLE"/*.AppImage >/dev/null 2>&1; then
  echo "==> AppImage вручную: tauri не передаёт ARCH"
  (
    cd "$BUNDLE"
    if [[ ! -f linuxdeploy-x86_64.AppImage ]]; then
      wget -q https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
      chmod +x linuxdeploy-x86_64.AppImage
    fi
    # --output appimage ищет плагин рядом или в PATH; tauri держит свой в кеше.
    if [[ ! -f linuxdeploy-plugin-appimage-x86_64.AppImage ]]; then
      wget -q https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage
      chmod +x linuxdeploy-plugin-appimage-x86_64.AppImage
    fi
    # linuxdeploy разбирает зависимости всех ELF внутри AppDir, в том числе
    # наших инструментов; их библиотеки (libminizip для zsign и т.п.) лежат
    # рядом в resources/linux-x64 — показываем ему эту папку.
    TOOLS_LIB="$PWD/Signum.AppDir/usr/lib/Signum/resources/linux-x64"
    LD_LIBRARY_PATH="$TOOLS_LIB${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    PATH="$PWD:$PATH" ARCH=x86_64 ./linuxdeploy-x86_64.AppImage --appdir Signum.AppDir --output appimage
    mv -f Signum-x86_64.AppImage "Signum_${VERSION}_amd64.AppImage"
    sha256sum "Signum_${VERSION}_amd64.AppImage"
  )
fi

shopt -s nullglob
APPIMAGES=("$BUNDLE"/*.AppImage)
if [[ ${#APPIMAGES[@]} -eq 0 ]]; then
  echo "ERROR: AppImage не собран" >&2
  exit 1
fi

echo "==> artifacts:"
ls -la src-tauri/target/release/bundle/appimage/ 2>/dev/null || true
ls -la src-tauri/target/release/bundle/deb/ 2>/dev/null || true
DEBS=(src-tauri/target/release/bundle/deb/*.deb)
if [[ ${#DEBS[@]} -eq 0 ]]; then
  echo "ERROR: deb не собран" >&2
  exit 1
fi
