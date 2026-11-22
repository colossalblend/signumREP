from __future__ import annotations

import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from aiohttp.test_utils import TestClient, TestServer

from signum_bot import ota, ui
from signum_bot.config import Config
from signum_bot.db import Db


class LinkExpiryTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cfg = replace(
            Config.load(),
            token="test:token",
            data_dir=Path(self.tmp.name),
            link_ttl_free_hours=1,
            link_ttl_paid_hours=12,
            public_base_url="https://example.com",
        )
        self.cfg.prepare()
        self.db = Db(self.cfg.db_path)
        self.addCleanup(self.db.conn.close)
        self.db.user(1)
        self.path = self.cfg.files_dir / "fixture" / "test.ipa"
        self.path.parent.mkdir()
        self.path.write_bytes(b"test archive")
        self.client = TestClient(TestServer(ota.build_app(self.cfg, self.db)))
        await self.client.start_server()
        self.addAsyncCleanup(self.client.close)

    def add_file(self, token, paid, created=10000, *, legacy=False):
        ttl = self.cfg.link_hours(paid)
        with patch("signum_bot.db.now", return_value=created):
            self.db.file_add(
                token,
                adam_id="123",
                tg_id=1,
                account_id=1,
                title="Test App",
                name="test.ipa",
                path=str(self.path),
                size=self.path.stat().st_size,
                paid_with="" if legacy else paid,
                ttl_hours=0 if legacy else ttl,
                expires_at=created + (24 if legacy else ttl) * 3600,
            )

    async def test_plan_expiry_and_captions_match(self):
        with patch("signum_bot.ota.now", return_value=10000):
            for paid, hours in (("free", 1), ("sub", 12), ("credit", 12)):
                self.assertEqual(ota.expiry(self.cfg, paid), 10000 + hours * 3600)
                self.add_file(paid, paid)
                row = self.db.file(paid)
                for encrypted in (False, True):
                    caption = ui.ready_ota(
                        "Test App", 10, row["ttl_hours"], "test@example.com", encrypted
                    )
                    self.assertIn(f"Ссылка живёт {hours} ч.", caption)
                response = await self.client.get(f"/i/{paid}")
                self.assertEqual(response.status, 200)
                self.assertIn(f"Ссылка живёт {hours} ч.", await response.text())

    async def test_all_routes_expire_exactly_without_waiting_for_cleaner(self):
        self.add_file("free", "free")
        deadline = 10000 + 3600
        urls = ("/i/free", "/ota/free.plist", "/f/free.ipa")
        with patch("signum_bot.ota.now", return_value=deadline - 1):
            for url in urls:
                response = await self.client.get(url)
                self.assertEqual(response.status, 200)
                await response.read()
        for timestamp in (deadline, deadline + 1):
            with patch("signum_bot.ota.now", return_value=timestamp):
                for url in urls:
                    response = await self.client.get(url)
                    self.assertEqual(response.status, 404)
                    self.assertIn("Ссылка устарела", await response.text())
                response = await self.client.get(
                    "/f/free.ipa", headers={"Range": "bytes=0-1"}
                )
                self.assertEqual(response.status, 404)
        self.assertTrue(self.path.exists())
        self.assertIsNotNone(self.db.file("free"))

    async def test_subscription_change_does_not_change_existing_deadline(self):
        self.add_file("free", "free")
        self.db.extend_sub(1, 30)
        with patch("signum_bot.ota.now", return_value=10000 + 2 * 3600):
            self.assertEqual((await self.client.get("/i/free")).status, 404)
        self.add_file("paid", "sub")
        self.db._run("update users set sub_until=0 where tg_id=1")
        with patch("signum_bot.ota.now", return_value=10000 + 3 * 3600):
            self.assertEqual((await self.client.get("/i/paid")).status, 200)

    async def test_existing_links_migrate_by_actual_payment_and_only_once(self):
        for i, paid in enumerate(("free", "sub", "credit")):
            created = 10000 + i * 1000
            self.add_file(paid, paid, created, legacy=True)
            with patch("signum_bot.db.now", return_value=created + 2):
                self.db.history_add(1, "123", "Test App", "ota", 1, "ok", paid)
        self.assertEqual(ota.migrate_links(self.cfg, self.db), 3)
        for i, paid in enumerate(("free", "sub", "credit")):
            row = self.db.file(paid)
            ttl = 1 if paid == "free" else 12
            self.assertEqual(row["paid_with"], paid)
            self.assertEqual(row["ttl_hours"], ttl)
            self.assertEqual(row["expires_at"], 10000 + i * 1000 + ttl * 3600)
        self.assertEqual(ota.migrate_links(self.cfg, self.db), 0)


if __name__ == "__main__":
    unittest.main()
