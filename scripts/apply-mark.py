# -*- coding: utf-8 -*-
"""Поставить выбранный знак иконкой приложения.

Старые иконки не удаляются, а складываются в brand/icons-backup/ — вернуть
можно этим же скриптом с ключом --restore.

Запуск:
    python scripts/apply-mark.py 03-zarubka
    python scripts/apply-mark.py --restore
"""
import os
import shutil
import sys

from PIL import Image

# Windows-консоль в cp1251 падает на знаке умножения и части кириллицы.
try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MARKS = os.path.join(ROOT, "brand", "marks")
BACKUP = os.path.join(ROOT, "brand", "icons-backup")
TAURI = os.path.join(ROOT, "src-tauri", "icons")
ASSET = os.path.join(ROOT, "assets", "signum-icon.png")

# Что просит tauri.conf.json, плюс исходник для интерфейса.
TARGETS = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "256x256.png": 256,
    "icon.png": 512,
}

# Размеры внутри .ico. Windows берёт нужный сам: 16 — трей и заголовок окна,
# 256 — крупная плитка в проводнике. Пропустить мелкие значит получить
# размытую иконку именно там, где она видна чаще всего.
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]

# .icns у Pillow принимает только квадраты из этого ряда.
ICNS_SIZES = [16, 32, 64, 128, 256, 512]


def backup_once():
    """Копируем оригиналы один раз: повторный запуск не должен затереть их."""
    if os.path.isdir(BACKUP) and os.listdir(BACKUP):
        print("резервная копия уже есть:", BACKUP)
        return
    os.makedirs(BACKUP, exist_ok=True)
    for name in os.listdir(TAURI):
        shutil.copy2(os.path.join(TAURI, name), os.path.join(BACKUP, name))
    if os.path.isfile(ASSET):
        shutil.copy2(ASSET, os.path.join(BACKUP, "signum-icon.png"))
    print("оригиналы сохранены в", BACKUP, "—", len(os.listdir(BACKUP)), "файлов")


def restore():
    if not os.path.isdir(BACKUP) or not os.listdir(BACKUP):
        sys.exit("нечего восстанавливать: " + BACKUP + " пуст")
    for name in os.listdir(BACKUP):
        source = os.path.join(BACKUP, name)
        target = ASSET if name == "signum-icon.png" else os.path.join(TAURI, name)
        shutil.copy2(source, target)
        print("вернул", name)
    print("старая иконка на месте")


def apply(slug):
    # Готовый файл (лучше 1024 — из него же собирается .icns) или знак из brand/marks.
    master_path = slug if os.path.isfile(slug) else os.path.join(MARKS, slug + "-512.png")
    if not os.path.isfile(master_path):
        sys.exit("нет знака: " + master_path)
    master = Image.open(master_path).convert("RGBA")

    backup_once()
    print()

    for name, size in TARGETS.items():
        master.resize((size, size), Image.LANCZOS).save(os.path.join(TAURI, name))
        print("  %-16s %d×%d" % (name, size, size))

    ico = os.path.join(TAURI, "icon.ico")
    master.save(ico, format="ICO", sizes=[(s, s) for s in ICO_SIZES])
    print("  %-16s %s" % ("icon.ico", ", ".join(str(s) for s in ICO_SIZES)))

    icns = os.path.join(TAURI, "icon.icns")
    try:
        # Pillow требует исходник не меньше самого крупного размера в наборе.
        master.resize((1024, 1024), Image.LANCZOS).save(icns, format="ICNS")
        print("  %-16s %s" % ("icon.icns", ", ".join(str(s) for s in ICNS_SIZES)))
    except Exception as error:
        print("  icon.icns — не вышло:", error)

    master.resize((512, 512), Image.LANCZOS).save(ASSET)
    print("  %-16s 512×512 (интерфейс: боковая панель, установка, удаление)"
          % "signum-icon.png")


def main():
    if "--restore" in sys.argv:
        restore()
        return
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    if not args:
        sys.exit("укажите знак, например: python scripts/apply-mark.py 03-zarubka")
    apply(args[0])
    print()
    print("Иконка заменена. Вернуть старую:  python scripts/apply-mark.py --restore")


if __name__ == "__main__":
    sys.exit(main())
