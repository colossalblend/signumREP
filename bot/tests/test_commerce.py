from __future__ import annotations

import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock
from aiogram.types import InlineKeyboardButton, InlineKeyboardMarkup

from signum_bot import ui
from signum_bot.commerce import activate_demo, create_order, issue_api_key, key_digest
from signum_bot.config import Config
from signum_bot.db import Db, now
from signum_bot.theme import Theme
from signum_bot.handlers import Deps, build_router
from signum_bot.ipatool import Ipatool


class CommerceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cfg = replace(
            Config.load(),
            data_dir=Path(self.tmp.name),
            payment_mode="demo",
            production=False,
        )
        self.db = Db(self.cfg.db_path)
        self.addCleanup(self.db.conn.close)
        self.db.user(1)
        self.db.user(2)

    def test_ruble_pricing_and_demo_paywall(self):
        self.assertEqual(self.cfg.currency, "RUB")
        self.assertEqual(self.cfg.amount("month"), self.cfg.prices["month"] * 100)
        text, markup = ui.paywall(self.cfg, 3, 0, 0)
        labels = " ".join(
            button.text for row in markup.inline_keyboard for button in row
        )
        self.assertIn("₽", labels)
        self.assertNotIn("⭐", labels)
        self.assertNotIn("Демо-оплата", text)
        self.assertNotIn("каждые 12 часов", text)

    def test_demo_order_only_activates_once_and_excludes_revenue(self):
        order = create_order(self.cfg, self.db, 1, "single")
        self.assertEqual(activate_demo(self.db, 2, order), "invalid")
        self.assertEqual(activate_demo(self.db, 1, order), "activated")
        self.assertEqual(activate_demo(self.db, 1, order), "complete")
        self.assertEqual(self.db.credits(1), 1)
        self.assertEqual(self.db.stats()["revenue"], 0)
        self.assertEqual(self.db.stats()["demo_payments"], 1)

    def test_expired_blocked_and_non_demo_orders_do_not_activate(self):
        order = create_order(self.cfg, self.db, 1, "month")
        self.db._run("update orders set expires_at=0 where id=?", (order,))
        self.assertEqual(activate_demo(self.db, 1, order), "expired")
        order = create_order(self.cfg, self.db, 1, "month")
        self.db.set_blocked(1, True)
        self.assertEqual(activate_demo(self.db, 1, order), "invalid")
        self.db.set_blocked(1, False)
        self.db._run("update orders set mode='disabled' where id=?", (order,))
        self.assertEqual(activate_demo(self.db, 1, order), "invalid")
        self.assertEqual(self.db.sub_active(1), 0)

    def test_subscription_renewal_extends_remaining_days(self):
        until = self.db.extend_sub(1, 7)
        order = create_order(self.cfg, self.db, 1, "month")
        self.assertEqual(activate_demo(self.db, 1, order), "activated")
        self.assertEqual(self.db.sub_active(1), until + 30 * 86400)

    def test_api_access_is_separate_and_snapshots_quota(self):
        order = create_order(replace(self.cfg, api_downloads=42), self.db, 1, "api")
        self.assertEqual(activate_demo(self.db, 1, order), "activated")
        user = self.db.one("select * from users where tg_id=1")
        self.assertEqual(user["api_remaining"], 42)
        self.assertGreater(user["api_until"], now())
        self.assertEqual(self.db.sub_active(1), 0)
        previous = user["api_until"]
        order = create_order(self.cfg, self.db, 1, "api")
        activate_demo(self.db, 1, order)
        user = self.db.one("select * from users where tg_id=1")
        self.assertEqual(user["api_until"], previous + 30 * 86400)
        self.assertEqual(user["api_remaining"], 42 + self.cfg.api_downloads)

    def test_api_purchase_after_expiry_resets_unused_allowance(self):
        self.db._run("update users set api_until=1,api_remaining=42 where tg_id=1")
        activate_demo(self.db, 1, create_order(self.cfg, self.db, 1, "api"))
        user = self.db.one("select * from users where tg_id=1")
        self.assertEqual(user["api_remaining"], self.cfg.api_downloads)

    def test_api_keys_are_hashed_and_rotation_replaces_old_key(self):
        first = issue_api_key(self.db, 1)
        second = issue_api_key(self.db, 1)
        self.assertNotEqual(first, second)
        rows = self.db.all("select * from api_keys")
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["digest"], key_digest(second))
        self.assertNotIn(first, tuple(rows[0]))
        self.assertNotIn(second, tuple(rows[0]))

    def test_topup_does_not_extend_access_and_carries_over_after_expiry(self):
        activate_demo(self.db, 1, create_order(self.cfg, self.db, 1, "api"))
        until = self.db.one("select api_until from users where tg_id=1")[0]
        order = create_order(self.cfg, self.db, 1, "api500")
        self.assertEqual(activate_demo(self.db, 1, order), "activated")
        self.assertEqual(activate_demo(self.db, 1, order), "complete")
        user = self.db.one("select * from users where tg_id=1")
        self.assertEqual(user["api_until"], until)
        self.assertEqual(user["api_extra_remaining"], 500)
        self.db._run("update users set api_until=1 where tg_id=1")
        activate_demo(self.db, 1, create_order(self.cfg, self.db, 1, "api"))
        user = self.db.one("select * from users where tg_id=1")
        self.assertEqual(user["api_extra_remaining"], 500)
        self.assertEqual(user["api_remaining"], 1000)

    def test_topup_requires_active_api(self):
        with self.assertRaises(ValueError):
            create_order(self.cfg, self.db, 1, "api100")

    def test_install_screen_has_selected_status_app_and_size_icons(self):
        theme = Theme({})
        text = theme.text(ui.ready_ota("SpotLuna", 1234, 12, "test@example.com", True))
        for identifier in (
            "5237699328843200968",
            "5409357944619802453",
            "5215684705168401226",
        ):
            self.assertIn(f'emoji-id="{identifier}"', text)
        self.assertIn("Ссылка живёт 12 ч.", text)
        markup = InlineKeyboardMarkup(
            inline_keyboard=[
                [
                    InlineKeyboardButton(
                        text="Установить", url="https://example.com/i/token"
                    )
                ]
            ]
        )
        button = theme.keyboard(markup).inline_keyboard[0][0]
        self.assertEqual(button.icon_custom_emoji_id, "4967477144973214631")
        self.assertEqual(button.text, "Установить")
        self.assertEqual(button.url, "https://example.com/i/token")
        self.assertEqual(button.style, "success")


class CheckoutHandlerTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cfg = replace(
            Config.load(),
            data_dir=Path(self.tmp.name),
            payment_mode="demo",
            production=False,
        )
        self.db = Db(self.cfg.db_path)
        self.addCleanup(self.db.conn.close)
        self.db.user(1)
        router = build_router(
            Deps(
                self.cfg,
                self.db,
                SimpleNamespace(),
                Ipatool(self.cfg, self.db),
                SimpleNamespace(),
                SimpleNamespace(),
            )
        )
        self.callbacks = {
            h.callback.__name__: h.callback for h in router.callback_query.handlers
        }
        self.message = SimpleNamespace(
            photo=[], edit_text=AsyncMock(), answer=AsyncMock()
        )
        self.cq = SimpleNamespace(
            data="pay:month",
            from_user=SimpleNamespace(id=1, username="test", full_name="Test"),
            message=self.message,
            answer=AsyncMock(),
            bot=SimpleNamespace(send_invoice=AsyncMock()),
        )

    async def test_checkout_never_sends_invoice_and_activates_only_on_confirmation(
        self,
    ):
        await self.callbacks["pay"](self.cq)
        self.cq.bot.send_invoice.assert_not_awaited()
        self.assertEqual(self.db.sub_active(1), 0)
        text = self.message.edit_text.await_args.args[0]
        self.assertIn("Проверка покупки", text)
        self.assertIn("Деньги не списываются", text)
        self.assertIn("₽", text)
        button = self.message.edit_text.await_args.kwargs[
            "reply_markup"
        ].inline_keyboard[0][0]
        self.cq.data = button.callback_data
        await self.callbacks["demo_pay"](self.cq)
        self.assertGreater(self.db.sub_active(1), now())
        until = self.db.sub_active(1)
        await self.callbacks["demo_pay"](self.cq)
        self.assertEqual(self.db.sub_active(1), until)

    async def test_api_demo_purchase_only_opens_api(self):
        self.cq.data = "pay:api"
        await self.callbacks["pay"](self.cq)
        button = self.message.edit_text.await_args.kwargs[
            "reply_markup"
        ].inline_keyboard[0][0]
        self.cq.data = button.callback_data
        await self.callbacks["demo_pay"](self.cq)
        self.assertEqual(self.db.sub_active(1), 0)
        self.assertIn("Signum API", self.message.edit_text.await_args.args[0])
        user = self.db.one("select * from users where tg_id=1")
        self.assertGreater(user["api_until"], now())
        self.assertEqual(user["api_remaining"], self.cfg.api_downloads)


if __name__ == "__main__":
    unittest.main()
