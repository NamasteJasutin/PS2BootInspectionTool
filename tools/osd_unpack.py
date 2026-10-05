#!/usr/bin/env python3
"""Decompress the LZ stream used by the OSDSYS loader stub (and its assets).

Stream layout (derived from the stub's routine at 0x100B30):
  u32 LE   decompressed size
  repeat:
    u32 BE descriptor: bits 31..2 = flags for the next 30 tokens (MSB first),
                       bits 1..0  = n, selecting the offset/length split
    30 tokens: flag 0 -> 1 literal byte
               flag 1 -> u16 BE h: offset = (h & (0x3FFF >> n)) + 1
                                   length = (h >> (14 - n)) + 3

usage: osd_unpack.py <OSDSYS elf> <out.bin>      (unpack the embedded program)
       osd_unpack.py --raw <file> <offset> <out> (unpack a stream at an offset)
"""
import struct
import sys
from pathlib import Path


def unpack(src: bytes, pos: int = 0):
    """Return (decompressed bytes, number of source bytes consumed)."""
    start = pos
    size, = struct.unpack_from("<I", src, pos)
    pos += 4
    out = bytearray()
    left = 0
    while len(out) < size:
        if left == 0:
            desc, = struct.unpack_from(">I", src, pos)
            pos += 4
            n = desc & 3
            shift, mask = 14 - n, 0x3FFF >> n
            left = 30
        if desc & 0x80000000:
            h, = struct.unpack_from(">H", src, pos)
            pos += 2
            off = (h & mask) + 1
            for _ in range((h >> shift) + 3):
                out.append(out[-off])
        else:
            out.append(src[pos])
            pos += 1
        desc = (desc << 1) & 0xFFFFFFFF
        left -= 1
    return bytes(out[:size]), pos - start


# The stub is linked at 0x100000 with its single PT_LOAD at file offset 0x80,
# and calls unpack(0x100D80, 0x200000).
STUB_STREAM_FILE_OFFSET = 0x100D80 - 0x100000 + 0x80


def main():
    if sys.argv[1] == "--raw":
        data = Path(sys.argv[2]).read_bytes()
        out, used = unpack(data, int(sys.argv[3], 0))
        Path(sys.argv[4]).write_bytes(out)
    else:
        data = Path(sys.argv[1]).read_bytes()
        out, used = unpack(data, STUB_STREAM_FILE_OFFSET)
        Path(sys.argv[2]).write_bytes(out)
    print(f"unpacked {used:#x} -> {len(out):#x} bytes")


if __name__ == "__main__":
    main()
