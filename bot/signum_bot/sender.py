from __future__ import annotations

import asyncio
import logging
import time
import os
from contextlib import suppress
from pathlib import Path
from typing import Callable

from telethon import TelegramClient
from aiogram import Bot
from telethon.tl.types import InputPeerUser

from .config import Config
from .ota import human_size

# Подписка Signum не меняет серверный лимит Telegram.
MTPROTO_MAX = 2000 * 1024 * 1024
LOG = logging.getLogger("signum-bot.sender")


class FileSender:
    """Отправка файлов через MTProto.

    Bot API отдаёт документом максимум 50 МБ — этого не хватает почти ни на одно
    приложение. Telethon ходит тем же ботом, но напрямую по MTProto, и поднимает
    потолок до 2 ГБ. Логика бота при этом остаётся на aiogram.
    """

    def __init__(self, cfg: Config) -> None:
        self.cfg = cfg
        self.client: TelegramClient | None = None
        self.premium_client: TelegramClient | None = None
        self._lock = asyncio.Lock()
        self._start_lock = asyncio.Lock()
        self._max_file_bytes = MTPROTO_MAX
        self.sessions = None
        if cfg.session_encryption:
            from .telegram_sessions import TelegramSessions
            self.sessions = TelegramSessions(cfg.data_dir)

    @property
    def ready(self) -> bool:
        return bool(self.cfg.api_id and self.cfg.api_hash)

    def limit(self, plan_limit: int) -> int:
        """Сколько байт реально уедет в чат при таком тарифе."""
        if not self.ready:
            return min(plan_limit, self.cfg.upload_limit)
        return min(
            plan_limit, self._max_file_bytes, self.cfg.mtproto_max_mb * 1024 * 1024
        )

    async def start(self) -> None:
        async with self._start_lock:
            try:
                async with asyncio.timeout(45):
                    await self._start()
            except BaseException:
                await self.close()
                raise

    async def _start(self) -> None:
        if not self.ready or self.client is not None:
            return
        client = TelegramClient(
            self.sessions.acquire(self.cfg.data_dir / "sender.session")
            if self.sessions else str(self.cfg.data_dir / "sender.session"),
            self.cfg.api_id,
            self.cfg.api_hash,
            receive_updates=False,  # обновления забирает aiogram, дублировать незачем
            **self.transport(),
        )
        try:
            await client.start(bot_token=self.cfg.token)
            me = await client.get_me()
            # У обычного бота нет Premium; настройка не обходит этот лимит.
            self._max_file_bytes = (4000 if me.premium else 2000) * 1024 * 1024
        except BaseException:
            await client.disconnect()
            raise
        client.parse_mode = "html"
        self.client = client
        if self.sessions:
            self.sessions.save()
        if self.cfg.premium_session and self.cfg.upload_relay:
            await self._connect_premium()

    async def _connect_premium(self) -> None:
        session_path = Path(self.cfg.premium_session)
        if not session_path.is_absolute():
            session_path = self.cfg.data_dir / session_path
        if not (self.sessions.exists(session_path) if self.sessions else session_path.with_suffix(".session").is_file()):
            LOG.warning("Premium upload session missing; run tools/connect_premium.py")
            return
        client = TelegramClient(
            self.sessions.acquire(session_path) if self.sessions else str(session_path),
            self.cfg.api_id, self.cfg.api_hash, receive_updates=False,
            **self.transport(),
        )
        try:
            await client.connect()
            if not await client.is_user_authorized():
                raise RuntimeError("Premium session needs authorization")
            me = await client.get_me()
            if me.bot or not me.premium:
                raise RuntimeError("Upload account has no Telegram Premium")
            await client.get_input_entity(self.cfg.upload_relay)
            async with Bot(self.cfg.token) as bot:
                bot_me = await bot.get_me()
                member = await bot.get_chat_member(self.cfg.upload_relay, bot_me.id)
                if member.status not in {"administrator", "creator"}:
                    raise RuntimeError(
                        "Bot must be an administrator in the relay channel"
                    )
            client.parse_mode = "html"
            self.premium_client = client
            if self.sessions:
                self.sessions.save()
            self._max_file_bytes = 4000 * 1024 * 1024
            LOG.info("Premium upload relay connected")
        except asyncio.CancelledError:
            await client.disconnect()
            raise
        except Exception as exc:
            LOG.warning(
                "Premium upload relay unavailable (%s); normal upload remains active",
                type(exc).__name__,
            )
            await client.disconnect()

    def transport(self):
        if os.environ.get("TG_TRANSPORT_URL"):
            from .telegram_transport import ConnectionWebsocket
            return {"connection": ConnectionWebsocket}
        return {}

    async def close(self) -> None:
        if self.premium_client is not None:
            await self.premium_client.disconnect()
            self.premium_client = None
        if self.client is not None:
            await self.client.disconnect()
            self.client = None
        if self.sessions:
            self.sessions.close()

    async def send(
        self,
        chat_id: int,
        path: Path,
        caption: str,
        on_progress: Callable[[str], None] | None = None,
    ) -> None:
        """Заливает файл в чат. Бросает исключение, если Telegram не принял."""
        await self.start()
        if self.client is None:
            raise RuntimeError("MTProto не настроен: нужны TG_API_ID и TG_API_HASH")
        if path.stat().st_size > self.limit(path.stat().st_size):
            raise ValueError("Файл больше доступного размера для отправки в чат")
        last = 0.0

        def report(sent: int, size: int) -> None:
            nonlocal last
            if on_progress is None:
                return
            now = time.monotonic()
            if now - last < 2 and sent < size:
                return
            last = now
            share = sent / size if size else 1
            on_progress(f"{human_size(sent)} из {human_size(size)}  ·  {share:.0%}")

        # Заливка одна на бота: параллельные большие файлы забивают канал и
        # чаще ловят таймаут, чем доезжают.
        async with self._lock:
            if path.stat().st_size > MTPROTO_MAX and self.premium_client:
                await self._send_via_relay(chat_id, path, caption, report)
                return
            await self.client.send_file(
                InputPeerUser(
                    chat_id, 0
                ),  # бот знает собеседника, access_hash не нужен
                str(path),
                caption=caption[:1024],
                file_name=path.name,
                force_document=True,
                part_size_kb=512,
                progress_callback=report,
            )

    async def _send_via_relay(
        self, chat_id: int, path: Path, caption: str, report
    ) -> None:
        assert self.premium_client is not None
        account = await self.premium_client.get_me()
        if not account.premium:
            self._max_file_bytes = MTPROTO_MAX
            raise RuntimeError("Подписка Telegram Premium у отправителя закончилась")
        posted = await self.premium_client.send_file(
            self.cfg.upload_relay,
            str(path),
            caption=caption[:1024],
            force_document=True,
            part_size_kb=512,
            progress_callback=report,
        )
        try:
            async with Bot(self.cfg.token) as bot:
                await bot.copy_message(
                    chat_id=chat_id,
                    from_chat_id=self.cfg.upload_relay,
                    message_id=posted.id,
                )
        finally:
            with suppress(Exception):
                await self.premium_client.delete_messages(
                    self.cfg.upload_relay, [posted.id]
                )
