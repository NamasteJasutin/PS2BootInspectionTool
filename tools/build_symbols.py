#!/usr/bin/env python3
"""Build analysis/symbols/osdsys.tsv and osdsnd.tsv.

Names are gathered from the address -> name tables and inline mentions in notes/*.md, then
merged with the hand-written lists below (syscall stubs, libc, SDK libraries, opening helpers
named only in prose).  Conflicts (same address, different names) are resolved in favour of the
hand-written / more specific name; the alternative is kept in the third column.

usage: build_symbols.py            (writes both files)
"""
import collections
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NOTES = ROOT / "notes"
OUT = ROOT / "analysis" / "symbols"

ADDR = re.compile(r"0x0*([0-9A-Fa-f]{5,8})")
IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")

# ---------------------------------------------------------------- notes ------------------

def gather_osdsys():
    found = collections.defaultdict(list)  # addr -> [(name, src)]

    def add(addr, name, src):
        name = re.sub(r"\(.*$", "", name.strip().strip("`")).strip().strip("`")
        if not IDENT.match(name) or name in ("ctx", "V", "I", "mc", "main_2"):
            return
        a = int(addr, 16)
        if not 0x200000 <= a < 0x2C9000:
            return
        if (name, src) not in found[a]:
            found[a].append((name, src))

    for nf in ("osdsys_flow.md", "opening.md", "opening_scene1.md", "sound.md"):
        for ln, line in enumerate((NOTES / nf).read_text().splitlines(), 1):
            src = f"{nf}:{ln}"
            line = line.strip()
            if line.startswith("|"):
                cells = [c.strip() for c in line.strip("|").split("|")]
                if len(cells) >= 2 and ADDR.search(cells[0]) and not ADDR.search(cells[1]):
                    addrs = ADDR.findall(cells[0])
                    c1 = re.sub(r"\([^)]*\)", "", cells[1])
                    names = [n.strip() for n in re.split(r" / |, |→", c1)]
                    if len(addrs) == len(names):
                        for a, n in zip(addrs, names):
                            if n and not n.startswith("("):
                                add(a, n, src)
                    elif len(names) == 1 and len(addrs) > 1:
                        for i, a in enumerate(addrs):
                            add(a, names[0] + ("" if i == 0 else f"_{i + 1}"), src)
            for m in re.finditer(r"`([A-Za-z_][A-Za-z0-9_]*)(?:\([^)`]*\))?`\s*\(`(0x[0-9A-Fa-f]+)`", line):
                add(m.group(2)[2:], m.group(1), src)
            for m in re.finditer(r"`(0x[0-9A-Fa-f]+)`\s*=\s*`([A-Za-z_][A-Za-z0-9_]*)", line):
                add(m.group(1)[2:], m.group(2), src)
    return found


def gather_osdsnd():
    """sound.md names OSDSND handlers as `snd:xxxxx` with a description; turn the command
    table (section 1.4) and the structural mentions (section 2) into names."""
    out = {}
    text = (NOTES / "sound.md").read_text()
    cmdnames = {
        0x5001: "OsdSnd_Reset", 0x5002: "OsdSnd_Quit", 0x5005: "OsdSnd_LoadBankWithBody",
        0x5006: "OsdSnd_RegisterBank", 0x5007: "OsdSnd_WaitSpuTransfer", 0x5008: "OsdSnd_FreeBank",
        0x5009: "OsdSnd_OpenSeq", 0x500A: "OsdSnd_SetUpdateRate", 0x500B: "OsdSnd_CloseSeq",
        0x500C: "OsdSnd_SetReverbMode", 0x500D: "OsdSnd_SetReverbVolume", 0x500E: "OsdSnd_SetReverbDelay",
        0x500F: "OsdSnd_SetReverbFeedback", 0x5010: "OsdSnd_GetActiveVoices", 0x5011: "OsdSnd_ZeroSpuRam",
        0x5012: "OsdSnd_SetMasterVolume", 0x5013: "OsdSnd_SetSeqVolume", 0x5014: "OsdSnd_PlaySeq",
        0x5015: "OsdSnd_StopSeq", 0x5016: "OsdSnd_SetTempo", 0x5017: "OsdSnd_GetTempo",
        0x5018: "OsdSnd_Stub5018", 0x5019: "OsdSnd_Stub5019", 0x501A: "OsdSnd_SpuWrite",
        0x501B: "OsdSnd_SpuWriteSwapped", 0x5100: "OsdSnd_TimerStart", 0x5101: "OsdSnd_TimerStop",
        0x5200: "OsdSnd_PlaySE", 0x5201: "OsdSnd_SetSEVolume",
    }
    for ln, line in enumerate(text.splitlines(), 1):
        m = re.match(r"\|\s*(0x[0-9A-Fa-f]{4})(?:\s*/\s*(0x[0-9A-Fa-f]{4}))?\s*\|\s*snd:([0-9a-f]{5})(?:\s*/\s*([0-9a-f]{5}))?\s*\|", line)
        if m:
            cmds = [int(m.group(1), 16)] + ([int(m.group(2), 16)] if m.group(2) else [])
            addrs = [m.group(3)] + ([m.group(4)] if m.group(4) else [])
            for c, a in zip(cmds, addrs):
                out[int(a, 16)] = (cmdnames[c], f"sound.md:{ln} (cmd {c:#x})")
    manual = {
        0x00000: ("_start", "sound.md §2.1 module entry"),
        0x000DC: ("InitThread", "sound.md §2.1"),
        0x0022C: ("RpcServerThread", "sound.md §2.1 (server 0x80000601)"),
        0x002C8: ("RpcDispatch", "sound.md §2.1"),
        0x03768: ("SequencerThread", "sound.md §2.1"),
        0x037B0: ("TimerIrqHandler", "sound.md §2.1"),
        0x03838: ("OsdSnd_TimerStart", "sound.md §1.4 cmd 0x5100"),
        0x0392C: ("OsdSnd_TimerStop", "sound.md §1.4 cmd 0x5101"),
        0x16C10: ("SeqTick", "sound.md §2.2"),
        0x1A208: ("FreeFinishedVoices", "sound.md §2.2"),
        0x1A098: ("SeqReadEvent", "sound.md §2.2"),
        0x19F64: ("SeqReadVarLen", "sound.md §2.2"),
        0x17AD8: ("VibratoPass", "sound.md §2.2"),
        0x171A4: ("SeqNoteOn", "sound.md §2.2"),
        0x18344: ("VoiceAlloc", "sound.md §2.2"),
        0x18914: ("VoicePitch", "sound.md §2.2"),
        0x18C04: ("VoiceVolume", "sound.md §2.2"),
        0x1370C: ("SpuSetVoiceRegs", "sound.md §2.2"),
        0x18560: ("SeqNoteOff", "sound.md §2.2"),
        0x1446C: ("SpuInit", "sound.md §2.3"),
        0x1C304: ("SEPickVoice", "sound.md §2.4"),
        0x1DBB0: ("PITCHTAB", "sound.md §2.2 (data)"),
        0x1E070: ("PANTAB", "sound.md §2.2 (data)"),
        0x1D2A0: ("REVERB_PRESETS", "sound.md §2.3 (data)"),
        0x21C10: ("g_banks", "sound.md §2.1 (data)"),
        0x22210: ("g_seqSlots", "sound.md §2.1 (data)"),
        0x21670: ("g_voices", "sound.md §2.1 (data)"),
        0x1CFE8: ("g_rpcResult", "sound.md §2.1 (data)"),
    }
    for a, v in manual.items():
        out.setdefault(a, v)
    return out


# ---------------------------------------------------------------- manual -----------------

SYSCALLS = {
    0x00: "RFU000_FullReset", 0x01: "ResetEE", 0x02: "SetGsCrt", 0x03: "RFU003", 0x04: "Exit",
    0x05: "RFU005", 0x06: "LoadExecPS2", 0x07: "ExecPS2", 0x08: "RFU008", 0x09: "RFU009",
    0x0A: "AddSbusIntcHandler", 0x0B: "RemoveSbusIntcHandler", 0x0C: "Interrupt2Iop",
    0x0D: "SetVTLBRefillHandler", 0x0E: "SetVCommonHandler", 0x0F: "SetVInterruptHandler",
    0x10: "AddIntcHandler", 0x11: "RemoveIntcHandler", 0x12: "AddDmacHandler", 0x13: "RemoveDmacHandler",
    0x14: "_EnableIntc", 0x15: "_DisableIntc", 0x16: "_EnableDmac", 0x17: "_DisableDmac",
    0x18: "_SetAlarm", 0x19: "_ReleaseAlarm", -0x1A: "_iEnableIntc", -0x1B: "_iDisableIntc",
    -0x1C: "_iEnableDmac", -0x1D: "_iDisableDmac", -0x1E: "_iSetAlarm", -0x1F: "_iReleaseAlarm",
    0x20: "CreateThread", 0x21: "DeleteThread", 0x22: "StartThread", 0x23: "ExitThread",
    0x24: "ExitDeleteThread", 0x25: "TerminateThread", -0x26: "iTerminateThread",
    0x27: "DisableDispatchThread", 0x28: "EnableDispatchThread", 0x29: "ChangeThreadPriority",
    -0x2A: "iChangeThreadPriority", 0x2B: "RotateThreadReadyQueue", -0x2C: "_iRotateThreadReadyQueue",
    0x2D: "ReleaseWaitThread", -0x2E: "iReleaseWaitThread", 0x2F: "GetThreadId", 0x30: "ReferThreadStatus",
    -0x31: "iReferThreadStatus", 0x32: "SleepThread", 0x33: "WakeupThread", -0x34: "_iWakeupThread",
    0x35: "CancelWakeupThread", -0x36: "iCancelWakeupThread", 0x37: "SuspendThread", -0x38: "_iSuspendThread",
    0x39: "ResumeThread", -0x3A: "iResumeThread", 0x3B: "JoinThread", 0x3C: "SetupThread", 0x3D: "SetupHeap",
    0x3E: "EndOfHeap", 0x3F: "RFU063", 0x40: "CreateSema", 0x41: "DeleteSema", 0x42: "SignalSema",
    -0x43: "iSignalSema", 0x44: "WaitSema", 0x45: "PollSema", -0x46: "iPollSema", 0x47: "ReferSemaStatus",
    -0x48: "iReferSemaStatus", 0x49: "RFU073_iDeleteSema", 0x4A: "SetOsdConfigParam", 0x4B: "GetOsdConfigParam",
    0x4C: "GetGsHParam", 0x4D: "GetGsVParam", 0x4E: "SetGsHParam", 0x4F: "SetGsVParam",
    0x50: "CreateEventFlag", 0x51: "DeleteEventFlag", 0x52: "SetEventFlag", -0x53: "iSetEventFlag",
    0x54: "ClearEventFlag", -0x55: "iClearEventFlag", 0x56: "WaitEventFlag", 0x57: "PollEventFlag",
    -0x58: "iPollEventFlag", 0x59: "ReferEventFlagStatus", -0x5A: "iReferEventFlagStatus", 0x5B: "RFU091",
    0x5C: "EnableIntcHandler", -0x5C: "iEnableIntcHandler", 0x5D: "DisableIntcHandler",
    -0x5D: "iDisableIntcHandler", 0x5E: "EnableDmacHandler", -0x5E: "iEnableDmacHandler",
    0x5F: "DisableDmacHandler", -0x5F: "iDisableDmacHandler", 0x60: "KSeg0", 0x61: "EnableCache",
    0x62: "DisableCache", 0x63: "GetCop0", 0x64: "FlushCache", 0x66: "CpuConfig", -0x67: "iGetCop0",
    -0x68: "iFlushCache", -0x6A: "iCpuConfig", 0x6B: "sceSifStopDma", 0x6C: "SetCPUTimerHandler",
    0x6D: "SetCPUTimer", 0x6E: "SetOsdConfigParam2", 0x6F: "GetOsdConfigParam2", 0x70: "GsGetIMR",
    -0x70: "iGsGetIMR", 0x71: "GsPutIMR", -0x71: "iGsPutIMR", 0x72: "SetPgifHandler", 0x73: "SetVSyncFlag",
    0x74: "SetSyscall", 0x75: "_print", 0x76: "sceSifDmaStat", -0x76: "isceSifDmaStat", 0x77: "sceSifSetDma",
    -0x77: "isceSifSetDma", 0x78: "sceSifSetDChain", -0x78: "isceSifSetDChain", 0x79: "sceSifSetReg",
    0x7A: "sceSifGetReg", 0x7B: "ExecOSD", 0x7C: "Deci2Call", 0x7D: "PSMode", 0x7E: "MachineType",
    0x7F: "GetMemorySize",
}


def syscall_stubs():
    """Scan the stub block 0x258480..0x258D00 (addiu v1,zero,N ; syscall) and name each."""
    import struct
    d = (ROOT / "extracted" / "osdsys_200000.bin").read_bytes()
    out = {}
    seen = collections.Counter()
    for off in range(0x58400, 0x58D00, 4):
        w, = struct.unpack_from("<I", d, off)
        if (w >> 16) == 0x2403 and struct.unpack_from("<I", d, off + 4)[0] == 0x0000000C:
            n = struct.unpack_from("<h", d, off)[0]
            name = SYSCALLS.get(n, f"syscall_{n & 0xFF:02x}")
            seen[name] += 1
            if seen[name] > 1:
                name += "2"  # second stub with the same number (AddIntcHandler2 / AddDmacHandler2)
            out[0x200000 + off] = (name, f"syscall stub {n:#x} (standard EE kernel name)")
    return out


MANUAL = {
    # ---- opening helpers named in prose only (opening.md §2, §4, §5) -----------------------
    0x216838: ("OpeningLoadAssets", "opening.md §2 (calls 0x216A50, 0x216918) [I]"),
    0x216918: ("TextureTableLoad", "opening.md §2 / opening_scene1.md §8.2: walks the 25-entry table, loads per scene"),
    0x216A50: ("OpeningDmaInit", "opening.md §2: sceDmaReset + VIF0/VIF1/? channel flags [I]"),
    0x217318: ("SetAlpha", "opening.md §2 table"),
    0x217368: ("SetScissor", "opening.md §2 table"),
    0x217398: ("TexSprite", "opening.md §2 table"),
    0x217408: ("FlatSprite", "opening.md §2 table"),
    0x217468: ("SetFrame", "opening.md §2 table"),
    0x2177C0: ("ProjectVertex", "opening_scene1.md §9: ApplyMatrix + 1/w + FTOI4"),
    0x2184F8: ("SetTextureDesc", "opening.md §2 table (second SetTexture entry: takes a descriptor)"),
    0x218540: ("SetZTest", "opening.md §2 table"),
    0x218588: ("SetZWrite", "opening.md §2 table"),
    0x218628: ("SetFieldOffset", "opening.md §2 table"),
    0x218688: ("SaveFrameHalfWidth", "opening.md §5 step 3"),
    0x218878: ("TemporalBlur", "opening.md §5 step 2"),
    0x218AF8: ("DefocusPasses", "opening.md §5 step 6 / scene1 §8.2 (pass count 0 = reset state)"),
    0x218DC8: ("DrawLetterbox", "opening.md §5 FrameEnd"),
    0x218F50: ("DrawFullScreenFlat", "opening_scene1.md §8.1: full-screen FlatSprite with alpha"),
    0x219098: ("Overlay_SceText_Draw", "opening.md §5 FrameEnd (0x219258/0x219098)"),
    0x219258: ("Overlay_SceText_Update", "opening.md §5 FrameEnd (counter 0..240 step 4)"),
    0x219590: ("Overlay_Update", "calls Overlay_SceText_Update [I]"),
    0x2195B0: ("BuildFogMesh", "opening.md §4 Fog mesh"),
    0x219870: ("DrawFog", "opening.md §5 step 4"),
    0x21AD38: ("DrawOrbs", "opening.md §5 step 5"),
    0x21B5F8: ("GlassShadeVertexScene0", "opening.md §5 step 5 (per-vertex callback installed in 0x2C8808)"),
    0x21BB90: ("DrawGlassCube", "opening.md §5 step 5"),
    0x21C4A0: ("BuildLightMap", "opening.md §4 Light map"),
    0x21C988: ("TowerFaceVertsA", "opening.md §5 step 1 (per-face vertex data)"),
    0x21CB40: ("TowerFaceVertsB", "opening.md §5 step 1 (per-face vertex data)"),
    0x21CC38: ("DrawTowers", "opening.md §5 step 1"),
    0x21D190: ("DefocusBlur", "opening.md §5 step 6"),
    0x21D240: ("FadeToBlack", "opening.md §5 step 7"),
    0x220E18: ("CopyFrameToOffscreen", "opening.md §5 step 5 (before the first glass pass)"),
    # ---- sound (sound.md §1) -------------------------------------------------------------
    0x2000C0: ("SifDmaCopyToIop", "sound.md §1.3 (plain SIF DMA of headers/sequences)"),
    0x200130: ("SoundUploadBody", "sound.md §1.3 (64 KiB chunks through IOPBUF, cmd 0x501A/0x5007)"),
    0x200250: ("SoundUploadBanks", "sound.md §1.3"),
    0x2004B8: ("SoundInit", "sound.md §1.3"),
    0x2007E8: ("SoundShutdown", "sound.md §1.3"),
    0x2009F8: ("SoundReverbFadeStep", "sound.md §1.3"),
    0x26DA90: ("SndRpc", "sound.md §1.1 RPC wrapper (wait, cmd, a1..a6)"),
    0x26DD58: ("SndRpcBind", "sound.md §1.1 binds server 0x80000601"),
    # ---- SIF / RPC (identified from bodies: sceSifBindRpc/CallRpc patterns) [I] -----------
    0x270080: ("sceSifCallRpc", "body: client, fno, mode, send, ssize, recv, rsize, end_func [I]"),
    0x2702C8: ("sceSifBindRpc", "body: client, server id, mode [I]"),
    0x270718: ("sceSifInitRpc", "osdsys_flow.md §2.1 step 2 (SIF RPC init) [I]"),
    0x2706F0: ("sceSifExitRpc", "osdsys_flow.md §2.1 step 2 [I]"),
    0x270C38: ("sceSifAddCmdHandler", "body: cmd id, handler, data [I]"),
    0x270C90: ("sceSifInitCmd", "called once from sceSifInitRpc [I]"),
    0x270C68: ("sceSifExitCmd", "called from sceSifExitRpc [I]"),
    0x2708A8: ("sceSifWriteBackDCache", "body: (addr, size) before SIF DMA [I]"),
    0x26F768: ("sceSifIopReset", "osdsys_flow.md §2.1 step 2: sends the UDNL string to server 0x80000003 [I]"),
    0x26F720: ("sceSifIopSync", "osdsys_flow.md §2.1 step 2: polls SBUS reg 4 bit 0x40000 [I]"),
    0x256EE8: ("LoadFileGetIopAddr", "osdsys_flow.md §2.1 step 9: LOADFILE RPC fno 3 reads an IOP word [I]"),
    0x256BA8: ("LoadFileBind", "binds the LOADFILE RPC server [I]"),
    # ---- CDVD (libcdvd RPC fno identified in bodies) [I] ------------------------------------
    0x25AC38: ("sceCdInit", "osdsys_flow.md §2.1 step 3 [I]"),
    0x25AAA0: ("cdSCmdBegin", "takes the S-command semaphore for fno [I]"),
    0x25BD18: ("sceCdBootCertify", "osdsys_flow.md §2.1 step 8 (S-cmd RPC fno 0x1E) [I]"),
    0x25BAE8: ("sceCdForbidDVDP", "osdsys_flow.md §2.1 step 8 (S-cmd RPC fno 0x18) [I]"),
    0x25D8A8: ("sceCdReadWakeUpTime", "osdsys_flow.md §2.1 step 9 (S-cmd RPC fno 0x29) [I]"),
    0x25D728: ("sceCdStreamInit", "osdsys_flow.md §2.1 step 8: sector size 0x930/0x940 stream setup [I, unverified]"),
    0x25E1A0: ("sceCdReadRegionParams", "osdsys_flow.md §3 (ConsoleRegion) [I]"),
    # ---- memory card (mcserv RPC fno 1..0xF per osdsys_flow.md §6) [I names] --------------
    0x25E298: ("sceMcInit", "osdsys_flow.md §2.1 step 3 (mcserv fno 0xFE) [I]"),
    0x25EA40: ("sceMcGetInfo", "osdsys_flow.md §6 (RPC fno 1)"),
    0x25E418: ("sceMcOpen", "osdsys_flow.md §6 (fno 2)"),
    0x25E538: ("sceMcClose", "osdsys_flow.md §6 (fno 3)"),
    0x25E5C8: ("sceMcSeek", "osdsys_flow.md §6 (fno 4)"),
    0x25E6E8: ("sceMcRead", "osdsys_flow.md §6 (fno 5)"),
    0x25E7C0: ("sceMcWrite", "osdsys_flow.md §6 (fno 6)"),
    0x25EB88: ("sceMcGetDir", "osdsys_flow.md §6 (fno 0xD)"),
    0x25EFD8: ("sceMcSetFileInfo", "osdsys_flow.md §6 (fno 0xE)"),
    0x25EE70: ("sceMcDelete", "osdsys_flow.md §6 (fno 0xF)"),
    0x25E500: ("sceMcMkdir", "osdsys_flow.md §6"),
    0x25E8F8: ("sceMcSync", "osdsys_flow.md §6"),
    # ---- pad / multitap / remote (RPC server ids in bodies) [I] ---------------------------
    0x257140: ("scePadInit", "osdsys_flow.md §2.1 step 7: binds padman 0x80000100/101 [I]"),
    0x2722D0: ("sceMtapInit", "binds mtapman 0x80000901/902 [I]"),
    0x272260: ("sceMtapPortOpen", "mtapman RPC fno 1 [I]"),
    0x25F400: ("sceRmInit", "binds 0x80000C01 (XRMMAN2 remote control) [I]"),
    # ---- misc runtime ---------------------------------------------------------------------
    0x263270: ("RuntimeInit", "osdsys_flow.md §2.1 step 1 (one-time runtime init)"),
    0x2631C0: ("RuntimeInitOnce", "called once from RuntimeInit [I]"),
    0x206E58: ("LoadAssetArchives", "osdsys_flow.md §2.1 step 7"),
    0x20DCC8: ("FontInit", "osdsys_flow.md §2.1 step 7"),
    0x207C90: ("TimezoneSetup", "osdsys_flow.md §2.1 step 11"),
    0x20FB78: ("CdPlayerStart", "osdsys_flow.md §2.1 step 12 (sound/CD-player streaming layer)"),
    0x213C08: ("CdPlayerThread", "osdsys_flow.md §2.3"),
    0x20FAD0: ("CdPlayerIntcHandler", "osdsys_flow.md §2.3 (INTC 3)"),
    0x272F30: ("sceDmaReset", "libdma: called by ResetGraphics and the module inits [I]"),
    0x273010: ("sceDmaGetChan", "libdma: index -> DMAC channel registers [I]"),
    0x273038: ("bzero_", "byte-wise zero fill [I]"),
    # ---- libvu0 (bodies read; standard SCE names) [I] --------------------------------------
    0x273070: ("sceVu0ClipAll", "libvu0 [I]"),
    0x273100: ("sceVu0ScaleVector", "libvu0 [I]"),
    0x273118: ("sceVu0ViewScreenMatrix", "libvu0 (used by TimelineStep) [I]"),
    0x273220: ("sceVu0LightColorMatrix", "libvu0 [I]"),
    0x273288: ("sceVu0NormalLightMatrix", "libvu0 [I]"),
    0x273348: ("sceVu0CameraMatrix", "libvu0 [I]"),
    0x2733F8: ("sceVu0ClampVector", "libvu0 [I]"),
    0x273420: ("sceVu0RotMatrix", "libvu0 / opening_scene1.md §9 [I]"),
    0x273470: ("sceVu0RotMatrixY", "libvu0 [I]"),
    0x273518: ("sceVu0RotMatrixX", "libvu0 [I]"),
    0x2735C0: ("sceVu0RotMatrixZ", "libvu0 [I]"),
    0x273668: ("_sceVu0SinCos", "libvu0 helper [I]"),
    0x2736E0: ("sceVu0UnitMatrix", "libvu0 [I]"),
    0x273708: ("sceVu0FTOI0", "libvu0 [I]"),
    0x273718: ("sceVu0FTOI4", "libvu0 [I]"),
    0x273728: ("sceVu0CopyMatrix", "libvu0 [I]"),
    0x273750: ("sceVu0CopyVector", "libvu0 [I]"),
    0x273760: ("sceVu0TransMatrix", "libvu0 / opening_scene1.md §9 [I]"),
    0x273790: ("sceVu0ScaleVectorXYZ", "libvu0 [I]"),
    0x2737A8: ("sceVu0SubVector", "libvu0 [I]"),
    0x2737C0: ("sceVu0AddVector", "libvu0 [I]"),
    0x2737D8: ("sceVu0DivVector", "libvu0 (used by CameraMatrix) [I]"),
    0x273848: ("sceVu0TransposeMatrix", "libvu0 (used by NormalLightMatrix) [I]"),
    0x273890: ("sceVu0Normalize", "libvu0 [I]"),
    0x2738C8: ("sceVu0InnerProduct", "libvu0 [I]"),
    0x2738F0: ("sceVu0OuterProduct", "libvu0 [I]"),
    0x273910: ("sceVu0MulMatrix", "libvu0 / opening_scene1.md §9 [I]"),
    0x273958: ("sceVu0ApplyMatrix", "libvu0 [I]"),
    # ---- second pass: identified while reading the named export [I unless noted] -----------
    0x268840: ("strrchr", "body: last occurrence [I]"),
    0x267FA4: ("strchr", "body: vectorised scan for a byte [I]"),
    0x267E78: ("strcat", "body: vectorised strlen + copy [I]"),
    0x2675AC: ("memcpy", "body: 32-byte unrolled copy [I]"),
    0x26FD68: ("sceOpen", "fileio RPC (server 0x80000001) [I]"),
    0x26FA10: ("sceRead", "fileio RPC [I]"),
    0x26FBD0: ("sceLseek", "fileio RPC [I]"),
    0x26FCB0: ("sceClose", "fileio RPC fno 1 [I]"),
    0x26FEC0: ("sceFsInit", "binds the fileio server 0x80000001 [I]"),
    0x256D48: ("sceSifLoadModule", "LOADFILE RPC wrapper [I]"),
    0x256C68: ("_sceSifLoadModule", "LOADFILE RPC [I]"),
    0x209748: ("LoadIopModuleFromRom", "builds 'rom0:'+name (or mc path) and calls sceSifLoadModule"),
    0x2099A0: ("Dev9ServLoaded", "after XDEV9SERV loaded (sets up the dev9 RPC) [I]"),
    0x2096C8: ("LoaderStateSet", "(state record, value): store + SignalSema"),
    0x2096E8: ("LoaderStateWait", "(state record, value): WaitSema until state >= value"),
    0x200ED0: ("LzDecompress", "OSD LZ stream decoder (README: osd_unpack.py), returns output size"),
    0x200F08: ("LzOutputSize", "reads the u32 output size of an LZ stream"),
    0x200CF8: ("LzReadHeader", "LZ stream header"),
    0x200DA0: ("LzDecode", "LZ stream body decoder"),
    0x20F020: ("RomdirParse", "(buf, end, table): parses a nested ROMDIR archive header into a 16-entry table"),
    0x20F0B8: ("RomdirFind", "(table, name, out): finds a nested archive entry; out = {ptr, size}"),
    0x271D70: ("EnableIntcSafe", "DI/EI wrapper around _EnableIntc"),
    0x204428: ("CfgWord1Bit3", "config word 1 bit 3 (passed inverted to cdSCmd_fno30)"),
    0x204480: ("CfgSetWord1Bit3", "config setter (wake-up path)"),
    0x2044C0: ("CfgSetWord1BitX", "config setter (wake-up path)"),
    0x204570: ("CfgTimezone", "config word 0 bits 20..28"),
    0x25DAA8: ("cdSCmd_fno30", "S-command RPC fno 0x30 with one int arg; main loops on it until status bit 0x80 clears [I: possibly sceCdRcBypassCtl / sceCdSetMediumRemoval]"),
    0x26F280: ("sceGsResetGraph", "libgraph (mode, interlace, omode, ffmode) [I]"),
    0x26EB80: ("sceGsSetDefDBuff", "libgraph (db, psm, w, h, ztest, zpsm, clear) [I]"),
    0x26EB20: ("sceGsSwapDBuff", "libgraph (db, id) [I]"),
    0x26E210: ("sceGsSetHalfOffset", "libgraph (drawenv, x, y, field) [I]"),
    0x26EE30: ("sceGsPutDrawEnv", "libgraph: GIF DMA of a draw-env packet [I]"),
    0x26F270: ("sceGsGetGParam", "libgraph [I]"),
    0x26EA88: ("sceGsSyncV_", "libgraph: reads GParam and CSR FIELD; called by VideoInit before ResetGraph [I]"),
    0x26F208: ("sceGsResetPath", "libgraph: VIF1/VU1/GIF path reset [I]"),
    0x272B48: ("sceDevVif0Reset", "libdev [I]"),
    0x272B20: ("sceDevVif1Reset", "libdev [I]"),
    0x272BA0: ("sceDevVu0Reset", "libdev (VI12 bit 1) [I]"),
    0x272B88: ("sceDevVu1Reset", "libdev (VI12 bit 9) [I]"),
    0x272B70: ("sceDevGifReset", "libdev [I]"),
    # ---- data ------------------------------------------------------------------------------
    0x1F0000: ("ctx", "osdsys_flow.md: global OSD context (outside the image; label only)"),
    0x2C8700: ("g_frame", "opening.md §1"),
    0x2C8704: ("g_scene", "opening.md §1"),
    0x2C8708: ("g_nextScene", "opening.md §1"),
    0x2C870C: ("g_startScene", "opening.md §1"),
    0x2C8710: ("g_subState", "opening.md §1"),
    0x2C8714: ("g_letterbox", "opening.md §1"),
    0x2C87A0: ("g_latchedDiscState", "osdsys_flow.md §2.4"),
    0x288EB0: ("g_camPos", "opening.md §3"),
    0x288EC0: ("g_camDir", "opening.md §3"),
    0x288ED0: ("g_camUp", "opening.md §3"),
    0x2C8798: ("g_camRoll", "opening.md §3"),
    0x346EC0: ("g_stage", "opening.md §3"),
    0x289058: ("g_stageThresholds", "opening.md §3"),
    0x288EE0: ("g_lightDirs", "opening.md §3"),
    0x287700: ("g_textureTable", "opening.md §2"),
    0x288F10: ("g_blendModes", "opening.md §2"),
    0x2891E0: ("g_slotTable", "opening.md §4"),
    0x2895F0: ("g_slotPositions", "opening.md §4"),
    0x349950: ("g_lightMap", "opening.md §4"),
    0x273990: ("vu1_init_chain", "opening.md §2 (DMA chain uploading the VU1 program)"),
    0x2D9C00: ("g_moduleTable", "osdsys_flow.md §2.2"),
    0x27B420: ("g_moduleCount", "osdsys_flow.md §2.2"),
    0x2DA4A0: ("g_moduleThreads", "osdsys_flow.md §2.2"),
    0x27E6F8: ("sema_moduleDone", "osdsys_flow.md §2.3"),
    0x27E6FC: ("sema_vblankInput", "osdsys_flow.md §2.3"),
    0x27E710: ("sema_vsyncRequest", "osdsys_flow.md §2.3"),
    0x27E714: ("sema_flipRequest", "osdsys_flow.md §2.3"),
    0x27E718: ("sema_flipGo", "osdsys_flow.md §2.3"),
    0x27E71C: ("sema_vsyncDone", "osdsys_flow.md §2.3"),
    0x27E720: ("sema_soundFlush", "osdsys_flow.md §2.3"),
    0x27C654: ("g_vblankCount", "osdsys_flow.md §3"),
    0x27B414: ("g_setupDoneFlag", "osdsys_flow.md §3"),
    0x27B418: ("g_hddBootFlag", "osdsys_flow.md §3"),
    0x27B41C: ("g_hddBootStatus", "osdsys_flow.md §3"),
    0x27B400: ("g_romRegionCache", "osdsys_flow.md §3"),
    0x27B3D8: ("g_stringTable", "osdsys_flow.md §3"),
    0x27B504: ("g_assetTable", "sound.md §1.3"),
    0x2C9700: ("g_nvramConfig", "osdsys_flow.md §2.1 step 10"),
    0x2C9780: ("g_configWord0", "osdsys_flow.md §2.1 step 10"),
    0x2C9784: ("g_configWord1", "osdsys_flow.md §2.1 step 10"),
    0x303218: ("g_discThreadEnable", "osdsys_flow.md §3"),
    0x30321C: ("g_discVerifyEnable", "osdsys_flow.md §3"),
    0x303230: ("g_discState", "osdsys_flow.md §4.1"),
    0x27A180: ("sema_sound", "sound.md §1.2"),
    0x27A188: ("g_iopSoundBuf", "sound.md §1.3 (IOPBUF)"),
    0x4169C0: ("g_sndRpcBuf", "sound.md §1.1"),
    0x2AE9A0: ("g_sndRpcClient", "sound.md §1.1"),
    0x2C9018: ("g_scene1Brightness", "opening_scene1.md §12"),
    0x2C87E4: ("g_firstFrameFlag", "opening_scene1.md §12"),
    0x2C87E8: ("g_fadeOutAlpha", "opening_scene1.md §12"),
    0x2C87F4: ("g_fadeOutFlag", "opening_scene1.md §12"),
    0x2C8800: ("g_fadeOutStartFrame", "opening_scene1.md §12"),
    0x2C900C: ("g_textState", "opening_scene1.md §12"),
    0x2C9010: ("g_textAlpha", "opening_scene1.md §12"),
    0x289ED0: ("g_prismPositions", "opening_scene1.md §12"),
}

# conflicts between notes: winner first, loser recorded as alternative
RESOLVE = {
    0x201E98: "HistoryUpdate",
    0x2042C0: "LanguageValidated",
    0x2053F0: "ConsoleRegionRaw",
    0x205880: "ConsoleRegion",
    0x209270: "ExecMcUpdate",
    0x209548: "FatalBootError",
    0x21E5B0: "Scene1_Dispatch",
    0x25BD18: "sceCdBootCertify",
}


def build_osdsys():
    found = gather_osdsys()
    rows = {}
    for a, lst in found.items():
        names = []
        for n, s in lst:
            n = n.replace("`", "")
            if n not in [x for x, _ in names]:
                names.append((n, s))
        if a in RESOLVE:
            win = RESOLVE[a]
            alts = [n for n, _ in names if n != win]
            src = next((s for n, s in names if n == win), names[0][1])
            rows[a] = (win, src + ("" if not alts else "; alt: " + ", ".join(alts)))
        elif len(names) == 1:
            rows[a] = (names[0][0], names[0][1])
        else:
            # same notes, different spellings: take the longest (most specific)
            names.sort(key=lambda t: -len(t[0]))
            rows[a] = (names[0][0], names[0][1] + "; alt: " + ", ".join(n for n, _ in names[1:]))
    for a, (n, s) in {**syscall_stubs(), **MANUAL}.items():
        if a in rows and rows[a][0] != n:
            s = s + f"; alt: {rows[a][0]} <{rows[a][1].split(';')[0]}>"
        rows[a] = (n, s)
    return rows


def write(path, rows, header):
    OUT.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        f.write(header)
        f.write("address\tname\tsource\n")
        for a in sorted(rows):
            n, s = rows[a]
            f.write(f"{a:08X}\t{n}\t{s}\n")
    print(f"{path}: {len(rows)} symbols")


if __name__ == "__main__":
    write(OUT / "osdsys.tsv", build_osdsys(),
          "# OSDSYS (SCPH-70004 ROM 2.00 E) symbol table, generated by tools/build_symbols.py\n"
          "# address = virtual address in extracted/osdsys_200000.bin; source = where the name was proposed\n"
          "# ([I] = inferred SDK/library identity, see the notes).  Alternatives are listed as 'alt:'.\n")
    write(OUT / "osdsnd.tsv", gather_osdsnd(),
          "# OSDSND (rom0:OSDSND IRX, IOP) symbol table, generated by tools/build_symbols.py\n"
          "# address = ELF vaddr (file offset = vaddr + 0xA0); source = notes/sound.md section\n")
