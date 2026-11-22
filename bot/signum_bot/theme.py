from __future__ import annotations

import logging
import re

from aiogram import Bot
from aiogram.client.default import Default
from aiogram.client.session.middlewares.base import BaseRequestMiddleware
from aiogram.exceptions import TelegramBadRequest
from aiogram.types import InlineKeyboardMarkup

from .selected_emojis import load_selection, numbered_text, selected_icon

LOG = logging.getLogger("signum-bot.theme")


class Theme(BaseRequestMiddleware):
    def __init__(self, emojis: dict[str, str]) -> None:
        self.selection = load_selection()
        emojis = {**emojis, **self.selection.get("text", {})}
        self.emojis = emojis
        self.custom_enabled = bool(emojis or self.selection)
        self.styles_enabled = True
        self.pattern = (
            re.compile(
                "|".join(re.escape(e) for e in sorted(emojis, key=len, reverse=True))
            )
            if emojis
            else None
        )

    @classmethod
    async def load(cls, bot: Bot) -> "Theme":
        try:
            stickers = await bot.get_forum_topic_icon_stickers()
            emojis = {
                s.emoji: s.custom_emoji_id
                for s in stickers
                if s.emoji and s.custom_emoji_id and (s.is_animated or s.is_video)
            }
        except Exception:
            LOG.info("Custom emoji list unavailable; using standard icons")
            emojis = {}
        return cls(emojis)

    def text(self, value: str) -> str:
        if not self.custom_enabled or "<tg-emoji" in value:
            return value
        value = numbered_text(value, self.selection["numbers"])
        if not self.pattern:
            return value
        return "".join(
            part
            if part.startswith("<")
            else self.pattern.sub(
                lambda match: (
                    f'<tg-emoji emoji-id="{self.emojis[match[0]]}">{self.selection.get("fallbacks", {}).get(match[0], match[0])}</tg-emoji>'
                ),
                part,
            )
            for part in re.split(r"(<[^>]*>)", value)
        )

    def keyboard(self, markup: InlineKeyboardMarkup) -> InlineKeyboardMarkup:
        rows = []
        for row in markup.inline_keyboard:
            buttons = []
            for button in row:
                changes = {}
                action = button.callback_data or ""
                if self.styles_enabled:
                    if action.startswith(("dl:", "ota:", "pay:", "demo:", "acadd")) or (
                        button.url and button.text == "Установить"
                    ):
                        changes["style"] = "success"
                    elif action.startswith(("stop:", "acd:")):
                        changes["style"] = "danger"
                    elif action in {"cat", "srch", "buy"}:
                        changes["style"] = "primary"
                if self.custom_enabled:
                    selected = selected_icon(button, self.selection)
                    emoji = next(
                        (e for e in self.emojis if button.text.startswith(e)), None
                    )
                    if selected:
                        changes.update(
                            text=selected[1], icon_custom_emoji_id=selected[0]
                        )
                    elif emoji:
                        changes.update(
                            text=button.text[len(emoji) :].strip(),
                            icon_custom_emoji_id=self.emojis[emoji],
                        )
                buttons.append(button.model_copy(update=changes))
            rows.append(buttons)
        return markup.model_copy(update={"inline_keyboard": rows})

    def decorate(self, method):
        changes = {}
        markup = getattr(method, "reply_markup", None)
        if isinstance(markup, InlineKeyboardMarkup):
            changes["reply_markup"] = self.keyboard(markup)
        if not getattr(method, "entities", None) and not getattr(
            method, "caption_entities", None
        ):
            for key in ("text", "caption"):
                value = getattr(method, key, None)
                mode = getattr(method, "parse_mode", None)
                if isinstance(value, str) and (
                    mode == "HTML" or isinstance(mode, Default)
                ):
                    changes[key] = self.text(value)
        media = getattr(method, "media", None)
        media_mode = getattr(media, "parse_mode", None)
        if (
            media is not None
            and hasattr(media, "caption")
            and media.caption
            and (media_mode == "HTML" or isinstance(media_mode, Default))
        ):
            changes["media"] = media.model_copy(
                update={"caption": self.text(media.caption)}
            )
        return method.model_copy(update=changes)

    async def __call__(self, make_request, bot, method):
        try:
            return await make_request(bot, self.decorate(method))
        except TelegramBadRequest as exc:
            reason = str(exc).lower()
            if self.custom_enabled and any(
                word in reason for word in ("emoji", "premium", "entity")
            ):
                self.custom_enabled = False
                LOG.info(
                    "Custom emoji unavailable for this bot; standard icons enabled"
                )
                return await make_request(bot, self.decorate(method))
            if self.styles_enabled and "style" in reason:
                self.styles_enabled = False
                return await make_request(bot, self.decorate(method))
            raise
