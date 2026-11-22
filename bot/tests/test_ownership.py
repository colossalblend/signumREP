from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from signum_bot.ipatool import IpatoolError  # noqa: E402
from signum_bot.ownership import OwnedCatalog  # noqa: E402


class OwnershipTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.tool = SimpleNamespace(
            cfg=SimpleNamespace(accounts_dir=Path(self.tmp.name)),
            versions=AsyncMock(),
            prepare_catalog_probe=AsyncMock(),
        )
        self.catalog = OwnedCatalog(self.tool)
        self.account = {"id": 1, "keychain": "test-key"}
        self.rows = [
            {"app_store_id": "111", "title": "Owned cover"},
            {
                "app_store_id": "111",
                "title": "Owned old build",
                "external_version_id": "222",
            },
            {
                "app_store_id": "111",
                "title": "Missing old build",
                "external_version_id": "333",
            },
            {"app_store_id": "444", "title": "Not purchased"},
        ]

    def tearDown(self) -> None:
        self.tmp.cleanup()

    async def test_only_owned_apps_and_matching_build_ids(self) -> None:
        self.tool.versions.side_effect = [["222"], []]
        scan = await self.catalog.scan(self.account, self.rows)
        self.assertEqual(
            [row["title"] for row in scan.versions], ["Owned cover", "Owned old build"]
        )
        self.assertEqual(scan.unchecked, 0)
        self.assertEqual(self.tool.versions.await_count, 2)
        self.tool.prepare_catalog_probe.assert_awaited_once()

    async def test_account_cache_and_force_refresh(self) -> None:
        self.tool.versions.return_value = ["222"]
        await self.catalog.scan(self.account, self.rows[:3])
        await self.catalog.scan(self.account, self.rows[:3])
        self.assertEqual(self.tool.versions.await_count, 1)
        await self.catalog.scan({"id": 2, "keychain": "other-key"}, self.rows[:3])
        self.assertEqual(self.tool.versions.await_count, 2)
        self.tool.versions.return_value = []
        scan = await self.catalog.scan(self.account, self.rows[:3], force=True)
        self.assertEqual(scan.versions, [])
        self.assertEqual(self.tool.versions.await_count, 3)

    async def test_session_changes_invalidate_cache(self) -> None:
        self.tool.versions.return_value = ["222"]
        await self.catalog.scan(self.account, self.rows[:1])
        home = Path(self.tmp.name) / "1/.ipatool"
        home.mkdir(parents=True)
        (home / "account").write_bytes(b"new session")
        self.tool.versions.return_value = []
        (home / "cookies").write_bytes(b"valid store cookies" * 10)
        scan = await self.catalog.scan(self.account, self.rows[:1])
        self.assertEqual(scan.versions, [])
        self.assertEqual(scan.unchecked, 0)
        self.assertEqual(self.tool.versions.await_count, 2)

    async def test_inconclusive_checks_are_hidden_and_not_cached(self) -> None:
        self.tool.versions.side_effect = IpatoolError("timeout")
        for _ in range(2):
            scan = await self.catalog.scan(self.account, self.rows[:3])
            self.assertEqual(scan.versions, [])
            self.assertEqual(scan.unchecked, 3)
        self.assertEqual(self.tool.versions.await_count, 2)

    async def test_missing_cookies_do_not_prove_absence(self) -> None:
        home = Path(self.tmp.name) / "1/.ipatool"
        home.mkdir(parents=True)
        (home / "account").write_bytes(b"session")
        self.tool.versions.return_value = []
        scan = await self.catalog.scan(self.account, self.rows[:1])
        self.assertEqual(scan.unchecked, 1)
        self.assertEqual(len(self.catalog.cache), 0)

    async def test_expired_session_is_not_mistaken_for_empty_purchases(self) -> None:
        self.tool.versions.side_effect = IpatoolError(
            "failed", "password token is expired; empty songList"
        )
        scan = await self.catalog.scan(self.account, self.rows)
        self.assertEqual(scan.unchecked, 4)
        self.assertEqual(scan.versions, [])
        self.tool.versions.assert_awaited_once()

    async def test_region_block_confirms_cover_but_not_old_build(self) -> None:
        self.tool.versions.side_effect = IpatoolError(
            "failed", "license was granted but download was blocked"
        )
        scan = await self.catalog.scan(self.account, self.rows[:3])
        self.assertEqual([row["title"] for row in scan.versions], ["Owned cover"])
        self.assertEqual(scan.unchecked, 2)


if __name__ == "__main__":
    unittest.main()
