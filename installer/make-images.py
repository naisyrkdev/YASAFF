#!/usr/bin/env python3

from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
INSTALLER = ROOT / "installer"
WIZARD_BACKGROUND = (212, 85, 0)

def square(im: Image.Image) -> Image.Image:
    im = im.crop(im.getbbox())
    side = max(im.size)
    out = Image.new("RGBA", (side, side))
    out.paste(im, ((side - im.width) // 2, (side - im.height) // 2))
    return out

def on_background(icon: Image.Image, size: tuple[int, int], icon_side: int, color) -> Image.Image:
    out = Image.new("RGB", size, color)
    scaled = icon.resize((icon_side, icon_side), Image.LANCZOS)
    out.paste(scaled, ((size[0] - icon_side) // 2, (size[1] - icon_side) // 2), scaled)
    return out

def main() -> None:
    icon = square(Image.open(ASSETS / "icon.png").convert("RGBA"))
    icon256 = icon.resize((256, 256), Image.LANCZOS)
    icon256.save(ASSETS / "icon-256.png", optimize=True)
    icon256.save(ASSETS / "icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    for scale in (1, 2):
        on_background(icon, (164 * scale, 314 * scale), 120 * scale, WIZARD_BACKGROUND).save(
            INSTALLER / f"wizard-large-{164 * scale}.bmp")
        on_background(icon, (55 * scale, 55 * scale), 50 * scale, (255, 255, 255)).save(
            INSTALLER / f"wizard-small-{55 * scale}.bmp")

if __name__ == "__main__":
    main()
