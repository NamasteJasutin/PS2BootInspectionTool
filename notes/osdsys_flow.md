# OSDSYS top-level control flow and the play-history data

Subject: `rom0:OSDSYS` from SCPH-70004 (ROM 2.00 E, `ROMVER = 0200EC20040614`), decompressed
image at `0x200000`; plus `rom0:EELOAD` (raw, loads at `0x82000`) and `rom0:KERNEL` (raw,
`0x80000000`) for the boot chain. All addresses are virtual addresses in those images.
`ctx` = the global context struct at `0x1F0000`.

Conventions: **[V]** = verified from code (decompilation and/or disassembly read for this
note); **[I]** = inferred / educated guess (naming of SDK calls, hardware meaning, or code
that was only skimmed). Function addresses in parentheses are where the claim comes from.

---

## 1. Boot chain (KERNEL → EELOAD → OSDSYS)

* **[V]** KERNEL has one routine (`0x80005598`) that builds an argument block starting with
  the literal `"EELOAD"`, then a program path, then the caller's args, and (re)starts EELOAD
  with it. Two thin wrappers sit right behind it:
  * `0x80005988` = `ExecOSD(argc, argv)` → `0x80005598("rom0:OSDSYS", argc, argv)`.
  * `0x800059A0` → `0x80005598("rom0:OSDSYS", 1, {"BootBrowser"})` **[I]** this is what a
    game's `Exit()` ends up in (returns to the browser without the opening).
* **[V]** EELOAD `_start` (`0x82008`) clears bss, sets up heap/stack and calls
  `main(argc, argv)` at `0x82388`. A two-entry default table at `0x8FF08` holds
  `{"rom0:TESTMODE", "rom0:OSDSYS"}`.
  * `argc < 2` (cold boot, started by the kernel with no program): waits for the IOP
    (`0x82170`), then if bit 4 of CDVD register `0x1F402005` is set it tries
    `rom0:TESTMODE` **[I: "test mode" flag]**, otherwise loads `rom0:OSDSYS` and does
    `ExecPS2(entry, gp, 1, {"rom0:OSDSYS"})` (`0x8256C`–`0x82604`, `0x822B8`).
    **So on a normal power-on OSDSYS receives `argc = 1`, `argv[0] = "rom0:OSDSYS"`.**
  * `argc ≥ 2` (the `LoadExecPS2` path): `argv[1]` is either a program path, or
    `"moduleload"` / `"moduleload2 <IOP reset string>"` followed by `-m <irx>`, `-k <irx>`,
    `-x <encrypted elf>` items. EELOAD resets the IOP (`"rom0:UDNL rom0:EELOADCNF"` unless
    `moduleload2` supplied its own string), loads the ELF and `ExecPS2`s it with the
    remaining args (`0x823B8`–`0x82568`).
  * Load failures go back to the OSD through syscall `0x7B` (`ExecOSD`):
    result `-2` → `ExecOSD(1, {"BootIllegal"})` (`0x82350`); any other negative result →
    `ExecOSD(2, {"BootError", <path>})` (`0x82310`). **[I]** what exactly makes the loader
    return −2 was not traced (loader `0x82AD0`, LOADFILE RPC).
* **[V]** Resulting OSDSYS command lines (argv[0] is always the OSDSYS path):

  | argv[1..] | origin | effect in OSDSYS `main` |
  |---|---|---|
  | *(none)* | cold boot | `ctx[0x5E8] = 1` → opening |
  | `BootBrowser` | kernel `0x800059A0`; OSDSYS itself (`0x202258`) when SYSTEM.CNF handling fails | browser, no opening |
  | `BootError <path>` | EELOAD; OSDSYS `0x209548` (with `"BOOT_ROM_MODULE"`) | browser; if `<path>` contains `dvdplayer.elf` or `DVDELF` an error message is shown |
  | `BootIllegal` | EELOAD | disc state forced to "illegal", warning scene |
  | `BootClock`, `BootOpening`, `BootWarning` | not produced by anything in this ROM that I found **[I: debug/other callers]** | clock / opening / warning scene |
  | `SkipMc`, `SkipHdd`, `SkipForbid` | idem | skip MC update check / HDD module probing / the DVD-forbid call |

---

## 2. `main` (`0x209EB8`)

### 2.1 Arguments and early init, in order **[V unless noted]**

1. `0x263270` one-time runtime init; `argc == 0` → `ExecOSD(2, {"BootError","BOOT_ROM_MODULE"})` (`0x209548`).
2. `0x270718` SIF RPC init. Then `argv[0]` decides how the IOP is rebooted (`0x2094D8` matches
   `"<prefix><digits>:"`):
   * `rom…:` → IOP reset string `"rom0:UDNL rom0:OSDCNF"`;
   * `mc…:` → same directory as argv[0] with the file name replaced by `osdmain.irx`, flag `0x100`;
   * anything else → BootError.
   `0x2706F0` (RPC exit), `0x26F768` (IOP reset), spin on `0x26F720` until the IOP is back,
   `0x270718` again.
3. `0x25AC38(1)` CD/DVD library init **[I: `sceCdInit(SCECdINoD)`]**; if the console region
   (`0x205880`) is 3 (China) load `rom0:ADDROM2`; `0x25E298` memory-card library init;
   `0x20EF30` patches the region letter into the system folder names (see §5.2).
4. Scan `argv[1..]` for exact `SkipMc`, `SkipHdd`, `SkipForbid`.
5. Unless `SkipMc`: `0x209378` looks on both memory cards for `B?EXEC-SYSTEM/osd210.elf`, then
   `…/osdmain.elf`, and if found `LoadExecPS2("moduleload", {"-m rom0:SIO2MAN","-m rom0:MCSERV","-x mc%d:%s", argv[1..]})` (`0x209270`) — the system-update hook.
6. `0x209A90`: if `rom0:XDEV9` exists, start a helper thread (`0x2099F0`, prio 3) that loads
   `XDEV9` + `XDEV9SERV`. Otherwise, unless `SkipHdd`, `0x209D80` probes `ATAD`, `XFLASH`,
   `XFROMMAN`, `HDDLOAD`; only if `HDDLOAD` loads is the "HDD boot" flag `0x27B418` set.
   **On this ROM `rom0:XDEV9` exists, so `0x209D80` is never called and the HDD-boot flag
   stays 0** (see §3, `0x205960`).
7. `0x2722D0`, `0x272260(0..3)`, `0x25F400(0)`, `0x257140(0)` — graphics-path / pad library
   init **[I]**; `0x206E58(argv[0])` loads the rom0 asset archives; `0x20DCC8(asset 0)` font
   init; `0x2004B8` sound init (result kept in `0x27E70C`).
8. Reads `rom0:ROMVER`, turns `"0200EC"` into the 4 bytes {2, 0, 'E', 'C'} and passes them to
   `0x25BD18` **[I: `sceCdBootCertify`]**; `0x25D728(0,0,0,5,…)` **[I: unknown CDVD call]**;
   `ctx[0x0C] = 0`; unless `SkipForbid`, loop on `0x25BAE8` **[I: `sceCdForbidDVDP`]**.
9. Wait for the XDEV9 helper thread (`0x209B10`). `0x25D8A8` returns a clock, a 16-bit user
   word, a *wake-up reason* and a flag **[I: `sceCdReadWakeUpTime`]**; two IOP-memory reads
   via `0x256EE8` (address `0x3C0`, then the pointer found there) fetch a flag byte.
   Wake-up reason 0/1/`0x100` = normal boot. Reason 2 with flag bit 2, reason 3 with flag
   bit 4, or any other reason, power the console off again through `0x209588` (never
   returns). With reason 2 the console is also powered off right before the main loop unless
   HDD boot is enabled, in which case the loop runs once and then powers off. **[I]** for
   the meaning (timer wake-up of recorder-type hardware?); **[V]** for the branches. None of
   this triggers on an ordinary power-on.
10. `0x204158` reads the NVRAM config block into `0x2C9700` (`0x203DA8`), copies its first 15
    bytes to `0x2C9770` and unpacks bytes `0x0F`–`0x14` into the packed config words
    `0x2C9780/84` (`0x203F18`, `0x203A08`). It
    returns 1 when the "OSD initialised" bit (bit 7 of config byte `0x11`) is **clear**; in
    that case `0x205950` flags *initial setup needed*. Then language is validated and the
    string table selected (`0x204F50`, `0x204F70`), and `0x208660` pushes the settings to the
    kernel (`SetOsdConfigParam`/`SetOsdConfigParam2`).
11. `0x207C90` time-zone setup, `0x26DA90(1, 0x1031, !spdif)` digital-out setting,
    **`0x204858` loads the play history (§5.3)**, `0x208980` GS/VIF reset,
    **`0x206C70` registers the modules (2.2)**, **`0x209460` runs every module's init
    callback** and stores non-negative results (thread ids) in order at `0x2DA4A0[]`.
12. Seven semaphores are created (all max 1, initial 0) and the service threads started (2.3).
    `0x20FB78(0x20, 0x680500, 0x680000)` starts the sound/CD-player streaming layer,
    `0x20F228(5)` starts the disc-state thread. Context defaults: `ctx[0x10] = 0x65`,
    `ctx[0x14] = -1`, `ctx[0x18] = 0`, `ctx[0x5E4] = 0`, `ctx[0xB0] = 5`, `ctx[0x13C4] = 100`, …
13. `0x207930` creates the double buffer (640×256 PAL / 640×224 NTSC, video mode 3 / 2),
    `0x207A10` applies the RGB/YPbPr setting, the VBlank-start handler `0x208550` is installed
    on INTC 2, and main lowers its own priority to `0x1E`.

### 2.2 Module registry — what `0x2059C0` registers **[V]**

`0x205A08` appends a 7-word descriptor (`0x1C` bytes) to a table at `0x2D9C00` (room for 64,
write pointer `0x27B420`). `0x2059C0` is a strict wrapper that rejects descriptors whose
words 0, 2 or 3 are null. Descriptor words:

| word | meaning | default if null |
|---|---|---|
| 0 | `init()` — called once from `0x209460`; returns a thread id (≥ 0) or a negative value | — |
| 1 | unused here | stub returning −100 |
| 2 | `name(language)` → string (`0x205BD0` calls it) | — |
| 3 | `version(language)` → string (`0x205C18`) | — |
| 4 | extra getter (`0x205C60`) | stub returning 0 |
| 5, 6 | unused here | stubs returning 0 / −100 |

So the registry doubles as the data source for the *Version* screen **[I]** and as the list
of UI threads. `0x206C70` registers, in this order:

| # | registered by | init | notes |
|---|---|---|---|
| 0 | `0x205E30` | `0x205DA8` (fetches a string from the drive controller into `0x2DA420`, `"Unknown"` on failure **[I: console model name]**; returns < 0) | info only |
| 1 | `0x216270` | `0x216200` | **Opening** — creates thread `0x2162C0` (stack `0x323650`, size `0x20000`, prio 6) |
| 2 | `0x220FF0` | `0x220F80` | **Clock / main menu / system configuration** — thread `0x221450` (stack `0x34C180`, prio 6) |
| 3 | `0x247C88` | `0x247C18` | **Browser** (memory cards, disc icon, audio-CD player) — thread `0x248418` (stack `0x3EEEC0`, prio 6) |
| 4 | `0x205EB8` | none | info only (version string `"2.00"`) |
| 5 | `0x206170` | `0x205F00` (reads `rom0:PS1ID`/`PS1VER`; returns −1) | info only |
| 6 | `0x206A50` | `0x206868` (reads `rom1:DVDID`/`DVDVER`; returns −1) | info only |
| 7 | `0x206C28` | none | info only |

Hence `0x2DA4A0[0..2]` = {opening thread, clock thread, browser thread}. All three threads
are started immediately but begin with `SleepThread()`. Module names "clock" and "browser"
are **[I]**, taken from the `BootClock` / `BootBrowser` arguments that select them.

### 2.3 Threads, handlers and semaphores **[V]**

| entry | prio | stack | role |
|---|---|---|---|
| `main` | `0x1E` after init | — | scheduler loop (2.4) |
| `0x207BB0` | 1 | `0x2DA520` / `0x2000` | *flip thread*: `WaitSema(0x27E718)`; `0x207BE8` swaps the double buffer; `SignalSema(0x27E71C)` |
| `0x208DB8` | 3 | `0x2DC520` / `0x2000` | *per-VBlank input thread*: waits on `0x27E6FC`; pad hot-plug + read into `ctx[0xC58..]`, remote/eject keys, swaps ○/× for regions other than Japan/China, drives the HDD-boot state machine (`0x208A08`), wakes the config worker, ticks the disc thread (`0x20F358`), then signals `0x27E720`. Thread id also in `ctx[0]` |
| `0x209220` | 3 | `0x2FE520` / `0x2000` | waits on `0x27E720`; flushes the queued sound commands (`0x200A98`, unless `ctx[0xCE0]`) and copies the CD-player status into `ctx[0xCEC..]` (`0x20FD88`) |
| `0x208800` | 4 | `0x2DE520` / `0x20000` | *config / memory-card worker*: sleeps; on wake-up executes the command code in `ctx[0xA4]` (3–0x1A: MC and NVRAM operations). Thread id in `ctx[4]` |
| `0x20F478` | 5 | `0x301210` / `0x2000` | *disc-state thread* (§4.1), also polls the RTC about once a second into `ctx[0xCB8..0xCCC]` (`0x200F70`) and writes NVRAM config on request |
| `0x213C08` | `0x20` | `0x303240` / `0x20000` | sound / CD-player streaming (`0x20FB78`), with its own INTC 3 handler `0x20FAD0` |
| `0x2162C0`, `0x221450`, `0x248418` | 6 | see 2.2 | the three UI modules |
| `0x2099F0` | 3 | — | temporary XDEV9 loader |

VBlank-start handler `0x208550` (INTC 2): increments the frame counters `0x27C654` and
`0x300768`; if a module asked for a plain vsync (`0x27E710` pollable) it signals `0x27E71C`;
if a module asked for a flip (`0x27E714` pollable) it signals `0x27E718` (→ flip thread →
`0x27E71C`); and it signals `0x27E6FC` every VBlank (input thread).

`0x27E6F8` is the **"module finished" semaphore**: only `main` waits on it; the opening
(`0x2162C0`), clock (`0x221450`) and browser (`0x248418`) threads signal it when they return
control.

### 2.4 The main loop / state machine **[V]**

Two context words drive everything:

* `ctx[0x5E8]` — *which module runs next*: 1 opening (normal), 2 clock/main menu,
  3 browser, 4 opening module in its warning scene, 5 browser opened on the audio-CD
  player, 0 "nothing, a launch is pending".
* `ctx[0x14]` — *pending launch request*: −1 none, 0 PS2 DVD, 1 PS2 CD, 2 PS1 disc,
  3 DVD-Video, 4/5 unsupported disc classes, 6 HDD/flash boot.

Initial value of `ctx[0x5E8]` (`0x20A6A0`–`0x20A8CC`): `BootError …` (argc ≥ 3) → 3 (plus the
DVD-player error message: `ctx[0x18] = 1`, localized string `0x61` copied to `ctx+0x1C`);
otherwise 1, then each argument is examined: `BootClock` → 2, `BootBrowser` → 3,
`BootOpening` → 1, `BootWarning` → 4, `BootIllegal` → `0x20F408()` + 4, any other `Boot…`
→ 3, anything not starting with `Skip` → 3. (A test for `"Initialize"` exists but sits
inside the `Boot` prefix branch, so it can never match.) Some wake-up-flag combinations also
force 3. If HDD boot is enabled and an argument was given, main waits for the HDD loader and
may set `ctx[0x5E8] = 0, ctx[0x14] = 6`.

```
loop:
    if ctx[0x14] != -1:
        r = Launch(ctx[0x14])            # 0x203970 – does not return on success
        ctx[0x14] = -1
        ctx[0x5E8] = 4 if r == 3         # illegal disc → warning scene
                   = 3 (+ctx[0x18]=1, message 0x61) if r == 5   # DVD player missing
                   = 3 otherwise          # fall back to the browser
    ResetGraphics()                       # 0x208980
    FlushCache(2)
    tid = threads[0] if ctx[0x5E8] == 4
        = threads[2] if ctx[0x5E8] == 5
        = threads[ctx[0x5E8] - 1]         # 0x2DA4A0[]
    WakeupThread(tid); ctx[0x9C] = 0
    WaitSema(0x27E6F8)                    # module ran to completion
    repeat (forever on a normal boot; see 2.1 step 9 for the wake-up-reason-2 exit)
```

Each module, before signalling, writes `ctx[0x5E8]`, `ctx[0x14]` and `ctx[0x5EC]`
(= *who ran last*: 1 opening, 2 clock, 3 browser):

* **Opening** `0x2162C0`: `SleepThread` → `0x216520` (pick scene: warning if `ctx[0x5E8]==4`)
  → `0x216568` (start jingle) → `0x216340` (init) → `0x20F3C8(scene != 0)` → `0x216490`
  (frame loop) → **`0x2165A0` (decide)** → `0x20F3C8(1)` → `SignalSema(0x27E6F8)`.
  Decision in `0x2165A0`:
  * initial setup needed (`0x205930`) and no HDD boot → `ctx[0x5E8] = 2` (clock module,
    which then runs the language/clock set-up), `ctx[0x5EC] = 1`;
  * otherwise switch on the disc state latched by the timeline (`0x2C87A0`, copied from
    `ctx[0x10]` in `0x21A608` when the animation reaches its last stage, or every frame in
    the warning scene):

    | latched state | result |
    |---|---|
    | `0x6A`, `0x6B` | `ctx[0x14] = 2` (PS1) |
    | `0x6C`, `0x6D` | `ctx[0x14] = 1` (PS2 CD) |
    | `0x6E` | `ctx[0x14] = 0` (PS2 DVD) |
    | `0x6F` / `0x70` | `ctx[0x14] = 5` / `4` (end up in the browser) |
    | `0x73`, `0x75` | `ctx[0x14] = 3` (DVD-Video) |
    | `0x72` | `ctx[0x5E8] = 5` if `ctx[0xCF8] > 0` (audio tracks found) else 2 |
    | `0x74` | `ctx[0x5E8] = 4` (warning scene) |
    | anything else (no disc, still detecting, unknown) | `ctx[0x5E8] = 2` (clock/main menu) |

    and `ctx[0x5EC] = 1` whenever no launch was requested.
* **Clock** `0x221450` / `0x2210A8`: remembers the last disc state it reacted to
  (`0x2C901C`, seeded from `ctx[0x13C4]`, or 100 = "no disc" when `ctx[0x5EC] == 1`, i.e.
  coming straight from the opening, so that a disc already in the drive counts as new). When
  `ctx[0x10]` changes to a bootable/known class it plays the exit transition and leaves with
  `ctx[0x14]` = 0 (`0x6E`), 1 (`0x6C/6D`), 2 (`0x6A/6B`), 3 (`0x73/75`), 5 (`0x71`), or with
  `ctx[0x5E8]` = 5 (`0x72`), 4 (`0x74`), 3 (user chose Browser, or `0x6F/0x70`);
  `ctx[0x5EC] = 2`.
* **Browser** `0x248418`: exit code prepared by `0x248008(next)` / `0x248068(discState)` and
  committed by `0x248130`: `ctx[0x5E8]` = next module (0 with a launch code 0–3 when the user
  starts the disc, 4 on an illegal disc, otherwise the module asked for), `ctx[0x14]` =
  launch code or −1, `ctx[0x5EC] = 3`, `ctx[0x18] = 0`.

### 2.5 Launching (`0x203970`) **[V]**

| `ctx[0x14]` | handler | behaviour |
|---|---|---|
| 0, 1 | `0x202AB0` | Disable the disc thread (`0x20F3B8(0)`); verify the disc and obtain its ID (`0x2024C0`, §4.1) — "not ready" → return 2, "illegal" → force state `0x74` and return 3. Read `cdrom0:\SYSTEM.CNF;1` (≤ 1023 bytes), find the `BOOT2` line, take the value, check that the file name after `:`/`\` matches the first 10 characters of the disc ID (any failure → `ExecOSD(1,{"BootBrowser"})`, `0x202258`). Then shut subsystems down (`0x2021E8(1)`), **update + save the play history (`0x204EC0(discId)`)**, and `LoadExecPS2("rom0:PS2LOGO", 1, {boot path})`. |
| 2 | `0x202D50` | `0x203390` derives the PS1 title ID (the `BOOT` line of SYSTEM.CNF → file name between the last `\`/`:` and `;`; `VER` line → second arg; fallbacks: two hard-coded titles, `PSX.EXE` → `"???"`). Failure → return 4. Else shutdown, **history update with that ID**, `LoadExecPS2("rom0:PS1DRV", 2, {id, ver})`. |
| 3 | `0x202E00` | **History update with the fixed name `"DVDVIDEO"`** (done first, so it is recorded even when no player is found), then look for a memory-card DVD player update (`B?EXEC-DVDPLAYER/dvdplayer.elf`) or the ROM player (`rom1:DVDID`…); none → return 5. Launch through `moduleload …` / `moduleload2 rom1:UDNL rom1:DVDCNF`. (If `ctx[0x13C0]` is non-zero the function takes a different, non-returning path that was not analysed.) |
| 4, 5 | — | return 1 (→ browser) |
| 6 | `0x203750` | HDD / flash boot (`rom0:HDDBOOT` or `xfrom:XFROMBOOT` argument sets, `ExecPS2(0x100000, …)`) |

---

## 3. Small accessor functions

| addr | proposed name | meaning | status |
|---|---|---|---|
| `0x205310` | `RomRegionRaw` | Reads `rom0:ROMVER` once, maps the region letter (byte 4): `J`→0, `A`/`H`→1, `E`→2, `C`→3; caches in `0x27B400`; −1 if unreadable. Byte 5 (`C`/`D`/`T`) cached in `0x27B3E0`. | V |
| `0x2058D0` | `RomRegion` | `max(RomRegionRaw(), 0)`: 0 Japan, 1 America/Asia, 2 Europe, 3 China. **2 on this console.** | V |
| `0x205908` | `IsPAL` | `RomRegionRaw() == 2`. Used for 256- vs 224-line buffers, video mode 3 vs 2 (`0x207930`), 50/60 Hz timing constants. | V |
| `0x2053F0` / `0x205880` | `ConsoleRegion` | Finer region from `rom0:OSDVER` byte 4 (`J`0 `A`1 `E`2 `C`3 `R`4 `K`5 `H`6). This ROM's OSDVER is `0200????`; the `?` letters are replaced by bytes from a CDVD "region parameters" query (`0x2050D0` → `0x25E1A0`). Falls back to `RomRegion()`. Also fixes the default language (`0x205528`). | V (query name I) |
| `0x205930` | `NeedsInitialSetup` | `0x27B414 == 0`. The flag starts at 1, is cleared by `0x205950` when NVRAM says the OSD was never initialised, and set again by `0x205940` when the set-up screens finish (`0x2297C0`). | V |
| `0x205960` | `HddBootEnabled` | `0x27B418 != 0`; only set when an `HDDLOAD` module was loaded (`0x209D80`). **Always 0 on this ROM** (see 2.1 step 6). | V |
| `0x205970` | `HddBootStatus` | `0x27B41C`: 0 pending, 1 boot image ready, −1 failed; driven by `0x208A08`. Irrelevant on this ROM. | V |
| `0x204218` | `ScreenType` | Bits 1–2 of the packed config word `0x2C9780`, values > 2 → 0. 0 = 4:3, 1 = Full Screen, 2 = 16:9 (`0x208660` writes it into the `screenType` field of `SetOsdConfigParam`). The opening sets `letterbox = (ScreenType() != 1)`. | V (value names I) |
| `0x204F50` → `0x2042C0` | `Language` | Config bits 4–8, validated against the console region (`0x205770`), else the region default. 0 jpn, 1 eng, 2 fre, 3 spa, 4 ger, 5 ita, 6 dut, 7 por, 8 rus, 9 kor, 10 tch, 11 sch. | V |
| `0x208530` | `VblankCount` | `0x27C654`, +1 in every VBlank-start interrupt (`0x208550`) — real elapsed fields, unlike the opening's own rendered-frame counter. | V |
| `0x2089F0` | `HddLoaderFromFlash` | `0x300540 == 6` (state of the HDD/flash loader state machine). Irrelevant on this ROM. | V / I |
| `0x207AD8` | `WaitVSyncBegin` | Asks the VBlank handler for a wake-up (`0x27E710`), waits (`0x27E71C`), then samples the interlace field (`ctx[0xC44] = !CSR.FIELD`), applies the half-line offset to the current back buffer's draw env and loads it. No buffer swap. Called once at the start of a scene. | V |
| `0x207B80` | `EndFrameFlip` | Asks for a flip (`0x27E714`) and waits (`0x27E71C`). At the next VBlank the flip thread runs `0x207BE8`: `ctx[0xC44] = !CSR.FIELD`, `ctx[0xC40] ^= 1`, half-offset, swap double buffer `ctx+0xA10`, load the new draw env. One call = one displayed frame. | V |
| `0x20F3C8` | `DiscSetVerifyEnable` | Sets `0x30321C`. When 0 the disc thread does **not** run the (drive-reading) PS2 disc verification on a newly seen PS2 disc; it reports `0x6C–0x6E` straight from the drive's disc-type register. The opening passes 0 for the normal animation and 1 for the warning scene, and restores 1 afterwards. **[I]** purpose: avoid drive access hitching the animation. | V |
| `0x20F3B8` | `DiscThreadEnable` | Sets `0x303218`; 0 suspends verification and the thread's periodic work (used while launching). | V |
| `0x205020` | `GetString(id)` | Pointer `id` of the current language table (`0x27B3D8`), with ids `0x56`/`0x57` swapped to alternates outside Japan/China (the ○/× labels). | V |
| `0x207900` / `0x207918` | `AssetPtr` / `AssetSize` | (known) | — |
| `0x200BE8` | `SoundCmd` | Queues a sound command (128-entry ring at `ctx+0x5F8`), flushed by `0x200A98`. | V (skimmed) |
| `0x2677D0` / `0x2677C0` | `rand` / `srand` | newlib LCG `seed = seed*1103515245 + 12345; return seed & 0x7FFFFFFF`. Seed starts at 1 and is **not** seeded at boot (only the CD player calls `srand`), so the opening's random numbers are the same after every cold boot. | V |

---

## 4. Status codes and context fields

### 4.1 `ctx[0x10]` — disc state (writer: disc-state thread) **[V]**

`0x20F478` wakes once per VBlank (ticked by the input thread through `0x20F358`) and calls
`0x20F678`, which translates the CDVD *disc type* hardware register (byte at `0x1F40200F`)
into the code stored in both `0x303230` and `ctx[0x10]`:

| register | code | meaning (libcdvd name **[I]**) |
|---|---|---|
| `0x00` | 100 (`0x64`) | no disc / tray open |
| `0x01` | `0x65` | detecting (also the fallback for any unlisted value, and the value main stores before the thread runs) |
| `0x02` / `0x03` / `0x04` | `0x66` / `0x67` / `0x68` | detecting: CD / DVD single layer / DVD dual layer |
| `0x05` | `0x69` | unknown / unreadable disc |
| `0x10` / `0x11` | `0x6A` / `0x6B` | PlayStation CD / with CD-DA |
| `0x12` / `0x13` | `0x6C` / `0x6D` | PlayStation 2 CD / with CD-DA |
| `0x14` | `0x6E` | PlayStation 2 DVD |
| `0x20` / `0x21` / `0x22` | `0x6F` / `0x70` / `0x71` | not identified (never launched; handled as "show in browser") |
| `0xFD` | `0x72` | audio CD |
| `0xFE` | `0x73` | DVD-Video |
| `0xFF` | `0x74` | illegal / unrecognised media |
| `0xFC` | `0x75` | treated exactly like DVD-Video **[I: DVD-VR]** |

Extra rules in `0x20F678`:

* **Illegal is sticky.** Once the state is `0x74` it stays `0x74` until the register reads 0
  (disc removed), then becomes 100. (If `ctx[0x0C]` were non-zero it would never clear.)
  `0x20F408` forces `0x74` from outside (the `BootIllegal` argument and a failed launch).
* **PS2 discs are verified** when both enable flags are set (`0x303218`, `0x30321C`) and the
  state is not already the target code: `0x20F8E8` → `0x2024C0` reads disc key data twice
  **[I: `sceCdReadKey`]**, cross-checks it and decodes the 11-character title ID
  (`AAAA_NNN.NN`). Result 2 ("not ready") leaves the state unchanged for a retry on the next
  tick, 3 ("bad") gives `0x74`, success gives `0x6C/0x6D/0x6E`. With verification disabled
  the target code is stored unverified; the launch path repeats the check anyway.

Readers: opening timeline `0x21A608` (in stage 1, without HDD boot, it only advances once
the state is *settled* — 100, `0x6A…0x70`, `0x72…0x75` — and more than 2 s worth of frames
have been rendered; in stage 2 it latches the state into `0x2C87A0` once and picks the
closing sound from it), clock `0x2210A8`, browser `0x245A00`, CD player `0x212608`.

### 4.2 Other context fields

| field | meaning | writers | status |
|---|---|---|---|
| `ctx[0x00]`, `ctx[0x04]` | thread ids of the input thread and the config worker | `main` | V |
| `ctx[0x0C]` | "keep illegal state forever" switch read by `0x20F678`; the warning scene (`0x21A608` stage 6) only reacts to disc changes while it is 0 | **only written once: `main` stores 0** (a scan of every store with a `0x1F0000` base found no other writer) → constant 0 in this build | V |
| `ctx[0x10]` | disc state, §4.1 | `0x20F678`, `0x20F408`, `main` (init `0x65`) | V |
| `ctx[0x14]` | pending launch request, §2.4 | `main`, `0x2165A0`, `0x2210A8`, `0x248130` | V |
| `ctx[0x18]`, `ctx+0x1C` | "show error message in browser" flag and the message text | `main`, cleared by browser `0x248130` | V |
| `ctx[0x5E4]` | 0 while the clock/browser module is interactive, 1 once it is exiting | `0x2210A8`, `0x248130` | V (purpose I) |
| `ctx[0x5E8]` | next module, §2.4 | `main`, `0x2165A0`, `0x2210A8`, `0x248130` | V |
| `ctx[0x5EC]` | module that ran last: 1 opening, 2 clock, 3 browser. Read by the clock module (`0x2210E4`, `0x22B9B8`) to tell "arrived from the opening" from "returned from the browser" | same three | V |
| `ctx[0xA4]`, `ctx[0xB0]` | config-worker command code and its state (5 idle, 6 busy, 8 done, 9 …) | many (`0x2010xx`, `0x239xxx`) | V (skimmed) |
| `ctx+0x138` … `+0x305` | play-history table, §5 | `0x2046F0`, `0x201E98`, `0x204858` | V |
| `ctx+0x306` (22 bytes), `ctx[0x31C]` | evicted history record and "have one" flag | `0x201E98` | V |
| `ctx[0xC40]`, `ctx[0xC44]` | back-buffer index (toggles per flip) and current interlace field | `0x207BE8`, `0x207AD8`, `0x207930` | V |
| `ctx[0xC50]`, `ctx[0xC54]` | frame size: 640 × 256 (PAL) or 224 | `0x207930` | V |
| `ctx[0xC58..0xC5B]`, `ctx[0xC78]` | merged pad data (active-low buttons) and its length | `0x208DB8` | V |
| `ctx[0xCB8..0xCCC]` | RTC as decimal ints: year (2000+), month, day, hour, minute, second | `0x200F70` | V |
| `ctx+0xCEC` … | CD-player status snapshot; **`ctx[0xCF8]` = number of audio tracks** on the current disc | `0x20FD88`, `0x212AD0` | V (track meaning I) |
| `ctx[0x13C4]` | last disc state the browser acknowledged; seeds the clock module | browser `0x245A10` etc. | V |

---

## 5. The play-history file

### 5.1 Record layout — 21 records × 22 (`0x16`) bytes = 462 (`0x1CE`) bytes **[V]**

The file is a raw image of the table at `ctx+0x138`; there is no header, checksum or
version field.

| offset | size | field | details |
|---|---|---|---|
| `+0x00` | 16 | `name` | title ID, NUL-padded (`strncpy(…, 16)`). PS2: 11-char ID decoded from the disc (`SLES_123.45`); PS1: file name from SYSTEM.CNF `BOOT=`; DVD-Video: `DVDVIDEO`. First byte 0 = empty slot. |
| `+0x10` | 1 | `count` | launch counter, starts at 1 |
| `+0x11` | 1 | `mask` | 6-bit set (bits 0–5); starts as `0x01` |
| `+0x12` | 1 | `index` | number (0–5) of the bit most recently added to `mask`; 7 = record is "maxed out" |
| `+0x13` | 1 | — | never written by this code (keeps whatever the slot or file contained) |
| `+0x14` | 2 | `date` | little-endian, signed when compared: `day + (month << 5) + ((year − 2000) << 9)` from the RTC (`0x201E68`) |

No time of day and no per-launch log: only the date of the latest launch.

### 5.2 Where it lives **[V]**

`<port>:/B?DATA-SYSTEM/history` on a PS2 memory card, port 0 first, then port 1. The `?` is
patched at boot by `0x20EF30` from `RomRegion()`: 0 → `I`, 1 → `A`, 2 → `E`, 3 → `C`
(default `R`). **This console: `mc0:/BEDATA-SYSTEM/history`.** The same letter goes into
`B?EXEC-SYSTEM` and `B?EXEC-DVDPLAYER`. Folder-name getters: `0x20EFE0` (with leading `/`),
`0x20EFF0` (without), `0x20F000`, `0x20F010`.

Companion files written into the same folder: `icon.sys` (rewritten on every save; 1776 bytes
taken from a per-region template at `0x27A84C` / `0x27AC14` (Japan) / `0x27AFDC` (China);
title "Ｙｏｕｒ Ｓｙｓｔｅｍ Ｃｏｎｆｉｇｕｒａｔｉｏｎ", icon name `_SCE8`), `history.old`
(below), and for China only an icon file `_SCE8` from asset `0x4D`.

### 5.3 Loading **[V]**

`0x2046F0(port)`: query the card (**[I]** `sceMcGetInfo`); require result ≥ −1, type 2 (PS2
card) and "formatted"; open `<folder>/history` read-only; read up to `0x1CE` bytes straight
into `ctx+0x138`; close. Returns 0 or a negative error.

* At boot `0x204858` (called from `main` before any thread exists) tries port 0, then port 1;
  if both fail the table is zeroed. **This is the table the opening animation sees.** No
  memory card (or no history file) ⇒ empty table ⇒ no towers.
* A short file simply fills fewer bytes; nothing validates the contents.

### 5.4 Update when a title is launched **[V]**

`0x204EC0(name)` is called from the three launch paths (§2.5) after the disc has been
accepted and just before `LoadExecPS2`:

```
port = 0; if Load(0) fails: port = 1; if Load(1) fails: zero the table, port = 0
Update(name)                      # 0x201E98
if Save(port) fails: Save(port ^ 1)      # 0x204AC0
```

`Update` (`0x201E98`, checked against the disassembly):

```
ctx[0x31C] = 0; found = false; minCount = minDate = INT_MAX; victim = 0
for i in 0..20:                                   # single pass
    r = table[i]; c = r.count                     # value before any change
    if c < minCount: minCount = c; victim = i
    if c == minCount and (s16)r.date < minDate: minDate = r.date; victim = i
    if strncmp(r.name, name, 16) == 0:
        found = true
        r.date = today
        if (r.mask & 0x3F) == 0x3F:               # all six bits already set
            if c < 0x3F: r.count = c + 1
            else:        r.count = 0x3F; r.index = 7
        else:
            c = min(c + 1, 0x7F)
            if c >= 14 and (c - 14) % 10 == 0:    # c = 14, 24, 34, 44, 54
                do b = rand() % 6 while r.mask has bit b
                r.index = b; r.mask |= 1 << b
            r.count = c
call rand() (minute*60 + second) times             # stirs the generator with the RTC
if not found:
    empty = [i : table[i].name[0] == 0]
    maxed = number of non-empty records with index == 7
    if maxed == 21: return                         # table full of maxed titles: not recorded
    if empty: slot = empty[rand() % len(empty)]    # random empty slot, not the first one
    else:     slot = victim
              copy table[victim] (22 bytes) to ctx+0x306; ctx[0x31C] = 1
    strncpy(table[slot].name, name, 16)
    count = 1; mask = 0x01; index = 0; date = today      # byte +0x13 untouched
```

Consequences:

* Life of one title: launch 1 → `count 1, mask 0x01, index 0`. A new random bit is added at
  launches 14, 24, 34, 44 and 54, so `mask` is full (`0x3F`) at 54 launches. The counter then
  runs on to 63 and on the 64th launch the record is frozen at `count = 0x3F, index = 7`.
  Only the date changes after that. (The `0x7F` clamp is never reached in practice.)
* `index` always names the bit that was added last, i.e. the one still "growing"; all other
  set bits are complete. Between two additions `count` tells how far the newest one has got
  (10 launches per step after the first 14).
* Which of the six bits a title gets, and which empty slot a new title lands in, are random
  — so the same play history yields different tables on different consoles.
* **Eviction** happens only when all 21 slots are occupied and an unknown title is launched:
  the victim is the record with the lowest `count`; among equal counts the earlier `date`
  wins. Quirk: `minDate` is not reset when a lower count is found later in the scan, so the
  tie-break can be skewed by dates of records with higher counts seen earlier. A record that
  has reached `index == 7` has `count = 63` and is therefore evicted last; if every record is
  maxed nothing is evicted and the new title is dropped.
* Titles are matched on the first 16 bytes of the name only; there is no per-region or
  per-disc distinction beyond the ID string. All DVD-Video discs share the one `DVDVIDEO`
  record. Audio CDs and failed launches are never recorded.

`Save` (`0x204AC0(port)`):

1. Card must be a formatted PS2 card. List the root (`/*`) and look for the system folder.
2. Folder present: need ≥ 2 free clusters. Absent: need ≥ 10 free clusters, then create it.
   (China only: +3, or +9 when `icon.sys` is missing, and the `_SCE8` icon file is written.)
3. Set the folder's attribute word to `0x2007` **[I: read/write/execute + hidden-from-game
   flags; set-file-info with "attribute" valid]**.
4. Delete `<folder>/history`, then create it and write the 462-byte table
   (`0x2048A0(port, path, ctx+0x138, 0x1CE, append = 0)`; create+write-only open).
5. Rewrite `icon.sys`.
6. If `ctx[0x31C]` is set, append the 22-byte evicted record to `<folder>/history.old`
   (`0x2048A0(…, ctx+0x306, 0x16, append = 1)`: open read/write, or create; seek to end;
   write). `history.old` is never read back by OSDSYS.

Failures are silent: the launch proceeds whether or not the history could be saved.

---

## 6. Function names

| address | proposed name | notes |
|---|---|---|
| `0x80005598` (KERNEL) | `KLoadExec` | builds `"EELOAD" path args…` and restarts EELOAD |
| `0x80005988` (KERNEL) | `ExecOSD` | → `rom0:OSDSYS` with caller args |
| `0x800059A0` (KERNEL) | `ExitToBrowser` | → `rom0:OSDSYS BootBrowser` |
| `0x82388` (EELOAD) | `eeload_main` | |
| `0x82310` / `0x82350` (EELOAD) | `BootErrorToOsd` / `BootIllegalToOsd` | |
| `0x209EB8` | `main` | |
| `0x209548` | `FatalBootError` | `ExecOSD(2, {"BootError","BOOT_ROM_MODULE"})` |
| `0x209588` | `PowerOffConsole` | never returns |
| `0x2094D8` | `HasDevicePrefix` | matches `<prefix><digits>:` |
| `0x209378` / `0x209270` | `TryMcSystemUpdate` / `ExecMcUpdate` | `osd210.elf`, `osdmain.elf` |
| `0x209A90` / `0x2099F0` / `0x209B10` | `StartDev9Loader` / `Dev9LoaderThread` / `WaitDev9Loader` | |
| `0x209D80` | `ProbeHddModules` | not reached on this ROM |
| `0x209460` | `InitModules` | fills `0x2DA4A0[]` with thread ids |
| `0x206C70` | `RegisterAllModules` | |
| `0x205A08` / `0x2059C0` | `ModuleRegister` / `ModuleRegisterChecked` | table `0x2D9C00`, 7 words each |
| `0x205AE8` / `0x205B10` / `0x205BD0` / `0x205C18` / `0x205C60` | `ModuleCount` / `ModuleInitFn` / `ModuleName` / `ModuleVersion` / `ModuleExtra` | |
| `0x216270`, `0x216200`, `0x2162C0` | `OpeningRegister`, `OpeningCreateThread`, `OpeningThread` | |
| `0x220FF0`, `0x220F80`, `0x221450`, `0x2210A8` | `ClockRegister`, `ClockCreateThread`, `ClockThread`, `ClockRun` | |
| `0x247C88`, `0x247C18`, `0x248418` | `BrowserRegister`, `BrowserCreateThread`, `BrowserThread` | |
| `0x248008` / `0x248068` / `0x248130` | `BrowserRequestExit` / `BrowserRequestLaunch` / `BrowserCommitExit` | |
| `0x2165A0` | `OpeningDecideNext` | §2.4 |
| `0x207BB0` / `0x207BE8` | `FlipThread` / `FlipBuffers` | |
| `0x207AD8` / `0x207B80` | `WaitVSyncBegin` / `EndFrameFlip` | |
| `0x207930` / `0x207A10` | `VideoInit` / `VideoApplyOutputMode` | |
| `0x208550` | `VblankStartHandler` | INTC 2 |
| `0x208530` | `VblankCount` | |
| `0x208DB8` | `InputThread` | per VBlank |
| `0x209220` | `SoundFlushThread` | |
| `0x208800` | `ConfigWorkerThread` | commands in `ctx[0xA4]` |
| `0x208980` | `ResetGraphics` | |
| `0x208660` | `ApplyOsdConfigToKernel` | |
| `0x204158` / `0x203F18` / `0x203A08` / `0x203DA8` | `LoadNvramConfig` / `ReadAndUnpackConfig` / `UnpackConfig` / `ReadConfigBlock` | |
| `0x2041E0` / `0x204218` / `0x204280` / `0x204528` | `CfgSpdif` / `ScreenType` / `CfgVideoOut` / `CfgTimezoneOffset` | |
| `0x204F50` / `0x2042C0` / `0x204F70` / `0x205770` | `Language` / `LanguageValidated` / `SetLanguage` / `LanguageAllowedForRegion` | |
| `0x205020` | `GetString` | |
| `0x205310` / `0x2058D0` / `0x205908` | `RomRegionRaw` / `RomRegion` / `IsPAL` | |
| `0x2053F0` / `0x205880` / `0x205840` / `0x205528` | `ConsoleRegionRaw` / `ConsoleRegion` / `DefaultLanguage` / `PickDefaultLanguage` | |
| `0x2050D0` | `ReadRegionParams` | CDVD query, cached at `0x27B3F0` |
| `0x205930` / `0x205940` / `0x205950` | `NeedsInitialSetup` / `SetupDone` / `SetupRequired` | |
| `0x205960` / `0x205970` / `0x2089F0` / `0x208A08` | `HddBootEnabled` / `HddBootStatus` / `HddLoaderFromFlash` / `HddBootStateMachine` | |
| `0x20EF30` | `PatchRegionFolderNames` | |
| `0x20EFE0` / `0x20EFF0` / `0x20F000` / `0x20F010` | `SysDataDir` / `SysDataDirNoSlash` / `SysExecDir` / `DvdPlayerDir` | |
| `0x20F228` / `0x20F478` / `0x20F358` | `DiscThreadStart` / `DiscThread` / `DiscThreadTick` | |
| `0x20F678` | `DiscStateUpdate` | register `0x1F40200F` → `ctx[0x10]` |
| `0x20F3B8` / `0x20F3C8` / `0x20F408` | `DiscThreadEnable` / `DiscSetVerifyEnable` / `DiscForceIllegal` | |
| `0x20F8E8` / `0x2024C0` | `DiscVerifyFromThread` / `Ps2DiscVerifyAndGetId` | returns 0 ok, 2 retry, 3 illegal |
| `0x200F70` | `ReadRtcToCtx` | |
| `0x20FD88` | `CdPlayerSnapshot` | fills `ctx+0xCEC` |
| `0x203970` | `Launch` | dispatch on `ctx[0x14]` |
| `0x202AB0` / `0x202D50` / `0x202E00` / `0x203750` | `LaunchPs2Disc` / `LaunchPs1Disc` / `LaunchDvdVideo` / `LaunchHddBoot` | |
| `0x203390` / `0x203198` | `Ps1GetBootId` / `CnfGetValue` | |
| `0x202380` / `0x202418` / `0x202330` / `0x202A38` | `CnfMatchKey` / `CnfCopyToken` / `CnfNextLine` / `BootPathMatchesDiscId` | |
| `0x2021E8` | `ShutdownSubsystems` | before any exec |
| `0x202258` | `ExecOsdBootBrowser` | |
| `0x204858` | `HistoryLoadAtBoot` | |
| `0x2046F0` | `HistoryLoad(port)` | |
| `0x204EC0` | `HistoryRecordLaunch(name)` | load → update → save |
| `0x201E98` | `HistoryUpdate(name)` | |
| `0x201E68` | `HistoryDateStamp` | |
| `0x204AC0` | `HistorySave(port)` | |
| `0x2048A0` / `0x204A48` | `McWriteFile(port, path, buf, len, append)` / `McFileExists` | |
| `0x25EA40`, `0x25E418`, `0x25E538`, `0x25E5C8`, `0x25E6E8`, `0x25E7C0`, `0x25EB88`, `0x25EFD8`, `0x25EE70`, `0x25E500`, `0x25E8F8` | mc `GetInfo`, `Open`, `Close`, `Seek`, `Read`, `Write`, `GetDir`, `SetFileInfo`, `Delete`, `Mkdir`, `Sync` | RPC function numbers 1, 2, 3, 4, 5, 6, 0xD, 0xE, 0xF (V); SDK names I |
| `0x200BE8` / `0x200A98` | `SoundCmd` / `SoundFlush` | |
| `0x2677D0` / `0x2677C0` | `rand` / `srand` | |
| `0x267518` / `0x268134` / `0x2684C8` / `0x268680` / `0x268278` / `0x268890` / `0x267C98` / `0x26765C` | `memcmp` / `strcmp` / `strncmp` / `strncpy` / `strcpy` / `strstr` / `sprintf` / `memset` | I (by usage; `memcmp`, `rand` read) |

## 7. Not determined

* The exact SDK identity of several CDVD calls in `main` (`0x25BD18`, `0x25D728`, `0x25BAE8`,
  `0x25D8A8`, `0x25DAA8`) and the meaning of the wake-up reason / IOP flag bits that lead to
  an immediate power-off.
* Disc-type register values `0x20`–`0x22` (codes `0x6F`–`0x71`) and `0xFC` (`0x75`).
* What makes EELOAD's loader return −2 (→ `BootIllegal`), and who (if anyone) passes
  `BootClock` / `BootOpening` / `BootWarning` / `Skip…` to OSDSYS.
* The `ctx[0x13C0] != 0` branch of the DVD-Video launcher (`0x202E00`).
* Descriptor words 1, 5, 6 of the module table and `ctx[0x9C]` (cleared by main before each
  module run) — no reader found.
* The internals of the clock and browser modules beyond their entry/exit protocol.
