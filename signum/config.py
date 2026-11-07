from __future__ import annotations

import os
import platform
import sys
from dataclasses import dataclass
from pathlib import Path


APP_VERSION = "1.0.4"
APP_BUILD = "7"
DEFAULT_API_BASE_URL = "https://api.appbank.pw/signum"


def _app_dir() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parent.parent


def _resource_dir() -> Path:
    frozen_resource_dir = getattr(sys, "_MEIPASS", None)
    if frozen_resource_dir:
        return Path(frozen_resource_dir)
    return Path(__file__).resolve().parent.parent


APP_DIR = _app_dir()
RESOURCE_DIR = _resource_dir()
ROOT_DIR = APP_DIR

# ipatool generations, newest first. v5 = ipatool-cpp (Sorvigolova, SAP signer
# built in, no anisette), v4 = ipatool-go (majd + sapfix, needs anisette.exe).
IPATOOL_VERSIONS = ("v5", "v4")


def _pick_ipatool(platform_dir: Path, exe_name: str) -> tuple[Path, str]:
    requested = os.getenv("SIGNUM_IPATOOL_VERSION", "").strip()
    order = (requested, *IPATOOL_VERSIONS) if requested else IPATOOL_VERSIONS
    for version in order:
        candidate = platform_dir / version / exe_name
        if candidate.is_file():
            return candidate, version
    return platform_dir / IPATOOL_VERSIONS[0] / exe_name, IPATOOL_VERSIONS[0]


@dataclass(frozen=True)
class Settings:
    api_base_url: str
    app_version: str
    app_build: str
    devkey: str
    api_timeout: float
    update_interval_seconds: int
    tools_dir: Path
    downloads_dir: Path
    ideviceinfo_path: Path | str
    ideviceinstaller_path: Path | str
    ipatool_path: Path | str
    ipatool_version: str

    @property
    def ipatool_is_cpp(self) -> bool:
        return self.ipatool_version == "v5"

    @classmethod
    def load(cls) -> "Settings":
        system = platform.system().lower()
        if system == "windows":
            tools_dir = RESOURCE_DIR / "apple-tools" / "windows-x64"
            support_dir = tools_dir / "support"
            exe_name = "ipatool.exe"
            ideviceinfo = os.getenv("SIGNUM_IDEVICEINFO", support_dir / "ideviceinfo.exe")
            ideviceinstaller = os.getenv("SIGNUM_IDEVICEINSTALLER", support_dir / "ideviceinstaller.exe")
        else:
            arm = platform.machine().lower() in {"arm64", "aarch64"}
            tools_dir = RESOURCE_DIR / "apple-tools" / ("macos-arm64" if system == "darwin" and arm else "macos-x64")
            exe_name = "ipatool"
            ideviceinfo = os.getenv("SIGNUM_IDEVICEINFO", "ideviceinfo")
            ideviceinstaller = os.getenv("SIGNUM_IDEVICEINSTALLER", "ideviceinstaller")

        ipatool_override = os.getenv("SIGNUM_IPATOOL")
        if ipatool_override:
            ipatool: Path | str = Path(ipatool_override)
            ipatool_version = Path(ipatool_override).parent.name
        else:
            ipatool, ipatool_version = _pick_ipatool(tools_dir, exe_name)

        downloads_dir = Path(os.getenv("SIGNUM_DOWNLOADS_DIR", ROOT_DIR / "downloads"))
        return cls(
            api_base_url=os.getenv("SIGNUM_API_BASE_URL", DEFAULT_API_BASE_URL).rstrip("/"),
            app_version=os.getenv("SIGNUM_APP_VERSION", APP_VERSION),
            app_build=os.getenv("SIGNUM_APP_BUILD", APP_BUILD),
            devkey=os.getenv("SIGNUM_DEVKEY", ""),
            api_timeout=float(os.getenv("SIGNUM_API_TIMEOUT", "15")),
            update_interval_seconds=int(os.getenv("SIGNUM_UPDATE_INTERVAL_SECONDS", "600")),
            tools_dir=tools_dir,
            downloads_dir=downloads_dir,
            ideviceinfo_path=ideviceinfo,
            ideviceinstaller_path=ideviceinstaller,
            ipatool_path=ipatool,
            ipatool_version=ipatool_version,
        )
