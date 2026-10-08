# Hidden ROM content: what a console owner or PCSX2 user never sees

Second pass after `devkit_survey.md` (read that first; nothing from it is repeated here except
where this pass corrects it). Subjects: the DTL-H30101 devkit ROM (1.50 A, `D`), retail
SCPH-39001 (1.60 A) and SCPH-70004 (2.00 E), plus the three `.nvm` files next to them.

Conventions: **[V]** verified from the data / disassembly, **[I]** inferred. Address prefixes:
`tm:` = TESTMODE (EE, `0x100000`), `ts:` = TESTSPU (IOP IRX, relocatable from 0), `k:` =
KERNEL (`0x80000000`), none = unpacked OSDSYS (`0x200000`). "dev" = the devkit 1.50 build,
"1.60" = SCPH-39001, "2.00" = SCPH-70004.

Method: Ghidra 12.1 headless through a copy of the `ps2-bios-ghidra` runner with a raw-EE
target family (`analysis/hidden/run_ghidra.sh`: base/bss/gp/entries) and an IRX variant
(`run_ghidra_irx.sh`: Ghidra's ELF loader keeps the `.symtab` names that TESTSPU, SECRMAN and
XCDVDMAN carry). Exports in `analysis/hidden/<target>/c/`, `functions.tsv`, `data_xrefs.tsv`;
string/address tables in `analysis/hidden/*_straddr.tsv`. The 2.00 kernel/OSDSYS/EELOAD
exports from earlier passes (`analysis/kernel`, `analysis/osdsys_named`, `analysis/eeload`)
were reused and cross-checked against the devkit linear disassemblies in
`analysis/devkit/*.dis`. The Ghidra `work/` projects were deleted (disk).

## Reachability at a glance

| feature | reachable on a retail console? | reachable in PCSX2? | devkit-only? |
|---|---|---|---|
| `rom0:TESTMODE` factory test (§1) | **No** by normal means: EELOAD picks it only when CDVD reg `0x1F402005` bit 4 is set, which only the factory jig sets [V code / I who sets the bit] | **No** via the ROM path (PCSX2 never sets the bit). **Yes** by extracting the ELF and booting it: 1.50/1.60 are plain ELFs, 2.00 is a self-unpacking stub ELF; colour bar, pad menu, rumble, tone work, the MechaCon test-code channel does not (§1.6) | No: every ROM has it; the dev one is the older 1.50 build |
| `rom0:TESTSPU` tone / S/PDIF / MechaCon relay (§1.5) | Only as part of TESTMODE | Loads (normal IRX) but its MechaCon poll (S-cmd 0x90) never returns a code | No |
| Memory-card system update `B?EXEC-SYSTEM/osdNNN.elf` (§2) | **Yes** on every console: any PS2-formatted card whose file opens makes the OSD reboot into EELOAD `-x`; the only gate is the MechaCon decrypting the KELF | **Yes** (same path; PCSX2 emulates the KELF decrypt) | No; dev looks for `osd160.elf` and has no `SkipMc` |
| DECI2 manager + kernel debug stub (§3) | Present on every console, never fed: `Deci2ReqSend` to a host returns −10 without a link; every non-syscall exception parks the EE in the stub waiting for a host | Same code runs; the SIF2 (DMA ch 7) transport is never serviced | No (TOOLs add an IOP-side manager + host link) |
| Kernel `kprintf` boot log → EE SIO `0x1000F180` (§3.4) | Emitted on every console; visible only with the port wired out | **Yes**: PCSX2's "EE SIO" console captures `0x1000F180`, so the `# Initialize …` lines, the DECI2 banner and `# Restart.` appear | No |
| DECI2 panic / step printer → IOP-bus UART `0x1F80380C` (§3.3) | Device exists only on TOOLs [I]; code is in every kernel | Nothing attached; users see nothing | Code: no; hardware: TOOL |
| `MachineType` word `0xBFC001F8` (§3.5) | 0 on all four ROMs; syscall returns 0 | 0 | No |
| `secrman_for_dex`: no card command `0xF7` "key change" (§4.2) | n/a | n/a (PCSX2 emulates the card side generically) | **Yes** |
| XCDVDMAN MechaCon-version branches (§4.1) | Same tests in retail (silent); dev adds prints | Same | Prints only |
| `cdrom0:sce_dev5;1` raw-device handle (§4.3) | 1.60+ only; used by XLOADFILE | Yes | No |
| NVM fields (§5) | Real consoles have the full MechaCon map; PCSX2 files are blank except the OSD block | PCSX2 writes only the OSD config block | The dev NVM is a real dump of a DEX unit |
| `rom0:LOGO` = PS1-mode shell/libs (§6) | Runs whenever a PS1 disc boots (TBIN loads it) | Runs in PCSX2's PS1 mode too | No; dev is the older build without the "Licensed by" strings |

---

## 1. TESTMODE and TESTSPU: the factory test program

### 1.1 Format, load address, entry [V]

| ROM | TESTMODE | load | entry | main | $gp | .bss |
|---|---|---|---|---|---|---|
| dev 1.50 | plain ELF, 0x16A38 bytes, one PT_LOAD (file 0x1000, 0x1579E bytes, memsz 0x18938) | 0x100000 | 0x100008 | tm:1003C8 | 0x11D6F0 | 0x1157A0..0x118938 |
| 1.60 | plain ELF, 0x1DFB0 (file 0x1000, 0x1CD36 / memsz 0x2FECC) | 0x100000 | 0x100008 | tm:100938 | 0x124C70 | 0x11CD40..0x12FECC |
| 2.00 | stub + LZ: a 0xF014-byte ELF at 0x1000000 (entry 0x1000008; sections .text.Expand* = the OSDSYS decoder) whose .data at file 0xE00 is the stream; `osd_unpack.py --raw TESTMODE 0xE00` gives 0x1CFB6 bytes for 0x100000 | 0x100000 | 0x100008 | tm:100950 | 0x124EF0 | 0x11D000..0x130178 |

_start is the SDK crt0 (clear bss, SetupThread, SetupHeap, main). Library tags: dev
PsIIlibgraph1620 / libdma 1620 / libpad 1630 / libkernl1620; 1.60 and 2.00 ...1600 (an older
library set; the test program was not rebuilt against new libs). 1.60 and 2.00 have identical
readable strings and their main/menu/RPC decompile to the same control flow, so 2.00 = 1.60
recompiled and packed. [V]

TESTSPU is an IOP IRX (e_type 0xFF80, .iopmod, .symtab/.strtab, source testspu.c): dev 0x4655
(entry ts:588), 1.60 0x693F (ts:D34), 2.00 0x6E54 (ts:E2C). Imports: dev libsd intrman
loadcore sifcmd stdio thbase; 1.60/2.00 add cdvdman thsemap vblank (2.00 also modload). [V]

### 1.2 How it is reached [V]

el:main (survey 2.2) on a cold boot: `if (*(byte*)0x1F402005 & 0x10) LoadElfAll("rom0:TESTMODE")
else "rom0:OSDSYS"`. 0x1F402005 is the CDVD N-command status register; XCDVDMAN itself only
tests its bits 6/7 (ready). [I] bit 4 is the MechaCon "test mode" flag raised by the factory
jig that talks to the MechaCon directly (1.5 shows the MechaCon relaying jig commands). PCSX2
never sets it, so the branch is dead in the emulator; no OSD/kernel/IOP-ROM module writes it.

### 1.3 The 1.50 (devkit) TESTMODE: a fixed sequence driven by the circle button [V]

tm:main (1003C8), in order:

1. tm:100298 reads rom0:ROMVER (63 bytes): retail = (byte5=='C'); ntsc = !(byte4=='E'||byte4=='O').
2. Retail units only (retail==1): load rom0:ADDDRV, open rom1:ROMDIR (the DVD-player ROM); if
   the open fails print `can't read rom1:` and hang. On the devkit (D) this test is skipped.
3. tm:1000C0 loads rom0:LIBSD, rom0:XSIO2MAN, rom0:XPADMAN, rom0:TESTSPU (errors
   `loading libsd.irx failed`, `Can't load module sio2man/padman/main`). TESTSPU starts its
   1 kHz tone at once (1.5).
4. tm:100278 ResetGraph + libdma init; pad init tm:102070 (PadInit, PadPortOpen(0,0)).
5. Test A "AVE": tm:1008B0(2 or 3) sets NTSC 640x448 (mode 2) or PAL 640x512 (mode 3) via
   SetGsCrt, builds a double buffer and draws tm:100658: eight full-height vertical bars
   white, yellow, cyan, green, magenta, red, blue, grey (GS sprites, GS-RGB colours
   0xFFFFFF 0x00FFFF 0xFFFF00 0x00FF00 0xFF00FF 0x0000FF 0xFF0000 0x808080). Then tm:101DE8(1)
   M1 ON (small motor), (3) M2 ON (big motor 0xFF via PadSetActDirect), wait for CIRCLE pressed
   (tm:101D28, pad byte 3 bit 0x20, prints `Push Button`), SIF command 0 to TESTSPU = stop the
   tone (tm:100180), ResetGraph same mode, wait CIRCLE released (`Release Button`), M2 OFF.
6. Test B "DVE": tm:1008B0(0x82/0x83) = DVDNTSC 720x480 / DVDPAL 720x575, M2 ON, wait, re-set,
   wait release, M2 OFF.
7. Test C: tm:1008B0(0x51) = DTV1080I 1920x1080 interlaced, same button dance.
8. RDRAM test tm:100220 -> tm:102128: prints
   `Now checking RDRAM . -----It takes maybe 30 seconds or so.-----`, grabs the largest free
   block up to 0x1FFBFF0 (the test assumes a 32 MB space), writes a 32-bit LCG (tm:1020E8,
   seed 1) to pseudo-random word addresses 8M times, reads back the same sequence, tallies
   mismatches per channel (A/B) x 4 MB bank (8 banks) (tm:102108 maps addr->ch/bank), prints
   the `| Ch.A | Ch.B |` table with OK/NG per 4 MB row, then `RDRAM A ch:OK/NG`,
   `RDRAM B ch:OK/NG`. Any NG -> `RDRAM Test failed!` and an infinite loop.
9. M2 ON, M1 OFF, wait 5 s (tm:1001D0(5) = 300 vsyncs), PadPortClose/PadEnd, print `END`, loop.

tm:1008B0 also knows mode 0x3C = VESA3C 1024x768 (prints VESA3C, ResetGraph, DefDBuffV) but
main never requests it in this build, and rejects anything else with `INVALID MODE 0x%x!!`.
So the devkit TESTMODE exercises: video out in 3 standards, both rumble motors, the circle
button, SPU2 output (tone), RDRAM, and on retail units the DVD-player ROM presence.

### 1.4 The 1.60 / 2.00 TESTMODE: a jig-driven RPC server plus a pad menu [V]

tm:main (100938 / 100950): loads rom0:ADDDRV, rom0:LIBSD, rom0:TESTSPU (retried), rom0:TSIO2MAN,
rom0:TPADMAN (the T* drivers are the pad stack built for the test program; 1.60 ships them as
separate ROM entries), reads ROMVER (`rom name %s`), then registers a SIF RPC server id 0x11,
handler tm:1007D0 (2.00: 1007E8) and starts a pad thread at tm:1003A8 (2.00: 1003C0), prio 10,
4000-byte stack.

RPC handler (fno from the IOP, 12 cases, jump table tm:11A7F0):

| fno | name (from TESTSPU) | EE does | result |
|---|---|---|---|
| 1 | AVE_TEST | pattern on NTSC(2)/PAL(3) | 1 |
| 2 | DVE_TEST | DVDNTSC(0x82)/DVDPAL(0x83) | 1 |
| 3 | DTV1080I_TEST | DTV1080I, pll=1 | 1 |
| 4 | RDRAM_TEST | same RDRAM test as 1.50 (tm:1076C0) | 1 ok / 0x80 fail |
| 5-8 | SPU2 digital/analog on/off | `fno %d is not EE testing` | 0x81 |
| 9 | DEV1ROM_TEST | retail: open/read/close rom1:ROMDIR (`Open failed %d`) | 1 / 0x80; non-C answers 1 |
| 10 | VESA3C_TEST | VESA3C 1024x768 | 1 |
| 11 | PLL_CONT_L 540 | prints `PLL 54.0MHz`, DTV1080I pll=0 | 1 |
| 12 | PLL_CONT_H 539 | prints `PLL 53.9MHz`, DTV1080I pll=1 | 1 |

The result returns as a 16-byte SIF command id 0xA (word 3 = result) that TESTSPU's
EndEETestHandler turns into a semaphore signal. Codes 11/12 toggle the GS PLL between the two
1080i pixel clocks (tm:1009E0(0x51, pll) writes the clock select) - a video-PLL test nothing
else in the ROM touches.

Pad menu thread (tm:1003A8), port 0 slot 0, buttons read as (byte2<<8|byte3)^0xFFFF:

| button | effect |
|---|---|
| SELECT (0x100, prints `select`) | next page 0..5 (0 = sequential tests, 1-5 = pattern pages) |
| X (0x40) | page 0: next test in {1 AVE,2 DVE,3 DTV1080I,4 RDRAM,11 PLL LO,12 PLL HI} (table tm:118B00); pages 1-5: step video mode forward |
| CIRCLE (0x20, `maru`) | toggle the colour flag (`disp %d color %d ...`) |
| UP (0x1000, `up`) / DOWN (0x4000) | level +/-1 (mod 5) |
| RIGHT (0x2000) / LEFT (0x8000) | pitch +/-1 (mod 5) |
| R1 (0x8, `R1`) / R2 (0x2, `R2`) | video mode next / previous (tm:102AC8 / tm:102BB0) |
| SQUARE (0x80) | set the pll flag for the next 1080i set-up |

Pages 1-5 call tm:1035B0 -> tm:102C98(page,color,level,pitch,dtha,testmode,pll) (print
`disp %d color %d level %d pitch %d dtha %d testmode %d pll %d`): sets the mode, then draws
pattern `page`: 0 `kityo` = the 8-bar colour bar of 1.50, 1 `cross talk`, 2 `smpte color bar`,
3 `primary color`, 4 `cross hatch`, 5 `gradation` (source colorbar/cbdraw.c, assertion
`count < PACKET_SIZE`). level/pitch parametrise the cross-talk and primary patterns.

Video modes the 1.60/2.00 program can set (tm:102C98; name table printed by tm:1013E8; R1
steps in this order): NTSC 640x448 I (2) -> DVDNTSC 720x480 I (0x82) -> PAL 640x556 I (3) ->
DVDPAL 720x556 I (0x83) -> DTV480P 720x480 (0x50) -> DVD480P 720x480 (0xD0) -> DTV720P 1280x720
(0x52) -> DTV1080I 1920x1080 I (0x51) -> VESA1A-1D 640x480 (0x1A-0x1D) -> VESA2A-2E 800x600
(0x2A-0x2E) -> VESA3B-3E 1024x768 (0x3B-0x3E) -> VESA4A-4B 1280x1024 (0x4A-0x4B) -> back to
NTSC: 23 modes, i.e. every SetGsCrt mode the GS supports, including the VESA ones no game or
OSD ever uses. The 1.50 build has only 2, 3, 0x82, 0x83, 0x51 and 0x3C.

1.60 prints `button %x` on every poll; libpad RPC error strings (`PadPortOpen: rpc error`...)
are in the 1.60 libpad. The 1.50 rumble test is gone; `Play Sound`/`Stop Sound` (SIF command
0xB to TESTSPU) replace the automatic tone.

### 1.5 TESTSPU: the IOP half [V]

dev 1.50 (start, soundTest, StopSound, IntTrans, IntFunc, wait_loop): start registers SIF
command handler 0 -> StopSound and starts soundTest (prio 100, 0x800 stack), which SdInit,
enables SPU2 IRQs, sets both cores' master volume 0x3FFF, DMAs the embedded VAG `1KSine.48`
(0x4F0 bytes = a 1 kHz sine at 48 kHz) to SPU RAM 0x15010, programs all 24 voices of both
cores (vol 0x1EFF, pitch 0x400, reverb attr 0x105), keys on voice 0 of core 0, loops until
Playing is cleared by the EE's command 0, then key-off + `voice completed...`. So the dev
TESTMODE's first screen plays a continuous 1 kHz tone until circle is pressed.

1.60 / 2.00 (GetMekacon, SetMekacon, PlaySound, StopSound, TestThread, EndEETestHandler,
SoundThread, SoundHandler, start): start creates two threads and two semaphores, registers
SIF handlers 0xA (EE result) and 0xB (play/stop from the pad menu). TestThread is the factory
jig's command loop:

```
bind RPC 0x11 (the EE server; "EE test bind errr" + hang if it never appears), PlaySound()
loop:
  code = GetMekacon()   # S-command 0x90: 1 byte in, 2 out [status, code]; retried while status&0x80 ("Mekacon read error")
  1,2,3,4,9,10,11,12 -> CallRpc(0x11, fno=code) and wait for the EE's 0xA reply
  5 / 6   -> SdSetCoreAttr(SPDIF_MODE, 0xF / 0)   # SPU2_DIGITAL_ON / OFF: optical out
  7 / 8   -> PlaySound() / StopSound()            # SPU2_ANALOG_ON / OFF: the 1 kHz tone
  13 (2.00 only) -> LoadStartModule("rom0:XDEV9") once; 1 if started else 0x80  # DEV9_TEST
  other non-zero -> "NO TEST CODE %d"
  if code != 0: SetMekacon(result)   # S-command 0x91: 2 bytes in [0x91, result]
  WaitVblankEnd()
```

Name table ts:1140 (1.60) / ts:1260 (2.00): 1 AVE_TEST, 2 DVE_TEST, 3 DTV1080I_TEST, 4
RDRAM_TEST, 5 SPU2_DIGITAL_ON, 6 SPU2_DIGITAL_OFF, 7 SPU2_ANALOG_ON, 8 SPU2_ANALOG_OFF, 9
DEV1ROM_TEST, 10 VESA3C_TEST, 11 PLL_CONT_L 540, 12 PLL_CONT_H 539, (2.00) 13 DEV9_TEST. Each
step is logged `code %d %s return %x` to the IOP TTY (nowhere on a retail unit). So from 1.60
on the line drives the whole test through the MechaCon: the jig speaks to the MechaCon over
its own link, the MechaCon hands the code to the IOP through two otherwise-unknown S-commands
(0x90 read / 0x91 write), the IOP farms the EE-side tests over SIF RPC. These two S-commands
appear in no SDK header I have [I]; PCSX2's S-command switch has no case for them, so under
PCSX2 GetMekacon spins on the error status forever [I] while the EE pad menu still works.

### 1.6 Where TESTMODE's text goes [V]

Every printf in TESTMODE (all three builds) is SDK libc -> write(1) -> DECI2 EE-TTY:
tm:10CD08 (dev) / tm:113E00 (1.60) opens protocol 0x210 with Deci2Open (syscall 0x7C sub 1),
builds packets and sends them with Deci2ReqSend (tm:10CAE0). If ReqSend fails the write
returns -1 silently - and it always fails on a unit with no DECI2 host, because the kernel
returns -10 when the host link flag is clear (3.2). The only text that reaches the EE SIO
(tm:10CDC8 writes 0x1000F180) is the libgraph/libdma/libpad error printf
(`sceGsSyncPath: ... does not terminate`, `libpad: ...`, `TTY: send err %d`). So on a TEST
kit, a retail console or PCSX2 the factory test shows only its patterns; the RDRAM table, the
`Push Button` prompts and the `code %d %s return` log are visible only on a TOOL with a host.

### 1.7 dev vs 1.60 vs 2.00 summary [V]

| | dev 1.50 | 1.60 | 2.00 |
|---|---|---|---|
| control | fixed sequence, circle to advance | MechaCon-relayed jig codes + pad menu | as 1.60 |
| pad driver | XSIO2MAN/XPADMAN | TSIO2MAN/TPADMAN | same (from ROM) |
| video modes | 5 (+VESA3C unused) | 23 | 23 |
| patterns | colour bar | + cross talk, SMPTE bar, primary colour, cross hatch, gradation | same |
| rumble | yes (M1/M2) | no | no |
| sound | tone auto-on, stopped by circle | tone on/off by jig or pad; S/PDIF toggle | same |
| extra tests | DVD ROM1 (retail only), RDRAM | + PLL 53.9/54.0 MHz, VESA3C | + DEV9 (rom0:XDEV9) |
| packaging | plain ELF | plain ELF | stub + LZ |

---

## 2. The memory-card system-update path

### 2.1 What the OSD looks for [V]

| ROM | files, in order | function | skip switch |
|---|---|---|---|
| dev 1.50 | `%s/osd160.elf`, then `%s/osdmain.elf` (strings 0x2A32C8, 0x2A32D8) | 0x207340, called once from main at 0x20773C | none (no `SkipMc` string in this build) |
| 1.60 | `%s/osd170.elf`, `%s/osdmain.elf` (0x2A3A58/68) | 0x2071F0 region | `SkipMc` (0x2A3B20, tested at 0x207614) |
| 2.00 | `%s/osd210.elf`, `%s/osdmain.elf` (0x2C3B80/90) | TryMcSystemUpdate 0x209378 | `SkipMc`, plus `SkipHdd`, `SkipForbid` |

`%s` is the region-patched system folder from SysExecDir(): `B?EXEC-SYSTEM` with
? = I/A/E/C from rom0:ROMVER (/BREXEC-SYSTEM is the template; osdsys_flow.md 5.2). The number
in osdNNN.elf is the next OSD version the ROM expects to be updated to (1.50->160, 1.60->170,
2.00->210); osdmain.elf is the version-independent fallback.

### 2.2 The checks before launch, exactly [V]

For each of the two file names, the per-port probe (2.00 FUN_002064b0 -> FUN_00201888; 1.50
tm-equivalent FUN_00204a30 -> FUN_002016c0) does, for port 0 then port 1, slot 0 only:

1. sceMcGetInfo. Require result in range, card type == 2 (a PS2 memory card), and the
   "formatted" field == 1. A PS1 card (type 1) or unformatted card is skipped. [V]
2. sceMcOpen(port, slot, "<B?EXEC-SYSTEM>/osdNNN.elf") read-only, sceMcSync. Success = the
   file exists and opened. sceMcClose immediately.

That is the whole gate in the ROM: card present + PS2 + formatted + the file opens. There is
**no size check, no icon.sys check, no in-ROM signature check**, and the file is never read
by the OSD. The first card where either name opens wins (port 0 before port 1). [V]

### 2.3 What is launched, and the argv [V]

ExecMcUpdate (2.00 0x209270; 1.50 0x207238) builds, for the port d and path s that matched:

```
LoadExecPS2("moduleload",
  { "-m rom0:SIO2MAN", "-m rom0:MCSERV", "-x mc%d:%s"  (with d and the full file path),
    argv[1..] of the OSD })     # 1.50 also "-m rom0:MCMAN" and the form "-x mc%d:%s/%s"
```

LoadExecPS2 goes through the kernel to EELOAD, which resets the IOP
(`rom0:UDNL rom0:EELOADCNF`), processes the `-m`/`-k`/`-x` items, and for the `-x` item calls
LoadElfEncrypted (eeload el:82C40): LoadFile RPC fno 5 = LF_F_MG_ELF_LOAD, i.e. the file is
loaded as a **KELF through the MechaCon's decryption**. If that returns -2 EELOAD jumps to
BootIllegalToOsd (`rom0:OSDSYS BootIllegal`); any other negative -> BootErrorToOsd. So the one
real gate on the update executable is the KELF signature the MechaCon enforces, which lives in
the MechaCon, not the ROM. [V for the call chain; I that -2 is the signature-reject path]

### 2.4 Relevance [V/I]

This is exactly the mechanism HDD-OSD updates and the FreeMCBoot exploit use: FreeMCBoot
ships a KELF named osdNNN.elf / osdmain.elf in B?EXEC-SYSTEM so the OSD relaunches it at boot.
The ROM contribution is only "does a PS2-formatted card hold a file with that name, and does
EELOAD's `-x` KELF load succeed". The dev 1.50 build has no `SkipMc`, so there is no in-ROM
way to suppress the card check on that unit. [V for the ROM behaviour; I for the exploit note]

---

## 3. Kernel debug facilities present on every console

Addresses below are from the 2.00 export (analysis/kernel) and confirmed against the devkit
linear disassembly (analysis/devkit/kernel_dev.dis); the devkit kernel build date is
`May 11 2000`, 2.00 is `Feb 6 2003`, same structure.

### 3.1 The DECI2 manager [V]

Deci2Init (k:8000EF18, string `EE DECI2 Manager version 0.06`) runs during HardwareInit on
every boot. It:
- prints the banner `\n%s %s %s\n  CPUID=%x, BoardID=%x, ROMGEN=%04x-%04x, %dM` (version +
  build date/time, CPU PRId, a board id, the two ROMGEN words at 0xBFC00100/102, and
  GetMemorySize in MB) via kprintf (so it is part of the EE-SIO boot log, 3.4);
- registers three protocol handlers (FUN_8000EE90): proto 0x201 (DCMP) -> k:8000F360;
  0x21F (the "DBG"/debug protocol) -> handler k:80012178; 0x230 (KTTY) -> k:80010040;
- sets up a 0x30-entry buffer pool and enables DMAC channel 7 (SIF2) with interrupt handlers
  0x1E and 0x1F pointing at the transport (FUN_8000FF68 writes b000e000/f240/f260, the SBUS
  and SIF2 registers).

Deci2Call is syscall **0x7C**, and it is *not* in the 128-entry syscall table (that slot is
the undefined-syscall printer): exc_syscall special-cases 0x7C before the table lookup
(devkit k:80000288, same in 2.00) and dispatches to FUN_8000ECC8, a switch over sub-functions:

| sub | meaning |
|---|---|
| 1 | Deci2Open(proto, buf, handler) |
| 2 | Deci2Close(socket) |
| 3 | Deci2ReqSend(socket, dest) |
| 4 | Deci2ExRecv / poll-drain (calls FUN_80012540) |
| 5 | Deci2Poll-ish (FUN_8000F910) |
| 6 | Deci2 receive (FUN_80013CC0) |
| 7 | (alias of ReqSend) |
| 8 | Deci2 state query (FUN_8000EC00) |
| 9 | Deci2 state query 2 (FUN_8000EC78) |
| 0x10 | debug attach/break (FUN_80010208 then FUN_80012540) |

So the manager and 10 Deci2Call sub-functions are in every kernel. **Nothing reaches it
without a host**: Deci2ReqSend (FUN_800139C0) first calls FUN_8000E860 (checks the link flag);
when the destination is 'H'/'I' and the corresponding link word (0x800231D4 / 0x800231D8,
devkit 0x800231D4/D8) is 0 it returns -10 (`0xFFFFFFF6`) immediately. Those link words are
set only from the debug exception path (3.2) in response to a host, i.e. they stay 0 on a
unit with no TOOL DECI2 hardware. Under PCSX2 the SIF2 transport (DMA ch 7) is never serviced,
so the same applies.

### 3.2 The debug exception vector 0x80000100 [V]

0x80000100 is a single `j 0x800136E4` (devkit) to a handler that saves k1/ra/v0, calls the
debug notifier chain FUN_80013498 -> FUN_800120D0 -> FUN_80013474 -> FUN_80012138(2) ->
FUN_80013570, then `eret`. More importantly, the **common exception vector 0x80000180**
dispatches by (Cause & 0x7C)>>2 through a table at k:800148C0 (devkit): every ordinary
exception code points at `exc_default` (k:80020280? -> the devkit entry 0x80023648 "H6" in the
table), **except** TLB-refill/interrupt/syscall/debug which have their own vectors. exc_default
and the Break/TLB paths all end in the sequence FUN_80013F10 -> FUN_80013EEC -> FUN_80012BB0(1)
-> FUN_80013FE8. FUN_80012BB0 is the debug event handler: it packages the thread context, then
FUN_80012540 **loops forever** sending a DECI2 "event" packet (type 0x15) to the host and
waiting for the host to resume it (`while (DAT_80024124 != 0 || ...)`).

Consequence: on any PS2, if the EE takes an unhandled exception - a `break`, an address error,
a bus error, a coprocessor-unusable, an overflow - the kernel does not crash to a screen; it
**parks in the debug stub waiting for a DECI2 host to attach**. With no host (every retail
console, every PCSX2 session) the machine simply hangs there. The `\npc=%08x\n`,
`step while BD=1 and branch=0`, `not xgkt inst` and `UART: Frame/Parity/Overrun error`
messages belong to this single-step/break machinery (FUN_80012B48, FUN_800136F0) and are
emitted to the IOP-bus UART at 0x1F80380C (FUN_800137E0 writes sRambf80380c), a device present
only on TOOL/TEST hardware [I]. PCSX2 exposes none of this.

### 3.3 The IOP-bus UART printer [V]

FUN_800137E0 (the DECI2 panic/step output) writes each byte to 0x1F80380C and polls status at
0x1F803820 - the TOOL's second UART, not the EE SIO. The DECI2 panic printer FUN_8000E7B0
(`\n# EE DECI2 Panic!!!\n\t` + `<proto> ... DCMP: MakeError code=%d`) uses it and then hangs.
Not visible on retail or PCSX2.

### 3.4 kprintf and the EE SIO boot log [V]

kprintf (k:800073E0 2.00, k:800073E8 devkit) -> formatter -> putchar FUN_80006E58 (maps \n to
\r\n) -> FUN_80012C30 (devkit) / FUN_80013488 (1.60): spin while SIO status 0x1000F130 & 0xF000
== 0x8000 (TX FIFO full), then store the byte to **0x1000F180 (EE SIO TXFIFO)**. Identical
chain in all builds. The messages the kernel emits during a cold boot, in order (survey 3.2
lists the strings; this pass confirms the order through HardwareInit):

```
# Initialize Start.
# Initialize GS ...  INTC ...  TIMER ...  DMAC/VU1/VIF1/GIF/VU0/VIF0/IPU ...  FPU ...
# Initialize User Memory ...  # Initialize Scratch Pad ...
# TLB spad=0 kernel=1:%d default=%d:%d extended=%d:%d      (TlbInit)
# Initialize Done.
<blank> EE DECI2 Manager version 0.06 <build> <date>
  CPUID=%x, BoardID=%x, ROMGEN=%04x-%04x, %dM                 (Deci2Init banner)
# Restart.                                                    (at each LoadExecPS2 / ExecOSD)
```

(preceded by the RDRAM module's `# Initialize memory (rev:...)` and
`# Total accessable memory size: %d MB ...`, which also target the EE SIO - confirmed, the
RDRAM module writes 0x1000F180 at bfc432f8). **PCSX2 has an "EE SIO" console source**: when it
is enabled, these lines appear in the PCSX2 log window. So the kernel boot log a dev-kit owner
would read on the serial port is the same text a PCSX2 user can see today - it is not
devkit-specific, only the serial connector is.

### 3.5 _print stub and MachineType [V]

_print (syscall 0x75) is `jr ra` (a no-op) in all four kernels (devkit k:800074D0). MachineType
(syscall 0x7E) returns the ROM word at 0xBFC001F8, which is 0 in all four ROMs (devkit
included); PSMode ORs 0x8000 into that for the result of a different query. The devkit does
not mark itself as a devkit in that word.

---

## 4. DEX vs CEX in XCDVDMAN and SECRMAN

### 4.1 The MechaCon-version branches in XCDVDMAN [V]

FUN_00003320 (devkit; 1.60 FUN_00003210) reads the MechaCon firmware version with S-command
_sceCdMV into a 3-byte value and sets two flags:

- version < 0x10800  -> flag A (DAT_83bc) = 0, else 1   (dev prints MECACON_V00010800_mae / _ikou)
- version < 0x20200  -> flag B (DAT_83c0) = 0, else 1   (dev prints MECACON_V00020200_mae / _ikou)

(`mae` = before, `ikou` = from/after, so the flags mean "MechaCon is at least v1.08.00" and
"...at least v2.02.00".) **Retail 1.60 computes exactly the same two flags**
(`DAT_8a6c = 0x107ff < v`, `DAT_8a70 = 0x201ff < v`) - the only difference is the devkit build
prints the four debug strings when its verbose flag DAT_83a0 > 0; the comparison and the two
flags are identical. So these are not a DEX feature, just a version gate with the prints left
in on the dev build.

What the flags do: flag B (>= v2.02.00) gates a read-timing/path in the DVD read setup
(FUN_00003604: `*(p+1)=='0' && DAT_83c0==0 && ...`); flag A (>= v1.08.00) gates a branch in
the disc-change/tray handler (FUN_0000430c). These are MechaCon-generation workarounds (early
vs late mechacons), independent of CEX/DEX. [V for the branches; I for "generation workaround"]

### 4.2 secrman_for_dex vs secrman_for_cex: the missing key-change step [V]

SecrAuthCard (both builds, FUN_000000a4) runs the card-authentication handshake with a PS2
memory card's MagicGate chip. The CEX build (secrman_for_cex, retail 1.60) has, right after
`mechacon auth 0x80`:

```
if (FUN_00001998(card, slot, 1) == 0) { restart }       # the step logged "card auth key change"
else { FUN_00001b28(card, slot, 0xF0, 0); ... continue } # "card auth 0x00" onward
```

FUN_00001998 issues SIO2 card command byte **0x81 0xF7 0x01** (local_3c[0]=0x81, [1]=0xF7,
[2]=1) - a MagicGate "change/rotate the session key" command. The DEX build (secrman_for_dex,
the devkit) does **not have FUN_00001998 or the 0xF7 command at all**: its SecrAuthCard goes
straight from `mechacon auth 0x80` to `FUN_00001ae8(card,slot,0xF0,0)` = `card auth 0x00`, so
the "card auth key change" string and the key-change round-trip are simply absent. Everything
else (the 0x00-0x14 card-auth and 0x80-0x88 mechacon-auth exchange, card decrypt 0x40-0x43,
SecrCardBootFile/SecrDiskBootFile) is byte-for-byte the same sequence.

Operational meaning [V for the code, I for the cryptographic interpretation]: MagicGate
memory-card authentication derives a session key between the card and the MechaCon. The CEX
(retail) MechaCon performs an extra key-change step (command 0xF7) that the DEX MechaCon does
not; the DEX SECRMAN omits it to match. This means a DEX unit and a retail unit run *different
card-authentication handshakes*. It does **not** make retail cards unreadable on a DEX for
plain file I/O (MCMAN/MCSERV handle that and are byte-identical) - the auth path matters for
MagicGate-protected content and for the KELF/`-x` card-boot path (SecrCardBootFile). In
practice a devkit can read and write ordinary saves on a retail card; the divergence is in the
protected-content keying, which is why DEX and retail are kept apart for signed executables.
PCSX2 emulates the card/MechaCon auth generically and does not implement either 0xF7 variant,
so this distinction is invisible there.

### 4.3 sce_dev5 handling added in 1.60 [V]

XCDVDMAN 1.60 adds a special filename `sce_dev5;1`: in the file-open path (FUN_000001c0) after
normalising the name it compares it to `"sce_dev5;1"`, and on a match opens a raw whole-disc
handle (LBN -1, length 0 - a sentinel the read path treats as "read from current position
with no fixed extent"). XLOADFILE 1.60 adds the matching string `cdrom0:sce_dev5`. The devkit
1.50 XCDVDMAN/XLOADFILE have neither string. This is a 1.50->1.60 addition (a raw device node
used by later disc utilities / the DVD player update), not a DEX/CEX difference. [V]

---

## 5. NVM byte-diff and field interpretation

Files: the devkit `.nvm` next to the ROM (a real 1 KiB dump of the DTL-H30101), `scph39001.NVM`
and `SCPH-70004_BIOS_V12_PAL_200.NVM` (both under ~/Library/Application Support/PCSX2/bios).
Non-zero lines captured in analysis/hidden/nvm_*.hex.

### 5.1 The three files at a glance [V]

- **Devkit `.nvm`**: a genuine dump. Filled regions: MechaCon calibration/parameter tables at
  0x000-0x08F; region/parameter block at 0x180 (`30 30 30 14 67 11 2C 01 05 28 00 79` =
  "000" + bytes); model string **`DTL-H30101`** at 0x1A0; id/config blocks at 0x1C0-0x1FF;
  flags at 0x280 (`01` and a trailing `01`) and 0x290 (`40 ... 40`); the OSD config block at
  0x310. Everything else is 0xFF (erased flash) or 0x00.
- **scph39001.NVM**: produced by PCSX2. **Entirely zero except 0x310** = `30 21 81 0E ... E0`
  (16 bytes). No MechaCon map, no model string, no ids.
- **SCPH-70004 .NVM**: a real retail dump - full MechaCon map 0x000-0x12F, model `SCPH-70004`
  at 0x1B0, region block + console/i.LINK ids at 0x1C0-0x1FF, language default block at 0x180
  (`EEengEE`), config at 0x310, and the MagicGate/console-id key material at 0x320-0x3FF.

### 5.2 The differing fields [V bytes / I field names]

| offset | devkit | scph39001 (PCSX2) | SCPH-70004 (retail) | field |
|---|---|---|---|---|
| 0x000-0x12F | MechaCon cal tables | all 00 | MechaCon cal tables | MechaCon NVRAM mirror; PCSX2 leaves it blank |
| 0x180 | `30 30 30 14 67 ...` | 00 | `45 45 65 6e 67 45 45` (`EEengEE`) | region/default-language block |
| 0x1A0/0x1B0 | `DTL-H30101` @0x1A0 | 00 | `SCPH-70004` @0x1B0 | model name string |
| 0x1C0-0x1FF | id/param blocks | 00 | console id, i.LINK id (GUID), region params | per-console identity |
| 0x280/0x290 | `01`.. / `40`.. | 00 | different | init/flags |
| 0x300-0x31F | `.. 3D 21 AE 20 00 ..` @0x310 | `30 21 81 0E ...` | `35 21 C0 3C 20 25 ...` @0x2C0 then 0x310 | OSD config block (language, screen, mode) |
| 0x320-0x3FF | 00/FF | 00 | key material | MagicGate / console keys (retail only) |

The devkit's model string is the one place the NVM says "devkit". Nothing in the ROM reads
the model string; it is used by the dumping/PC tools and the service menus, not by boot code.

### 5.3 OSD config block and the "initialised" bit [V bit / I mapping]

1.50 ReadConfigBlock (0x203008) and 2.00 (0x203DA8) both call sceCdReadConfig for config area
1, 2x16 bytes, which PCSX2 maps to NVM 0x300-0x31F for ROMs before 1.70. UnpackConfig
(1.50 0x202D08, 2.00 0x203A08) returns 1 (= "initial setup needed") when **bit 7 of config
byte 0x11 is clear**. In the devkit NVM the config block at 0x310 is `3D 21 AE 20 00 ...`, so
byte 0x11 (file 0x311) = 0x21: bit 7 clear -> on that unit the first-boot clock/language
set-up runs. The PCSX2 scph39001 blank has byte 0x311 = 0x21 too (`30 21 81 0E`), same "not
initialised" state. [V]

### 5.4 What PCSX2 writes itself [V/I]

PCSX2's scph39001.NVM is blank except the 16-byte OSD config at 0x310 - so PCSX2 generates
*only* the OSD config block for a pre-1.70 ROM and leaves the MechaCon map, model string and
ids zero. (Its SCPH-70004 file here is a real dump the user supplied, not PCSX2-made, so it
has the full map.) When the NVM is absent PCSX2 creates one with just that block; it does not
synthesise a model string or console id for the pre-1.70 config layout. [V that scph39001 is
blank elsewhere; I that this is PCSX2's generation behaviour]

---

## 6. rom0:LOGO

### 6.1 What it is [V]

rom0:LOGO is a **stub + LZ-packed MIPS R3000 (IOP-side / PS1-compatibility) program**, not
data. Layout: an 8-byte header {load address 0x30000, decompressed size}, a 0x44-byte copy
loader, then an OSD-style LZ stream with a 24-bit size word; `osd_unpack.py --raw LOGO 0x54`
decompresses it (dev: 0x20F60 bytes; 1.60 and 2.00: 0x210C0 bytes). The loader at the tail
of the file (dev entry at ROM addr 0x1A45E4) copies the stub to RAM 0x80190000, runs the
decompressor, then jumps to 0x30000. [V]

### 6.2 What is in it [V]

The decompressed image is the **PlayStation 1 graphics/SPU/system library set used in PS1
backward-compatibility mode**: readable strings include `System ROM Version 1.0`,
`Copyright 1993,1994 (C) Sony Computer Entertainment Inc.`,
`$Id: sys.c,v 1.140 1998/01/12 ... $`, `$Id: intr.c,v 1.75 1997/... $`, the libgpu/libgs API
trace strings (`ResetGraph`, `DrawSync(%d)...`, `PutDrawEnv`, `LoadImage`, `ClearOTag`...),
the libspu timeout strings (`SPU : Timeout [%s]`, `libspu: not supported ioctl (%d)`),
`!!!WARNING!!! : Not PS Disk`, `Audio Disk !!`, `Shell Opened !!`, and
`Library Programs (c) 1993-1997 Sony Computer Entertainment Inc., All Rights Reserved.` - i.e.
the PS1 "kernel"/BIOS runtime the PS2 maps in when it runs a PlayStation 1 title. It is loaded
by TBIN (the PS1-mode glue, whose strings include `PS compatible mode by M.T.`), which
references the name `LOGO`. PS1DRV handles the PS2-side of PS1 playback; LOGO is the PS1 ROM
image itself. [V]

### 6.3 dev vs 1.60 vs 2.00 [V]

1.60 and 2.00 LOGO are byte-identical to each other (decompressed). The devkit (1.50) LOGO is
an **older build** of the same program: 0x20F60 vs 0x210C0 bytes, and it is **missing the
`Licensed by Sony Computer Entertainment (Inc. / Europe)` boot-screen strings** that 1.60/2.00
carry (three variants, used for the PS1 region-licence screen). The decompressor stub itself
is the same code at a slightly different link address (dev entry 0x1A45E4 vs 0x1A4530). So the
difference is a version bump plus the added licence-screen text, not a devkit feature. The
`0x14748 bytes, not unpacked by the survey` entry is now resolved: it is the packed PS1 BIOS
runtime. [V]

---

## Most interesting findings (ranked by how surprising they are to a PS2/PCSX2 user)

1. **An unhandled EE exception on any PS2 does not crash - it silently waits for a debugger.**
   The common exception vector routes `break`, address errors, bus errors, cop-unusable and
   overflow into the kernel's DECI2 debug stub, which loops forever sending an "event" packet
   to a DECI2 host and waiting to be resumed (3.2). With no host (every retail console, every
   PCSX2 run) the machine just hangs in that loop. The full EE kernel debugger - single-step,
   breakpoints, the `\npc=%08x\n` / `step while BD=1 and branch=0` machinery - ships in every
   console's ROM, inert.

2. **The factory test program is a complete, bootable diagnostic suite in every ROM.** A full
   TESTMODE with a colour-bar / SMPTE / cross-hatch / gradation pattern generator across 23
   video modes (every SetGsCrt mode incl. the unused VESA ones), an 8-bank RDRAM walk, rumble,
   a 1 kHz tone and S/PDIF toggles - gated only by one bit (0x1F402005 bit 4) that only the
   factory jig sets. You can extract and run the ELF, and most of it works; only the MechaCon
   test-code relay (two undocumented S-commands 0x90/0x91) is dead off the production line (1).

3. **The whole FreeMCBoot / HDD-OSD update hook is four lines of ROM with almost no checks.**
   The OSD, at boot, looks on each memory card for `B?EXEC-SYSTEM/osdNNN.elf` (or osdmain.elf)
   and, if the file merely *opens* on a PS2-formatted card, relaunches it via EELOAD `-x` - no
   size, icon or in-ROM signature check (2). The only real gate is the MechaCon's KELF
   decryption in LoadElfEncrypted. The exact file name is version-specific: osd160 (1.50),
   osd170 (1.60), osd210 (2.00).

4. **The kernel's `# Initialize ... / # Restart.` boot log is visible in PCSX2 today.** It goes
   to the EE SIO at 0x1000F180, which PCSX2's "EE SIO" console source captures - the same text
   a devkit owner reads over the serial port, including the `EE DECI2 Manager version 0.06`
   banner with CPUID/BoardID/ROMGEN/memory-size (3.4). Not devkit-exclusive at all.

5. **The devkit's only cryptographic difference is one missing memory-card command.**
   `secrman_for_dex` omits the MagicGate "key change" step (SIO2 card command 0xF7) that
   `secrman_for_cex` runs; otherwise the auth handshake is byte-identical (4.2). That single
   omission is why DEX and retail keep separate card keying for protected content and signed
   executables - ordinary saves still interchange.

6. **rom0:LOGO is the PlayStation 1 BIOS, packed.** The survey's un-identified 0x14748-byte
   blob is the PS1 graphics/SPU/system library runtime (`System ROM Version 1.0`,
   `Copyright 1993,1994`) that the PS2 maps in for PS1 discs; the devkit's copy predates the
   `Licensed by SCE` boot-screen strings 1.60/2.00 added (6).

7. **The MechaCon "mae/ikou" debug prints are not a DEX thing** - retail 1.60 computes the
   same two MechaCon-version flags (>= v1.08.00, >= v2.02.00) silently; the dev build only
   left the printf in (4.1).

## What I could not determine

- **Who actually sets 0x1F402005 bit 4.** It is [I] the MechaCon test-mode flag from the
  factory jig; no ROM code writes it, and I have no MechaCon firmware to confirm.
- **The two MechaCon S-commands 0x90 / 0x91** that TESTSPU uses to receive a test code and
  return a result (1.5): their definition is in the MechaCon, not in any ROM module or SDK
  header I have. (This is why TESTMODE cannot be fully exercised under PCSX2.)
- **BoardID** in the DECI2 banner - its source register was not traced; CPUID is the EE PRId
  and ROMGEN is the pair at 0xBFC00100/102, but BoardID's origin I did not pin down.
- **Whether PCSX2 would in fact stop at the debug stub or trap earlier.** I read the ROM code
  path (3.2); I did not run a faulting program under PCSX2 to see whether its EE recompiler
  reports the exception before the ROM handler runs.
- **The exact -2 reject condition in EELOAD's `-x` KELF load** (2.3) - the LOADFILE RPC
  fno 5 path returns it, but the signature check itself is MechaCon-side and not in the ROM.
- A function-level diff of 1.50 OSDSYS/KERNEL against a *retail* 1.50 (no such ROM available),
  as in the survey.
