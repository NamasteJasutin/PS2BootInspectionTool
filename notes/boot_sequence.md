# Boot sequence: power-on to the first frame of the opening

Subject: SCPH-70004, ROM 2.00 E (`ROMVER = 0200EC20040614`). Cold power-on with no arguments,
which is the only path that leads to the tower animation. The opening itself is in
`notes/opening.md`; the OSDSYS main loop and modules are in `notes/osdsys_flow.md`; the sound
driver in `notes/sound.md`. This note only covers *what happens before the first frame* and
*what gates it*, so that the black-screen period can be reproduced.

Sources: `analysis/reset/`, `analysis/kernel/`, `analysis/eeload/` (Ghidra, names from
`analysis/symbols/{reset,kernel,eeload}.tsv`) and `analysis/osdsys_named/` (OSDSYS with
`analysis/symbols/osdsys.tsv` applied). Address prefixes: `rom:` = ROM image (`0xBFC00000`),
`k:` = KERNEL (`0x80000000`), `el:` = EELOAD (`0x82000`), no prefix = OSDSYS (`0x200000`),
`iop:` = IOP module. Function names in `code` are the ones in the symbol tables.

Conventions: **[V]** verified by reading the decompilation / disassembly; **[I]** inferred
(hardware behaviour, SDK identity, or a duration that was estimated rather than read).
**No measurement on hardware or in an emulator exists for any of the times below**; every
duration is an estimate from data sizes and known bus speeds, and the notes say so.

---

## 1. Overview

```
power-on
  │ EE and IOP both start at 0xBFC00000 (same ROM)                               rom:_reset_vector [V]
  ├─ IOP: iop_reset → finds IOPBOOT in ROMDIR → IOPBOOT loads rom0:IOPBTCONF modules  [V/I]
  │        … SIFMAN sets SMFLAG|0x10000 … EESYNC sets SMFLAG|0x40000 (boot end)        [V]
  └─ EE : ee_reset → rdram_init → ee_load_kernel (copies KERNEL to 0x80000000) → k:_kernel_entry [V]
            k: TlbInit, IntcDmacInit, HardwareInit, ThreadInit, SifInit(waits SMFLAG&0x10000),
               Deci2Init, CreateRomThread("EELOAD",0x82000) → StartThreadRaw                [V]
            el:_start → main(argc=0) → WaitIopReady (SMFLAG&0x40000) → sceSifInitRpc →
               LoadElfAll("rom0:OSDSYS") (LOADFILE RPC, ROM→RAM 0x100000) → ExecPS2 [V]
            OSDSYS stub 0x100008: LZ-decompress 0x586DC → 0xC8F74 bytes at 0x200000, jump 0x200008 [V]
            _start → main(argc=1, {"rom0:OSDSYS"})                                           [V]
               ├─ sceSifIopReset("rom0:UDNL rom0:OSDCNF") + sceSifIopSync   ← IOP reboot #2 [V]
               │     IOP: UDNL loads the IOPBTCONF inside rom0:OSDCNF (39 modules incl.
               │          XSIO2MAN, XMCMAN/XMCSERV, XPADMAN, XCDVDMAN/XCDVDFSV, OSDSND)      [V]
               ├─ sceCdInit, sceMcInit, TryMcSystemUpdate (first memory-card access)       [V]
               ├─ StartDev9Loader (thread), sceMtapInit, sceRmInit, scePadInit             [V]
               ├─ LoadAssetArchives (≈1.7 MB from ROM, LZ-decoded), FontInit               [V]
               ├─ SoundInit (410 KB of samples → SPU2 RAM, banks/sequences registered)      [V]
               ├─ CDVD: BootCertify, StreamInit, ForbidDVDP, WaitDev9Loader, ReadWakeUpTime,
               │        NVRAM config, language, cdSCmd_fno30, time zone, SPDIF             [V]
               ├─ HistoryLoadAtBoot (mc0:/BEDATA-SYSTEM/history → ctx+0x138)               [V]
               ├─ ResetGraphics, RegisterAllModules, InitModules (3 UI threads, asleep)      [V]
               ├─ 7 semaphores, 4 service threads, CdPlayerStart, DiscThreadStart          [V]
               ├─ VideoInit (SetGsCrt PAL 640×256 interlaced; TV gets sync, black)          [V]
               ├─ AddIntcHandler(VBLANK_START) → from here VBlank interrupts are counted    [V]
               └─ main loop: WakeupThread(OpeningThread); WaitSema(sema_moduleDone)        [V]
            OpeningThread: OpeningPickScene → OpeningStartSound (queues SNDBOOTS) →
               OpeningInit (textures, scene data, VU1 program, WaitVSyncBegin) →
               OpeningRun: frame 0 … (first 1–2 frames forced black)                       [V]
```

---

## 2. Call tree (named, with addresses)

### 2.1 RESET (ROM at `0xBFC00000`) **[V]**

```
rom:_reset_vector        BFC00000  mfc0 PRId; PRId < 0x59 → BFC02000 (IOP, R3000) else → BFC00800 (EE)
├─ rom:iop_reset         BFC02000  (IOP) clears R3000 breakpoint regs, isolates+flushes cache,
│                                  writes SSBUS timing table (BFC024A8 / BFC02560 by chip rev),
│                                  progress byte → 0xBF802070 (1..9), RAM_SIZE → 0xBF801060,
│                                  iop_romdir_lookup("IOPBOOT") → jump with a0 = 2; else spin (0xFA)
│                                  (TBIN is tried first only when PRId ≥ 0x10 and 0xBF801450 bit 3)
└─ rom:ee_reset          BFC00800  Config 0x73003, Status 0x70400000, Count/Compare,
                                   0xB000F500 ← -1, TLB entry for scratchpad 0x70000000,
                                   rom:rdram_init (BFC41000, the RDRAM module) → result to 0x70003FF0
                                   (negative → hang),
                                   rom:ee_load_kernel BFC00C00: romdir_find_root/romdir_lookup("KERNEL"),
                                     copy KERNEL (0x16E28 bytes) to 0xA0000000, zero up to 0xA0080000,
                                   rom:ee_flush_caches, jump 0x80001000
```

The remaining EE code in RESET (`BFC00EA0`..`BFC013F8`, CDVD S-command helpers using
`0x1F402017`) is not referenced by the kernel or by the cold-boot path; its caller was not
found **[V: no xrefs from KERNEL]**.

### 2.2 KERNEL (`0x80000000`) **[V]**

```
k:_kernel_entry          80001000  saves 0x70003FF0 (RDRAM result) and ROM word 0xBFC001F8
├─ k:TlbInit             80005C30  "# TLB spad=0 kernel=..." TLB setup
├─ k:IntcDmacInit        80002050  INTC/DMAC/SBUS handler tables, _EnableIntc(1)
├─ k:HardwareInit        8000DAD8  "# Initialize Start.": GsInit, SetGsCrt(1,1,1), IntcMaskSet,
│                                  TimerInit, ResetEE(0x7F) (DMAC,VU1,VIF1,GIF,VU0,VIF0,IPU),
│                                  FpuInit, UserMemoryClear(0x80000), ScratchpadClear
├─ k:ThreadInit          80004EC0  256 TCBs at 0x8001A640 (0x4C each); thread 0 = CreateRomThread("EENULL", 0x81FC0, prio 0x80)
├─ Status/Config from 0x80015444/48
├─ k:Deci2Init           8000EF18  "EE DECI2 Manager version 0.06" (TTY/debugger protocol, idle without a host)
├─ k:SifInit             80006238  clears SIF cmd buffers 0x80021240, SBUS regs,
│                                  **spins until SMFLAG (0xB000F230) & 0x10000**  ← IOP SIFMAN up
├─ k:SetIdleStackTop(0x81FE0), k:SetVSyncFlag(0,0)
├─ k:CreateRomThread     80005388  ("EELOAD", 0x82000, 0x81000, prio 0): copies rom0:EELOAD to 0x82000,
│                                  argc = 0 (no argument block), FlushDCache/FlushICache
└─ k:StartThreadRaw      80005140  TCB state = running, EPC = 0x82000, ReturnFromException → EELOAD
```

Syscall dispatch: exception 8 → `k:exc_syscall` (`80000280`), table at `0x80014F40`
(handlers named `k_<Name>` in `analysis/kernel/`). Of interest for the boot chain:
`k_LoadExecPS2` = `KLoadExec` (`80005598`), `k_ExecOSD` (`80005988`) = `KLoadExec("rom0:OSDSYS",
argc, argv)`, `k_Exit` (`80000D80` → `ExitToBrowser`, `800059A0`) = `KLoadExec("rom0:OSDSYS",
1, {"BootBrowser"})`. `KLoadExec` terminates all other threads, `HardwareRestart`
(`8000DBA0`, clears RAM from 0x82000), re-copies EELOAD to 0x82000 with the argument block
`"EELOAD"\0<path>\0<args…>` and jumps to it. **So a game's `Exit()` and every `LoadExecPS2`
go through EELOAD again, with argc ≥ 2; only the cold boot gives argc = 0.**

### 2.3 EELOAD (`0x82000`) **[V]**

```
el:_start                82008   zero bss 0x91200..0x955A4, SetupThread(gp 0xA91F0, stack 0x91200/0x2000),
                                 SetupHeap(0x955A4, 0x2000) → main(argc, argv)
el:main                  82388
├─ el:RuntimeInit        82F30
├─ (argc ≥ 2 only)       LoadExecPS2 path: StrPrefix "moduleload"/"moduleload2 ", IopResetAndWait
│                        ("rom0:UDNL rom0:EELOADCNF" unless moduleload2 gave a string), -m/-k/-x items,
│                        LoadElfAll(argv[1]) → -2 → BootIllegalToOsd, <0 → BootErrorToOsd, ExecLoaded
├─ el:WaitIopReady       82170   **spins until SIF SMFLAG (0x1000F230) & 0x40000, then acknowledges**
├─ el:sceSifInitRpc      84180   installs SIF cmd handlers 8/9/A/C, handshake on SBUS reg 0x80000002
├─ FlushCache(0)
├─ if (*(byte*)0x1F402005 & 0x10): LoadElfAll("rom0:TESTMODE") [I: factory test mode]
│  else: LoadElfAll("rom0:OSDSYS")  → el:LoadElf 82AD0: LoadFileBind (server 0x80000006, retries with
│        1M-iteration delays), sceSifCallRpc(fno 1 = LF_F_ELF_LOAD, path, "all") → {entry, gp} at 0x8FF10
└─ el:ExecLoaded         822B8   FlushCache, SifExitDma, ExecPS2(entry 0x100008, gp, 1, {"rom0:OSDSYS"})
```

### 2.4 OSDSYS stub and `main` (`0x200000`) **[V]**

`rom0:OSDSYS` is an ELF with one segment at `0x100000` (`0x586DC` bytes, entry `0x100008`).
The stub LZ-decodes the payload (decoder at `0x100B30`, format in `README.md`) to `0x200000`
(`0xC8F74` bytes) and jumps to `0x200008` (`_start`), which calls `main` (`209EB8`) with the
arguments EELOAD passed.

`main`, in order (the step numbers match `osdsys_flow.md` §2.1):

```
 1 RuntimeInit 263270
 2 sceSifInitRpc 270718; HasDevicePrefix("rom") → reset string "rom0:UDNL rom0:OSDCNF";
   sceSifExitRpc; sceSifIopReset 26F768 (SIF cmd 0x80000003 carrying the string);
   do { sceSifIopSync 26F720 } while (!= 1)   ← **spins on SMFLAG & 0x40000** (IOP reboot #2)
   sceSifInitRpc
 3 sceCdInit(1) 25AC38 (binds 0x80000592.. with delay loops); ConsoleRegion → (China: ADDROM2);
   sceMcInit 25E298 (binds 0x80000400, mcserv fno 0xFE); PatchRegionFolderNames
 4 scan argv for SkipMc/SkipHdd/SkipForbid (none on cold boot)
 5 TryMcSystemUpdate 209378: for port 0, 1: sceMcGetInfo + sceMcSync, look for
   B?EXEC-SYSTEM/osd210.elf, osdmain.elf      ← **first memory-card access** (card mount on the IOP)
 6 StartDev9Loader 209A90: rom0:XDEV9 exists → Dev9LoaderThread (prio 3) loads XDEV9, XDEV9SERV
   (LoadIopModuleFromRom 209748 → sceSifLoadModule); main continues
 7 sceMtapInit 2722D0 (binds 0x80000901/902), sceMtapPortOpen ×4, sceRmInit 25F400 (0x80000C01),
   scePadInit 257140 (0x80000100/101);
   LoadAssetArchives 206E58: for each entry of the asset table 27B4F8:
       type 1 FONTM:    sceOpen/sceLseek/sceRead rom0:FONTM (0xB42DC = 738 KB) → 0x700000, LzDecompress → 0x1A00000
       type 2 archives: sceRead rom0:FNTIMAGE (208 KB), SNDIMAGE (407 KB), TEXIMAGE (276 KB), ICOIMAGE (72 KB)
                        → 0x700000, RomdirParse → nested table 0x2DA468
       type 4 entries:  RomdirFind + LzDecompress (or memcpy, type 3) into the placed buffers
   AssetPtr(0) → FontInit 20DCC8
   SoundInit 2004B8 (sound.md §1.3): CreateSema, SndRpcBind (0x80000601), cmd 0x5001,
       SoundUploadBanks 200250: IOP heap alloc 0x10000, SoundUploadBody ×2 (SNDBOOTB 0x56F10 +
       SNDOSDDB 0xD480 bytes in 64 KiB chunks: sceSifSetDma + wait, cmd 0x501A, cmd 0x5007),
       SifDmaCopyToIop ×10 (headers/sequences), then 2 banks, 8 sequences, volumes, cmd 0x5100
 8 sceOpen/sceRead/sceClose rom0:ROMVER; sceCdBootCertify 25BD18 ({2,0,'E','C'}); sceCdStreamInit 25D728;
   RomRegion; ctx[0x0C] = 0; do { sceCdForbidDVDP 25BAE8 } while (status & 0x80)
 9 WaitDev9Loader 209B10 (LoaderStateWait on the XDEV9 thread's semaphore, state ≥ 2);
   do { sceCdReadWakeUpTime 25D8A8 } while (== 0); LoadFileGetIopAddr(0x3C0) ×2 loops;
   wake-up reason 0/1/0x100 → continue (2/3/other → PowerOffConsole)
10 LoadNvramConfig 204158 (ReadConfigBlock → sceCdReadNVM RPCs); SetupRequired if never initialised;
   Language/SetLanguage; ApplyOsdConfigToKernel 208660 (SetOsdConfigParam/2);
   do { cdSCmd_fno30 25DAA8 (!CfgWord1Bit3) } while (status & 0x80 || result == 0)
11 CfgTimezone, CfgTimezoneOffset → TimezoneSetup 207C90; SndRpc(1, 0x1031, !CfgSpdif);
   HistoryLoadAtBoot 204858 → HistoryLoad(0) [→ HistoryLoad(1)]: sceMcGetInfo, sceMcOpen, sceMcRead(0x1CE), sceMcClose
   ResetGraphics 208980; RegisterAllModules 206C70; InitModules 209460
       (OpeningCreateThread → CreateThread/StartThread OpeningThread 2162C0, prio 6 → SleepThread; same for clock, browser)
12 CreateSema ×7 (sema_moduleDone 27E6F8 … sema_soundFlush 27E720);
   CreateThread/StartThread: FlipThread 207BB0 (prio 1), InputThread 208DB8 (3), SoundFlushThread 209220 (3),
   ConfigWorkerThread 208800 (4); CdPlayerStart 20FB78 (CdPlayerThread 213C08, INTC 3 handler);
   DiscThreadStart(5) 20F228 (DiscThread 20F478); ctx defaults (ctx[0x10] = 0x65 "detecting")
13 VideoInit 207930: 640×256 (PAL), sceGsSyncV_, sceGsResetGraph(2, 1, 3 = PAL, 1), sceGsSetDefDBuff(…, clear),
   sceGsSwapDBuff ×2; VideoApplyOutputMode 207A10; DI; AddIntcHandler(2 = VBLANK_START, VblankStartHandler 208550); EI;
   EnableIntcSafe(2); ChangeThreadPriority(main, 0x1E)
14 ctx[0x5E8] = 1 (opening); HddBootEnabled() == 0 on this ROM
15 loop: ResetGraphics; FlushCache(2); WakeupThread(g_moduleThreads[0] = OpeningThread); ctx[0x9C] = 0;
         WaitSema(sema_moduleDone)
```

### 2.5 OpeningThread up to the first frame **[V]**

```
OpeningThread 2162C0: SleepThread → (woken by main)
├─ OpeningPickScene 216520   g_startScene = (ctx[0x5E8] == 4) → 0; g_letterbox = (ScreenType() != 1)
├─ OpeningStartSound 216568  SoundCmd(0x5014, 0): *queues* "play SNDBOOTS" (ring at ctx+0x5F8)
├─ OpeningInit 216340
│   ├─ OpeningLoadAssets 216838: sceDev*Reset ×5, sceGsResetPath, clear VU1 mem, OpeningDmaInit,
│   │     217768 / 216CC8 / 218508 / 2168B8 (packet buffers, state), TextureTableLoad 216918 →
│   │     LoadTexture 217B98 for the scene-0 entries (0, 2–6, 8, 10–12: ≈ 300 KB of texels, mips built on the EE, uploaded via VIF1/GIF)
│   ├─ ctx[0xA60]/[0xB50] draw-env tweak, 21A550 (timeline reset: g_stage = 0)
│   ├─ Scene0Init 21D470  history table → tower list, light map, fog mesh; kicks the VU1 program chain 273990
│   ├─ Scene1_InitOnce 21E578, Overlay_Reset 219558
│   └─ WaitVSyncBegin 207AD8  **SignalSema(sema_vsyncRequest); WaitSema(sema_vsyncDone)** ← first VBlank wait
│       g_frame = ctx[0xC40] (back-buffer index, 0 or 1)
├─ DiscSetVerifyEnable(0)
└─ OpeningRun 216490: g_subState = 0; while (g_scene != 2) { FrameBegin (TimelineStep, scissor);
       Scene0Dispatch (sub-state 0: Scene0Start + Scene0Draw); FrameEnd (Overlay_Update, DrawLetterbox,
       EndFrameFlip = SignalSema(sema_flipRequest); WaitSema(sema_vsyncDone); g_frame++) }
```

`FadeToBlack` (`21D240`) draws a full black sprite while `g_frame < 2`, so the first one or
two rendered frames (depending on the back-buffer index at start) are black; the towers appear
on frame 2 (or 1), i.e. within ≤ 3 VBlanks of `OpeningInit` finishing.

The queued play command is sent to the IOP by `SoundFlushThread` at the first VBlank after it
was queued (`InputThread` → `sema_soundFlush` → `SoundFlush` → blocking RPC). That VBlank falls
inside `OpeningInit`, **so the boot chord is keyed on 1–3 fields before the first non-black
frame**; the audible part (program 0, tick 480) starts 0.67 s later (`sound.md` §4).

---

## 3. IOP side

### 3.1 First boot (from power-on) **[V for the lists and EESYNC; I for IOPBOOT's internals]**

`rom:iop_reset` jumps to `rom0:IOPBOOT` (0xBFC4A000), which reads `rom0:IOPBTCONF` (strings
`IOPBTCONF`, `!addr` in IOPBOOT) and loads, in order:

```
@800  SYSMEM LOADCORE EXCEPMAN INTRMANP INTRMANI SSBUSC DMACMAN TIMEMANP TIMEMANI SYSCLIB HEAPLIB
      EECONF THREADMAN VBLANK IOMAN MODLOAD ROMDRV STDIO SIFMAN IGREETING SIFCMD REBOOT LOADFILE
      CDVDMAN CDVDFSV SIFINIT FILEIO SECRMAN EESYNC
```

* `SIFMAN` initialises the SIF and sets `SMFLAG |= 0x10000` [I: standard SIF protocol; the EE
  kernel's `SifInit` waits for exactly this bit **[V]**].
* `EESYNC` (`extracted/rom0/EESYNC`, 0x160 bytes of code) registers a *post-boot callback*
  with LOADCORE (export 0x14) that calls SIFMAN export 0x18 with `0x40000` — i.e. `SMFLAG |=
  0x40000` once every module of the list has been started **[V]**. That bit is what
  `el:WaitIopReady` and `sceSifIopSync` poll.
* `LOADFILE` provides the RPC server `0x80000006` used by EELOAD to load OSDSYS; `CDVDMAN`
  spins up the drive controller during its own init [I].

### 3.2 Second boot (`rom0:UDNL rom0:OSDCNF`, from OSDSYS `main` step 2) **[V for the list]**

`UDNL` (strings: `IOPBTCONF`, `!include`, `panic ! '%s' not found`) resets the IOP and boots
the `IOPBTCONF` found *inside* the `rom0:OSDCNF` ROMDIR archive:

```
@800  SYSMEM LOADCORE EXCEPMAN INTRMANP INTRMANI SSBUSC DMACMAN TIMEMANP TIMEMANI SYSCLIB HEAPLIB
      EECONF THREADMAN VBLANK IOMAN MODLOAD ROMDRV ADDDRV STDIO SIFMAN IGREETING XSIFCMD REBOOT
      XLOADFILE XCDVDMAN XCDVDFSV SIFINIT XFILEIO SECRMAN EESYNC RMRESET CLEARSPU XSIO2MAN
      XMTAPMAN XMCMAN XMCSERV XPADMAN XRMMAN2 OSDSND
```

Note that `EESYNC` is *not* last here: `RMRESET`, `CLEARSPU` (silences the SPU2), `XSIO2MAN`,
`XMTAPMAN`, `XMCMAN`, `XMCSERV`, `XPADMAN`, `XRMMAN2` and `OSDSND` start after it. Since
EESYNC's callback is a LOADCORE *post-boot* callback it still runs after the whole list [I:
LOADCORE semantics]; if it did not, OSDSYS's RPC binds (`sceMcInit`, `scePadInit`, `SndRpcBind`,
…) would simply spin in their retry loops until the servers appear — every bind in `main` is
written as a retry loop with a delay (`0xFFFF`–`0xFFFFF` iterations ≈ 0.2–3.5 ms at 294 MHz).
Either way the EE does not proceed past step 3 until these servers exist.

RPC servers used before the first frame (all registered during this boot): `0x80000003`
(sysmem/IOP reset, heap for `SoundUploadBanks`), `0x80000006` (XLOADFILE: `LoadIopModuleFromRom`,
`LoadFileGetIopAddr`), `0x80000001` (XFILEIO: `sceOpen/sceRead`), `0x80000592..` (XCDVDFSV),
`0x80000400` (XMCSERV), `0x80000100/101` (XPADMAN), `0x80000901/902` (XMTAPMAN), `0x80000C01`
(XRMMAN2), `0x80000601` (OSDSND).

The `EELOADCNF` archive (used only on the `LoadExecPS2` path) holds a third list, a copy of the
first one with `XLOADFILE`/`NCDVDMAN`.

---

## 4. Timeline with estimates

Nothing here was measured. Each row gives the work that is done and a best-effort duration
derived from data sizes, loop structure and typical PS2 bus speeds; the basis is stated so the
numbers can be replaced when a capture exists.

| # | Phase (who) | What gates the next step | Estimate | Basis / status |
|---|---|---|---|---|
| 0 | Power-on; both CPUs start at ROM | — | 0 | — |
| 1 | EE `ee_reset`: RDRAM init, copy KERNEL (94 KB) + zero 512 KB, caches | `rdram_init` result ≥ 0 | 5–30 ms | [I] RDRAM training loops in `rdram_init` (FUN_bfc41268, 3.9 KB of code); memory copy is trivial |
| 2 | EE kernel init (`TlbInit` … `Deci2Init`) | — | < 5 ms | [I] no waits except `SifInit` |
| 3 | EE `SifInit` spins on `SMFLAG & 0x10000` | IOP has started SIFMAN (19th module) | overlaps 4 | [V] wait, [I] when SIFMAN comes up: ≈ 60 % through boot #1 |
| 4 | IOP boot #1: IOPBOOT + 29 modules from ROM (255 KB of IRX, relocated/linked by LOADCORE on a 36 MHz R3000) | `EESYNC` post-boot callback → `SMFLAG & 0x40000` | 0.3–0.8 s | [I] module sizes from ROMDIR; typical IOP module load ≈ 10–25 ms each |
| 5 | EELOAD: `WaitIopReady` (end of 4), `sceSifInitRpc` handshake | IOP SIFCMD/LOADFILE | ≈ 0 after 4 | [V] |
| 6 | EELOAD `LoadElfAll("rom0:OSDSYS")`: IOP reads 363 KB from ROM and DMAs it to 0x100000 | LOADFILE RPC completes | 0.1–0.3 s | [I] ROM read on the IOP bus ≈ 1.5–4 MB/s |
| 7 | OSDSYS stub decompresses 363 KB → 823 KB at 0x200000 | — | 30–100 ms | [I] byte-wise LZ decoder, EE at 294 MHz |
| 8 | `main` step 2: `sceSifIopReset("rom0:UDNL rom0:OSDCNF")`, spin in `sceSifIopSync` | IOP boot #2 complete (`SMFLAG & 0x40000`) | 0.5–1.2 s | [V] wait; [I] 39 modules (638 KB of IRX incl. OSDSND 174 KB, XMCMAN 80 KB, XCDVDMAN 60 KB, XPADMAN 45 KB); XCDVDMAN/XSIO2MAN init talks to the mechacon/SIO2 |
| 9 | Library binds (`sceCdInit`, `sceMcInit`, `sceMtapInit`, `sceRmInit`, `scePadInit`, `sceFsInit`, `SndRpcBind`) | servers registered | < 10 ms total | [V] loops; [I] succeed first time after 8 |
| 10 | `TryMcSystemUpdate`: `sceMcGetInfo` on ports 0 and 1, directory lookups | XMCMAN mounts the card (superblock, FAT) | 0.1–0.5 s with a card in port 0; ≈ 20–50 ms per empty port | [I] SIO2 memory-card page reads ≈ 0.3–1 ms each; 8 MB card FAT ≈ 64 pages |
| 11 | `LoadAssetArchives`: 5 ROM files, ≈ 1.7 MB, through XFILEIO; LZ decode ≈ 1.1 MB on the EE | all reads return (blocking RPCs) | 0.4–1.0 s | [I] fileio over SIF ≈ 2–4 MB/s; decoder ≈ 20 MB/s. **Largest single EE-side phase** |
| 12 | `FontInit` | — | < 20 ms | [I] |
| 13 | `SoundInit`: 7 × (64 KiB SIF DMA + SPU2 DMA + wait) + 10 small DMAs + ≈ 25 RPCs | cmd 0x5007 after each chunk | 0.15–0.4 s | [I] SPU2 DMA ≈ 1–2 MB/s; RPC round trip ≈ 0.2–0.5 ms |
| 14 | CDVD S-commands (`sceCdBootCertify`, `sceCdForbidDVDP` loop, `sceCdReadWakeUpTime` loop, NVRAM read for `LoadNvramConfig`, `cdSCmd_fno30` loop) | mechacon answers | 20–100 ms | [I] each S-cmd ≈ 1–5 ms; NVRAM read of the config block (≥ 0x15 bytes) in 2-byte words |
| 15 | `WaitDev9Loader` | XDEV9 + XDEV9SERV loaded by the helper thread (started at step 6, runs on the IOP in parallel with 11–13) | usually 0 | [V] wait; [I] on a slim without the expansion bay XDEV9 fails fast → state 3, no wait |
| 16 | `HistoryLoadAtBoot`: `sceMcGetInfo` + open/read/close on port 0 (card already mounted) | mcserv | 10–50 ms | [I] |
| 17 | `ResetGraphics`, module registry, `InitModules`, semaphores, 6 threads, `CdPlayerStart`, `DiscThreadStart` | — | < 10 ms | [V] no waits |
| 18 | `VideoInit`: `sceGsResetGraph(PAL)`, double buffer with clear | — | ≤ 1 field (`sceGsSyncV_`) | [V]; from here the TV has sync and shows the cleared (black) buffer |
| 19 | main loop → `WakeupThread(OpeningThread)` | — | 0 | [V] |
| 20 | `OpeningInit`: texture conversion + GS upload (≈ 300 KB), tower geometry from the history table, VU1 program, `WaitVSyncBegin` | next VBlank | 1–3 fields (20–60 ms at 50 Hz) | [I] CPU work; [V] the VBlank wait |
| 21 | Frames 0–1 black (`FadeToBlack`, `g_frame < 2`), towers from frame 1 or 2 | `EndFrameFlip` per frame | 1–2 fields | [V] |

**Sum (cold boot, one memory card, no disc): ≈ 2.0–4.5 s of black screen** before the first
tower frame, dominated by the two IOP boots (4 + 8 ≈ 0.8–2.0 s) and the asset load (11 ≈
0.4–1.0 s). **[I]** — this matches the commonly observed "about three seconds" on a slim
console but has not been checked against a capture or an emulator log. For the app a default
of ≈ 3 s, with the GS "sync but black" state from step 18 (≈ 0.1–0.2 s before the first frame),
is the best current guess.

What does *not* gate the first frame **[V]**:

* the disc: `DiscThread` runs from step 17 on, and `TimelineStep` only consults the disc state
  to decide when the dive (stage 2) may begin (≥ 2 s into the animation, `opening.md` §3);
* the sound: `OpeningStartSound` only queues a command; the IOP plays it when the flush thread
  gets to it (first VBlank inside `OpeningInit`). If OSDSND were missing the queue would still
  be flushed (RPCs would fail) and the picture would be unaffected;
* the XDEV9 thread is waited for at step 15, i.e. before the opening, but it finishes long
  before that on this hardware [I].

### 4.1 After the first frame (for completeness, from `opening.md` §6)

Stage 1 lasts ≥ 2 s and until the disc state has settled (100 = no disc is immediate once
XCDVDMAN reports an empty tray; a disc needs the drive to spin up and identify it, typically
2–5 s on hardware [I], at most ≈ 10.2 s of drift); then the 2.1 s dive. Fastest total: 4.1 s
of animation. With PAL timing the integrator uses dt = 1.2 per 50 Hz frame, so wall-clock
times are the same as NTSC.

---

## 5. Where OSDSYS waits (for reproducing the black period) **[V]**

In program order, from `main` entry to the first frame. "spin" = busy loop on the EE, "RPC" =
`sceSifCallRpc` in blocking mode (the calling thread sleeps on a semaphore until the IOP's
reply arrives), "sema" = `WaitSema`.

| Where | Kind | Waits for |
|---|---|---|
| `main` step 2 `sceSifIopSync` loop | spin | `SMFLAG & 0x40000` — IOP boot #2 finished (EESYNC) |
| `sceCdInit` 25AC38, `sceMcInit` 25E298, `scePadInit` 257140, `sceMtapInit` 2722D0, `sceRmInit` 25F400, `sceFsInit` 26FEC0, `LoadFileBind` 256BA8, `SndRpcBind` 26DD58 | spin + retry (`sceSifBindRpc` until the server id answers; delay loops of 0xFFFF/0x10000/0xFFFFF iterations between tries) | the corresponding IOP module's RPC server |
| `TryMcSystemUpdate` 209378 | RPC (`sceMcGetInfo`/`sceMcSync`, `sceMcOpen` …) | XMCMAN card mount + directory reads |
| `LoadAssetArchives` 206E58 | RPC (`sceOpen`, `sceLseek`, `sceRead`, `sceClose` per file) | XFILEIO reading rom0: files (ROMDRV) |
| `SoundUploadBody` 200130 | `sceSifSetDma` + `sceSifDmaStat` poll per chunk; RPC `0x501A`, `0x5007` | SIF DMA, SPU2 DMA completion |
| `SoundInit` 2004B8 | ≈ 25 blocking RPCs | OSDSND |
| `sceCdBootCertify`, `sceCdForbidDVDP` (loop while `status & 0x80`), `sceCdReadWakeUpTime` (loop until non-zero), `cdSCmd_fno30` (loop), `LoadNvramConfig` | RPC (S-command server, `cdSCmdBegin` semaphore 2AE4F4) | XCDVDMAN ↔ mechacon |
| `LoadFileGetIopAddr` 256EE8 ×2 (loop until 0) | RPC | XLOADFILE |
| `WaitDev9Loader` 209B10 → `LoaderStateWait` 2096E8 | sema (27C6C4) | `Dev9LoaderThread` reaching state 2 or 3 |
| `HistoryLoadAtBoot` 204858 | RPC | XMCSERV |
| `VideoInit` → `sceGsSyncV_` 26EA88 | spin on GS CSR | field boundary [I] |
| main loop `WaitSema(sema_moduleDone)` | sema | the opening thread signalling at the end of the animation |
| `OpeningInit` → `WaitVSyncBegin` 207AD8 | sema (`sema_vsyncDone`, signalled by `VblankStartHandler` through `sema_vsyncRequest`) | next VBLANK_START |
| every frame `EndFrameFlip` 207B80 | sema (`sema_flipRequest` → `VblankStartHandler` → `FlipThread` → `sema_vsyncDone`) | next VBLANK_START |
| `SoundFlushThread` 209220 (separate thread) | sema (`sema_soundFlush`, once per VBlank from `InputThread`) then blocking RPCs | OSDSND accepting the play command |

Threads that exist when the first frame is drawn (priorities): `FlipThread` 1, `InputThread` 3,
`SoundFlushThread` 3, `ConfigWorkerThread` 4, `DiscThread` 5, `OpeningThread` 6 (running),
`ClockThread`/`BrowserThread` 6 (asleep), `CdPlayerThread` 0x20, `main` 0x1E (in `WaitSema`).

---

## 6. Verified vs. inferred, and what is not determined

Verified from code: the whole EE call chain (§2), the two IOPBTCONF lists and what EESYNC does
(§3), the order of every blocking call in `main` and the opening thread (§2.4, §2.5, §5), the
sizes of everything that is loaded (ROMDIR), the "first frames black" rule and the fact that
the play command is queued before the first VBlank wait.

Inferred: every duration in §4; IOPBOOT/UDNL/LOADCORE internals (only their strings and the
EESYNC module were read); that `SIFMAN` sets `SMFLAG & 0x10000`; what `0x1F402005 & 0x10`
means in EELOAD; the SDK identity of the `sce*` wrappers (RPC server ids and function numbers
were read, the names are by analogy); that XDEV9 fails quickly on this model.

Not determined:

* Any real timing. The useful measurements would be: SMFLAG bit 0x40000 timestamps for both
  IOP boots, the duration of `LoadAssetArchives`, and the VBlank count at the first non-black
  frame (`g_vblankCount` 27C654 is only started at step 13, so a probe would have to use the
  COP0 Count register or an emulator trace).
* Whether `rom0:XDEV9` loads successfully on an SCPH-70004 (no expansion bay) — only the
  branch structure was read.
* The CDVD call `cdSCmd_fno30` (`25DAA8`) and `sceCdStreamInit` (`25D728`): S-command numbers
  are known, SDK names are guesses.
* The RESET-resident CDVD helpers (`rom:FUN_bfc00ea0` …): no caller found.
