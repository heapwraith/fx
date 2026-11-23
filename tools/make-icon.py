"""Draws the app icon and writes crates/fx-ui/resources/fx.ico and logo-128.png.

Design (identity "Pathdash", graphite and cyan): a graphite superellipse
plate; a ring ("you are here") from which a path leaves, getting brighter as
it speeds up, and ends in a forward chevron. Light comes from the top left:
a soft glow on the plate, an inner edge lit on top and dark at the bottom,
and a shadow only under the drawing. Sizes up to 24 px use their own
drawing: a thicker path and chevron, no ring, trail or effects.

Geometry is on a 256-unit grid (the same as the SVG in the identity page).
Each size is drawn at 8x and downsampled.

Run: python3 tools/make-icon.py [preview-dir]  (needs Pillow and numpy).
With a directory, also writes PNG previews and a sheet on dark and light.
"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
SS = 8  # supersampling
CYAN = (0x2F, 0xD9, 0xE8)
CYAN_LIGHT = (0x5F, 0xE6, 0xF2)
CYAN_PALE = (0x7D, 0xEE, 0xF7)


def hexrgb(h):
    return tuple(int(h[i : i + 2], 16) for i in (1, 3, 5))


def cubic(p0, p1, p2, p3, steps=64):
    out = []
    for i in range(steps + 1):
        t = i / steps
        a, b, c, d = (1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t**2, t**3
        out.append(tuple(a * p0[j] + b * p1[j] + c * p2[j] + d * p3[j] for j in (0, 1)))
    return out


def squircle(lo, hi):
    """The superellipse plate between `lo` and `hi` (256 units), as points."""
    m = (lo + hi) / 2
    k1, k2 = lo + (hi - lo) * 30 / 224, hi - (hi - lo) * 30 / 224
    pts = []
    pts += cubic((m, lo), (k2, lo), (hi, k1), (hi, m))
    pts += cubic((hi, m), (hi, k2), (k2, hi), (m, hi))
    pts += cubic((m, hi), (k1, hi), (lo, k2), (lo, m))
    pts += cubic((lo, m), (lo, k1), (k1, lo), (m, lo))
    return pts


class Canvas:
    """A float RGBA canvas (premultiplied-free 'over' compositing)."""

    def __init__(self, n):
        self.n = n
        self.k = n / 256
        self.rgb = np.zeros((n, n, 3))
        self.a = np.zeros((n, n))

    def mask(self, draw_fn):
        m = Image.new("L", (self.n, self.n), 0)
        draw_fn(ImageDraw.Draw(m), self.k)
        return np.asarray(m, dtype=float) / 255

    def over(self, color, alpha):
        """Composites `color` (HxWx3 or rgb tuple, 0-255) with `alpha` (HxW)."""
        c = np.broadcast_to(np.asarray(color, dtype=float), self.rgb.shape)
        out_a = alpha + self.a * (1 - alpha)
        safe = np.where(out_a > 0, out_a, 1)
        self.rgb = (c * alpha[..., None] + self.rgb * (self.a * (1 - alpha))[..., None]) / safe[..., None]
        self.a = out_a

    def image(self, size):
        data = np.dstack([self.rgb, self.a * 255]).clip(0, 255).astype(np.uint8)
        return Image.fromarray(data, "RGBA").resize((size, size), Image.LANCZOS)


def coords(n):
    """x, y in 256 units for each pixel center."""
    g = (np.arange(n) + 0.5) * 256 / n
    return np.meshgrid(g, g)


def ramp(t, stops):
    """Color at `t` (array) along `stops` [(offset, (r, g, b), alpha)]."""
    t = np.clip(t, 0, 1)
    rgb = np.zeros(t.shape + (3,))
    a = np.zeros(t.shape)
    for (o0, c0, a0), (o1, c1, a1) in zip(stops, stops[1:]):
        sel = (t >= o0) & (t <= o1)
        u = ((t - o0) / max(o1 - o0, 1e-9))[sel]
        for j in range(3):
            rgb[..., j][sel] = c0[j] + (c1[j] - c0[j]) * u
        a[sel] = a0 + (a1 - a0) * u
    return rgb, a


def poly(points, k):
    return [(x * k, y * k) for x, y in points]


def stroke(points, width):
    """A round brush dragged along a polyline (round caps and joins, no seams)."""

    def fn(d, k):
        p = poly(points, k)
        rad = width * k / 2
        for (x0, y0), (x1, y1) in zip(p, p[1:]):
            steps = max(1, int(np.hypot(x1 - x0, y1 - y0)))
            for i in range(steps + 1):
                t = i / steps
                cx, cy = x0 + (x1 - x0) * t, y0 + (y1 - y0) * t
                d.ellipse([cx - rad, cy - rad, cx + rad, cy + rad], fill=255)

    return fn


def draw_large(size):
    n = size * SS
    c = Canvas(n)
    x, y = coords(n)
    plate = c.mask(lambda d, k: d.polygon(poly(squircle(16, 240), k), fill=255))

    # Plate: diagonal graphite gradient, then the top-left glow.
    vx, vy = 216 - 40, 240 - 16
    t = ((x - 40) * vx + (y - 16) * vy) / (vx * vx + vy * vy)
    rgb, _ = ramp(t, [(0, hexrgb("#4A505A"), 1), (0.55, hexrgb("#262A31"), 1), (1, hexrgb("#101215"), 1)])
    c.over(rgb, plate)
    r = np.hypot(x - 70, y - 44) / 160
    c.over((255, 255, 255), plate * np.clip(1 - r, 0, 1) * 0.16)

    # Speed lines.
    lines = c.mask(lambda d, k: (stroke([(50, 108), (84, 108)], 9)(d, k), stroke([(66, 84), (110, 84)], 9)(d, k)))
    c.over(CYAN, lines * 0.22)

    # The drawing: path, chevron, ring.
    path = cubic((58, 176), (94, 176), (100, 128), (138, 128)) + [(174, 128)]
    trail = c.mask(stroke(path, 20))
    head = c.mask(stroke([(150, 86), (196, 128), (150, 170)], 26))
    ring_out = c.mask(lambda d, k: d.ellipse([(58 - 18.5) * k, (176 - 18.5) * k, (58 + 18.5) * k, (176 + 18.5) * k], fill=255))
    ring_in = c.mask(lambda d, k: d.ellipse([(58 - 11.5) * k, (176 - 11.5) * k, (58 + 11.5) * k, (176 + 11.5) * k], fill=255))

    # Shadow under the drawing only (dy 7, blur 7), kept inside the plate.
    group = np.maximum.reduce([trail, head, ring_out])
    sh = Image.fromarray((group * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(7 * c.k))
    sh = np.roll(np.asarray(sh, dtype=float) / 255, round(7 * c.k), axis=0)
    c.over(hexrgb("#012C2A"), sh * 0.5 * plate)

    tt = (x - 44) / (176 - 44)
    _, ta = ramp(tt, [(0, CYAN, 0.25), (1, CYAN, 1.0)])
    c.over(CYAN, trail * ta)
    rgb, _ = ramp((y - 86) / 84, [(0, hexrgb("#E9FEFF"), 1), (1, CYAN_LIGHT, 1)])
    c.over(rgb, head)
    c.over(CYAN_LIGHT, (ring_out - ring_in).clip(0, 1) * 0.85)
    c.over(hexrgb("#1A1D22"), ring_in)

    # Gloss on the top of the plate.
    gloss = cubic((44, 62), (66, 32), (100, 24), (128, 24)) + cubic((128, 24), (164, 24), (198, 32), (214, 62))
    gloss += cubic((214, 62), (170, 50), (88, 50), (44, 62))
    c.over((255, 255, 255), c.mask(lambda d, k: d.polygon(poly(gloss, k), fill=255)) * 0.1)

    # Inner edge: lit on top, dark at the bottom.
    inner = c.mask(lambda d, k: d.polygon(poly(squircle(18.5, 237.5), k), fill=255))
    edge = (plate - inner).clip(0, 1)
    rgb, ea = ramp((y - 16) / 224, [(0, (255, 255, 255), 0.35), (0.45, (255, 255, 255), 0.05), (1, (0, 0, 0), 0.3)])
    c.over(rgb, edge * ea)
    return c.image(size)


def draw_small(size):
    n = size * SS
    c = Canvas(n)
    plate = c.mask(lambda d, k: d.polygon(poly(squircle(8, 248), k), fill=255))
    c.over(hexrgb("#22262C"), plate)
    inner = c.mask(lambda d, k: d.polygon(poly(squircle(16, 240), k), fill=255))
    c.over((255, 255, 255), (plate - inner).clip(0, 1) * 0.28)
    path = cubic((50, 182), (90, 182), (98, 128), (142, 128))
    c.over(CYAN, c.mask(stroke(path, 30)) * 0.7)
    c.over(CYAN_PALE, c.mask(stroke([(146, 70), (206, 128), (146, 186)], 38)))
    return c.image(size)


def draw(size):
    return draw_small(size) if size <= 24 else draw_large(size)


def main():
    root = Path(__file__).resolve().parent.parent
    res = root / "crates" / "fx-ui" / "resources"
    res.mkdir(parents=True, exist_ok=True)
    images = [draw(s) for s in SIZES]
    out = res / "fx.ico"
    images[-1].save(out, format="ICO", sizes=[(s, s) for s in SIZES], append_images=images[:-1])
    draw(128).save(res / "logo-128.png", optimize=True)
    print(f"{out.relative_to(root)}: {', '.join(map(str, SIZES))} px; logo-128.png")
    if len(sys.argv) > 1:
        preview = Path(sys.argv[1])
        preview.mkdir(parents=True, exist_ok=True)
        for s, im in zip(SIZES, images):
            im.save(preview / f"icon-{s}.png")
        width = sum(SIZES) + 12 * len(SIZES) + 12
        sheet = Image.new("RGBA", (width, (256 + 24) * 2))
        for row, bg in enumerate([(32, 32, 32, 255), (243, 243, 243, 255)]):
            band = Image.new("RGBA", (width, 256 + 24), bg)
            px = 12
            for s, im in zip(SIZES, images):
                band.paste(im, (px, 12 + (256 - s) // 2), im)
                px += s + 12
            sheet.paste(band, (0, row * (256 + 24)))
        sheet.save(preview / "sheet.png")


if __name__ == "__main__":
    main()
