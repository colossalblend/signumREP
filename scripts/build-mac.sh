#!/usr/bin/env bash
# Builds the macOS app. Universal by default: an Intel-only build makes macOS
# warn that "support for Intel apps is ending" and runs under Rosetta on every
# Apple Silicon Mac. Set SIGNUM_MAC_TARGET=native to build only for this Mac.
# Подпись ad-hoc ("signingIdentity": "-" в tauri.conf.json) — без сертификата
# разработчика. На Apple Silicon это не роскошь, а условие запуска: неподписанный
# arm64-код там не стартует вовсе, и пользователь видит «Не удается открыть
# программу» без пояснений. Gatekeeper она не отменяет: при первом запуске
# приложение нужно открыть через правую кнопку.
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${SIGNUM_MAC_TARGET:-universal}"

# The bundled helpers are copied into the .app as-is. A binary that arrives
# without its executable bit spawns with EACCES: ipatool then cannot log in and
# sapsigner cannot sign Store requests, which is why the version picker showed
# raw build numbers instead of "v8.0.1".
chmod +x \
  apple-tools/macos-arm64/v2/ipatool \
  apple-tools/macos-arm64/v2/sapsigner \
  apple-tools/macos-arm64/v3-legacy/ipatool \
  apple-tools/macos-x64/v2/ipatool \
  apple-tools/macos-x64/v2/sapsigner \
  apple-tools/macos-x64/v3-legacy/ipatool \
  apple-tools/macos-arm64/v4/ipatool \
  apple-tools/macos-x64/v4/ipatool \
  apple-tools/macos-arm64/v5/ipatool \
  apple-tools/macos-x64/v5/ipatool 2>/dev/null || true
chmod +x apple-tools/macos-*/support/* 2>/dev/null || true

if [[ "${TARGET}" == "universal" && ! -f apple-tools/macos-arm64/support/ideviceinfo ]]; then
  echo "note: apple-tools/macos-arm64/support/ is empty — the Apple Silicon slice will"
  echo "      run the Intel device tools through Rosetta 2. Run this on an M-series Mac"
  echo "      to make it fully native:  bash scripts/dev-prepare-mac-arm64-tools.sh"
fi

echo "==> npm ci"
npm ci

if [[ "${TARGET}" == "universal" ]]; then
  echo "==> rustup targets"
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  echo "==> build macOS DMG (universal)"
  npm run build:mac-universal
else
  echo "==> build macOS DMG (native $(uname -m))"
  npm run build:mac
fi

echo "==> artifacts:"
for base in src-tauri/target/universal-apple-darwin/release/bundle src-tauri/target/release/bundle; do
  ls -la "${base}/dmg/" 2>/dev/null || true
  ls -la "${base}/macos/" 2>/dev/null || true
done
