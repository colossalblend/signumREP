from __future__ import annotations

import asyncio
import json
import os
import time
from dataclasses import dataclass, field
from pathlib import Path

from .api_secrets import ApiSecrets
from .db import now


class AppleWebError(RuntimeError):
    pass


@dataclass
class Worker:
    proc: asyncio.subprocess.Process
    generation: int
    touched: float = field(default_factory=time.monotonic)
    lock: asyncio.Lock = field(default_factory=asyncio.Lock)
    restored: bool = False


class AppleWeb:
    def __init__(self, cfg, db):
        self.cfg, self.db = cfg, db
        self.vault = ApiSecrets(cfg.data_dir)
        suffix = ".exe" if os.name == "nt" else ""
        self.binary = Path(
            os.getenv("APPLE_BRIDGE", "")
            or Path(__file__).resolve().parent.parent
            / "bin"
            / ("signum-apple-bridge" + suffix)
        )
        self.workers: dict[int, Worker] = {}
        self.slots = asyncio.Semaphore(2)
        self.lifecycle = asyncio.Lock()
        self.keeper = None
        self.country_options = None

    async def stop_worker(self, identifier):
        worker = self.workers.pop(identifier, None)
        if worker and worker.proc.returncode is None:
            worker.proc.kill()
            await worker.proc.wait()

    async def maintain(self):
        while True:
            await asyncio.sleep(30)
            async with self.lifecycle:
                for identifier, worker in list(self.workers.items()):
                    if (
                        not worker.lock.locked()
                        and time.monotonic() - worker.touched > 600
                    ):
                        await self.stop_worker(identifier)

    async def worker(self, identifier, generation):
        async with self.lifecycle:
            existing = self.workers.get(identifier)
            if existing and (
                existing.generation != generation
                or existing.proc.returncode is not None
            ):
                await self.stop_worker(identifier)
                existing = None
            if existing:
                return existing
            if not self.binary.is_file():
                raise AppleWebError(
                    "Сервис Apple временно недоступен. Попробуйте позже."
                )
            if len(self.workers) >= 16:
                idle = [
                    (row.touched, key)
                    for key, row in self.workers.items()
                    if not row.lock.locked() and time.monotonic() - row.touched > 600
                ]
                if not idle:
                    raise AppleWebError(
                        "Сейчас много запросов. Повторите через минуту."
                    )
                await self.stop_worker(min(idle)[1])
            proc = await asyncio.create_subprocess_exec(
                str(self.binary),
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.DEVNULL,
                limit=4 * 1024 * 1024,
            )
            worker = Worker(proc, generation)
            self.workers[identifier] = worker
            if self.keeper is None:
                self.keeper = asyncio.create_task(self.maintain())
            return worker

    def saved(self, identifier, generation):
        row = self.db.one(
            "select * from apple_web_sessions where account_id=? and generation=?",
            (identifier, generation),
        )
        if not row:
            return None
        return row

    async def request(self, account, action, **fields):
        identifier = int(account["id"]) if account is not None else 0
        generation = int(account["session_generation"]) if account is not None else 0
        worker = await self.worker(identifier, generation)
        async with worker.lock, self.slots:
            payload = {"action": action, **fields}
            if account is not None:
                payload.update(email=account["email"], password=account["password"])
            saved = self.saved(identifier, generation)
            if not worker.restored and saved and saved["session_secret"]:
                payload["state"] = json.loads(
                    self.vault.decrypt(saved["session_secret"])
                )
            try:
                async with asyncio.timeout(180):
                    worker.proc.stdin.write(
                        json.dumps(payload, ensure_ascii=False).encode() + b"\n"
                    )
                    await worker.proc.stdin.drain()
                    line = await worker.proc.stdout.readline()
                    result = json.loads(line)
            except (TimeoutError, asyncio.CancelledError):
                await self.stop_worker(identifier)
                raise
            except (ValueError, OSError, ConnectionError):
                await self.stop_worker(identifier)
                raise AppleWebError("Нет ответа от Apple. Повторите запрос.") from None
            worker.restored = True
            worker.touched = time.monotonic()
            state = result.pop("state", None)
            if account is not None and state is not None:
                self.db._run(
                    "insert into apple_web_sessions(account_id,generation,session_secret,updated_at) values(?,?,?,?) on conflict(account_id) do update set generation=excluded.generation,session_secret=excluded.session_secret,updated_at=excluded.updated_at,history_secret=case when apple_web_sessions.generation=excluded.generation then apple_web_sessions.history_secret else '' end",
                    (
                        identifier,
                        generation,
                        self.vault.encrypt(json.dumps(state)),
                        now(),
                    ),
                )
            if result.get("status") == "ready" and "region" in result:
                region = result["region"]
                self.db._run(
                    "update accounts set country_code=?,country_name=? where id=?",
                    (
                        region.get("countryCode", ""),
                        region.get("countryName", ""),
                        identifier,
                    ),
                )
                if action == "region_change":
                    self.db.kbsync_reset(identifier)
                    self.db._run(
                        "delete from ownership_cache where account_id=?", (identifier,)
                    )
                    self.db._run(
                        "update apple_web_sessions set history_secret='' where account_id=?",
                        (identifier,),
                    )
            return result

    def history_cache(self, account):
        row = self.saved(account["id"], account["session_generation"])
        if row and row["history_secret"]:
            cache = json.loads(self.vault.decrypt(row["history_secret"]))
            if cache.get("cached_at", 0) > now() - 3600:
                return cache
        return {"apps": [], "has_more": True, "loaded": False}

    async def history(self, account, page=1, refresh=False, code=None):
        if refresh:
            self.db._run(
                "update apple_web_sessions set history_secret='' where account_id=?",
                (account["id"],),
            )
        cache = (
            {"apps": [], "has_more": True, "loaded": False}
            if refresh
            else self.history_cache(account)
        )
        page = max(1, min(1000, page))
        start, end = (page - 1) * 8, page * 8
        batches = 0
        while (
            (not cache["loaded"] or len(cache["apps"]) < end)
            and cache["has_more"]
            and batches < 5
        ):
            batches += 1
            action = "history_next" if cache["loaded"] else "history"
            result = await self.request(account, action, code=code)
            code = None
            if result.get("status") != "ready":
                return result
            known = {row["id"] for row in cache["apps"]}
            for app in result.get("apps", []):
                if app.get("id") and app["id"] not in known:
                    cache["apps"].append(app)
                    known.add(app["id"])
            cache.update(
                has_more=bool(result.get("has_more")), loaded=True, cached_at=now()
            )
            self.db._run(
                "update apple_web_sessions set history_secret=?,updated_at=? where account_id=?",
                (self.vault.encrypt(json.dumps(cache)), now(), account["id"]),
            )
        return {
            "status": "ready",
            "apps": cache["apps"][start:end],
            "has_more": end < len(cache["apps"]) or cache["has_more"],
            "total": len(cache["apps"]),
            "page": page,
            "next_page": page
            if len(cache["apps"]) < end and cache["has_more"]
            else page + 1,
        }

    async def regions(self):
        if self.country_options is None:
            result = await self.request(None, "regions")
            self.country_options = result.get("regions", [])
        return self.country_options

    async def close(self):
        if self.keeper:
            self.keeper.cancel()
            try:
                await self.keeper
            except asyncio.CancelledError:
                pass
        for identifier in list(self.workers):
            await self.stop_worker(identifier)
