#!/usr/bin/env python3
"""Dump the EXTINFO records (date / version / comment) of every ROMDIR entry.

usage: extinfo.py <rom image>

EXTINFO is a byte stream; each ROMDIR entry owns `ext` bytes of it, in ROMDIR order.
Record = u16 value, u8 size, u8 type: type 1 = date (BCD yyyymmdd in the next 4 bytes),
type 2 = version (value is the u16 version), type 3 = comment (size bytes of text),
type 0x7F = null (value only).
"""
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from romdir import parse  # noqa: E402


def records(blob: bytes):
    p = 0
    while p + 4 <= len(blob):
        value, size, typ = struct.unpack_from("<HBB", blob, p)
        p += 4
        data = blob[p:p + size]
        p += size
        if typ == 1:
            d = data[::-1].hex()
            yield f"date={d[:4]}-{d[4:6]}-{d[6:8]}"
        elif typ == 2:
            yield f"ver={value >> 8}.{value & 0xFF:02d}"
        elif typ == 3:
            yield "comment=" + data.rstrip(b"\0").decode("ascii", "replace")
        elif typ == 0x7F:
            yield f"null={value:#x}"
        else:
            yield f"type{typ:#x}={value:#x}:{data.hex()}"


def main():
    rom = Path(sys.argv[1]).read_bytes()
    entries = parse(rom)
    ext_off = next(off for name, off, size, ext in entries if name == "EXTINFO")
    p = ext_off
    for name, off, size, ext in entries:
        recs = list(records(rom[p:p + ext])) if ext else []
        p += ext
        print(f"{name:10s} {size:#8x}  " + "  ".join(recs))


if __name__ == "__main__":
    main()
