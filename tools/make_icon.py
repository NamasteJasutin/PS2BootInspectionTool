#!/usr/bin/env python3
"""Renders the app icon: a field of tower tops seen from the boot camera, with the
screen's blue glow and lettering-free lavender highlight. Output: docs/icon.png (1024 px)
and app/Icon/AppIcon.icns (via iconutil).

usage: make_icon.py
"""
import math
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SIZE = 1024
S = 4                       # supersampling
W = SIZE * S


def rounded_mask(size, radius):
    m = Image.new("L", (size, size), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return m


def render():
    img = Image.new("RGB", (W, W), (4, 6, 16))
    d = ImageDraw.Draw(img)

    # Background: deep blue vignette with a brighter centre, like the fog behind the towers.
    glow = Image.new("RGB", (W, W), (0, 0, 0))
    gd = ImageDraw.Draw(glow)
    cx, cy = W * 0.5, W * 0.56
    for i in range(40, 0, -1):
        r = W * 0.055 * i
        k = (40 - i) / 40
        gd.ellipse((cx - r, cy - r * 0.8, cx + r, cy + r * 0.8), fill=(int(6 + 20 * k), int(10 + 36 * k), int(30 + 110 * k)))
    glow = glow.filter(ImageFilter.GaussianBlur(W * 0.06))
    img = Image.blend(img, glow, 0.9)
    d = ImageDraw.Draw(img)

    # Towers: squares on a grid in perspective around a vanishing point, each extruded
    # towards it (the near cap faces the viewer, the sides fall away).
    vp = (W * 0.5, W * 0.56)
    cols, rows = 5, 4
    cells = []
    for r in range(rows):
        for c in range(cols):
            gx = (c - (cols - 1) / 2) * 1.0
            gy = (r - (rows - 1) / 2) * 1.0 + 0.15
            # pseudo-random height from the grid position, like the console's jitter
            h = 0.55 + 0.45 * ((math.sin(c * 12.9898 + r * 78.233) * 43758.5453) % 1.0)
            if abs(gx) < 0.6 and abs(gy) < 0.6:
                h *= 0.25      # keep the centre open for the glow
            cells.append((gx, gy, h))
    cells.sort(key=lambda t: -(t[0] ** 2 + t[1] ** 2))     # far first

    cell = W * 0.155
    for gx, gy, h in cells:
        depth = 1.0 / (1.0 + 0.0)      # caps all at the same depth plane
        near = 0.40 + 0.42 * h         # taller tower = cap nearer = larger
        far = 0.22
        def proj(x, y, s):
            return (vp[0] + (x * cell * 1.25) * s * 2.2, vp[1] + (y * cell * 1.25) * s * 2.2)
        half = 0.34
        capq = [proj(gx + dx, gy + dy, near) for dx, dy in ((-half, -half), (half, -half), (half, half), (-half, half))]
        farq = [proj(gx + dx, gy + dy, far) for dx, dy in ((-half, -half), (half, -half), (half, half), (-half, half))]
        # side faces (darker, bluish), only the two facing the viewer
        base = (165, 178, 215)
        bright = 0.35 + 0.65 * h
        for i in range(4):
            a, b = capq[i], capq[(i + 1) % 4]
            fa, fb = farq[i], farq[(i + 1) % 4]
            mid = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
            # visible if the edge faces away from the vanishing point
            nx, ny = (b[1] - a[1]), -(b[0] - a[0])
            if (mid[0] - vp[0]) * nx + (mid[1] - vp[1]) * ny < 0:
                continue
            shade = 0.32 if abs(nx) > abs(ny) else 0.46
            col = tuple(int(v * shade * bright) for v in base)
            d.polygon([a, b, fb, fa], fill=col)
        capcol = tuple(min(255, int(v * (0.75 + 0.55 * bright))) for v in base)
        d.polygon(capq, fill=capcol)
        # a hint of the panel texture: one inset line on the cap
        inset = [((p[0] - vp[0]) * 0.86 + vp[0] * 0 + p[0] * 0.14 * 0, p[1]) for p in capq]
        cxp = sum(p[0] for p in capq) / 4
        cyp = sum(p[1] for p in capq) / 4
        inner = [(cxp + (p[0] - cxp) * 0.72, cyp + (p[1] - cyp) * 0.72) for p in capq]
        d.polygon(inner, outline=tuple(int(v * 0.85) for v in capcol), width=max(1, W // 400))

    # Blue haze over everything, stronger near the centre.
    haze = Image.new("RGB", (W, W), (0, 0, 0))
    hd = ImageDraw.Draw(haze)
    for i in range(30, 0, -1):
        r = W * 0.028 * i
        k = ((30 - i) / 30) ** 1.6
        hd.ellipse((vp[0] - r, vp[1] - r * 0.7, vp[0] + r, vp[1] + r * 0.7), fill=(int(60 * k), int(110 * k), int(235 * k)))
    haze = haze.filter(ImageFilter.GaussianBlur(W * 0.05))
    img = Image.blend(img, Image.eval(haze, lambda v: v), 0.0)
    img = Image.fromarray(__import__("numpy").clip(__import__("numpy").asarray(img, dtype=int) + __import__("numpy").asarray(haze, dtype=int) // 2, 0, 255).astype("uint8"))

    # A lavender orb trail arcing through the field, like the light orbs.
    d = ImageDraw.Draw(img, "RGBA")
    pts = [(vp[0] + math.cos(t) * W * 0.26, vp[1] - 0.05 * W + math.sin(t * 1.7) * W * 0.09) for t in [i / 40 * 3.4 - 1.2 for i in range(41)]]
    for i in range(len(pts) - 1):
        a = int(255 * (i / len(pts)))
        d.line([pts[i], pts[i + 1]], fill=(215, 200, 255, a), width=max(2, W // 110))
    ox, oy = pts[-1]
    for rad, al in ((W * 0.09, 50), (W * 0.05, 110), (W * 0.022, 255)):
        d.ellipse((ox - rad, oy - rad, ox + rad, oy + rad), fill=(220, 210, 255, al))

    img = img.resize((SIZE, SIZE), Image.LANCZOS)
    # macOS icon shape: rounded square with a margin.
    margin = int(SIZE * 0.09)
    inner = SIZE - 2 * margin
    body = img.resize((inner, inner), Image.LANCZOS)
    out = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    mask = rounded_mask(inner, int(inner * 0.225))
    out.paste(body, (margin, margin), mask)
    return out


def main():
    icon = render()
    docs = ROOT / "docs" / "icon.png"
    icon.save(docs)
    iconset = ROOT / "app" / "Icon" / "AppIcon.iconset"
    iconset.mkdir(parents=True, exist_ok=True)
    for px in (16, 32, 128, 256, 512):
        icon.resize((px, px), Image.LANCZOS).save(iconset / f"icon_{px}x{px}.png")
        icon.resize((px * 2, px * 2), Image.LANCZOS).save(iconset / f"icon_{px}x{px}@2x.png")
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(ROOT / "app" / "Icon" / "AppIcon.icns")], check=True)
    print("wrote", docs, "and app/Icon/AppIcon.icns")


if __name__ == "__main__":
    main()
