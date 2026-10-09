# OSDSYS hand-overs and environment checks across five ROMs

Subject: `rom0:OSDSYS` (the EE-side on-screen display) in

| tag | ROM | ROMVER | OSDSYS packaging | image used |
|---|---|---|---|---|
| **1.00** | SCPH-10000 | `0100JC20000117` | plain ELF, one PT_LOAD at `0x200000` (0x8D1EC bytes) + three LZ-packed sub-programs `MOPEN`/`MCLOCK`/`MBROWS` | scratch `osdsys_100_200000.bin` |
| **1.50** | DTL-H30101 (devkit) | `0150AD20001228` | stub + LZ (stream at file 0xE00) | `analysis/devkit/osdsys_dev_200000.bin` |
| **1.60A** | SCPH-39001 | `0160AC20020207` | stub + LZ | `analysis/devkit/osdsys_ret160_200000.bin` |
| **1.60E** | SCPH-30004R | `0160EC20011004` | stub + LZ; **the LZ stream is byte-identical to 1.60A's, only the stub differs (98 bytes), and that stub hot-patches the unpacked code — §6.1** | same image as 1.60A |
| **2.00** | SCPH-70004 | `0200EC20040614` | stub + LZ | `extracted/osdsys_200000.bin` |

Conventions as in `osdsys_flow.md`: **[V]** read in the Ghidra decompilation / disassembly of the
named build (addresses are virtual, in the unpacked image), **[I]** inferred. `ctx` = the OSD
context block: `0x1C0000` in 1.00, `0x1F0000` in 1.50/1.60/2.00. "1.60" without a letter means
the shared 1.60 image. Decompilations: 2.00 from `analysis/osdsys_named/`; 1.00/1.50/1.60 and the
1.00 sub-programs were decompiled for this note with the same headless runner
(`analysis/hidden/run_ghidra.sh` style, projects deleted afterwards); the per-build entry data
used were: 1.00 bss `0x28D200–0x4702B0`, gp `0x295170`, main `0x204D68`; 1.50 bss
`0x2A7700–0x3F0A30`, gp `0x2AE870`, main `0x2075B8`; 1.60 bss `0x2A7F00–0x3F32B0`, gp
`0x2AF070`, main `0x207478`; MOPEN/MCLOCK/MBROWS load at `0x500000`/`0x600000`/`0x7A0000`
(entry = load + 8, main `0x500488`/`0x600628`/`0x7AD9A8`). Existing notes
(`osdsys_flow.md`, `menu_survey.md`, `hidden_features.md` §2, `devkit_survey.md` §2.3) are not
repeated; they cover the 2.00 flow in depth and are referenced where a claim is theirs.

---

## 0. The 1.00 OSD is a different program

Everything from 1.50 on is one monolithic OSDSYS with the opening/clock/browser as threads.
1.00 is a **loader plus plug-in modules** [V, `main` `0x204D68`]:

1. Loads IOP modules `rom0:CLEARSPU`, `SIO2MAN`, `MCMAN`, `MCSERV`, `PADMAN`, `OSDSND`
   (`sceSifLoadModule` `0x2092E8`); there is **no IOP reset**, no `UDNL`, no `OSDCNF`.
2. Memory-card system-update check (§2.1) — first thing after the IOP modules, unconditional.
3. Reads the NVRAM config (§4), applies S/PDIF and `SetGsVParam`.
4. Registers module *descriptors* with `0x204838(path, slot)`: `rom0:OSOPEN` (slot 1),
   `rom0:OSCLOCK` (2), `rom0:OSBROWS` (3), then — unless the OSD was restarted with
   `BootError BOOT_ROM_MODULE` or `BootError <path containing osdsys.elf>` —
   **`mc1:/BIEXEC-SYSTEM/OSBROWS` and `mc0:/BIEXEC-SYSTEM/OSBROWS`** (slot 3 again), then
   `rom0:OSFONTM` (10), `rom0:OSFONTS` (11).
   A descriptor is a 19/20-byte text file: `"<version>\n<module name>\n<hex load address>\n"`
   (`rom0:OSOPEN` = `100\nMOPEN\n00500000\n`, `OSCLOCK` = `100\nMCLOCK\n00600000`, `OSBROWS`
   = `100\nMBROWS\n007a0000`, `OSFONTM` = `100\nFONTM\n01d00000`). `0x204838` parses the first
   line with `atoi` and **replaces the slot only if the new version is greater** than what is
   registered (`0x41EF7C + slot*0x414`); the module file name is resolved relative to the
   descriptor's directory (`:` for `rom`/`host` prefixes, `/` otherwise — `"host"` at `0x28B720`
   is only used here, nothing passes a `host:` path).
5. `0x204AF0` then loads every registered slot: if the module path contains both `MBROWS` and
   `mc` it goes through the LOADFILE RPC **fno 5 with section name `"all"`** (`0x209460` →
   `0x209328`), i.e. the MagicGate/KELF loader; everything else is read raw into `0x1000000`.
   The image is LZ-decompressed (`0x200D78`, same scheme as `osd_unpack.py`) to the descriptor's
   load address and, unless the name contains `FONT`, a thread (prio 5, 128 KB stack) is created
   at `load + 8`.

So on a SCPH-10000 a PS2 memory card carrying `BIEXEC-SYSTEM/OSBROWS` (version > 100) and a
KELF `BIEXEC-SYSTEM/MBROWS` replaces the browser at every boot, with the card choosing the load
address [V]. This is the ancestor of the `osdNNN.elf` update hook and is additional to it.

The modules call back into the loader through `ctx`: MOPEN's decision (`0x500488`), MCLOCK's
disc watcher (`0x6002F8`) and MBROWS's exit (`0x7AD840`/`0x7AD8F0`) write `ctx[0x114]`
(launch code) and `ctx[0x8F80]` (next module) exactly like the 2.00 `ctx[0x14]`/`ctx[0x5E8]`
protocol. The loader's dispatcher `0x203310` has one extra case, **`ctx[0x114] == −2` →
`LoadExecPS2(ctx + 0x10, 0, NULL)`**, a generic "run this path" launch [V]; no ROM module
writes −2 (MBROWS writes only −1/0..3, MCLOCK 0..5) [V], so it exists for a replacement
module [I].

---

## 1. argv and launch modes

### 1.1 Who starts OSDSYS with what

* Kernel: every one of the five KERNELs contains `"EELOAD"`, `"rom0:OSDSYS"`, `"BootBrowser"`
  [V strings]; the 2.00 mechanics (`ExecOSD` = `KLoadExec("rom0:OSDSYS", argc, argv)`,
  `ExitToBrowser` = `rom0:OSDSYS BootBrowser`) are in `osdsys_flow.md` §1.
* EELOAD: 1.00 has only `rom0:OSDSYS`, `BootError`, `moduleload` (no `TESTMODE`, no
  `moduleload2`, no `UDNL` — the 1.00 chain never resets the IOP) [V strings]. 1.50 and
  **1.60E share a byte-identical EELOAD** (`BootError` only); 1.60A and 2.00 add `BootIllegal`
  [V `cmp`]. So the `BootIllegal` warning scene exists in the 1.60 OSD image but on a
  SCPH-30004R nothing produces the argument.

### 1.2 Arguments OSDSYS compares against

| string | 1.00 | 1.50 | 1.60 | 2.00 | effect |
|---|---|---|---|---|---|
| `BootError <path>` (argc ≥ 3, argv[1]) | V | V | V | V | browser; if `<path>` contains `dvdplayer.elf` / `DVDVIDEO.ELF` (1.00) or `dvdplayer.elf` / `DVDELF` (others) → "DVD Player is not set up." (1.00 copies the Japanese or English literal by language; later builds string `0x60`/`0x61`) |
| `BOOT_ROM_MODULE` | V | V | V | V | the `<path>` OSDSYS itself passes when it cannot start (`FatalBootError`: argc 0, bad argv[0] prefix); 1.00 also uses it to suppress the card-browser load (§0) |
| `BootClock` | V | V | V | V | clock / main menu |
| `BootBrowser` | V | V | V | V | browser |
| `BootOpening` | V | V | V | V | opening |
| `BootWarning` | — | — | V | V | warning scene |
| `BootIllegal` | — | — | V | V | force disc state `0x74` + warning scene |
| `Initialize` | — | V | V | V | sets a flag (`0x26F670` / `0x26FDF0` / `0x27C658`) that the config worker thread tests **at its start** to rewrite the NVRAM block with defaults, and that `main` tests before parsing argv. In 1.60/2.00 the compare sits inside the `memcmp(arg,"Boot",4)==0` branch and can never match; in 1.50 it is reachable (only argv[1] is examined) but the flag is set after both readers have run (`ChangeThreadPriority(main, 0x1E)` at `0x207B80` precedes the parse, so the prio-4 worker has already started). Dead in all three [V code, I scheduling]. |
| `SkipMc` | — | — | V | V | skip the memory-card update probe |
| `SkipHdd` | — | — | V | V | skip the `ATAD`/`HDDLOAD` probe |
| `SkipForbid` | — | — | — | V | skip `sceCdForbidDVDP` |
| any other `Boot…`, anything not starting with `Skip` | 1.00: other → browser | 1.50: other → browser | V | V | browser |

Parsing differences [V]: 1.00 and 1.50 look at **argv[1] only**; 1.60 and 2.00 scan all of
argv[1..] (last `Boot…` wins). argv[0] is used from 1.50 on to pick the IOP reset string
(`rom…:` → `rom0:UDNL rom0:OSDCNF`; `mc…:` → `<dir>/osdmain.irx` with flag `0x100`; anything
else → `BootError BOOT_ROM_MODULE`); 1.00 ignores argv[0]. 2.00 additionally derives the
directory for the HDD/flash helper modules from argv[0]: `rom…:` → `rom0:<name>`, `mc…:` →
`<dir of argv[0]>/<name>`, **anything else → `host0:../modules/<name>`** (`LoadIopModuleFromRom`
`0x209748`, probe `0x209888`) — a TOOL/host leftover that is unreachable because a non-`rom`/
`mc` argv[0] already died in `FatalBootError` [V].

---

## 2. External files OSDSYS looks for

Region letter `?` in `B?EXEC-SYSTEM` / `B?EXEC-DVDPLAYER` / `B?DATA-SYSTEM`: 1.00 hard-codes `I`
(`/BIEXEC-…`, `/BIDATA-SYSTEM`); 1.50/1.60/2.00 patch `/BREXEC-…` from `rom0:ROMVER` byte 4
(`J`→`I`, `A`/`H`→`A`, `E`→`E`, 2.00 adds `C`→`C`) [V, `osdsys_flow.md` §5.2 for 2.00; 1.50
`0x203FC0`, 1.60 `0x204238`].

### 2.1 Memory-card system update (every ROM, every boot)

| ROM | file(s), port 0 then 1, slot 0 | gate | launch |
|---|---|---|---|
| 1.00 | `/BIEXEC-SYSTEM/osdsys.elf` | `sceMcOpen` succeeds (no card-type / format check) [V `0x2049E0`] | `LoadExecPS2("moduleload", 4, {"-m rom0:SIO2MAN","-m rom0:MCMAN","-m rom0:MCSERV","-x mc%d:/BIEXEC-SYSTEM/osdsys.elf"})` — the OSD's own argv is **not** forwarded |
| 1.50 | `%s/osd160.elf`, `%s/osdmain.elf` | PS2 card, formatted, file opens (`hidden_features.md` §2.2) | `moduleload -m SIO2MAN -m MCMAN -m MCSERV -x mc%d:%s/%s` + OSD argv[1..]; no `SkipMc` |
| 1.60 | `%s/osd170.elf`, `%s/osdmain.elf` (`0x207200`) | same, unless `SkipMc` | same form `-x mc%d:%s` |
| 2.00 | `%s/osd210.elf`, `%s/osdmain.elf` (`0x209378`) | same, unless `SkipMc` | same |

The executable is loaded by EELOAD's `-x` = LOADFILE fno 5 (MechaCon KELF decrypt); failure
returns to the OSD as `BootError <path>` (1.60A/2.00: `BootIllegal` on −2). 1.00 has the extra
browser-module replacement of §0.

### 2.2 DVD player

| ROM | ROM player | memory-card player | condition for the card player | launch |
|---|---|---|---|---|
| 1.00 | **none** (no `rom1:` string anywhere) | `mc0:/BIEXEC-DVDPLAYER/dvdplayer.elf`, then `mc1:` (`0x202DB0`) | file opens; **no version/region check** | `moduleload -m rom0:SIO2MAN -m rom0:MCMAN -m rom0:MCSERV -x mc%d:/BIEXEC-DVDPLAYER/dvdplayer.elf` (`0x202CD0`). `dvdplayer-j.ver` / `dvdplayer-e.ver` are read only for the Version page (`0x201670`, Shift-JIS aware). |
| 1.50 / 1.60 | `rom1:DVDID` (= `"N.NN<letter>"`) and `rom1:DVDVER` read at boot (`0x204FC8`); player = `moduleload2 rom1:UDNL rom1:DVDCNF` with `-k rom1:EROMDRV`, `-m erom0:UDFIO`, `-x erom0:DVDELF` | `B?EXEC-DVDPLAYER/dvdplayer.elf` | `0x2049A0`: read `B?EXEC-DVDPLAYER/dvdplayer.id` and `dvdplayer-e.ver` (≤ 31 bytes each) from port 0/1; parse id as `major.minor<letter>`; accept iff `letter == rom1 letter` (or rom1 letter == −2, which is never set — a missing `rom1:DVDID` leaves −1 and **disables card players**), `(major,minor) > rom1 (major,minor)`, and `dvdplayer-e.ver` is neither `"1.00\n"` nor `"1.01\n"`; then the `.elf` must open. First card wins. | card: `moduleload -m rom0:SIO2MAN -m rom0:MCSERV -x mc%d:%s/dvdplayer.elf` (`0x202600`); ROM: as left. Both require the CDVD disc-type register `0x1F40200F == 0xFE` at launch time, else `ExecOSD("BootBrowser")` [V `0x2026E0`, `0x20274C`] — **removed by the 1.60E stub patch (§6.1)**. |
| 2.00 | `rom1:DVDID?` / `rom1:DVDVER?` with `?` = the console's region-parameter letter (`sceCdReadRegionParams`), falling back to `rom1:DVDID`; the 70004 rom1 holds `DVDID{A,C,E,J,M,O,R,U}` = `3.10x` and `EROMDRV{A..U}` [V rom1 listing]. Launch args patched the same way: `-k rom1:EROMDRV<letter>` and the `-x erom0:DVDELF` string gets byte 12 ← `'P'` and byte 14 ← letter (`0x202E00`), falling back to `EROMDRV`/`DVDELF` when no region letter. | same as 1.60 (`0x2061E8`) | same rules | same; disc register must be `0xFE` **or `0xFC`** |

### 2.3 HDD / flash boot

| ROM | ROM contents | what OSDSYS does |
|---|---|---|
| 1.00 | — | nothing |
| 1.50, 1.60A, 1.60E | `ATAD`, `HDDLOAD`, `HDDOSD` (all three byte-identical across the three ROMs) | `main` (1.50 `0x20774C` unconditional; 1.60 `0x207660` unless `SkipHdd`): `sceSifLoadModule("rom0:ATAD")`; if that succeeds, `sceSifLoadModule("rom0:HDDLOAD", "-osd 0x100000 -stat %#x")` and, if *that* succeeds, HDD boot is enabled. `HDDLOAD` ("HDD_Loader", 00/07/17, imports `atad`, `secrman`, `cdvdman`) reads **sector 0** of the drive, unlocks the ATA password, reads the boot image, has SECRMAN decrypt it (`HDL:securiyt decript.`, `cannot secur data`) into EE `0x100000` and posts a status word [V strings/imports]. The OSD polls that status for up to 50/60 × 20 frames ≈ 20 s (1.50 in the disc-state function `0x206B98`; 1.60 in the opening timeline `0x215FD0`) and, once ready, launches with `ExecPS2(0x100000, gp 0, {"rom0:HDDBOOT", OSD argv[1..]})` after an IOP reset (`0x202D48`). No `hdd0:__mbr`/`__system`/`osdmain.elf` string exists in any ROM — the file-system-level hand-over lives in the sector-0 KELF (the HDD's MBR program), not in the ROM [V]. |
| 2.00 | `XDEV9`, `XDEV9SERV`; no `HDDLOAD`/`ATAD` | code for `ATAD`/`XFLASH`/`XFROMMAN`/`HDDLOAD` probing, the `xfrom:XFROMBOOT` argv and HDDLOAD RPC commands that send `xfrom:%s/osd210.elf` + `xfrom:%s/osdmain.elf` (`0x208BB0`, part of `HddBootStateMachine`) is present but **dead**: `rom0:XDEV9` exists, so the probe is skipped and `HddBootEnabled()` stays 0 (`osdsys_flow.md` §2.1 step 6). The `xfrom:` strings are the PSX/DESR flash-OSD path sharing this code base [I]. |

`rom0:HDDOSD` (1.50/1.60 only) is itself a stub+LZ EE program (stream at file `0xD80`, 0x36AA0
bytes at `0x100000`): an older, trimmed OSDSYS that looks for `%s/osd120.elf`, loads
`osd110.irx`/`rom0:ATAD`/`rom0:HDDLOAD`, and falls back to `rom0:OSDSYS` [V strings]. **No ROM
module references the name `HDDOSD`** (only ROMDIR) — an orphan, presumably started by the HDD
MBR program or an HDD utility disc [I].

### 2.4 Other paths

| path | ROMs | reader / purpose |
|---|---|---|
| `cdrom0:\SYSTEM.CNF;1` | all | §3 |
| `cdrom0:\PSXMYST\MYST.CCS;1` → `SLPS_000.24`, `cdrom0:\CDROM\LASTPHOT\ALL_C.NBN;1` → `SLPS_000.65`, `cdrom0:\PSX.EXE;1` → `"???"` | all | PS1 title fallbacks when `SYSTEM.CNF` is absent (`osdsys_flow.md` §2.5; 1.00 `0x202FF0`) |
| `B?DATA-SYSTEM/history`, `icon.sys`, `history.old` | all | play history (1.00 `0x203828`; later as in `osdsys_flow.md` §5) |
| `rom0:ROMVER` | 1.50+ | region letter, `sceCdBootCertify` block (`devkit_survey.md` §2.3). 1.00 never reads it. |
| `rom0:OSDVER` | 2.00 | finer console region (`osdsys_flow.md` §3) |
| `rom0:PS1ID`, `rom0:PS1VER?`/`rom0:PS1VER` | 2.00 | PS1 driver version line; `?` = region letter (ROM has `PS1VERA/C/E/H/J`) (`0x205F00`) |
| `rom1:DVDID`, `rom1:DVDVER` (+`?` in 2.00) | 1.50+ | §2.2 |
| `rom0:ADDROM2` | 2.00 | loaded when `ConsoleRegion()==3` (China); the asset table then uses the `GB18030` font instead of `FONTM`, `FNTASCI2` instead of `FNTASCII`, `TEXOPNGM` instead of `TEXOPNGE` with the source-device word changed (`rom2:` string at `0x2C39B8`) [V `0x206E58`, I that the word selects `rom2:`]. Regions 4/5/6 (`R`/`K`/`H`) swap `TEXOPNGE` for `TEXOPNGW`. |
| `host0:../modules/` | 2.00 | §1.2, unreachable |
| `xfrom:%s/osd210.elf`, `xfrom:%s/osdmain.elf`, `xfrom:XFROMBOOT` | 2.00 | §2.3, dead |
| `mc?:/BIEXEC-SYSTEM/OSBROWS` (+ `MBROWS`) | 1.00 | §0 |

---

## 3. SYSTEM.CNF keys

Only three keys are ever parsed, identically in all five builds [V grep + code]:

| key | reader | use |
|---|---|---|
| `BOOT2` | PS2 launcher (1.00 `0x202A38`, 1.60 `0x2022A0`, 2.00 `0x202AB0`) | value after `=`; the file name after the last `:`/`\` must equal the first 10 characters of the disc ID read through `sceCdReadKey` (`0x202228` in 1.60), otherwise `ExecOSD("BootBrowser")`; the value becomes argv[0] of `rom0:PS2LOGO` |
| `BOOT` | PS1 launcher | file name between last `\`/`:` and `;` → PS1 title ID (argv[0] of `rom0:PS1DRV`) |
| `VER` | PS1 launcher | argv[1] of `rom0:PS1DRV` (second line) |

`VMODE`, `HDDUNITPOWER`, `PARAM2` etc. do not occur in any OSDSYS image; whatever reads them is
downstream (PS2LOGO/game) and outside this note. The file is read with at most 1023 bytes
(1.00: 2048 for the PS1 path) [V].

---

## 4. Config / NVRAM bits and special inputs

### 4.1 The OSD config block (config area 1, 2 × 16 bytes)

All builds read it with `sceCdOpenConfig(1,0,2)` / `ReadConfig` / `CloseConfig` (S-cmd RPC
fno `0x0E`/`0x10`/`0x0F`, mapped by the companion cdvdfsv survey). Bytes 0–14 are copied aside
as the **per-driver option nibbles**, byte `0x0F` onward is the OSD config proper.

| byte.bits | 1.00 (`0x203580`) | 1.50/1.60/2.00 (`UnpackConfig` 2.00 `0x203A08`, 1.50 `0x202D08`) | consumer |
|---|---|---|---|
| `0x00` | → `ps1drvConfig` byte in `SetOsdConfigParam` (`0x203CC0`) | `& 0x11` → `ctx[0x1224]` → `ps1drvConfig` bits 5–12 (bit 0 Disc Speed Fast, bit 4 Texture Mapping Smooth) [V bits, I meaning] | kernel → PS1DRV |
| `0x00..0x0E` nibbles | — | "Options" pages: option *k* of a module lives in byte *k/2*, low nibble (even) / high nibble (odd), 3 bits (`0x2257A8`, `0x225340`) | §4.3 |
| `0x0F.0` | S/PDIF | S/PDIF | `0x1031` sound command at boot, `SetOsdConfigParam` |
| `0x0F.1-2` | screen type 0/1/2 | same (4:3 / Full / 16:9) | `SetOsdConfigParam`, opening letterbox |
| `0x0F.3` | video out RGB/YPbPr | same | `SetGsVParam` (1.00) / `SetOsdConfigParam` |
| `0x0F.4` | **language 0 = Japanese / 1 = English** (the only language bit in 1.00) | old 1-bit language, used only when `0x0F.5-7 == 0` | string tables, `dvdplayer-j/-e.ver` |
| `0x0F.5-7` | — | "new layout" marker | — |
| `0x10.0-4` | — | language 0..11, validated against region | — |
| `0x11.0-2`+`0x12` | — | 11-bit timezone offset | `SetOsdConfigParam` bits 21–31 |
| `0x11.3` / `0x11.4` / `0x11.5-6` | — | daylight-saving / 12-24 h / date format → `OsdConfigParam2` byte 1 bits 4/5/6-7 (`0x208660`) | kernel |
| **`0x11.7`** | — | **"OSD initialised"**: clear → `SetupRequired()` → first-boot wizard after the opening (`hidden_features.md` §5.3). 1.00 has no such bit and no wizard. | clock module |
| `0x13.0`+`0x14` | — | 9-bit field → config word 0 bits 20–28 (not consumed by anything found) | — |
| `0x13.4` | — | 2.00: "progressive set by DVD player" → the DVD Player options getter `0x206800` returns the **Clear Progressive Setting** page only when this bit is set [V]; choosing Yes sets `ctx[0x13C0]`, which makes `LaunchDvdVideo` clear the bit and write the block instead of launching (`0x202E00`) [V] | 2.00 |
| `0x13.5` | — | 2.00: value id `0x13` (`0x2044A8`) — gates whether the **Remote Control** item is listed in System Configuration (`0x223758`, `menu_survey.md` §1.1) [V] | 2.00 |
| `0x13.6` | — | 2.00: **Remote Control On/Off** (`CfgWord1Bit3` `0x204428`); at boot `main` sends S-cmd fno `0x30` = `sceCdRcBypassCtl(!bit)` and the config page sends it on change (`0x225900`, worker) [V fno, V name from cdvdfsv] | 2.00 (1.50/1.60 cdvdfsv has no fno 0x30) |
| `0x13.7` | — | value id `0x11` (`0x204468`), consumer not found | — |

Region/language sources: 1.00 none (Japan hard-coded, two languages); 1.50/1.60 `rom0:ROMVER`
byte 4 (`J`/`A`/`H`/`E`; no `C`) plus the CDVD region parameters are **not** read (cdvdfsv 1.50/1.60
has no fno `0x43`); 2.00 adds `rom0:OSDVER`, `sceCdReadRegionParams` (fno `0x43`) and the China
font/texture swaps of §2.4. The ○/× swap outside Japan/China is in the input thread of 1.50+
(`osdsys_flow.md` §2.3).

### 4.2 Special inputs

* **No "skip the opening" button exists** in any build: the opening threads (2.00 `0x2162C0`
  region, 1.60/1.50 equivalents, 1.00 `MOPEN`) never read the pad fields, and the input threads
  (`0x208DB8`, 1.60 `0x206E00`, 1.00 `0x204308`) only merge pad/remote data and the eject key
  [V: no reader of `ctx[0xC58..]` / `ctx+0x95D8` outside the clock/browser code].
* **Nothing is sampled "held at boot"** except in 2.00, where a wake-up reason 2/3 with specific
  IOP flag bits forces the browser or powers the console off (`osdsys_flow.md` §2.1 step 9).
* △ from the main menu opens **Version Information** (1.00: MCLOCK, 1.50+: `menu_survey.md`
  §1.1); the **screen-size reset combo** (hold LEFT+□+L1+L2+R1+R2 = `0x808F` for 120 frames,
  release, START) exists in 1.50 (`0x23F8A8`), 1.60 (`0x240178`) and 2.00 (`0x2482B8`); it is
  not in 1.00's MBROWS/MCLOCK [V grep for `0x808f`].

### 4.3 The Version Information "Options" pages

Each registered module may expose an options string `"name,val0,val1\n…"`; the page editor
(`0x225C30` in 2.00, `0x605008` in 1.00's MCLOCK) assigns each option a *value id*:

| module | options string | value id → storage | effect |
|---|---|---|---|
| Console (`Unit` in 1.00) | `Diagnosis,Off,On` (all five builds, every language) | id 4 = `ctx[0x1234]` (RAM only, never saved) | On → `ctx[0x13B8]=1`, Off → 0; the disc thread then issues **S-cmd fno `0x19` = `sceCdAutoAdjustCtrl(On ? 0 : 1)`** (2.00 `0x20F9F8`; 1.00 worker cmds `0x17`/`0x18` → `0x20E300`) [V fno; name V from the cdvdfsv dispatcher]. I.e. "Diagnosis On" *disables* the drive's auto-adjust. |
| PlayStation Driver | `Disc Speed,…\nTexture Mapping,…` (`Play Options`) | ids `0x16`,`0x17` → config byte 0 low/high nibble → `ps1drvConfig` | saved to NVRAM by worker cmd `0x12` |
| DVD Player | 2.00 only: `Clear Progressive Setting,No,Yes` (shown when `0x13.4` set) | id `0x15` = `ctx[0x13C0]` | §4.1 |
| MAC Address | 2.00 only, module #7 (`0x206C28`): shows the Ethernet MAC obtained through the `XDEV9SERV` loader result (`0x27C6B0`; `--:--:--:--:--:--` when absent), Chinese layout variant | — | display only |

---

## 5. Disc-type dispatch

The CDVD disc-type register `0x1F40200F` is folded into `ctx` disc states (`osdsys_flow.md`
§4.1 for 2.00). Differences by build [V: 1.00 `0x204080`, 1.50 `0x206B98`, 1.60 `0x20B9F8`,
2.00 `0x20F678`]:

| register | state | OSD action (opening decision / clock) | notes |
|---|---|---|---|
| `0x00` | 100 | no disc → clock/menu | 1.50+: clears a sticky `0x74` |
| `0x01`–`0x04` | `0x65`–`0x68` | "detecting", wait | |
| `0x05` | `0x69` | unknown disc → clock/menu ("could not be read" in browser) | |
| `0x10`/`0x11` | `0x6A`/`0x6B` | launch PS1 (`rom0:PS1DRV`) | 1.60 `LaunchPs1Disc` re-reads the register and bails to the browser unless `0x10`/`0x11` (`0x2025C4`) — **patched out in 1.60E** |
| `0x12`/`0x13`/`0x14` | `0x6C`/`0x6D`/`0x6E` | launch PS2 (`rom0:PS2LOGO`) | 1.60+: verified with `sceCdReadKey` in the disc thread (`ReadKey failed`, result 3 → `0x74`); 1.00/1.50 verify only in the launcher |
| `0x20`/`0x21`/`0x22` | `0x6F`/`0x70`/`0x71` | launch codes 5/4 (→ browser) / `0x71` → CD player (1.60 clock) | unidentified classes, never launched |
| `0xFC` | 2.00: `0x75` (= DVD-Video); ≤ 1.60: `0x65` (detecting forever) | 2.00 DVD player | 2.00-only (DVD-VR [I]) |
| `0xFD` | `0x72` | audio CD → CD player if tracks found | |
| `0xFE` | `0x73` | DVD-Video → DVD player (§2.2) | 1.50+: launch rechecks the register |
| `0xFF` | `0x74` | illegal → warning scene | 1.50+: sticky until the disc is removed; 1.50 Japan units (`RomRegion()==0` → `ctx[0x0C]=1`, `main` `0x2078EC`) keep it **forever** — 2.00 stores `ctx[0x0C]=0` |
| anything else | `0x65` | | |

Launch-time checks that can still bounce back to the browser: PS2 — `SYSTEM.CNF` opens, has
`BOOT2`, and the path matches the key-derived disc ID; PS1 — register `0x10`/`0x11` (1.50+);
DVD — register `0xFE`(/`0xFC`) and a player found. The 1.00 PS2 launcher reads the disc key block once
(`0x202848` → `0x20EBB0`) and has no illegal-disc concept.

---

## 6. Surprises, leftovers and region branches

### 6.1 SCPH-30004R hot-patches its own OSDSYS at load time [V]

The 1.60E `OSDSYS` file differs from 1.60A's in 98 bytes, all inside the loader stub at file
`0x230`–`0x2A3` (stub vaddr `0x1001B0`–`0x100218`). 1.60A has plain syscall stubs there; 1.60E
replaces the `ExecPS2` (syscall 7) wrapper — which the stub calls once, at `0x100108`, to start
the unpacked program — with code that first stores eight words into the decompressed image:

| image addr | original (1.60A) | 1.60E value | where | effect |
|---|---|---|---|---|
| `0x2022F4` | `bne s1,2` | `b` | `LaunchPs2Disc` after `Ps2DiscVerifyAndGetId` | "not ready" (2) result ignored |
| `0x20230C` | `bnel s1,3` | `b` | same | **"ReadKey failed / illegal" (3) result ignored** — no `0x74`, no warning |
| `0x20237C` | `jal printf("can't open 'SYSTEM.CNF'")` | `jal 0x202600` (`LaunchDvdVideo`) | PS2 launcher, open failed | a PS2-typed disc without `SYSTEM.CNF` is handed to the DVD player before falling to the browser |
| `0x202460` | `jal CnfNextLine` | `jal 0x202550` (`LaunchPs1Disc`) | the `BOOT2` search loop | a `SYSTEM.CNF` whose first line is not `BOOT2` is launched as a **PS1 disc** |
| `0x2024E4` | `jal ExecOsdBootBrowser(1)` | `nop` | after `BootPathMatchesDiscId` failed | **BOOT2/disc-ID mismatch no longer rejects the disc** |
| `0x2025C4` | `bnez v1` (register is `0x10`/`0x11`) | `b` | `LaunchPs1Disc` | PS1 disc-type re-check removed |
| `0x2026E0`, `0x20274C` | `beq v1,0xFE` | `b` | `LaunchDvdVideo` (card and ROM player) | DVD-Video re-check removed |

So on a SCPH-30004R the OSD will launch a disc the MechaCon could not verify, launch a disc whose
`BOOT2` does not match its ID, and tries the DVD player / PS1 driver as fallbacks — the common
1.60 image is strict, the 2001-10 European ROM deliberately loosens it. Reason not determined;
the fallback chain reads like a workaround for drives that mis-report disc types [I]. Any tool
that classifies behaviour by "OSDSYS version" must treat 1.60E separately from 1.60A.

### 6.2 Other findings

* **1.00's card-replaceable browser and generic launch** (§0): descriptor-driven module loading
  from `mc?:/BIEXEC-SYSTEM/OSBROWS`, KELF-loaded, card-chosen load address; launch code −2 =
  `LoadExecPS2(any path)` with no ROM writer.
* **Debug prints left in every build**: `ExecutePs2GameDisk`, `can't open/read 'SYSTEM.CNF'`,
  `ReadKey failed` (1.60+), 1.00 additionally `Program name = %s`, `title search timeout!`,
  `WARRNING : Already Initialize`, the libcdvd `Libcdvd bind err …`/`call cdread cmd` set,
  `sceCdCbfunc= %d`, and the `TTY:` DECI2 client — all through `write(1)` → DECI2 TTY, invisible
  without a host (`hidden_features.md` §1.6).
* **`"host"` in 1.00** (descriptor path syntax) and **`host0:../modules/` in 2.00** (helper
  module directory) are the only host-filesystem traces; neither is reachable.
* **`rom0:HDDOSD`** orphan in 1.50/1.60 (§2.3).
* **`Initialize` argv** (§1.2) is dead code in three builds.
* **Diagnosis** is `sceCdAutoAdjustCtrl` (§4.3); **Remote Control** is `sceCdRcBypassCtl`
  (2.00 only; the 1.50/1.60 cdvdfsv has neither fno `0x30` nor WakeUpTime `0x29` nor
  RegionParams `0x43`, so those 2.00 calls would fail on an older IOP side).
* **China** (`ConsoleRegion()==3`, 2.00): `rom0:ADDROM2`, `GB18030` font, `FNTASCI2`,
  `TEXOPNGM`, a different `icon.sys` template and an extra `_SCE8` icon file in the history
  folder (`osdsys_flow.md` §5.2), the MAC line in a different layout. Regions `R`/`K`/`H`
  get `TEXOPNGW`.
* **Japan**: 1.50 makes the illegal state permanent (§5); the ○/× swap is skipped for J/C in
  1.50+; 1.00 is Japan-only by construction (folder letter `I`, 2 languages, `SCPH-10000`
  literal on the Version page).
* **1.00 has no first-boot wizard, no ROMVER read, no `sceCdBootCertify`, no `ForbidDVDP`**,
  and never resets the IOP.
* **1.60E ships the 1.50 EELOAD** (no `BootIllegal`) with the 1.60 OSDSYS.

---

## 7. Cross-ROM comparison

| feature | 1.00 J | 1.50 dev | 1.60 A | 1.60 E | 2.00 E |
|---|---|---|---|---|---|
| OSD architecture | loader + `MOPEN`/`MCLOCK`/`MBROWS` plug-ins | monolithic | monolithic | monolithic (same image as 1.60A + 8 stub patches) | monolithic |
| argv understood | `BootError`, `BootClock`, `BootBrowser`, `BootOpening` (argv[1] only) | + `Initialize` (dead) | + `BootWarning`, `BootIllegal`, `SkipMc`, `SkipHdd` (all argv) | same OSD; EELOAD never emits `BootIllegal` | + `SkipForbid` |
| IOP reset by OSD | no | `rom0:UDNL rom0:OSDCNF` / `osdmain.irx` | same | same | same |
| MC system update | `/BIEXEC-SYSTEM/osdsys.elf` (open only) + card `OSBROWS`/`MBROWS` module | `osd160.elf`/`osdmain.elf` | `osd170.elf`/`osdmain.elf` | same | `osd210.elf`/`osdmain.elf` (+ dead `xfrom:` variants) |
| DVD player | card only, no checks | ROM (`rom1`) + card if newer, same letter | same | same, but disc-type recheck removed | same + region-letter file names, `0xFC` accepted |
| HDD boot | — | live (`ATAD`→`HDDLOAD`→`ExecPS2(0x100000,"rom0:HDDBOOT")`), no skip | live, `SkipHdd` | live, `SkipHdd` | code present, dead (XDEV9) |
| SYSTEM.CNF keys | `BOOT2`, `BOOT`, `VER` | same | same | same | same |
| disc verify (`sceCdReadKey`) | launcher only | launcher only | disc thread + launcher | result ignored in launcher | disc thread + launcher |
| BOOT2 ↔ disc-ID check | yes | yes | yes | **disabled** | yes |
| config bits | `0x0F` only (spdif/screen/video/jpn-eng) | full layout incl. initialised bit | same | same | + remote control, progressive flag |
| first-boot wizard | no | yes | yes | yes | yes |
| region source | fixed Japan | ROMVER byte 4 | ROMVER | ROMVER | ROMVER + OSDVER + RegionParams |
| Version page options | Diagnosis, Play Options | same | same | same | + Clear Progressive Setting, Remote Control, MAC Address |
| screen-size reset combo | no | yes | yes | yes | yes |
| opening skip button | no | no | no | no | no |
| `BootError` DVD message | `dvdplayer.elf` / `DVDVIDEO.ELF` | `dvdplayer.elf` / `DVDELF` | same | same | same |
| `host` traces | descriptor syntax | — | — | — | `host0:../modules/` |

---

## 8. Tool-worthy findings (ranked)

1. **The ROM-version-specific update file name and gate** — show which of
   `osdsys.elf` / `osd160.elf` / `osd170.elf` / `osd210.elf` / `osdmain.elf` the user's ROM looks
   for in `B?EXEC-SYSTEM` (with the region letter derived from their ROMVER) and whether their
   card passes the PS2-type/formatted/opens gate; plus the 1.00-only `OSBROWS` descriptor
   replacement. Directly explains FreeMCBoot-style behaviour per BIOS.
2. **1.60E's eight stub patches** — a tool reading the SCPH-30004R OSDSYS must apply them before
   analysing or emulating launch behaviour; they change whether an unverified or mismatched
   disc boots. Detectable from the stub bytes at file `0x230`.
3. **DVD-player override rules** — given `rom1:DVDID` (or its absence) and a card's
   `dvdplayer.id`/`dvdplayer-e.ver`, predict whether the OSD takes the card player, the ROM
   player, or shows "DVD Player is not set up."; 1.00 takes any card `dvdplayer.elf`.
4. **The NVRAM config decoder** — bit map of bytes `0x0F`–`0x14` (language, screen, video,
   timezone, clock format, initialised bit, remote-control, progressive flag) and the option
   nibbles in bytes `0x00`–`0x0E`, with the per-version subset; tells the user whether their NVM
   will trigger the first-boot wizard, which menu items appear, and what `sceCdRcBypassCtl` /
   `sceCdAutoAdjustCtrl` the OSD will send.
5. **HDD-boot reachability** — `ATAD` + `HDDLOAD` present and probe not skipped (1.50/1.60) vs
   dead (2.00) vs absent (1.00); what the sector-0 KELF receives (`rom0:HDDBOOT` + argv).
6. **Disc-type → action table with per-version exceptions** (`0xFC`, sticky `0xFF`, Japan 1.50
   permanent illegal, 1.60E relaxations).
7. **argv vocabulary per ROM** (which `Boot…`/`Skip…` words a launcher may pass, and that
   `Initialize` does nothing).
8. **China/region asset swaps and the `host0:`/`xfrom:` leftovers** — mostly of documentary
   interest.

## 9. Could not determine

* Why SCPH-30004R patches its OSDSYS (§6.1) and whether other 1.60E boards (30003/30002) carry
  the same stub.
* The exact EROM entry names the 2.00 `-x erom0:DVD…` argument resolves to (the EROM is
  encrypted; the code produces `DVDPL<letter>` from the string `DVDELF`).
* Who starts `rom0:HDDOSD`, and the format/semantics of the sector-0 image `HDDLOAD` decrypts
  (the MBR program is on the drive, not in ROM).
* Config bits `0x13.0`+`0x14` (9-bit field) and `0x13.7` — unpacked but no consumer found.
* The meaning of disc-type register values `0x20`–`0x22` and of `0xFC` beyond "treated as
  DVD-Video".
* 1.00: the exact Version-page layout rendered by MCLOCK and whether MBROWS exposes any way to
  reach launch code −2 (only ROM modules were read; a card module could).
* Whether the 1.50 devkit's `D` in the certify block (`devkit_survey.md`) changes any OSD path —
  no OSD code reads the byte back after `sceCdBootCertify`.
* The kernel side (`ExecOSD` callers) for 1.00/1.50/1.60 was checked by strings only; the
  2.00 reading in `osdsys_flow.md` §1 is assumed to hold.
