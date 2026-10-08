#!/usr/bin/env python3
"""Disassemble a range of a raw little-endian MIPS (R3000) image with capstone.

    r3000dis.py <image> <base> <start> <end-or-+len>
"""
import sys
import capstone

img, base, start = sys.argv[1], int(sys.argv[2], 16), int(sys.argv[3], 16)
end = int(sys.argv[4][1:], 16) + start if sys.argv[4].startswith('+') else int(sys.argv[4], 16)
d = open(img, 'rb').read()
md = capstone.Cs(capstone.CS_ARCH_MIPS, capstone.CS_MODE_MIPS32 | capstone.CS_MODE_LITTLE_ENDIAN)
for i in md.disasm(d[start - base:end - base], start):
    print(f"{i.address:08x}  {i.mnemonic:8s} {i.op_str}")
