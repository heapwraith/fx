"""Draws the fx app icon (task T2.1) and writes crates/fx-ui/resources/fx.ico.

Design: a teal folder (the fx accent) seen from the front, with a lighter
front flap and a double chevron for speed. Sizes 16-24 drop the chevron so
the folder stays crisp. Each size is drawn at 8x and downsampled.

Run: python3 tools/make-icon.py  (needs Pillow). Also writes PNG previews to
the directory given as the first argument, if any.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw

SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
SS = 8  # supersampling

BACK_TOP = (11, 122, 117)  # accent (light theme)
BACK_BOTTOM = (8, 92, 88)
FRONT_TOP = (72, 222, 206)  # accent (dark theme), a bit lighter
FRONT_BOTTOM = (22, 168, 156)
MARK = (255, 255, 255)


def gradient(size, top, bottom):
    w, h = size
    g = Image.new("RGBA", (1, h))
    for y in range(h):
        t = y / max(h - 1, 1)
        g.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    return g.resize((w, h))


def shape(canvas, box, radius, top, bottom):
    """Pastes a vertical gradient through a rounded-rectangle mask."""
    mask = Image.new("L", canvas.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(box, radius, fill=255)
    canvas.paste(gradient(canvas.size, top, bottom), (0, 0), mask)


def draw(size):
    n = size * SS
    u = n / 32  # design grid: 32 units
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    small = size <= 24

    # Back of the folder with its tab (top left).
    tab = [2 * u, 4 * u, 14 * u, 10 * u]
    shape(img, tab, 2 * u, BACK_TOP, BACK_TOP)
    back = [2 * u, 6.5 * u, 30 * u, 27 * u]
    shape(img, back, 2.5 * u, BACK_TOP, BACK_BOTTOM)

    # Front flap, slightly lower, lighter.
    front = [2 * u, 11 * u, 30 * u, 28 * u]
    shadow = Image.new("L", img.size, 0)
    ImageDraw.Draw(shadow).rounded_rectangle(
        [front[0], front[1] - 0.6 * u, front[2], front[3]], 2.5 * u, fill=70
    )
    img.paste((0, 0, 0, 255), (0, 0), shadow)
    shape(img, front, 2.5 * u, FRONT_TOP, FRONT_BOTTOM)

    if not small:
        # Double chevron (">>") centered on the flap.
        d = ImageDraw.Draw(img)
        cy = (front[1] + front[3]) / 2
        half, width, stroke = 4.2 * u, 3.6 * u, 2.1 * u
        for x0 in (11.2 * u, 16.6 * u):
            pts = [(x0, cy - half), (x0 + width, cy), (x0, cy + half)]
            d.line(pts, fill=MARK + (255,), width=round(stroke), joint="curve")
            for p in (pts[0], pts[2]):
                r = stroke / 2
                d.ellipse([p[0] - r, p[1] - r, p[0] + r, p[1] + r], fill=MARK + (255,))
    return img.resize((size, size), Image.LANCZOS)


def main():
    root = Path(__file__).resolve().parent.parent
    images = [draw(s) for s in SIZES]
    out = root / "crates" / "fx-ui" / "resources" / "fx.ico"
    out.parent.mkdir(parents=True, exist_ok=True)
    images[-1].save(out, format="ICO", sizes=[(s, s) for s in SIZES], append_images=images[:-1])
    print(f"{out.relative_to(root)}: {', '.join(map(str, SIZES))} px")
    if len(sys.argv) > 1:
        preview = Path(sys.argv[1])
        preview.mkdir(parents=True, exist_ok=True)
        for s, im in zip(SIZES, images):
            im.save(preview / f"fx-{s}.png")
        sheet = Image.new("RGBA", (sum(SIZES) + 10 * len(SIZES), 256 + 20), (32, 32, 32, 255))
        light = Image.new("RGBA", sheet.size, (243, 243, 243, 255))
        x = 5
        for s, im in zip(SIZES, images):
            sheet.paste(im, (x, 10), im)
            light.paste(im, (x, 10), im)
            x += s + 10
        both = Image.new("RGBA", (sheet.width, sheet.height * 2))
        both.paste(sheet, (0, 0))
        both.paste(light, (0, sheet.height))
        both.save(preview / "sheet.png")


if __name__ == "__main__":
    main()
