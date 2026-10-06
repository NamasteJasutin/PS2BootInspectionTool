#!/usr/bin/env python3
"""Render the OSDSYS bitmap fonts (FNTIMAGE archive) to PNG.

Layouts come from the font uploader at 0x20E000 (see notes/menu_survey.md §4):

    FNTASCII  256 x 480  4-bit, 8 glyphs per row, cell 32 x 40, glyph table 0x27E730
    FNTEX000  512 x 760  4-bit, 16 per row, cell 32 x 40, glyph table 0x27ED30 (0x130 glyphs)
    FNTEX001  512 x 760  4-bit, idem, table 0x27F6B0
    FNTEXOSD  512 x  80  4-bit, 16 per row, table 0x280030 (32 glyphs, the "\\x07oNNN" symbols)

Every glyph table entry is {int32 x_offset_in_cell, int32 width}. Pixels index the 16-entry
RGBA CLUT at 0x2801B0 (alpha ramp: 0 transparent, 1-4 black, 5-15 grey -> white).

Output: extracted/png/font_<asset>.png (whole sheet, CLUT applied, on a grey background),
and extracted/png/font_ascii_sample.png with "Browser" / "System Configuration" composed
from the glyph table, as a check of the metrics.
"""
import struct
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
BASE = 0x200000
img = (ROOT / "extracted" / "osdsys_200000.bin").read_bytes()
FNT = ROOT / "extracted" / "FNTIMAGE_unpacked"
OUT = ROOT / "extracted" / "png"

SHEETS = {  # asset, width, height, glyphs per row, glyph table address, glyph count
    "FNTASCII": (256, 480, 8, 0x27E730, 0x61),
    "FNTEX000": (512, 760, 16, 0x27ED30, 0x130),
    "FNTEX001": (512, 760, 16, 0x27F6B0, 0x130),
    "FNTEXOSD": (512, 80, 16, 0x280030, 0x20),
}
CELL_W, CELL_H = 32, 40


def clut():
    return np.frombuffer(img, "u1", 64, 0x2801B0 - BASE).reshape(16, 4)


def unpack4(raw, w, h):
    b = np.frombuffer(raw, "u1", w * h // 2)
    idx = np.empty(w * h, "u1")
    idx[0::2], idx[1::2] = b & 15, b >> 4  # low nibble = left pixel (GS PSMT4)
    return idx.reshape(h, w)


def to_rgba(idx):
    pal = clut().astype(int)
    rgba = pal[idx]
    rgba[..., 3] = np.minimum(rgba[..., 3] * 2, 255)  # GS alpha 0x80 = opaque
    return rgba.astype("u1")


def composite(rgba, bg=(96, 96, 160)):
    a = rgba[..., 3:4].astype(int)
    out = (rgba[..., :3].astype(int) * a + np.array(bg) * (255 - a)) // 255
    return Image.fromarray(out.astype("u1"), "RGB")


def glyph_table(addr, count):
    return [struct.unpack_from("<ii", img, addr + i * 8 - BASE) for i in range(count)]


def glyph(idx_sheet, per_row, table, n):
    x0, w = table[n]
    col, row = n % per_row, n // per_row
    x = col * CELL_W + x0
    y = row * CELL_H
    return idx_sheet[y:y + CELL_H, x:x + w]


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    sheets = {}
    for name, (w, h, per_row, taddr, count) in SHEETS.items():
        idx = unpack4((FNT / name).read_bytes(), w, h)
        sheets[name] = (idx, per_row, glyph_table(taddr, count))
        composite(to_rgba(idx)).save(OUT / f"font_{name}.png")
        print(f"{name}: {w}x{h}, {count} glyphs, table {taddr:#x}")

    # Compose a sample line from the ASCII sheet using the metrics (advance = width + 1).
    idx, per_row, table = sheets["FNTASCII"]
    lines = ["Browser", "System Configuration", "The quick brown fox 0123456789"]
    canvas = np.zeros((CELL_H * len(lines) + 8, 640), "u1")
    for li, text in enumerate(lines):
        pen = 8
        for ch in text:
            n = ord(ch) - 0x20
            if not 0 <= n < len(table):
                continue
            g = glyph(idx, per_row, table, n)
            canvas[li * CELL_H:li * CELL_H + CELL_H, pen:pen + g.shape[1]] = np.maximum(
                canvas[li * CELL_H:li * CELL_H + CELL_H, pen:pen + g.shape[1]], g)
            pen += g.shape[1] + 1
    composite(to_rgba(canvas)).save(OUT / "font_ascii_sample.png")
    print("ASCII metrics (x_offset, width) for ' '..'/':", table[:16])


if __name__ == "__main__":
    main()
