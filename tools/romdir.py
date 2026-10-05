#!/usr/bin/env python3
"""Parse a PS2 BIOS ROMDIR table and extract its modules.

usage: romdir.py <rom image> [outdir]
"""
import struct
import sys
from pathlib import Path


def parse(rom: bytes):
    """Return [(name, offset, size, extinfo_size)] for every ROMDIR entry."""
    base = rom.find(b"RESET\0\0\0\0\0")
    if base < 0:
        raise ValueError("no ROMDIR found")
    entries, off, p = [], 0, base
    while rom[p]:
        name = rom[p:p + 10].rstrip(b"\0").decode("ascii")
        ext, size = struct.unpack_from("<HI", rom, p + 10)
        entries.append((name, off, size, ext))
        off += (size + 15) & ~15
        p += 16
    return entries


def main():
    rom = Path(sys.argv[1]).read_bytes()
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else None
    for name, off, size, ext in parse(rom):
        print(f"{name:12s} off={off:#08x} size={size:#08x}")
        if out and name != "-":
            out.mkdir(parents=True, exist_ok=True)
            (out / name).write_bytes(rom[off:off + size])


if __name__ == "__main__":
    main()
