#!/usr/bin/env python3
"""Minimal R5900 linear disassembler for raw images / ELF segments.

usage: r5900dis.py <file> <file_offset> <vaddr> <nbytes>
       r5900dis.py <elf> @<vaddr> <nbytes>        (resolve vaddr through PT_LOAD)
"""
import struct
import sys
from pathlib import Path

import rabbitizer

rabbitizer.config.regNames_namedRegisters = True
rabbitizer.config.pseudos_enablePseudos = True
rabbitizer.config.toolchainTweaks_treatJAsUnconditionalBranch = False


def elf_segments(data):
    """Return [(vaddr, file_off, filesz, memsz)] of an ELF32 LE image."""
    phoff, = struct.unpack_from("<I", data, 0x1C)
    phentsize, phnum = struct.unpack_from("<HH", data, 0x2A)
    segs = []
    for i in range(phnum):
        t, off, va, _, fsz, msz = struct.unpack_from("<6I", data, phoff + i * phentsize)
        if t == 1:
            segs.append((va, off, fsz, msz))
    return segs


def disasm(data, off, vaddr, n):
    for i in range(0, n, 4):
        w, = struct.unpack_from("<I", data, off + i)
        ins = rabbitizer.Instruction(w, vram=vaddr + i, category=rabbitizer.InstrCategory.R5900)
        yield vaddr + i, w, ins.disassemble()


def main():
    data = Path(sys.argv[1]).read_bytes()
    if sys.argv[2].startswith("@"):
        va = int(sys.argv[2][1:], 0)
        n = int(sys.argv[3], 0)
        for sva, soff, fsz, _ in elf_segments(data):
            if sva <= va < sva + fsz:
                off = soff + va - sva
                break
        else:
            sys.exit("vaddr not in any PT_LOAD")
    else:
        off, va, n = (int(x, 0) for x in sys.argv[2:5])
    for a, w, s in disasm(data, off, va, n):
        print(f"{a:08x}: {w:08x}  {s}")


if __name__ == "__main__":
    main()
