from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

from signum_bot.commerce import create_order, fulfill_order
from signum_bot.config import Config
from signum_bot.db import Db, now
from signum_bot.payment_notifications import deliver_pending


class NotificationTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.db = Db(Path(self.tmp.name) / "notifications.db")
        self.cfg = SimpleNamespace(admins={99, 100})
        self.bot = SimpleNamespace(send_message=AsyncMock())
        self.db.user(1, "customer", "<b>Имя</b>")
        cfg = Config.load()
        identifier = create_order(cfg, self.db, 1, "week")
        order = self.db.one("select * from orders where id=?", (identifier,))
        fulfill_order(self.db, order, "real", "charge-fixture", notify_admins=self.cfg.admins)

    def tearDown(self):
        self.db.conn.close()
        self.tmp.cleanup()

    async def test_each_admin_receives_purchase_once_with_escaped_customer(self):
        await deliver_pending(self.bot, self.cfg, self.db)
        await deliver_pending(self.bot, self.cfg, self.db)
        self.assertEqual(self.bot.send_message.await_count, 2)
        recipients = {call.args[0] for call in self.bot.send_message.await_args_list}
        self.assertEqual(recipients, self.cfg.admins)
        text = self.bot.send_message.await_args.args[1]
        self.assertIn("&lt;b&gt;Имя&lt;/b&gt;", text)
        self.assertIn("Подписка на неделю", text)
        self.assertIn("@customer", text)
        self.assertIn("<code>1</code>", text)

    async def test_failed_delivery_survives_db_reopen_and_does_not_block_other_admin(self):
        self.bot.send_message.side_effect = [RuntimeError("temporary"), None]
        await deliver_pending(self.bot, self.cfg, self.db)
        pending = self.db.one("select * from payment_notifications where delivered_at=0")
        self.assertIsNotNone(pending)
        self.assertGreater(pending["next_attempt"], now())
        path = Path(self.tmp.name) / "notifications.db"
        self.db.conn.close()
        self.db = Db(path)
        self.db._run("update payment_notifications set next_attempt=0 where delivered_at=0")
        self.bot.send_message.reset_mock(side_effect=True)
        await deliver_pending(self.bot, self.cfg, self.db)
        self.bot.send_message.assert_awaited_once()
        self.assertEqual(self.bot.send_message.await_args.args[0], pending["admin_id"])

    async def test_removed_admin_never_receives_purchase_details(self):
        self.cfg.admins = {99}
        await deliver_pending(self.bot, self.cfg, self.db)
        self.bot.send_message.assert_awaited_once()
        self.assertEqual(self.bot.send_message.await_args.args[0], 99)
