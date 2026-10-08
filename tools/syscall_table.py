#!/usr/bin/env python3
"""Locate and print the EE kernel syscall table of a raw rom0:KERNEL image (base 0x80000000).

usage: syscall_table.py <KERNEL> [KERNEL2]
The table is 128 words of handler addresses (0x8000xxxx / 0xA000xxxx) and is found as the
longest run of such words. With two images, entries whose target *code bytes* differ are
listed (the handlers are compared by the first 16 instructions).
"""
import struct
import sys
from pathlib import Path

BASE = 0x80000000


def table(img: bytes):
    words = struct.unpack_from("<%dI" % (len(img) // 4), img, 0)
    best, run, start = (0, 0), 0, 0
    for i, w in enumerate(words):
        ok = (w & 0xDFFF0000) == 0x80000000 and (w & 0x1FFFFFFF) < len(img)
        if ok:
            if run == 0:
                start = i
            run += 1
            if run > best[0]:
                best = (run, start)
        else:
            run = 0
    n, s = best
    return BASE + s * 4, list(words[s:s + 128])


def main():
    imgs = [Path(p).read_bytes() for p in sys.argv[1:]]
    tabs = [table(i) for i in imgs]
    for p, (addr, t) in zip(sys.argv[1:], tabs):
        print(f"{p}: table at {addr:#x}")
    distinct = [len(set(t)) for _, t in tabs]
    print("distinct handlers:", distinct)
    for n in range(128):
        row = [f"{t[n]:#010x}" for _, t in tabs]
        note = ""
        if len(imgs) == 2:
            a, b = tabs[0][1][n], tabs[1][1][n]
            ca = imgs[0][(a & 0x1FFFFFFF):(a & 0x1FFFFFFF) + 64]
            cb = imgs[1][(b & 0x1FFFFFFF):(b & 0x1FFFFFFF) + 64]
            # compare instruction words ignoring the low 16 bits of lui/addiu/jal targets
            def norm(c):
                ws = struct.unpack_from("<16I", c, 0) if len(c) >= 64 else ()
                return tuple((w >> 26) if (w >> 26) in (3, 15, 9, 13, 35, 43) else w for w in ws)
            note = "" if norm(ca) == norm(cb) else "  <-- code differs"
        print(f"{n:#04x}  " + "  ".join(row) + note)


if __name__ == "__main__":
    main()
