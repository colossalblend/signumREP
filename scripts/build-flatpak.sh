#!/usr/bin/env bash
# Собрать Signum как Flatpak. Запускать на Linux, не в WSL.
set -euo pipefail
cd "$(dirname "$0")/.."

APP_ID=pw.appbank.signum
STAGE=flatpak/stage

echo "==> зависимости flatpak"
if command -v apt-get >/dev/null 2>&1; then
  sudo apt-get install -y flatpak flatpak-builder || true
fi
flatpak remote-add --if-not-exists --user flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y flathub org.gnome.Platform//47 org.gnome.Sdk//47

echo "==> сборка бинаря"
npm ci
npm run build
cargo build --release --manifest-path src-tauri/Cargo.toml

echo "==> раскладка файлов для flatpak-builder"
rm -rf "$STAGE"
mkdir -p "$STAGE/resources/linux-x64"
cp src-tauri/target/release/signum "$STAGE/signum"
cp -r linux_x64/. "$STAGE/resources/linux-x64/"
cp src-tauri/icons/256x256.png "$STAGE/icon.png"
cp "flatpak/$APP_ID.desktop" "$STAGE/$APP_ID.desktop"
cp "flatpak/$APP_ID.metainfo.xml" "$STAGE/$APP_ID.metainfo.xml"

echo "==> flatpak-builder"
flatpak-builder --force-clean --user --install \
  flatpak/build-dir "flatpak/$APP_ID.yml"

echo
echo "Готово. Запуск:  flatpak run $APP_ID"
echo "Собрать файл для раздачи:"
echo "  flatpak-builder --repo=flatpak/repo --force-clean flatpak/build-dir flatpak/$APP_ID.yml"
echo "  flatpak build-bundle flatpak/repo Signum.flatpak $APP_ID"
