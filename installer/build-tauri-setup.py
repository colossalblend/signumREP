                      
"""Собирает Rust/Tauri установщик SignumSetup.exe."""

from __future__ import annotations

import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC_SETUP = ROOT / "src-setup"
OUT_DIR = ROOT / "dist"
INSTALLERS = ROOT / "installers"


def package_version() -> str:
    for line in (SRC_SETUP / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        if line.startswith("version") and '"' in line:
            return line.split('"', 2)[1]
    raise RuntimeError("Не найдена версия src-setup/Cargo.toml")


def append_payload(exe: Path, payload: Path) -> None:
    if not payload.is_file():
        raise FileNotFoundError(f"Не найден payload: {payload}")
    with zipfile.ZipFile(payload) as archive:
        if "signum.exe" not in archive.namelist():
            raise RuntimeError("Payload не содержит signum.exe")
    with exe.open("ab") as output, payload.open("rb") as source:
        shutil.copyfileobj(source, output, length=1024 * 1024)


def main() -> int:
    cmd = ["npx", "--yes", "tauri", "build", "--no-bundle"]
    if sys.platform.startswith("win"):
                            
        completed = subprocess.run(cmd, cwd=SRC_SETUP, shell=True)
    else:
        completed = subprocess.run(cmd, cwd=SRC_SETUP)
    if completed.returncode != 0:
        return completed.returncode

    exe = SRC_SETUP / "target" / "release" / "signum-setup.exe"
    if not exe.is_file():
                                 
        alt = SRC_SETUP / "target" / "release" / "SignumSetup.exe"
        exe = alt if alt.is_file() else exe
    if not exe.is_file():
        print("Не найден собранный exe", file=sys.stderr)
        return 1

    payload = ROOT / "installer" / "payload" / "SignumPayload.zip"
    if not payload.is_file():
        payload = SRC_SETUP / "target" / "release" / "SignumPayload.zip"
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    INSTALLERS.mkdir(parents=True, exist_ok=True)
    dest = OUT_DIR / "SignumSetup.exe"
    versioned_dest = INSTALLERS / f"Signum_{package_version()}_Setup.exe"
    shutil.copy2(exe, dest)
    append_payload(dest, payload)
    shutil.copy2(exe, versioned_dest)
    append_payload(versioned_dest, payload)

                                                                                    
    shutil.copy2(payload, OUT_DIR / "SignumPayload.zip")
    shutil.copy2(payload, dest.with_name("SignumPayload.zip"))

    print(dest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
