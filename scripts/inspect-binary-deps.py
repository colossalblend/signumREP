"""Таблицы импорта Windows-бинарей для проверки зависимостей установщика.

Использование: python inspect-binary-deps.py <file.exe|file.dll> ...
Печатает JSON: {путь: {"imports": [[вид, имя_dll], ...]}}.
Вид: "import" (обычный) или "delay" (отложенная загрузка).
Если файл не разобрался, "imports" равен null: вызывающий считает это
ошибкой, а не отсутствием зависимостей.
"""

from __future__ import annotations

import json
import sys

try:
    import pefile
except ImportError:
    sys.exit("нужен модуль pefile: python -m pip install pefile")


def imports_of(path: str):
    try:
        pe = pefile.PE(path, fast_load=True)
        pe.parse_data_directories(
            directories=[
                pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_IMPORT"],
                pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT"],
            ]
        )
    except Exception:
        return None
    result = []
    for kind, attr in (("import", "DIRECTORY_ENTRY_IMPORT"),
                       ("delay", "DIRECTORY_ENTRY_DELAY_IMPORT")):
        for entry in getattr(pe, attr, []) or []:
            name = entry.dll.decode("ascii", "replace") if entry.dll else ""
            if name:
                result.append([kind, name])
    pe.close()
    return result


def main() -> int:
    out = {path: {"imports": imports_of(path)} for path in sys.argv[1:]}
    print(json.dumps(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
