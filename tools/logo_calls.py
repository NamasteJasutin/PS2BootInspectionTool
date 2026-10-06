#!/usr/bin/env python3
"""Crude call-argument recovery for the unpacked PS2LOGO image (loads at 0x100000).

Walks a function linearly, tracking immediates / gp-relative globals / simple ALU ops
symbolically, and prints every `jal` with its argument registers (a0-a3, t0-t3) and the
8-byte stack slots used for arguments 9+ (EE ABI).  Values that cannot be followed are
shown as expressions or `?`.

usage: logo_calls.py <func_vaddr> [end_vaddr]     (end defaults to the next function in functions.tsv)
"""
import re
import struct
import sys
from pathlib import Path

import rabbitizer

rabbitizer.config.regNames_namedRegisters = True
rabbitizer.config.pseudos_enablePseudos = False

ROOT = Path(__file__).resolve().parent.parent
IMG = ROOT / "extracted" / "ps2logo_100000.bin"
FUNCS = ROOT / "analysis" / "ps2logo" / "functions.tsv"
BASE = 0x100000
GP = 0x1389F0
ARGREGS = ["a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3"]

data = IMG.read_bytes()


def func_ends(start):
    ends = sorted(int(l.split("\t")[0], 16) for l in FUNCS.read_text().splitlines()[1:])
    for e in ends:
        if e > start:
            return e
    return start + 0x1000


def simplify(v):
    try:
        return str(int(eval(v)))  # noqa: S307 - our own arithmetic strings
    except Exception:
        return v


CALLN = [0]

FIELD_NAMES = {
    0x42: "A B C D FIX", 0x43: "A B C D FIX", 0x50: "SBP SBW SPSM DBP DBW DPSM",
    0x08: "WMS WMT MINU MAXU MINV MAXV", 0x09: "WMS WMT MINU MAXU MINV MAXV", 0x46: "CLAMP",
    0x44: " ".join(f"DM{i}{j}" for i in range(4) for j in range(4)), 0x45: "DTHE", 0x4a: "FBA", 0x4b: "FBA",
    0x0a: "F", 0x3d: "FCR FCG FCB", 0x4c: "FBP FBW PSM FBMSK", 0x4d: "FBP FBW PSM FBMSK", 0x54: "DATA",
    0x62: "IDMSK ID", 0x34: "TBP1 TBW1 TBP2 TBW2 TBP3 TBW3", 0x35: "TBP1 TBW1 TBP2 TBW2 TBP3 TBW3",
    0x36: "TBP4 TBW4 TBP5 TBW5 TBP6 TBW6", 0x37: "TBP4 TBW4 TBP5 TBW5 TBP6 TBW6", 0x49: "PABE",
    0x00: "PRIM IIP TME FGE ABE AA1 FST CTXT FIX", 0x1b: "IIP TME FGE ABE AA1 FST CTXT FIX", 0x1a: "AC",
    0x01: "R G B A Q", 0x22: "MSK", 0x40: "SCAX0 SCAX1 SCAY0 SCAY1", 0x41: "SCAX0 SCAX1 SCAY0 SCAY1",
    0x60: "ID IDMSK", 0x02: "S T", 0x47: "ATE ATST AREF AFAIL DATE DATM ZTE ZTST",
    0x48: "ATE ATST AREF AFAIL DATE DATM ZTE ZTST",
    0x06: "TBP0 TBW PSM TW TH TCC TFX CBP CPSM CSM CSA CLD", 0x07: "TBP0 TBW PSM TW TH TCC TFX CBP CPSM CSM CSA CLD",
    0x14: "LCM MXL MMAG MMIN MTBA L K", 0x15: "LCM MXL MMAG MMIN MTBA L K",
    0x16: "PSM CBP CPSM CSM CSA CLD", 0x17: "PSM CBP CPSM CSM CSA CLD", 0x3b: "TA0 TA1 AEM",
    0x1c: "CBW COU COV", 0x53: "XDIR", 0x51: "SSAX SSAY DSAX DSAY DIR", 0x52: "RRW RRH", 0x03: "U V",
    0x18: "OFX OFY", 0x19: "OFX OFY", 0x05: "X Y Z", 0x0c: "X Y Z", 0x04: "X Y Z F", 0x0b: "X Y Z F",
    0x4e: "ZBP PSM ZMSK", 0x4f: "ZBP PSM ZMSK",
}
REG_NAMES = {0x42: "ALPHA_1", 0x43: "ALPHA_2", 0x50: "BITBLTBUF", 0x08: "CLAMP_1", 0x09: "CLAMP_2", 0x46: "COLCLAMP",
    0x44: "DIMX", 0x45: "DTHE", 0x4a: "FBA_1", 0x4b: "FBA_2", 0x61: "FINISH", 0x0a: "FOG", 0x3d: "FOGCOL",
    0x4c: "FRAME_1", 0x4d: "FRAME_2", 0x54: "HWREG", 0x62: "LABEL", 0x34: "MIPTBP1_1", 0x35: "MIPTBP1_2",
    0x36: "MIPTBP2_1", 0x37: "MIPTBP2_2", 0x49: "PABE", 0x00: "PRIM", 0x1b: "PRMODE", 0x1a: "PRMODECONT",
    0x01: "RGBAQ", 0x22: "SCANMSK", 0x40: "SCISSOR_1", 0x41: "SCISSOR_2", 0x60: "SIGNAL", 0x02: "ST",
    0x47: "TEST_1", 0x48: "TEST_2", 0x06: "TEX0_1", 0x07: "TEX0_2", 0x14: "TEX1_1", 0x15: "TEX1_2",
    0x16: "TEX2_1", 0x17: "TEX2_2", 0x3b: "TEXA", 0x1c: "TEXCLUT", 0x3f: "TEXFLUSH", 0x53: "TRXDIR",
    0x51: "TRXPOS", 0x52: "TRXREG", 0x03: "UV", 0x18: "XYOFFSET_1", 0x19: "XYOFFSET_2", 0x05: "XYZ2",
    0x0c: "XYZ3", 0x04: "XYZF2", 0x0b: "XYZF3", 0x4e: "ZBUF_1", 0x4f: "ZBUF_2", 0xfe: "NOP"}

# field-id -> name, from the table at 0x121110 (99 records: reg, n, n*(id, width, shift))
FIELD = {}
_p = 0x121110 - BASE
for _ in range(99):
    reg, n = data[_p], data[_p + 1]
    names = FIELD_NAMES.get(reg, "").split()
    for k in range(n):
        fid = data[_p + 2 + 3 * k]
        FIELD.setdefault(fid, names[k] if k < len(names) else f"f{fid:02x}")
    _p += 2 + 3 * n


def regname(v):
    try:
        v = int(v)
    except ValueError:
        return f"REG({v})"
    ctx = "" if not (v & 0x80) else "(ctx)"
    return REG_NAMES.get(v & 0x7f, f"reg{v & 0x7f:02x}") + ctx


def pretty(tgt, args):
    if tgt == 0x100F38:
        out = []
        i = 1
        while i + 1 < len(args) and args[i] not in ("0", "?"):
            try:
                fname = FIELD.get(int(args[i]), f"f{int(args[i]):x}")
            except ValueError:
                fname = f"f({args[i]})"
            out.append(f"{fname}={args[i + 1]}")
            i += 2
        return f"{regname(args[0])}{{{', '.join(out)}}}"
    if tgt == 0x100710:
        try:
            n = int(args[0])
        except ValueError:
            n = 0
        pairs = [f"{regname(args[1 + 2 * k])}={args[2 + 2 * k]}" for k in range(n) if 2 + 2 * k < len(args)]
        return f"PACKET(n={args[0]}: {', '.join(pairs)})"
    return f"call {tgt:06x}({', '.join(args)})"


def run(start, end):
    regs = {"zero": "0", "gp": str(GP)}
    stack = {}
    hi = lo = "?"
    for a in range(start, end, 4):
        w, = struct.unpack_from("<I", data, a - BASE)
        ins = rabbitizer.Instruction(w, vram=a, category=rabbitizer.InstrCategory.R5900)
        txt = ins.disassemble()
        op, _, rest = txt.partition(" ")
        ops = [o.strip().replace("$", "") for o in rest.split(",")] if rest else []

        def g(r):
            return regs.get(r, "?" + r)

        def imm(s):
            return int(s, 0)

        if op == "jal":
            tgt = int(ops[0].split("_")[-1], 16) if ops[0].startswith("func_") else int(ops[0], 16)
            # delay slot
            nxt, = struct.unpack_from("<I", data, a + 4 - BASE)
            dins = rabbitizer.Instruction(nxt, vram=a + 4, category=rabbitizer.InstrCategory.R5900)
            exec_one(dins.disassemble(), regs, stack)
            args = [simplify(g(r)) for r in ARGREGS]
            sargs = [simplify(stack.get(o, "?")) for o in range(0, 0x100, 8)]
            while sargs and sargs[-1] == "?":
                sargs.pop()
            CALLN[0] += 1
            rid = f"r{CALLN[0]}"
            print(f"{a:06x}: {rid} = {pretty(tgt, args + sargs)}")
            regs["v0"] = rid
            for r in ARGREGS + ["v1", "at", "t4", "t5", "t6", "t7", "t8", "t9"]:
                regs.pop(r, None)
            continue
        if op in ("jr",) and ops[0] == "ra":
            break
        exec_one(txt, regs, stack)
    return regs


def exec_one(txt, regs, stack):
    op, _, rest = txt.partition(" ")
    ops = [o.strip().replace("$", "") for o in rest.split(",")] if rest else []

    def g(r):
        return regs.get(r, "?" + r)

    try:
        if op in ("addiu", "daddiu"):
            regs[ops[0]] = simplify(f"({g(ops[1])})+({int(ops[2], 0)})")
        elif op == "ori":
            regs[ops[0]] = simplify(f"({g(ops[1])})|({int(ops[2], 0)})")
        elif op == "andi":
            regs[ops[0]] = simplify(f"({g(ops[1])})&({int(ops[2], 0)})")
        elif op == "lui":
            regs[ops[0]] = str(int(ops[1], 0) << 16)
        elif op in ("daddu", "addu", "or"):
            if ops[2] == "zero":
                regs[ops[0]] = g(ops[1])
            elif ops[1] == "zero":
                regs[ops[0]] = g(ops[2])
            else:
                regs[ops[0]] = simplify(f"({g(ops[1])})+({g(ops[2])})")
        elif op in ("subu", "dsubu"):
            regs[ops[0]] = simplify(f"({g(ops[1])})-({g(ops[2])})")
        elif op in ("sll", "dsll"):
            regs[ops[0]] = simplify(f"({g(ops[1])})<<({int(ops[2], 0)})")
        elif op in ("sra", "srl", "dsra", "dsrl"):
            regs[ops[0]] = simplify(f"({g(ops[1])})>>({int(ops[2], 0)})")
        elif op in ("lw", "ld", "lh", "lhu", "lb", "lbu"):
            m = re.match(r"(-?0x[0-9A-Fa-f]+|-?\d+)?\((\w+)\)", ops[1])
            off = int(m.group(1), 0) if m.group(1) else 0
            base = m.group(2)
            if base == "sp":
                regs[ops[0]] = stack.get(off, f"?sp{off:#x}")
            else:
                addr = simplify(f"({g(base)})+({off})")
                try:
                    regs[ops[0]] = f"[{int(addr):#x}]"
                except ValueError:
                    regs[ops[0]] = f"[{addr}]"
        elif op in ("sw", "sd"):
            m = re.match(r"(-?0x[0-9A-Fa-f]+|-?\d+)?\((\w+)\)", ops[1])
            off = int(m.group(1), 0) if m.group(1) else 0
            if m.group(2) == "sp":
                stack[off] = g(ops[0])
        elif op == "mflo":
            regs[ops[0]] = regs.get("lo", "?lo")
        elif op == "mfhi":
            regs[ops[0]] = regs.get("hi", "?hi")
        elif op in ("mult", "multu"):
            if len(ops) == 3:
                regs["lo"] = simplify(f"({g(ops[1])})*({g(ops[2])})")
                regs[ops[0]] = regs["lo"]
            else:
                regs["lo"] = simplify(f"({g(ops[0])})*({g(ops[1])})")
        elif op in ("div", "divu"):
            regs["lo"] = simplify(f"({g(ops[1])})//({g(ops[2])})")
        elif op in ("movz", "movn"):
            regs[ops[0]] = f"({g(ops[0])} {op} {g(ops[1])} if {g(ops[2])})"
        elif op in ("mfc1", "cvt.w.s", "trunc.w.s"):
            regs[ops[0]] = "?f"
        elif op.startswith(("b", "j")):
            pass
        elif ops and op not in ("nop", "sync", "syncl", "sync.l", "sync.p"):
            if op.startswith(("s", "c.", "mtc", "ctc", "lq", "sq", "lwc", "swc")):
                return
            regs[ops[0]] = f"?{op}"
    except Exception:
        pass


if __name__ == "__main__":
    s = int(sys.argv[1], 16)
    e = int(sys.argv[2], 16) if len(sys.argv) > 2 else func_ends(s)
    run(s, e)
