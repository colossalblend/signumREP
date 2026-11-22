from __future__ import annotations

import json
import re
from pathlib import Path

from aiogram.types import InlineKeyboardButton

CONFIG_PATH = Path(__file__).parent / "assets" / "button-emojis.json"
LABEL = re.compile(r"[A-Za-zА-Яа-яЁё0-9].*", re.S)
ORDINAL = re.compile(r"^([0-9]+)\.\s*")


def load_selection() -> dict:
    return json.loads(CONFIG_PATH.read_text(encoding="utf-8"))


def label_text(text: str) -> str:
    match = LABEL.search(text)
    return match[0] if match else text


def selected_icon(
    button: InlineKeyboardButton, selection: dict
) -> tuple[str, str] | None:
    text = button.text.strip()
    label = label_text(text).casefold()
    action = button.callback_data or ""
    if action == "menu" and not label.startswith("назад"):
        return selection["navigation"]["menu"], label_text(text)
    if button.url and label in selection.get("labels", {}):
        return selection["labels"][label], label_text(text)
    if text.startswith(("◀", "⬅")) or label.startswith("назад"):
        return selection["navigation"]["back"], (
            label_text(text) if LABEL.search(text) else "Назад"
        )
    if text.startswith(("▶", "➡")) or label.startswith(("вперед", "вперёд")):
        return selection["navigation"]["forward"], (
            label_text(text) if LABEL.search(text) else "Вперёд"
        )
    if action == "menu":
        return selection["navigation"]["menu"], label_text(text)
    if action.startswith("cg:"):
        slug = action.split(":")[1]
        icon = selection["catalog"].get(slug)
        if icon is None:
            icon = next(
                (
                    value
                    for prefix, value in selection["catalog"].items()
                    if slug.startswith(prefix + "-")
                ),
                None,
            )
        if icon:
            return icon, label_text(text)
    icon = selection["actions"].get(action)
    if icon is None:
        icon = selection["actions"].get(action.split(":")[0] + ":")
    if icon:
        return icon, label_text(text)
    ordinal = ORDINAL.match(text)
    if ordinal:
        icon = selection["numbers"].get(ordinal[1])
        if icon:
            return icon, text[ordinal.end() :]
    return None


def numbered_text(text: str, numbers: dict[str, str]) -> str:
    def replace(match):
        digits = "".join(
            f'<tg-emoji emoji-id="{numbers[digit]}">{digit}\ufe0f\u20e3</tg-emoji>'
            for digit in match["digits"]
        )
        return match["prefix"] + digits + match["suffix"]

    return re.sub(
        r"(?m)^(?P<prefix>\s*(?:<b>)?)(?P<digits>[0-9]+)(?P<suffix>\.(?:</b>)?\s)",
        replace,
        text,
    )
