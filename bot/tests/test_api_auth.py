from __future__ import annotations

import asyncio
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

from aiohttp.test_utils import TestClient, TestServer

from signum_bot import ota
from signum_bot.api_keys import create_key, update_key
from signum_bot.commerce import issue_api_key
from signum_bot.config import Config
from signum_bot.customer_api import CustomerApi
from signum_bot.db import Db, now
from signum_bot.ipatool import Ipatool, IpatoolError


class AppleApiFixture(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.cfg = replace(
            Config.load(),
            bridge_enabled=False,
            api_trusted_proxies=(),
            public_base_url="https://example.com",
            data_dir=Path(self.tmp.name),
            api_rate_limit=100,
            api_accounts_max=3,
        )
        self.db = Db(self.cfg.db_path)
        for owner in (1, 2):
            self.db.user(owner)
            self.db._run(
                "update users set api_until=?,api_remaining=10 where tg_id=?",
                (now() + 86400, owner),
            )
        self.tool = Ipatool(self.cfg, self.db)
        self.gate = asyncio.Event()

        async def login(account, code=None):
            if account["password"] == "slow":
                await self.gate.wait()
            if account["password"] == "twofactor" and code != "123456":
                raise IpatoolError("Two-factor verification code required")
            if account["password"] == "bad":
                raise IpatoolError("secret-password user@secret.example login failed")

        self.tool.login = AsyncMock(side_effect=login)
        self.tool.revalidate = AsyncMock()
        self.tool.versions = AsyncMock(return_value=["456"])
        self.api = CustomerApi(
            SimpleNamespace(
                cfg=self.cfg, db=self.db, tool=self.tool, store=SimpleNamespace()
            )
        )
        self.client = TestClient(TestServer(ota.build_app(self.cfg, self.db, self.api)))
        await self.client.start_server()
        self.headers = {
            "Authorization": "Bearer " + issue_api_key(self.db, 1),
            "Idempotency-Key": "login-test-001",
        }
        self.other = {
            "Authorization": "Bearer " + issue_api_key(self.db, 2),
            "Idempotency-Key": "login-test-001",
        }

    async def asyncTearDown(self):
        await self.client.close()
        self.db.conn.close()
        self.tmp.cleanup()

    async def login(self, password="normal", email="first@example.com", headers=None):
        return await self.client.post(
            "/api/v1/auth/jobs",
            json={"email": email, "password": password},
            headers=headers or self.headers,
        )

    async def drain(self):
        tasks = list(self.api.auth.tasks.values())
        await asyncio.wait_for(asyncio.gather(*tasks), timeout=2)

    async def status(self, job_id, headers=None):
        return await self.client.get(
            "/api/v1/auth/jobs/" + job_id, headers=headers or self.headers
        )

    async def code(self, job_id, challenge, code="123456", headers=None):
        return await self.client.post(
            "/api/v1/auth/jobs/" + job_id + "/code",
            json={"challenge_token": challenge, "code": code},
            headers=headers or self.headers,
        )


class AppleApiTests(AppleApiFixture):
    async def test_account_restore_list_download_and_delete_require_original_key(self):
        initial = await (await self.login()).json()
        await self.drain()
        account_id = (await (await self.status(initial["job_id"])).json())["account_id"]
        _, extra = create_key(self.db, 1, "Other", expires_at=now() + 3600)
        headers = {
            "Authorization": "Bearer " + extra,
            "Idempotency-Key": "foreign-download-001",
        }
        for credentials in (headers, self.other):
            self.assertEqual(
                (
                    await self.client.get(
                        "/api/v1/accounts/" + account_id, headers=credentials
                    )
                ).status,
                404,
            )
            self.assertEqual(
                (
                    await self.client.post(
                        "/api/v1/accounts/" + account_id + "/restore",
                        json={},
                        headers=credentials,
                    )
                ).status,
                404,
            )
            self.assertEqual(
                (
                    await self.client.delete(
                        "/api/v1/accounts/" + account_id, headers=credentials
                    )
                ).status,
                404,
            )
            response = await self.client.post(
                "/api/v1/downloads",
                json={"account_id": account_id, "adam_id": "123", "kind": "ipa"},
                headers={**credentials, "Idempotency-Key": "foreign-download-001"},
            )
            self.assertEqual(response.status, 404)
        result = await (
            await self.client.get("/api/v1/accounts", headers=headers)
        ).json()
        self.assertEqual(result["accounts"], [])
        result = await (
            await self.client.post(
                "/api/v1/accounts/" + account_id + "/restore",
                json={},
                headers=self.headers,
            )
        ).json()
        self.assertEqual(result["account_id"], account_id)
        self.assertNotIn("password", result)

    async def test_api_session_warmup_requires_active_owner_and_key(self):
        await self.login()
        await self.drain()
        row = self.db.one("select * from api_accounts")
        self.tool.run = AsyncMock(return_value={"success": True})
        await self.api.auth.warm_tick()
        self.tool.run.assert_awaited_once()
        self.assertGreater(
            self.db.one("select kbsync_warmed_at from api_accounts")[0], 0
        )
        await self.api.auth.warm_tick()
        self.tool.run.assert_awaited_once()
        self.db._run("update api_accounts set kbsync_warmed_at=0")
        self.db._run("update api_keys set state='paused' where id=?", (row["key_id"],))
        await self.api.auth.warm_tick()
        self.tool.run.assert_awaited_once()

    async def test_login_without_2fa_does_not_need_a_code_or_bot_account(self):
        response = await self.login()
        self.assertEqual(response.status, 202)
        result = await response.json()
        self.assertIn(result["status"], ("queued", "authenticating", "ready"))
        await self.drain()
        result = await (await self.status(result["job_id"])).json()
        self.assertEqual(result["status"], "ready")
        self.assertEqual(result["action"], "none")
        self.assertNotIn("challenge_token", result)
        self.assertEqual((await self.code(result["job_id"], "unused")).status, 409)
        self.assertEqual(self.db.one("select count(*) from accounts")[0], 0)
        row = self.db.one("select * from api_accounts")
        self.assertNotEqual(row["password_secret"], "normal")
        self.assertEqual(
            self.api.auth.engine_account(1, row["id"])["password"], "normal"
        )

    async def test_2fa_challenge_is_owned_bound_to_key_and_single_use(self):
        result = await (await self.login("twofactor")).json()
        await self.drain()
        job_id = result["job_id"]
        result = await (await self.status(job_id)).json()
        self.assertEqual(result["status"], "requires_2fa")
        self.assertEqual(result["action"], "submit_2fa_code")
        token = result["challenge_token"]
        self.assertNotEqual(
            self.db.one("select challenge_digest from api_auth_jobs")[0], token
        )
        self.assertEqual((await self.status(job_id, self.other)).status, 404)
        self.assertEqual(
            (await self.code(job_id, token, headers=self.other)).status, 404
        )
        _, extra = create_key(self.db, 1, "Other", expires_at=now() + 3600)
        extra_headers = {"Authorization": "Bearer " + extra}
        self.assertEqual((await self.status(job_id, extra_headers)).status, 404)
        self.assertEqual((await self.code(job_id, "forged")).status, 403)
        self.assertEqual((await self.code(job_id, token)).status, 202)
        await self.drain()
        self.assertEqual((await (await self.status(job_id)).json())["status"], "ready")
        self.assertEqual((await self.code(job_id, token)).status, 409)

    async def test_wrong_2fa_codes_rotate_challenge_and_stop_after_three(self):
        job_id = (await (await self.login("twofactor")).json())["job_id"]
        await self.drain()
        previous = None
        for _ in range(3):
            result = await (await self.status(job_id)).json()
            token = result["challenge_token"]
            if previous:
                self.assertNotEqual(token, previous)
                self.assertEqual((await self.code(job_id, previous)).status, 403)
            self.assertEqual((await self.code(job_id, token, "000000")).status, 202)
            previous = token
            await self.drain()
        result = await (await self.status(job_id)).json()
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["error"], "too_many_2fa_attempts")
        self.assertEqual(self.db.one("select password_secret from api_accounts")[0], "")

    async def test_expired_login_rejects_code_and_discards_password(self):
        job_id = (await (await self.login("twofactor")).json())["job_id"]
        await self.drain()
        self.db._run("update api_auth_jobs set expires_at=0")
        result = await (await self.status(job_id)).json()
        self.assertEqual(result["status"], "expired")
        self.assertNotIn("challenge_token", result)
        self.assertEqual((await self.code(job_id, "expired")).status, 409)
        self.assertEqual(self.db.one("select password_secret from api_accounts")[0], "")

    async def test_login_idempotence_rejects_changed_credentials(self):
        first = await (await self.login("slow")).json()
        second = await (await self.login("slow")).json()
        self.assertEqual(first["job_id"], second["job_id"])
        self.assertEqual((await self.login("changed")).status, 409)
        self.gate.set()
        await self.drain()
        self.tool.login.assert_awaited_once()

    async def test_wrong_password_error_never_exposes_tool_output(self):
        job_id = (await (await self.login("bad")).json())["job_id"]
        await self.drain()
        result = await (await self.status(job_id)).json()
        self.assertEqual(result["status"], "failed")
        self.assertNotIn("secret", str(result))
        self.assertEqual(result["error"], "apple_login_failed")

    async def test_multiple_accounts_are_isolated_and_require_explicit_selection(self):
        for index in range(2):
            await self.login(
                email=f"person{index}@example.com",
                headers={
                    **self.headers,
                    "Idempotency-Key": f"login-test-00{index + 1}",
                },
            )
            await self.drain()
        accounts = (
            await (
                await self.client.get("/api/v1/accounts", headers=self.headers)
            ).json()
        )["accounts"]
        self.assertEqual(len(accounts), 2)
        self.assertNotIn("password", str(accounts))
        self.assertEqual(
            (
                await self.client.get("/api/v1/apps/123/versions", headers=self.headers)
            ).status,
            409,
        )
        identifier = accounts[0]["account_id"]
        response = await self.client.get(
            "/api/v1/apps/123/versions",
            params={"account_id": identifier},
            headers=self.headers,
        )
        self.assertEqual(response.status, 200)
        self.assertEqual(self.tool.versions.await_args.args[0]["api_id"], identifier)
        await self.login(email="person0@example.com", headers=self.other)
        await self.drain()
        self.assertEqual(self.db.one("select count(*) from api_accounts")[0], 3)
        engines = {row[0] for row in self.db.all("select engine_id from api_accounts")}
        self.assertEqual(len(engines), 3)
        self.assertEqual(
            (
                await self.client.delete(
                    "/api/v1/accounts/" + identifier, headers=self.other
                )
            ).status,
            404,
        )
        self.assertEqual(
            (
                await self.client.delete(
                    "/api/v1/accounts/" + identifier, headers=self.headers
                )
            ).status,
            200,
        )

    async def test_cancel_and_key_revocation_prevent_login_completion(self):
        job_id = (await (await self.login("slow")).json())["job_id"]
        await asyncio.sleep(0)
        response = await self.client.delete(
            "/api/v1/auth/jobs/" + job_id, headers=self.headers
        )
        self.assertEqual((await response.json())["status"], "canceled")
        next_headers = {**self.headers, "Idempotency-Key": "login-test-002"}
        job_id = (await (await self.login("slow", headers=next_headers)).json())[
            "job_id"
        ]
        await asyncio.sleep(0)
        key_id = self.db.one("select id from api_keys where tg_id=1")[0]
        update_key(self.db, 1, key_id, {"state": "revoked"})
        self.gate.set()
        await self.drain()
        self.assertNotEqual(
            self.db.one("select status from api_auth_jobs where id=?", (job_id,))[0],
            "ready",
        )

    async def test_invalid_input_readonly_and_login_flood_are_rejected(self):
        self.assertEqual((await self.login(password="")).status, 422)
        self.assertEqual((await self.login(email="bad-email")).status, 422)
        self.assertEqual(
            (
                await self.client.post(
                    "/api/v1/auth/jobs",
                    json={
                        "email": "first@example.com",
                        "password": "x",
                        "padding": "x" * 5000,
                    },
                    headers=self.headers,
                )
            ).status,
            413,
        )
        self.api.auth.rates[1] = (10, 10**12)
        self.assertEqual((await self.login()).status, 429)
        readonly = {
            "Authorization": "Bearer " + issue_api_key(self.db, 1, scopes="read"),
            "Idempotency-Key": "login-test-002",
        }
        self.assertEqual((await self.login(headers=readonly)).status, 403)

    async def test_restart_expires_waiting_code_and_drops_challenge(self):
        job_id = (await (await self.login("twofactor")).json())["job_id"]
        await self.drain()
        await self.client.close()
        api = CustomerApi(
            SimpleNamespace(
                cfg=self.cfg, db=self.db, tool=self.tool, store=SimpleNamespace()
            )
        )
        self.client = TestClient(TestServer(ota.build_app(self.cfg, self.db, api)))
        self.api = api
        await self.client.start_server()
        result = await (await self.status(job_id)).json()
        self.assertEqual(result["status"], "expired")
        self.assertEqual(result["error"], "restart_login")


if __name__ == "__main__":
    unittest.main()
