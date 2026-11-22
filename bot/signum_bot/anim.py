from __future__ import annotations

import asyncio
import html
import time
from typing import Any

from aiogram import Bot
from aiogram.exceptions import (
    TelegramBadRequest,
    TelegramNetworkError,
    TelegramRetryAfter,
)
from aiogram.types import InlineKeyboardMarkup, Message

# Правку сообщения делаем через колбэк: мастер входа держит только id чата и
# сообщения, а карточка приложения — сам объект Message.
Editor = Any

FRAMES = ("🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘")
FILLED, EMPTY = "▰", "▱"
# Telegram считает правки в общий лимит сообщений на чат, поэтому кадр раз в
# 4 секунды: анимация живая, а до 429 далеко даже на получасовой закачке.
TICK = 4.0


async def waiting(
    message: Message,
    operation,
    title: str = "Загружаю список",
    *,
    new_message: bool = False,
):
    task = asyncio.ensure_future(operation)
    progress = None
    note = None
    try:
        done, _ = await asyncio.wait({task}, timeout=0.7)
        if not done:
            note = (
                await message.answer("Готовлю приложение…") if new_message else message
            )
            progress = Progress.of(note, "")
            await progress.start(title)
        return await task
    finally:
        if progress:
            progress.stop()
        if new_message and note:
            try:
                await note.delete()
            except (TelegramBadRequest, TelegramNetworkError):
                pass
        if not task.done():
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass


def bar(done: int, total: int, width: int = 10) -> str:
    total = max(1, total)
    filled = max(0, min(width, round(width * done / total)))
    return FILLED * filled + EMPTY * (width - filled)


class Progress:
    """Одно живое сообщение на всю операцию: спиннер, шаги, прогресс-бар.

    Правки идут по таймеру, а не на каждый чих: иначе Telegram начинает
    отвечать 429 ровно в тот момент, когда пользователю важнее всего видеть,
    что бот жив.
    """

    def __init__(self, edit: Editor, title: str, steps: int) -> None:
        self.edit = edit
        self.title = title
        self.steps = max(1, steps)
        self.step = 0
        self.stage = ""
        self.detail = ""
        self._frame = 0
        self._last = ""
        self._task: asyncio.Task[Any] | None = None
        self.markup: InlineKeyboardMarkup | None = None
        self.percent: float | None = None
        self._started = time.monotonic()

    def render(self) -> str:
        spin = FRAMES[self._frame % len(FRAMES)]
        head = f"{spin} <b>{html.escape(self.stage)}</b>"
        rows = [head, ""]
        if self.title:
            rows.extend([f"<b>{html.escape(self.title)}</b>", ""])
        if self.percent is None:
            position = self._frame % 9
            rows.append(EMPTY * position + FILLED * 2 + EMPTY * (8 - position))
        else:
            rows.append(f"{bar(int(self.percent), 100)}  {self.percent:.0f}%")
        if self.detail:
            rows.append(f"<i>{html.escape(self.detail)}</i>")
        elapsed = int(time.monotonic() - self._started)
        if elapsed >= 10:
            rows.append(f"<i>Время ожидания: {elapsed // 60}:{elapsed % 60:02d}</i>")
        return "\n".join(rows)

    async def _push(self) -> None:
        text = self.render()
        if text == self._last:
            return
        try:
            async with asyncio.timeout(10):
                await self.edit(text, self.markup)
            self._last = text
        except TelegramRetryAfter as exc:
            await asyncio.sleep(exc.retry_after)
        except TelegramBadRequest:
            pass
        except TelegramNetworkError:
            pass
        except TimeoutError:
            pass

    async def _loop(self) -> None:
        while True:
            await asyncio.sleep(TICK)
            self._frame += 1
            await self._push()

    async def start(self, stage: str, detail: str = "") -> "Progress":
        self.stage, self.detail = stage, detail
        await self._push()
        self._task = asyncio.create_task(self._loop())
        return self

    async def to(
        self, stage: str, detail: str = "", percent: float | None = None
    ) -> None:
        self.percent = percent
        self.step = min(self.steps, self.step + 1)
        self.stage, self.detail = stage, detail
        self._frame += 1
        await self._push()

    def hint(self, detail: str) -> None:
        """Меняет подпись без запроса к Telegram: подхватит следующий кадр."""
        self.detail = detail

    async def finish(
        self, text: str, markup: InlineKeyboardMarkup | None = None
    ) -> None:
        self.stop()
        try:
            await self.edit(text, markup)
        except TelegramBadRequest:
            pass

    def stop(self) -> None:
        if self._task and not self._task.done():
            self._task.cancel()
        self._task = None

    @classmethod
    def of(cls, message: Message, title: str, steps: int = 1) -> "Progress":
        async def edit(text: str, markup: InlineKeyboardMarkup | None) -> None:
            if message.photo:
                await message.edit_caption(caption=text, reply_markup=markup)
            else:
                await message.edit_text(text, reply_markup=markup)

        return cls(edit, title, steps)

    @classmethod
    def at(
        cls,
        bot: Bot,
        chat_id: int,
        message_id: int,
        title: str,
        steps: int = 1,
        *,
        photo: bool = False,
    ) -> "Progress":
        async def edit(text: str, markup: InlineKeyboardMarkup | None) -> None:
            if photo:
                await bot.edit_message_caption(
                    caption=text,
                    chat_id=chat_id,
                    message_id=message_id,
                    reply_markup=markup,
                )
            else:
                await bot.edit_message_text(
                    text, chat_id=chat_id, message_id=message_id, reply_markup=markup
                )

        return cls(edit, title, steps)

    async def __aenter__(self) -> "Progress":
        return self

    async def __aexit__(self, *_exc: Any) -> None:
        self.stop()
