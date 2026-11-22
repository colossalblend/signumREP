from __future__ import annotations

import io
import os
import shutil
import tempfile
import zipfile
from contextlib import contextmanager
from pathlib import Path

from .api_secrets import ApiSecrets

MAX_SESSION_BYTES = 16 * 1024 * 1024


class SessionVault:
    def __init__(self, data_dir):
        self.cipher = ApiSecrets(data_dir).cipher
        self.ram = Path("/dev/shm") / f"signum-sessions-{os.getuid()}"
        if not Path("/dev/shm").is_dir():
            raise RuntimeError("Session encryption requires a RAM filesystem")
        self.ram.mkdir(mode=0o700, exist_ok=True)
        if self.ram.stat().st_uid != os.getuid():
            raise RuntimeError("Session RAM directory ownership mismatch")
        self.ram.chmod(0o700)

    def archive(self, home):
        return home / ".store-session.enc"

    def contents(self, home):
        path = self.archive(home)
        if not path.exists():
            return None
        if path.stat().st_size > MAX_SESSION_BYTES * 2:
            raise RuntimeError("Session archive too large")
        return self.cipher.decrypt(path.read_bytes())

    def cookies_broken(self, home):
        content = self.contents(home)
        if content is None:
            return False
        with zipfile.ZipFile(io.BytesIO(content)) as archive:
            if not any(n in archive.namelist() for n in ("account", "keyring/account")):
                return False
            try:
                return archive.getinfo("cookies").file_size < 16
            except KeyError:
                return True

    @contextmanager
    def open(self, home, *, persist=True):
        home.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="apple-", dir=self.ram) as temporary:
            working = Path(temporary)
            target = working / ".ipatool"
            content = self.contents(home)
            if content is not None:
                target.mkdir(mode=0o700)
                with zipfile.ZipFile(io.BytesIO(content)) as archive:
                    if sum(item.file_size for item in archive.infolist()) > MAX_SESSION_BYTES:
                        raise RuntimeError("Session archive too large")
                    for item in archive.infolist():
                        relative = Path(item.filename)
                        if relative.is_absolute() or ".." in relative.parts:
                            raise RuntimeError("Invalid session archive path")
                        if item.is_dir():
                            continue
                        file = target / relative
                        file.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                        file.write_bytes(archive.read(item))
                        file.chmod(0o600)
            elif (home / ".ipatool").is_dir():
                shutil.copytree(home / ".ipatool", target,
                                ignore=shutil.ignore_patterns("*.lock", "ipa"))
            try:
                yield working
            finally:
                if persist and target.is_dir():
                    buffer = io.BytesIO()
                    total = 0
                    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
                        for file in target.rglob("*"):
                            if not file.is_file() or file.is_symlink() or file.name.endswith(".lock"):
                                continue
                            relative = file.relative_to(target)
                            if "ipa" in relative.parts:
                                continue
                            total += file.stat().st_size
                            if total > MAX_SESSION_BYTES:
                                raise RuntimeError("Session archive too large")
                            archive.writestr(relative.as_posix(), file.read_bytes())
                    destination = self.archive(home)
                    temporary_file = destination.with_suffix(".part")
                    with temporary_file.open("wb") as file:
                        temporary_file.chmod(0o600)
                        file.write(self.cipher.encrypt(buffer.getvalue()))
                        file.flush()
                        os.fsync(file.fileno())
                    os.replace(temporary_file, destination)
                    if (home / ".ipatool").is_dir():
                        shutil.rmtree(home / ".ipatool")
