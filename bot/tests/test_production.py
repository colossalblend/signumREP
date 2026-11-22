from __future__ import annotations

import gzip
import logging
import sqlite3
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from signum_bot.api_secrets import ApiSecrets
from signum_bot.config import Config
from signum_bot.db import Db
from signum_bot.production import (
    BotGuard,
    RedactedFormatter,
    EncryptedFormatter,
    StorageBusy,
    backup_database,
    claim_download,
    release_storage,
    reserve_storage,
    set_maintenance,
)


class ProductionTests(unittest.IsolatedAsyncioTestCase):
    def test_encrypted_logs_hide_customer_identifiers(self):
        from cryptography.fernet import Fernet
        cipher = Fernet(Fernet.generate_key())
        formatter = EncryptedFormatter(RedactedFormatter("%(message)s"), cipher)
        record = logging.LogRecord("test", logging.INFO, "", 0,
                                   "customer 987654321 email person@example.com", (), None)
        encrypted = formatter.format(record)
        self.assertNotIn("987654321", encrypted)
        self.assertNotIn("person@example.com", encrypted)
        self.assertEqual(cipher.decrypt(encrypted.encode()).decode(), "customer 987654321 email [email]")

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cfg = replace(
            Config.load(),
            data_dir=Path(self.tmp.name),
            bridge_enabled=False,
            admins={99},
        )
        self.cfg.prepare()
        self.db = Db(self.cfg.db_path)
        self.addCleanup(self.db.conn.close)
        self.db.user(1)

    def test_storage_reservations_are_shared_across_processes(self):
        other = Db(self.cfg.db_path)
        self.addCleanup(other.conn.close)
        with patch(
            "signum_bot.production.shutil.disk_usage",
            return_value=SimpleNamespace(free=10 * 1024**3),
        ):
            reserve_storage(self.cfg, self.db, "first", 6 * 1024**3)
            with self.assertRaises(StorageBusy):
                reserve_storage(self.cfg, other, "second", 4 * 1024**3)
            self.assertEqual(
                other.one("select count(*) from storage_reservations")[0], 1
            )
            release_storage(self.db, "first")
            reserve_storage(self.cfg, other, "second", 4 * 1024**3)

    def test_backup_is_encrypted_complete_and_database_can_be_opened(self):
        self.db.account_add(
            1, "fixture@example.com", "fixture-password", "fixture-keychain"
        )
        path = backup_database(self.cfg)
        data = path.read_bytes()
        self.assertNotIn(b"fixture-password", data)
        restored = self.cfg.data_dir / "restored.db"
        restored.write_bytes(
            gzip.decompress(ApiSecrets(self.cfg.data_dir).cipher.decrypt(data))
        )
        connection = sqlite3.connect(restored)
        try:
            self.assertEqual(
                connection.execute("pragma quick_check").fetchone()[0], "ok"
            )
            self.assertEqual(
                connection.execute("select count(*) from accounts").fetchone()[0], 1
            )
        finally:
            connection.close()

    def test_same_apple_id_never_reassigns_another_owners_session(self):
        first = self.db.account_add(1, "Shared@Example.com", "first", "one")
        second = self.db.account_add(2, "shared@example.com", "second", "two")
        self.assertNotEqual(first, second)
        self.assertEqual(self.db.account(first)["tg_id"], 1)
        self.assertEqual(self.db.account(first)["password"], "first")

    def test_free_limit_counts_transitive_identity_groups_and_active_downloads(self):
        for email, owner in [
            ("one@example.com", 1),
            ("one@example.com", 2),
            ("two@example.com", 2),
            ("two@example.com", 3),
        ]:
            self.db.link_identity(email, owner)
        self.assertEqual(self.db.family(3), [1, 2, 3])
        for owner in [1, 2, 3]:
            claim_download(self.cfg, self.db, owner, str(owner), "free")
        with self.assertRaises(RuntimeError):
            claim_download(self.cfg, self.db, 3, "fourth", "free")
        self.db._run("delete from download_claims where id=?", ("1",))
        claim_download(self.cfg, self.db, 3, "retry", "free")
        self.assertEqual(self.db.week_uses(1), 3)

    async def test_maintenance_blocks_customers_but_preserves_admin_access(self):
        set_maintenance(self.db, "bot", True)
        guard = BotGuard(self.db, self.cfg.admins)
        handler = AsyncMock()
        event = SimpleNamespace(
            from_user=SimpleNamespace(id=1),
            chat=SimpleNamespace(type="private"),
            answer=AsyncMock(),
        )
        await guard(handler, event, {})
        handler.assert_not_awaited()
        event.answer.assert_awaited_once()
        event.from_user.id = 99
        await guard(handler, event, {})
        handler.assert_awaited_once()

    async def test_handler_failure_releases_capacity_and_hides_exception_details(self):
        guard = BotGuard(self.db)
        handler = AsyncMock(side_effect=RuntimeError("secret fixture password"))
        event = SimpleNamespace(
            from_user=SimpleNamespace(id=1),
            chat=SimpleNamespace(type="private"),
            answer=AsyncMock(),
        )
        await guard(handler, event, {})
        self.assertEqual(guard.total, 0)
        self.assertEqual(guard.active, {})
        self.assertNotIn("secret", event.answer.await_args.args[0])

    def test_log_formatter_redacts_credentials(self):
        values = [
            "123456789:fixture-bot-token",
            "sg_" + "x" * 43,
            "live_fixture_secret",
            "fixture@example.com",
            "https://example.com/private",
        ]
        record = logging.LogRecord(
            "fixture", logging.INFO, "fixture.py", 1, " ".join(values), (), None
        )
        formatted = RedactedFormatter("%(message)s").format(record)
        for value in values:
            self.assertNotIn(value, formatted)
