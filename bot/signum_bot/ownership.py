from __future__ import annotations

import asyncio
import hashlib
import json
import sqlite3
import time
from collections import deque
from dataclasses import dataclass
from typing import Any, Callable, Sequence

from cachetools import TTLCache

from .ipatool import Ipatool, IpatoolError


@dataclass(frozen=True)
class Verdict:
    ids: frozenset[str] = frozenset()
    unknown: bool = False
    region_owned: bool = False
    session_problem: bool = False


@dataclass(frozen=True)
class Scan:
    versions: list[dict[str, Any]]
    unchecked: int


class OwnedCatalog:
    def __init__(self, tool: Ipatool) -> None:
        self.tool = tool
        self.cache: TTLCache = TTLCache(maxsize=2048, ttl=600)
        self.locks: dict[int, asyncio.Lock] = {}
        self.db = getattr(tool, "db", None)

    def session_revision(self, account: sqlite3.Row) -> tuple:
        if "session_generation" in account.keys():
            current = self.db.account(int(account["id"])) if self.db else account
            stamp = hashlib.sha256(
                f"{account['keychain']}:{current['session_generation']}".encode()
            ).hexdigest()
            return int(account["id"]), stamp
        home = self.tool.cfg.accounts_dir / str(account["id"]) / ".ipatool"
        modified = []
        for path in (home / "account", home / "keyring/account"):
            try:
                modified.append(path.stat().st_mtime_ns)
            except OSError:
                modified.append(0)
        return int(account["id"]), account["keychain"], tuple(modified)

    def broken_cookies(self, account: sqlite3.Row) -> bool:
        if getattr(self.tool, "session_vault", None):
            return self.tool.session_vault.cookies_broken(self.tool.home(int(account["id"])))
        home = self.tool.cfg.accounts_dir / str(account["id"]) / ".ipatool"
        signed_in = (home / "account").is_file() or (home / "keyring/account").is_file()
        if not signed_in:
            return False
        try:
            return (home / "cookies").stat().st_size < 16
        except OSError:
            return True

    def cached(self, revision: tuple, adam: str) -> Verdict | None:
        key = (*revision, adam)
        verdict = self.cache.get(key)
        if verdict is not None or not self.db:
            return verdict
        row = self.db.one(
            "select payload from ownership_cache where account_id=? and session_key=? "
            "and adam_id=? and expires_at>?",
            (revision[0], str(revision[1:]), adam, int(time.time())),
        )
        if row:
            data = json.loads(row["payload"])
            verdict = Verdict(
                ids=frozenset(data["ids"]), region_owned=data["region_owned"]
            )
            self.cache[key] = verdict
        return verdict

    def remember(self, revision: tuple, adam: str, verdict: Verdict) -> None:
        if verdict.unknown:
            return
        self.cache[(*revision, adam)] = verdict
        if self.db:
            self.db._run(
                "insert or replace into ownership_cache values (?,?,?,?,?)",
                (
                    revision[0],
                    str(revision[1:]),
                    adam,
                    json.dumps(
                        {
                            "ids": sorted(verdict.ids),
                            "region_owned": verdict.region_owned,
                        }
                    ),
                    int(time.time())
                    + (86400 if verdict.ids or verdict.region_owned else 3600),
                ),
            )
            self.db._run(
                "delete from ownership_cache where expires_at<=?", (int(time.time()),)
            )
            self.db._run(
                "delete from ownership_cache where rowid in "
                "(select rowid from ownership_cache order by expires_at desc limit -1 offset 4096)"
            )

    async def probe(self, account: sqlite3.Row, adam_id: str, home=None) -> Verdict:
        try:
            ids = await self.tool.versions(
                account,
                adam_id,
                timeout=45,
                **({"home_override": home} if home else {}),
            )
        except IpatoolError as exc:
            text = f"{exc} {exc.output}".lower()
            if (
                exc.needs_login
                or exc.needs_2fa
                or any(
                    part in text
                    for part in (
                        "token is expired",
                        "token expired",
                        "failed to decrypt",
                        "decryption failed",
                    )
                )
            ):
                return Verdict(unknown=True, session_problem=True)
            if "license was granted but download was blocked" in text or (
                "not available for download" in text and "storefront" in text
            ):
                return Verdict(region_owned=True)
            if any(
                part in text
                for part in (
                    "empty songlist",
                    "not purchased",
                    "license not found",
                    "license is required",
                    "purchase history",
                )
            ):
                return Verdict(unknown=self.broken_cookies(account))
            return Verdict(unknown=True)
        if not ids and self.broken_cookies(account):
            return Verdict(unknown=True)
        return Verdict(ids=frozenset(ids))

    async def scan(
        self,
        account: sqlite3.Row,
        versions: Sequence[dict[str, Any]],
        *,
        force: bool = False,
        progress: Callable[[int, int], None] | None = None,
        phase: Callable[[str], None] | None = None,
    ) -> Scan:
        account_id = int(account["id"])
        async with self.locks.setdefault(account_id, asyncio.Lock()):
            revision = self.session_revision(account)
            identities = list(
                dict.fromkeys(
                    str(v.get("app_store_id") or "").strip() for v in versions
                )
            )
            if force:
                if self.db:
                    self.db._run(
                        "delete from ownership_cache where account_id=?", (account_id,)
                    )
                for key in list(self.cache):
                    if key[0] == account_id and key[-1] in identities:
                        del self.cache[key]
            verdicts: dict[str, Verdict] = {}
            pending = deque()
            for adam in identities:
                verdict = self.cached(revision, adam)
                if verdict is not None:
                    verdicts[adam] = verdict
                elif adam.isdigit():
                    pending.append(adam)
                else:
                    verdicts[adam] = Verdict(unknown=True)
            if pending:
                if phase:
                    phase("prepare")
                await self.tool.prepare_catalog_probe(account)
                revision = self.session_revision(account)
                if phase:
                    phase("checking")
            if progress and verdicts:
                progress(len(verdicts), len(identities))

            async def worker(home=None):
                while pending:
                    adam = pending.popleft()
                    verdict = await self.probe(account, adam, home)
                    verdicts[adam] = verdict
                    self.remember(revision, adam, verdict)
                    if verdict.session_problem:
                        while pending:
                            verdicts[pending.popleft()] = verdict
                    if progress:
                        progress(len(verdicts), len(identities))

            if pending:
                if hasattr(self.tool, "probe_homes"):
                    async with self.tool.probe_homes(
                        account, min(4, len(pending))
                    ) as homes:
                        tasks = [asyncio.create_task(worker(home)) for home in homes]
                        try:
                            await asyncio.gather(*tasks)
                        finally:
                            for task in tasks:
                                if not task.done():
                                    task.cancel()
                            await asyncio.gather(*tasks, return_exceptions=True)
                else:
                    await worker()
            elif progress:
                progress(len(identities), len(identities))
            owned, unchecked = [], 0
            for version in versions:
                verdict = verdicts[str(version.get("app_store_id") or "").strip()]
                external = str(
                    version.get("external_version_id")
                    or version.get("externalVersionId")
                    or ""
                ).strip()
                if verdict.unknown or (verdict.region_owned and external):
                    unchecked += 1
                elif (
                    external in verdict.ids
                    if external
                    else bool(verdict.ids) or verdict.region_owned
                ):
                    owned.append(version)
            return Scan(owned, unchecked)
