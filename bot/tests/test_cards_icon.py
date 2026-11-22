import base64
import io
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from PIL import Image

from signum_bot.cards import Cards


class IconTests(unittest.IsolatedAsyncioTestCase):
    async def test_catalog_icon_uses_bridge_and_is_cached_on_disk(self):
        with tempfile.TemporaryDirectory() as directory:
            cfg = SimpleNamespace(
                data_dir=Path(directory),
                bridge_enabled=True,
                public_base_url="https://example.com",
                http_port=8088,
            )
            cards = Cards(cfg)
            buffer = io.BytesIO()
            Image.new("RGB", (800, 800), "red").save(buffer, "PNG")
            result = {"data": base64.b64encode(buffer.getvalue()).decode()}
            with (
                patch("signum_bot.bridge.bridge_key", return_value=bytes(32)),
                patch(
                    "signum_bot.bridge.BridgeWorker.exchange",
                    new_callable=AsyncMock,
                    return_value=result,
                ) as exchange,
            ):
                url = "https://cdn.appbank.pw/normal/rocket/main.png"
                first = await cards.icon(url)
                second = await cards.icon(url)
                self.assertEqual(first.size, (512, 512))
                self.assertEqual(second.getpixel((0, 0)), (255, 0, 0, 255))
                exchange.assert_awaited_once()
                self.assertEqual(exchange.await_args.args[1], "catalog_icon")
