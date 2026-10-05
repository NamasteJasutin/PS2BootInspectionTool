#!/usr/bin/env python3
"""Convert the opening-scene textures to PNG using the descriptor table inside OSDSYS.

Reads the 25 texture descriptors at 0x287700 (stride 0xF0) and the asset-name table at
0x27B4F8 from extracted/osdsys_200000.bin, and the unpacked assets from
extracted/TEXIMAGE_unpacked/.  Output: extracted/png/<asset>.png (RGBA).
"""
import struct
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
BASE = 0x200000
img = (ROOT / "extracted" / "osdsys_200000.bin").read_bytes()


def asset_names():
    names, a = [], 0x27B4F8
    while True:
        n, = struct.unpack_from("<I", img, a - BASE)
        if n == 0xFFFFFFFF:
            return names
        names.append(img[n - BASE:img.index(b"\0", n - BASE)].decode() if n else None)
        a += 16


def decode(raw, w, h, fmt, clut):
    if fmt == 0:
        return np.frombuffer(raw, "u1", w * h * 4).reshape(h, w, 4).copy()
    if fmt == 2:  # 16-bit A1B5G5R5
        v = np.frombuffer(raw, "<u2", w * h).reshape(h, w)
        chan = [((v >> s) & 31) << 3 for s in (0, 5, 10)] + [np.where(v >> 15, 255, 128)]
        return np.dstack(chan).astype("u1")
    if fmt in (3, 4):  # 8-bit alpha, RGB white (3) or black (4); shown as grey for viewing
        a = np.frombuffer(raw, "u1", w * h).reshape(h, w)
        return np.dstack([a, a, a, np.full_like(a, 255)])
    if fmt == 5:  # (intensity, alpha) pairs
        v = np.frombuffer(raw, "u1", w * h * 2).reshape(h, w, 2)
        return np.dstack([v[..., 0]] * 3 + [v[..., 1]])
    if fmt == 0x14:  # 4-bit indexed, low nibble first
        b = np.frombuffer(raw, "u1", w * h // 2)
        idx = np.empty(w * h, "u1")
        idx[0::2], idx[1::2] = b & 15, b >> 4
        pal = np.frombuffer(img, "u1", 64, clut - BASE).reshape(16, 4)
        return pal[idx].reshape(h, w, 4).copy()
    raise ValueError(f"unknown format {fmt}")


def main():
    names = asset_names()
    count, = struct.unpack_from("<I", img, 0x2C86E4 - BASE)
    out = ROOT / "extracted" / "png"
    out.mkdir(parents=True, exist_ok=True)
    for i in range(count):
        f = struct.unpack_from("<16i", img, 0x287700 + i * 0xF0 - BASE)
        name, clut, w, h, mips, skip, fmt = names[f[1]], f[2], f[6], f[7], f[8], f[9], f[10]
        raw = (ROOT / "extracted" / "TEXIMAGE_unpacked" / name).read_bytes()[skip:]
        rgba = decode(raw, w, h, fmt, clut)
        rgba[..., 3] = np.minimum(rgba[..., 3].astype(int) * 2, 255)  # GS alpha 0x80 = opaque
        Image.fromarray(rgba, "RGBA").save(out / f"{name}.png")
        print(f"{i:2d} {name:9s} {w}x{h} fmt={fmt:#x} mips={mips}")


if __name__ == "__main__":
    main()
