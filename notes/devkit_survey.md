# Survey: the DTL-H30101 development-kit BIOS (ROM 1.50 A, `0150AD20001228`)

Subject: `DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0` (4 MB) plus the `.nvm` and
`.inf` next to it. Reference ROMs for diffing: SCPH-39001 (`0160AC20020207`, retail 1.60 A,
the closest generation available), SCPH-10000 (1.00 J) and SCPH-70004 (2.00 E, the one the
rest of these notes describe). **No retail 1.50 ROM was available**, so wherever the devkit
differs from 1.60 the difference may be a 1.50→1.60 change rather than a devkit change; the
text says so each time.

Conventions as in the other notes: **[V]** verified by reading the data / disassembly,
**[I]** inferred. Addresses: `rom:` = ROM image (`0xBFC00000`), `k:` = KERNEL
(`0x80000000`), `el:` = EELOAD (`0x82000`), no prefix = unpacked OSDSYS (`0x200000`).

Method: `tools/romdir.py` splits, `tools/extinfo.py` (new) dumps the EXTINFO records,
`tools/osd_unpack.py` unpacks OSDSYS, `tools/r5900dis.py` gives linear disassemblies,
`tools/syscall_table.py` (new) finds and compares the kernel syscall tables. **Ghidra was not
run** (the machine ran out of disk during this survey), so everything below comes from
byte comparison, strings and linear disassembly; the per-module listings are in
`analysis/devkit/` (`romdir_*.txt`, `extinfo_*.txt`, `*.dis`, `syscalls_dev_vs_160.txt`,
`rom_dev/`, `rom_ret160/`, `rom_ret100/`, `rom_ret200/`).

---

## 0. What this unit is

* The dump's own README calls it a **"fat" PlayStation 2 TEST kit (DTL-H30101)**, and the
  `.inf` written by the dumper reports `MEMsize=32MB` for the EE (and 2 MB IOP, DEV9 rev
  0x30, two i.LINK ports). **[V from the dump's metadata]** So this is the *TEST* (debugging
  station) family, not the DTL-T10000 *TOOL*: retail-like hardware with a DEX MechaCon, 32 MB
  of RDRAM, no 128 MB and no on-board DECI2 Ethernet/host file system. The 128 MB / `host:`
  expectations in the task brief do not apply to this ROM (see §3.3 and §5).
* ROMVER `0150AD20001228`: version 1.50, region `A`, type **`D`** (retail = `C`), built
  2000-12-28. VERSTR `System ROM Version 5.0 12/28/00 A`. **[V]**
* The ROMDIR's EXTINFO comment (`tools/extinfo.py`) says the image was *assembled* on
  2003-05-20 from `PS20150AD20001228.conf` → `PS20150AD20001228+100.bin` in
  `kuma@rom-server/~/sdex/…` — "sdex" being the DEX build tree; the retail 1.60 comment is
  `…/g30k/g/app/rom`. The IOP config archives (`OSDCNF`, `EELOADCNF`) carry their own
  comments: built 2000-12-28 by `aki@aki-linux`. **[V]**

---

## 1. Module inventory (question 1)

Both ROMs have 92 ROMDIR entries. Full tables: `analysis/devkit/romdir_dev.txt`,
`romdir_ret160.txt`; dates/versions: `extinfo_dev.txt`, `extinfo_ret160.txt`.

### 1.1 Entries only in the devkit / only in retail 1.60 **[V]**

| only in devkit | size | note |
|---|---|---|
| `RDRAM1` | 0x2E64 | second RDRAM-init variant, at ROM 0x44000 |
| `RDRAM2` | 0x2F64 | third variant, at ROM 0x47000 |

| only in retail 1.60 | size | note |
|---|---|---|
| `ROMGSCRT` | 0x3898 | 1.60's externalised `SetGsCrt` tables |
| `TSIO2MAN`, `TPADMAN` | 0x207D, 0xA01D | pad/SIO2 drivers used by 1.60's `TESTMODE` |

**None of these is a devkit feature.** `RDRAM`/`RDRAM1`/`RDRAM2` are also present in retail
1.00 J (`romdir_ret100.txt`), and nothing jumps to the extra two: `ee_reset` calls exactly
`0x9FC41000` (`rom:BFC00874`–`BFC00884`), the `RDRAM` module only references its own data
(`0x9FC43xxx`), and `RDRAM1`/`RDRAM2` are linked for `0xBFC4xxxx` (uncached) and contain no
cross-references (`analysis/devkit/rdram*_dev.dis`). They are unused alternates of the same
"Initialize memory / Total accessable memory size" routine. `ROMGSCRT`/`T*MAN` are 1.60
additions. No `host`/`hostfs`, `DECI2`/`dsidb`, `TTY`, `sbin`-variant, debugger or extra
`eeload` module exists in the devkit ROMDIR; `SBIN` is byte-identical to retail. **[V]**

### 1.2 Byte-identity of shared modules **[V]**

Of the 90 shared entries, **69 are byte-identical** to retail 1.60 A (every IOP kernel
module, `IOPBOOT`, `IOPBTCONF`/`IOPBTCON2`, `TBIN`, `KROM`/`KROMG`, `SBIN`, `MCMAN`,
`PADMAN`, `CDVDMAN`, `UDNL`, `PS1DRV`, all five asset archives `FONTM`/`FNTIMAGE`/`SNDIMAGE`/
`TEXIMAGE`/`ICOIMAGE`, `OSDSND`, `HDDOSD`, `HDDLOAD`, all `X*` modules except `XCDVDMAN`/
`XLOADFILE`, …) even though their EXTINFO dates differ (dates are repackaging dates).
The ones that differ:

| module | dev / 1.60 size | what differs | devkit-specific? |
|---|---|---|---|
| `RESET` | same | 3 bytes = BCD date at ROM 0x100 (`28 12 00` vs `07 02 02`) | no, code identical |
| `ROMVER`, `VERSTR`, `EXTINFO`, `ROMDIR` | — | version/date text, offsets | no |
| `RDRAM` | 0x2E74 / 0x3014 | older routine (no CPU-clock measurement prologue); same strings | no (1.00 J-era) |
| `KERNEL` | 0x161D8 / 0x16C48 | build string `May 11 2000` vs `Feb  7 2002`; 1.60 adds a `ROMVER` reader and a bigger `SetGsCrt`; same syscall map (§3) | **no** |
| `EELOAD` | 0xF630 / 0xF210 | 1.60 adds the `BootIllegal` path; otherwise same strings/structure (§2) | no |
| `EELOADCNF`, `OSDCNF` | 428/412, 487/487 | only the build comment; module lists identical | no |
| `OSDSYS` | 0x4C460 / 0x4C944 | 1.50 vs 1.60 OSD (§2) | no |
| `SECRMAN` | 0x44B1 / 0x44E1 | module name **`secrman_for_dex`** vs `secrman_for_cex`; retail has one extra step string `card auth key change` | **yes** |
| `XCDVDMAN` | 0xC9B5 / 0xD3A5 | dev has debug prints `MECACON_V00010800_mae/ikou`, `MECACON_V00020200_mae/ikou`; 1.60 adds `sce_dev5;1` handling | mixed [I] |
| `XLOADFILE` | 0x2A09 / 0x2B99 | 1.60 adds `cdrom0:sce_dev5` | no |
| `EECONF` | 0xE91 / 0xF41 | dev copy is byte-identical to 1.00 J's | no |
| `LIBSD` | `PsIIlibsd 1511` vs `2070` | library version | no |
| `TESTMODE`, `TESTSPU` | 0x16A38/0x1DFB0, 0x4655/0x693F | older factory-test program (dev strings: `RDRAM Test failed!`, `DTV1080I`, `VESA3C`, loads `rom0:XSIO2MAN`/`XPADMAN`; 1.60's loads `TSIO2MAN`/`TPADMAN`) | no |
| `ATAD` | same size | 351 bytes differ (not examined) | unknown |
| `LOGO`, `PS2LOGO` | 0x14748/0x14694, 0x42244/0x34CC4 | compressed payloads, not unpacked here | unknown |

So, from the ROM contents alone, **the devkit ROM is the ordinary 1.50 A ROM set with the
DEX variant of SECRMAN and the `D` letter in ROMVER.** Everything else that differs from the
1.60 reference lines up with known 1.50→1.60 changes.

---

## 2. Boot path (question 2)

### 2.1 RESET → RDRAM → KERNEL **[V]**

Identical code to retail (`RESET` differs by the date only). `ee_reset` → `rom:rdram_init`
(`0xBFC41000`) → result stored at `0x70003FF0` → `k:_kernel_entry`. The RDRAM module's entry
(`BFC41000`–`BFC4114C`): calls the real init (`BFC41150`) with a packed config word, prints
`# failed to initialize memory: InitRDRAM returned %ld` on error, otherwise formats
`# Total accessable memory size: %d MB (%c:%d:%d:%d) (%d:%d:%04x)` with
`size_MB = (result >> 32) & 0x7FFF` and **returns `size_MB << 20`** (`BFC4113C`). The kernel
stores that in `k:GetMemorySize`'s variable (`k:80001000`, same as 2.00). So the memory size
is *detected*, not configured per ROM — see §3.3.

### 2.2 EELOAD **[V]**

`el:main` is at `0x82350` (2.00 E: `0x82388`); same structure as the retail one in
`boot_sequence.md` §2.3: `argc ≥ 2` → `moduleload`/`moduleload2 ` prefix test, IOP reset with
`rom0:UDNL rom0:EELOADCNF`, `-m`/`-k`/`-x` items, load, `BootError` on failure (there is no
`BootIllegal`/−2 case in 1.50 — the string does not exist in the module); `argc < 2` →
`WaitIopReady`, then `if (*(byte*)(0x1F402005) & 0x10) load rom0:TESTMODE else rom0:OSDSYS`
(`0x82534`–`0x825B4`; CDVD pointer `0x90280 = 0x1F402000`, path table `0x90288 =
{"rom0:TESTMODE","rom0:OSDSYS"}`). String set of the module: `rom0:OSDSYS`, `rom0:TESTMODE`,
`BootError`, `moduleload`, `moduleload2 `, `rom0:UDNL rom0:EELOADCNF`, `rom0:UDNL ` — **no
`host:`, `mc0:` or other devkit boot target.**

### 2.3 OSDSYS **[V unless noted]**

* Unpacked with `osd_unpack.py` (stream at ELF offset 0xE00 works for this build too):
  0x4B4AF → 0xA76B4 bytes at `0x200000` (1.60: 0xA7EB4). Entry/stub layout as in 2.00.
* Readable-string diff against 1.60 (`analysis/devkit/osdsys_*_strings.txt`) is tiny:
  devkit-only `%s/osd160.elf` (the memory-card update file it looks for; 1.60 looks for
  `osd170.elf`) and `Libcdvd bind err %d CD_Init %d`; retail-1.60-only `BootIllegal`,
  `BootWarning`, `SkipMc`, `SkipHdd`, `ReadKey failed`. Hence the 1.50 argument vocabulary is
  `BootError`, `BootClock`, `BootBrowser`, `BootOpening`, `Initialize`, `BOOT_ROM_MODULE`
  (strings at `0x2A32E8`–`0x2A33F0`), plus the `rom…:`/`mc…:` argv[0] prefix test and the
  `-x mc%d:%s` system-update launch. **No `host`, `TESTMODE`, `Dev`/`DEX` text anywhere in
  the OSD; the "Sony Computer Entertainment" / warning text and the language tables are the
  same data as retail** (the asset archives are byte-identical, and the OSD strings differ
  only as listed). There is no developer overlay.
* The one place the OSD looks at the `D`: `RomRegionRaw` (`0x203FC0`, 2.00 E: `0x205310`)
  reads `rom0:ROMVER`, maps byte 4 (`A`/`H`→1, `E`→2, `J`→0; no China case in 1.50) and
  stores **byte 5 (`C`/`D`/`T`) at `0x26E564`**, with a getter at `0x2040C8`. The getter's
  only caller is `main` (`0x207890`): it builds the certify block {major, minor, region
  letter from the table `"JAE"` at `0x2A3390`, **byte 5**} and passes it to
  `sceCdBootCertify` (`0x25B1C8`, `0x2078AC`). In 2.00 E the same byte is stored at
  `0x27B3E0` and never read back (`analysis/osdsys_named/data_xrefs.tsv`), the certify call
  getting its letters from the ROMVER text directly. **[I]** the MechaCon compares this type
  letter with its own (CEX/DEX) identity; a retail MechaCon fed `D` would refuse — this is
  the only ROM-side "I am a devkit" statement.
* `TTY:` strings (`TTY: send err %d`, `TTY: receive error`, …) are the SDK's DECI2 TTY client
  in `libdeci2`; **they are present in retail OSDSYS too** (4 of them in 1.60 and 2.00 E).
  Not devkit-specific.
* Not verified: a function-level diff of `main` against a retail 1.50 (unavailable). The
  1.60 `main` differences (Skip*, BootIllegal/Warning) are documented 1.60 features.

### 2.4 IOP side **[V]**

`IOPBTCONF` (first boot) is byte-identical to retail; the lists inside `EELOADCNF` and
`OSDCNF` are identical to 1.60's (`boot_sequence.md` §3.1/§3.2 apply unchanged). The only
IOP module that is DEX-specific is `SECRMAN` ("secrman_for_dex", same exports/strings minus
`card auth key change`). **[I]** it carries the DEX MagicGate key set so memory cards and
`-k`/`-x` encrypted loads authenticate against a DEX MechaCon; the ability of a TEST to boot
unsigned/CD-R discs lives in the MechaCon firmware, not in the ROM.

---

## 3. Kernel (question 3)

### 3.1 Syscall map **[V]**

`tools/syscall_table.py` finds the 128-entry table at `k:800144C0` (dev; 1.60: `80014D00`,
2.00 E: `80014F40`). Both have **101 distinct handlers in the same slots**; after
normalising relocations the only handlers whose code differs are `SetGsCrt` (0x02) and the
`Get/SetGsH/VParam` family (0x4C/0x4E/0x4F) — the 1.60 `ROMGSCRT` change — and
`SetAlarm`/`ReleaseAlarm`/`SetEventFlag`/`GetCop0` by constant offsets only
(`syscalls_dev_vs_160.txt`). No extra syscall, no debug-only entry.

* `_print` (0x75) is `jr ra` in all three kernels (`k:800074D0` dev).
* `Deci2Call` (0x7C) is **not** in the table (that slot is the "undefined syscall" printer
  `k:80001564`); the dispatcher special-cases 0x7C before the table lookup (`k:80000288`,
  same in 1.60) and jumps to `k:80013784` → `k:8000E250`, a 16-way switch (sub-functions
  1..16, jump table `k:80015810`) implementing Deci2Open/Close/ReqSend/Poll/ExRecv/…
  Identical mechanism in retail. **The DECI2 manager ("EE DECI2 Manager version 0.06",
  protocol handlers for 0x201 DCMP, 0x21F, 0x230) is part of every PS2 kernel; without a
  host adapter it simply never receives anything.**
* `MachineType` (0x7E) returns the ROM word at `0xBFC001F8`, which is **0 in all four ROMs**
  (devkit included); `PSMode` ORs 0x8000 into it. The devkit does not mark itself there.

### 3.2 Kernel TTY output **[V]**

`k:kprintf` (`k:800073E8` dev) → formatter → putchar pointer `k:80014ED8` → `k:80006E58`
(`\n` → `\r\n`) → `k:80012C30`: waits while `SIO_ISR (0x1000F130) & 0xF000 == 0x8000` (TX
FIFO full) and writes the byte to **`0x1000F180` (EE SIO TXFIFO)**. The retail 1.60 kernel
has the identical chain (`k:80006E58` → `k:80013488`), so the kernel's boot log goes to the
EE serial port on *every* console; a devkit merely has the port wired to a connector. The
messages a devkit owner sees on that port during a cold boot (all present in the dev KERNEL,
strings at `KERNEL+0x149D3…0x15DB9`):

```
# TLB spad=0 kernel=1:%d default=%d:%d extended=%d:%d      (TlbInit)
# Initialize Start.  # Initialize GS ...  # Initialize INTC ...  # Initialize TIMER ...
# Initialize DMAC/VU1/VIF1/GIF/VU0/VIF0/IPU ...  # Initialize FPU ...
# Initialize User Memory ...  # Initialize Scratch Pad ...  # Initialize Done.
EE DECI2 Manager version 0.06
  CPUID=%x, BoardID=%x, ROMGEN=%04x-%04x, %dM                 (Deci2Init banner; %dM = GetMemorySize)
# Restart. / # Restart Done. / # Restart Without Memory Clear [Done].   (every LoadExecPS2 / ExecOSD)
# panic ! dir not found / # panic ! '%s' not found              (CreateRomThread, KLoadExec)
# Syscall: undefined (%d), # INTC(%d) Handler does not exist., # <Thread> No active threads, …
# EE DECI2 Panic!!!  DCMP: MakeError code=%d
```

The RDRAM module prints before the kernel, through its own routine: `# Initialize memory
(rev:%d.%02d, ctm:%dMhz, cpuclk:%dMhz %s)` and `# Total accessable memory size: %d MB …`
(`RDRAM+0x2C40`, `+0x2CD0`). **[I]** that routine also targets the EE SIO (not traced).

### 3.3 Memory size **[V]**

No 32/128 MB constant exists in RESET, RDRAM or KERNEL: `GetMemorySize` returns whatever
the RDRAM init detected (§2.1); `UserMemoryClear`, `SetupThread` (default stack top =
`GetMemorySize() − 0x1000`), `TlbInit` and the DECI2 banner all derive from it. For this
DTL-H30101 that is 32 MB (dump `.inf`). A TOOL's 128 MB would be reported by the same code
if the RDRAM init accepts the configuration; whether this exact RDRAM routine does was not
determined (the three RDRAM variants in the ROM are the only candidates, and only the first
is called).

---

## 4. NVRAM / configuration (question 4)

The `.nvm` next to the ROM is a **real 1 KiB dump** (`DTL-H30101_…_[6A001573].nvm`): it
carries MechaCon calibration tables at 0x000–0x08F, region parameters at 0x180
(`30 30 30 14 67 11 2C 01 05 28 00 79` — `"000"` + bytes), the model string **`DTL-H30101`**
at 0x1A0, data blocks at 0x1C0–0x1FF (console/i.LINK ids **[I]**), 0x280/0x290 flags, and
the OSD configuration at 0x310. The PCSX2-made `scph39001.NVM` is blank except 0x310.
**[V for the bytes, I for the field names]**

OSD config: 1.50's `ReadConfigBlock` (`0x203008`) does `sceCdOpenConfig(1, 0, 2)` →
`sceCdReadConfig` → `sceCdCloseConfig` exactly like 2.00 E (`ReadConfigBlock`
`0x203DA8`), i.e. config area 1, 2 × 16 bytes, which PCSX2 maps to NVM 0x300–0x31F for ROMs
before 1.70. `UnpackConfig` (`0x202D08`) is the same bit layout as 2.00's (`0x203A08`):
language in byte 0x0F bit 4 (old 1-bit form, when byte 0x0F ≫ 5 == 0) or byte 0x10 & 0x1F,
"OSD initialised" = bit 7 of byte 0x11, etc. In the devkit NVM: bytes 0x30F–0x314 =
`00 3D 21 AE 20 00`:

| field | value | meaning |
|---|---|---|
| byte 0x0F | 0x00 | old-format language bit = 0 (→ region default English after validation) [I] |
| byte 0x11 | 0x21 | **bit 7 clear → "OSD not initialised"**, so after the opening the clock/setup module runs (`osdsys_flow.md` §2.4) [V for the bit, I for the consequence on this build] |
| 0x31F | 0x9F | checksum-ish trailer [I] |

No "developer"/"DEX" flag is read from NVRAM by OSDSYS, EELOAD or KERNEL (the only ROM-type
input is ROMVER byte 5, §2.3). Region/language defaults come from `rom0:ROMVER` (`A`) and
the CDVD region parameters exactly as in retail 1.60 (neither 1.50 nor 1.60 has a
`rom0:OSDVER` file or string; that is a 2.00-era addition). The PCSX2 blank for SCPH-39001 contains
`30 21 81 0E …` at 0x310, i.e. the same "not initialised" bit — so neither file, as shipped,
skips the first-boot set-up.

---

## 5. What a research tool could expose (question 5), prioritised

1. **The serial/TTY boot log** — the list in §3.2 in the order the kernel emits it
   (`TlbInit` → `HardwareInit` → `Deci2Init` banner with `CPUID/BoardID/ROMGEN/xxM` →
   `CreateRomThread("EELOAD")`), preceded by the RDRAM module's two lines and followed by
   `# Restart.` at each `LoadExecPS2`. It is real output the console produces, on retail too;
   on a devkit it is visible. Evidence: `k:80012C30` (SIO TXFIFO), string table
   `KERNEL+0x149D3…`, `RDRAM+0x2C40`.
2. **The `D` handshake** — `rom0:ROMVER` byte 5 → `0x26E564` → `sceCdBootCertify({1, 50,
   'A', 'D'})` at `0x2078AC`. Showing the certify block the OSD sends, and that a retail
   console sends `C`, is the single ROM-level devkit difference in the boot chain.
3. **Memory size as detected** — `# Total accessable memory size: %d MB`, `GetMemorySize`,
   and the derived values (user-memory clear range, default stack top, TLB extended pages).
   For this unit 32 MB; the tool should display the detected figure rather than assume 128.
4. **Module inventory diff** (§1) — a side-by-side ROMDIR with "identical / differs / only
   here" colouring, EXTINFO build dates/versions, and the build comments
   (`sdex` tree, `aki@aki-linux`, 2003-05-20 re-assembly). Evidence: `analysis/devkit/`.
5. **DEX SECRMAN** — module name string and the missing `card auth key change` step
   (`SECRMAN` strings). Low priority: nothing user-visible at boot.
6. **DECI2 hooks** — `Deci2Call` dispatch (`k:80000288` → `k:8000E250`, 16 sub-functions)
   and the three registered protocols. Same on retail; interesting only as a "what the
   host could attach to" diagram.
7. **Not available on this ROM** (and therefore not worth planning around): `host:` boot,
   `hostfs`, `dsidb`/`drfp`, a developer overlay or different opening text, 128 MB.

---

## 6. Findings → confidence → evidence

| # | finding | conf. | evidence |
|---|---|---|---|
| 1 | ROM is 1.50 A, type `D`, assembled 2003-05-20 from the `sdex` tree | V | `rom_dev/ROMVER`, `VERSTR`, `extinfo_dev.txt` line ROMDIR |
| 2 | DTL-H30101 is a TEST unit with 32 MB, not a 128 MB TOOL | V (dump metadata) / I (hardware class) | `DTL-H30101_…v4_[3BE9C9E5].inf`, README |
| 3 | 69/90 shared modules byte-identical to retail 1.60; all IOP kernel modules, assets, `SBIN`, `TBIN`, `IOPBTCONF` identical | V | `cmp` run, §1.2 |
| 4 | Only devkit-specific module content: `secrman_for_dex`; `D` in ROMVER | V | `SECRMAN` strings; `ROMVER` |
| 5 | `RDRAM1`/`RDRAM2` unreferenced (also in 1.00 J) | V | `reset_dev.dis` (`BFC00874`), `rdram*_dev.dis` |
| 6 | RESET code identical to retail (3 date bytes) | V | `cmp -l RESET` |
| 7 | EELOAD: same boot targets as retail, no host/mc path; 1.50 lacks `BootIllegal` | V | `eeload_dev.dis` `0x82350`–`0x825B4`, strings |
| 8 | OSDSYS: only `osd160.elf` and one message differ from 1.60 strings; no dev overlay/text | V (strings), I (no code-level diff vs retail 1.50) | `osdsys_*_strings.txt` |
| 9 | OSDSYS reads ROMVER byte 5 and passes it to `sceCdBootCertify` | V | `osdsys_dev.dis` `0x203FC0`, `0x2040C8`, `0x207890`–`0x2078AC` |
| 10 | Kernel syscall map identical (101 handlers); `_print` stub; `Deci2Call` special-cased; `MachineType` word = 0 | V | `syscalls_dev_vs_160.txt`, `kernel_dev.dis` `0x80000288`, `xxd 0x1F8` |
| 11 | kprintf → EE SIO `0x1000F180` in dev and retail | V | `kernel_dev.dis` `0x80012C30`, `kernel_ret160.dis` `0x80013488` |
| 12 | Memory size detected by RDRAM init and returned as bytes; no constant | V | `rdram_dev.dis` `0xBFC410D4`–`0xBFC4114C`, `k:80001000` |
| 13 | NVM: real dump, model `DTL-H30101` at 0x1A0, OSD config at 0x310 with "not initialised" bit clear | V (bytes) / I (PCSX2 layout mapping, field meanings) | `xxd` of `.nvm`, `0x203008`, `0x202D08` |
| 14 | TTY strings in OSDSYS are generic SDK code | V | present in 1.60 and 2.00 E OSDSYS |
| 15 | `XCDVDMAN` dev build has MECACON debug prints; 1.60 adds `sce_dev5` | V (strings) / I (meaning) | `XCDVDMAN` strings |
| 16 | `TESTMODE`/`TESTSPU`/`LIBSD`/`EECONF`/`ATAD`/`LOGO`/`PS2LOGO` differ by version only | I | sizes, versions, 1.00 J identity of `EECONF` |

Not determined: a function-level diff of OSDSYS/KERNEL against a *retail* 1.50 (no such ROM
here); what the 351 differing bytes of `ATAD` and the compressed `LOGO`/`PS2LOGO` payloads
change; whether the RDRAM routine would accept a 128 MB configuration; where the RDRAM
module's own prints go; the `v2` Rust helper mentioned in the brief is not present in the
tree (`v2/` does not exist), so the "assets load" check could not be repeated.
