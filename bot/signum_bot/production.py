from __future__ import annotations

import asyncio
import gzip
import logging
import os
import re
import shutil
import sqlite3
import tempfile
from contextlib import closing
from datetime import datetime, timezone
from logging.handlers import RotatingFileHandler
from pathlib import Path

from aiogram import BaseMiddleware
from cachetools import TTLCache

from .api_secrets import ApiSecrets
from .db import now
from .rate_limits import take_request

LOG = logging.getLogger("signum-bot.production")


class RedactedFormatter(logging.Formatter):
    def format(self, record):
        text = super().format(record)
        text = re.sub(r"\b\d{6,}:[\w-]+", "[token]", text)
        text = re.sub(r"\bsg_[\w-]{43}\b", "[api-key]", text)
        text = re.sub(r"\b(?:test|live)_[\w-]+", "[payment-key]", text)
        text = re.sub(r"https?://\S+", "[url]", text)
        return re.sub(r"[\w.+-]+@[\w.-]+", "[email]", text)


class EncryptedFormatter(logging.Formatter):
    def __init__(self, formatter, cipher):
        super().__init__()
        self.formatter = formatter
        self.cipher = cipher

    def format(self, record):
        return self.cipher.encrypt(self.formatter.format(record).encode()).decode()


def configure_logs(cfg, name):
    folder = cfg.data_dir / "logs"
    folder.mkdir(parents=True, exist_ok=True)
    formatter = RedactedFormatter("%(asctime)s %(levelname)s %(name)s: %(message)s")
    file = RotatingFileHandler(
        folder / f"{name}.log",
        maxBytes=5 * 1024 * 1024,
        backupCount=3,
        encoding="utf-8",
    )
    stream = logging.StreamHandler()
    stream.setLevel(logging.ERROR)
    file.setFormatter(EncryptedFormatter(formatter, ApiSecrets(cfg.data_dir).cipher)
                      if cfg.session_encryption else formatter)
    stream.setFormatter(formatter)
    logging.basicConfig(level=logging.INFO, handlers=[file, stream], force=True)
    logging.getLogger("httpx").setLevel(logging.WARNING)
    logging.getLogger("aiohttp.access").setLevel(logging.WARNING)
    logging.getLogger("aiogram.event").setLevel(logging.WARNING)


class BotGuard(BaseMiddleware):
    def __init__(self, db, admins=()):
        self.db = db
        self.admins = admins
        self.rates = TTLCache(maxsize=4096, ttl=60)
        self.active = {}
        self.total = 0

    async def __call__(self, handler, event, data):
        actor = getattr(event, "from_user", None)
        if not actor:
            return await handler(event, data)
        chat = getattr(event, "chat", None) or getattr(
            getattr(event, "message", None), "chat", None
        )
        if chat is not None and chat.type != "private":
            return
        if self.db.blocked(actor.id):
            return
        if (
            not take_request(self.rates, actor.id, 30)
            or self.active.get(actor.id, 0) >= 2
            or self.total >= 64
        ):
            if hasattr(event, "data"):
                await event.answer("Подождите немного и повторите.")
            return
        if actor.id not in self.admins and maintenance_enabled(self.db, "bot"):
            target = getattr(event, "message", None) or event
            if hasattr(event, "data"):
                await event.answer()
            await target.answer(
                "<b>Signum обновляется</b>\nСкоро вернёмся. Ваши покупки и подписка сохраняются."
            )
            return
        self.active[actor.id] = self.active.get(actor.id, 0) + 1
        self.total += 1
        try:
            return await handler(event, data)
        except asyncio.CancelledError:
            raise
        except Exception as exc:
            LOG.error("Bot handler failed (%s)", type(exc).__name__)
            target = getattr(event, "message", None) or event
            try:
                await target.answer(
                    "Не удалось выполнить запрос. Попробуйте ещё раз через минуту."
                )
            except Exception:
                pass
        finally:
            self.total -= 1
            remaining = self.active[actor.id] - 1
            if remaining:
                self.active[actor.id] = remaining
            else:
                self.active.pop(actor.id, None)


class StorageBusy(RuntimeError):
    def __init__(self):
        super().__init__(
            "Сейчас недостаточно свободного места. Повторите загрузку позже."
        )


def reserve_storage(cfg, db, identifier, size):
    with db.conn:
        db.conn.execute("begin immediate")
        db.conn.execute(
            "delete from storage_reservations where expires_at<=?", (now(),)
        )
        reserved = db.conn.execute(
            "select coalesce(sum(bytes),0) from storage_reservations where id!=?",
            (identifier,),
        ).fetchone()[0]
        free = shutil.disk_usage(cfg.data_dir).free
        if free - reserved - size < 1024**3:
            raise StorageBusy()
        db.conn.execute(
            "insert or replace into storage_reservations(id,bytes,expires_at) values(?,?,?)",
            (identifier, size, now() + 7200),
        )


def release_storage(db, identifier):
    db._run("delete from storage_reservations where id=?", (identifier,))


def maintenance_enabled(db, service):
    row = db.one(
        "select value from service_settings where name=?", ("maintenance:" + service,)
    )
    return row is not None and row[0] == "1"


def set_maintenance(db, service, enabled):
    if service not in ("bot", "api"):
        raise ValueError("Unknown service")
    db._run(
        "insert or replace into service_settings(name,value) values (?,?)",
        ("maintenance:" + service, "1" if enabled else "0"),
    )


def claim_download(cfg, db, tg_id, identifier, paid):
    with db.conn:
        db.conn.execute("begin immediate")
        db.conn.execute("delete from download_claims where expires_at<=?", (now(),))
        if (
            db.blocked(tg_id)
            or maintenance_enabled(db, "bot")
            and tg_id not in cfg.admins
        ):
            raise RuntimeError("Загрузки сейчас недоступны.")
        if paid == "free" and db.week_uses(tg_id) >= cfg.free_per_week:
            raise RuntimeError(
                "Бесплатные скачивания закончились. Дождитесь обновления лимита или выберите подписку."
            )
        if paid == "credit":
            pending = db.conn.execute(
                "select count(*) from download_claims where tg_id=? and paid='credit'",
                (tg_id,),
            ).fetchone()[0]
            if db.credits(tg_id) <= pending:
                raise RuntimeError("Все оплаченные скачивания уже используются.")
        if paid == "sub" and not db.sub_active(tg_id):
            raise RuntimeError(
                "Подписка закончилась. Продлите её и повторите скачивание."
            )
        db.conn.execute(
            "insert into download_claims(id,tg_id,paid,expires_at) values (?,?,?,?)",
            (identifier, tg_id, paid, now() + 7200),
        )


def backup_database(cfg):
    from .database_crypto import connect_database
    folder = cfg.data_dir / "backups"
    folder.mkdir(parents=True, exist_ok=True)
    vault = ApiSecrets(cfg.data_dir)
    with tempfile.TemporaryDirectory(dir=cfg.data_dir) as temporary:
        snapshot = Path(temporary) / "snapshot.db"
        with (
            closing(connect_database(cfg.db_path)) as source,
            closing(connect_database(snapshot)) as target,
        ):
            source.backup(target)
            if target.execute("pragma quick_check").fetchone()[0] != "ok":
                raise RuntimeError("Backup integrity check failed")
        encrypted = vault.cipher.encrypt(gzip.compress(snapshot.read_bytes()))
    stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S-%f")
    path = folder / f"database-{stamp}.db.gz.enc"
    with open(path, "xb") as stream:
        os.chmod(path, 0o600)
        stream.write(encrypted)
    for old in sorted(folder.glob("database-*.db.gz.enc"), reverse=True)[28:]:
        old.unlink()
    return path


async def backup_loop(cfg):
    while True:
        try:
            await asyncio.to_thread(backup_database, cfg)
            LOG.info("Encrypted database backup complete")
        except Exception as exc:
            LOG.error("Database backup failed (%s)", type(exc).__name__)
        await asyncio.sleep(6 * 3600)
