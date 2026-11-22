from __future__ import annotations

import asyncio
import copy
import json
import os
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from uuid import uuid4
from unittest.mock import patch

import httpx
from aiohttp import web
from aiohttp.test_utils import TestClient, TestServer

from signum_bot import ota
from signum_bot.commerce import create_order, issue_api_key
from signum_bot.config import Config
from signum_bot.customer_api import CustomerApi
from signum_bot.db import Db, now
from signum_bot.payments import Payments, PaymentError


class PaymentTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.cfg = replace(
            Config.load(),
            bridge_enabled=False,
            data_dir=Path(self.tmp.name),
            payment_mode="yookassa",
            production=False,
            yookassa_shop_id="123",
            yookassa_secret_key="test_fixture",
        )
        self.db = Db(self.cfg.db_path)
        for identifier in (1, 2):
            self.db.user(identifier)
            self.db._run(
                "update users set api_until=? where tg_id=?", (now() + 3600, identifier)
            )
        self.records = {}
        self.calls = []
        self.fail_create = False
        self.clients = []

        async def transport(request):
            self.calls.append(
                (
                    request.method,
                    request.url.path,
                    request.headers.get("Idempotence-Key"),
                )
            )
            if request.method == "POST":
                if self.fail_create:
                    raise httpx.ConnectTimeout("temporary")
                data = json.loads(request.content)
                identifier = str(uuid4())
                data.update(
                    id=identifier,
                    status="pending",
                    paid=False,
                    test=True,
                    recipient={"account_id": "123"},
                    confirmation={
                        "confirmation_url": "https://yoomoney.ru/checkout/test"
                    },
                )
                self.records[identifier] = data
                return httpx.Response(200, json=data)
            return httpx.Response(
                200,
                json=copy.deepcopy(self.records[request.url.path.rsplit("/", 1)[1]]),
            )

        self.payments = Payments(
            self.cfg, self.db, transport=httpx.MockTransport(transport)
        )

    async def asyncTearDown(self):
        for client in self.clients:
            await client.close()
        await self.payments.close()
        self.db.conn.close()
        self.tmp.cleanup()

    async def checkout(self, plan="single"):
        identifier = create_order(self.cfg, self.db, 1, plan)
        await self.payments.checkout(identifier, 1)
        order = self.db.one("select * from orders where id=?", (identifier,))
        return identifier, self.records[order["payment_id"]]

    async def test_checkout_and_verified_success_are_idempotent(self):
        identifier = create_order(self.cfg, self.db, 1, "single")
        urls = await asyncio.gather(
            self.payments.checkout(identifier, 1), self.payments.checkout(identifier, 1)
        )
        self.assertEqual(urls[0], urls[1])
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(await self.payments.verify(identifier, 1), "pending")
        self.assertEqual(self.db.credits(1), 0)
        payment = self.records[
            self.db.one("select payment_id from orders where id=?", (identifier,))[0]
        ]
        payment.update(status="succeeded", paid=True)
        await asyncio.gather(
            self.payments.verify(identifier, 1), self.payments.verify(identifier, 1)
        )
        self.assertEqual(self.db.credits(1), 1)
        self.assertEqual(len(self.db.all("select * from payments")), 1)
        self.assertEqual(self.db.stats()["revenue"], 0)
        self.assertEqual(self.db.one("select mode from payments")[0], "test")

    async def test_wrong_amount_currency_owner_test_or_shop_never_grants_purchase(self):
        for field, value in (
            ("amount", {"value": "0.01", "currency": "RUB"}),
            ("amount", {"value": "29.00", "currency": "USD"}),
            ("amount", {"value": "NaN", "currency": "RUB"}),
            ("test", False),
            ("metadata", {"order_id": "forged", "tg_id": "1"}),
            ("recipient", {"account_id": "other"}),
        ):
            with self.subTest(field=field, value=value):
                identifier, payment = await self.checkout()
                payment.update(status="succeeded", paid=True)
                payment[field] = value
                with self.assertRaises(PaymentError):
                    await self.payments.verify(identifier, 1)
        self.assertEqual(self.db.credits(1), 0)
        self.assertEqual(len(self.db.all("select * from payment_notifications")), 0)

    async def test_verified_real_payment_queues_admin_notice_once(self):
        identifier, payment = await self.checkout("month")
        self.payments.cfg = replace(self.cfg, yookassa_secret_key="live_fixture", admins={99})
        self.db._run("update orders set payment_test=0 where id=?", (identifier,))
        payment.update(status="succeeded", paid=True, test=False)
        await asyncio.gather(self.payments.verify(identifier, 1), self.payments.verify(identifier, 1))
        notices = self.db.all("select * from payment_notifications")
        self.assertEqual(len(notices), 1)
        self.assertEqual(notices[0]["admin_id"], 99)
        self.assertEqual(notices[0]["charge_id"], payment["id"])

    async def test_test_payment_does_not_queue_success_notice(self):
        identifier, payment = await self.checkout("month")
        self.payments.cfg = replace(self.cfg, admins={99})
        payment.update(status="succeeded", paid=True)
        await self.payments.verify(identifier, 1)
        self.assertEqual(len(self.db.all("select * from payment_notifications")), 0)

    async def test_another_owner_cannot_create_or_check_payment(self):
        identifier, _ = await self.checkout()
        with self.assertRaises(PaymentError):
            await self.payments.checkout(identifier, 2)
        with self.assertRaises(PaymentError):
            await self.payments.verify(identifier, 2)

    async def test_canceled_or_unpaid_success_never_grants(self):
        identifier, payment = await self.checkout()
        payment.update(status="succeeded", paid=False)
        self.assertEqual(await self.payments.verify(identifier), "pending")
        payment.update(status="canceled")
        self.assertEqual(await self.payments.verify(identifier), "canceled")
        self.assertEqual(self.db.credits(1), 0)

    async def test_failed_creation_can_retry_with_same_provider_idempotence_key(self):
        identifier = create_order(self.cfg, self.db, 1, "month")
        self.fail_create = True
        with self.assertRaises(PaymentError):
            await self.payments.checkout(identifier, 1)
        self.fail_create = False
        await self.payments.checkout(identifier, 1)
        self.assertEqual(self.calls[0][2], self.calls[1][2])
        self.assertEqual(self.db.sub_active(1), 0)

    async def test_subscription_and_api_checkout_share_verified_payment_flow(self):
        for plan in ("week", "month", "halfyear", "year", "api"):
            identifier, payment = await self.checkout(plan)
            self.assertEqual(payment["amount"]["value"], f"{self.cfg.amount(plan) / 100:.2f}")
            self.assertEqual(await self.payments.verify(identifier, 1), "pending")
        self.assertEqual(self.db.sub_active(1), 0)

    async def test_encrypted_bridge_retry_reuses_provider_idempotence_key(self):
        from signum_bot.bridge import seal, unseal
        key = bytes.fromhex("ab" * 32)
        records = {}
        requests = []

        async def transport(request):
            self.assertNotIn("authorization", request.headers)
            self.assertNotIn(self.cfg.yookassa_secret_key.encode(), request.content)
            body = json.loads(request.content)
            self.assertEqual(body["op"], "yookassa")
            payload = unseal(body["payload"], key)
            self.assertEqual(payload["kind"], "yookassa")
            self.assertEqual(payload["path"], "payments")
            requests.append(payload["idempotence"])
            identifier = payload["idempotence"]
            if identifier not in records:
                payment = copy.deepcopy(payload["json"])
                payment.update(id=str(uuid4()), status="pending", paid=False, test=True,
                               recipient={"account_id": "123"},
                               confirmation={"confirmation_url": "https://yoomoney.ru/checkout/test"})
                records[identifier] = payment
                raise httpx.ReadTimeout("reply lost", request=request)
            return httpx.Response(200, json={"payload": seal({
                "kind": "yookassa", "status": 200, "data": records[identifier]
            }, key)})

        await self.payments.bridge_http.aclose()
        self.payments.bridge_http = httpx.AsyncClient(transport=httpx.MockTransport(transport))
        self.payments.cfg = replace(self.cfg, bridge_enabled=True)
        identifier = create_order(self.cfg, self.db, 1, "week")
        with patch.dict(os.environ, {"BRIDGE_KEY": key.hex()}):
            with self.assertRaises(PaymentError):
                await self.payments.checkout(identifier, 1)
            await self.payments.checkout(identifier, 1)
        self.assertEqual(requests, [identifier, identifier])
        self.assertEqual(len(records), 1)
        self.assertEqual(self.db.sub_active(1), 0)

    async def test_webhook_checks_provider_instead_of_trusting_submitted_status(self):
        identifier, payment = await self.checkout()
        app = web.Application(client_max_size=16384)
        app.router.add_post("/payments/yookassa", self.payments.webhook)
        client = TestClient(TestServer(app))
        await client.start_server()
        self.clients.append(client)
        notification = {
            "type": "notification",
            "event": "payment.succeeded",
            "object": {"id": payment["id"], "paid": True, "status": "succeeded"},
        }
        response = await client.post("/payments/yookassa", json=notification)
        self.assertEqual(response.status, 200)
        self.assertEqual(self.db.credits(1), 0)
        payment.update(status="succeeded", paid=True)
        self.db._run("update orders set expires_at=0 where id=?", (identifier,))
        for _ in range(2):
            self.assertEqual(
                (await client.post("/payments/yookassa", json=notification)).status, 200
            )
        self.assertEqual(self.db.credits(1), 1)

    async def test_api_topup_order_scope_idempotence_and_private_status(self):
        deps = SimpleNamespace(
            cfg=self.cfg,
            db=self.db,
            payments=self.payments,
            tool=SimpleNamespace(),
            store=SimpleNamespace(),
        )
        api = CustomerApi(deps)
        client = TestClient(TestServer(ota.build_app(self.cfg, self.db, api)))
        await client.start_server()
        self.clients.append(client)
        headers = {
            "Authorization": "Bearer " + issue_api_key(self.db, 1),
            "Idempotency-Key": "topup-test-001",
        }
        body = {"plan": "api500"}
        first = await (
            await client.post("/api/v1/billing/orders", headers=headers, json=body)
        ).json()
        second = await (
            await client.post("/api/v1/billing/orders", headers=headers, json=body)
        ).json()
        self.assertEqual(first["order_id"], second["order_id"])
        self.assertEqual(first["downloads"], 500)
        self.assertTrue(first["test"])
        self.assertEqual(first["amount_rub"], "149.00")
        self.assertEqual(len(self.records), 1)
        order = self.db.one("select * from orders where id=?", (first["order_id"],))
        self.records[order["payment_id"]].update(status="succeeded", paid=True)
        path = "/api/v1/billing/orders/" + first["order_id"]
        data = await (await client.get(path, headers=headers)).json()
        self.assertEqual(data["status"], "complete")
        self.assertEqual(
            self.db.one("select api_extra_remaining from users where tg_id=1")[0], 500
        )
        other = {"Authorization": "Bearer " + issue_api_key(self.db, 2)}
        self.assertEqual((await client.get(path, headers=other)).status, 404)
        self.assertEqual(
            (
                await client.post(
                    "/api/v1/billing/orders", headers=headers, json={"plan": "api100"}
                )
            ).status,
            409,
        )
        readonly = {
            "Authorization": "Bearer " + issue_api_key(self.db, 1, scopes="read"),
            "Idempotency-Key": "topup-test-002",
        }
        self.assertEqual(
            (
                await client.post("/api/v1/billing/orders", headers=readonly, json=body)
            ).status,
            403,
        )
        self.assertEqual(
            (await client.get("/api/v1/billing/packages", headers=readonly)).status, 200
        )


if __name__ == "__main__":
    unittest.main()
