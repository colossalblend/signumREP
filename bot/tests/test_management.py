from __future__ import annotations

import os
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from aiogram.fsm.context import FSMContext
from aiogram.fsm.storage.base import StorageKey
from aiogram.fsm.storage.memory import MemoryStorage

from signum_bot.api_keys import create_key
from signum_bot.config import Config
from signum_bot.db import Db, now
from signum_bot.management import EditApi, build_management


class ManagementTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.cfg = replace(Config.load(), data_dir=Path(self.tmp.name), admins={99})
        self.db = Db(self.cfg.db_path)
        for owner in (1, 2, 99):
            self.db.user(owner)
            self.db._run(
                "update users set api_until=?,api_remaining=1000 where tg_id=?",
                (now() + 86400, owner),
            )
        self.editor = AsyncMock()
        router = build_management(
            SimpleNamespace(cfg=self.cfg, db=self.db), self.editor
        )
        self.callbacks = {
            item.callback.__name__: item.callback
            for item in router.callback_query.handlers
        }
        self.messages = {
            item.callback.__name__: item.callback for item in router.message.handlers
        }
        self.storage = MemoryStorage()
        self.state = FSMContext(
            self.storage, StorageKey(bot_id=1, chat_id=1, user_id=1)
        )
        self.message = SimpleNamespace(text="", answer=AsyncMock())
        self.cq = SimpleNamespace(
            from_user=SimpleNamespace(id=1),
            data="",
            answer=AsyncMock(),
            message=self.message,
        )

    async def asyncTearDown(self):
        await self.storage.close()
        self.db.conn.close()
        self.tmp.cleanup()

    async def test_customer_cannot_open_other_keys_or_change_client_balance(self):
        self.cq.data = "kp:2:0"
        await self.callbacks["key_page"](self.cq, self.state)
        self.editor.assert_not_awaited()
        self.cq.data = "ap:2"
        await self.callbacks["pause_client"](self.cq)
        self.assertEqual(
            self.db.one("select api_enabled from users where tg_id=2")[0], 1
        )
        await self.state.set_state(EditApi.quota)
        await self.state.update_data(owner=2)
        self.message.from_user = SimpleNamespace(id=1)
        self.message.text = "9000 9000"
        await self.messages["save_client_number"](self.message, self.state)
        self.assertEqual(
            self.db.one("select api_remaining from users where tg_id=2")[0], 1000
        )

    async def test_admin_can_set_balance_pause_and_change_key_quota_with_audit(self):
        self.cq.from_user.id = 99
        self.message.from_user = SimpleNamespace(id=99)
        self.cq.data = "aq:2"
        await self.callbacks["client_number"](self.cq, self.state)
        self.message.text = "500 250"
        await self.messages["save_client_number"](self.message, self.state)
        self.assertEqual(
            tuple(
                self.db.one(
                    "select api_remaining,api_extra_remaining from users where tg_id=2"
                )
            ),
            (500, 250),
        )
        self.cq.data = "ap:2"
        await self.callbacks["pause_client"](self.cq)
        self.assertEqual(
            self.db.one("select api_enabled from users where tg_id=2")[0], 0
        )
        identifier, _ = create_key(self.db, 2, "Website", expires_at=now() + 3600)
        self.cq.data = "kq:" + identifier
        await self.callbacks["key_number"](self.cq, self.state)
        self.message.text = "42"
        await self.messages["save_key_number"](self.message, self.state)
        self.assertEqual(
            self.db.one(
                "select quota_remaining from api_keys where id=?", (identifier,)
            )[0],
            42,
        )
        actions = [row[0] for row in self.db.all("select action from api_audit")]
        self.assertIn("api_quota_changed", actions)
        self.assertIn("api_pause_changed", actions)
        self.assertIn("key_updated:quota_remaining", actions)

    async def test_cancel_key_dialog_clears_state(self):
        identifier, _ = create_key(self.db, 1, "Website", expires_at=now() + 3600)
        await self.state.set_state(EditApi.key_quota)
        self.cq.data = "ki:" + identifier
        await self.callbacks["key_card"](self.cq, self.state)
        self.assertIsNone(await self.state.get_state())

    async def test_revocation_needs_confirmation_and_is_permanent(self):
        identifier, _ = create_key(self.db, 1, "Website", expires_at=now() + 3600)
        self.cq.data = "kr:" + identifier
        await self.callbacks["confirm_revoke"](self.cq)
        self.assertEqual(
            self.db.one("select state from api_keys where id=?", (identifier,))[0],
            "active",
        )
        self.cq.data = "kx:" + identifier
        await self.callbacks["revoke"](self.cq)
        self.assertEqual(
            self.db.one("select state from api_keys where id=?", (identifier,))[0],
            "revoked",
        )

    def test_config_allows_standalone_api_without_bot_token(self):
        with patch.dict(os.environ, {"BOT_TOKEN": ""}):
            self.assertEqual(Config.load(require_bot_token=False).token, "")
            with self.assertRaises(SystemExit):
                Config.load()


if __name__ == "__main__":
    unittest.main()
