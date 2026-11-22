"""16:9 announcement: old Signum icon -> new Signum icon."""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent
OUT = ROOT / "signum-icon-rebrand-16x9.png"
OLD = ROOT / "icons-backup" / "signum-icon.png"
NEW = ROOT.parent / "assets" / "signum-icon.png"

W, H = 1920, 1080
BG = (15, 20, 32, 255)
CARD = (22, 29, 44, 255)
TITLE = (238, 241, 247, 255)
MUTED = (139, 151, 171, 255)
ACCENT = (107, 140, 255, 255)
ARROW = (168, 180, 198, 255)


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "segoeuib.ttf" if bold else "segoeui.ttf"
    return ImageFont.truetype(fr"C:\Windows\Fonts\{name}", size)


def load_icon(path: Path, size: int) -> Image.Image:
    im = Image.open(path).convert("RGBA")
    return im.resize((size, size), Image.Resampling.LANCZOS)


def drop_shadow(img: Image.Image, radius: int = 28, offset: int = 18) -> Image.Image:
    w, h = img.size
    canvas = Image.new("RGBA", (w + offset * 2 + radius, h + offset * 2 + radius), (0, 0, 0, 0))
    shadow = Image.new("RGBA", img.size, (0, 0, 0, 160))
    shadow.putalpha(img.split()[-1].point(lambda a: min(160, a)))
    canvas.paste(shadow, (offset + 6, offset + 14), shadow)
    canvas = canvas.filter(ImageFilter.GaussianBlur(radius // 2))
    canvas.paste(img, (offset, offset), img)
    return canvas


def centered(draw: ImageDraw.ImageDraw, text: str, y: int, f: ImageFont.FreeTypeFont, fill):
    x0, y0, x1, y1 = draw.textbbox((0, 0), text, font=f)
    draw.text(((W - (x1 - x0)) / 2, y), text, font=f, fill=fill)


def arrow(draw: ImageDraw.ImageDraw, cx: int, cy: int, length: int = 140):
    x1, x2 = cx - length // 2, cx + length // 2
    draw.line((x1, cy, x2 - 8, cy), fill=ARROW, width=8)
    draw.polygon(
        [(x2, cy), (x2 - 28, cy - 18), (x2 - 28, cy + 18)],
        fill=ARROW,
    )


def main() -> None:
    canvas = Image.new("RGBA", (W, H), BG)
    glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    g = ImageDraw.Draw(glow)
    g.ellipse((W // 2 - 520, 120, W // 2 + 520, 980), fill=(40, 70, 160, 55))
    canvas = Image.alpha_composite(canvas, glow.filter(ImageFilter.GaussianBlur(90)))

    draw = ImageDraw.Draw(canvas)
    title_f = font(72, bold=True)
    label_f = font(26, bold=True)

    centered(draw, "Новая иконка Signum", 150, title_f, TITLE)

    icon_size = 440
    old_i = drop_shadow(load_icon(OLD, icon_size))
    new_i = drop_shadow(load_icon(NEW, icon_size))

    gap = 260
    total = old_i.width + new_i.width + gap
    x_old = (W - total) // 2
    x_new = x_old + old_i.width + gap
    y_icon = 280
    canvas.alpha_composite(old_i, (x_old, y_icon))
    canvas.alpha_composite(new_i, (x_new, y_icon))

    draw = ImageDraw.Draw(canvas)
    arrow(
        draw,
        cx=x_old + old_i.width + gap // 2,
        cy=y_icon + old_i.height // 2,
    )

    def label_under(img_x: int, img_w: int, text: str):
        x0, y0, x1, y1 = draw.textbbox((0, 0), text, font=label_f)
        tw = x1 - x0
        draw.text((img_x + (img_w - tw) / 2, y_icon + img_w - 8 + 36), text, font=label_f, fill=MUTED)

    label_under(x_old + 18, old_i.width - 36, "Было")
    label_under(x_new + 18, new_i.width - 36, "Стало")

    canvas.convert("RGB").save(OUT, "PNG", optimize=True)
    print(OUT)


if __name__ == "__main__":
    main()
