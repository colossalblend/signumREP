from __future__ import annotations

import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

from aiogram.methods import SendMessage
from aiogram.types import MessageEntity

from signum_bot.cards import Cards
from signum_bot.config import Config
from signum_bot.db import Db
from signum_bot.emoji_ids import replace_custom_emojis
from signum_bot.handlers import Adm, Ask, Auth, Deps, build_router
from signum_bot.ipatool import Ipatool
from signum_bot.sender import FileSender
from signum_bot.store import Store
from signum_bot.theme import Theme


def entity(text, emoji, identifier):
    start = text.index(emoji)
    return MessageEntity(
        type="custom_emoji",
        offset=len(text[:start].encode("utf-16-le")) // 2,
        length=len(emoji.encode("utf-16-le")) // 2,
        custom_emoji_id=identifier,
    )


class EmojiIdTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        cfg = replace(
            Config.load(), token="test:token", admins={1}, data_dir=Path(self.tmp.name)
        )
        self.db = Db(cfg.db_path)
        self.addCleanup(self.db.conn.close)
        self.router = build_router(
            Deps(
                cfg,
                self.db,
                Store(cfg),
                Ipatool(cfg, self.db),
                FileSender(cfg),
                Cards(cfg),
            )
        )
        self.handler = next(
            h
            for h in self.router.message.handlers
            if h.callback.__name__ == "emoji_extract"
        )
        self.state = AsyncMock()

    def message(self, text, entities, user_id=1, caption=False):
        return SimpleNamespace(
            text=None if caption else text,
            caption=text if caption else None,
            entities=None if caption else entities,
            caption_entities=entities if caption else None,
            from_user=SimpleNamespace(id=user_id),
            chat=SimpleNamespace(type="private"),
            answer=AsyncMock(),
            answer_document=AsyncMock(),
        )

    async def test_multiline_text_preserves_plain_emojis_and_html(self):
        text = "🧩 Каталог Signum - 🗂️\nПоиск — 🔎\n<b>Текст</b> 🐈"
        entities = [
            entity(text, "🔎", "2222222222222222222"),
            entity(text, "🗂️", "1111111111111111111"),
        ]
        expected = "🧩 Каталог Signum - 1111111111111111111\nПоиск — 2222222222222222222\n<b>Текст</b> 🐈"
        self.assertEqual(replace_custom_emojis(text, entities), expected)
        message = self.message(text, entities)
        matched, _ = await self.handler.check(message, raw_state=None)
        self.assertTrue(matched)
        await self.handler.callback(message, self.state)
        message.answer.assert_awaited_once_with(expected, parse_mode=None)
        message.answer_document.assert_not_awaited()
        theme = Theme({"🐈": "333"})
        method = SendMessage(chat_id=1, text=expected, parse_mode=None)
        self.assertEqual(theme.decorate(method).text, expected)

    async def test_caption_and_repeated_emoji_are_replaced(self):
        text = "🗂 + 🗂"
        entities = [
            MessageEntity(
                type="custom_emoji", offset=0, length=2, custom_emoji_id="111"
            ),
            MessageEntity(
                type="custom_emoji", offset=5, length=2, custom_emoji_id="222"
            ),
        ]
        message = self.message(text, entities, caption=True)
        matched, _ = await self.handler.check(message, raw_state=None)
        self.assertTrue(matched)
        await self.handler.callback(message, self.state)
        message.answer.assert_awaited_once_with("111 + 222", parse_mode=None)

    async def test_normal_messages_and_other_users_are_not_intercepted(self):
        text = "Каталог — 🗂"
        for message in (
            self.message(text, []),
            self.message(text, [entity(text, "🗂", "111")], user_id=2),
        ):
            matched, _ = await self.handler.check(message, raw_state=None)
            self.assertFalse(matched)
        message = self.message(text, [entity(text, "🗂", "111")])
        for state in (Auth.password.state, Auth.code.state, Adm.broadcast.state):
            matched, _ = await self.handler.check(message, raw_state=state)
            self.assertFalse(matched)
        matched, _ = await self.handler.check(message, raw_state=Ask.search.state)
        self.assertTrue(matched)

    async def test_expanded_prompt_is_returned_in_full_when_too_long(self):
        text = "🗂 " * 300
        entities = [
            MessageEntity(
                type="custom_emoji",
                offset=i * 3,
                length=2,
                custom_emoji_id="1111111111111111111",
            )
            for i in range(300)
        ]
        message = self.message(text, entities)
        await self.handler.callback(message, self.state)
        message.answer.assert_not_awaited()
        message.answer_document.assert_awaited_once()
        document = message.answer_document.call_args.args[0]
        self.assertEqual(document.data.decode("utf-8"), "1111111111111111111 " * 300)


if __name__ == "__main__":
    unittest.main()
