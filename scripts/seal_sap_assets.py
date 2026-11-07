"""Запечатывает артефакты FairPlay в один контейнер для kbsync.

Дылибы Apple лежали рядом с утилитой открытыми файлами: их можно было просто
скопировать вместе с kbsync и получить рабочий генератор подтверждений. Здесь
они складываются в assets.bin под AES-256-GCM; ключ Signum передаёт утилите
через окружение, и без него контейнер бесполезен.

Формат: 12 байт nonce, дальше GCM. Внутри повторяются записи
[1 байт длины имени][имя][4 байта длины BE][данные].

Использование:
    python scripts/seal_sap_assets.py <папка-с-артефактами> <assets.bin> <ключ-hex>
"""
import os
import sys
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

NAMES = ["CommerceKit", "storeagent", "CommerceCore", "CoreFP", "CoreFP.icxs"]


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__)
        return 2
    src, dst, key_hex = sys.argv[1], sys.argv[2], sys.argv[3].strip()
    key = bytes.fromhex(key_hex)
    if len(key) != 32:
        print("ключ должен быть 32 байта (64 hex-символа)")
        return 2

    blob = bytearray()
    for name in NAMES:
        path = os.path.join(src, name)
        if not os.path.isfile(path):
            print(f"нет артефакта: {path}")
            return 1
        data = open(path, "rb").read()
        raw = name.encode()
        blob.append(len(raw))
        blob += raw
        blob += len(data).to_bytes(4, "big")
        blob += data
        print(f"  {name}: {len(data)} байт")

    # Nonce детерминированный от ключа: контейнер собирается один раз при сборке,
    # переиспользования nonce с разным содержимым не возникает, а сборка остаётся
    # воспроизводимой — одинаковый вход даёт одинаковый файл.
    nonce = key[:12]
    sealed = AESGCM(key).encrypt(nonce, bytes(blob), None)
    with open(dst, "wb") as out:
        out.write(nonce + sealed)
    print(f"запечатано: {dst}, {os.path.getsize(dst)} байт")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
