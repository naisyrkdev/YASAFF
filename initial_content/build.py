#!/usr/bin/env python3
import json
import sys
import zipfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
GAMES = HERE / "games"
DIST = HERE / "dist"
FONT = "DejaVuSans-Bold.ttf"
FONT_DIRS = ["/usr/share/fonts/truetype/dejavu", "/usr/share/fonts/TTF", "C:/Windows/Fonts"]

IMAGES = {
    "game_logo.png": ((1200, 400), "logo", False),
    "square_cover.png": ((1024, 1024), "square cover", False),
    "standard_cover.png": ((730, 1024), "standard cover", False),
    "3d_box_spine.png": ((120, 1024), "spine", True),
    "3d_top_image.png": ((1024, 120), "top", False),
    "3d_bottom_image.png": ((1024, 120), "bottom", False),
    "3d_right_image.png": ((120, 1024), "right side", True),
}

COLORS = {
    "omaro-the-thief": (59, 31, 92),
    "cards-are-gone": (31, 92, 58),
    "super-army-3d": (92, 74, 31),
}

STAMP = (2026, 1, 1, 0, 0, 0)


def font(size):
    for d in FONT_DIRS:
        path = Path(d) / FONT
        if path.is_file():
            return ImageFont.truetype(str(path), size)
    return ImageFont.load_default(size)


def fitted(draw, lines, width, height):
    size = height
    while size > 8:
        f = font(size)
        boxes = [draw.textbbox((0, 0), line, font=f) for line in lines]
        w = max(b[2] - b[0] for b in boxes)
        h = sum(b[3] - b[1] for b in boxes) + size * 0.3 * (len(lines) - 1)
        if w <= width and h <= height:
            return f
        size = int(size * 0.9)
    return font(8)


def placeholder(path, name, color, size, label, turned):
    w, h = size
    canvas = (h, w) if turned else (w, h)
    img = Image.new("RGB", canvas, color)
    draw = ImageDraw.Draw(img)
    lines = [f"{name} · {label}"] if canvas[1] < 200 else [name, label]
    f = fitted(draw, lines, canvas[0] * 0.9, canvas[1] * 0.7)
    draw.multiline_text((canvas[0] / 2, canvas[1] / 2), "\n".join(lines), font=f, fill="white", anchor="mm", align="center", spacing=f.size * 0.3)
    if turned:
        img = img.rotate(90, expand=True)
    img.save(path)


def build(game, regenerate):
    manifest = json.loads((game / "yasaffConfig.json").read_text(encoding="utf-8"))
    color = COLORS.get(game.name, (40, 44, 52))
    for file, (size, label, turned) in IMAGES.items():
        path = game / file
        if regenerate or not path.exists():
            placeholder(path, manifest["name"], color, size, label, turned)
    DIST.mkdir(exist_ok=True)
    out = DIST / f"{manifest['id']}.zip"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for path in sorted(p for p in game.rglob("*") if p.is_file()):
            info = zipfile.ZipInfo(path.relative_to(game).as_posix(), STAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, path.read_bytes())
    print(out.relative_to(HERE))


def main():
    regenerate = "--placeholders" in sys.argv[1:]
    for game in sorted(p for p in GAMES.iterdir() if (p / "yasaffConfig.json").is_file()):
        build(game, regenerate)


if __name__ == "__main__":
    main()
