#!/usr/bin/env python3
"""Peek at initialised data of the unpacked OSDSYS image (loaded at 0x200000).

usage: peek.py <f|i|x|h|b|s> <vaddr> [count]     (float / int32 / hex32 / int16 / byte / string)
"""
import struct
import sys
from pathlib import Path

BASE = 0x200000
IMG = Path(__file__).resolve().parent.parent / "extracted" / "osdsys_200000.bin"
FMT = {"f": ("<f", 4), "i": ("<i", 4), "x": ("<I", 4), "h": ("<h", 2), "b": ("<B", 1)}


def main():
    data = IMG.read_bytes()
    kind, addr = sys.argv[1], int(sys.argv[2], 0)
    n = int(sys.argv[3], 0) if len(sys.argv) > 3 else 1
    if kind == "s":
        end = data.index(b"\0", addr - BASE)
        print(data[addr - BASE:end].decode("latin1"))
        return
    fmt, size = FMT[kind]
    for i in range(n):
        v, = struct.unpack_from(fmt, data, addr - BASE + i * size)
        txt = f"{v:#010x}" if kind == "x" else (f"{v:.6g}" if kind == "f" else str(v))
        print(f"{addr + i * size:08x}: {txt}")


if __name__ == "__main__":
    main()
