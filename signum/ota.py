from __future__ import annotations

import asyncio
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from urllib.parse import urlparse

import httpx

from signum.config import Settings
from signum.models import OtaUpdate
from signum.utils import ProgressCallback


class OtaManager:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings

    @property
    def can_self_update(self) -> bool:
        return bool(getattr(sys, "frozen", False)) and sys.platform.startswith("win")

    async def download_update(self, update: OtaUpdate, *, progress: ProgressCallback | None = None) -> Path:
        if not update.download_url:
            raise ValueError("OTA response does not contain download URL")

        target = Path(tempfile.gettempdir()) / self._download_name(update)
        async with httpx.AsyncClient(timeout=None, follow_redirects=True) as client:
            async with client.stream("GET", update.download_url) as response:
                response.raise_for_status()
                total = int(response.headers.get("content-length") or 0)
                written = 0
                with target.open("wb") as file:
                    async for chunk in response.aiter_bytes():
                        if not chunk:
                            continue
                        file.write(chunk)
                        written += len(chunk)
                        if progress and total:
                            progress(written / total * 100, f"Downloading Signum {update.version}")
        if progress:
            progress(100, f"Signum {update.version} downloaded")
        return target

    async def install_update_and_restart(self, downloaded_exe: Path) -> None:
        if not self.can_self_update:
            raise RuntimeError("Self-update is available only in the packaged Windows app")
        if "setup" in downloaded_exe.stem.lower():
            subprocess.Popen(
                [str(downloaded_exe), "--silent"],
                cwd=str(downloaded_exe.parent),
                creationflags=subprocess.CREATE_NO_WINDOW,
            )
            await asyncio.sleep(0.4)
            os._exit(0)
        current_exe = Path(sys.executable).resolve()
        script = self._write_update_script()
        subprocess.Popen(
            [
                "powershell.exe",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                str(script),
                str(current_exe),
                str(downloaded_exe),
            ],
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        await asyncio.sleep(0.4)
        os._exit(0)

    def _download_name(self, update: OtaUpdate) -> str:
        parsed = urlparse(update.download_url or "")
        name = Path(parsed.path).name
        if name.lower().endswith(".exe"):
            return name
        return f"Signum-{update.version or 'update'}.exe"

    @staticmethod
    def _write_update_script() -> Path:
        script = Path(tempfile.gettempdir()) / "signum_ota_update.ps1"
        script.write_text(
            """
param(
    [string]$CurrentExe,
    [string]$DownloadedExe
)

Start-Sleep -Seconds 2
for ($i = 0; $i -lt 60; $i++) {
    try {
        Copy-Item -LiteralPath $DownloadedExe -Destination $CurrentExe -Force
        Start-Process -FilePath $CurrentExe
        Remove-Item -LiteralPath $DownloadedExe -Force -ErrorAction SilentlyContinue
        break
    } catch {
        Start-Sleep -Seconds 1
    }
}
""".lstrip(),
            encoding="utf-8",
        )
        return script
