from __future__ import annotations

import os
import shutil
import sqlite3
import tempfile
from contextlib import closing
from pathlib import Path

from .session_vault import SessionVault


class TelegramSessions:
    def __init__(self, data_dir):
        self.vault = SessionVault(data_dir)
        self.opened = []

    @staticmethod
    def paths(path):
        legacy = path if str(path).endswith(".session") else Path(str(path) + ".session")
        return legacy, legacy.with_suffix(".session.enc")

    def exists(self, path):
        return any(file.is_file() for file in self.paths(path))

    def acquire(self, path):
        legacy, encrypted = self.paths(path)
        temporary = Path(tempfile.mkdtemp(prefix="telegram-", dir=self.vault.ram))
        working = temporary / "session.session"
        try:
            if encrypted.exists():
                working.write_bytes(self.vault.cipher.decrypt(encrypted.read_bytes()))
            elif legacy.exists():
                with closing(sqlite3.connect(legacy.as_uri() + "?mode=ro", uri=True)) as source:
                    with closing(sqlite3.connect(working)) as target:
                        source.backup(target)
            if working.exists():
                working.chmod(0o600)
            self.opened.append((legacy, encrypted, working))
            return str(working)
        except BaseException:
            shutil.rmtree(temporary)
            raise

    def save(self):
        for legacy, encrypted, working in self.opened:
            if not working.is_file():
                continue
            snapshot = working.with_name("snapshot.sqlite")
            try:
                with closing(sqlite3.connect(working)) as source:
                    with closing(sqlite3.connect(snapshot)) as target:
                        source.backup(target)
                encrypted.parent.mkdir(parents=True, exist_ok=True)
                pending = encrypted.with_suffix(".part")
                with pending.open("wb") as stream:
                    pending.chmod(0o600)
                    stream.write(self.vault.cipher.encrypt(snapshot.read_bytes()))
                    stream.flush()
                    os.fsync(stream.fileno())
                os.replace(pending, encrypted)
                for suffix in ("", "-wal", "-shm", "-journal"):
                    Path(str(legacy) + suffix).unlink(missing_ok=True)
            finally:
                snapshot.unlink(missing_ok=True)

    def close(self):
        try:
            self.save()
        finally:
            for _, _, working in self.opened:
                shutil.rmtree(working.parent)
            self.opened.clear()
