from __future__ import annotations

import asyncio
import tempfile
import threading
import unittest
from contextlib import suppress
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from cachetools import TTLCache

from signum_bot.cards import Cards
from signum_bot.config import Config
from signum_bot.rate_limits import take_request
from signum_bot.store import Store


class ResponsivenessTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cfg = replace(Config.load(), data_dir=Path(self.tmp.name))

    async def test_concurrent_metadata_requests_share_one_fetch_and_cache(self):
        store = Store(self.cfg)
        response = SimpleNamespace(
            raise_for_status=lambda: None, json=lambda: {"results": []}
        )
        store._client = SimpleNamespace(
            get=AsyncMock(return_value=response), aclose=AsyncMock()
        )
        await asyncio.gather(*(store.search("same app") for _ in range(10)))
        await store.search("same app")
        store._client.get.assert_awaited_once()
        await store.close()

    async def test_cancelled_waiter_does_not_cancel_shared_metadata_fetch(self):
        store = Store(self.cfg)
        started, finish = asyncio.Event(), asyncio.Event()

        async def get(*args, **kwargs):
            started.set()
            await finish.wait()
            return SimpleNamespace(
                raise_for_status=lambda: None, json=lambda: {"results": []}
            )

        store._client = SimpleNamespace(
            get=AsyncMock(side_effect=get), aclose=AsyncMock()
        )
        first = asyncio.create_task(store.search("same app"))
        second = asyncio.create_task(store.search("same app"))
        await started.wait()
        first.cancel()
        with suppress(asyncio.CancelledError):
            await first
        finish.set()
        self.assertEqual(await second, [])
        store._client.get.assert_awaited_once()
        await store.close()

    async def test_card_render_does_not_block_other_bot_coroutines(self):
        cards = Cards(self.cfg)
        started = asyncio.Event()
        release = threading.Event()
        loop = asyncio.get_running_loop()

        def render(*args):
            loop.call_soon_threadsafe(started.set)
            release.wait(2)
            return b"rendered"

        cards._grid = render
        task = asyncio.create_task(cards.grid("title", "subtitle", []))
        try:
            await asyncio.wait_for(started.wait(), timeout=1)
            self.assertFalse(task.done())
        finally:
            release.set()
        self.assertEqual(await task, b"rendered")

    def test_rate_window_resets_while_client_keeps_polling(self):
        cache = TTLCache(maxsize=10, ttl=60)
        with patch(
            "signum_bot.rate_limits.time.monotonic", side_effect=[0, 20, 40, 60]
        ):
            self.assertTrue(take_request(cache, "client", 2))
            self.assertTrue(take_request(cache, "client", 2))
            self.assertFalse(take_request(cache, "client", 2))
            self.assertTrue(take_request(cache, "client", 2))


if __name__ == "__main__":
    unittest.main()
