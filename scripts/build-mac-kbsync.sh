#!/bin/bash
# Сборка kbsync под macOS.
#
# Запускать НА маке:  bash build-mac-kbsync.sh
# Результат: ~/signum-kbsync-macos/ — бинарь и библиотека рядом с ним.
#
# Зачем отдельный скрипт: собирается не с первой попытки, и все три грабли
# стоит записать один раз, а не вспоминать каждый раз заново.
set -euo pipefail
export PATH="$PATH:/usr/local/bin:/opt/homebrew/bin:/usr/local/go/bin"

SRC="${1:-$HOME/emu}"                 # дерево sapsigner/impl/emu
OUT="$HOME/signum-kbsync-macos"

command -v go   >/dev/null || { echo "нет go: brew install go"; exit 1; }
command -v brew >/dev/null || { echo "нет brew"; exit 1; }

echo "== unicorn"
brew list unicorn >/dev/null 2>&1 || brew install unicorn
UC_LIB=$(brew --prefix unicorn)/lib
UC_DYLIB=$(ls "$UC_LIB"/libunicorn.*.dylib | head -1)
echo "  $UC_DYLIB"

# Грабля 1. Официальные Go-привязки unicorn передают size_t там, где заголовки
# 2.1.4 объявляют uint64_t. На 64 битах это одно число, но cgo считает типы
# разными и отказывается собирать. Патчим локальную копию, а не кеш модулей.
echo "== привязки unicorn"
MOD=$(go env GOMODCACHE)/github.com/unicorn-engine/unicorn@v0.0.0-20241221030228-28990888443e
LOCAL="$HOME/unicorn-go-patched"
if [ ! -d "$LOCAL" ]; then
  [ -d "$MOD" ] || ( cd "$SRC" && go mod download github.com/unicorn-engine/unicorn )
  mkdir -p "$LOCAL" && cp -R "$MOD"/. "$LOCAL"/ && chmod -R u+w "$LOCAL"
fi
python3 - "$LOCAL" <<'PY'
import io, sys, os
p = os.path.join(sys.argv[1], "bindings/go/unicorn/unicorn.go")
lines = io.open(p, encoding="utf-8").read().split("\n")
n = 0
for i, line in enumerate(lines):
    if "C.uc_mem_" in line and "C.size_t(" in line:
        lines[i] = line.replace("C.size_t(", "C.uint64_t(")
        n += 1
io.open(p, "w", encoding="utf-8", newline="").write("\n".join(lines))
print("  исправлено строк:", n)
PY

echo "== сборка"
cd "$SRC"
go mod edit -replace "github.com/unicorn-engine/unicorn=$LOCAL"
# Грабля 2. Go по умолчанию не пускает @loader_path в cgo LDFLAGS, а он нужен
# именно затем, чтобы библиотека ехала рядом с бинарём.
export CGO_LDFLAGS_ALLOW='.*'
CGO_CFLAGS="-I$(brew --prefix unicorn)/include" \
CGO_LDFLAGS="-L$UC_LIB -lunicorn" \
  go build -trimpath -o "$HOME/kbsync.tmp" ./cmd/kbsync

echo "== раскладка"
rm -rf "$OUT" && mkdir -p "$OUT"
mv "$HOME/kbsync.tmp" "$OUT/kbsync"
cp "$UC_DYLIB" "$OUT/"
DY=$(basename "$UC_DYLIB")
chmod u+w "$OUT/$DY"

# Грабля 3. Собранный бинарь ссылается на библиотеку по пути Homebrew, которого
# на чужом маке нет. Переписываем на @loader_path и переподписываем — иначе
# macOS отвергнет изменённый бинарь.
install_name_tool -change "$UC_DYLIB" "@loader_path/$DY" "$OUT/kbsync"
install_name_tool -id "@loader_path/$DY" "$OUT/$DY"
codesign --force --sign - "$OUT/$DY"
codesign --force --sign - "$OUT/kbsync"

echo
echo "готово: $OUT"
otool -L "$OUT/kbsync" | sed -n '2,3p'
echo "  Рядом нужен assets.bin — тот же, что на Windows."
