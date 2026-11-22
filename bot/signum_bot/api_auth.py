from __future__ import annotations

import asyncio
import hmac
import logging
import re
import secrets
import shutil
from contextlib import suppress

from aiohttp import web
from cachetools import TTLCache

from .api_context import ApiError, KEY_ID, OWNER
from .api_secrets import ApiSecrets
from .commerce import key_digest
from .db import now
from .ipatool import IpatoolError
from .rate_limits import take_request

ACTIVE = ("queued", "authenticating", "requires_2fa", "verifying_2fa")
MESSAGES = {
    "queued": "Вход ожидает очереди.",
    "authenticating": "Выполняется вход в Apple ID.",
    "requires_2fa": "Введите 6 цифр кода с устройства Apple или из SMS.",
    "verifying_2fa": "Проверяется код подтверждения.",
    "ready": "Apple ID подключён.",
    "failed": "Войти не удалось. Проверьте почту и пароль, затем создайте новый запрос.",
    "expired": "Время входа истекло. Начните заново.",
    "canceled": "Вход отменён.",
}


class AppleApiAuth:
    def __init__(self, deps):
        self.cfg, self.db, self.tool = deps.cfg, deps.db, deps.tool
        self.vault = ApiSecrets(self.cfg.data_dir)
        self.slots = asyncio.Semaphore(min(2, self.cfg.concurrency))
        self.tasks = {}
        self.challenges = TTLCache(maxsize=256, ttl=self.cfg.api_auth_ttl)
        self.rates = TTLCache(maxsize=4096, ttl=3600)
        self.code_rates = TTLCache(maxsize=4096, ttl=60)
        self.keeper = None
        self.warmer = None
        self.warm_retries = TTLCache(maxsize=4096, ttl=900)

    def register(self, app):
        app.router.add_post("/api/v1/auth/jobs", self.login)
        app.router.add_get("/api/v1/auth/jobs/{job_id}", self.status)
        app.router.add_post("/api/v1/auth/jobs/{job_id}/code", self.code)
        app.router.add_delete("/api/v1/auth/jobs/{job_id}", self.cancel)
        app.router.add_get("/api/v1/accounts", self.accounts)
        app.router.add_get("/api/v1/accounts/{account_id}", self.account_status)
        app.router.add_post("/api/v1/accounts/{account_id}/restore", self.restore)
        app.router.add_delete("/api/v1/accounts/{account_id}", self.delete_account)
        app.on_startup.append(self.start)
        app.on_cleanup.append(self.close)

    def allowed(self, owner, key_id):
        return (
            self.db.one(
                "select k.id from api_keys k join users u on u.tg_id=k.tg_id where k.id=? and k.tg_id=? and k.state='active' and k.expires_at>? and u.api_enabled=1 and u.api_until>? and u.blocked=0",
                (key_id, owner, now(), now()),
            )
            is not None
        )

    async def body(self, request):
        try:
            raw = await request.read()
            if len(raw) > 4096:
                raise ApiError(413, "request_too_large")
            import json

            data = json.loads(raw)
        except (ValueError, UnicodeError):
            raise ApiError(422, "invalid_json") from None
        if not isinstance(data, dict):
            raise ApiError(422, "invalid_json")
        return data

    def job_for(self, request):
        row = self.db.one(
            "select * from api_auth_jobs where id=? and tg_id=? and key_id=?",
            (request.match_info["job_id"], request[OWNER], request[KEY_ID]),
        )
        if not row:
            raise ApiError(404, "auth_job_not_found")
        if row["expires_at"] <= now() and row["status"] in ACTIVE:
            self.finish(row, "expired", "auth_expired")
            task = self.tasks.get(row["id"])
            if task:
                task.cancel()
            row = self.db.one("select * from api_auth_jobs where id=?", (row["id"],))
        return row

    def result(self, row):
        status = row["status"]
        result = {
            "job_id": row["id"],
            "status": status,
            "message": MESSAGES[status],
            "expires_at": row["expires_at"],
            "poll_after_seconds": 5 if status in ACTIVE else None,
            "action": "submit_2fa_code"
            if status == "requires_2fa"
            else "poll"
            if status in ACTIVE
            else "none",
            "error": row["error"] or None,
        }
        if status == "ready":
            result["account_id"] = row["account_id"]
        if status == "requires_2fa":
            token = self.challenges.get(row["id"])
            if token:
                result.update(
                    challenge_token=token,
                    code_url=f"/api/v1/auth/jobs/{row['id']}/code",
                    attempts_remaining=max(0, 3 - row["attempts"]),
                )
        return result

    async def login(self, request):
        data = await self.body(request)
        email, password = data.get("email"), data.get("password")
        if (
            not isinstance(email, str)
            or not re.fullmatch(
                r"[^\s@]{1,128}@[^\s@]{1,125}\.[^\s@]{1,63}", email.strip()
            )
            or len(email) > 254
            or not isinstance(password, str)
            or not 1 <= len(password) <= 256
        ):
            raise ApiError(422, "invalid_credentials_format")
        email = email.strip().casefold()
        request_key = request.headers.get("Idempotency-Key", "")
        if not re.fullmatch(r"[A-Za-z0-9_.:-]{8,128}", request_key):
            raise ApiError(422, "idempotency_key_required")
        owner = request[OWNER]
        fingerprint = self.vault.fingerprint(owner, email, password)
        old = self.db.one(
            "select * from api_auth_jobs where tg_id=? and request_key=?",
            (owner, request_key),
        )
        if old:
            if old["fingerprint"] != fingerprint or old["key_id"] != request[KEY_ID]:
                raise ApiError(409, "idempotency_conflict")
            return web.json_response(self.result(old), status=202)
        if not take_request(self.rates, owner, 10, period=3600):
            raise ApiError(429, "login_rate_limit_exceeded")
        self.expire_jobs()
        if (
            self.db.one(
                "select count(*) from api_auth_jobs where tg_id=? and status in ('queued','authenticating','requires_2fa','verifying_2fa')",
                (owner,),
            )[0]
            >= 2
        ):
            raise ApiError(429, "too_many_login_jobs")
        if (
            self.db.one(
                "select count(*) from api_auth_jobs where status in ('queued','authenticating','requires_2fa','verifying_2fa')"
            )[0]
            >= 32
        ):
            raise ApiError(503, "login_queue_full")
        account = self.db.one(
            "select * from api_accounts where tg_id=? and email=?", (owner, email)
        )
        if account and account["key_id"] != request[KEY_ID]:
            raise ApiError(404, "account_not_found")
        if account and self.db.one(
            "select id from api_auth_jobs where account_id=? and status in ('queued','authenticating','requires_2fa','verifying_2fa')",
            (account["id"],),
        ):
            raise ApiError(409, "login_already_running")
        if not account:
            if (
                self.db.one(
                    "select count(*) from api_accounts where tg_id=?", (owner,)
                )[0]
                >= self.cfg.api_accounts_max
            ):
                raise ApiError(429, "account_limit_exceeded")
            account_id = secrets.token_urlsafe(18)
            self.db._run(
                "insert into api_accounts(id,tg_id,engine_id,email,password_secret,keychain,created_at,key_id) values (?,?,?,?,?,?,?,?)",
                (
                    account_id,
                    owner,
                    -secrets.randbits(62) - 1,
                    email,
                    self.vault.encrypt(password),
                    secrets.token_urlsafe(24),
                    now(),
                    request[KEY_ID],
                ),
            )
            account = self.db.one(
                "select * from api_accounts where id=?", (account_id,)
            )
        elif account["status"] != "ready":
            self.db._run(
                "update api_accounts set password_secret=?,status='new' where id=?",
                (self.vault.encrypt(password), account["id"]),
            )
        identifier = secrets.token_urlsafe(32)
        status = "ready" if account["status"] == "ready" else "queued"
        self.db._run(
            "insert into api_auth_jobs(id,tg_id,account_id,key_id,request_key,fingerprint,status,created_at,expires_at) values (?,?,?,?,?,?,?,?,?)",
            (
                identifier,
                owner,
                account["id"],
                request[KEY_ID],
                request_key,
                fingerprint,
                status,
                now(),
                now() + self.cfg.api_auth_ttl,
            ),
        )
        if status == "queued":
            self.launch(identifier)
        return web.json_response(
            self.result(
                self.db.one("select * from api_auth_jobs where id=?", (identifier,))
            ),
            status=202,
        )

    def launch(self, identifier, code=None):
        task = asyncio.create_task(self.run(identifier, code))
        self.tasks[identifier] = task
        task.add_done_callback(
            lambda finished: (
                self.tasks.pop(identifier, None)
                if self.tasks.get(identifier) is finished
                else None
            )
        )

    def engine_account(self, owner, identifier, key_id=None):
        row = self.db.one(
            "select * from api_accounts where id=? and tg_id=?", (identifier, owner)
        )
        if not row:
            raise ApiError(404, "account_not_found")
        if key_id is not None and row["key_id"] != key_id:
            raise ApiError(404, "account_not_found")
        return {
            **dict(row),
            "id": row["engine_id"],
            "api_id": row["id"],
            "password": self.vault.decrypt(row["password_secret"])
            if row["password_secret"]
            else "",
        }

    def finish(self, row, status, error=""):
        with self.db.conn:
            self.db.conn.execute(
                "update api_auth_jobs set status=?,error=?,challenge_digest='' where id=? and status in ('queued','authenticating','requires_2fa','verifying_2fa')",
                (status, error, row["id"]),
            )
            self.db.conn.execute(
                "update api_accounts set status='error',password_secret='' where id=? and status!='ready'",
                (row["account_id"],),
            )
        self.challenges.pop(row["id"], None)

    async def run(self, identifier, code=None):
        row = self.db.one("select * from api_auth_jobs where id=?", (identifier,))
        try:
            async with self.slots:
                if row["expires_at"] <= now() or not self.allowed(
                    row["tg_id"], row["key_id"]
                ):
                    self.finish(row, "expired", "auth_expired")
                    return
                self.db._run(
                    "update api_auth_jobs set status=? where id=?",
                    ("verifying_2fa" if code else "authenticating", identifier),
                )
                account = self.engine_account(
                    row["tg_id"], row["account_id"], row["key_id"]
                )
                try:
                    async with asyncio.timeout(max(1, row["expires_at"] - now())):
                        await self.tool.login(account, code)
                except IpatoolError as exc:
                    attempts = self.db.one(
                        "select attempts from api_auth_jobs where id=?", (identifier,)
                    )[0]
                    if exc.needs_2fa and attempts < 3:
                        challenge = secrets.token_urlsafe(32)
                        self.challenges[identifier] = challenge
                        self.db._run(
                            "update api_auth_jobs set status='requires_2fa',error=?,challenge_digest=? where id=?",
                            (
                                "invalid_2fa_code" if code else "",
                                key_digest(challenge),
                                identifier,
                            ),
                        )
                        self.db._run(
                            "update api_accounts set status='need2fa' where id=?",
                            (row["account_id"],),
                        )
                        return
                    self.finish(
                        row,
                        "failed",
                        "too_many_2fa_attempts"
                        if attempts >= 3
                        else "apple_login_failed",
                    )
                    return
                if row["expires_at"] <= now() or not self.allowed(
                    row["tg_id"], row["key_id"]
                ):
                    self.finish(row, "expired", "auth_expired")
                    return
                with self.db.conn:
                    self.db.conn.execute(
                        "update api_auth_jobs set status='ready',error='',challenge_digest='' where id=?",
                        (identifier,),
                    )
                    self.db.conn.execute(
                        "update api_accounts set status='ready',session_generation=session_generation+1 where id=?",
                        (row["account_id"],),
                    )
                    self.db.conn.execute(
                        "delete from ownership_cache where account_id=?",
                        (account["id"],),
                    )
                self.db.link_identity(account["email"], row["tg_id"])
        except asyncio.CancelledError:
            self.finish(row, "canceled", "login_canceled")
            raise
        except TimeoutError:
            self.finish(row, "expired", "auth_expired")
        except Exception:
            self.finish(row, "failed", "apple_login_failed")
        finally:
            self.challenges.pop(identifier, None) if self.db.one(
                "select status from api_auth_jobs where id=?", (identifier,)
            )[0] != "requires_2fa" else None

    async def status(self, request):
        return web.json_response(self.result(self.job_for(request)))

    async def code(self, request):
        row = self.job_for(request)
        if row["status"] != "requires_2fa":
            raise ApiError(409, "code_not_required")
        if not take_request(self.code_rates, row["id"], 5):
            raise ApiError(429, "code_rate_limit_exceeded")
        data = await self.body(request)
        token, code = data.get("challenge_token"), data.get("code")
        if (
            not isinstance(token, str)
            or len(token) > 128
            or not hmac.compare_digest(key_digest(token), row["challenge_digest"])
        ):
            raise ApiError(403, "invalid_challenge_token")
        if not isinstance(code, str) or not re.fullmatch(r"[0-9]{6}", code):
            raise ApiError(422, "invalid_2fa_format")
        changed = self.db._run(
            "update api_auth_jobs set status='verifying_2fa',attempts=attempts+1,challenge_digest='' where id=? and status='requires_2fa' and attempts<3",
            (row["id"],),
        )
        if not changed.rowcount:
            raise ApiError(409, "code_already_submitted")
        self.challenges.pop(row["id"], None)
        self.launch(row["id"], code)
        return web.json_response(
            self.result(
                self.db.one("select * from api_auth_jobs where id=?", (row["id"],))
            ),
            status=202,
        )

    async def cancel(self, request):
        row = self.job_for(request)
        task = self.tasks.get(row["id"])
        if task:
            task.cancel()
            with suppress(asyncio.CancelledError):
                await task
        self.finish(row, "canceled", "login_canceled")
        return web.json_response(
            self.result(
                self.db.one("select * from api_auth_jobs where id=?", (row["id"],))
            )
        )

    def public_account(self, row):
        return {
            "account_id": row["id"],
            "email": row["email"],
            "status": row["status"],
            "created_at": row["created_at"],
            "session_warmed_at": row["kbsync_warmed_at"] or None,
            "refresh_interval_seconds": 43200,
        }

    async def accounts(self, request):
        return web.json_response(
            {
                "accounts": [
                    self.public_account(row)
                    for row in self.db.all(
                        "select * from api_accounts where tg_id=? and key_id=? order by created_at",
                        (request[OWNER], request[KEY_ID]),
                    )
                ],
                "limit": self.cfg.api_accounts_max,
            }
        )

    async def account_status(self, request):
        row = self.db.one(
            "select * from api_accounts where id=? and tg_id=? and key_id=?",
            (request.match_info["account_id"], request[OWNER], request[KEY_ID]),
        )
        if not row:
            raise ApiError(404, "account_not_found")
        return web.json_response(self.public_account(row))

    async def restore(self, request):
        account = self.engine_account(
            request[OWNER], request.match_info["account_id"], request[KEY_ID]
        )
        if account["status"] != "ready":
            raise ApiError(409, "login_required")
        return await self.account_status(request)

    async def delete_account(self, request):
        identifier, owner = request.match_info["account_id"], request[OWNER]
        row = self.db.one(
            "select * from api_accounts where id=? and tg_id=? and key_id=?",
            (identifier, owner, request[KEY_ID]),
        )
        if not row:
            raise ApiError(404, "account_not_found")
        if self.db.one(
            "select id from api_jobs where account_id=? and status in ('queued','downloading')",
            (identifier,),
        ):
            raise ApiError(409, "account_busy")
        for job in self.db.all(
            "select * from api_auth_jobs where account_id=? and status in ('queued','authenticating','requires_2fa','verifying_2fa')",
            (identifier,),
        ):
            task = self.tasks.get(job["id"])
            if task:
                task.cancel()
                with suppress(asyncio.CancelledError):
                    await task
            self.finish(job, "canceled", "account_deleted")
        async with self.tool.lock(row["engine_id"]):
            self.db._run(
                "delete from ownership_cache where account_id=?", (row["engine_id"],)
            )
            self.db._run("delete from api_accounts where id=?", (identifier,))
            home = (self.cfg.accounts_dir / str(row["engine_id"])).resolve()
            if home.parent == self.cfg.accounts_dir.resolve():
                await asyncio.to_thread(shutil.rmtree, home, True)
        return web.json_response({"deleted": True})

    def expire_jobs(self):
        for row in self.db.all(
            "select * from api_auth_jobs where expires_at<=? and status in ('queued','authenticating','requires_2fa','verifying_2fa')",
            (now(),),
        ):
            self.finish(row, "expired", "auth_expired")
            if row["id"] in self.tasks:
                self.tasks[row["id"]].cancel()

    async def maintain(self):
        while True:
            self.expire_jobs()
            self.db._run(
                "delete from api_auth_jobs where expires_at<? and status not in ('queued','authenticating','requires_2fa','verifying_2fa')",
                (now() - 86400,),
            )
            await asyncio.sleep(30)

    async def warm_tick(self):
        if not getattr(self.tool, "is_cpp", False):
            return
        rows = self.db.all(
            "select * from api_accounts where status='ready' and kbsync_warmed_at<=? order by kbsync_warmed_at limit 100",
            (now() - 43200,),
        )
        for row in rows:
            if row["id"] in self.warm_retries or not self.allowed(
                row["tg_id"], row["key_id"]
            ):
                continue
            lock = self.tool.lock(row["engine_id"])
            if lock.locked():
                continue
            try:
                async with lock:
                    current = self.db.one(
                        "select * from api_accounts where id=? and status='ready'",
                        (row["id"],),
                    )
                    if not current or not self.allowed(row["tg_id"], row["key_id"]):
                        continue
                    account = self.engine_account(
                        row["tg_id"], row["id"], row["key_id"]
                    )
                    result = await self.tool.run(
                        account, ["kbsync", "--refresh"], timeout=600
                    )
                    if result.get("success") is not True:
                        raise IpatoolError("Session refresh failed")
                    self.db._run(
                        "update api_accounts set kbsync_warmed_at=? where id=?",
                        (now(), row["id"]),
                    )
            except Exception as exc:
                self.warm_retries[row["id"]] = True
                if isinstance(exc, IpatoolError) and exc.needs_login:
                    self.db._run(
                        "update api_accounts set status='error' where id=?",
                        (row["id"],),
                    )
                logging.getLogger("signum-bot.api").warning(
                    "API session refresh failed (%s)", type(exc).__name__
                )

    async def warm_loop(self):
        while True:
            await self.warm_tick()
            await asyncio.sleep(60)

    async def start(self, app):
        for row in self.db.all(
            "select * from api_auth_jobs where status in ('queued','authenticating','requires_2fa','verifying_2fa')"
        ):
            self.finish(row, "expired", "restart_login")
        self.keeper = asyncio.create_task(self.maintain())
        self.warmer = asyncio.create_task(self.warm_loop())

    async def close(self, app):
        tasks = (
            list(self.tasks.values())
            + ([self.keeper] if self.keeper else [])
            + ([self.warmer] if self.warmer else [])
        )
        for task in tasks:
            task.cancel()
        for task in tasks:
            with suppress(asyncio.CancelledError):
                await task
