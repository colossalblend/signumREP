import unittest
import tempfile
import time
import json
from pathlib import Path
import httpx
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from signum_bot.store import Store, appmagic_app


class SearchTests(unittest.IsolatedAsyncioTestCase):
    async def test_catalog_uses_signed_bridge_and_caches_real_response(self):
        with tempfile.TemporaryDirectory() as directory:
            cfg = SimpleNamespace(
                api_base_url="https://api.example/signum",
                public_base_url="https://example.com",
                http_port=8088,
                bridge_enabled=True,
                data_dir=Path(directory),
            )
            store = Store(cfg)
            catalog = {"groups": [{"id": "bank"}], "apps": [{"id": "app"}]}
            with (
                patch("signum_bot.bridge.bridge_key", return_value=bytes(32)),
                patch(
                    "signum_bot.bridge.BridgeWorker.exchange",
                    new_callable=AsyncMock,
                    return_value={"data": catalog},
                ) as exchange,
            ):
                try:
                    self.assertEqual(await store.catalog(), catalog)
                    self.assertEqual(await store.catalog(), catalog)
                    exchange.assert_awaited_once()
                    self.assertEqual(exchange.await_args.args[1], "catalog")
                    self.assertTrue((Path(directory) / "store-catalog.json").exists())
                finally:
                    await store.close()

    async def test_unavailable_catalog_uses_recent_snapshot_but_rejects_expired_one(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            store = Store(
                SimpleNamespace(
                    api_base_url="https://example.com", data_dir=Path(directory)
                )
            )
            data = {"groups": [], "apps": [{"id": "real"}]}
            store._save_catalog(data)
            store._get = AsyncMock(side_effect=httpx.ConnectTimeout("unavailable"))
            self.assertEqual(await store.catalog(), data)
            path = Path(directory) / "store-catalog.json"
            saved = json.loads(path.read_text())
            saved["time"] = time.time() - 86401
            path.write_text(json.dumps(saved))
            with self.assertRaises(httpx.ConnectTimeout):
                await store.catalog()

    async def test_appmagic_returns_removed_ios_apps_and_deduplicates(self):
        store = Store(SimpleNamespace())
        item = {
            "name": "Removed Bank",
            "store_ids": ["1_com.android", "2_123"],
            "unitedPublisher": {"name": "Bank"},
            "removed": True,
        }
        store._get = AsyncMock(
            return_value={
                "applications": [
                    item,
                    item,
                    {"name": "Android", "store_ids": ["1_android"]},
                ]
            }
        )
        rows = await store.search("bank")
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["adam_id"], "123")
        self.assertTrue(rows[0]["removed"])
        self.assertIn("appmagic.rocks", store._get.await_args.args[0])

    async def test_developer_search_merges_stores_without_duplicate_apps(self):
        store = Store(SimpleNamespace())
        store._get = AsyncMock(
            return_value={
                "results": [
                    {"artistId": 123, "artistName": "Publisher", "trackId": 456}
                ]
            }
        )
        rows = await store.developers("Publisher")
        self.assertEqual(rows, [{"artist_id": "123", "name": "Publisher", "apps": 1}])

    def test_android_only_result_is_not_an_iphone_app(self):
        self.assertIsNone(appmagic_app({"name": "Android", "store_ids": ["1_android"]}))
