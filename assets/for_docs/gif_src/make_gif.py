#!/usr/bin/env python3
"""Build an animated GIF slideshow (with crossfade transitions) from every image in this folder.

Usage:
    python3 make_gif.py                      # writes ../showcase.gif
    python3 make_gif.py -o out.gif --width 800 --hold 3 --fade 0.8

Requires Pillow (pip install pillow).
"""

import argparse
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
EXTENSIONS = {".png", ".jpg", ".jpeg", ".webp", ".bmp"}


def load_images(folder: Path, width: int) -> list[Image.Image]:
    files = sorted(
        (p for p in folder.iterdir() if p.suffix.lower() in EXTENSIONS),
        key=lambda p: (p.name.startswith("physical_device"), p.name),
    )
    if not files:
        raise SystemExit(f"No images found in {folder}")
    images = []
    for path in files:
        im = Image.open(path).convert("RGB")
        height = round(im.height * width / im.width)
        images.append(im.resize((width, height), Image.LANCZOS))
        print(f"  + {path.name}")
    # Images of different aspect ratios get letterboxed onto the first one's canvas.
    size = images[0].size
    for i, im in enumerate(images):
        if im.size != size:
            im.thumbnail(size, Image.LANCZOS)
            canvas = Image.new("RGB", size, (0, 0, 0))
            canvas.paste(im, ((size[0] - im.width) // 2, (size[1] - im.height) // 2))
            images[i] = canvas
    return images


def ease_in_out(t: float) -> float:
    return t * t * (3 - 2 * t)


def build_frames(images, hold_ms, fade_ms, fade_steps):
    frames, durations = [], []
    step_ms = max(20, fade_ms // fade_steps)
    for i, current in enumerate(images):
        nxt = images[(i + 1) % len(images)]
        frames.append(current)
        durations.append(hold_ms)
        for s in range(1, fade_steps):
            frames.append(Image.blend(current, nxt, ease_in_out(s / fade_steps)))
            durations.append(step_ms)
    return frames, durations


def shared_palette(images) -> Image.Image:
    # One palette built from all sources keeps colours stable between frames (no flicker).
    w, h = images[0].size
    strip = Image.new("RGB", (w, h * len(images)))
    for i, im in enumerate(images):
        strip.paste(im, (0, h * i))
    return strip.quantize(colors=256, method=Image.Quantize.MEDIANCUT)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("-i", "--input", type=Path, default=HERE, help="folder with source images")
    parser.add_argument("-o", "--output", type=Path, default=HERE.parent / "showcase.gif", help="output gif path")
    parser.add_argument("--width", type=int, default=800, help="output width in px (height keeps aspect)")
    parser.add_argument("--hold", type=float, default=2.5, help="seconds each image is shown")
    parser.add_argument("--fade", type=float, default=0.6, help="seconds of crossfade between images")
    parser.add_argument("--steps", type=int, default=8, help="number of frames in each crossfade")
    args = parser.parse_args()

    print(f"Loading images from {args.input}")
    images = load_images(args.input, args.width)
    frames, durations = build_frames(images, int(args.hold * 1000), int(args.fade * 1000), max(2, args.steps))

    palette = shared_palette(images)
    quantized = [f.quantize(palette=palette, dither=Image.Dither.NONE) for f in frames]

    quantized[0].save(
        args.output,
        save_all=True,
        append_images=quantized[1:],
        duration=durations,
        loop=0,
        optimize=True,
        disposal=1,
    )
    size_kb = args.output.stat().st_size / 1024
    print(f"Wrote {args.output} ({len(frames)} frames, {size_kb:.0f} KiB)")


if __name__ == "__main__":
    main()
