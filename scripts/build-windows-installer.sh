#!/bin/bash
# Установщик Signum одним файлом.
#
# Устройство простое: обычный zip с приложением дописывается в хвост
# exe-заглушки. Заглушка ищет архив в себе самой (zip читается с конца, поэтому
# приклеенный к бинарю он остаётся валидным). Отдельный файл рядом нужен только
# для отладки голой заглушки.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
VERSION=$(grep -m1 '^version' src-setup/Cargo.toml | cut -d'"' -f2)
OUT="$ROOT/installers/Signum_${VERSION}_Setup.exe"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

echo "== приложение"
[ -f src-tauri/target/release/signum.exe ] || { echo "нет signum.exe — сначала npm run tauri build -- --no-bundle"; exit 1; }
mkdir -p "$STAGE/payload"
cp src-tauri/target/release/signum.exe "$STAGE/payload/"

echo "== деинсталлятор"
( cd src-setup && cargo build --release --quiet )
cp src-setup/target/release/uninstall.exe "$STAGE/payload/"

echo "== инструменты Apple"
#
# Только windows-x64: ветки macos-* и linux-* в установщике под Windows не
# нужны никому, а весили они около двухсот мегабайт — люди качали их впустую.
mkdir -p "$STAGE/payload/resources/apple-tools"
cp -r apple-tools/windows-x64 "$STAGE/payload/resources/apple-tools/windows-x64"
[ -f apple-tools/SOURCE.json ] && cp apple-tools/SOURCE.json "$STAGE/payload/resources/apple-tools/"

# Проверка зависимостей вместо списка имён.
#
# Раньше здесь был список из пяти файлов, и он не заметил бы ни пропажу
# библиотеки в support/, ни libunicorn.dll рядом с v4. Теперь разбираем таблицы
# импорта и требуем, чтобы каждая несистемная зависимость лежала рядом со своим
# бинарём — именно там, где Windows её ищет.
echo "== проверка зависимостей"
#
# Не список имён, а разбор таблиц импорта: однажды в пакет не попала
# libunicorn.dll, и у всех, кто ставил с нуля, молча переставали работать
# снятые с продажи приложения. Список файлов такое не поймал бы.
python - "$STAGE/payload/resources/apple-tools/windows-x64" "$ROOT/scripts/inspect-binary-deps.py" <<'PY' || exit 1
import json, os, subprocess, sys

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

SYSTEM = {
    "kernel32.dll", "user32.dll", "advapi32.dll", "ws2_32.dll", "msvcrt.dll",
    "ucrtbase.dll", "bcrypt.dll", "crypt32.dll", "shell32.dll", "ole32.dll",
    "oleaut32.dll", "gdi32.dll", "iphlpapi.dll", "secur32.dll", "dbghelp.dll",
    "winmm.dll", "setupapi.dll", "version.dll", "userenv.dll", "psapi.dll",
    "netapi32.dll", "dnsapi.dll", "wldap32.dll", "normaliz.dll", "cfgmgr32.dll",
    "rpcrt4.dll", "comdlg32.dll", "shlwapi.dll", "imm32.dll", "winspool.drv",
    "mpr.dll", "wtsapi32.dll", "powrprof.dll", "propsys.dll", "uxtheme.dll",
    "dwmapi.dll", "opengl32.dll", "msimg32.dll", "comctl32.dll", "ntdll.dll",
    "winhttp.dll", "wininet.dll", "usp10.dll", "mswsock.dll", "msvcp140.dll",
    "vcruntime140.dll", "vcruntime140_1.dll", "gdiplus.dll", "ncrypt.dll",
    "wsock32.dll",
}

root, parser = sys.argv[1], sys.argv[2]
if not os.path.isfile(parser):
    sys.exit("  нет разборщика зависимостей: %s" % parser)

problems, checked = [], 0
for folder, _, files in os.walk(root):
    binaries = [f for f in files if f.lower().endswith((".exe", ".dll"))]
    if not binaries:
        continue
    here = {f.lower() for f in files}
    paths = [os.path.join(folder, f) for f in binaries]
    done = subprocess.run([sys.executable, parser] + paths,
                          capture_output=True, text=True)
    if done.returncode != 0 or not done.stdout.strip():
        sys.exit("  разборщик не отработал на %s: %s"
                 % (folder, (done.stderr or "пустой вывод").strip()[:200]))
    for path, info in json.loads(done.stdout).items():
        imports = info.get("imports")
        if imports is None:
            sys.exit("  не разобрался бинарь: %s" % path)
        checked += 1
        for _kind, name in imports:
            low = name.lower()
            if low in here or low in SYSTEM or low.startswith("api-ms-win"):
                continue
            problems.append("%s/%s -> %s"
                            % (os.path.basename(folder), os.path.basename(path), name))

# Ноль проверенных файлов — это не «всё хорошо», а сломанная проверка.
if checked < 20:
    sys.exit("  проверено лишь %d бинарей — проверка сломана" % checked)

if problems:
    print("  рядом с бинарями не хватает библиотек:")
    for line in sorted(set(problems)):
        print("    " + line)
    sys.exit(1)
print("  все зависимости на месте (%d бинарей)" % checked)
PY

echo "== интерфейс установщика"
# setup-dist собирается отдельно от основного приложения, и раньше сборщик его
# не трогал — брал, что лежало. Из-за этого в окно установки уехали старый
# логотип и старый сиреневый флажок, хотя в исходниках всё было заменено.
npx vite build --config vite.setup.config.ts >/dev/null 2>&1 || {
  echo "  не собрался интерфейс установщика"; exit 1; }
echo "  собран"

echo "== заглушка"
# Пересобираем всегда: в её ресурсы вшита иконка, и старая заглушка на диске
# ничего об этом не знает.
#
# Кеш ресурсов сносим принудительно: tauri-build не считает замену .ico поводом
# пересобраться, и заглушка молча уезжает со старой иконкой. Именно так один
# раз и вышло — в панели задач был пустой лист.
rm -rf src-setup/target/release/build/signum-setup-* 2>/dev/null || true
touch src-setup/build.rs
( cd src-setup && cargo build --release --quiet ) || {
  echo "  заглушка не собралась"; exit 1; }
STUB=src-setup/target/release/signum-setup.exe
[ -f "$STUB" ] || { echo "заглушка не собралась: $STUB"; exit 1; }

echo "== проверка иконки"
#
# tauri-build кеширует ресурс с иконкой и не пересобирает его при замене
# .ico — сборка при этом рапортует успех. Один раз так и вышло: установщик
# уехал со старой иконкой внутри. Сверяем побайтово.
python - "$ROOT/src-tauri/icons/icon.ico" "$STAGE/payload/signum.exe" "$STUB" <<'PY' || exit 1
import os, struct, sys

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

ico_path, exe_paths = sys.argv[1], sys.argv[2:]
ico = open(ico_path, "rb").read()

# Каталог .ico: по 16 байт на запись, в конце смещение и размер картинки.
count = struct.unpack_from("<H", ico, 4)[0]
images = []
for i in range(count):
    entry = 6 + i * 16
    width = ico[entry] or 256
    size, offset = struct.unpack_from("<II", ico, entry + 8)
    image = ico[offset:offset + size]
    if len(image) >= 200:
        images.append((width, image))

bad = False
for exe_path in exe_paths:
    name = os.path.basename(exe_path)
    try:
        exe = open(exe_path, "rb").read()
    except OSError as error:
        print("  %s — не открылся: %s" % (name, error))
        bad = True
        continue
    missing = [w for w, image in images if image not in exe]
    if missing:
        print("  %s — нет иконки размеров: %s"
              % (name, ", ".join(str(w) for w in missing)))
        bad = True
    else:
        print("  %s — иконка совпадает (%d размеров)" % (name, len(images)))

if bad:
    print()
    print("  Причина обычно одна: tauri-build взял ресурс с иконкой из кеша.")
    print("  Лечится так (для нужного проекта):")
    print("    rm -rf <проект>/target/release/build/<crate>-*")
    print("    touch <проект>/build.rs")
    sys.exit(1)
PY


echo "== пакет"
export STAGE
( cd "$STAGE/payload" && python -c "
import zipfile,os,sys
z=zipfile.ZipFile(os.environ['STAGE']+'/SignumPayload.zip','w',zipfile.ZIP_DEFLATED,compresslevel=6)
n=0
for root,_,files in os.walk('.'):
    for f in files:
        p=os.path.join(root,f)
        z.write(p, os.path.relpath(p,'.').replace(os.sep,'/'))
        n+=1
z.close(); print(f'  файлов: {n}')
" )

echo "== склейка"
mkdir -p "$ROOT/installers"
cat "$STUB" "$STAGE/SignumPayload.zip" > "$OUT"
SIZE=$(stat -c %s "$OUT")
echo
echo "готово: $OUT"
# bc есть не везде (на этой машине его нет), и без него строка печатала
# «0.0 МБ» — мелочь, но в отчёте о сборке это прямая ложь.
awk -v b="$SIZE" 'BEGIN { printf "  размер: %.1f МБ\n", b/1048576 }'
echo "  sha256: $(sha256sum "$OUT" | cut -d' ' -f1)"
