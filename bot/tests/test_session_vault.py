from __future__ import annotations

import io
from contextlib import closing
import tempfile
import unittest
import zipfile
from pathlib import Path

from cryptography.fernet import Fernet, InvalidToken

from signum_bot.session_vault import SessionVault
from signum_bot.telegram_sessions import TelegramSessions


class SessionVaultTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.vault = SessionVault.__new__(SessionVault)
        self.vault.cipher = Fernet(Fernet.generate_key())
        self.vault.ram = self.root / "ram"
        self.vault.ram.mkdir()
        self.home = self.root / "account"
        legacy = self.home / ".ipatool"
        legacy.mkdir(parents=True)
        (legacy / "account").write_bytes(b"private-session-fixture")
        (legacy / "cookies").write_bytes(b"cookie-secret-fixture-123")

    def tearDown(self):
        self.temporary.cleanup()

    def test_migration_and_updates_leave_only_ciphertext(self):
        with self.vault.open(self.home) as working:
            (working / ".ipatool" / "cookies").write_bytes(b"updated-secret-cookie")
        self.assertFalse((self.home / ".ipatool").exists())
        encrypted = self.vault.archive(self.home).read_bytes()
        self.assertNotIn(b"private-session-fixture", encrypted)
        self.assertNotIn(b"updated-secret-cookie", encrypted)
        self.assertFalse(list(self.vault.ram.iterdir()))
        with self.vault.open(self.home, persist=False) as working:
            self.assertEqual((working / ".ipatool" / "cookies").read_bytes(), b"updated-secret-cookie")
            (working / ".ipatool" / "cookies").write_bytes(b"isolated-probe-cookie")
        self.assertEqual(encrypted, self.vault.archive(self.home).read_bytes())
        self.assertFalse(self.vault.cookies_broken(self.home))

    def test_wrong_key_does_not_replace_archive(self):
        with self.vault.open(self.home):
            pass
        original = self.vault.archive(self.home).read_bytes()
        self.vault.cipher = Fernet(Fernet.generate_key())
        with self.assertRaises(InvalidToken):
            with self.vault.open(self.home):
                self.fail("Unexpected decryption")
        self.assertEqual(original, self.vault.archive(self.home).read_bytes())
        self.assertFalse(list(self.vault.ram.iterdir()))

    def test_archive_path_escape_is_rejected(self):
        content = io.BytesIO()
        with zipfile.ZipFile(content, "w") as archive:
            archive.writestr("../escape", "secret")
        self.vault.archive(self.home).write_bytes(self.vault.cipher.encrypt(content.getvalue()))
        with self.assertRaises(RuntimeError):
            with self.vault.open(self.home):
                self.fail("Path escape accepted")
        self.assertFalse((self.vault.ram / "escape").exists())

    def test_failure_preserves_two_factor_session(self):
        with self.assertRaises(RuntimeError):
            with self.vault.open(self.home) as working:
                (working / ".ipatool" / "pending").write_bytes(b"two-factor-state")
                raise RuntimeError("Awaiting code")
        with self.vault.open(self.home) as working:
            self.assertEqual((working / ".ipatool" / "pending").read_bytes(), b"two-factor-state")

    def test_telegram_database_remains_encrypted_between_connections(self):
        import sqlite3
        path = self.root / "premium.session"
        with closing(sqlite3.connect(path)) as connection:
            connection.execute("create table sessions (auth_key blob)")
            connection.execute("insert into sessions values (?)", (b"telegram-auth-fixture",))
            connection.commit()
        sessions = TelegramSessions.__new__(TelegramSessions)
        sessions.vault = self.vault
        sessions.opened = []
        working = sessions.acquire(path)
        with closing(sqlite3.connect(working)) as connection:
            self.assertEqual(connection.execute("select auth_key from sessions").fetchone()[0], b"telegram-auth-fixture")
        sessions.save()
        self.assertFalse(path.exists())
        self.assertNotIn(b"telegram-auth-fixture", path.with_suffix(".session.enc").read_bytes())
        sessions.close()
        self.assertFalse(list(self.vault.ram.iterdir()))
        restored = sessions.acquire(path)
        with closing(sqlite3.connect(restored)) as connection:
            self.assertEqual(connection.execute("select auth_key from sessions").fetchone()[0], b"telegram-auth-fixture")
        sessions.close()
