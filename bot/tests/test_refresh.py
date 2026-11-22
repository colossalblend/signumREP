from __future__ import annotations

import os
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
os.environ["BOT_TOKEN"] = "test:token"

from signum_bot.cards import Cards
from signum_bot.config import Config
from signum_bot.db import Db, now
from signum_bot.handlers import Deps, build_router
from signum_bot.ipatool import Ipatool, IpatoolError
from signum_bot.maintenance import KbsyncKeeper
from signum_bot.sender import FileSender
from signum_bot.store import Store
from signum_bot import ui


class RefreshTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.cfg = replace(
            Config.load(), data_dir=Path(self.tmp.name), ipatool_engine="v6"
        )
        self.db = Db(self.cfg.db_path)
        self.tool = Ipatool(self.cfg, self.db)
        self.tool.run = AsyncMock(return_value={"success": True})
        self.keeper = KbsyncKeeper(self.db, self.tool)

    def tearDown(self) -> None:
        self.db.conn.close()
        self.tmp.cleanup()

    def account(
        self, tg_id: int, *, subscribed: bool = True, status: str = "ready"
    ) -> int:
        self.db.user(tg_id)
        if subscribed:
            self.db.extend_sub(tg_id, 30)
        account_id = self.db.account_add(
            tg_id, f"{tg_id}@example.com", "test-password", "test-key"
        )
        self.db.account_status(account_id, status)
        return account_id

    async def test_only_ready_unblocked_subscribers_are_warmed(self) -> None:
        paid = self.account(1)
        self.account(2, subscribed=False)
        self.account(3, status="need2fa")
        self.account(4)
        self.db._run("update users set blocked=1 where tg_id=4")
        await self.keeper.tick()
        self.tool.run.assert_awaited_once()
        args, kwargs = self.tool.run.call_args
        self.assertEqual(args[0]["id"], paid)
        self.assertEqual(args[1], ["kbsync", "--refresh"])
        self.assertEqual(kwargs["timeout"], 600)
        self.assertGreater(self.db.account(paid)["kbsync_warmed_at"], 0)

    async def test_other_versions_refreshes_apple_and_never_uses_numbered_fake_labels(
        self,
    ):
        identifier = self.account(1)
        self.tool.versions = AsyncMock(return_value=["889111", "889112"])
        deps = Deps(
            self.cfg,
            self.db,
            SimpleNamespace(),
            self.tool,
            SimpleNamespace(),
            SimpleNamespace(),
        )
        router = build_router(deps)
        callback = next(
            handler.callback
            for handler in router.callback_query.handlers
            if handler.callback.__name__ == "app_versions"
        )
        message = SimpleNamespace(photo=[], edit_text=AsyncMock())
        cq = SimpleNamespace(
            data="ver:123:0",
            from_user=SimpleNamespace(id=1),
            answer=AsyncMock(),
            message=message,
        )
        await callback(cq)
        await callback(cq)
        self.assertEqual(self.tool.versions.await_count, 2)
        self.assertEqual(self.tool.versions.await_args.args[0]["id"], identifier)
        buttons = message.edit_text.await_args.kwargs["reply_markup"].inline_keyboard
        labels = " ".join(button.text for row in buttons for button in row)
        self.assertIn("889112", labels)
        self.assertNotIn("Версия 1", labels)

    async def test_selected_version_metadata_comes_from_apple(self):
        identifier = self.account(1)
        self.tool.run.return_value = {
            "displayVersion": "5.2.1",
            "downloadURL": "https://private.example/file",
        }
        metadata = await self.tool.version_metadata(
            self.db.account(identifier), "123", "889112"
        )
        self.assertEqual(metadata, {"version": "5.2.1"})
        self.assertEqual(
            self.tool.run.await_args.args[1],
            ["get-version-metadata", "-i", "123", "--external-version-id", "889112"],
        )

    async def test_unavailable_response_does_not_guess_account_region(self):
        self.account(1)
        self.tool.versions = AsyncMock(
            side_effect=IpatoolError(
                "no result",
                "Error: app is not available for download in your region/storefront",
            )
        )
        deps = Deps(
            self.cfg,
            self.db,
            SimpleNamespace(),
            self.tool,
            SimpleNamespace(),
            SimpleNamespace(),
        )
        callback = next(
            handler.callback
            for handler in build_router(deps).callback_query.handlers
            if handler.callback.__name__ == "app_versions"
        )
        message = SimpleNamespace(photo=[], edit_text=AsyncMock())
        cq = SimpleNamespace(
            data="ver:123:0",
            from_user=SimpleNamespace(id=1),
            answer=AsyncMock(),
            message=message,
        )
        await callback(cq)
        text = message.edit_text.await_args.args[0]
        self.assertIn("Не удалось получить версии", text)
        self.assertNotIn("стране", text)
        buttons = message.edit_text.await_args.kwargs["reply_markup"].inline_keyboard
        self.assertTrue(
            any(button.callback_data == "ver:123:0" for row in buttons for button in row)
        )

    async def test_selecting_old_version_replaces_current_version_image(self):
        self.account(1)
        self.db.app_save(
            {
                "adam_id": "123",
                "title": "Happ",
                "version": "5.3.0",
                "min_ios": "15.0",
                "size": 123,
            }
        )
        self.tool.version_metadata = AsyncMock(return_value={"version": "4.13.1"})
        cards = SimpleNamespace(
            hero=AsyncMock(return_value=b"old-version-image"),
            file_ids={"app:123": "latest-version-photo"},
        )
        deps = Deps(
            self.cfg, self.db, SimpleNamespace(), self.tool, SimpleNamespace(), cards
        )
        callback = next(
            handler.callback
            for handler in build_router(deps).callback_query.handlers
            if handler.callback.__name__ == "select_version"
        )
        message = SimpleNamespace(
            photo=[True], edit_caption=AsyncMock(), edit_media=AsyncMock()
        )
        cq = SimpleNamespace(
            data="vsel:123:889112",
            from_user=SimpleNamespace(id=1),
            answer=AsyncMock(),
            message=message,
        )
        await callback(cq)
        self.assertEqual(cards.hero.await_args.args[1], ["Версия  ·  4.13.1"])
        media = message.edit_media.await_args.args[0]
        self.assertEqual(media.media.data, b"old-version-image")
        self.assertIn("4.13.1", media.caption)
        self.assertNotIn("5.3.0", media.caption)
        buttons = message.edit_media.await_args.kwargs["reply_markup"].inline_keyboard
        self.assertTrue(
            any(
                button.callback_data == "dl:123:889112"
                for row in buttons
                for button in row
            )
        )
        self.assertEqual(self.db.app("123")["version"], "5.3.0")

    async def test_interval_survives_restart_and_refreshes_at_twelve_hours(
        self,
    ) -> None:
        paid = self.account(1)
        await self.keeper.tick()
        await KbsyncKeeper(self.db, self.tool).tick()
        self.assertEqual(self.tool.run.await_count, 1)
        self.db._run(
            "update accounts set kbsync_warmed_at=? where id=?",
            (now() - 12 * 3600, paid),
        )
        await self.keeper.tick()
        self.assertEqual(self.tool.run.await_count, 2)

    async def test_failure_backoff_and_busy_account(self) -> None:
        paid = self.account(1)
        async with self.tool.lock(paid):
            await self.keeper.tick()
        self.tool.run.assert_not_awaited()
        self.tool.run.side_effect = IpatoolError("failed")
        await self.keeper.tick()
        await self.keeper.tick()
        self.assertEqual(self.tool.run.await_count, 1)
        self.assertEqual(self.db.account(paid)["kbsync_warmed_at"], 0)

    async def test_unconfirmed_result_is_not_marked_warm(self) -> None:
        paid = self.account(1)
        self.tool.run.return_value = {"success": False}
        await self.keeper.tick()
        self.assertEqual(self.db.account(paid)["kbsync_warmed_at"], 0)

    async def test_expired_subscription_and_deleted_account_are_skipped(self) -> None:
        paid = self.account(1)
        stale = self.db.account(paid)
        self.db._run("update users set sub_until=? where tg_id=1", (now() - 1,))
        self.assertFalse(await self.tool.warm_kbsync(stale))
        self.db.account_delete(paid)
        self.assertFalse(await self.tool.warm_kbsync(stale))
        self.tool.run.assert_not_awaited()

    async def test_cpp_flags_and_legacy_purchase_command(self) -> None:
        paid = self.account(1)
        account = self.db.account(paid)
        self.assertNotIn(
            "--non-interactive", self.tool.command(account, ["auth", "info"])
        )
        legacy = Ipatool(replace(self.cfg, ipatool_engine="go"), self.db)
        legacy.run = AsyncMock(return_value={"apps": [{"id": 123}]})
        self.assertIn("--non-interactive", legacy.command(account, ["auth", "info"]))
        self.assertEqual(await legacy.purchases(account), [{"id": 123}])

    async def test_v6_metadata_helper_preserves_account_and_auth_routing(self):
        account = self.db.account(self.account(1))
        tool = Ipatool(replace(self.cfg, ipatool_metadata="store-helper"), self.db)
        for args in (["list-versions", "-i", "123"],
                     ["get-version-metadata", "-i", "123", "--external-version-id", "456"]):
            command = tool.command(account, args)
            self.assertEqual(command[0], "store-helper")
            self.assertEqual(command[-len(args):], args)
            self.assertEqual(command[command.index("--keychain-passphrase") + 1], account["keychain"])
        self.assertEqual(tool.command(account, ["auth", "info"])[0], self.cfg.ipatool)
        self.assertEqual(tool.command(account, ["download", "-i", "123"])[0], self.cfg.ipatool)
        legacy = Ipatool(replace(tool.cfg, ipatool_engine="go"), self.db)
        self.assertEqual(legacy.command(account, ["list-versions", "-i", "123"])[0], self.cfg.ipatool)

    async def test_bundle_lookup_uses_country_fallback(self) -> None:
        store = Store(self.cfg)
        store._get = AsyncMock(
            side_effect=[
                {"results": []},
                {"results": [{"trackId": 123, "bundleId": "com.test.app"}]},
            ]
        )
        results = await store.search("com.test.app")
        self.assertEqual(results[0]["adam_id"], "123")
        self.assertEqual(
            store._get.call_args_list[0].args[1]["bundleId"], "com.test.app"
        )
        self.assertEqual(store._get.call_args_list[1].args[1]["country"], "us")

    async def test_photo_navigation_and_version_delivery_buttons(self) -> None:
        deps = Deps(
            self.cfg,
            self.db,
            Store(self.cfg),
            self.tool,
            FileSender(self.cfg),
            Cards(self.cfg),
        )
        router = build_router(deps)
        callbacks = {
            item.callback.__name__: item.callback
            for item in router.callback_query.handlers
        }
        message = SimpleNamespace(
            photo=[True],
            edit_caption=AsyncMock(),
            edit_text=AsyncMock(),
            edit_media=AsyncMock(),
        )
        cq = SimpleNamespace(
            message=message,
            answer=AsyncMock(),
            from_user=SimpleNamespace(id=10, first_name="Test"),
        )
        await callbacks["how_cb"](cq)
        message.edit_caption.assert_awaited_once()
        message.edit_text.assert_not_awaited()
        await callbacks["menu_cb"](cq, AsyncMock())
        message.edit_media.assert_awaited_once()
        kb = ui.app_kb("123", version="456", back_to="ver:123:0")
        actions = [button.callback_data for row in kb.inline_keyboard for button in row]
        self.assertIn("dl:123:456", actions)
        self.assertIn("ota:123:456", actions)
        main_actions = [
            b.callback_data
            for row in ui.main_menu(False, purchases=False).inline_keyboard
            for b in row
        ]
        self.assertNotIn("pur:1", main_actions)
        self.assertIn("cat", main_actions)

    async def test_apple_id_wizard_edits_photo_caption(self) -> None:
        deps = Deps(
            self.cfg,
            self.db,
            Store(self.cfg),
            self.tool,
            FileSender(self.cfg),
            Cards(self.cfg),
        )
        router = build_router(deps)
        callbacks = {
            item.callback.__name__: item.callback
            for item in router.callback_query.handlers
        }
        messages = {
            item.callback.__name__: item.callback for item in router.message.handlers
        }
        state = AsyncMock()
        message = SimpleNamespace(
            photo=[True],
            edit_caption=AsyncMock(),
            chat=SimpleNamespace(id=1),
            message_id=2,
        )
        await callbacks["account_add_start"](
            SimpleNamespace(message=message, answer=AsyncMock()), state
        )
        self.assertTrue(state.update_data.call_args.kwargs["note_photo"])
        state.get_data.return_value = {"chat": 1, "note": 2, "note_photo": True}
        bot = SimpleNamespace(
            edit_message_caption=AsyncMock(), edit_message_text=AsyncMock()
        )
        await messages["account_add_email"](
            SimpleNamespace(text="test@example.com", delete=AsyncMock(), bot=bot), state
        )
        bot.edit_message_caption.assert_awaited_once()
        bot.edit_message_text.assert_not_awaited()

    async def test_login_resets_stamp_and_paywall_keeps_only_user_benefits(
        self,
    ) -> None:
        paid = self.account(1)
        self.db.kbsync_warmed(paid)
        await self.tool.login(self.db.account(paid))
        self.assertEqual(self.db.account(paid)["kbsync_warmed_at"], 0)
        text, _ = ui.paywall(self.cfg, 3, 0, self.db.sub_active(1))
        self.assertNotIn("KBSync", text)
        self.assertIn("Неограниченное количество скачиваний", text)
        self.assertNotIn("каждые 12 часов", text)
        self.assertNotIn("Меньше ожидания", text)
        self.assertNotIn("Premium", text)
        self.assertNotIn("тестовая", text)

    async def test_catalog_callback_filters_and_requires_account(self) -> None:
        self.account(1)
        store = Store(self.cfg)
        rows = [{"app_store_id": str(100 + i), "title": f"Cover {i}"} for i in range(4)]
        store.catalog_app = AsyncMock(
            return_value={"title": "Т-Банк", "category": "banks", "versions": rows}
        )
        self.tool.versions = AsyncMock(side_effect=[["222"], [], [], ["333"]])
        home = self.tool.home(self.db.account_of(1)["id"]) / ".ipatool"
        home.mkdir()
        (home / "account").write_bytes(b"encrypted test account")
        (home / "cookies").write_bytes(b"test cookies" * 10)
        cards = Cards(self.cfg)
        cards.grid = AsyncMock(return_value=b"photo")
        router = build_router(
            Deps(self.cfg, self.db, store, self.tool, FileSender(self.cfg), cards)
        )
        callback = next(
            h.callback
            for h in router.callback_query.handlers
            if h.callback.__name__ == "catalog_app"
        )
        message = SimpleNamespace(
            photo=[True],
            edit_caption=AsyncMock(),
            edit_media=AsyncMock(),
            edit_text=AsyncMock(),
        )
        cq = SimpleNamespace(
            data="ca:tbank",
            message=message,
            answer=AsyncMock(),
            from_user=SimpleNamespace(id=1),
        )
        await callback(cq)
        self.assertEqual(
            [item[1] for item in cards.grid.call_args.args[2]], ["Cover 0", "Cover 3"]
        )
        caption = message.edit_media.call_args.args[0].caption
        self.assertIn("Доступно вам: <b>2</b>", caption)
        self.assertNotIn("Доступных сборок: 4", caption)
        cq.from_user.id = 2
        await callback(cq)
        self.assertIn("Apple ID", message.edit_caption.call_args.kwargs["caption"])
        self.assertEqual(self.tool.versions.await_count, 4)


if __name__ == "__main__":
    unittest.main()
