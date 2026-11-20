# -*- coding: utf-8 -*-
"""Отрисовка знаков Signum в PNG.

Рисуем напрямую, а не переводим SVG: во-первых, вырезы в знаках 01 и 03 должны
быть настоящей прозрачностью, а svglib маски не поддерживает; во-вторых, так
получается предсказуемое сглаживание на всех размерах.

Приём простой: всё рисуется в восемь раз крупнее и потом уменьшается
фильтром Ланцоша. Это даёт чистые края даже там, где Pillow умеет только
резкие примитивы.

Запуск:  python scripts/render-marks.py
"""
import os
import sys

from PIL import Image, ImageDraw

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "brand", "marks")
SCALE = 8                      # во столько раз крупнее рисуем
MASTER = 512                   # исходный размер знака
SIZES = [512, 256, 128, 64, 48, 32, 16]

ACCENT = (91, 108, 240)        # #5b6cf0
ACCENT_DEEP = (43, 52, 136)    # вдавленный тон для 04
ACCENT_SOFT = (129, 140, 248)  # #818cf8

GROUND_DARK = (17, 20, 28)
GROUND_LIGHT = (255, 255, 255)


def u(value):
    """Перевод из 64-клеточной сетки макета в пиксели холста."""
    return value * MASTER * SCALE / 64.0


def bezier(p0, p1, p2, p3, steps=220):
    """Точки кубической кривой Безье."""
    pts = []
    for i in range(steps + 1):
        t = i / steps
        m = 1 - t
        x = m * m * m * p0[0] + 3 * m * m * t * p1[0] + 3 * m * t * t * p2[0] + t * t * t * p3[0]
        y = m * m * m * p0[1] + 3 * m * m * t * p1[1] + 3 * m * t * t * p2[1] + t * t * t * p3[1]
        pts.append((x, y))
    return pts


def thick_path(draw, pts, width, fill):
    """Толстая линия с круглыми стыками.

    Ставим круг в каждой точке: при восьмикратном увеличении шов не виден, а
    скруглённые концы получаются сами собой — как stroke-linecap="round".
    """
    r = width / 2.0
    draw.line(pts, fill=fill, width=int(round(width)))
    for x, y in pts:
        draw.ellipse([x - r, y - r, x + r, y + r], fill=fill)


def s_curve(a, b, c, d, e, f, g, h):
    """S из двух кубических дуг, заданных в координатах сетки."""
    return (bezier(a, b, c, d) + bezier(d, e, f, g) + bezier(g, *h))


def fill_canvas(img):
    """Убрать пустые поля: знак должен занимать холст целиком.

    В разметке знаки нарисованы с запасом по краям — так удобнее считать
    геометрию, но в иконке это выглядит как будто картинку отдалили.
    Обрезаем по непрозрачным пикселям и растягиваем до края.

    Пропорции сохраняем: у штандарта высота много больше ширины, и растяжение
    до квадрата его бы расплющило.
    """
    box = img.getbbox()
    if not box:
        return img
    cut = img.crop(box)
    side = img.size[0]
    w, h = cut.size
    k = side / float(max(w, h))
    new = (max(1, int(round(w * k))), max(1, int(round(h * k))))
    cut = cut.resize(new, Image.LANCZOS)
    out = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    out.paste(cut, ((side - new[0]) // 2, (side - new[1]) // 2))
    return out


def canvas():
    side = MASTER * SCALE
    return Image.new("RGBA", (side, side), (0, 0, 0, 0)), side


# ── знаки ───────────────────────────────────────────────────────────────────

def mark_seal():
    """01 Печать: S вырезана из диска, вырез прозрачный."""
    img, side = canvas()
    mask = Image.new("L", (side, side), 0)
    md = ImageDraw.Draw(mask)
    md.ellipse([u(5), u(5), u(59), u(59)], fill=255)

    pts = (bezier((u(42), u(23)), (u(42), u(16)), (u(22), u(16)), (u(22), u(26)))
           + bezier((u(22), u(26)), (u(22), u(33)), (u(42), u(31)), (u(42), u(39)))
           + bezier((u(42), u(39)), (u(42), u(49)), (u(22), u(49)), (u(22), u(42))))
    thick_path(md, pts, u(7), 0)

    solid = Image.new("RGBA", (side, side), ACCENT + (255,))
    solid.putalpha(mask)
    return solid


def mark_standard():
    """02 Штандарт: древко, перекладина, кольцо и диск."""
    img, side = canvas()
    d = ImageDraw.Draw(img)
    # Табличка наверху: главное отличие штандарта от креста.
    d.rounded_rectangle([u(20), u(6), u(44), u(17)], radius=u(3), fill=ACCENT + (255,))
    d.rounded_rectangle([u(29), u(17), u(35), u(58)], radius=u(3), fill=ACCENT + (255,))
    d.rounded_rectangle([u(13), u(23), u(51), u(29)], radius=u(3), fill=ACCENT + (255,))
    d.ellipse([u(25), u(35), u(39), u(49)], fill=ACCENT_SOFT + (255,))
    return img


def mark_notch():
    """03 Зарубка: из блока сняты два паза, остаток читается как S."""
    img, side = canvas()
    mask = Image.new("L", (side, side), 0)
    md = ImageDraw.Draw(mask)
    md.rounded_rectangle([u(8), u(8), u(56), u(56)], radius=u(13), fill=255)
    md.rounded_rectangle([u(33), u(21), u(60), u(30)], radius=u(4.5), fill=0)
    md.rounded_rectangle([u(4), u(34), u(31), u(43)], radius=u(4.5), fill=0)

    solid = Image.new("RGBA", (side, side), ACCENT + (255,))
    solid.putalpha(mask)
    return solid


def mark_impress():
    """04 Оттиск: буква темнее поля, будто вдавлена."""
    img, side = canvas()
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([u(7), u(7), u(57), u(57)], radius=u(12), fill=ACCENT + (255,))
    pts = (bezier((u(41), u(24)), (u(41), u(18)), (u(24), u(18)), (u(24), u(26)))
           + bezier((u(24), u(26)), (u(24), u(32)), (u(41), u(30.5)), (u(41), u(38)))
           + bezier((u(41), u(38)), (u(41), u(46)), (u(24), u(46)), (u(24), u(40))))
    thick_path(d, pts, u(6.5), ACCENT_DEEP + (255,))
    return img


def mark_engrave():
    """05 Гравировка: S из прямых отрезков с прямыми углами."""
    img, side = canvas()
    d = ImageDraw.Draw(img)
    w = u(9)
    pts = [(u(45), u(19)), (u(24), u(19)), (u(24), u(31)),
           (u(40), u(31)), (u(40), u(45)), (u(19), u(45))]
    d.line(pts, fill=ACCENT + (255,), width=int(round(w)))
    # Прямые углы вместо скруглённых: в каждом изломе ставим квадрат.
    for x, y in pts[1:-1]:
        d.rectangle([x - w / 2, y - w / 2, x + w / 2, y + w / 2], fill=ACCENT + (255,))
    return img


MARKS = [
    ("01-pechat", "Печать", mark_seal),
    ("02-shtandart", "Штандарт", mark_standard),
    ("03-zarubka", "Зарубка", mark_notch),
    ("04-ottisk", "Оттиск", mark_impress),
    ("05-gravirovka", "Гравировка", mark_engrave),
]


def contact_sheet(rendered, ground, name):
    """Лист сравнения: все знаки в ряд, по убыванию размера."""
    pad, gap = 48, 36
    row_sizes = [128, 64, 48, 32, 16]
    width = pad * 2 + 160 * len(MARKS) + gap * (len(MARKS) - 1)
    height = pad * 2 + 160 + 40 + 60
    sheet = Image.new("RGB", (width, height), ground)

    x = pad
    for slug, _title, _fn in MARKS:
        big = rendered[slug][128]
        sheet.paste(big, (x + 16, pad), big)
        # Мелкие размеры выстраиваем по нижнему краю — так видно, что теряется.
        sx, sy = x, pad + 128 + 26
        for s in row_sizes[1:]:
            small = rendered[slug][s]
            sheet.paste(small, (sx, sy + (48 - s)), small)
            sx += s + 10
        x += 160 + gap

    out = os.path.join(OUT, name)
    sheet.save(out)
    return out


def main():
    os.makedirs(OUT, exist_ok=True)
    rendered = {}

    for slug, title, fn in MARKS:
        master = fill_canvas(fn())
        rendered[slug] = {}
        for size in SIZES:
            img = master.resize((size, size), Image.LANCZOS)
            rendered[slug][size] = img
            img.save(os.path.join(OUT, "%s-%d.png" % (slug, size)))
        print("%-16s %s — %d размеров" % (slug, title, len(SIZES)))

    dark = contact_sheet(rendered, GROUND_DARK, "sheet-dark.png")
    light = contact_sheet(rendered, GROUND_LIGHT, "sheet-light.png")
    print()
    print("листы сравнения:")
    print("  " + dark)
    print("  " + light)
    print()
    print("всего файлов:", len(os.listdir(OUT)))


if __name__ == "__main__":
    sys.exit(main())
