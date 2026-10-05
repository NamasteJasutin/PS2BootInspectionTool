#!/usr/bin/env python3
"""Split and decompress the nested asset archives of a PS2 BIOS.

Expects the rom0 modules in extracted/rom0 (see romdir.py). For each of TEXIMAGE,
ICOIMAGE, SNDIMAGE and FNTIMAGE writes the raw entries to extracted/<ARCHIVE>/ and the
decompressed ones to extracted/<ARCHIVE>_unpacked/.
"""
from pathlib import Path

from osd_unpack import unpack
from romdir import parse

ROOT = Path(__file__).resolve().parent.parent / "extracted"
SKIP = {"RESET", "ROMDIR", "EXTINFO", "-"}


def main():
    for arc in ("TEXIMAGE", "ICOIMAGE", "SNDIMAGE", "FNTIMAGE"):
        data = (ROOT / "rom0" / arc).read_bytes()
        raw_dir, out_dir = ROOT / arc, ROOT / f"{arc}_unpacked"
        raw_dir.mkdir(exist_ok=True)
        out_dir.mkdir(exist_ok=True)
        for name, off, size, _ in parse(data):
            if name in SKIP:
                continue
            blob = data[off:off + size]
            (raw_dir / name).write_bytes(blob)
            out, used = unpack(blob)
            (out_dir / name).write_bytes(out)
            print(f"{arc}/{name:9s} {size:#8x} -> {len(out):#8x}")


if __name__ == "__main__":
    main()
