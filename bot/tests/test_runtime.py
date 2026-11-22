from __future__ import annotations

import asyncio
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from aiogram.exceptions import TelegramBadRequest
from aiogram.methods import SendMessage
from aiogram.types import InlineKeyboardButton, InlineKeyboardMarkup

from signum_bot.anim import Progress
from signum_bot import ui
from signum_bot.config import Config
from signum_bot.db import Db
from signum_bot.ipatool import Ipatool
from signum_bot.ownership import OwnedCatalog
from signum_bot.sender import FileSender, MTPROTO_MAX
from signum_bot.theme import Theme


class RuntimeTests(unittest.IsolatedAsyncioTestCase):
    def test_customer_errors_never_echo_internal_details(self):
        for reason in (
            "KBSync generation timeout",
            "[WinError 2] D:\\private\\accounts\\secret",
            "Не удалось разобрать JSON от движка",
            "KeyError: session",
        ):
            with self.subTest(reason=reason):
                text = ui.failed(reason)
                self.assertNotIn(reason, text)
                self.assertIn("Попробуйте ещё раз", text)
        self.assertIn("Apple ID", ui.failed("Сначала подключите свой Apple ID."))

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cfg = replace(
            Config.load(),
            token="test:token",
            data_dir=Path(self.tmp.name),
            ipatool_engine="v6",
        )
        self.db = Db(self.cfg.db_path)
        self.addCleanup(self.db.conn.close)
        self.db.user(1)
        self.account_id = self.db.account_add(
            1, "test@example.com", "password", "test-key"
        )
        self.account = self.db.account(self.account_id)
        self.tool = Ipatool(self.cfg, self.db)
        self.tool.prepare_catalog_probe = AsyncMock()

    async def test_persistent_cache_survives_restart_and_background_warm(self):
        tool = SimpleNamespace(
            cfg=self.cfg,
            db=self.db,
            prepare_catalog_probe=AsyncMock(),
            versions=AsyncMock(side_effect=[["222"], []]),
        )
        rows = [{"app_store_id": "111"}, {"app_store_id": "333"}]
        result = await OwnedCatalog(tool).scan(self.account, rows)
        self.assertEqual(result.versions, rows[:1])
        home = self.tool.home(self.account_id) / ".ipatool"
        home.mkdir()
        (home / "account").write_bytes(b"updated kbsync blob")
        (home / "cookies").write_bytes(b"valid test cookies" * 10)
        self.db.kbsync_warmed(self.account_id)
        result = await OwnedCatalog(tool).scan(self.db.account(self.account_id), rows)
        self.assertEqual(result.versions, rows[:1])
        self.assertEqual(tool.versions.await_count, 2)
        tool.prepare_catalog_probe.assert_awaited_once()
        self.db._run(
            "update accounts set session_generation=session_generation+1 where id=?",
            (self.account_id,),
        )
        tool.versions.side_effect = [[], ["444"]]
        result = await OwnedCatalog(tool).scan(self.db.account(self.account_id), rows)
        self.assertEqual(result.versions, rows[1:])
        self.assertEqual(tool.versions.await_count, 4)

    async def test_parallel_checks_use_isolated_sessions_and_preserve_order(self):
        home = self.tool.home(self.account_id) / ".ipatool"
        home.mkdir()
        (home / "account").write_bytes(b"cached encrypted session")
        (home / "cookies").write_bytes(b"original cookies" * 10)
        active, peak = 0, 0
        paths = set()
        barrier = asyncio.Event()

        async def versions(account, adam_id, *, timeout, home_override):
            nonlocal active, peak
            active += 1
            peak = max(peak, active)
            paths.add(home_override)
            if peak == 4:
                barrier.set()
            await asyncio.wait_for(barrier.wait(), timeout=1)
            (home_override / ".ipatool/cookies").write_bytes(
                b"rewritten worker cookies"
            )
            await asyncio.sleep(0.01)
            active -= 1
            return ["222"] if int(adam_id) % 2 else []

        self.tool.versions = AsyncMock(side_effect=versions)
        rows = [{"app_store_id": str(101 + i)} for i in range(8)]
        result = await OwnedCatalog(self.tool).scan(self.account, rows)
        self.assertEqual(result.versions, rows[::2])
        self.assertEqual(peak, 4)
        self.assertEqual(len(paths), 4)
        self.assertTrue(all(not p.exists() for p in paths))
        self.assertEqual((home / "cookies").read_bytes(), b"original cookies" * 10)

    async def test_unknown_checks_are_not_persisted_and_expired_cache_is_rechecked(
        self,
    ):
        from signum_bot.ipatool import IpatoolError

        tool = SimpleNamespace(
            cfg=self.cfg,
            db=self.db,
            prepare_catalog_probe=AsyncMock(),
            versions=AsyncMock(side_effect=IpatoolError("timeout")),
        )
        rows = [{"app_store_id": "111"}]
        await OwnedCatalog(tool).scan(self.account, rows)
        self.assertEqual(self.db.one("select count(*) n from ownership_cache")["n"], 0)
        tool.versions.side_effect = None
        tool.versions.return_value = ["222"]
        await OwnedCatalog(tool).scan(self.account, rows)
        self.db._run("update ownership_cache set expires_at=0")
        await OwnedCatalog(tool).scan(self.account, rows)
        self.assertEqual(tool.versions.await_count, 3)

    async def test_loading_keeps_animating_without_fake_zero_counter(self):
        updated = asyncio.Event()
        frames = []

        async def record(text, markup):
            frames.append(text)
            if len(frames) >= 3:
                updated.set()

        editor = AsyncMock(side_effect=record)
        progress = Progress(editor, "Приложение", 78)
        progress._started = 810
        with (
            patch("signum_bot.anim.TICK", 0.01),
            patch("signum_bot.anim.time", SimpleNamespace(monotonic=lambda: 1000)),
        ):
            try:
                await progress.start("Подготавливаю аккаунт")
                await asyncio.wait_for(updated.wait(), timeout=2)
                self.assertGreaterEqual(editor.await_count, 3)
                texts = [call.args[0] for call in editor.await_args_list]
                self.assertGreater(len(set(texts)), 1)
                self.assertNotIn("0/78", texts[-1])
                self.assertIn("3:10", texts[-1])
            finally:
                progress.stop()

    async def test_custom_emoji_permission_failure_falls_back(self):
        theme = Theme({"💎": "1234"})
        markup = InlineKeyboardMarkup(
            inline_keyboard=[
                [InlineKeyboardButton(text="💎 Подписка", callback_data="buy")]
            ]
        )
        method = SendMessage(
            chat_id=1, text="💎 <b>Подписка</b>", reply_markup=markup, parse_mode="HTML"
        )
        request = AsyncMock(
            side_effect=[
                TelegramBadRequest(method=method, message="CUSTOM_EMOJI_INVALID"),
                "sent",
            ]
        )
        self.assertEqual(await theme(request, None, method), "sent")
        fallback = request.await_args_list[-1].args[1]
        self.assertNotIn("tg-emoji", fallback.text)
        self.assertIsNone(
            getattr(
                fallback.reply_markup.inline_keyboard[0][0],
                "icon_custom_emoji_id",
                None,
            )
        )
        self.assertEqual(method.text, "💎 <b>Подписка</b>")

    async def test_premium_relay_copies_document_and_cleans_channel(self):
        sender = FileSender(
            replace(self.cfg, api_id=1, api_hash="test", upload_relay=-100123)
        )
        sender.premium_client = SimpleNamespace(
            get_me=AsyncMock(return_value=SimpleNamespace(premium=True)),
            send_file=AsyncMock(return_value=SimpleNamespace(id=10)),
            delete_messages=AsyncMock(),
        )
        api = SimpleNamespace(copy_message=AsyncMock())
        context = AsyncMock()
        context.__aenter__.return_value = api
        with patch("signum_bot.sender.Bot", return_value=context):
            await sender._send_via_relay(99, Path("test.ipa"), "caption", None)
        api.copy_message.assert_awaited_once_with(
            chat_id=99, from_chat_id=-100123, message_id=10
        )
        sender.premium_client.delete_messages.assert_awaited_once_with(-100123, [10])
        api.copy_message.side_effect = RuntimeError("temporary failure")
        with (
            patch("signum_bot.sender.Bot", return_value=context),
            self.assertRaises(RuntimeError),
        ):
            await sender._send_via_relay(99, Path("test.ipa"), "caption", None)
        self.assertEqual(sender.premium_client.delete_messages.await_count, 2)

    async def test_premium_expiry_stops_upload(self):
        sender = FileSender(self.cfg)
        sender._max_file_bytes = MTPROTO_MAX * 2
        sender.premium_client = SimpleNamespace(
            get_me=AsyncMock(return_value=SimpleNamespace(premium=False)),
            send_file=AsyncMock(),
        )
        with self.assertRaises(RuntimeError):
            await sender._send_via_relay(1, Path("test.ipa"), "caption", None)
        self.assertEqual(sender._max_file_bytes, MTPROTO_MAX)
        sender.premium_client.send_file.assert_not_awaited()


if __name__ == "__main__":
    unittest.main()
