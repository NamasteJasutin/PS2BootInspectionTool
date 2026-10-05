#!/usr/bin/env python3
"""VU1 micro-program disassembler + VIF1 DMA chain walker for the OSDSYS image.

usage: vudis.py chain <vaddr>              walk a source-chain DMA list, decode VIF codes,
                                           disassemble any MPG payload
       vudis.py mpg <vaddr> <count> [pc0]  disassemble <count> 64-bit micro instructions
"""
import struct
import sys
from pathlib import Path

BASE = 0x200000
IMG = Path(__file__).resolve().parent.parent / "extracted" / "osdsys_200000.bin"

BC = "xyzw"


def dest(w):
    d = (w >> 21) & 0xF
    return "." + "".join(c for i, c in enumerate("xyzw") if d & (8 >> i)) if d != 0xF else ""


def upper(w):
    op = w & 0x3F
    ft, fs, fd = (w >> 16) & 31, (w >> 11) & 31, (w >> 6) & 31
    d = dest(w)
    F = lambda n: f"vf{n:02d}"

    def bc3(name):
        return f"{name}{BC[op & 3]}{d} {F(fd)}, {F(fs)}, {F(ft)}{BC[op & 3]}"

    if op < 0x1C:
        return bc3(["ADD", "SUB", "MADD", "MSUB", "MAX", "MINI", "MUL"][op >> 2])
    simple = {0x1C: ("MULq", "Q"), 0x1D: ("MAXi", "I"), 0x1E: ("MULi", "I"), 0x1F: ("MINIi", "I"),
              0x20: ("ADDq", "Q"), 0x21: ("MADDq", "Q"), 0x22: ("ADDi", "I"), 0x23: ("MADDi", "I"),
              0x24: ("SUBq", "Q"), 0x25: ("MSUBq", "Q"), 0x26: ("SUBi", "I"), 0x27: ("MSUBi", "I")}
    if op in simple:
        n, r = simple[op]
        return f"{n}{d} {F(fd)}, {F(fs)}, {r}"
    three = {0x28: "ADD", 0x29: "MADD", 0x2A: "MUL", 0x2B: "MAX", 0x2C: "SUB", 0x2D: "MSUB",
             0x2E: "OPMSUB", 0x2F: "MINI"}
    if op in three:
        return f"{three[op]}{d} {F(fd)}, {F(fs)}, {F(ft)}"
    if op >= 0x3C:
        idx = fd
        col = op & 3
        if idx < 4:
            n = ["ADDA", "SUBA", "MADDA", "MSUBA"][idx]
            return f"{n}{BC[col]}{d} ACC, {F(fs)}, {F(ft)}{BC[col]}"
        if idx == 4:
            return f"ITOF{[0, 4, 12, 15][col]}{d} {F(ft)}, {F(fs)}"
        if idx == 5:
            return f"FTOI{[0, 4, 12, 15][col]}{d} {F(ft)}, {F(fs)}"
        if idx == 6:
            return f"MULA{BC[col]}{d} ACC, {F(fs)}, {F(ft)}{BC[col]}"
        tbl = {
            (7, 0): "MULAq{d} ACC, {fs}, Q", (7, 1): "ABS{d} {ft}, {fs}",
            (7, 2): "MULAi{d} ACC, {fs}, I", (7, 3): "CLIPw {fs}, {ft}w",
            (8, 0): "ADDAq{d} ACC, {fs}, Q", (8, 1): "MADDAq{d} ACC, {fs}, Q",
            (8, 2): "ADDAi{d} ACC, {fs}, I", (8, 3): "MADDAi{d} ACC, {fs}, I",
            (9, 0): "SUBAq{d} ACC, {fs}, Q", (9, 1): "MSUBAq{d} ACC, {fs}, Q",
            (9, 2): "SUBAi{d} ACC, {fs}, I", (9, 3): "MSUBAi{d} ACC, {fs}, I",
            (10, 0): "ADDA{d} ACC, {fs}, {ft}", (10, 1): "MADDA{d} ACC, {fs}, {ft}",
            (10, 2): "MULA{d} ACC, {fs}, {ft}",
            (11, 0): "SUBA{d} ACC, {fs}, {ft}", (11, 1): "MSUBA{d} ACC, {fs}, {ft}",
            (11, 2): "OPMULA ACC, {fs}, {ft}", (11, 3): "NOP",
        }
        t = tbl.get((idx, col))
        if t:
            return t.format(d=d, fs=F(fs), ft=F(ft))
    return f".upper {w:#010x}"


def lower(w, pc):
    op = w >> 25
    it, is_, id_ = (w >> 16) & 31, (w >> 11) & 31, (w >> 6) & 31
    imm11 = w & 0x7FF
    simm11 = imm11 - 0x800 if imm11 & 0x400 else imm11
    imm15 = ((w >> 10) & 0x7800) | imm11
    d = dest(w)
    F = lambda n: f"vf{n:02d}"
    I = lambda n: f"vi{n:02d}"
    tgt = f"L{(pc + 1 + simm11) & 0x7FF:03x}"
    if op == 0x00: return f"LQ{d} {F(it)}, {simm11}({I(is_)})"
    if op == 0x01: return f"SQ{d} {F(is_)}, {simm11}({I(it)})"
    if op == 0x04: return f"ILW{d} {I(it)}, {simm11}({I(is_)})"
    if op == 0x05: return f"ISW{d} {I(it)}, {simm11}({I(is_)})"
    if op == 0x08: return f"IADDIU {I(it)}, {I(is_)}, {imm15}"
    if op == 0x09: return f"ISUBIU {I(it)}, {I(is_)}, {imm15}"
    imm24 = w & 0xFFFFFF
    if op == 0x10: return f"FCEQ vi01, {imm24:#x}"
    if op == 0x11: return f"FCSET {imm24:#x}"
    if op == 0x12: return f"FCAND vi01, {imm24:#x}"
    if op == 0x13: return f"FCOR vi01, {imm24:#x}"
    imm12 = ((w >> 10) & 0x800) | imm11
    if op == 0x14: return f"FSEQ {I(it)}, {imm12:#x}"
    if op == 0x15: return f"FSSET {imm12:#x}"
    if op == 0x16: return f"FSAND {I(it)}, {imm12:#x}"
    if op == 0x17: return f"FSOR {I(it)}, {imm12:#x}"
    if op == 0x18: return f"FMEQ {I(it)}, {I(is_)}"
    if op == 0x1A: return f"FMAND {I(it)}, {I(is_)}"
    if op == 0x1B: return f"FMOR {I(it)}, {I(is_)}"
    if op == 0x1C: return f"FCGET {I(it)}"
    if op == 0x20: return f"B {tgt}"
    if op == 0x21: return f"BAL {I(it)}, {tgt}"
    if op == 0x24: return f"JR {I(is_)}"
    if op == 0x25: return f"JALR {I(it)}, {I(is_)}"
    if op == 0x28: return f"IBEQ {I(it)}, {I(is_)}, {tgt}"
    if op == 0x29: return f"IBNE {I(it)}, {I(is_)}, {tgt}"
    if op == 0x2C: return f"IBLTZ {I(is_)}, {tgt}"
    if op == 0x2D: return f"IBGTZ {I(is_)}, {tgt}"
    if op == 0x2E: return f"IBLEZ {I(is_)}, {tgt}"
    if op == 0x2F: return f"IBGEZ {I(is_)}, {tgt}"
    if op == 0x40:
        f = w & 0x3F
        if f == 0x30: return f"IADD {I(id_)}, {I(is_)}, {I(it)}"
        if f == 0x31: return f"ISUB {I(id_)}, {I(is_)}, {I(it)}"
        if f == 0x32:
            imm5 = id_ - 32 if id_ & 16 else id_
            return f"IADDI {I(it)}, {I(is_)}, {imm5}"
        if f == 0x34: return f"IAND {I(id_)}, {I(is_)}, {I(it)}"
        if f == 0x35: return f"IOR {I(id_)}, {I(is_)}, {I(it)}"
        if f >= 0x3C:
            col = f & 3
            fsf, ftf = BC[(w >> 21) & 3], BC[(w >> 23) & 3]
            tbl = {
                (0x0C, 0): f"MOVE{d} {F(it)}, {F(is_)}", (0x0C, 1): f"MR32{d} {F(it)}, {F(is_)}",
                (0x0D, 0): f"LQI{d} {F(it)}, ({I(is_)}++)", (0x0D, 1): f"SQI{d} {F(is_)}, ({I(it)}++)",
                (0x0D, 2): f"LQD{d} {F(it)}, (--{I(is_)})", (0x0D, 3): f"SQD{d} {F(is_)}, (--{I(it)})",
                (0x0E, 0): f"DIV Q, {F(is_)}{fsf}, {F(it)}{ftf}", (0x0E, 1): f"SQRT Q, {F(it)}{ftf}",
                (0x0E, 2): f"RSQRT Q, {F(is_)}{fsf}, {F(it)}{ftf}", (0x0E, 3): "WAITQ",
                (0x0F, 0): f"MTIR {I(it)}, {F(is_)}{fsf}", (0x0F, 1): f"MFIR{d} {F(it)}, {I(is_)}",
                (0x0F, 2): f"ILWR{d} {I(it)}, ({I(is_)})", (0x0F, 3): f"ISWR{d} {I(it)}, ({I(is_)})",
                (0x10, 0): f"RNEXT{d} {F(it)}, R", (0x10, 1): f"RGET{d} {F(it)}, R",
                (0x10, 2): f"RINIT R, {F(is_)}{fsf}", (0x10, 3): f"RXOR R, {F(is_)}{fsf}",
                (0x19, 0): f"MFP{d} {F(it)}, P", (0x1A, 0): f"XTOP {I(it)}", (0x1A, 1): f"XITOP {I(it)}",
                (0x1B, 0): f"XGKICK {I(is_)}",
                (0x1C, 0): f"ESADD P, {F(is_)}", (0x1C, 1): f"ERSADD P, {F(is_)}",
                (0x1C, 2): f"ELENG P, {F(is_)}", (0x1C, 3): f"ERLENG P, {F(is_)}",
                (0x1D, 0): f"EATANxy P, {F(is_)}", (0x1D, 1): f"EATANxz P, {F(is_)}",
                (0x1D, 2): f"ESUM P, {F(is_)}",
                (0x1E, 0): f"ESQRT P, {F(is_)}{fsf}", (0x1E, 1): f"ERSQRT P, {F(is_)}{fsf}",
                (0x1E, 2): f"ERCPR P, {F(is_)}{fsf}", (0x1E, 3): "WAITP",
                (0x1F, 0): f"ESIN P, {F(is_)}{fsf}", (0x1F, 1): f"EATAN P, {F(is_)}{fsf}",
                (0x1F, 2): f"EEXP P, {F(is_)}{fsf}",
            }
            t = tbl.get((id_, col))
            if t:
                return "NOP" if t == "MOVE. vf00, vf00" else t
    return f".lower {w:#010x}"


def mpg(data, vaddr, count, pc0=0):
    for i in range(count):
        lo, up = struct.unpack_from("<II", data, vaddr - BASE + i * 8)
        pc = pc0 + i
        flags = "".join(c for c, b in (("I", 31), ("E", 30), ("M", 29), ("D", 28), ("T", 27)) if up >> b & 1)
        if up >> 31:
            low = f"LOI {struct.unpack('<f', struct.pack('<I', lo))[0]:.9g}"
        else:
            low = lower(lo, pc)
        print(f"  {pc:03x}: {upper(up):34s} | {low}" + (f"   [{flags}]" if flags.strip("I") else ""))


VIFCMD = {0x00: "NOP", 0x01: "STCYCL", 0x02: "OFFSET", 0x03: "BASE", 0x04: "ITOP", 0x05: "STMOD",
          0x06: "MSKPATH3", 0x07: "MARK", 0x10: "FLUSHE", 0x11: "FLUSH", 0x13: "FLUSHA",
          0x14: "MSCAL", 0x15: "MSCALF", 0x17: "MSCNT", 0x20: "STMASK", 0x30: "STROW", 0x31: "STCOL",
          0x4A: "MPG", 0x50: "DIRECT", 0x51: "DIRECTHL"}


def vif_stream(data, segs):
    """Decode VIF codes over the concatenation of (vaddr, nbytes) segments."""
    buf = b"".join(data[a - BASE:a - BASE + n] for a, n in segs)
    addrs = [a + i for a, n in segs for i in range(0, n, 4)]
    p = 0
    while p < len(buf):
        code, = struct.unpack_from("<I", buf, p)
        at = addrs[p // 4]
        cmd, num, imm = (code >> 24) & 0x7F, (code >> 16) & 0xFF, code & 0xFFFF
        p += 4
        if code == 0:
            continue
        if cmd & 0x60 == 0x60:
            vn, vl = (cmd >> 2) & 3, cmd & 3
            n = num or 256
            size = (n * 2 + 3) & ~3 if (vn == 3 and vl == 3) else ((32 >> vl) * (vn + 1) * n + 31) // 32 * 4
            flg = ("+TOPS" if imm & 0x8000 else "") + (" unsigned" if imm & 0x4000 else "") + (" mask" if cmd & 0x10 else "")
            src = addrs[p // 4] if p < len(buf) else 0
            print(f"  {at:08x}: UNPACK {['S','V2','V3','V4'][vn]}-{[32,16,8,5][vl]} num={n} addr={imm & 0x3FF:#x}{flg}  ({size} bytes at {src:08x})")
            p += size
            continue
        name = VIFCMD.get(cmd, f"cmd{cmd:#x}")
        print(f"  {at:08x}: {name} num={num:#x} imm={imm:#x}")
        if cmd == 0x4A:
            n = num or 256
            mpg(data, addrs[p // 4], n, imm)
            p += n * 8
        elif cmd == 0x20:
            p += 4
        elif cmd in (0x30, 0x31):
            p += 16
        elif cmd in (0x50, 0x51):
            p += (imm or 65536) * 16


def chain(data, vaddr):
    stack = []
    while True:
        lo, hi = struct.unpack_from("<QQ", data, vaddr - BASE)
        qwc, tid, addr = lo & 0xFFFF, (lo >> 28) & 7, (lo >> 32) & 0x7FFFFFFF
        name = ["refe", "cnt", "next", "ref", "refs", "call", "ret", "end"][tid]
        print(f"{vaddr:08x}: DMAtag {name} qwc={qwc:#x} addr={addr:#x}")
        pay = vaddr + 16 if name in ("cnt", "next", "call", "ret", "end") else addr
        vif_stream(data, [(vaddr + 8, 8), (pay, qwc * 16)])
        if name == "cnt":
            vaddr = pay + qwc * 16
        elif name == "next":
            vaddr = addr
        elif name in ("ref", "refs"):
            vaddr += 16
        elif name == "call":
            stack.append(pay + qwc * 16)
            vaddr = addr
        elif name == "ret" and stack:
            vaddr = stack.pop()
        else:
            break


def main():
    data = IMG.read_bytes()
    if sys.argv[1] == "chain":
        chain(data, int(sys.argv[2], 0))
    else:
        pc0 = int(sys.argv[4], 0) if len(sys.argv) > 4 else 0
        mpg(data, int(sys.argv[2], 0), int(sys.argv[3], 0), pc0)


if __name__ == "__main__":
    main()
