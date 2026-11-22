from __future__ import annotations

import asyncio
import base64
import hashlib
import io
import re
from pathlib import Path
from typing import Iterable, Sequence
from functools import lru_cache
from urllib.parse import urlsplit

import httpx
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont
from cachetools import TTLCache

from .config import Config

# 16:9 — Telegram показывает такую картинку во всю ширину и не режет её.
W, H = 1280, 720
COLS, ROWS = 4, 2
PER_CARD = COLS * ROWS

BG_TOP = (22, 29, 44)
BG_BOTTOM = (15, 20, 32)
GLOW = (89, 107, 245)
PANEL = (25, 34, 51, 255)
PANEL_LINE = (66, 80, 105, 150)
TEXT = (238, 241, 255)
MUTED = (134, 142, 178)
ACCENT_A = (89, 107, 245)
ACCENT_B = (112, 210, 222)
ASSETS = Path(__file__).parent / "assets"

FONT_DIRS = (
    "/usr/share/fonts/truetype/dejavu",
    "/usr/share/fonts/truetype/liberation",
    "C:/Windows/Fonts",
    "/System/Library/Fonts",
)
FONT_FILES = {
    "bold": (
        "DejaVuSans-Bold.ttf",
        "LiberationSans-Bold.ttf",
        "seguisb.ttf",
        "arialbd.ttf",
    ),
    "regular": (
        "DejaVuSans.ttf",
        "LiberationSans-Regular.ttf",
        "segoeui.ttf",
        "arial.ttf",
    ),
}


@lru_cache(maxsize=32)
def _font(kind: str, size: int) -> ImageFont.FreeTypeFont:
    for directory in FONT_DIRS:
        for name in FONT_FILES[kind]:
            path = Path(directory) / name
            if path.is_file():
                try:
                    return ImageFont.truetype(str(path), size)
                except OSError:
                    continue
    return ImageFont.load_default(size)


# В системных шрифтах нет эмодзи — вместо них рисуются пустые квадраты.
# Проще выкинуть их из картинки, чем тащить второй шрифт на сотню мегабайт.
GLYPHLESS = re.compile("[\U0001f000-\U0001faff\u2190-\u2bff\ufe0f\u200d]")


def _fit(
    draw: ImageDraw.ImageDraw, text: str, font: ImageFont.FreeTypeFont, width: int
) -> str:
    """Подрезает строку под ширину плитки: обрывок лучше наезжающих букв."""
    text = " ".join(GLYPHLESS.sub("", text).split())
    if draw.textlength(text, font=font) <= width:
        return text
    while text and draw.textlength(text + "…", font=font) > width:
        text = text[:-1]
    return text.rstrip() + "…"


def _title_layout(draw, text, width, maximum=21, minimum=17):
    text = " ".join(GLYPHLESS.sub("", str(text)).split())[:500]

    def wrap(font):
        remaining, lines = text, []
        while remaining:
            if draw.textlength(remaining, font=font) <= width:
                lines.append(remaining)
                break
            low, high = 1, len(remaining)
            while low < high:
                middle = (low + high + 1) // 2
                if draw.textlength(remaining[:middle], font=font) <= width:
                    low = middle
                else:
                    high = middle - 1
            space = remaining.rfind(" ", 0, low + 1)
            end = space if space > 0 else low
            lines.append(remaining[:end].rstrip())
            remaining = remaining[end:].lstrip()
            if len(lines) == 2 and remaining:
                return lines + [remaining]
        return lines or [""]

    for size in range(maximum, minimum - 1, -1):
        font = _font("bold", size)
        lines = wrap(font)
        if len(lines) <= 2:
            return font, lines
    return font, [lines[0], _fit(draw, " ".join(lines[1:]), font, width)]


def _centered(
    draw: ImageDraw.ImageDraw,
    text: str,
    font: ImageFont.FreeTypeFont,
    cx: float,
    y: float,
    fill,
) -> None:
    draw.text(
        (cx - draw.textlength(text, font=font) / 2, y), text, font=font, fill=fill
    )


def _center_in(
    draw: ImageDraw.ImageDraw,
    text: str,
    font: ImageFont.FreeTypeFont,
    cx: float,
    cy: float,
    fill,
) -> None:
    """Ставит короткую надпись ровно в центр кружка.

    Считаем по чернильной рамке глифа, а не по высоте строки: у цифр сверху и
    снизу разный запас, и от него надпись уезжает вверх и влево.
    """
    box = draw.textbbox((0, 0), text, font=font)
    draw.text(
        (cx - (box[0] + box[2]) / 2, cy - (box[1] + box[3]) / 2),
        text,
        font=font,
        fill=fill,
    )


def _lh(font: ImageFont.FreeTypeFont) -> int:
    """Высота строки со всеми выносными элементами.

    Раскладка считается от неё, а не от подобранных на глаз чисел: иначе при
    другом шрифте на сервере подписи вылезают за плитку.
    """
    ascent, descent = font.getmetrics()
    return ascent + descent


def _background() -> Image.Image:
    base = Image.new("RGB", (W, H), BG_BOTTOM)
    top = Image.new("RGB", (W, H), BG_TOP)
    mask = Image.linear_gradient("L").resize((W, H)).transpose(Image.FLIP_TOP_BOTTOM)
    canvas = Image.composite(top, base, mask).convert("RGBA")

    # Мягкое свечение сверху: картинка перестаёт быть плоским прямоугольником.
    glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse(
        (W // 2 - 560, -460, W // 2 + 560, 290), fill=(*GLOW, 60)
    )
    canvas = Image.alpha_composite(canvas, glow.filter(ImageFilter.GaussianBlur(130)))
    draw = ImageDraw.Draw(canvas)
    draw.line((64, H - 46, W - 64, H - 46), fill=(51, 63, 85), width=1)
    draw.text((64, H - 34), "SIGNUM / APP LIBRARY", font=_font("bold", 12), fill=MUTED)
    draw.text((W - 180, H - 34), "iPhone + iPad", font=_font("regular", 12), fill=MUTED)
    return canvas


SS = 4  # во столько раз рисуем маску крупнее, чтобы скруглению хватило полутонов


def _round_mask(size: int, radius: int) -> Image.Image:
    """Маска скругления с честным сглаживанием.

    Скруглённый прямоугольник рисуется без антиалиасинга, и на углах остаётся
    лесенка. Рисуем вчетверо крупнее и уменьшаем — края получаются мягкими.
    """
    mask = Image.new("L", (size * SS, size * SS), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (0, 0, size * SS - 1, size * SS - 1), radius=radius * SS, fill=255
    )
    return mask.resize((size, size), Image.LANCZOS)


def _rounded(image: Image.Image, size: int, radius: int) -> Image.Image:
    image = image.convert("RGBA").resize((size, size), Image.LANCZOS)
    # у иконки может быть своя прозрачность — не затираем её, а перемножаем
    image.putalpha(
        ImageChops.multiply(image.getchannel("A"), _round_mask(size, radius))
    )
    return image


def _placeholder(size: int, radius: int, letter: str) -> Image.Image:
    tile = Image.new("RGBA", (size, size), (255, 255, 255, 38))
    tile.putalpha(ImageChops.multiply(tile.getchannel("A"), _round_mask(size, radius)))
    draw = ImageDraw.Draw(tile)
    _center_in(draw, letter, _font("bold", int(size * 0.42)), size / 2, size / 2, TEXT)
    return tile


def _shadow_sprite(size: int, radius: int, blur: int = 16) -> Image.Image:
    """Одна мягкая тень на все иконки: считаем её один раз, а не под каждой."""
    pad = blur * 3
    sprite = Image.new("RGBA", (size + pad * 2, size + pad * 2), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (size, size), (0, 0, 0, 150))
    shadow.putalpha(
        ImageChops.multiply(shadow.getchannel("A"), _round_mask(size, radius))
    )
    sprite.alpha_composite(shadow, (pad, pad))
    return sprite.filter(ImageFilter.GaussianBlur(blur))


def overlay_of(canvas: Image.Image) -> tuple[Image.Image, ImageDraw.ImageDraw]:
    layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    return layer, ImageDraw.Draw(layer)


HEADER_TOP = 48
HEADER_BOTTOM = 152


def _header(canvas: Image.Image, title: str, subtitle: str) -> None:
    draw = ImageDraw.Draw(canvas)
    title_font, sub_font = _font("bold", 42), _font("regular", 22)
    draw.rounded_rectangle(
        (64, HEADER_TOP + 8, 70, HEADER_TOP + 52), radius=3, fill=ACCENT_A
    )
    draw.text(
        (90, HEADER_TOP), _fit(draw, title, title_font, 780), font=title_font, fill=TEXT
    )
    if subtitle:
        draw.text(
            (90, HEADER_TOP + _lh(title_font) + 6),
            _fit(draw, subtitle, sub_font, 880),
            font=sub_font,
            fill=MUTED,
        )
    mark = _font("bold", 22)
    draw.text(
        (W - 64 - draw.textlength("Signum", font=mark), HEADER_TOP + 14),
        "Signum",
        font=mark,
        fill=(96, 104, 142),
    )


class Cards:
    """Рисует 16:9-карточки со списками приложений.

    Иконки лежат на диске, готовые картинки — file_id в памяти: повторный показ
    того же экрана не тратит ни сеть, ни процессор.
    """

    def __init__(self, cfg: Config) -> None:
        self.cfg = cfg
        self.dir = cfg.data_dir / "icons"
        self.dir.mkdir(parents=True, exist_ok=True)
        self.file_ids: TTLCache = TTLCache(maxsize=256, ttl=1800)
        self.banners: TTLCache = TTLCache(maxsize=16, ttl=3600)

    async def banner(self, screen: str = "home") -> bytes:
        if screen in self.banners:
            return self.banners[screen]
        result = await asyncio.to_thread(self._banner, screen)
        self.banners[screen] = result
        return result

    def _banner(self, screen: str) -> bytes:
        titles = {
            "home": (
                "Твои приложения.",
                "Всегда под рукой.",
                "Каталог, версии и IPA в одном месте",
            ),
            "catalog": (
                "Найди своё.",
                "Верни любимое.",
                "Банки и сервисы из каталога Signum",
            ),
            "search": (
                "Одно сообщение.",
                "Нужное приложение.",
                "Название, bundle ID или ссылка App Store",
            ),
        }
        first, second, note = titles.get(screen, titles["home"])
        canvas = _background()
        draw = ImageDraw.Draw(canvas)
        draw.rounded_rectangle((64, 60, 234, 99), radius=19, fill=(39, 49, 73))
        draw.text((85, 68), "SIGNUM / iOS", font=_font("bold", 17), fill=TEXT)
        draw.text((64, 194), first, font=_font("bold", 52), fill=TEXT)
        draw.text((64, 262), second, font=_font("bold", 52), fill=ACCENT_B)
        draw.text((66, 362), note, font=_font("regular", 23), fill=MUTED)
        for x, label in ((64, "КАТАЛОГ"), (235, "APP STORE"), (420, "IPA")):
            width = int(draw.textlength(label, font=_font("bold", 16))) + 40
            draw.rounded_rectangle(
                (x, 453, x + width, 502),
                radius=15,
                fill=(31, 41, 61),
                outline=(60, 75, 101),
            )
            draw.text((x + 20, 468), label, font=_font("bold", 16), fill=TEXT)
        draw.rounded_rectangle(
            (902, 156, 1190, 516),
            radius=42,
            fill=(26, 35, 53),
            outline=(70, 84, 111),
            width=2,
        )
        logo = ASSETS / "signum.png"
        if logo.is_file():
            with Image.open(logo) as source:
                canvas.alpha_composite(_rounded(source, 184, 44), (954, 205))
        else:
            draw.text((993, 217), "S", font=_font("bold", 126), fill=ACCENT_A)
        draw = ImageDraw.Draw(canvas)
        _centered(draw, "Signum", _font("bold", 30), 1046, 418, TEXT)
        _centered(draw, "Твоя библиотека iOS", _font("regular", 16), 1046, 463, MUTED)
        return self._encode(canvas, 92)

    async def icon(self, url: str) -> Image.Image | None:
        if not url:
            return None
        path = self.dir / f"{hashlib.sha1(url.encode()).hexdigest()}.png"
        if not path.exists():
            try:
                async with httpx.AsyncClient(
                    timeout=httpx.Timeout(12, connect=3),
                    follow_redirects=True,
                    trust_env=False,
                ) as client:
                    if getattr(self.cfg, "bridge_enabled", False) and urlsplit(
                        url
                    ).hostname in {"cdn.appbank.pw", "api.appbank.pw"}:
                        from .bridge import BridgeWorker

                        result = await BridgeWorker(self.cfg).exchange(
                            client, "catalog_icon", url=url
                        )
                        content = base64.b64decode(result["data"], validate=True)
                    else:
                        response = await client.get(url)
                        response.raise_for_status()
                        content = response.content
                image = Image.open(io.BytesIO(content)).convert("RGBA")
                image.thumbnail((512, 512), Image.LANCZOS)
                image.save(path, "PNG")
            except Exception:  # noqa: BLE001 - без иконки просто рисуем букву
                return None
        try:
            return Image.open(path).convert("RGBA")
        except Exception:  # noqa: BLE001
            path.unlink(missing_ok=True)
            return None

    @staticmethod
    def _encode(canvas: Image.Image, quality: int) -> bytes:
        buffer = io.BytesIO()
        canvas.convert("RGB").save(buffer, "JPEG", quality=quality, optimize=True)
        return buffer.getvalue()

    async def grid(
        self, title: str, subtitle: str, items: Sequence[tuple[str, str, str]]
    ) -> bytes:
        """items: (иконка, название, приписка). Рисуем максимум восемь."""
        items = list(items)[:PER_CARD]
        icons = await asyncio.gather(*(self.icon(url) for url, _, _ in items))
        return await asyncio.to_thread(self._grid, title, subtitle, items, icons)

    def _grid(self, title, subtitle, items, icons) -> bytes:
        canvas = _background()

        name_font = _font("bold", 21)
        note_font = _font("regular", 17)
        badge_font = _font("bold", 16)

        # Высота плитки складывается из того, что в неё кладут, поэтому подпись
        # не может вылезти за край — сколько нужно, столько плитка и займёт.
        rows = max(1, -(-len(items) // COLS))
        text_w = 268 - 52
        layouts = [
            _title_layout(ImageDraw.Draw(canvas), name, text_w) for _, name, _ in items
        ]
        name_rows = max((len(lines) for _, lines in layouts), default=1)
        name_height = name_rows * _lh(name_font) + (name_rows - 1) * 3
        icon_size, radius = (96 if rows > 1 and name_rows > 1 else 120), 28
        pad_top, gap_name, gap_note, pad_bottom = 22, 14, 4, 16
        name_y = pad_top + icon_size + gap_name
        note_y = name_y + name_height + gap_note
        panel_h = note_y + _lh(note_font) + pad_bottom
        cell_w, cell_h = 268, panel_h + 30

        free = H - HEADER_BOTTOM - 64
        top = HEADER_BOTTOM + max(0, (free - rows * cell_h)) // 2

        positions = []
        for index in range(len(items)):
            row = index // COLS
            in_row = min(COLS, len(items) - row * COLS)
            # неполный ряд ставим по центру, иначе одинокая плитка жмётся в угол
            left = (W - cell_w * in_row) // 2
            positions.append((left + (index % COLS) * cell_w, top + row * cell_h))

        # 1. подложки под плитки
        panels, pdraw = overlay_of(canvas)
        for x, y in positions:
            pdraw.rounded_rectangle(
                (x + 12, y, x + cell_w - 12, y + panel_h),
                radius=28,
                fill=PANEL,
                outline=PANEL_LINE,
            )
        canvas = Image.alpha_composite(canvas, panels)

        # 2. тени под иконками
        sprite = _shadow_sprite(icon_size, radius)
        pad = (sprite.width - icon_size) // 2
        icon_x = (cell_w - icon_size) // 2
        shadows = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
        for x, y in positions:
            shadows.alpha_composite(sprite, (x + icon_x - pad, y + pad_top + 8 - pad))
        canvas = Image.alpha_composite(canvas, shadows)

        # 3. иконки
        for (x, y), (_, name, _), icon in zip(positions, items, icons):
            tile = (
                _rounded(icon, icon_size, radius)
                if icon
                else _placeholder(icon_size, radius, (name or "?")[:1].upper())
            )
            canvas.alpha_composite(tile, (x + icon_x, y + pad_top))

        # 4. подписи и номера — номер плитки совпадает с номером кнопки
        draw = ImageDraw.Draw(canvas)
        _header(canvas, title, subtitle)
        text_w = cell_w - 52
        for index, ((x, y), (_, name, note)) in enumerate(zip(positions, items), 1):
            bx, by, size_badge = x + 26, y + 10, 28
            draw.rounded_rectangle(
                (bx, by, bx + size_badge, by + size_badge), radius=9, fill=ACCENT_A
            )
            _center_in(
                draw,
                str(index),
                badge_font,
                bx + size_badge / 2,
                by + size_badge / 2,
                TEXT,
            )
            cx = x + cell_w / 2
            font, name_lines = layouts[index - 1]
            height = len(name_lines) * _lh(font) + (len(name_lines) - 1) * 3
            label_y = y + name_y + (name_height - height) / 2
            for line in name_lines:
                _centered(draw, line, font, cx, label_y, TEXT)
                label_y += _lh(font) + 3
            if note:
                _centered(
                    draw,
                    _fit(draw, note, note_font, text_w),
                    note_font,
                    cx,
                    y + note_y,
                    MUTED,
                )

        return self._encode(canvas, 88)

    async def hero(self, app: dict[str, str], lines: Iterable[str]) -> bytes:
        """Крупная карточка одного приложения."""
        icon = await self.icon(str(app.get("icon") or ""))
        return await asyncio.to_thread(self._hero, app, list(lines), icon)

    def _hero(self, app, lines, icon) -> bytes:
        canvas = _background()

        size, radius = 248, 56
        icon_x, icon_y = 104, HEADER_BOTTOM + (H - HEADER_BOTTOM - 40 - size) // 2
        sprite = _shadow_sprite(size, radius, blur=24)
        pad = (sprite.width - size) // 2
        shadows = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
        shadows.alpha_composite(sprite, (icon_x - pad, icon_y + 14 - pad))
        canvas = Image.alpha_composite(canvas, shadows)
        tile = (
            _rounded(icon, size, radius)
            if icon
            else _placeholder(size, radius, str(app.get("title") or "?")[:1].upper())
        )
        canvas.alpha_composite(tile, (icon_x, icon_y))

        draw = ImageDraw.Draw(canvas)
        _header(
            canvas,
            str(app.get("header") or "Приложение"),
            str(app.get("subheader") or ""),
        )

        x = icon_x + size + 68
        width = W - x - 72
        title_font, title_lines = _title_layout(
            draw, app.get("title") or "", width, 48, 36
        )
        artist_font = _font("regular", 24)
        line_font = _font("regular", 26)
        lines = [line for line in list(lines)[:4] if line]

        # Считаем высоту блока заранее и ставим его по центру относительно
        # иконки: короткий список не висит вверху, длинный не наезжает на низ.
        title_height = len(title_lines) * _lh(title_font) + (len(title_lines) - 1) * 3
        block = title_height
        if app.get("artist"):
            block += _lh(artist_font) + 2
        block += 34 + len(lines) * (_lh(line_font) + 8)
        y = icon_y + (size - block) // 2

        for line in title_lines:
            draw.text((x, y), line, font=title_font, fill=TEXT)
            y += _lh(title_font) + 3
        y -= 3
        if app.get("artist"):
            draw.text(
                (x, y),
                _fit(draw, str(app["artist"]), artist_font, width),
                font=artist_font,
                fill=MUTED,
            )
            y += _lh(artist_font) + 2

        y += 14
        draw.rounded_rectangle((x, y, x + 110, y + 5), radius=3, fill=ACCENT_B)
        y += 20
        for line in lines:
            draw.text(
                (x, y), _fit(draw, line, line_font, width), font=line_font, fill=TEXT
            )
            y += _lh(line_font) + 8

        return self._encode(canvas, 90)
