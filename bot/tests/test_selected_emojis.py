from __future__ import annotations

import unittest
from unittest.mock import AsyncMock

from aiogram.exceptions import TelegramBadRequest
from aiogram.methods import SendMessage
from aiogram.types import InlineKeyboardButton, InlineKeyboardMarkup

from signum_bot import ui
from signum_bot.selected_emojis import load_selection, numbered_text
from signum_bot.theme import Theme


class SelectedEmojiTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.theme = Theme({})
        self.selection = load_selection()

    def button(self, text, action):
        markup = InlineKeyboardMarkup(
            inline_keyboard=[[InlineKeyboardButton(text=text, callback_data=action)]]
        )
        return self.theme.keyboard(markup).inline_keyboard[0][0]

    def test_main_menu_has_selected_icons_and_preserves_actions(self):
        original = ui.main_menu(False)
        decorated = self.theme.keyboard(original)
        for source_row, row in zip(original.inline_keyboard, decorated.inline_keyboard):
            for source, button in zip(source_row, row):
                self.assertEqual(source.callback_data, button.callback_data)
                icon = self.selection["actions"].get(button.callback_data)
                if icon:
                    self.assertEqual(button.model_dump()["icon_custom_emoji_id"], icon)
                    self.assertFalse(
                        button.text.startswith(("▦", "⌕", "📥", "🍏", "ℹ"))
                    )
        self.assertEqual(original.inline_keyboard[0][0].text, "▦ Каталог Signum")

    def test_bank_logos_are_on_catalog_sections_and_apps_keep_numbers(self):
        for slug, family in [
            ("tbank", "tbank"),
            ("sber", "sber"),
            ("vk", "vk"),
            ("mts", "mts"),
        ]:
            with self.subTest(slug=slug):
                button = self.button("📂 Раздел", f"cg:{slug}:0")
                self.assertEqual(
                    button.icon_custom_emoji_id, self.selection["catalog"][family]
                )
                self.assertEqual(button.text, "Раздел")
        button = self.button("1.  Приложение", "ca:sber-invest")
        self.assertEqual(button.icon_custom_emoji_id, self.selection["numbers"]["1"])
        button = self.button("2.  Другое приложение", "ca:unlisted")
        self.assertEqual(button.icon_custom_emoji_id, self.selection["numbers"]["2"])

    def test_number_icons_keep_titles_and_latest_version_star(self):
        for digit in range(1, 9):
            button = self.button(f"{digit}.  ⭐️ Версия", "vsel:123:456")
            self.assertEqual(
                button.icon_custom_emoji_id, self.selection["numbers"][str(digit)]
            )
            self.assertEqual(button.text, "⭐️ Версия")

    def test_back_buttons_take_priority_over_target_action(self):
        button = self.button("◀︎ Назад", "srch")
        self.assertEqual(
            button.icon_custom_emoji_id, self.selection["navigation"]["back"]
        )
        self.assertEqual(button.text, "Назад")
        for text, direction, label in [
            ("◀︎", "back", "Назад"),
            ("▶︎", "forward", "Вперёд"),
        ]:
            button = self.button(text, "noop")
            self.assertEqual(
                button.icon_custom_emoji_id, self.selection["navigation"][direction]
            )
            self.assertEqual(button.text, label)
        button = self.button("⌂ Главное меню", "menu")
        self.assertEqual(
            button.icon_custom_emoji_id, self.selection["navigation"]["menu"]
        )

    def test_application_actions_keep_versions_and_target(self):
        markup = ui.app_kb("123", back_to="ca:tbank")
        decorated = self.theme.keyboard(markup)
        for row in decorated.inline_keyboard[:3]:
            button = row[0]
            action = button.callback_data.split(":")[0] + ":"
            self.assertEqual(
                button.icon_custom_emoji_id, self.selection["actions"][action]
            )
        self.assertEqual(decorated.inline_keyboard[0][0].callback_data, "dl:123")

    def test_numbered_instructions_preserve_html_and_other_numbers(self):
        value = (
            "<b>1.</b> Войти\n2. Скачать\n10. Готово\nID: 5242407832770340528\n١. Текст"
        )
        result = numbered_text(value, self.selection["numbers"])
        self.assertIn(
            f'<b><tg-emoji emoji-id="{self.selection["numbers"]["1"]}">1️⃣</tg-emoji>.</b> Войти',
            result,
        )
        self.assertEqual(result.count("<tg-emoji"), 4)
        self.assertIn("ID: 5242407832770340528", result)
        self.assertIn("١. Текст", result)
        self.assertEqual(
            self.theme.decorate(
                SendMessage(chat_id=1, text=value, parse_mode=None)
            ).text,
            value,
        )

    async def test_permission_rejection_restores_plain_menu(self):
        method = SendMessage(
            chat_id=1,
            text="<b>1.</b> Войти",
            parse_mode="HTML",
            reply_markup=ui.main_menu(False),
        )
        request = AsyncMock(
            side_effect=[
                TelegramBadRequest(method=method, message="CUSTOM_EMOJI_INVALID"),
                "sent",
            ]
        )
        self.assertEqual(await self.theme(request, None, method), "sent")
        first = request.await_args_list[0].args[1]
        fallback = request.await_args_list[1].args[1]
        self.assertIn("tg-emoji", first.text)
        self.assertEqual(fallback.text, method.text)
        self.assertEqual(
            fallback.reply_markup.inline_keyboard[0][0].text, "▦ Каталог Signum"
        )
        self.assertIsNone(
            getattr(
                fallback.reply_markup.inline_keyboard[0][0],
                "icon_custom_emoji_id",
                None,
            )
        )


if __name__ == "__main__":
    unittest.main()
