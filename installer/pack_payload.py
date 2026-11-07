                      
"""Собирает Windows-payload: signum.exe + единая структура Apple-инструментов."""

from __future__ import annotations

import argparse
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RELEASE = ROOT / "src-tauri" / "target" / "release"
OUT = Path(__file__).resolve().parent / "payload" / "SignumPayload.zip"


def add_file(zf: zipfile.ZipFile, src: Path, arc: str) -> None:
    zf.write(src, arcname=arc.replace("\\", "/"))


def pack(release: Path, dest: Path) -> Path:
    exe = release / "signum.exe"
    apple_tools = release / "resources" / "apple-tools"
    if not exe.is_file():
        raise FileNotFoundError(f"Нет {exe}. Сначала соберите десктоп: npm run build:win")
    if not apple_tools.is_dir():
        raise FileNotFoundError(f"Нет {apple_tools}")

    dest.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(dest, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as zf:
        add_file(zf, exe, "signum.exe")
        # Отдельный деинсталлятор (собирается из src-setup). Без него в системе
        # остаётся только uninstall.cmd-заглушка, которая не чистит данные.
        uninstaller = ROOT / "src-setup" / "target" / "release" / "uninstall.exe"
        if uninstaller.is_file():
            add_file(zf, uninstaller, "uninstall.exe")
        for path in sorted(apple_tools.rglob("*")):
            if path.is_file():
                add_file(
                    zf,
                    path,
                    str(Path("resources") / "apple-tools" / path.relative_to(apple_tools)),
                )
    return dest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", type=Path, default=RELEASE)
    parser.add_argument("--out", type=Path, default=OUT)
    args = parser.parse_args()
    out = pack(args.release, args.out)
    print(out)
    print(f"{out.stat().st_size / (1024 * 1024):.1f} МБ")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
