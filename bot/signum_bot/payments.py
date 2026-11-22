from __future__ import annotations

import asyncio
import logging
import re
from contextlib import suppress
from decimal import Decimal, InvalidOperation
from urllib.parse import urlparse

import httpx
from aiohttp import web
from cachetools import TTLCache
from cryptography.exceptions import InvalidTag

from .commerce import fulfill_order
from .db import now
from .rate_limits import take_request
from .ui import PLAN_TITLES

LOG = logging.getLogger("signum-bot.payments")
PAYMENT_ID = re.compile(r"[a-fA-F0-9-]{36}")


class PaymentError(Exception):
    pass


class Payments:
    def __init__(self, cfg, db, *, transport=None):
        self.cfg, self.db = cfg, db
        self.http = httpx.AsyncClient(
            base_url="https://api.yookassa.ru/v3/",
            auth=(cfg.yookassa_shop_id, cfg.yookassa_secret_key),
            timeout=httpx.Timeout(20, connect=8),
            transport=transport,
        )
        self.bridge_http = httpx.AsyncClient(
            timeout=httpx.Timeout(15, connect=3), trust_env=False
        )
        self.locks = TTLCache(maxsize=1024, ttl=86400)
        self.rates = TTLCache(maxsize=4096, ttl=60)
        self.worker = None

    @property
    def ready(self):
        return bool(
            self.cfg.payment_mode == "yookassa"
            and self.cfg.yookassa_shop_id
            and self.cfg.yookassa_secret_key
        )

    @property
    def test(self):
        return self.cfg.yookassa_secret_key.startswith("test_")

    async def call(self, method, path, **kwargs):
        try:
            if self.cfg.bridge_enabled:
                from .bridge import BridgeWorker, seal, unseal

                worker = BridgeWorker(self.cfg)
                result = await worker.exchange(
                    self.bridge_http, "yookassa",
                    payload=seal({
                        "kind": "yookassa", "method": method, "path": path,
                        "shop": self.cfg.yookassa_shop_id,
                        "secret": self.cfg.yookassa_secret_key,
                        "idempotence": kwargs.get("headers", {}).get("Idempotence-Key", ""),
                        "json": kwargs.get("json"),
                    }, worker.key),
                )
                reply = unseal(result["payload"], worker.key)
                if reply.get("kind") != "yookassa":
                    raise ValueError
                payload = reply["data"]
                status = int(reply["status"])
            else:
                response = await self.http.request(method, path, **kwargs)
                status = response.status_code
                payload = response.json()
            if not isinstance(payload, dict):
                raise ValueError
            if not 200 <= status < 300:
                code = str(payload.get("code", ""))
                LOG.warning("Payment provider rejected request (HTTP %s, code=%s)",
                            status, code if re.fullmatch(r"[a-z0-9_]{1,64}", code) else "unknown")
                raise PaymentError("payment_provider_unavailable")
            return payload
        except (httpx.HTTPError, ValueError, KeyError, TypeError, InvalidTag) as exc:
            LOG.warning("Payment provider connection failed (%s)", type(exc).__name__)
            raise PaymentError("payment_provider_unavailable") from None

    def matches(self, payment, order):
        try:
            amount = payment["amount"]
            value = Decimal(amount["value"])
            return (
                payment["id"] == order["payment_id"]
                and amount["currency"] == "RUB"
                and value.is_finite()
                and value == Decimal(order["amount"]) / 100
                and payment.get("metadata", {}).get("order_id") == order["id"]
                and payment.get("metadata", {}).get("tg_id") == str(order["tg_id"])
                and payment.get("test") is bool(order["payment_test"])
                and str(
                    payment.get("recipient", {}).get(
                        "account_id", self.cfg.yookassa_shop_id
                    )
                )
                == self.cfg.yookassa_shop_id
            )
        except (KeyError, TypeError, AttributeError, InvalidOperation):
            return False

    async def checkout(self, identifier, tg_id):
        if self.cfg.production and self.test and tg_id not in self.cfg.admins:
            raise PaymentError("live_payments_not_configured")
        if not self.ready:
            raise PaymentError("payments_disabled")
        lock = self.locks.setdefault(identifier, asyncio.Lock())
        async with lock:
            order = self.db.one(
                "select * from orders where id=? and tg_id=?", (identifier, tg_id)
            )
            if not order or order["mode"] != "yookassa" or self.db.blocked(tg_id):
                raise PaymentError("order_unavailable")
            if order["payment_id"]:
                return order["confirmation_url"]
            if order["status"] != "pending" or order["expires_at"] <= now():
                raise PaymentError("order_expired")
            payload = await self.call(
                "POST",
                "payments",
                headers={"Idempotence-Key": identifier},
                json={
                    "amount": {
                        "value": f"{Decimal(order['amount']) / 100:.2f}",
                        "currency": "RUB",
                    },
                    "capture": True,
                    "confirmation": {
                        "type": "redirect",
                        "return_url": self.cfg.yookassa_return_url,
                    },
                    "description": "Signum · " + PLAN_TITLES[order["plan"]],
                    "metadata": {"order_id": identifier, "tg_id": str(tg_id)},
                    "save_payment_method": False,
                },
            )
            payment_id = payload.get("id", "")
            confirmation = payload.get("confirmation", {}).get("confirmation_url", "")
            host = urlparse(confirmation)
            if (
                not PAYMENT_ID.fullmatch(payment_id)
                or host.scheme != "https"
                or host.username
                or host.hostname not in {"yoomoney.ru", "yookassa.ru"}
            ):
                raise PaymentError("invalid_provider_response")
            candidate = dict(order)
            candidate.update(payment_id=payment_id, payment_test=int(self.test))
            if not self.matches(payload, candidate):
                raise PaymentError("payment_mismatch")
            self.db._run(
                "update orders set payment_id=?,confirmation_url=?,payment_test=? where id=?",
                (payment_id, confirmation, int(self.test), identifier),
            )
            return confirmation

    async def verify(self, identifier, tg_id=None):
        order = self.db.one("select * from orders where id=?", (identifier,))
        if (
            not order
            or (tg_id is not None and order["tg_id"] != tg_id)
            or order["mode"] != "yookassa"
        ):
            raise PaymentError("order_unavailable")
        if order["status"] in {"complete", "canceled"}:
            return order["status"]
        if (
            self.cfg.production
            and order["payment_test"]
            and order["tg_id"] not in self.cfg.admins
        ):
            raise PaymentError("live_payments_not_configured")
        if not self.ready or not PAYMENT_ID.fullmatch(order["payment_id"]):
            raise PaymentError("payment_not_created")
        self.db._run(
            "update orders set payment_checked_at=? where id=?", (now(), identifier)
        )
        payment = await self.call("GET", "payments/" + order["payment_id"])
        if not self.matches(payment, order):
            raise PaymentError("payment_mismatch")
        if payment.get("status") == "succeeded" and payment.get("paid") is True:
            result = fulfill_order(
                self.db,
                order,
                "test" if order["payment_test"] else "real",
                order["payment_id"],
                notify_admins=self.cfg.admins,
            )
            if result not in {"activated", "complete"}:
                raise PaymentError("order_unavailable")
            return "complete"
        if payment.get("status") == "canceled":
            self.db._run(
                "update orders set status='canceled' where id=? and status='pending'",
                (identifier,),
            )
            return "canceled"
        return "pending"

    def register(self, app):
        app.router.add_post("/payments/yookassa", self.webhook)
        app.on_startup.append(self.start)
        app.on_cleanup.append(self.close)

    async def webhook(self, request):
        if not take_request(self.rates, request.remote or "unknown", 30):
            raise web.HTTPTooManyRequests(headers={"Retry-After": "60"})
        try:
            body = await request.json()
            if (
                not isinstance(body, dict)
                or body.get("type") != "notification"
                or body.get("event") not in {"payment.succeeded", "payment.canceled"}
            ):
                raise ValueError
            payment_id = body.get("object", {}).get("id", "")
            if not isinstance(payment_id, str) or not PAYMENT_ID.fullmatch(payment_id):
                raise ValueError
        except (ValueError, AttributeError, TypeError):
            raise web.HTTPBadRequest() from None
        order = self.db.one("select id from orders where payment_id=?", (payment_id,))
        if not order:
            return web.json_response({"ok": True})
        try:
            await self.verify(order["id"])
        except PaymentError:
            raise web.HTTPServiceUnavailable() from None
        return web.json_response({"ok": True})

    async def reconcile(self):
        while True:
            orders = self.db.all(
                "select id from orders where mode='yookassa' and status='pending' and payment_id!='' and created_at>? order by payment_checked_at,created_at limit 100",
                (now() - 7 * 86400,),
            )
            for order in orders:
                try:
                    await self.verify(order["id"])
                except PaymentError:
                    LOG.warning("Payment verification deferred")
            await asyncio.sleep(30)

    async def start(self, app):
        if self.ready:
            self.worker = asyncio.create_task(self.reconcile())

    async def close(self, app=None):
        if self.worker:
            self.worker.cancel()
            with suppress(asyncio.CancelledError):
                await self.worker
        await self.http.aclose()
        await self.bridge_http.aclose()
