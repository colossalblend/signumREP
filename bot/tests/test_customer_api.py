from __future__ import annotations

import asyncio
import time
import plistlib
import tempfile
import unittest
import zipfile
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

from aiohttp.test_utils import TestClient, TestServer

from signum_bot import ota
from signum_bot.commerce import issue_api_key
from signum_bot.config import Config
from signum_bot.customer_api import CustomerApi
from signum_bot.db import Db, now
from signum_bot.ipatool import Ipatool, IpatoolError


class CustomerApiTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.cfg = replace(
            Config.load(),
            bridge_enabled=False,
            api_trusted_proxies=(),
            data_dir=Path(self.tmp.name),
            public_base_url="https://example.com",
            api_rate_limit=100,
            max_mb_sub=1,
        )
        self.db = Db(self.cfg.db_path)
        for tg_id in (1, 2):
            self.db.user(tg_id)
            self.db._run(
                "update users set api_until=?, api_remaining=3,api_bytes_remaining=10485760 where tg_id=?",
                (now() + 3600, tg_id),
            )
        account_id = self.db.account_add(
            1, "test@example.com", "secret-password", "secret-keychain"
        )
        self.db.account_status(account_id, "ready")
        self.tool = Ipatool(self.cfg, self.db)
        self.tool.revalidate = AsyncMock()
        self.gate = asyncio.Event()
        self.gate.set()

        async def download(
            account, adam, *, external_version_id, out_dir, on_start=None
        ):
            await self.gate.wait()
            ipa = out_dir / "test.ipa"
            ipa.write_bytes(b"test ipa")
            return ipa

        self.tool.download = AsyncMock(side_effect=download)
        self.tool.versions = AsyncMock(return_value=["456"])
        self.store = SimpleNamespace(
            lookup=AsyncMock(
                return_value={"adam_id": "123", "title": "Test App", "size": 8}
            ),
            search=AsyncMock(return_value=[]),
            catalog=AsyncMock(return_value={"apps": []}),
            catalog_app=AsyncMock(return_value={"versions": []}),
        )
        self.api = CustomerApi(
            SimpleNamespace(cfg=self.cfg, db=self.db, tool=self.tool, store=self.store)
        )
        self.client = TestClient(TestServer(ota.build_app(self.cfg, self.db, self.api)))
        await self.client.start_server()
        self.headers = {
            "Authorization": "Bearer " + issue_api_key(self.db, 1),
            "Idempotency-Key": "download-001",
        }
        self.other_headers = {"Authorization": "Bearer " + issue_api_key(self.db, 2)}

    async def asyncTearDown(self):
        await self.client.close()
        self.db.conn.close()
        self.tmp.cleanup()

    def remaining(self):
        return self.db.one("select api_remaining from users where tg_id=1")[0]

    async def submit(self, data=None, headers=None):
        return await self.client.post(
            "/api/v1/downloads",
            json=data if data is not None else {"adam_id": "123"},
            headers=headers or self.headers,
        )

    async def drain(self):
        await asyncio.wait_for(asyncio.gather(*list(self.api.tasks)), timeout=2)

    async def test_docs_public_but_keys_required_for_customer_data(self):
        response = await self.client.get("/api/docs")
        self.assertEqual(response.status, 200)
        response = await self.client.get("/api/v1/me")
        self.assertEqual(response.status, 401)
        response = await self.client.get("/api/v1/me", headers=self.headers)
        self.assertEqual(response.status, 200)
        data = await response.json()
        self.assertEqual(data["downloads_remaining"], 3)
        self.assertTrue(data["apple_id_connected"])
        self.assertNotIn("secret", str(data))

    async def test_download_job_is_private_to_its_creating_key(self):
        from signum_bot.api_keys import create_key

        result = await (await self.submit()).json()
        await self.drain()
        _, token = create_key(self.db, 1, "Second", expires_at=now() + 3600)
        headers = {"Authorization": "Bearer " + token}
        path = "/api/v1/downloads/" + result["job_id"]
        self.assertEqual((await self.client.get(path, headers=headers)).status, 404)
        self.assertEqual((await self.client.delete(path, headers=headers)).status, 404)
        self.assertEqual(
            (await self.client.get(path, headers=self.headers)).status, 200
        )

    async def test_ordinary_subscription_does_not_unlock_api(self):
        self.db.extend_sub(1, 30)
        self.db._run("update users set api_until=0 where tg_id=1")
        response = await self.client.get("/api/v1/me", headers=self.headers)
        self.assertEqual(response.status, 403)

    async def test_blocked_revoked_and_rotated_keys_stop_access(self):
        self.db.set_blocked(1, True)
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=self.headers)).status, 403
        )
        self.db.set_blocked(1, False)
        issue_api_key(self.db, 1)
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=self.headers)).status, 401
        )

    async def test_rate_limit_returns_429(self):
        self.api.cfg = replace(self.cfg, api_rate_limit=1)
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=self.headers)).status, 200
        )
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=self.headers)).status, 429
        )

    async def test_download_success_preserves_scope_and_paid_expiry(self):
        response = await self.submit()
        self.assertEqual(response.status, 202)
        identifier = (await response.json())["job_id"]
        await self.drain()
        response = await self.client.get(
            "/api/v1/downloads/" + identifier, headers=self.headers
        )
        data = await response.json()
        self.assertEqual(data["status"], "ready")
        self.assertIn("/f/", data["file_url"])
        self.assertEqual(self.remaining(), 2)
        file = self.db.one("select * from files")
        self.assertEqual(file["paid_with"], "api")
        self.assertEqual(file["expires_at"] - file["created_at"], 12 * 3600)
        self.assertEqual(self.db.week_uses(1), 0)
        self.assertEqual(
            (
                await self.client.get(
                    "/api/v1/downloads/" + identifier, headers=self.other_headers
                )
            ).status,
            404,
        )
        self.db._run("update files set expires_at=0")
        data = await (
            await self.client.get(
                "/api/v1/downloads/" + identifier, headers=self.headers
            )
        ).json()
        self.assertEqual(data["status"], "expired")
        self.assertNotIn("file_url", data)

    async def test_idempotency_prevents_duplicate_download_and_charge(self):
        self.gate.clear()
        first = await (await self.submit()).json()
        second = await (await self.submit()).json()
        self.assertEqual(first["job_id"], second["job_id"])
        self.assertEqual(self.remaining(), 2)
        self.assertEqual((await self.submit({"adam_id": "999"})).status, 409)
        headers = {**self.headers, "Idempotency-Key": "download-002"}
        self.assertEqual((await self.submit(headers=headers)).status, 409)
        self.gate.set()
        await self.drain()
        self.tool.download.assert_awaited_once()

    async def test_failed_job_refunds_exactly_once_and_hides_error_details(self):
        self.tool.download.side_effect = IpatoolError(
            "secret-password secret@example.com"
        )
        identifier = (await (await self.submit()).json())["job_id"]
        await self.drain()
        self.assertEqual(self.remaining(), 3)
        self.api.fail_job(identifier, "retry")
        self.assertEqual(self.remaining(), 3)
        data = await (
            await self.client.get(
                "/api/v1/downloads/" + identifier, headers=self.headers
            )
        ).json()
        self.assertEqual(data["status"], "failed")
        self.assertNotIn("secret", str(data))

    async def test_oversized_files_fail_before_download_and_refund(self):
        self.store.lookup.return_value["size"] = 2 * 1024 * 1024
        identifier = (await (await self.submit()).json())["job_id"]
        await self.drain()
        self.tool.download.assert_not_awaited()
        self.assertEqual(self.remaining(), 3)
        data = await (
            await self.client.get(
                "/api/v1/downloads/" + identifier, headers=self.headers
            )
        ).json()
        self.assertEqual(data["error"], "file_too_large")

    async def test_actual_oversized_download_refunds_and_removes_file(self):
        async def large_file(
            account, adam, *, external_version_id, out_dir, on_start=None
        ):
            ipa = out_dir / "big.ipa"
            ipa.write_bytes(b"0" * (2 * 1024 * 1024))
            return ipa

        self.tool.download.side_effect = large_file
        identifier = (await (await self.submit()).json())["job_id"]
        await self.drain()
        self.assertEqual(self.remaining(), 3)
        self.assertFalse(self.api.directory(identifier).exists())
        self.assertEqual(len(self.db.all("select * from files")), 0)

    async def test_ota_job_preserves_version_and_serves_install_page_and_manifest(self):
        async def archive(
            account, adam, *, external_version_id, out_dir, on_start=None
        ):
            ipa = out_dir / "app.ipa"
            with zipfile.ZipFile(ipa, "w") as output:
                output.writestr(
                    "Payload/Test.app/Info.plist",
                    plistlib.dumps(
                        {
                            "CFBundleIdentifier": "com.test.app",
                            "CFBundleShortVersionString": "2.0",
                        }
                    ),
                )
            return ipa

        self.tool.download.side_effect = archive
        identifier = (
            await (
                await self.submit(
                    {"adam_id": "123", "kind": "ota", "version_id": "456"}
                )
            ).json()
        )["job_id"]
        await self.drain()
        data = await (
            await self.client.get(
                "/api/v1/downloads/" + identifier, headers=self.headers
            )
        ).json()
        self.assertEqual(data["status"], "ready")
        self.assertIn("install_url", data)
        self.assertTrue(
            data["itms_services_url"].startswith(
                "itms-services://?action=download-manifest&url=https%3A%2F%2F"
            )
        )
        self.assertEqual(
            data["manifest_url"],
            "https://example.com/ota/"
            + self.db.one("select * from files")["token"]
            + ".plist",
        )
        self.assertEqual(
            self.tool.download.await_args.kwargs["external_version_id"], "456"
        )
        file = self.db.one("select * from files")
        response = await self.client.get("/i/" + file["token"])
        self.assertEqual(response.status, 200)
        self.assertIn("12", await response.text())
        response = await self.client.get("/ota/" + file["token"] + ".plist")
        manifest = plistlib.loads(await response.read())
        self.assertEqual(manifest["items"][0]["metadata"]["bundle-version"], "2.0")

    async def test_no_quota_missing_apple_id_and_bad_input_reject_jobs(self):
        self.db._run("update users set api_remaining=0 where tg_id=1")
        self.assertEqual((await self.submit()).status, 429)
        self.assertEqual((await self.submit({"adam_id": "../123"})).status, 422)
        self.assertEqual(
            (
                await self.submit(
                    headers={"Authorization": self.headers["Authorization"]}
                )
            ).status,
            422,
        )
        self.assertEqual(
            (
                await self.submit(
                    headers={**self.other_headers, "Idempotency-Key": "download-other"}
                )
            ).status,
            409,
        )
        self.assertEqual(len(self.db.all("select * from api_jobs")), 0)

    async def test_shutdown_and_restart_refund_interrupted_jobs(self):
        self.gate.clear()
        identifier = (await (await self.submit()).json())["job_id"]
        await self.api.close(None)
        self.assertEqual(self.remaining(), 3)
        self.db._run("update users set api_remaining=api_remaining-1 where tg_id=1")
        self.db._run(
            "update api_jobs set reserved=1,status='downloading' where id=?",
            (identifier,),
        )
        await self.api.recover(None)
        await self.api.recover(None)
        self.assertEqual(self.remaining(), 3)

    async def test_catalog_builds_are_filtered_for_owner(self):
        rows = [{"app_store_id": "123"}, {"app_store_id": "999"}]
        self.store.catalog_app.return_value = {"versions": rows, "title": "Bank"}
        self.api.owned.scan = AsyncMock(
            return_value=SimpleNamespace(versions=rows[:1], unchecked=0)
        )
        data = await (
            await self.client.get("/api/v1/catalog/tbank", headers=self.headers)
        ).json()
        self.assertEqual(data["versions"], rows[:1])
        self.assertEqual(self.api.owned.scan.await_args.args[0]["tg_id"], 1)

    async def test_docs_are_html_with_csp_and_schema_stays_public(self):
        response = await self.client.get("/api/docs")
        text = await response.text()
        self.assertIn("text/html", response.headers["Content-Type"])
        self.assertIn("Content-Security-Policy", response.headers)
        self.assertIn("Установить на iPhone", text)
        self.assertNotIn("@@NONCE@@", text)
        self.assertEqual((await self.client.get("/api/schema.json")).status, 200)

    async def test_expired_and_readonly_keys_cannot_create_download(self):
        self.db._run("update api_keys set expires_at=0 where tg_id=1")
        self.assertEqual((await self.submit()).status, 401)
        key = issue_api_key(self.db, 1, scopes="read")
        headers = {**self.headers, "Authorization": "Bearer " + key}
        self.assertEqual(
            (await self.client.get("/api/v1/me", headers=headers)).status, 200
        )
        self.assertEqual((await self.submit(headers=headers)).status, 403)
        self.assertEqual(self.remaining(), 3)

    async def test_rotation_does_not_reset_rate_limit(self):
        self.api.cfg = replace(self.cfg, api_rate_limit=1)
        await self.client.get("/api/v1/me", headers=self.headers)
        key = issue_api_key(self.db, 1)
        self.assertEqual(
            (
                await self.client.get(
                    "/api/v1/me", headers={"Authorization": "Bearer " + key}
                )
            ).status,
            429,
        )

    async def test_old_traffic_budget_does_not_limit_downloads(self):
        self.db._run("update users set api_bytes_remaining=0 where tg_id=1")
        await self.submit()
        await self.drain()
        self.assertEqual(
            self.db.one("select api_bytes_remaining from users where tg_id=1")[0], 0
        )
        response = await self.submit(
            headers={**self.headers, "Idempotency-Key": "download-002"}
        )
        identifier = (await response.json())["job_id"]
        await self.drain()
        data = await (
            await self.client.get(
                "/api/v1/downloads/" + identifier, headers=self.headers
            )
        ).json()
        self.assertEqual(data["status"], "ready")
        self.assertIsNone(data["error"])
        self.assertEqual(self.remaining(), 1)
        me = await (await self.client.get("/api/v1/me", headers=self.headers)).json()
        self.assertNotIn("traffic_remaining_bytes", me)

    async def test_extra_quota_is_reserved_and_refunded_to_original_balance(self):
        self.db._run(
            "update users set api_remaining=0,api_extra_remaining=2 where tg_id=1"
        )
        self.tool.download.side_effect = IpatoolError("failure")
        await self.submit()
        await self.drain()
        user = self.db.one(
            "select api_remaining,api_extra_remaining from users where tg_id=1"
        )
        self.assertEqual(tuple(user), (0, 2))

    async def test_owner_can_revoke_ready_file_without_refunding_consumed_download(
        self,
    ):
        identifier = (await (await self.submit()).json())["job_id"]
        await self.drain()
        file = self.db.one("select * from files")
        self.assertEqual(
            (
                await self.client.delete(
                    "/api/v1/downloads/" + identifier, headers=self.other_headers
                )
            ).status,
            404,
        )
        response = await self.client.delete(
            "/api/v1/downloads/" + identifier, headers=self.headers
        )
        self.assertEqual(response.status, 200)
        self.assertEqual(
            (await self.client.get("/f/" + file["token"] + ".ipa")).status, 404
        )
        self.assertEqual(self.remaining(), 2)

    async def test_owner_can_cancel_active_download_and_get_allowance_back(self):
        self.gate.clear()
        identifier = (await (await self.submit()).json())["job_id"]
        response = await self.client.delete(
            "/api/v1/downloads/" + identifier, headers=self.headers
        )
        self.assertEqual(response.status, 200)
        self.assertEqual(self.remaining(), 3)

    async def test_invalid_keys_are_rate_limited_before_db_lookup(self):
        self.api.ip_rates["127.0.0.1"] = (120, time.monotonic() + 60)
        response = await self.client.get(
            "/api/v1/me", headers={"Authorization": "Bearer wrong"}
        )
        self.assertEqual(response.status, 429)
        self.assertIn("Retry-After", response.headers)

    async def test_ready_file_supports_range_and_head_requests(self):
        await self.submit()
        await self.drain()
        file = self.db.one("select * from files")
        path = "/f/" + file["token"] + ".ipa"
        response = await self.client.get(path, headers={"Range": "bytes=0-3"})
        self.assertEqual(response.status, 206)
        self.assertEqual(await response.read(), b"test")
        self.assertEqual((await self.client.head(path)).status, 200)


if __name__ == "__main__":
    unittest.main()
