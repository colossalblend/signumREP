from __future__ import annotations

import asyncio
import os
import sys
from dataclasses import replace
from pathlib import Path
from unittest.mock import AsyncMock

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
os.environ["BOT_TOKEN"] = "preview:token"

from signum_bot.cards import Cards  # noqa: E402
from signum_bot.config import Config  # noqa: E402


async def main() -> None:
    output = ROOT / "design-preview"
    output.mkdir(exist_ok=True)
    cards = Cards(replace(Config.load(), data_dir=output))
    cards.icon = AsyncMock(return_value=None)
    for screen in ("home", "catalog", "search"):
        (output / f"{screen}.jpg").write_bytes(await cards.banner(screen))
    apps = [
        ("", name, note)
        for name, note in (
            ("СберБанк", "17.6 · iOS 15+"),
            ("Т-Банк", "7.12 · iOS 16+"),
            ("Альфа-Банк", "15.2 · iOS 15+"),
            ("ВТБ Онлайн", "20.1 · iOS 15+"),
            ("Газпромбанк", "5.8 · iOS 14+"),
            ("Совкомбанк", "8.4 · iOS 15+"),
            ("Яндекс Музыка", "2026.9 · iOS 16+"),
            ("VK", "8.9 · iOS 15+"),
        )
    ]
    (output / "grid.jpg").write_bytes(
        await cards.grid(
            "Каталог · Банки и сервисы", "Пример карточек · 8 приложений", apps
        )
    )
    with Image.open(ROOT / "signum_bot/assets/signum.png") as logo:
        cards.icon = AsyncMock(return_value=logo.convert("RGBA"))
    (output / "app.jpg").write_bytes(
        await cards.hero(
            {
                "title": "Signum",
                "artist": "Твоя библиотека iOS",
                "header": "Приложение",
            },
            [
                "Версия · 1.1.6",
                "Нужна iOS · 15.0+",
                "Размер · 125 МБ",
                "ID · com.signum.app",
            ],
        )
    )
    print(output)


if __name__ == "__main__":
    asyncio.run(main())
