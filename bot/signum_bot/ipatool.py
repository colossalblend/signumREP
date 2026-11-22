from __future__ import annotations

import asyncio
import json
import os
import re
import secrets
import shutil
import sqlite3
import tempfile
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any, Callable

from .config import Config
from .db import Db


class IpatoolError(RuntimeError):
    def __init__(self, message: str, output: str = "") -> None:
        super().__init__(message)
        self.output = output

    @property
    def needs_2fa(self) -> bool:
        text = f"{self} {self.output}".lower()
        return (
            "2fa" in text
            or "two-factor" in text
            or "auth-code" in text
            or "verification code" in text
        )

    @property
    def needs_login(self) -> bool:
        text = f"{self} {self.output}".lower()
        return (
            "failed to get account" in text
            or "keychain" in text
            or "not logged in" in text
        )

    @property
    def not_licensed(self) -> bool:
        text = f"{self} {self.output}".lower()
        return "license" in text or "not purchased" in text or "покупк" in text

    @property
    def unavailable(self) -> bool:
        text = f"{self} {self.output}".lower()
        return "not available" in text or "storefront" in text or "9610" in text


PERCENT = re.compile(r"(\d{1,3})\s*%")
RATIO = re.compile(
    r"(\d+(?:[.,]\d+)?)\s*(B|KB|MB|GB)\s*/\s*(\d+(?:[.,]\d+)?)\s*(B|KB|MB|GB)", re.I
)
UNITS = {"B": 1, "KB": 1024, "MB": 1024**2, "GB": 1024**3}


def parse_percent(text: str) -> float | None:
    """Достаёт процент из полосы загрузки ipatool.

    Полоса рисуется через возврат каретки, а не переводом строки, поэтому берём
    последнее совпадение в куске вывода, а не первое.
    """
    ratios = RATIO.findall(text)
    if ratios:
        done, done_unit, total, total_unit = ratios[-1]
        done_bytes = float(done.replace(",", ".")) * UNITS[done_unit.upper()]
        total_bytes = float(total.replace(",", ".")) * UNITS[total_unit.upper()]
        if total_bytes > 0:
            return max(0.0, min(100.0, done_bytes / total_bytes * 100))
    percents = PERCENT.findall(text)
    if percents:
        return max(0.0, min(100.0, float(percents[-1])))
    return None


class NoAccount(IpatoolError):
    """У пользователя нет подключённого Apple ID."""


class Ipatool:
    """Тонкая обёртка над бинарником ipatool.

    Каждый Apple ID живёт в своём HOME: ipatool держит сессию в keychain внутри
    домашней папки, поэтому иначе аккаунты затирали бы друг друга. По одной
    команде на аккаунт за раз — keychain берёт файловую блокировку.
    """

    def __init__(self, cfg: Config, db: Db) -> None:
        self.cfg = cfg
        self.db = db
        self._locks: dict[int, asyncio.Lock] = {}
        self._slots = asyncio.Semaphore(cfg.concurrency)
        self._probe_slots = asyncio.Semaphore(4)
        self.session_vault = None
        if cfg.session_encryption:
            from .session_vault import SessionVault
            self.session_vault = SessionVault(cfg.data_dir)

    def lock(self, account_id: int) -> asyncio.Lock:
        return self._locks.setdefault(account_id, asyncio.Lock())

    def home(self, account_id: int) -> Path:
        path = self.cfg.accounts_dir / str(account_id)
        path.mkdir(parents=True, exist_ok=True)
        return path

    @property
    def is_cpp(self) -> bool:
        return self.cfg.ipatool_engine in {"cpp", "v5", "v6"}

    def command(self, account: sqlite3.Row, args: list[str]) -> list[str]:
        flags = [] if self.is_cpp else ["--non-interactive"]
        metadata_request = args and args[0] in {
            "list-versions", "get-version-metadata"
        }
        executable = (
            self.cfg.ipatool_metadata
            if self.cfg.ipatool_engine == "v6" and metadata_request
            and self.cfg.ipatool_metadata
            else self.cfg.ipatool
        )
        return [
            executable,
            *flags,
            "--format",
            "json",
            "--keychain-passphrase",
            account["keychain"],
            *args,
        ]

    async def run(
        self, account, args, *, timeout=180, on_progress=None, on_start=None,
        home_override=None,
    ):
        if self.session_vault and home_override is None:
            with self.session_vault.open(self.home(int(account["id"]))) as working:
                return await self._run_process(
                    account, args, timeout=timeout, on_progress=on_progress,
                    on_start=on_start, home_override=working,
                )
        if self.session_vault and home_override is not None:
            if not Path(home_override).resolve().is_relative_to(self.session_vault.ram.resolve()):
                raise IpatoolError("Encrypted session probe must use RAM storage")
        return await self._run_process(
            account, args, timeout=timeout, on_progress=on_progress,
            on_start=on_start, home_override=home_override,
        )

    async def _run_process(
        self,
        account: sqlite3.Row,
        args: list[str],
        *,
        timeout: int = 180,
        on_progress: Callable[[float], None] | None = None,
        on_start: Callable[[asyncio.subprocess.Process], None] | None = None,
        home_override: Path | None = None,
    ) -> dict[str, Any]:
        home = home_override or self.home(int(account["id"]))
        env = dict(os.environ)
        env["HOME"] = str(home)
        env["USERPROFILE"] = str(home)
        command = self.command(account, args)
        proc = await asyncio.create_subprocess_exec(
            *command,
            stdin=asyncio.subprocess.DEVNULL,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env,
            cwd=str(home),
        )
        if on_start is not None:
            on_start(proc)

        stdout_chunks: list[str] = []
        stderr_chunks: list[str] = []

        async def pump(stream: asyncio.StreamReader, watch: bool) -> None:
            tail = ""
            while True:
                data = await stream.read(512)
                if not data:
                    break
                text = data.decode("utf-8", "replace")
                (stderr_chunks if watch else stdout_chunks).append(text)
                if not watch or on_progress is None:
                    continue
                tail = (tail + text)[-2048:]
                percent = parse_percent(tail)
                if percent is not None:
                    on_progress(percent)

        try:
            await asyncio.wait_for(
                asyncio.gather(pump(proc.stdout, False), pump(proc.stderr, True)),
                timeout=timeout,
            )
            await proc.wait()
        except asyncio.TimeoutError:
            if proc.returncode is None:
                proc.kill()
            await proc.wait()
            raise IpatoolError("ipatool не ответил вовремя")
        except asyncio.CancelledError:
            if proc.returncode is None:
                proc.kill()
            await proc.wait()
            raise
        return self._parse(
            "".join(stdout_chunks) + "\n" + "".join(stderr_chunks),
            command_failed=proc.returncode not in (0, None),
        )

    @staticmethod
    def _parse(raw: str, *, command_failed: bool) -> dict[str, Any]:
        payloads: list[dict[str, Any]] = []
        for line in raw.splitlines():
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(item, dict):
                payloads.append(item)
        for item in payloads:
            if item.get("level") == "error" or item.get("success") is False:
                raise IpatoolError(
                    str(item.get("error") or "ipatool вернул ошибку"), raw
                )
        for item in reversed(payloads):
            if item.get("success") is True or len(item) > 2:
                return item
        if command_failed or not payloads:
            raise IpatoolError("ipatool не вернул результат", raw)
        return payloads[-1]

    # --- вход -------------------------------------------------------------
    async def login(self, account: sqlite3.Row, code: str | None = None) -> None:
        args = ["auth", "login", "-e", account["email"], "-p", account["password"]]
        if code:
            args += ["--auth-code", code.strip()]
        async with self.lock(int(account["id"])):
            await self.run(account, args, timeout=180)
            self.db.kbsync_reset(int(account["id"]))
            self.db._run(
                "update accounts set session_generation=session_generation+1 where id=?",
                (int(account["id"]),),
            )
            self.db._run(
                "delete from ownership_cache where account_id=?", (int(account["id"]),)
            )

    async def revalidate(self, account: sqlite3.Row) -> None:
        """Проверяет, что сессия жива; молча перелогинивается, если нет."""
        try:
            async with self.lock(int(account["id"])):
                args = (
                    ["auth", "info"] if self.is_cpp else ["list-purchases", "-l", "1"]
                )
                await self.run(account, args, timeout=90)
        except IpatoolError as exc:
            if not exc.needs_login:
                raise
            await self.login(account)

    # --- каталог ----------------------------------------------------------
    async def search(
        self, account: sqlite3.Row, term: str, limit: int = 12
    ) -> list[dict[str, Any]]:
        async with self.lock(int(account["id"])):
            data = await self.run(
                account, ["search", "-l", str(limit), term], timeout=90
            )
        apps = data.get("apps") or []
        return [app for app in apps if isinstance(app, dict)]

    async def purchases(
        self, account: sqlite3.Row, page: int = 1, limit: int = 10
    ) -> list[dict[str, Any]]:
        if self.is_cpp:
            raise IpatoolError(
                "Движок ipatool-cpp не поддерживает список покупок. Найдите приложение через каталог или поиск."
            )
        async with self.lock(int(account["id"])):
            data = await self.run(
                account,
                ["list-purchases", "-p", str(page), "-l", str(limit)],
                timeout=120,
            )
        apps = data.get("apps") or data.get("purchases") or []
        return [app for app in apps if isinstance(app, dict)]

    async def warm_kbsync(self, account: sqlite3.Row) -> bool:
        if not self.is_cpp:
            return False
        async with self.lock(int(account["id"])):
            current = self.db.account(int(account["id"]))
            if current is None or current["status"] != "ready":
                return False
            owner = self.db.one(
                "select blocked, sub_until from users where tg_id=?",
                (current["tg_id"],),
            )
            if (
                owner is None
                or owner["blocked"]
                or not self.db.sub_active(current["tg_id"])
            ):
                return False
            result = await self.run(current, ["kbsync", "--refresh"], timeout=600)
            if result.get("success") is not True:
                raise IpatoolError("Движок не подтвердил прогрев KBSync")
            self.db.kbsync_warmed(int(current["id"]))
            return True

    async def prepare_catalog_probe(self, account: sqlite3.Row) -> None:
        if self.is_cpp:
            async with self.lock(int(account["id"])):
                await self.run(account, ["kbsync"], timeout=600)

    @asynccontextmanager
    async def probe_homes(self, account: sqlite3.Row, workers: int):
        home = self.home(int(account["id"]))
        if self.session_vault:
            with tempfile.TemporaryDirectory(prefix="probes-", dir=self.session_vault.ram) as root:
                homes = [Path(root) / str(i) for i in range(workers)]
                async with self.lock(int(account["id"])):
                    with self.session_vault.open(home, persist=False) as working:
                        source = working / ".ipatool"
                        if not source.is_dir():
                            raise IpatoolError("Войдите в Apple ID заново")
                        for target in homes:
                            await asyncio.to_thread(
                                shutil.copytree, source, target / ".ipatool",
                                ignore=shutil.ignore_patterns("*.lock", "ipa"),
                            )
                yield homes
            return
        with tempfile.TemporaryDirectory(prefix="probes-", dir=home) as root:
            homes = [Path(root) / str(i) for i in range(workers)]
            async with self.lock(int(account["id"])):
                source = home / ".ipatool"
                if not source.is_dir():
                    raise IpatoolError("Войдите в Apple ID заново")
                # Снимки защищают основной keychain от параллельных записей.
                for target in homes:
                    await asyncio.to_thread(
                        shutil.copytree,
                        source,
                        target / ".ipatool",
                        ignore=shutil.ignore_patterns("*.lock", "ipa"),
                    )
            yield homes

    async def versions(
        self,
        account: sqlite3.Row,
        adam_id: str,
        *,
        timeout: int | None = None,
        home_override: Path | None = None,
    ) -> list[str]:
        guard = self._probe_slots if home_override else self.lock(int(account["id"]))
        async with guard:
            data = await self.run(
                account,
                ["list-versions", "-i", str(adam_id)],
                timeout=timeout or (600 if self.is_cpp else 90),
                **({"home_override": home_override} if home_override else {}),
            )
        fields = (
            "externalVersionIdentifiers",
            "external_version_identifiers",
            "externalVersionIds",
            "versions",
        )
        if not any(key in data for key in fields):
            raise IpatoolError("Apple не вернул список версий")
        versions = next((data[key] for key in fields if key in data), None)
        if not isinstance(versions, list):
            raise IpatoolError("Apple вернул некорректный список версий")
        return list(
            dict.fromkeys(str(item) for item in versions if str(item).isdigit())
        )

    async def version_metadata(
        self,
        account: sqlite3.Row,
        adam_id: str,
        version: str,
        *,
        home_override: Path | None = None,
    ) -> dict:
        guard = self._probe_slots if home_override else self.lock(int(account["id"]))
        async with guard:
            data = await self.run(
                account,
                [
                    "get-version-metadata",
                    "-i",
                    str(adam_id),
                    "--external-version-id",
                    str(version),
                ],
                timeout=25,
                **({"home_override": home_override} if home_override else {}),
            )
        label = data.get("displayVersion") or data.get("versionString")
        if not isinstance(label, str) or not label.strip() or len(label) > 80:
            raise IpatoolError("Apple не вернул название версии")
        return {"version": label.strip()}

    async def purchase(self, account: sqlite3.Row, bundle_id: str) -> None:
        async with self.lock(int(account["id"])):
            await self.run(account, ["purchase", "-b", bundle_id], timeout=120)

    # --- скачивание -------------------------------------------------------
    async def download(
        self,
        account: sqlite3.Row,
        adam_id: str,
        *,
        external_version_id: str | None = None,
        out_dir: Path | None = None,
        on_progress: Callable[[float], None] | None = None,
        on_start: Callable[[asyncio.subprocess.Process], None] | None = None,
    ) -> Path:
        out_dir = out_dir or (
            self.home(int(account["id"])) / "ipa" / secrets.token_hex(4)
        )
        out_dir.mkdir(parents=True, exist_ok=True)
        args = ["download", "-i", str(adam_id), "-o", str(out_dir), "--purchase"]
        if external_version_id:
            args += ["--external-version-id", str(external_version_id)]
        async with self._slots:
            async with self.lock(int(account["id"])):
                await self.run(
                    account,
                    args,
                    timeout=1800,
                    on_progress=on_progress,
                    on_start=on_start,
                )
        files = sorted(
            out_dir.glob("*.ipa"), key=lambda p: p.stat().st_mtime, reverse=True
        )
        if not files:
            raise IpatoolError("ipatool отработал, но IPA не появился")
        self.db.account_used(int(account["id"]))
        return files[0]

    # --- выбор аккаунта ---------------------------------------------------
    def for_user(self, tg_id: int) -> sqlite3.Row:
        """Apple ID этого человека. Своего нет — качать не с чего."""
        account = self.db.account_of(tg_id)
        if account is None:
            raise NoAccount("Сначала подключите свой Apple ID.")
        if account["status"] != "ready":
            raise NoAccount(
                "Ваш Apple ID не на связи — загляните на страницу «Мой Apple ID»."
            )
        return account
