#!/bin/bash
# Установка Signum на macOS одной командой:
#   curl -fsSL https://appbank.pw/signum/install.sh | bash
#
# Скачивает последнюю версию, сверяет SHA-256 с сайтом, кладёт Signum в
# «Программы» и запускает. Файл, скачанный curl, не получает отметку карантина,
# поэтому macOS не показывает окно «Apple не может проверить Signum».
set -euo pipefail

SITE="https://appbank.pw/signum"
APP_NAME="Signum.app"

say()  { printf '\033[1m==> %s\033[0m\n' "$*"; }
fail() { printf '\033[31mОшибка: %s\033[0m\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || fail "этот установщик только для macOS."

TMP="$(mktemp -d -t signum-install)"
MOUNT="$TMP/mount"
cleanup() {
  hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT

say "Узнаём последнюю версию Signum…"
JSON="$(curl -fsSL --retry 2 "$SITE/version.php")" || fail "не удалось связаться с appbank.pw. Проверьте интернет."
INFO="$(osascript -l JavaScript -e '
function run(argv) {
  var m = JSON.parse(argv[0]).platforms.macos || {};
  return [m.download_url || "", m.sha256 || "", m.version || ""].join("\n");
}' "$JSON" 2>/dev/null)" || fail "сайт вернул непонятный ответ. Попробуйте позже."
URL="$(printf '%s\n' "$INFO" | sed -n 1p)"
SHA="$(printf '%s\n' "$INFO" | sed -n 2p)"
VERSION="$(printf '%s\n' "$INFO" | sed -n 3p)"
case "$URL" in
  https://appbank.pw/signum/*|https://api.appbank.pw/signum/*|https://apiz.appbank.pw/signum/*) ;;
  *) fail "сайт не дал ссылку на загрузку для macOS." ;;
esac
printf '%s' "$SHA" | grep -Eq '^[a-f0-9]{64}$' || fail "сайт не дал контрольную сумму файла."

say "Скачиваем Signum ${VERSION}…"
DMG="$TMP/Signum.dmg"
MIRROR="${URL/https:\/\/appbank.pw\//https://apiz.appbank.pw/}"
curl -fL --retry 2 --progress-bar -o "$DMG" "$URL" \
  || curl -fL --retry 2 --progress-bar -o "$DMG" "$MIRROR" \
  || fail "не удалось скачать Signum. Проверьте интернет и повторите."

say "Проверяем подлинность файла…"
ACTUAL="$(shasum -a 256 "$DMG" | awk '{print $1}')"
[ "$ACTUAL" = "$SHA" ] || fail "контрольная сумма не совпала — файл повреждён. Повторите установку."

say "Устанавливаем в «Программы»…"
mkdir -p "$MOUNT"
hdiutil attach "$DMG" -nobrowse -readonly -noautoopen -mountpoint "$MOUNT" -quiet \
  || fail "не удалось открыть образ .dmg."
[ -d "$MOUNT/$APP_NAME" ] || fail "в образе нет $APP_NAME."

osascript -e 'tell application id "pw.appbank.signum" to quit' >/dev/null 2>&1 || true
pkill -x signum >/dev/null 2>&1 || true
sleep 1

DEST="/Applications"
if [ -e "$DEST/$APP_NAME" ] && ! rm -rf "$DEST/$APP_NAME" 2>/dev/null; then
  say "Нужен пароль администратора, чтобы заменить старую версию:"
  sudo rm -rf "$DEST/$APP_NAME"
fi
if ! ditto "$MOUNT/$APP_NAME" "$DEST/$APP_NAME" 2>/dev/null; then
  say "Нужен пароль администратора для папки «Программы»:"
  sudo ditto "$MOUNT/$APP_NAME" "$DEST/$APP_NAME"
  sudo chown -R "$(id -un)" "$DEST/$APP_NAME" 2>/dev/null || true
fi
xattr -dr com.apple.quarantine "$DEST/$APP_NAME" 2>/dev/null \
  || sudo xattr -dr com.apple.quarantine "$DEST/$APP_NAME" 2>/dev/null || true

say "Готово. Запускаем Signum…"
open "$DEST/$APP_NAME"
