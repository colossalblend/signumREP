from __future__ import annotations

import asyncio
import html
import ipaddress
import json
import logging
import re
import secrets
import shutil
from contextlib import suppress
from pathlib import Path
from typing import TYPE_CHECKING

from aiohttp import web
from cachetools import TTLCache

from . import ota
from .commerce import API_TOPUPS, create_order, key_digest
from .payments import PaymentError
from .db import now
from .ipatool import IpatoolError, NoAccount
from .ownership import OwnedCatalog
from .rate_limits import take_request
from .api_context import ApiError, KEY_ID, OWNER, SCOPES
from .api_auth import AppleApiAuth
from .api_keys import create_key, keys_for, update_key
from .production import reserve_storage, release_storage, StorageBusy

if TYPE_CHECKING:
    from .handlers import Deps

LOG = logging.getLogger("signum-bot.api")


class CustomerApi:
    def __init__(self, deps: Deps):
        self.deps = deps
        self.db, self.cfg = deps.db, deps.cfg
        self.rates = TTLCache(maxsize=4096, ttl=60)
        self.ip_rates = TTLCache(maxsize=4096, ttl=60)
        self.active_requests: dict[int, int] = {}
        self.job_tasks: dict[str, asyncio.Task] = {}
        self.tasks: set[asyncio.Task] = set()
        self.slots = asyncio.Semaphore(self.cfg.concurrency)
        self.owned = OwnedCatalog(deps.tool)
        self.auth = AppleApiAuth(deps)
        self.global_rates = TTLCache(maxsize=1, ttl=60)
        self.inflight = 0
        self.trusted_proxies = tuple(
            ipaddress.ip_network(value) for value in self.cfg.api_trusted_proxies
        )

    def authenticate(self, request: web.Request) -> int:
        authorization = request.headers.get("Authorization", "")
        if not re.fullmatch(r"Bearer sg_[A-Za-z0-9_-]{43}", authorization):
            raise ApiError(401, "invalid_api_key")
        digest = key_digest(authorization[7:])
        user = self.db.one(
            "select u.*, k.expires_at key_until,k.scopes,k.id key_id,k.state key_state from api_keys k join users u on u.tg_id=k.tg_id where k.digest=?",
            (digest,),
        )
        if not user:
            raise ApiError(401, "invalid_api_key")
        if user["key_until"] <= now():
            raise ApiError(401, "api_key_expired")
        if user["key_state"] == "revoked":
            raise ApiError(401, "api_key_revoked")
        if user["key_state"] == "paused":
            raise ApiError(403, "api_key_paused")
        if not user["api_enabled"]:
            raise ApiError(403, "api_paused")
        if user["blocked"] or user["api_until"] <= now():
            raise ApiError(403, "api_subscription_required")
        if not take_request(self.rates, user["tg_id"], self.cfg.api_rate_limit):
            raise ApiError(429, "rate_limit_exceeded")
        scopes = frozenset(user["scopes"].split())
        capability = (
            "accounts"
            if request.path.startswith(("/api/v1/auth/", "/api/v1/accounts"))
            else "keys"
            if request.path.startswith("/api/v1/keys")
            else "billing"
            if request.path.startswith("/api/v1/billing/")
            else "downloads"
        )
        if "read" not in scopes or (
            request.method not in ("GET", "HEAD") and capability not in scopes
        ):
            raise ApiError(403, "insufficient_scope")
        request[SCOPES] = scopes
        request[KEY_ID] = user["key_id"]
        self.db._run(
            "update api_keys set last_used=? where digest=? and last_used<?",
            (now(), digest, now() - 60),
        )
        return int(user["tg_id"])

    @web.middleware
    async def middleware(self, request, handler):
        if not request.path.startswith("/api/"):
            return await handler(request)
        owner = None
        admitted = False
        try:
            if (
                not take_request(self.global_rates, "all", self.cfg.api_global_rate)
                or self.inflight >= self.cfg.api_global_requests
            ):
                raise ApiError(503, "server_busy")
            self.inflight += 1
            admitted = True
            if request.path.startswith("/api/v1/") and (
                len(request.query_string) > 2048 or (request.content_length or 0) > 4096
            ):
                raise ApiError(413, "request_too_large")
            remote = request.remote or "unknown"
            trusted = False
            try:
                peer = ipaddress.ip_address(remote)
                trusted = any(peer in network for network in self.trusted_proxies)
                if trusted and request.headers.get("X-Real-IP"):
                    remote = str(ipaddress.ip_address(request.headers["X-Real-IP"]))
            except ValueError:
                raise ApiError(400, "invalid_peer_address") from None
            if not take_request(self.ip_rates, remote, 120):
                raise ApiError(429, "ip_rate_limit_exceeded")
            if request.path not in ("/api/docs", "/api/schema.json", "/api/panel"):
                if (
                    not request.secure
                    and not (
                        trusted and request.headers.get("X-Forwarded-Proto") == "https"
                    )
                    and not ipaddress.ip_address(remote).is_loopback
                ):
                    raise ApiError(426, "https_required")
                request[OWNER] = self.authenticate(request)
                owner = request[OWNER]
                from .production import maintenance_enabled

                if (
                    maintenance_enabled(self.db, "api")
                    and owner not in self.cfg.admins
                    and not (
                        request.method == "GET"
                        and request.path.startswith("/api/v1/billing/orders/")
                    )
                ):
                    raise ApiError(503, "maintenance")
                if self.active_requests.get(owner, 0) >= 3:
                    owner = None
                    raise ApiError(429, "too_many_active_requests")
                self.active_requests[owner] = self.active_requests.get(owner, 0) + 1
            async with asyncio.timeout(60):
                response = await handler(request)
            response.headers.update(
                {
                    "Cache-Control": "no-store",
                    "X-Content-Type-Options": "nosniff",
                    "Referrer-Policy": "no-referrer",
                    "X-Frame-Options": "DENY",
                }
            )
            return response
        except ApiError as exc:
            return web.json_response(
                {"error": exc.code},
                status=exc.status,
                headers={"Cache-Control": "no-store", "Retry-After": "60"}
                if exc.status == 429
                else {"Cache-Control": "no-store"},
            )
        except NoAccount:
            return web.json_response(
                {
                    "error": "apple_id_required",
                    "message": "Подключите Apple ID через /api/v1/auth/jobs.",
                },
                status=409,
            )
        except TimeoutError:
            return web.json_response({"error": "request_timeout"}, status=504)
        except IpatoolError:
            return web.json_response(
                {"error": "apple_temporarily_unavailable"}, status=502
            )
        except web.HTTPException as exc:
            return web.json_response({"error": "request_rejected"}, status=exc.status)
        except Exception as exc:
            LOG.warning("API request failed (%s)", type(exc).__name__)
            return web.json_response({"error": "temporarily_unavailable"}, status=503)
        finally:
            if admitted:
                self.inflight -= 1
            if owner is not None:
                count = self.active_requests.get(owner, 1) - 1
                if count:
                    self.active_requests[owner] = count
                else:
                    self.active_requests.pop(owner, None)

    def register(self, app: web.Application):
        app.middlewares.append(self.middleware)
        app.router.add_get("/api/docs", self.docs)
        app.router.add_get("/api/panel", self.panel)
        app.router.add_get("/api/schema.json", self.schema)
        app.router.add_get("/api/v1/me", self.me)
        app.router.add_get("/api/v1/catalog", self.catalog)
        app.router.add_get("/api/v1/catalog/{slug}", self.catalog_app)
        app.router.add_get("/api/v1/search", self.search)
        app.router.add_get("/api/v1/developers", self.developers)
        app.router.add_get("/api/v1/developers/{artist_id}/apps", self.developer_apps)
        app.router.add_get("/api/v1/apps/{adam_id}", self.lookup)
        app.router.add_get("/api/v1/apps/{adam_id}/versions", self.versions)
        app.router.add_post("/api/v1/downloads", self.download)
        app.router.add_get("/api/v1/downloads/{job_id}", self.job)
        app.router.add_delete("/api/v1/downloads/{job_id}", self.delete_job)
        app.router.add_get("/api/v1/billing/packages", self.billing_packages)
        app.router.add_post("/api/v1/billing/orders", self.billing_order)
        app.router.add_get("/api/v1/billing/orders/{order_id}", self.billing_status)
        app.router.add_get("/api/v1/keys", self.key_list)
        app.router.add_post("/api/v1/keys", self.key_create)
        app.router.add_patch("/api/v1/keys/{key_id}", self.key_update)
        app.router.add_delete("/api/v1/keys/{key_id}", self.key_delete)
        self.auth.register(app)
        payments = getattr(self.deps, "payments", None)
        if payments:
            payments.register(app)
        app.on_startup.append(self.recover)
        app.on_cleanup.append(self.close)

    async def docs(self, request):
        nonce = secrets.token_urlsafe(18)
        metadata = {
            "base": self.cfg.public_base_url,
            "downloads": self.cfg.api_downloads,
            "topups": [
                {"plan": plan, "downloads": units, "price": self.cfg.prices[plan]}
                for plan, units in API_TOPUPS.items()
            ],
            "payment_test": self.cfg.payment_mode == "yookassa"
            and self.cfg.yookassa_secret_key.startswith("test_"),
            "price": self.cfg.prices["api"],
            "rate": self.cfg.api_rate_limit,
            "ttl": self.cfg.link_ttl_paid_hours,
        }
        page = (Path(__file__).parent / "assets" / "api-docs.html").read_text(
            encoding="utf-8"
        )
        page = page.replace("@@NONCE@@", html.escape(nonce)).replace(
            "@@METADATA@@",
            json.dumps(metadata, ensure_ascii=False).replace("<", "\\u003c"),
        )
        return web.Response(
            text=page,
            content_type="text/html",
            headers={
                "Content-Security-Policy": f"default-src 'none'; script-src 'nonce-{nonce}'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"
            },
        )

    async def panel(self, request):
        nonce = secrets.token_urlsafe(18)
        page = (
            (Path(__file__).parent / "assets" / "api-panel.html")
            .read_text(encoding="utf-8")
            .replace("@@NONCE@@", nonce)
        )
        return web.Response(
            text=page,
            content_type="text/html",
            headers={
                "Content-Security-Policy": f"default-src 'none'; script-src 'nonce-{nonce}'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"
            },
        )

    def management_key(self, request):
        row = self.db.one("select * from api_keys where id=?", (request[KEY_ID],))
        if row["quota_remaining"] is not None:
            raise ApiError(403, "limited_key_cannot_manage_keys")
        return row

    async def key_list(self, request):
        return web.json_response(
            {
                "keys": [dict(row) for row in keys_for(self.db, request[OWNER])],
                "current_key_id": request[KEY_ID],
            }
        )

    async def key_create(self, request):
        parent = self.management_key(request)
        data = await self.auth.body(request)
        rights = data.get("scopes", ["read", "downloads", "accounts"])
        if (
            not isinstance(rights, list)
            or not all(isinstance(right, str) for right in rights)
            or not set(rights) <= request[SCOPES]
        ):
            raise ApiError(403, "insufficient_scope")
        days = data.get("days", 30)
        if type(days) is not int or not 1 <= days <= 365:
            raise ApiError(422, "invalid_expiry")
        try:
            identifier, secret = create_key(
                self.db,
                request[OWNER],
                data.get("label", "Мой ключ"),
                scopes=rights,
                expires_at=min(parent["expires_at"], now() + days * 86400),
                quota=data.get("quota"),
            )
        except ValueError as exc:
            raise ApiError(422, str(exc)) from None
        return web.json_response(
            {
                "key_id": identifier,
                "api_key": secret,
                "message": "Сохраните ключ. Он показан один раз.",
            },
            status=201,
        )

    async def key_update(self, request):
        parent = self.management_key(request)
        changes = await self.auth.body(request)
        if "expires_at" in changes and (
            type(changes["expires_at"]) is not int
            or changes["expires_at"] > parent["expires_at"]
        ):
            raise ApiError(422, "invalid_expiry")
        target = self.db.one(
            "select scopes from api_keys where id=? and tg_id=?",
            (request.match_info["key_id"], request[OWNER]),
        )
        if target and not set(target["scopes"].split()) <= request[SCOPES]:
            raise ApiError(403, "insufficient_scope")
        try:
            row = update_key(
                self.db, request[OWNER], request.match_info["key_id"], changes
            )
        except ValueError as exc:
            raise ApiError(
                404 if str(exc) == "key_not_found" else 422, str(exc)
            ) from None
        return web.json_response(dict(row))

    async def key_delete(self, request):
        self.management_key(request)
        try:
            row = update_key(
                self.db,
                request[OWNER],
                request.match_info["key_id"],
                {"state": "revoked"},
            )
        except ValueError as exc:
            raise ApiError(
                404 if str(exc) == "key_not_found" else 422, str(exc)
            ) from None
        return web.json_response(dict(row))

    async def schema(self, request):
        return web.json_response(
            {
                "name": "Signum API",
                "version": "1",
                "base_url": self.cfg.public_base_url + "/api/v1",
                "authentication": "Authorization: Bearer <key from /api in Telegram>",
                "limits": {
                    "requests_per_minute": self.cfg.api_rate_limit,
                    "downloads_per_purchase": self.cfg.api_downloads,
                    "max_file_mb": self.cfg.max_mb_sub,
                    "link_hours": self.cfg.link_ttl_paid_hours,
                    "active_jobs_per_user": 1,
                    "accounts_per_user": self.cfg.api_accounts_max,
                    "login_job_seconds": self.cfg.api_auth_ttl,
                    "login_attempts_per_hour": 10,
                    "active_http_requests": self.cfg.api_global_requests,
                    "requests_per_minute_global": self.cfg.api_global_rate,
                },
                "endpoints": {
                    "GET /me": "API expiry, remaining downloads, Apple ID connection status",
                    "GET /catalog": "Catalogue apps",
                    "GET /catalog/{slug}": "Only builds owned by your connected Apple ID",
                    "GET /search?q=name": "App Store search",
                    "GET /apps/{adam_id}": "App metadata",
                    "GET /apps/{adam_id}/versions": "Owned version IDs",
                    "POST /downloads": {
                        "headers": {
                            "Idempotency-Key": "unique request ID, 8-128 characters"
                        },
                        "json": {
                            "adam_id": "123456",
                            "kind": "ipa or ota",
                            "version_id": "optional",
                        },
                        "result": "202: job_id, status; repeated request returns same job",
                    },
                    "GET /downloads/{job_id}": "Status, file_url, install_url, manifest_url, itms_services_url when ready",
                    "DELETE /downloads/{job_id}": "Cancel pending job or revoke ready file links",
                    "GET /billing/packages": "Additional download packages and RUB prices",
                    "POST /billing/orders": {
                        "headers": {
                            "Idempotency-Key": "unique order ID, 8-128 characters"
                        },
                        "json": {"plan": "api100, api500 or api1000"},
                        "result": "201: order_id, amount_rub, payment_url, test; billing scope required",
                    },
                    "GET /billing/orders/{order_id}": "Verified payment status and purchased downloads",
                    "POST /auth/jobs": {
                        "headers": {"Idempotency-Key": "unique login request ID"},
                        "json": {"email": "you@example.com", "password": "password"},
                        "result": "202: job_id, status, action, expires_at",
                    },
                    "GET /auth/jobs/{job_id}": "Login status; ready gives account_id, requires_2fa gives challenge_token",
                    "POST /auth/jobs/{job_id}/code": {
                        "json": {
                            "code": "123456",
                            "challenge_token": "token from status",
                        },
                        "result": "202: verifying_2fa; same API key required",
                    },
                    "DELETE /auth/jobs/{job_id}": "Cancel unfinished login",
                    "GET /accounts": "Own API Apple IDs; independent from bot accounts",
                    "DELETE /accounts/{account_id}": "Disconnect API Apple ID",
                    "GET /accounts/{account_id}": "Only an account created by this API key",
                    "POST /accounts/{account_id}/restore": "Select a ready account by ID using its original API key",
                    "GET /developers?q=name": "Find developers in Russian and US App Store",
                    "GET /developers/{artist_id}/apps": "List developer applications",
                    "GET /keys": "Own key metadata without secrets or hashes",
                    "POST /keys": {
                        "json": {
                            "label": "My site",
                            "scopes": ["read", "downloads", "accounts"],
                            "quota": 100,
                            "days": 30,
                        },
                        "result": "201: key_id, api_key (shown once); keys scope and unlimited management key required",
                    },
                    "PATCH /keys/{key_id}": "Change state, quota_remaining, expires_at, label",
                    "DELETE /keys/{key_id}": "Permanently revoke key",
                },
                "examples": {
                    "create_download": "curl -H 'Authorization: Bearer sg_...' -H 'Idempotency-Key: download-001' -H 'Content-Type: application/json' -d '{\"adam_id\":\"123456\",\"kind\":\"ipa\"}' "
                    + self.cfg.public_base_url
                    + "/api/v1/downloads"
                },
                "errors": {
                    "401": "Invalid or revoked key",
                    "403": "API subscription expired or account blocked",
                    "409": "Connect Apple ID, job already running, or idempotency conflict",
                    "422": "Invalid input",
                    "429": "Request or download quota exceeded",
                    "503": "Queue full or temporary failure",
                },
            },
            headers={"Cache-Control": "no-store"},
        )

    async def me(self, request):
        tg_id = request[OWNER]
        user = self.db.one(
            "select api_until, api_remaining,api_extra_remaining from users where tg_id=?",
            (tg_id,),
        )
        account = self.db.account_of(tg_id)
        api_accounts = self.db.all(
            "select id,email,status from api_accounts where tg_id=? and key_id=?",
            (tg_id, request[KEY_ID]),
        )
        key = self.db.one(
            "select id,label,quota_remaining,state,expires_at from api_keys where id=?",
            (request[KEY_ID],),
        )
        return web.json_response(
            {
                "api_until": user["api_until"],
                "downloads_remaining": user["api_remaining"]
                + user["api_extra_remaining"],
                "package_downloads_remaining": user["api_remaining"],
                "extra_downloads_remaining": user["api_extra_remaining"],
                "key_scopes": sorted(request[SCOPES]),
                "apple_id_connected": any(
                    row["status"] == "ready" for row in api_accounts
                )
                or bool(account and account["status"] == "ready"),
                "accounts_connected": sum(
                    row["status"] == "ready" for row in api_accounts
                ),
                "key": dict(key),
            }
        )

    async def billing_packages(self, request):
        return web.json_response(
            {
                "currency": "RUB",
                "packages": [
                    {
                        "plan": plan,
                        "downloads": units,
                        "amount_rub": self.cfg.prices[plan],
                    }
                    for plan, units in API_TOPUPS.items()
                ],
                "extends_api": False,
                "unused_extras_carry_over": True,
            }
        )

    async def billing_order(self, request):
        payments = getattr(self.deps, "payments", None)
        if not payments or not payments.ready:
            raise ApiError(503, "payments_disabled")
        data = await self.auth.body(request)
        if (
            not isinstance(data, dict)
            or not isinstance(data.get("plan"), str)
            or data["plan"] not in API_TOPUPS
        ):
            raise ApiError(422, "invalid_package")
        request_key = request.headers.get("Idempotency-Key", "")
        if not re.fullmatch(r"[A-Za-z0-9_.:-]{8,128}", request_key):
            raise ApiError(422, "idempotency_key_required")
        tg_id = request[OWNER]
        order = self.db.one(
            "select * from orders where tg_id=? and request_key=?", (tg_id, request_key)
        )
        if order and order["plan"] != data["plan"]:
            raise ApiError(409, "idempotency_conflict")
        identifier = (
            order["id"]
            if order
            else create_order(self.cfg, self.db, tg_id, data["plan"], request_key)
        )
        try:
            url = await payments.checkout(identifier, tg_id)
        except PaymentError:
            raise ApiError(503, "payment_provider_unavailable") from None
        order = self.db.one("select * from orders where id=?", (identifier,))
        return web.json_response(
            {
                "order_id": identifier,
                "status": order["status"],
                "downloads": order["units"],
                "amount_rub": f"{order['amount'] / 100:.2f}",
                "currency": "RUB",
                "payment_url": url,
                "test": bool(order["payment_test"]),
            },
            status=201,
        )

    async def billing_status(self, request):
        order = self.db.one(
            "select * from orders where id=? and tg_id=? and plan in ('api100','api500','api1000')",
            (request.match_info["order_id"], request[OWNER]),
        )
        if not order:
            raise ApiError(404, "order_not_found")
        payments = getattr(self.deps, "payments", None)
        if not payments:
            raise ApiError(503, "payments_disabled")
        try:
            status = await payments.verify(order["id"], request[OWNER])
        except PaymentError:
            raise ApiError(503, "payment_verification_unavailable") from None
        return web.json_response(
            {
                "order_id": order["id"],
                "status": status,
                "downloads": order["units"],
                "test": bool(order["payment_test"]),
            }
        )

    async def catalog(self, request):
        return web.json_response(await self.deps.store.catalog())

    async def catalog_app(self, request):
        slug = request.match_info["slug"]
        if not re.fullmatch(r"[a-zA-Z0-9_-]{1,80}", slug):
            raise ApiError(422, "invalid_app_slug")
        account = self.account_for(
            request[OWNER], request.query.get("account_id"), request[KEY_ID]
        )
        info = await self.deps.store.catalog_app(slug)
        result = await self.owned.scan(account, info.get("versions") or [])
        return web.json_response(
            {**info, "versions": result.versions, "unchecked": result.unchecked}
        )

    async def search(self, request):
        term = request.query.get("q", "").strip()
        if not term or len(term) > 200:
            raise ApiError(422, "invalid_search")
        return web.json_response({"apps": await self.deps.store.search(term, limit=24)})

    async def developers(self, request):
        term = request.query.get("q", "").strip()
        if not term or len(term) > 200:
            raise ApiError(422, "invalid_search")
        return web.json_response({"developers": await self.deps.store.developers(term)})

    async def developer_apps(self, request):
        artist = request.match_info["artist_id"]
        if not re.fullmatch(r"[0-9]{1,20}", artist):
            raise ApiError(422, "invalid_developer")
        return web.json_response({"apps": await self.deps.store.developer_apps(artist)})

    def adam_id(self, request):
        identifier = request.match_info["adam_id"]
        if not re.fullmatch(r"[0-9]{1,20}", identifier):
            raise ApiError(422, "invalid_app_id")
        return identifier

    def account_for(self, owner, identifier=None, key_id=None):
        if identifier:
            account = self.auth.engine_account(owner, identifier, key_id)
            if account["status"] != "ready":
                raise ApiError(409, "account_not_ready")
            return account
        rows = self.db.all(
            "select id from api_accounts where tg_id=? and status='ready' and (? is null or key_id=?)",
            (owner, key_id, key_id),
        )
        if len(rows) > 1:
            raise ApiError(409, "account_selection_required")
        if rows:
            return self.auth.engine_account(owner, rows[0]["id"], key_id)
        return self.deps.tool.for_user(owner)

    async def lookup(self, request):
        app = await self.deps.store.lookup(self.adam_id(request))
        if not app:
            raise ApiError(404, "app_not_found")
        return web.json_response(app)

    async def versions(self, request):
        account = self.account_for(
            request[OWNER], request.query.get("account_id"), request[KEY_ID]
        )
        return web.json_response(
            {"versions": await self.deps.tool.versions(account, self.adam_id(request))}
        )

    async def download(self, request):
        if not self.cfg.public_base_url:
            raise ApiError(503, "public_url_not_configured")
        data = await self.auth.body(request)
        adam = str(data.get("adam_id", ""))
        version = str(data.get("version_id") or "")
        kind = data.get("kind", "ipa")
        account_id = data.get("account_id", "")
        if (
            not re.fullmatch(r"[0-9]{1,20}", adam)
            or (version and not re.fullmatch(r"[0-9]{1,20}", version))
            or kind not in ("ipa", "ota")
            or not isinstance(account_id, str)
            or len(account_id) > 64
        ):
            raise ApiError(422, "invalid_download")
        request_key = request.headers.get("Idempotency-Key", "")
        if not re.fullmatch(r"[A-Za-z0-9_.:-]{8,128}", request_key):
            raise ApiError(422, "idempotency_key_required")
        tg_id = request[OWNER]
        old = self.db.one(
            "select * from api_jobs where tg_id=? and request_key=?",
            (tg_id, request_key),
        )
        if old:
            if old["key_id"] != request[KEY_ID]:
                raise ApiError(409, "idempotency_conflict")
            if (old["adam_id"], old["version_id"], old["kind"]) != (
                adam,
                version,
                kind,
            ) or (account_id and account_id != old["account_id"]):
                raise ApiError(409, "idempotency_conflict")
            return web.json_response(
                {"job_id": old["id"], "status": old["status"]}, status=202
            )
        account = self.account_for(tg_id, account_id, request[KEY_ID])
        account_id = account["api_id"] if "api_id" in account.keys() else ""
        identifier = secrets.token_urlsafe(16)
        with self.db.conn:
            active = self.db.conn.execute(
                "select tg_id from api_jobs where status in ('queued','downloading')"
            ).fetchall()
            if any(row["tg_id"] == tg_id for row in active):
                raise ApiError(409, "download_already_running")
            if len(active) >= 10:
                raise ApiError(503, "download_queue_full")
            key = self.db.conn.execute(
                "select quota_remaining from api_keys where id=?", (request[KEY_ID],)
            ).fetchone()
            key_reserved = key["quota_remaining"] is not None
            if key_reserved:
                changed = self.db.conn.execute(
                    "update api_keys set quota_remaining=quota_remaining-1 where id=? and quota_remaining>0 and state='active'",
                    (request[KEY_ID],),
                )
                if changed.rowcount != 1:
                    raise ApiError(429, "key_quota_exceeded")
            source = "base"
            used = self.db.conn.execute(
                "update users set api_remaining=api_remaining-1 where tg_id=? and api_remaining>0 and api_until>? and blocked=0",
                (tg_id, now()),
            )
            if used.rowcount != 1:
                source = "extra"
                used = self.db.conn.execute(
                    "update users set api_extra_remaining=api_extra_remaining-1 where tg_id=? and api_extra_remaining>0 and api_until>? and blocked=0",
                    (tg_id, now()),
                )
                if used.rowcount != 1:
                    raise ApiError(429, "download_quota_exceeded")
            self.db.conn.execute(
                "insert into api_jobs(id,tg_id,request_key,adam_id,version_id,kind,created_at,quota_source,key_id,key_reserved,account_id) values (?,?,?,?,?,?,?,?,?,?,?)",
                (
                    identifier,
                    tg_id,
                    request_key,
                    adam,
                    version,
                    kind,
                    now(),
                    source,
                    request[KEY_ID],
                    int(key_reserved),
                    account_id,
                ),
            )
        task = asyncio.create_task(self.run_job(identifier))
        self.tasks.add(task)
        self.job_tasks[identifier] = task
        task.add_done_callback(self.tasks.discard)
        task.add_done_callback(lambda _: self.job_tasks.pop(identifier, None))
        return web.json_response({"job_id": identifier, "status": "queued"}, status=202)

    async def job(self, request):
        row = self.db.one(
            "select * from api_jobs where id=? and tg_id=? and key_id=?",
            (request.match_info["job_id"], request[OWNER], request[KEY_ID]),
        )
        if not row:
            raise ApiError(404, "job_not_found")
        result = {
            "job_id": row["id"],
            "status": row["status"],
            "error": row["error"] or None,
        }
        file = self.db.file(row["file_token"]) if row["file_token"] else None
        if (
            file
            and file["expires_at"] > now()
            and (file["remote"] or Path(file["path"]).is_file())
        ):
            result.update(
                file_url=f"{self.cfg.public_base_url}/f/{file['token']}.ipa",
                expires_at=file["expires_at"],
                title=file["title"],
                size=file["size"],
            )
            if row["kind"] == "ota":
                result.update(
                    ota.installation_links(self.cfg.public_base_url, file["token"])
                )
        elif row["status"] == "ready":
            result["status"] = "expired"
        return web.json_response(result, headers={"Cache-Control": "no-store"})

    async def delete_job(self, request):
        identifier = request.match_info["job_id"]
        row = self.db.one(
            "select * from api_jobs where id=? and tg_id=? and key_id=?",
            (identifier, request[OWNER], request[KEY_ID]),
        )
        if not row:
            raise ApiError(404, "job_not_found")
        task = self.job_tasks.get(identifier)
        if task:
            task.cancel()
            with suppress(asyncio.CancelledError):
                await task
        file = self.db.file(row["file_token"]) if row["file_token"] else None
        if file:
            if file["remote"]:
                from .bridge import revoke_file

                await revoke_file(self.cfg, file["token"])
            self.db.file_delete(file["token"])
            await asyncio.to_thread(shutil.rmtree, self.directory(identifier), True)
        self.db._run(
            "update api_jobs set status='revoked',file_token='' where id=?",
            (identifier,),
        )
        return web.json_response({"job_id": identifier, "status": "revoked"})

    def fail_job(self, identifier, error):
        with self.db.conn:
            row = self.db.conn.execute(
                "select tg_id,reserved,quota_source,key_id,key_reserved from api_jobs where id=?",
                (identifier,),
            ).fetchone()
            if row and row["reserved"]:
                if row["key_reserved"]:
                    self.db.conn.execute(
                        "update api_keys set quota_remaining=quota_remaining+1 where id=? and quota_remaining is not null",
                        (row["key_id"],),
                    )
                field = (
                    "api_extra_remaining"
                    if row["quota_source"] == "extra"
                    else "api_remaining"
                )
                self.db.conn.execute(
                    f"update users set {field}={field}+1 where tg_id=?",
                    (row["tg_id"],),
                )
                self.db.conn.execute(
                    "update api_jobs set status='failed',error=?,reserved=0,key_reserved=0 where id=?",
                    (error, identifier),
                )

    def directory(self, identifier):
        base = self.cfg.files_dir.resolve()
        path = (base / ("api-" + identifier)).resolve()
        if path.parent != base:
            raise ValueError("Invalid API job directory")
        return path

    async def run_job(self, identifier):
        directory = self.directory(identifier)
        token = None
        try:
            async with self.slots:
                row = self.db.one("select * from api_jobs where id=?", (identifier,))
                account = self.account_for(
                    row["tg_id"], row["account_id"], row["key_id"]
                )
                if self.db.blocked(row["tg_id"]):
                    raise ApiError(403, "account_blocked")
                if row["key_id"] and not self.auth.allowed(row["tg_id"], row["key_id"]):
                    raise ApiError(403, "api_key_unavailable")
                self.db._run(
                    "update api_jobs set status='downloading' where id=?", (identifier,)
                )
                meta = await self.deps.store.lookup(row["adam_id"]) or {
                    "adam_id": row["adam_id"],
                    "title": row["adam_id"],
                }
                size_limit = self.cfg.max_bytes(True)
                reserve_storage(self.cfg, self.db, "api:" + identifier, size_limit * 2)
                if int(meta.get("size") or 0) > size_limit:
                    raise ApiError(422, "file_too_large")
                await self.deps.tool.revalidate(account)
                directory.mkdir(parents=True, exist_ok=True)
                proc = None
                limit_hit = False

                def started(value):
                    nonlocal proc
                    proc = value

                async def watch_size():
                    nonlocal limit_hit
                    while True:
                        await asyncio.sleep(1)
                        size = sum(
                            path.stat().st_size
                            for path in directory.iterdir()
                            if path.is_file()
                        )
                        if size > size_limit and proc and proc.returncode is None:
                            limit_hit = True
                            proc.kill()
                            return

                watcher = asyncio.create_task(watch_size())
                try:
                    ipa = await self.deps.tool.download(
                        account,
                        row["adam_id"],
                        external_version_id=row["version_id"] or None,
                        out_dir=directory,
                        on_start=started,
                    )
                except IpatoolError:
                    if limit_hit:
                        raise ApiError(422, "file_too_large") from None
                    raise
                finally:
                    watcher.cancel()
                    with suppress(asyncio.CancelledError):
                        await watcher
                if ipa.resolve().parent != directory:
                    raise ApiError(500, "invalid_download_path")
                if ipa.stat().st_size > size_limit:
                    raise ApiError(422, "file_too_large")
                signed = False
                if row["kind"] == "ota":
                    await asyncio.to_thread(ota.repack, ipa)
                    signed = await ota.sign(self.cfg, ipa)
                if ipa.stat().st_size > self.cfg.max_bytes(True):
                    raise ApiError(422, "file_too_large")
                facts = await asyncio.to_thread(ota.ipa_facts, ipa)
                if row["key_id"] and not self.auth.allowed(row["tg_id"], row["key_id"]):
                    raise ApiError(403, "api_key_unavailable")
                self.db.app_save(meta)
                token = secrets.token_urlsafe(24)
                self.db.file_add(
                    token,
                    adam_id=row["adam_id"],
                    account_id=account["id"],
                    tg_id=row["tg_id"],
                    name=ipa.name,
                    path=str(ipa),
                    size=ipa.stat().st_size,
                    title=meta.get("title") or row["adam_id"],
                    bundle_id=facts["bundle_id"] or meta.get("bundle_id", ""),
                    version=facts["version"] or meta.get("version", ""),
                    signed=int(signed),
                    encrypted=int(facts["encrypted"]),
                    paid_with="api",
                    ttl_hours=self.cfg.link_hours("api"),
                    expires_at=ota.expiry(self.cfg, "api"),
                )
                if self.cfg.bridge_enabled:
                    from .bridge import publish_file

                    await publish_file(self.cfg, self.db, token)
                    ipa.unlink()
                self.db.history_add(
                    row["tg_id"],
                    row["adam_id"],
                    meta.get("title") or row["adam_id"],
                    row["kind"],
                    account["id"],
                    "ok",
                    "api",
                )
                self.db._run(
                    "update api_jobs set status='ready',file_token=?,reserved=0,key_reserved=0 where id=?",
                    (token, identifier),
                )
        except asyncio.CancelledError:
            self.fail_job(identifier, "interrupted_retry_with_new_key")
            shutil.rmtree(directory, ignore_errors=True)
            raise
        except Exception as exc:
            code = (
                exc.code
                if isinstance(exc, ApiError)
                else "connect_apple_id_in_bot"
                if isinstance(exc, NoAccount)
                else "storage_unavailable"
                if isinstance(exc, StorageBusy)
                else "download_failed"
            )
            LOG.warning("API job failed (%s)", type(exc).__name__)
            if token:
                self.db.file_delete(token)
            self.fail_job(identifier, code)
            shutil.rmtree(directory, ignore_errors=True)

        finally:
            release_storage(self.db, "api:" + identifier)

    async def recover(self, app):
        self.db._run("delete from storage_reservations where id like 'api:%'")
        for row in self.db.all("select id from api_jobs where reserved=1"):
            self.fail_job(row["id"], "interrupted_retry_with_new_key")
            shutil.rmtree(self.directory(row["id"]), ignore_errors=True)

    async def close(self, app):
        tasks = list(self.tasks)
        for task in tasks:
            task.cancel()
        for task in tasks:
            with suppress(asyncio.CancelledError):
                await task
        await self.recover(app)
