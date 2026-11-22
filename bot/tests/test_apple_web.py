from __future__ import annotations

import json
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock, patch

from aiogram.fsm.context import FSMContext
from aiogram.fsm.storage.base import StorageKey
from aiogram.fsm.storage.memory import MemoryStorage

from signum_bot.apple_screens import AppleAction, build_apple_screens
from signum_bot.apple_web import AppleWeb, Worker
from signum_bot.config import Config
from signum_bot.db import Db, now
from signum_bot.ipatool import Ipatool


class AppleFixture(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.cfg = replace(Config.load(), data_dir=Path(self.tmp.name))
        self.db = Db(self.cfg.db_path)
        self.db.user(1)
        self.db.user(2)
        self.identifier = self.db.account_add(
            1, "demo@example.com", "password", "keychain"
        )
        self.db.account_status(self.identifier, "ready")
        self.account = self.db.account(self.identifier)
        self.service = AppleWeb(self.cfg, self.db)

    async def asyncTearDown(self):
        await self.service.close()
        self.db.conn.close()
        self.tmp.cleanup()


class AppleWebTests(AppleFixture):
    async def test_bridge_regions_and_addresses_without_network_or_credentials(self):
        regions = await self.service.regions()
        self.assertGreater(len(regions), 20)
        self.assertIn("USA", {row["code"] for row in regions})
        addresses = await self.service.request(None, "addresses", country="USA")
        self.assertTrue(addresses["addresses"])
        address = addresses["addresses"][0]
        self.assertEqual(address["countryCode"], "USA")
        self.assertTrue(address["postalCode"])

    async def test_sessions_encrypted_and_removed_when_account_disconnects(self):
        proc = SimpleNamespace(
            returncode=0,
            stdin=SimpleNamespace(write=Mock(), drain=AsyncMock()),
            stdout=SimpleNamespace(
                readline=AsyncMock(
                    return_value=json.dumps(
                        {
                            "status": "requires_2fa",
                            "state": {"history_cookies": {"myacinfo": "secret-cookie"}},
                        }
                    ).encode()
                    + b"\n"
                )
            ),
        )
        worker = Worker(proc, 0)
        with patch.object(self.service, "worker", AsyncMock(return_value=worker)):
            result = await self.service.request(self.account, "history")
        self.assertNotIn("state", result)
        row = self.db.one(
            "select * from apple_web_sessions where account_id=?", (self.identifier,)
        )
        self.assertNotIn("secret-cookie", row["session_secret"])
        self.assertIn(
            "secret-cookie", self.service.vault.decrypt(row["session_secret"])
        )
        self.db.account_delete(self.identifier)
        self.assertIsNone(
            self.db.one(
                "select * from apple_web_sessions where account_id=?",
                (self.identifier,),
            )
        )

    async def test_cached_history_pagination_deduplication_and_generation_isolation(
        self,
    ):
        self.db._run(
            "insert into apple_web_sessions(account_id,generation) values(?,0)",
            (self.identifier,),
        )
        responses = [
            {
                "status": "ready",
                "apps": [{"id": str(i), "name": f"App {i}"} for i in range(1, 10)],
                "has_more": True,
            },
            {
                "status": "ready",
                "apps": [{"id": "9", "name": "Duplicate"}]
                + [{"id": str(i), "name": f"App {i}"} for i in range(10, 14)],
                "has_more": False,
            },
        ]
        with patch.object(
            self.service, "request", AsyncMock(side_effect=responses)
        ) as request:
            first = await self.service.history(self.account)
            second = await self.service.history(self.account, page=2)
            self.assertEqual(request.await_count, 2)
            self.assertEqual(second["apps"][0]["id"], "9")
            self.assertFalse(second["has_more"])
            self.assertEqual(
                [row["id"] for row in second["apps"]], ["9", "10", "11", "12", "13"]
            )
            third = await self.service.history(self.account, page=3)
            self.assertFalse(third["has_more"])
            self.assertEqual(len(self.service.history_cache(self.account)["apps"]), 13)
            self.assertEqual(request.await_args.args[1], "history_next")
            self.assertEqual(len(first["apps"]), 8)
        self.db._run(
            "update accounts set session_generation=1 where id=?", (self.identifier,)
        )
        self.assertFalse(
            self.service.history_cache(self.db.account(self.identifier))["loaded"]
        )

    async def test_cache_expiry_does_not_extend_when_region_is_viewed(self):
        data = {
            "apps": [],
            "has_more": False,
            "loaded": True,
            "cached_at": now() - 4000,
        }
        self.db._run(
            "insert into apple_web_sessions(account_id,generation,history_secret,updated_at) values(?,0,?,?)",
            (self.identifier, self.service.vault.encrypt(json.dumps(data)), now()),
        )
        self.assertFalse(self.service.history_cache(self.account)["loaded"])


class AppleScreenTests(AppleFixture):
    async def asyncSetUp(self):
        await super().asyncSetUp()
        self.editor = AsyncMock()
        self.apple = SimpleNamespace(
            history=AsyncMock(),
            request=AsyncMock(),
            regions=AsyncMock(return_value=[{"code": "USA", "name": "США"}]),
            stop_worker=AsyncMock(),
        )
        self.deps = SimpleNamespace(
            cfg=self.cfg, db=self.db, tool=Ipatool(self.cfg, self.db), apple=self.apple
        )
        self.router = build_apple_screens(self.deps, self.editor)
        self.callbacks = {
            row.callback.__name__: row.callback
            for row in self.router.callback_query.handlers
        }
        self.messages = {
            row.callback.__name__: row.callback for row in self.router.message.handlers
        }
        self.storage = MemoryStorage()
        self.state = FSMContext(
            self.storage, StorageKey(bot_id=1, chat_id=1, user_id=1)
        )
        self.note = SimpleNamespace(text="", delete=AsyncMock(), answer=AsyncMock())
        self.note.answer.return_value = self.note
        self.cq = SimpleNamespace(
            data="pur:1",
            from_user=SimpleNamespace(id=1),
            message=self.note,
            answer=AsyncMock(),
        )
        self.note.from_user = self.cq.from_user

    async def asyncTearDown(self):
        await self.storage.close()
        await super().asyncTearDown()

    async def test_history_2fa_then_ready_without_storing_code(self):
        self.apple.history.side_effect = [
            {"status": "requires_2fa"},
            {
                "status": "ready",
                "apps": [
                    {
                        "id": "12345678",
                        "name": "<App>",
                        "date": "2026-10-02",
                        "price": "Бесплатно",
                    }
                ],
                "has_more": False,
                "page": 1,
            },
        ]
        await self.callbacks["purchases"](self.cq, self.state)
        self.assertEqual(await self.state.get_state(), AppleAction.code.state)
        self.note.text = "123456"
        await self.messages["verify_code"](self.note, self.state)
        self.assertIsNone(await self.state.get_state())
        self.assertNotIn("123456", json.dumps(await self.state.get_data()))
        self.assertIn("&lt;App&gt;", self.editor.await_args.args[1])
        self.assertEqual(self.db.app("12345678")["title"], "<App>")
        self.note.delete.assert_awaited_once()

    async def test_ready_history_does_not_request_code(self):
        self.apple.history.return_value = {
            "status": "ready",
            "apps": [],
            "has_more": False,
            "page": 1,
        }
        await self.callbacks["purchases"](self.cq, self.state)
        self.assertIsNone(await self.state.get_state())
        self.assertNotIn("Введите код", self.editor.await_args.args[1])

    async def test_region_requires_valid_confirmation_and_exact_owner(self):
        address = {
            "countryCode": "USA",
            "countryName": "США",
            "street": "Street",
            "city": "City",
            "postalCode": "10001",
            "phoneArea": "212",
            "phoneNumber": "5550100",
        }
        self.apple.request.return_value = {"status": "ready", "addresses": [address]}
        self.cq.data = "awc:USA"
        await self.callbacks["choose_country"](self.cq, self.state)
        self.assertEqual(await self.state.get_state(), AppleAction.confirm.state)
        self.apple.request.reset_mock()
        self.cq.data = "awapply:forged"
        await self.callbacks["apply_country"](self.cq, self.state)
        self.apple.request.assert_not_awaited()
        data = await self.state.get_data()
        self.cq.data = "awapply:" + data["confirm_token"]
        self.cq.from_user.id = 2
        await self.callbacks["apply_country"](self.cq, self.state)
        self.apple.request.assert_not_awaited()
        self.cq.from_user.id = 1
        self.apple.request.return_value = {
            "status": "ready",
            "region": {"countryName": "США", "countryCode": "USA"},
        }
        await self.callbacks["apply_country"](self.cq, self.state)
        self.assertEqual(self.apple.request.await_args.args[1], "region_change")
        self.assertIsNone(await self.state.get_state())

    async def test_three_invalid_codes_cancel_job_and_expired_job_never_sends_code(
        self,
    ):
        self.apple.history.return_value = {"status": "requires_2fa"}
        await self.callbacks["purchases"](self.cq, self.state)
        self.note.text = "123456"
        for _ in range(3):
            await self.messages["verify_code"](self.note, self.state)
        self.assertIsNone(await self.state.get_state())
        self.apple.stop_worker.assert_awaited_once()
        await self.state.set_state(AppleAction.code)
        await self.state.update_data(account_id=self.identifier, expires_at=now() - 1)
        self.apple.history.reset_mock()
        await self.messages["verify_code"](self.note, self.state)
        self.apple.history.assert_not_awaited()

    async def test_parent_router_delivers_code_to_child_instead_of_search(self):
        from signum_bot.handlers import build_router

        self.deps.store = SimpleNamespace()
        self.note.photo = []
        self.note.edit_text = AsyncMock()
        root = build_router(self.deps)
        self.apple.history.return_value = {
            "status": "ready",
            "apps": [],
            "has_more": False,
            "page": 1,
        }
        await self.state.set_state(AppleAction.code)
        await self.state.update_data(
            account_id=self.identifier,
            action="history",
            fields={"page": 1},
            expires_at=now() + 600,
        )
        self.note.text = "123456"
        self.note.chat = SimpleNamespace(type="private", id=1)
        self.note.entities = []
        await root.propagate_event(
            "message",
            self.note,
            state=self.state,
            raw_state=AppleAction.code.state,
            bot=SimpleNamespace(),
        )
        self.apple.history.assert_awaited_once()
