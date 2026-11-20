from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageOps


SOURCE = Path.home() / "Downloads" / "ыв"
OUT = Path(__file__).resolve().parents[1] / "telegram-catalog-guide"
SIZE = 1080
BG = "#0e1422"
PANEL = "#161e2e"
EDGE = "#2b3548"
TEXT = "#f4f6ff"
MUTED = "#a1aec5"
ACCENT = "#7782ff"
CYAN = "#66d5e4"
FONT_DIR = Path("C:/Windows/Fonts")

REGULAR = ImageFont.truetype(str(FONT_DIR / "segoeui.ttf"), 25)
MEDIUM = ImageFont.truetype(str(FONT_DIR / "seguisb.ttf"), 25)
SMALL = ImageFont.truetype(str(FONT_DIR / "seguisb.ttf"), 18)
TITLE = ImageFont.truetype(str(FONT_DIR / "seguisb.ttf"), 52)

CARDS = [
    {
        "title": "Каталог временно\nнедоступен",
        "body": "Не удалось загрузить каталог. Остальные функции Signum продолжают работать.",
        "shot": 1,
        "mark": (755, 452, 1050, 522),
        "caption": "Временный сбой каталога",
    },
    {
        "title": "Откройте раздел\n«Поиск»",
        "body": "Найдите приложение по названию, ссылке из App Store или номеру Apple ID.",
        "shot": 2,
        "mark": (380, 160, 1415, 220),
        "caption": "Поиск доступен в боковом меню",
    },
    {
        "title": "Введите название\nили Apple ID",
        "body": "Выберите нужное приложение из результатов поиска.",
        "shot": 3,
        "marks": [(380, 160, 1415, 225), (380, 322, 1415, 405)],
        "caption": "Поиск по названию или ID",
    },
    {
        "title": "Выберите приложение\nи версию",
        "body": "Откройте карточку результата, выберите доступную версию и продолжайте установку или экспорт.",
        "shot": 4,
        "mark": (520, 45, 1050, 795),
        "caption": "Карточка приложения и список версий",
    },
    {
        "title": "Установки и экспорт\nработают",
        "body": "Очередь доступна. Можно устанавливать найденные приложения или сохранять IPA на компьютер.",
        "shot": 5,
        "mark": (365, 260, 1430, 405),
        "caption": "Готовые задания остаются в очереди",
    },
    {
        "title": "Не помните название?\nОткройте «Аккаунт»",
        "body": "Войдите в Apple ID и загрузите историю покупок.",
        "shot": 6,
        "mark": (545, 395, 1255, 500),
        "caption": "История покупок в аккаунте Apple ID",
    },
    {
        "title": "Найдите приложение\nв покупках",
        "body": "Выберите приложение в истории покупок, затем установите его на iPhone или скачайте IPA.",
        "shot": 7,
        "mark": (470, 220, 695, 310),
        "caption": "Установка или скачивание IPA",
    },
]


def font_for(text: str) -> ImageFont.FreeTypeFont:
    return TITLE if "\n" in text else TITLE


def draw_lines(draw: ImageDraw.ImageDraw, text: str, xy: tuple[int, int], font: ImageFont.FreeTypeFont, fill: str, spacing: int = 7) -> int:
    draw.multiline_text(xy, text, font=font, fill=fill, spacing=spacing)
    return draw.multiline_textbbox(xy, text, font=font, spacing=spacing)[3]


def crop_for_card(source: Image.Image, index: int) -> tuple[Image.Image, tuple[int, int, int, int] | None]:
    if index == 4:
        # Keep the modal and hide most of the unrelated dimmed page.
        box = (425, 25, 1145, 820)
        return source.crop(box), (95, 28, 615, 735)
    if index == 6:
        # The account screenshot is a tall page; focus on its purchase-history row.
        box = (515, 30, 1270, 800)
        return source.crop(box), (32, 375, 725, 470)
    if index == 7:
        return source.copy(), (470, 220, 695, 310)
    return source.copy(), None


def make_card(index: int, info: dict) -> Image.Image:
    canvas = Image.new("RGB", (SIZE, SIZE), BG)
    draw = ImageDraw.Draw(canvas)
    draw.rounded_rectangle((40, 36, 314, 76), radius=20, fill=PANEL, outline=EDGE, width=1)
    draw.text((58, 45), "SIGNUM  /  ВРЕМЕННОЕ РЕШЕНИЕ", font=SMALL, fill=CYAN)
    draw.text((982, 45), f"{index:02d} / 07", font=SMALL, fill=MUTED, anchor="ra")

    title_bottom = draw_lines(draw, info["title"], (46, 102), TITLE, TEXT, 0)
    body_y = title_bottom + 17
    draw.multiline_text((48, body_y), info["body"], font=REGULAR, fill=MUTED, spacing=5)

    source = Image.open(SOURCE / f"{info['shot']}.png").convert("RGB")
    cropped, mark = crop_for_card(source, info["shot"])
    source.close()
    marks = info.get("marks", [info.get("mark")])

    # Fit each screenshot into a fixed frame without stretching the source.
    frame = (40, 350, 1040, 902)
    fw, fh = frame[2] - frame[0], frame[3] - frame[1]
    preview = ImageOps.contain(cropped, (fw, fh), Image.Resampling.LANCZOS)
    px = frame[0] + (fw - preview.width) // 2
    py = frame[1] + (fh - preview.height) // 2
    draw.rounded_rectangle(frame, radius=22, fill=PANEL, outline=EDGE, width=2)
    mask = Image.new("L", preview.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, preview.width - 1, preview.height - 1), radius=14, fill=255)
    canvas.paste(preview, (px, py), mask)

    if info["shot"] in (4, 6):
        actual_mark = mark
        if actual_mark:
            scale_x, scale_y = preview.width / cropped.width, preview.height / cropped.height
            box = (
                px + int(actual_mark[0] * scale_x),
                py + int(actual_mark[1] * scale_y),
                px + int(actual_mark[2] * scale_x),
                py + int(actual_mark[3] * scale_y),
            )
            draw.rounded_rectangle(box, radius=14, outline=ACCENT, width=5)
    else:
        source_marks = info.get("marks", [info.get("mark")])
        for source_mark in source_marks:
            if not source_mark:
                continue
            sx, sy = cropped.width / preview.width, cropped.height / preview.height
            box = (
                px + int(source_mark[0] / sx), py + int(source_mark[1] / sy),
                px + int(source_mark[2] / sx), py + int(source_mark[3] / sy),
            )
            draw.rounded_rectangle(box, radius=12, outline=ACCENT, width=5)

    draw.text((48, 934), info["caption"], font=MEDIUM, fill=TEXT)
    draw.rounded_rectangle((48, 984, 1032, 989), radius=3, fill=EDGE)
    draw.rounded_rectangle((48, 984, 48 + int(984 * index / 7), 989), radius=3, fill=ACCENT)
    return canvas


def main() -> None:
    if not all((SOURCE / f"{i}.png").is_file() for i in range(1, 8)):
        raise FileNotFoundError(f"Ожидались файлы 1.png–7.png в {SOURCE}")
    OUT.mkdir(parents=True, exist_ok=True)
    for index, info in enumerate(CARDS, 1):
        card = make_card(index, info)
        card.save(OUT / f"{index:02d}-signum-catalog.png", optimize=True)
    print(f"Создано 7 карточек 1080x1080: {OUT}")


if __name__ == "__main__":
    main()
