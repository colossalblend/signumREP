from __future__ import annotations

import subprocess
import sys
import time
import plistlib
import zipfile
from pathlib import Path

from signum.config import Settings
from signum.storage import CredentialStore
from signum.utils import CommandError, ProgressCallback, run_command, run_streaming_command


class IpaToolManager:
    def __init__(self, settings: Settings, store: CredentialStore) -> None:
        self.settings = settings
        self.store = store
        self.settings.downloads_dir.mkdir(parents=True, exist_ok=True)

    def _base_command(self) -> list[str | Path]:
        command: list[str | Path] = [self.settings.ipatool_path]
        if not self.settings.ipatool_is_cpp:
            # ipatool-cpp parses flags itself and has no --non-interactive
            command.append("--non-interactive")
        if self.settings.ipatool_version in {"v2", "v3"}:
            # Only the anisette-based engines need the helper next to them;
            # v4 (Go sapfix) and v5 (cpp) sign Apple actions themselves.
            self._ensure_anisette_helper()
        command += [
            "--keychain-passphrase",
            self.store.get_or_create_ipatool_keychain_passphrase(),
        ]
        return command

    def _ensure_anisette_helper(self) -> None:
        if not sys.platform.startswith("win"):
            return
        ipatool = Path(self.settings.ipatool_path)
        anisette = ipatool.parent / "anisette.exe"
        if not anisette.is_file():
            raise RuntimeError(
                f"Для входа в Apple ID нужен anisette.exe. Поместите файл рядом с ipatool.exe: {anisette}"
            )

    async def login(self, email: str, password: str, two_factor_code: str | None = None) -> None:
        if not email.strip() or not password:
            raise ValueError("Введите Apple ID и пароль")
        self._cleanup_cookie_lock_if_stale()
        command: list[str | Path] = [
            *self._base_command(),
            "auth",
            "login",
            "-e",
            email.strip(),
            "-p",
            password,
        ]
        if two_factor_code:
            command.extend(["--auth-code", two_factor_code.strip()])
        try:
            await run_command(command, timeout=120)
        except CommandError as exc:
            if self._is_cookie_lock_error(exc.output):
                self._cleanup_cookie_lock_if_stale(force=True)
                try:
                    await run_command(command, timeout=120)
                except CommandError as retry_exc:
                    if self._is_cookie_lock_error(retry_exc.output):
                        raise RuntimeError("ipatool занят другим процессом. Закройте другие окна Signum или подождите минуту и повторите вход.") from retry_exc
                    raise
            else:
                raise
        self.store.save_auth_session(email.strip())

    async def purchase(self, bundle_id: str) -> None:
        if not bundle_id:
            raise ValueError("bundle_id пустой")
        await run_command([*self._base_command(), "purchase", "-b", bundle_id], timeout=120)
        self.store.mark_purchased(bundle_id)

    async def download(
        self,
        bundle_id: str,
        *,
        app_store_id: str | None = None,
        progress: ProgressCallback | None = None,
    ) -> Path:
        if not bundle_id:
            raise ValueError("bundle_id пустой")
        app_store_id = self._normalize_app_store_id(app_store_id)
        started_at = time.time()
        before = {path: path.stat().st_mtime for path in self._ipa_files()}
        await self._run_download_command(
            bundle_id,
            app_store_id=app_store_id,
            progress=progress,
        )
        after = self._ipa_files()
        candidates = [
            path
            for path in after
            if path not in before or path.stat().st_mtime != before[path] or path.stat().st_mtime >= started_at - 2
        ]
        candidates = candidates or after
        matching = [path for path in candidates if self._ipa_bundle_id(path) == bundle_id]
        if not matching:
            matching = [path for path in after if self._ipa_bundle_id(path) == bundle_id]
        if not matching:
            seen_bundle_ids = sorted({bundle for path in candidates if (bundle := self._ipa_bundle_id(path))})
            details = f" Найдены bundle_id: {', '.join(seen_bundle_ids)}." if seen_bundle_ids else ""
            raise RuntimeError(f"ipatool download завершился без IPA для bundle_id {bundle_id}.{details}")
        return max(matching, key=lambda path: path.stat().st_mtime)

    async def _run_download_command(
        self,
        bundle_id: str,
        *,
        app_store_id: str | None,
        progress: ProgressCallback | None,
    ) -> None:
        command: list[str | Path] = [
            *self._base_command(),
            "download",
            "-o",
            self.settings.downloads_dir,
        ]
        if app_store_id:
            command.extend(["-i", app_store_id])
        else:
            command.extend(["-b", bundle_id])
        await run_streaming_command(command, progress=progress, timeout=900)

    @staticmethod
    def _normalize_app_store_id(app_store_id: str | None) -> str | None:
        if not app_store_id:
            return None
        value = app_store_id.strip()
        return value if value.isdecimal() else None

    def _ipa_files(self) -> list[Path]:
        return list(self.settings.downloads_dir.glob("*.ipa"))

    @staticmethod
    def _ipa_bundle_id(path: Path) -> str | None:
        try:
            with zipfile.ZipFile(path) as archive:
                info_name = next(
                    (
                        name
                        for name in archive.namelist()
                        if name.startswith("Payload/") and name.endswith(".app/Info.plist")
                    ),
                    None,
                )
                if not info_name:
                    return None
                payload = plistlib.loads(archive.read(info_name))
        except Exception:
            return None
        if not isinstance(payload, dict):
            return None
        bundle_id = payload.get("CFBundleIdentifier")
        return str(bundle_id) if bundle_id else None

    def _cookie_lock_path(self) -> Path:
        return Path.home() / ".ipatool" / "cookies.lock"

    def _cleanup_cookie_lock_if_stale(self, *, force: bool = False) -> None:
        lock_path = self._cookie_lock_path()
        if not lock_path.exists():
            return
        try:
            age_seconds = time.time() - lock_path.stat().st_mtime
            if force and not self._ipatool_process_running():
                lock_path.unlink(missing_ok=True)
                return
            if age_seconds > 30 and not self._ipatool_process_running():
                lock_path.unlink(missing_ok=True)
        except OSError:
            return

    def _ipatool_process_running(self) -> bool:
        if not sys.platform.startswith("win"):
            return False
        try:
            kwargs = {"capture_output": True, "text": True, "timeout": 5}
            if hasattr(subprocess, "CREATE_NO_WINDOW"):
                kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
            result = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ipatool.exe"], **kwargs)
        except Exception:
            return True
        return "ipatool.exe" in result.stdout.lower()

    @staticmethod
    def _is_cookie_lock_error(output: str) -> bool:
        lowered = output.lower()
        return "cookies.lock" in lowered or "cannot acquire lock" in lowered or "file locked" in lowered
