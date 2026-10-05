#!/usr/bin/env python3
"""List the import stubs of an IOP IRX module (default rom0:OSDSND).

Each import table in .text is: 0x41E00000, 0, version, 8-char library name, then one
8-byte stub per function (`jr $ra` / `li $zero, <ordinal>`), terminated by two zero words.
Prints: stub vaddr, library, ordinal, conventional function name (for the common ordinals).

usage: snd_irx_imports.py [irx]
"""
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NAMES = {
    "sysmem": {4: "AllocSysMemory", 5: "FreeSysMemory", 6: "QueryMemSize", 7: "QueryMaxFreeMemSize",
               8: "QueryTotalFreeMemSize", 14: "Kprintf"},
    "intrman": {4: "RegisterIntrHandler", 5: "ReleaseIntrHandler", 6: "EnableIntr", 7: "DisableIntr",
                8: "CpuDisableIntr", 9: "CpuEnableIntr", 17: "CpuSuspendIntr", 18: "CpuResumeIntr",
                23: "QueryIntrContext"},
    "loadcore": {5: "FlushIcache", 6: "RegisterLibraryEntries", 7: "ReleaseLibraryEntries",
                 12: "QueryBootMode"},
    "sifcmd": {4: "sceSifInitCmd", 14: "sceSifInitRpc", 15: "sceSifBindRpc", 16: "sceSifCallRpc",
               17: "sceSifRegisterRpc", 18: "sceSifCheckStatRpc", 19: "sceSifSetRpcQueue",
               20: "sceSifGetNextRequest", 21: "sceSifExecRequest", 22: "sceSifRpcLoop",
               23: "sceSifGetOtherData", 12: "sceSifSendCmd", 13: "isceSifSendCmd"},
    "sifman": {7: "sceSifSetDma", 8: "sceSifDmaStat", 29: "sceSifCheckInit", 5: "sceSifInit"},
    "stdio": {4: "printf"},
    "sysclib": {10: "memchr", 11: "memcmp", 12: "memcpy", 13: "memmove", 14: "memset", 16: "bcopy",
                17: "bzero", 19: "sprintf", 20: "strcat", 22: "strcmp", 23: "strcpy", 27: "strlen",
                29: "strncmp", 30: "strncpy"},
    "thbase": {4: "CreateThread", 5: "DeleteThread", 6: "StartThread", 8: "ExitThread",
               10: "TerminateThread", 14: "ChangeThreadPriority", 20: "GetThreadId", 24: "SleepThread",
               25: "WakeupThread", 26: "iWakeupThread", 33: "DelayThread", 34: "GetSystemTime",
               35: "SetAlarm", 36: "iSetAlarm", 37: "CancelAlarm", 38: "iCancelAlarm",
               39: "USec2SysClock", 40: "SysClock2USec"},
    "timrman": {4: "AllocHardTimer", 5: "ReferHardTimer", 6: "FreeHardTimer", 7: "SetTimerMode",
                8: "GetTimerStatus", 9: "SetTimerCounter", 10: "GetTimerCounter",
                11: "SetTimerCompare", 12: "GetTimerCompare", 13: "SetHoldMode", 14: "GetHoldMode",
                15: "GetHoldReg", 16: "GetHardTimerIntrCode"},
}


def imports(path):
    d = Path(path).read_bytes()
    phoff, = struct.unpack_from("<I", d, 0x1C)
    phnum, = struct.unpack_from("<H", d, 0x2C)
    for i in range(phnum):
        t, off, va, _, fsz, _ = struct.unpack_from("<6I", d, phoff + i * 32)
        if t == 1:
            break
    out = []
    for o in range(off, off + fsz - 20, 4):
        if struct.unpack_from("<II", d, o) != (0x41E00000, 0):
            continue
        lib = d[o + 12:o + 20].rstrip(b"\0").decode("latin1")
        p = o + 20
        while struct.unpack_from("<I", d, p)[0] == 0x03E00008:
            ordinal = struct.unpack_from("<I", d, p + 4)[0] & 0xFFFF
            out.append((p - off + va, lib, ordinal, NAMES.get(lib, {}).get(ordinal, "?")))
            p += 8
    return out


if __name__ == "__main__":
    f = sys.argv[1] if len(sys.argv) > 1 else ROOT / "extracted/rom0/OSDSND"
    for va, lib, n, name in imports(f):
        print(f"{va:08x}\t{lib}\t{n}\t{name}")
