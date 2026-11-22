from __future__ import annotations

import asyncio
import ipaddress
import sqlite3
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import AsyncMock

from test_api_auth import AppleApiFixture
from signum_bot.api_keys import create_key, update_key
from signum_bot.commerce import key_digest
from signum_bot.db import Db, now
from signum_bot.ipatool import IpatoolError
from signum_bot.api_lease import ApiLease


class KeyManagementTests(AppleApiFixture):
    async def test_create_list_pause_resume_revoke_and_scope_checks(self):
        response = await self.client.post(
            "/api/v1/keys",
            json={
                "label": "Website",
                "scopes": ["read", "downloads"],
                "quota": 2,
                "days": 1,
            },
            headers=self.headers,
        )
        self.assertEqual(response.status, 201)
        data = await response.json()
        identifier, secret = data["key_id"], data["api_key"]
        rows = self.db.all("select * from api_keys where tg_id=1")
        self.assertEqual(len(rows), 2)
        stored = self.db.one("select * from api_keys where id=?", (identifier,))
        self.assertEqual(stored["digest"], key_digest(secret))
        self.assertNotIn(secret, tuple(stored))
        listing = await (
            await self.client.get("/api/v1/keys", headers=self.headers)
        ).json()
        self.assertNotIn(secret, str(listing))
        self.assertNotIn("digest", str(listing))
        limited = {"Authorization": "Bearer " + secret}
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=limited)).status, 200
        )
        for state, expected in (("paused", 403), ("active", 200), ("revoked", 401)):
            response = await self.client.patch(
                "/api/v1/keys/" + identifier,
                json={"state": state},
                headers=self.headers,
            )
            self.assertEqual(response.status, 200)
            self.assertEqual(
                (await self.client.get("/api/v1/me", headers=limited)).status, expected
            )
        self.assertEqual(
            (
                await self.client.patch(
                    "/api/v1/keys/" + identifier,
                    json={"state": "active"},
                    headers=self.headers,
                )
            ).status,
            422,
        )

    async def test_other_client_cannot_change_keys_and_limited_key_cannot_delegate(
        self,
    ):
        identifier, secret = create_key(
            self.db,
            1,
            "Worker",
            scopes="read downloads accounts keys",
            expires_at=now() + 3600,
            quota=1,
        )
        self.assertEqual(
            (
                await self.client.patch(
                    "/api/v1/keys/" + identifier,
                    json={"quota_remaining": 500},
                    headers=self.other,
                )
            ).status,
            404,
        )
        limited = {"Authorization": "Bearer " + secret}
        self.assertEqual(
            (
                await self.client.post(
                    "/api/v1/keys", json={"label": "Bypass"}, headers=limited
                )
            ).status,
            403,
        )
        self.assertEqual(
            (
                await self.client.patch(
                    "/api/v1/keys/" + identifier,
                    json={"quota_remaining": None},
                    headers=limited,
                )
            ).status,
            403,
        )
        self.assertEqual(
            (
                await self.client.delete("/api/v1/keys/" + identifier, headers=limited)
            ).status,
            403,
        )
        read_id, read_secret = create_key(
            self.db, 1, "Read", scopes="read keys", expires_at=now() + 3600
        )
        read_headers = {"Authorization": "Bearer " + read_secret}
        self.assertEqual(
            (
                await self.client.post(
                    "/api/v1/keys",
                    json={"label": "Escalate", "scopes": ["read", "downloads"]},
                    headers=read_headers,
                )
            ).status,
            403,
        )
        master = self.db.one("select id from api_keys where tg_id=1 and is_default=1")[
            0
        ]
        self.assertEqual(
            (
                await self.client.patch(
                    "/api/v1/keys/" + master,
                    json={"quota_remaining": None},
                    headers=read_headers,
                )
            ).status,
            403,
        )

    async def test_client_pause_key_expiry_and_invalid_updates(self):
        identifier, _ = create_key(self.db, 1, "Worker", expires_at=now() + 3600)
        for body in (
            {"state": []},
            {"quota_remaining": -1},
            {"quota_remaining": True},
            {"scopes": "read"},
            {"expires_at": now() + 366 * 86400},
        ):
            self.assertEqual(
                (
                    await self.client.patch(
                        "/api/v1/keys/" + identifier, json=body, headers=self.headers
                    )
                ).status,
                422,
            )
        self.db._run("update users set api_enabled=0 where tg_id=1")
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=self.headers)).status, 403
        )
        self.db._run("update users set api_enabled=1 where tg_id=1")
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=self.headers)).status, 200
        )

    async def test_key_quota_is_atomic_and_refunded_once_with_user_quota(self):
        account_id = self.db.account_add(
            1, "legacy@example.com", "test", "test-keychain"
        )
        self.db.account_status(account_id, "ready")
        self.api.deps.store.lookup = AsyncMock(
            return_value={"adam_id": "123", "title": "App", "size": 8}
        )
        self.tool.download = AsyncMock(side_effect=IpatoolError("failed"))
        identifier, secret = create_key(
            self.db, 1, "Quota", expires_at=now() + 3600, quota=1
        )
        headers = {
            "Authorization": "Bearer " + secret,
            "Idempotency-Key": "download-key-001",
        }
        response = await self.client.post(
            "/api/v1/downloads", json={"adam_id": "123"}, headers=headers
        )
        job_id = (await response.json())["job_id"]
        await asyncio.gather(*list(self.api.tasks))
        self.assertEqual(
            self.db.one(
                "select quota_remaining from api_keys where id=?", (identifier,)
            )[0],
            1,
        )
        self.assertEqual(
            self.db.one("select api_remaining from users where tg_id=1")[0], 10
        )
        self.api.fail_job(job_id, "again")
        self.assertEqual(
            self.db.one(
                "select quota_remaining from api_keys where id=?", (identifier,)
            )[0],
            1,
        )
        update_key(self.db, 1, identifier, {"quota_remaining": 0})
        self.assertEqual(
            (
                await self.client.post(
                    "/api/v1/downloads",
                    json={"adam_id": "123"},
                    headers={**headers, "Idempotency-Key": "download-key-002"},
                )
            ).status,
            429,
        )
        self.assertEqual(
            self.db.one("select api_remaining from users where tg_id=1")[0], 10
        )
        update_key(self.db, 1, identifier, {"quota_remaining": 1})
        self.db._run("update users set api_remaining=0 where tg_id=1")
        self.assertEqual(
            (
                await self.client.post(
                    "/api/v1/downloads",
                    json={"adam_id": "123"},
                    headers={**headers, "Idempotency-Key": "download-key-003"},
                )
            ).status,
            429,
        )
        self.assertEqual(
            self.db.one(
                "select quota_remaining from api_keys where id=?", (identifier,)
            )[0],
            1,
        )

    async def test_global_load_limits_and_proxy_headers_require_trusted_peer(self):
        self.api.inflight = self.cfg.api_global_requests
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=self.headers)).status, 503
        )
        self.api.inflight = 0
        forged = {
            **self.headers,
            "X-Real-IP": "203.0.113.10",
            "X-Forwarded-Proto": "https",
        }
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=forged)).status, 200
        )
        self.assertNotIn("203.0.113.10", self.api.ip_rates)
        self.api.trusted_proxies = (ipaddress.ip_network("127.0.0.1/32"),)
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=forged)).status, 200
        )
        self.assertIn("203.0.113.10", self.api.ip_rates)
        self.api.cfg = replace(self.cfg, api_global_rate=1)
        self.api.global_rates.clear()
        self.assertEqual((await self.client.get("/api/schema.json")).status, 200)
        self.assertEqual((await self.client.get("/api/schema.json")).status, 503)


class KeyMigrationTests(unittest.TestCase):
    def test_old_unique_owner_keys_migrate_without_losing_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "test.db"
            conn = sqlite3.connect(path)
            conn.execute(
                "create table api_keys(digest text primary key,tg_id integer unique not null,created_at integer not null)"
            )
            conn.execute("insert into api_keys values ('existing-digest',1,10)")
            conn.commit()
            conn.close()
            db = Db(path)
            try:
                db.user(1)
                row = db.one("select * from api_keys")
                self.assertEqual(row["digest"], "existing-digest")
                self.assertTrue(row["id"])
                self.assertEqual(row["is_default"], 1)
                create_key(db, 1, "Second", expires_at=now() + 3600)
                self.assertEqual(db.one("select count(*) from api_keys")[0], 2)
            finally:
                db.conn.close()

    def test_api_process_lease_blocks_second_server_and_releases(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "api.lock"
            lease = ApiLease(path)
            try:
                with self.assertRaises(RuntimeError):
                    ApiLease(path)
            finally:
                lease.close()
            second = ApiLease(path)
            second.close()


if __name__ == "__main__":
    unittest.main()
