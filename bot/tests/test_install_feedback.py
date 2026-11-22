from __future__ import annotations

import hashlib
import hmac
import json
import os
import secrets
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from aiohttp import web
from aiohttp.test_utils import TestClient, TestServer

from signum_bot.config import Config
from signum_bot.db import Db, now
from signum_bot.install_feedback import (
    build_feedback_router,
    record_event,
    register_events,
)


class InstallationTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cfg = replace(
            Config.load(),
            data_dir=Path(self.tmp.name),
            admins={99},
            bridge_enabled=False,
        )
        self.db = Db(self.cfg.db_path)
        self.addCleanup(self.db.conn.close)
        self.token = secrets.token_urlsafe(18)
        self.path = self.cfg.files_dir / self.token / "example.ipa"
        self.path.parent.mkdir(parents=True)
        self.path.write_bytes(b"content")
        self.db.file_add(
            self.token,
            tg_id=1,
            path=str(self.path),
            title="Example",
            expires_at=now() + 3600,
        )
        self.event = {
            "id": secrets.token_hex(16),
            "token": self.token,
            "kind": "iphone_download_finished",
            "tg_id": 2,
        }
        self.callbacks = {
            handler.callback.__name__: handler.callback
            for handler in build_feedback_router(
                self.cfg, self.db
            ).callback_query.handlers
        }
        self.state = SimpleNamespace(
            clear=AsyncMock(), get_state=AsyncMock(return_value=None)
        )

    def cq(self, actor, action):
        return SimpleNamespace(
            data=f"ins:{self.token}:{action}",
            from_user=SimpleNamespace(id=actor),
            answer=AsyncMock(),
            message=SimpleNamespace(edit_text=AsyncMock(), answer=AsyncMock()),
            bot=SimpleNamespace(send_message=AsyncMock()),
        )

    def prompt(self):
        record_event(self.db, self.event)
        self.db._run("update installation_feedback set status='prompted'")

    async def test_event_uses_file_owner_and_retries_are_idempotent(self):
        record_event(self.db, self.event)
        row = self.db.one("select * from installation_feedback")
        self.assertEqual(row["tg_id"], 1)
        due = row["due_at"]
        with patch("signum_bot.install_feedback.now", return_value=now() + 20):
            record_event(self.db, self.event)
        self.assertEqual(
            self.db.one("select due_at from installation_feedback")[0], due
        )

    async def test_confirmation_cannot_delete_another_owners_file(self):
        self.prompt()
        cq = self.cq(2, "yes")
        await self.callbacks["answer"](cq, self.state)
        self.assertTrue(self.path.exists())
        self.assertIsNotNone(self.db.file(self.token))
        cq.message.edit_text.assert_not_awaited()

    async def test_owner_confirmation_removes_file_and_revokes_link(self):
        self.prompt()
        await self.callbacks["answer"](self.cq(1, "yes"), self.state)
        self.assertFalse(self.path.exists())
        self.assertIsNone(self.db.file(self.token))
        self.assertEqual(
            self.db.one("select status from installation_feedback")[0], "installed"
        )

    async def test_report_goes_to_admin_only_once(self):
        self.prompt()
        cq = self.cq(1, "issue")
        await self.callbacks["answer"](cq, self.state)
        cq.data = f"inr:{self.token}:0"
        await self.callbacks["choose_reason"](cq, self.state)
        await self.callbacks["choose_reason"](cq, self.state)
        cq.bot.send_message.assert_awaited_once()
        self.assertEqual(cq.bot.send_message.await_args.args[0], 99)
        self.assertIn("Не удалось установить", cq.bot.send_message.await_args.args[1])

    async def test_custom_reason_is_escaped_and_sent_to_admin(self):
        self.prompt()
        router = build_feedback_router(self.cfg, self.db)
        handler = router.message.handlers[0].callback
        state = SimpleNamespace(
            get_data=AsyncMock(return_value={"installation_token": self.token}),
            clear=AsyncMock(),
        )
        message = SimpleNamespace(
            from_user=SimpleNamespace(id=1),
            text="  Ошибка <test> & сбой  ",
            bot=SimpleNamespace(send_message=AsyncMock()),
            answer=AsyncMock(),
        )
        await handler(message, state)
        report = message.bot.send_message.await_args.args
        self.assertEqual(report[0], 99)
        self.assertIn("&lt;test&gt; &amp;", report[1])
        self.assertEqual(
            self.db.one("select status from installation_feedback")[0], "reported"
        )
        state.clear.assert_awaited_once()

    async def test_unsigned_event_is_rejected_and_signed_event_is_accepted(self):
        key = bytes(range(32))
        app = web.Application()
        register_events(app, self.cfg, self.db)
        with patch.dict(os.environ, {"BRIDGE_KEY": key.hex()}):
            async with TestClient(TestServer(app)) as client:
                event = {**self.event, "time": now()}
                body = json.dumps(event).encode()
                response = await client.post("/internal/install-events", data=body)
                self.assertEqual(response.status, 403)
                response = await client.post(
                    "/internal/install-events",
                    data=body,
                    headers={
                        "X-Signum-Signature": hmac.new(
                            key, body, hashlib.sha256
                        ).hexdigest()
                    },
                )
                self.assertEqual(response.status, 200)
                self.assertEqual(
                    self.db.one("select tg_id from installation_feedback")[0], 1
                )
