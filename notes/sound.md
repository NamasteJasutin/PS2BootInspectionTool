# Boot sound: OSDSYS (EE) -> OSDSND (IOP) -> SPU2

Scope: how the SCPH-70004 v2.00 PAL OSDSYS triggers its sounds, how the IOP module
`rom0:OSDSND` plays them, and what the `SND*` assets contain. Written from the decompilation
(`analysis/osdsys/`, `analysis/osdsnd/`) and from decoding the assets; nothing here was auditioned
on hardware.

Legend: **[V]** verified by reading the code / decoding the data; **[I]** inferred (reasoned, named
by analogy with Sony's libspu/libsnd API, or dependent on hardware behaviour not checked).

Addresses: `FUN_002xxxxx` = OSDSYS (EE, image base 0x200000); `snd:xxxxx` = OSDSND (IOP, ELF vaddr,
file offset = vaddr + 0xA0).

## 0. Tools and artefacts produced

| Path | What |
|---|---|
| `tools/snd_dump.py` | Dump an SShd bank (programs, tones, ADSR, SE table) and an SSsq sequence (events with tick, nominal seconds, driver update index, PAL seconds). No args = boot bank + boot sequence; `--all` = every asset. |
| `tools/snd_vag2wav.py [BOOT\|OSDD]` | Decode the ADPCM samples of a bank body to `extracted/snd_wav/<BANK>_<offset>.wav` (mono, 44.1 kHz = root-note rate). |
| `tools/snd_render.py [SEQ] [vol]` | Approximate offline render of a sequence (driver pitch/volume maths + textbook SPU ADSR, **no reverb**) to `extracted/snd_wav/SND<SEQ>_render.wav`; prints per-voice table and loudness curve. |
| `tools/snd_irx_imports.py` | Lists the import stubs of the IRX (to name the `FUN_0001c5xx..0001c78c` stubs). |
| `tools/snd_ghidra.sh` | Headless Ghidra run for OSDSND (project `ghidra_proj_snd/`, output `analysis/osdsnd/`). |

---

## 1. EE side

### 1.1 Transport [V]

* RPC wrapper `FUN_0026da90(wait, cmd, a1..a6)`. Client data at 0x2ae9a0, bound by `FUN_0026dd58`
  to SIF RPC server **0x80000601** (retries until the server answers, then sends cmd 1).
* Normal commands (everything OSDSYS uses for the boot sound): `sceSifCallRpc(client, fno = cmd,
  send = 0x4169c0, 0x40 bytes, recv = 0x4169c0, 0x10 bytes)`; the function returns the first
  received word.
  * Send buffer: word 0 = pointer to the buffer itself (ignored by the IOP), words 1..6 = `a1..a6`
    as 32-bit ints. The IOP reads `a1` at +4, `a2` at +8, `a3` at +0xC, mostly truncating to s16.
  * Reply: word 0 = handler return value.
* Other command classes handled by the same wrapper (not used for the boot sound): `0x6xxx` = "get
  struct" (0x40 bytes returned into a caller buffer), `0x7xxx` = "set struct" (caller's 0x40-byte
  struct is the send buffer), `0x7600` = batched voice attributes, `0x8100/0x8200/0x8600/0x8300..`
  = callback registration kept EE-side.
* The server is registered in OSDSND at `snd:0022c` with dispatcher `snd:002c8`. The module's
  ident string is `rspu2_driver`; it statically links "PsIIlibspu2" and "PsIIlibsnd2 130" (Sony's
  PS1-style SPU/sequence libraries ported to SPU2) and exposes them 1:1 over RPC: `0x0001..0x0025,
  0x0200, 0x1xxx` = Spu* calls, `0x4xxx` = Ss* (libsnd) calls. **OSDSND does not import LIBSD**; it
  drives the SPU2 registers at 0xBF900000 itself. [V]
* On top of that sits an OSD-specific layer, commands **0x5001..0x5201** (`snd:1a46c..1c5b0`), which
  is the only part that understands the `SShd`/`SSsq` formats. The libsnd SEQ/VAB player in the
  module is not used by OSDSYS. [V]

### 1.2 Deferred command queue [V]

Game-logic code does not call the RPC directly; it calls **`FUN_00200be8(cmd, a1, a2, a3)`**:

* Ring buffer of 128 entries x 8 bytes at 0x1f05f8: `{u16 cmd, u16 a1, u16 a2, u16 a3}` (arguments
  are truncated to 16 bits). Write index 0x1f0a00, read index 0x1f09fc, "being written" flag
  0x1f09f8. Returns -1 if 128 entries are pending.
* Drained by `FUN_00200a98`: takes semaphore `DAT_0027a180`, sends every pending entry as a
  blocking RPC `(cmd, a1, a2, a3)`, then calls `FUN_002009f8(60)` (reverb fade-in, below).
* `FUN_00200a98` is called from a small worker loop at 0x209220 (not a Ghidra function): wait on
  semaphore `DAT_0027e720`, skip if `*0x1f0ce0 != 0`, flush, loop. That semaphore is signalled from
  `FUN_00208db8`. [V] It is signalled once per video frame. [I]

So a sound command takes effect on the first flush after the frame that queued it, plus the RPC
latency, plus up to one sequencer update (~16 ms).

### 1.3 Sound init: `FUN_002004b8` -> `FUN_00200250` [V]

Called once from `main` (`FUN_00209eb8`). The asset-table accessors `FUN_00207900(i)` /
`FUN_00207918(i)` return pointer / size of table entry `i-1` (table of `{type, name, ptr, size}` at
0x27b504).

1. Create semaphore `DAT_0027a180`; bind RPC; `cmd 0x5001` (driver reset).
2. `FUN_00200250`:
   * Bind the IOP-heap RPC service 0x80000003 and allocate a **0x10000-byte IOP buffer**
     (`DAT_0027a188`, called `IOPBUF` below).
   * Upload sample bodies through that buffer in 64 KiB chunks (`FUN_00200130`): per chunk
     `sceSifSetDma(src, IOPBUF, 0x10000)` + wait, then `cmd 0x501a (IOPBUF, spu_addr, 0x10000)`
     and `cmd 0x5007` (wait for SPU transfer end). The last chunk is also sent as a full 64 KiB.
     * `SNDBOOTB` (0x56F10 bytes) -> SPU2 RAM **0x005010**
     * `SNDOSDDB` (0xD480 bytes) -> SPU2 RAM **0x085010**
   * Then copy headers/sequences into the same IOP buffer with plain SIF DMA (`FUN_002000c0`); they
     stay there for the lifetime of the OSD:

     | IOPBUF offset | Asset | later registered as |
     |---|---|---|
     | +0x0000 | SNDBOOTH | bank 0 (`DAT_0027a1a0`) |
     | +0x1000 | SNDBOOTS | sequence 0 |
     | +0x2000 | SNDTNNLS | sequence 1 |
     | +0x3000 | SNDCLOKS | sequence 2 |
     | +0x4000 | SNDTM30S | sequence 3 |
     | +0x5000 | SNDTM60S | sequence 4 |
     | +0x6000 | SNDOSDDH | bank 1 (`DAT_0027a1a2`) |
     | +0x7000 | SNDLOGOS | sequence 5 |
     | +0x8000 | SNDWARNS | sequence 6 |
     | +0x9000 | SNDRCLKS | sequence 7 |
3. Back in `FUN_002004b8`, in this order:
   * `cmd 2 (1)` select SPU2 core 1.
   * `cmd 0x500c (0, 4)`, `cmd 0x500c (1, 4)`: reverb mode 4 on core 0 and core 1, reverb enabled.
   * `cmd 0x5006 (IOPBUF, 0x5010)` -> bank id 0; `cmd 0x5006 (IOPBUF+0x6000, 0x85010)` -> bank id 1.
   * `cmd 0x500a (60)`: sequencer update-rate constant.
   * eight `cmd 0x5009 (bank 0, IOPBUF+off)` in the table order above -> sequence ids 0..7 (the
     returned ids are stored at `DAT_0027a190..19e`, but the rest of OSDSYS uses the literal
     numbers 0..7).
   * `cmd 0x5013 (seq, vol)` per sequence: BOOTS **0x42**, TNNLS 0x2A, CLOKS 0x2D, TM30S 0x1B,
     TM60S 0x1B, LOGOS 0x1B, WARNS 0x36, RCLKS 0x36.
   * `cmd 0x5201 (bank 1, 0x1C)`: sound-effect master volume.
   * `cmd 0x5012 (0, 0x3FFF, 0x3FFF)`, `cmd 0x5012 (1, 0x3FFF, 0x3FFF)`: master volume both cores.
   * `cmd 0x5100`: start the sequencer timer.

Shutdown is `FUN_002007e8`: stop (mode 1) and close all sequences, close both banks, `cmd 0x5101`.

Reverb fade-in `FUN_002009f8(60)`: on each of the first 60 queue flushes after init it queues
`cmd 0x500d (core 1, d, d)` with `d = (n * 0x7FFF*20 / 127) / 60`, n = 0..59, i.e. the core-1
reverb return volume ramps linearly from 0 to 5074 (0x13D2, ~15 % of full scale) and then stays.
Until that ramp has run the sequences are dry. [V] (How many of those flushes happen before the
opening starts was not measured.)

### 1.4 The 0x50xx command set [V unless noted]

Arguments are as seen by the IOP (`a1`, `a2`, `a3`); "core" always means "select this SPU2 core as
the driver's *current core* first" (and it stays selected).

| Cmd | Handler | Args | Effect |
|---|---|---|---|
| 0x5001 | snd:1a46c | - | Reset player state (24 sequence slots, 24 voice records, 127 bank slots), SPU init, per core: reverb work-area end address (core 0: 0x1FFFFF, core 1: 0x1DFFFF), reverb return volume 0. Leaves core 1 current. |
| 0x5002 | snd:1a5dc | - | Clear state, SPU quit (release IRQ/DMA handlers). |
| 0x5005 | snd:1a6b4 | bd_ptr, hd_ptr, spu_addr | Check "SShd", upload whole body (size from hd+4) and register bank. Unused by OSDSYS. |
| 0x5006 | snd:1a8fc | hd_ptr, spu_addr | Check "SShd" at hd+0xC; register bank in first free slot `{used, hd_ptr, spu_addr>>3}`; returns bank id or -1. |
| 0x5007 | snd:1aa54 | - | Block until the pending SPU RAM transfer completes. |
| 0x5008 | snd:1aa80 | bank | Free bank slot (fails if a live voice still uses it). |
| 0x5009 | snd:1abf8 | bank, sq_ptr | Check "SSsq" at sq+0xC; open in first free of 24 slots: read offset = 0x110, tempo/resolution from header; returns sequence id or -1. |
| 0x500a | snd:1b1a4 | rate | Set the update-rate constant used in the tempo formula (OSDSYS: 60). |
| 0x500b | snd:1af34 | seq | Close sequence slot (fails while playing). |
| 0x500c | snd:1afd4 | core, mode | Set reverb preset `mode` (work area cleared; this also zeroes the reverb return volume) and enable reverb on that core. |
| 0x500d | snd:1b04c | core, L, R | Set reverb return volume (SPU2 EVOLL/EVOLR). |
| 0x500e / 0x500f | snd:1b0ac / 1b128 | core, v | Set reverb delay / feedback parameter (only meaningful for echo/delay presets), enable reverb for all voices. Unused. |
| 0x5010 | snd:1b1e4 | 1 | Return bitmask of voice records that are active. |
| 0x5011 | snd:1bdcc | size | Zero-fill SPU RAM from 0x5010 for `size` bytes. Unused. |
| 0x5012 | snd:1b2fc | core, L, R | Master volume (MVOLL/MVOLR) of that core. |
| 0x5013 | snd:1b374 | seq, vol(0..127) | Set the sequence's master volume (overwrites byte 0 of the SSsq data in IOP RAM) and re-scale its sounding voices. Returns previous value. |
| 0x5014 | snd:1b5bc | seq | **Play / resume**: sets playing=1, paused=0, stopped=0. Does *not* rewind; position is at the start after open, stop or natural end. |
| 0x5015 | snd:1b6e0 | seq, mode, rr | **Stop / pause**, see below. |
| 0x5016 | snd:1bbb4 | seq, bpm(0..960) | Set tempo. |
| 0x5017 | snd:1bcf4 | seq | Get tempo. |
| 0x5018, 0x5019 | snd:1bd64, 1bd90 | | Stubs (return 0). |
| 0x501a | snd:1a844 | iop_ptr, spu_addr, size | DMA-write `size` bytes to SPU RAM at `spu_addr` (minimum legal address 0x5010). |
| 0x501b | snd:1a8a0 | spu_addr, iop_ptr, size | Same with swapped argument order. Unused. |
| 0x5100 | snd:03838 | - | Start the sequencer timer (section 2.2). |
| 0x5101 | snd:0392c | - | Stop/free the timer. |
| 0x5200 | snd:1be88 | bank, index | **Play sound effect** `index` of the bank's SE table on core 0; returns voice or -1. |
| 0x5201 | snd:1c24c | bank, vol | Set the SE table's master volume (word 0 of the table). |

`cmd 0x5015 (seq, mode, rr)`:

* mode 0 - stop: rewind to 0x110, playing=0, stopped=1, loop counters cleared. Every voice record
  owned by the sequence is marked inactive/released; if `rr != 0` the voice's **release rate is
  replaced by `rr` in linear mode** (ADSR2 low 6 bits = `rr`, exponential bit cleared); then all
  those voices are keyed off. With a linear release the fade lasts at most
  `4096 * 2^(rr-11) / 48000` s from full level: **rr 0x0F = 1.37 s, 0x0E = 0.68 s, 0x11 = 5.46 s**;
  `rr = 0` keeps each tone's own release (boot bank: exponential, time constant ~2.7 s).
* mode 1 - kill: as stop, but ADSR1/ADSR2 are zeroed first, i.e. an immediate cut.
* mode 2 - pause toggle: if playing -> playing=0, paused=1 and voices keyed off (with `rr` as
  above); else if not stopped -> resume.
* mode 3 - same toggle without touching the voices.
* All modes also clear the per-channel vibrato depth.

### 1.5 What the opening's calls mean

| Call | Where | Meaning |
|---|---|---|
| `(0x5014, 0)` | `FUN_00216568`, when boot state `*0x1f05e8 == 1` | Start sequence 0 = **SNDBOOTS**, the normal boot sound. [V] |
| `(0x5014, 6)` | `FUN_00216568`, state 4 | Start sequence 6 = **SNDWARNS** (5.5-minute ambient loop used by the alternative "state 4" opening). [V] |
| `(0x5014, 1)` | `FUN_0021a608`, end of opening, default disc case | Start sequence 1 = **SNDTNNLS**, a 1.25 s four-note transition cue. The boot sequence is *not* stopped; whatever is still sounding rings out under it. [V] |
| `(0x5014, 7)` | `FUN_0021a608`, disc types 0x6c..0x6e | Start sequence 7 = **SNDRCLKS** (nearly the same four notes as TNNLS, on programs 100..102 = copies of 50/52/2 with a faster release); the code then jumps to the rr 0x11 stop two rows below. [V] |
| `(0x5015, 0, 0, 0x0F)` | `FUN_0021a608` | Stop SNDBOOTS, fading its voices out linearly in <= 1.37 s. [V] |
| `(0x5015, 0, 0, 0x11)` | `FUN_0021a608` | Stop SNDBOOTS with a slow linear fade, <= 5.46 s. [V] |
| `(0x5015, 6, 0, 0x0F)` | `FUN_0021a608` case 6 | Stop SNDWARNS with the 1.37 s fade. [V] |

`FUN_00216568` runs every iteration of the opening thread (`FUN_002162c0`), so the play command is
re-queued for as long as the state variable stays 1/4; `FUN_002165a0` in the same loop normally
moves the state on, so in practice it is sent once at the first opening frame [I]. Re-sending it
while the sequence is playing is harmless; re-sending after the natural end would restart it.

Other users of the queue, for reference [V]: `(0x5200, 1, n)` = menu sound effect `n` from bank 1
(indices 0..6 and 10 are used); `(0x5014 / 0x5015, 2, ...)` = start / stop sequence 2 = SNDCLOKS (a
5-minute looping ambience) from menu code at 0x2210a8, 0x2297c0, 0x22e7f0. Other direct RPCs:
`cmd 0x1031` (SPDIF output mode) from `main`, raw Spu* calls from code around 0x20ffa8..0x214530
(not examined).

---

## 2. IOP side (OSDSND)

### 2.1 Module structure [V]

* Entry `snd:00000`: `QueryBootMode(3)` check, then starts an init thread (`snd:000dc`), which
  starts two threads:
  * RPC server thread (`snd:0022c`, priority 0x22): enables interrupts 0x24, 0x28 (SPU DMA) and 9
    (SPU2), registers server 0x80000601 with dispatcher `snd:002c8`, runs `sceSifRpcLoop`.
  * Sequencer thread (`snd:03768`, priority 0x23): `for(;;){ SleepThread(); tick(); }` with
    `tick` = `snd:16c10`.
* Dispatcher `snd:002c8`: big if-tree on the function number; result word at `snd:1cfe8`.
* Timer (`cmd 0x5100`, `snd:03838`): `AllocHardTimer(TC_HLINE, 16 bit, prescale 1)`, compare value
  **0x100**, mode 0x158 (HSYNC clock, reset on compare, repeat compare IRQ). The IRQ handler
  (`snd:037b0`) just `iWakeupThread`s the sequencer thread.
  -> update rate = line rate / 256: **PAL 15625/256 = 61.04 Hz**, NTSC 15734/256 = 61.46 Hz. [I: the
  line-rate figures and "exactly 256" are hardware assumptions]. The tempo maths assumes 60 Hz, so
  everything plays ~1.7 % (PAL) / ~2.4 % (NTSC) faster than the nominal sequence times.

State tables (all zeroed by cmd 0x5001):

* Banks, 0x0C bytes each at `snd:21c10`: `{u32 used, u32 hd_ptr, u32 spu_addr>>3}`.
* Sequence slots, 24 x 0x44 bytes at `snd:22210`:
  `+0 status, +1 running status, +2 data1, +3 data2, +4 s32 countdown (ticks<<12), +8 s32
  per-update decrement, +0xC pending key-on mask, +0x10 pending key-off mask, +0x14 read offset,
  +0x18 sq_ptr, +0x20 stopped, +0x24 bank id, +0x26 open, +0x28 playing, +0x2A..+0x38 loop state,
  +0x3C paused, +0x3E tempo, +0x40 resolution`.
* Voice records, 24 x 0x3C bytes (30 u16) at `snd:21670`; index = SPU2 voice number:
  `[0] active, [1] note, [2] channel, [3] sequence id (0xFFFF = free), [4] released/stealable,
  [5] allocation age, [7] tone index, [8..11],[25] vibrato state, [14] bank, [15] program volume,
  [16] velocity (after curve), [17] tone volume, [18]/[19] pan gain L/R, [20] channel volume,
  [21] sequence volume, [22] fine tune, [23] channel bend, [24] bend range, [26] root note,
  [27] volume-mode byte, [28] channel pan, [29] tone pan`.

### 2.2 Sequencer update (`snd:16c10`), once per timer wake-up [V]

```
free_finished_voices()            # snd:1a208: voice released && ENVX < 3 -> record cleared
for each slot that is open and playing:
    while slot.countdown < 1:
        read event at slot.offset (running status supported)   # snd:1a098
        if note event and channel's program is missing: skip 3 bytes
        else dispatch: 9n note-on, 8n note-off, Bn controller, Cn program, En bend,
                       FF 2F end, FF 51 tempo
        if end: rewind to 0x110, playing = 0, leave loop  (voices are NOT keyed off)
        delta = read MIDI variable-length number            # snd:19f64
        slot.countdown += delta << 12
    collect slot's key-on / key-off masks
    slot.countdown -= slot.decrement          # (resolution * bpm << 12) / rate / 60
SpuSetKey(OFF, all key-off bits); short busy-wait; SpuSetKey(ON, all key-on bits)
vibrato pass over voices (snd:17ad8)
```

With resolution 480, 90 bpm, rate 60 the decrement is exactly 12 ticks per update, so an event at
tick T fires on update `ceil(T/12)`.

Everything goes to the driver's *current core*. After init that is **core 1**, and the SE command
(0x5200) switches to core 0 only for its own duration, so: **sequences play on SPU2 core 1 voices
0..23, menu sound effects on core 0** [V]. SPU init (`snd:1446c`) sets core 1's external-input
volume to 0x7FFF, which is how core 0's output reaches the final mix on SPU2 [I].

Note-on (`snd:171a4`), given channel record `ch` (16 bytes in the SSsq file, section 3.2),
program header `P` and its tones (section 3.1):

* Tone selection by `P[0]`: `0xFF` = drum map (tone index = note - `P[6]`); bit 7 set = *layer*:
  every tone whose `[lo, hi]` range contains the note starts a voice; bit 7 clear = *split*: only
  the first matching tone.
* Voice allocation (`snd:18344`): first free record, else steal the oldest "released" one, else
  drop the note. 24 voices total, shared by all sequences.
* Pitch (`snd:18914`): `idx = 16*semitone_within_octave + 208 + fine + ((bend-64)*bend_range >> 2)`;
  `pitch = (PITCHTAB[idx] * 44100 << octaves_up  or  >> octaves_down) / 48000`.
  `PITCHTAB` (`snd:1dbb0`, 608 u16) is `0x1000 * 2^((i-208)/192)`, i.e. 16 steps per semitone.
  So a tone played at its root note with fine = 0 runs at 44100 Hz; `fine` is in 1/16 semitone.
  (Two entries are typos in the ROM: [64] = 0x0938 instead of 0x0983 and [172] = 0x0D0C instead of
  0x0E0C; neither is reachable without pitch bend.)
* Volume (`snd:18c04`), per side:
  `vol = (((seq_vol * ch_vol * prog_vol * velocity) >> 14) * tone_vol * pan_gain) >> 14`
  with `pan_gain` from a 32-step table (`snd:1e070`, index `(ch_pan + tone_pan - 64, clamped) >> 2`:
  0 = (128, 0) hard left, 16 = (120, 120), 31 = (0, 128) hard right). Maximum is 0x3D89. If the
  tone's volume-mode byte is non-zero the register gets `(vol >> 7) | mode << 8` instead (SPU sweep
  modes; unused in the boot bank).
* SPU2 voice registers written (`snd:1370c`): VOLL, VOLR, PITCH, start address
  `bank_spu_addr + tone.sample_offset` , ADSR1 and ADSR2 **raw from the tone**.
* Reverb send for that voice on/off from tone flag bit 7; key-on bit queued.
* Tone flag bit 0 clear marks the voice "released" immediately (it can be stolen and ignores later
  volume/pan controllers); a note-off still keys it off.

Note-off (`snd:18560`): every active voice with the same sequence, bank, channel and note is marked
released and keyed off (SPU release phase from ADSR2).

Controllers handled: 1 vibrato depth, 2 vibrato rate, 6 loop count, 7 channel volume (live voices
re-scaled), 10 pan (live voices re-panned), 0x40 ignored, 0x63 (NRPN MSB) value 20 = loop start,
30 = loop end (count from controller 6, 127 = forever), 0x62. Any other controller number is *not*
skipped and would desynchronise the stream. Program change also resets the channel's pitch bend to
centre. Pitch bend is a 1-data-byte event (0x40 = centre).

### 2.3 SPU2 setup relevant to the boot sound [V unless noted]

* SPU init (`snd:1446c`): both cores' dry and wet voice mixes fully enabled, MVOL 0, EVOL 0,
  core-1 external input (core 0 output) at 0x7FFF, SPDIF register 0xC032.
* Master volume: 0x3FFF on both cores (cmd 0x5012), fixed.
* Reverb: preset index 4 of the built-in libspu table (`snd:1d2a0`, work-area size 0x6FE0 bytes) on
  both cores. Its coefficients (`00E3 00A9 6F60 4FA8 BCE0 4510 BEF0 A680 5680 52C0 ...`) are the
  classic PS1 "Studio C / studio large" preset [I for the name]. Reverb enable bit set in each
  core's ATTR. Return level on core 1 ramps 0 -> 0x13D2 after init (section 1.3); core 0's stays 0,
  so menu SEs are effectively dry even though most set their reverb flag.
* Per-voice reverb send: tone flag bit 7. In the boot sequence every voice has it except the two
  hard-panned layers of program 0.
* Envelopes: the hardware ADSR generator with the tone's raw register values; the driver adds
  nothing (no software fades) except the release-rate override of cmd 0x5015.

### 2.4 Sound effects (bank 1), for completeness [V]

`cmd 0x5200 (bank, i)` (`snd:1be88`): table at `hd + hd[0x2C]`: `u32 master_vol, u32 last_index`,
entries of 0x40 bytes starting at table+0x20: `u16 volL, u16 volR, u16 pitch (0x1000 = 44.1 kHz),
u16 ?, u16 ADSR1, u16 ADSR2, u16 ?, u16 flags (bit 7 reverb), u32 sample byte offset`. Voice
chosen on core 0 by lowest envelope level (`snd:1c304`), registers
`vol = entry.vol * master_vol >> 7`, `pitch * 44100 / 48000`, key-on immediately, then core 1 is
re-selected. SNDOSDDH has 14 entries over 7 samples (`snd_dump.py .../SNDOSDDH`).

---

## 3. Data formats

All little-endian. `tools/snd_dump.py` prints everything below.

### 3.1 SShd bank header (SNDBOOTH, 0x4CA bytes + 2 pad) [V]

| Offset | Field |
|---|---|
| 0x00 | u32 header size (0x4CA; 0xFFFFFFFF in SNDOSDDH). Not read by the driver. |
| 0x04 | u32 body size (0x56F10). Only read by the unused cmd 0x5005. |
| 0x0C | "SShd" |
| 0x10 | u32 offset of program table (0x80), or -1 |
| 0x14 | u32 offset of velocity table (0x448), or -1 |
| 0x18 | u32 offset of vibrato waveform table, or -1 (absent in both banks) |
| 0x2C | u32 offset of sound-effect table, or -1 (only SNDOSDDH: 0x80) |
| other header words | 0xFFFFFFFF, never read |

Program table (at 0x80): `u16 max_program (0x66 = 102)`, then `u16 offset[max+1]` relative to the
table start (0xFFFF = no such program), then the program records.

Program record = 8-byte header + 16-byte tones:

| Byte | Header field |
|---|---|
| 0 | mode/count: 0xFF drum map; else bit 7 = layer (all matching tones) vs split (first match), low 7 bits = tone count - 1 |
| 1 | program volume 0..127 |
| 6 | drum-map base note (only for mode 0xFF) |
| 2..5, 7 | not read by the driver (always `40 00 0C 7F .. 00`; look like pan / bend-range defaults) [I] |

| Byte | Tone field |
|---|---|
| 0, 1 | lowest / highest note |
| 2 | root note (plays at 44.1 kHz) |
| 3 | s8 fine tune, 1/16 semitone |
| 4..5 | u16 sample offset / 8 (byte offset in the body = value * 8) |
| 6..7 | u16 SPU ADSR1 (attack mode+rate, decay rate, sustain level) |
| 8..9 | u16 SPU ADSR2 (sustain mode/dir/rate, release mode/rate) |
| 10 | volume-mode byte (0 = plain volume) |
| 11 | tone volume 0..127 |
| 12 | tone pan 0..127 (64 centre) |
| 13 | pitch-bend range (12 everywhere) |
| 14 | vibrato waveform index (0x7F everywhere, unused) |
| 15 | flags: bit 0 sustained voice, bit 5 vibrato enable, bit 7 reverb send |

Velocity table (at 0x448): 2 bytes (0) + 128-byte curve; identity in SNDBOOTH.

SNDBOOTH content: 17 programs (0, 1, 2, 40, 41, 42, 50..53, 60..62, 70, 100..102), 37 tones, 9
distinct samples. Programs used by the boot sequence:

| Prog | Mode | Vol | Tones: root / fine / sample / vol / pan / ADSR1,ADSR2 / flags |
|---|---|---|---|
| 0 | layer x3 | 115 | A#4 -7 / 0x31180 / 112 / L / AAFF,D130 / sustain; A#4 -9 / 0x31180 / 127 / R / AAFF,D130 / sustain; F4 -8 / 0x31180 / 120 / C / AAFF,D030 / sustain+reverb |
| 1 | split x1 | 100 | A#4 -8 / 0x31180 / 100 / C / B9FF,D130 / sustain+reverb |
| 50 | layer x2 | 100 | D#5 -10 / 0x1C230 / 112 / L / 3FFF,5FF0; E5 -4 / 0x1C230 / 127 / R / 3FFF,5FF0 (both sustain+reverb) |
| 51 | layer x3 | 100 | F5 -10 / 0x1FC30 / 100 / L; F#5 -14 / 0x1FC30 / 127 / R; G5 -4 / 0x1FC30 / 100 / C; all 3CFF,5FF0, sustain+reverb |
| 53 | layer x2 | 100 | F5 -6 / 0x1FC30 / 100 / L; F#5 -12 / 0x1FC30 / 127 / R; 3CFF,5FF0, sustain+reverb |
| 61 | layer x2 | 127 | D4 0 / 0x00000 / 127 / R / C6FF,DFF0; D4 +2 / 0x00000 / 115 / L / AFE0,CFAF; reverb, not "sustain" |
| 62 | layer x2 | 127 | mirror image of 61 (the AFE0 tone on the right, the C6FF tone on the left) |

Layers of one program use the same sample with slightly different root/fine values and opposite
pans, i.e. each note is a detuned stereo pair/trio.

ADSR readings (SPU2 at 48 kHz; attack = time to full level, computed with the standard SPU envelope
rules) [I for the absolute times]:

| Value | Meaning |
|---|---|
| ADSR1 AAFF | exponential attack ~0.12 s, no decay stage (sustain level = max) |
| ADSR1 B9FF | exponential attack ~1.6 s |
| ADSR1 3FFF / 3CFF | linear attack ~2.7 s / ~1.6 s |
| ADSR1 C6FF | exponential attack ~15 s (never completes; a slow swell) |
| ADSR1 AFE0 | exponential attack ~0.3 s, then decay to the lowest sustain level |
| ADSR2 D130 / D030 | sustain slowly decreasing (exponential), release exponential rate 0x10 (time constant ~2.7 s) |
| ADSR2 5FF0 / DFF0 | sustain effectively flat, release exponential rate 0x10 |
| ADSR2 CFAF | sustain decreasing, release exponential rate 0x0F (~1.4 s) |

### 3.2 Sample body (SNDBOOTB, 0x56F10 bytes) [V]

Headerless concatenation of PS ADPCM streams: 16-byte blocks = `byte0: shift (low nibble), filter
(high nibble); byte1: flags (1 = end, 2 = loop/repeat, 4 = loop start); 14 bytes = 28 4-bit
samples`. A sample starts at `tone offset * 8` and ends at the first block with flag bit 0; each is
followed by 16 padding bytes. Uploaded verbatim to SPU2 RAM at 0x5010, so SPU address =
0x5010 + body offset.

| Body offset | Length @44.1 kHz | Loop | Used by programs | Signal character (statistics only, not auditioned) [I] |
|---|---|---|---|---|
| 0x00000 | 3.909 s | one-shot | 60, 61, 62 | inharmonic cluster 440..970 Hz, swells for ~1.5 s then decays ~30 dB: a struck/bowed-metal texture |
| 0x180E0 | 0.663 s | yes (whole) | 52, 101 | steady broadband loop |
| 0x1C230 | 0.589 s | yes | 50, 100 | steady tonal cluster around 500..650 Hz |
| 0x1FC30 | 0.911 s | yes | 51, 53 | steady low drone, energy below 110 Hz |
| 0x255E0 | 0.887 s | yes | 2, 40, 102 | white noise |
| 0x2AD30 | 1.004 s | yes | 42 | noisy loop with low rumble |
| 0x31010 | 0.015 s | yes | 41 | single-cycle-style tone (~1.4 kHz) |
| 0x31180 | 4.319 s | yes (last 0.47 s) | 0, 1 | tonal pad, partials 414..833 Hz + 2.5 kHz, slowly decaying, then loops |
| 0x4BAB0 | 1.832 s | yes (from 1.03 s) | 70 | tonal, partials 666 Hz / 1.77 kHz / 2.67 kHz |

### 3.3 SSsq sequence (SNDBOOTS, 0x1CC bytes) [V]

| Offset | Field |
|---|---|
| 0x00 | u8 sequence master volume (file: 100; replaced at runtime by cmd 0x5013 -> 0x42 for the boot sound) |
| 0x02 | u16 resolution, ticks per quarter note (480) |
| 0x04 | u16 initial tempo, bpm (90) |
| 0x0C | "SSsq" |
| 0x10 | 16 channel records x 16 bytes (live state, modified in place while playing) |
| 0x110 | event stream |

Channel record: `[1] channel number, [2] program (0xFF = unused), [3] volume, [4] pan, [9] vibrato
depth, [10] pitch bend (file value 0; set to 0x40 by a program change), [11] set to 0x40 by a
program change (otherwise unused), [12] vibrato rate (0x10)`; `[14] = 0x64` and the rest are not
read [V for the listed ones].

Event stream: the first event starts at 0x110 **without** a delta; after every event comes a MIDI
variable-length delta (ticks) to the next one. Events are MIDI-like with optional running status:

| Bytes | Event |
|---|---|
| `8n kk vv` | note off |
| `9n kk vv` | note on (vv = 0 -> note off) |
| `Bn cc vv` | controller (supported set in section 2.2) |
| `Cn pp` | program change |
| `En vv` | pitch bend, one data byte, 0x40 = centre |
| `FF 51 lo hi` | tempo in bpm, u16 |
| `FF 2F 00` | end of sequence |

One tick = 60 / (bpm * resolution) s = 1.3889 ms at 90 bpm / 480.

Decoded SNDBOOTS (set-up events condensed; full list from `tools/snd_dump.py`):

| Tick | Nominal s | Event |
|---|---|---|
| 0 | 0.000 | tempo 90; programs: ch0=0, ch4=50, ch3=51, ch2=62, ch1=61 |
| 1..3 | 0.001..0.004 | volumes ch0=120, ch1=127, ch2=127, ch3=100, ch4=127; all pans 64 |
| 4 | 0.006 | ch4 note on C4 (60) vel 127 |
| 10 | 0.014 | ch2 note on D3 (50) vel 60 |
| 23 | 0.032 | ch1 note on G2 (43) vel 80 |
| 480 | 0.667 | ch3 note on F4 (65) vel 60; ch0 note on C4 (60) vel 127 |
| 484 | 0.672 | ch4 note off C4 |
| 960..963 | 1.333..1.337 | programs ch6=1, ch5=53; volumes ch5=100, ch6=110; pans 64 |
| 1440 | 2.000 | ch3 note off F4 |
| 2400 | 3.333 | ch0 note off C4 |
| 2423 | 3.365 | ch1 note off G2 |
| 2880 | 4.000 | ch6 note on G3 (55) vel 60 |
| 3130 | 4.347 | ch2 note off D3 |
| 3360 | 4.667 | ch1 note on B3 (59) vel 25 |
| 4560 | 6.333 | ch5 note on C4 (60) vel 50 |
| 5280 | 7.333 | ch5 note off C4 |
| 6000 | 8.333 | ch1 note off B3 |
| 6600 | 9.167 | ch6 note off G3; end of sequence |

Only 8 notes, which expand to 17 SPU voices (at most 12 at once). No loops, no pitch bend, no
vibrato, a single tempo.

The other sequences use the same grammar (all eight parse cleanly to their `FF 2F`): TNNLS/RCLKS 4
notes in 1.2 s, LOGOS/TM30S/TM60S one 0.67 s note, CLOKS (120 bpm) and WARNS (110 bpm) 126 notes
over 300 s / 327 s with an NRPN loop.

---

## 4. Boot sound timeline

Two time columns: *nominal* = sequence time at an ideal 60 Hz update; *PAL* = update index /
61.04 Hz (section 2.1) [I]. Add the trigger latency of section 1.2. Voice details are from
`tools/snd_render.py` (driver formulas [V]; envelope timing by SPU rules [I]; reverb not modelled).

| Nominal s | PAL s | What starts / stops |
|---|---|---|
| 0.01 | 0.02 | **ch4, program 50, C4, vel 127**: loop 0x1C230 as a hard-left/hard-right pair played ~15 semitones below root (17.9 / 17.2 kHz playback). Linear 2.7 s attack, so it only fades in. |
| 0.01 | 0.02 | **ch2, program 62, D3, vel 60**: one-shot 0x00000 an octave down (22 kHz, lasts 7.8 s) as L/R pair: one side with a 0.3 s attack that decays away by ~2.5 s, the other a very slow swell. |
| 0.03 | 0.03 | **ch1, program 61, G2, vel 80**: same one-shot 19 semitones down (14.7 kHz, 11.7 s long), mirrored L/R pair. Low rumble bed. |
| 0.67 | 0.66 | **ch0, program 0, C4, vel 127** - the main "chord": pad 0x31180 as three voices, L and R at ~24 kHz plus a centre voice a fourth higher (32 kHz) with reverb; fast 0.12 s attack. Loudest element; the mix peaks between 0.75 s and 1.0 s. |
| 0.67 | 0.66 | **ch3, program 51, F4, vel 60**: low drone 0x1FC30 x3 (L / R / centre, ~19..21 kHz), 1.6 s linear attack. |
| 0.67 | 0.67 | ch4 note off (reached only ~24 % of its level) -> exponential release, audible to ~4.6 s. |
| 2.00 | 1.97 | ch3 (drone) note off -> slow release. |
| 3.33 | 3.28 | ch0 (main pad) note off -> release, time constant ~2.7 s. |
| 3.37 | 3.31 | ch1 (G2 rumble) note off. |
| 4.00 | 3.93 | **ch6, program 1, G3, vel 60**: pad 0x31180 again, single centre voice with reverb at 18 kHz (a fourth below the main chord), 1.6 s attack: quiet second swell. |
| 4.35 | 4.28 | ch2 (D3) note off. |
| 4.67 | 4.59 | **ch1, program 61, B3, vel 25**: one-shot 0x00000 only 3 semitones below root (37 kHz, lasts 4.6 s), L/R pair; the highest-pitched use of that sample, and quiet. |
| 6.33 | 6.23 | **ch5, program 53, C4, vel 50**: drone 0x1FC30 L/R pair, quiet. |
| 7.33 | 7.21 | ch5 note off. |
| 8.33 | 8.19 | ch1 (B3) note off. |
| 9.17 | 9.01 | ch6 note off; sequence ends (slot stops, rewinds). Voices still in release ring on (ch6 and ch5 for several more seconds) unless OSDSYS stops the sequence. |

Loudness of the dry render (RMS, dBFS): rises from -44 dB at 0 s to **-18 dB at 0.75..1.25 s**, then
decays roughly 3..4 dB per second: -23 dB at 3 s, -28 dB at 4 s, -30 dB at 6 s, -36 dB at 8 s,
-39 dB at 9.2 s, -47 dB at 11 s. The render does not clip (peak 16406 of 32767).

How it ends in practice (section 1.5): at the end of the opening OSDSYS either starts the TNNLS
transition cue and lets the boot voices decay naturally, or sends stop with release rate 0x0F
(linear fade <= 1.37 s) or 0x11 (<= 5.46 s) depending on the disc state.

---

## 5. Not determined

* Exact relationship between video frames and the timer: whether the HSYNC counter period is 256
  or 257 lines, and the line rate OSDSYS's video mode actually produces. The "PAL s" column assumes
  15625 Hz / 256.
* When, in opening-animation frames, the play/stop commands are issued (the conditions in
  `FUN_0021a608` / `FUN_002165a0` belong to the opening state machine and were only skimmed), and
  how far the reverb fade-in has progressed when the boot sound starts.
* Reverb is identified by preset coefficients only; its audible contribution was not simulated.
* Program header bytes 2..5 and 7, tone byte 14's waveform table, SE entry words 3 and 6, and
  channel record byte 14 are never read by the code paths examined.
* Nothing was listened to: sample descriptions are from spectra/envelopes of the decoded WAVs.
* The generic libspu2/libsnd2 RPC commands (0x0001..0x4070, 0x6xxx..0x8xxx, 0xE621) were only
  mapped as far as OSDSYS uses them.
