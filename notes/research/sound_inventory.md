# Complete sound inventory of the supplied BIOS corpus

Thinker: Codex. Room: `.worktrees/ideas/snd-inventory`. Survey date: 2026-10-11. `bios/` and `repo/` were read only; the room contains research helpers, not product changes.

**[V] Result: five byte-distinct complete banks, of which two are native instrument banks with 21 programs; three are effect-bank variants. There are 21 byte-distinct referenced ADPCM sample streams, including one standalone test VAG, and nine byte-distinct stored scores. [V] A tenth musical item is the PS2 logo chord assembled by executable code from five effects. [I] The music maker should offer the nine scores and that chord recipe, with aliases and provenance.**

## Evidence, scope, and counting rules

`[V]` means bytes read in a supplied dump, including an explicitly located decompressed view, or a calculation from those bytes. `[I]` means a role interpretation, playback/hardware assumption, prior research not independently confirmed here, or an API proposal. An offset in a decompressed view is **not** an offset in its compressed input. `inventory.tsv` records both the view coordinate and the input locator. All offsets and sizes are hexadecimal in this report unless a decimal unit is stated.

[V] The sweep covers every regular input file, every ROMDIR member, nested `*IMAGE` archives, the known PS1/POPS/LOGO LZ loaders, plausible raw LZ modules including 1.00's `MOPEN`, `MCLOCK`, and `MBROWS`, ELF loader streams, and all 40 valid EROM gzip members. Signature candidates were parsed and rejected when their structure or sample boundaries failed. Full logical header-plus-body bytes were compared after hashing locally; a hash match was also checked with byte equality. **No Sony-data hashes are emitted in this report or TSV.** `B`, `A`, `S`, `Q`, and `E` identifiers are room-local cluster labels, not portable fingerprints.

[V] The usable sound corpus is 24 files: nine PS2 rom0 file occurrences covering six ROMVER identities, fourteen valid standalone PS1/PSone files, and one POPS file. A fifteenth standalone PS1 file is damaged and is treated separately. Duplicate filenames/dumps remain separate provenance occurrences, not extra content. The ROMVER identities are `0100JC20000117`, `0150AD20001228`, `0160EC20011004`, `0160AC20020207`, `0160AC20020319`, and `0200EC20040614`. The filename calling the 1.00 J dump “v0.1” does not change its embedded `0100` identity.

[V] The final parser accepts 168 bank/sample-container/score occurrences and rejects 85 signature candidates. It finds **no `pQES` or `SEQp` PS1 SEQ candidates**. The many literal `SSsq`, `SShd`, and `pBAV` strings visible inside compressed input are not additional plaintext assets. The accepted PS2 `SSsq` occurrences are 72: eight per rom0 file occurrence, with seven content clusters. There are 38 PS1 note-table occurrences: 24 licence tables and 14 SCE intro tables.

[V] More explicitly, there are **27 actual SShd bank occurrences but only four complete SShd content clusters**, plus the one common VAB cluster. The independent backward scan of structurally valid time-9999-terminated note tables finds the same 38 PS1 tables and no additional table. Thus the earlier fixed-window counts are not counts of distinct parsed assets.

[V] An offline room-local Rust validator using `ps2kit` accepts all **123 bank and SSsq occurrences**, confirms every SSsq ending update, and independently checks decoded length and looping for all **21 distinct ADPCM streams**. See `validation.txt`. The Python parser adds effect-table extent handling, strict ADPCM block validation, VAB declared/derived length equality, and an explicit sequence terminator requirement. The existing Rust instrument constructor alone does not enumerate direct effect entries.

[I] “Complete” here is the complete inventory recoverable through those identified sound formats and exposed containers. A signature-negative result cannot prove the absence of arbitrary unmarked PCM, executable synthesis, or an unknown compressed format. The DVD-player negative finding below is correspondingly bounded. No hardware audio capture or listening test was made; duration is a computed clock/sample quantity, not an observed console recording.

## Banks, sample sets, and exact pairing

The following bank facts and membership are [V]; descriptive audition labels are [I]. “Seconds” sums each referenced sample offset once, decoding through its first ADPCM end flag, at 44.1 kHz. A looping sample's stored traversal is finite; its possible sounding duration is unbounded. Complete bank sizes include the declared body, including its guard/padding blocks, and exclude padding between the header and body.

| Cluster | Suggested name | Header + body bytes | Programs / tones / effects | Samples | Stored sample seconds | Supplied first → last |
|---|---|---:|---:|---:|---:|---|
| B01 | PS2 game-logo chord bank | `0x1E0 + 0x2C90 = 0x2E70` | 0 / 0 / 5 | 1 | 0.452063 | PS2 1.00 J → 2.00 E; all six identities |
| B02 | PS1 shell instrument bank | `0x1220 + 0xA860 = 0xBA80` | 4 / 8 / 0 | 3 | 1.710476 | PS1 1.0 J → PSone 4.5 A; PS2 1.00 J → 2.00 E; POPS 660 also |
| B03 | PS2 boot and ambience instrument bank | `0x4CA + 0x56F10 = 0x573DA` | 17 / 39 / 0 | 9 | 14.130794 | PS2 1.00 J → 2.00 E; all six identities |
| B04 | PS2 OSD effects, 14 entries | `0x420 + 0xD480 = 0xD8A0` | 0 / 0 / 14 | 7 | 2.154286 | PS2 1.50 A devkit → 2.00 E; all five later identities |
| B05 | PS2 OSD effects, 12 entries | `0x3A0 + 0xD480 = 0xD820` | 0 / 0 / 12 | 7 | 2.154286 | PS2 1.00 J only; both duplicate rom0 files |

[V] B04 and B05 have **byte-identical entire bodies and byte-identical first twelve 64-byte effect records**. Their table last-index values are 13 versus 11, both with stored master volume 100. B04 adds effect indices 12 and 13, both using the already-present sample at body `+0xA890`; it adds no sample. B05 must retain a separate complete-bank cluster but does not add twelve new instrument/effect choices. The 14-entry bank already exists in the 1.50 devkit: it is not 2.00-only.

[V] The unique sample pool totals **18.491952 stored seconds**: B01 + B02 + B03 + B04 + S01, without counting B05's shared samples again. All 21 streams differ in their parsed ADPCM bytes. Matching a sample does not make its program or effect equivalent: tuning, envelope, volume, pan, layering and reverb can differ.

[V] Coverage of every identified bank body is complete: after merging all referenced sample extents, the only uncovered bytes are 16-byte ADPCM guard blocks (one in B01, one in B03, seven in each shared OSD body); B02 and S01 have no uncovered body bytes. There is no unreferenced extra sample hiding in any of these parsed bodies. This check does not extend the negative claim to unmarked data elsewhere in a ROM.

### `SShd` boundaries

[V] B03 has a real header length at header `+0x00` and body length at `+0x04`. For B01/B04/B05, `+0x00` is **`0xFFFFFFFF`**, not a four-gigabyte header and not evidence of corruption. These banks have no program/velocity/LFO tables; their effect table pointer at `+0x2C` is `0x80`. Its `+4` word is the last effect index, so the true header extent is `0x80 + 0x20 + (last + 1) * 0x40`. This gives `0x1E0`, `0x420`, and `0x3A0` respectively. The complete tables fit exactly in the named header payloads where those exist.

[V] Later OSD banks use **named pairs**: `SNDIMAGE/SNDBOOTH` with `SNDBOOTB`, and `SNDIMAGE/SNDOSDDH` with `SNDOSDDB`. Their ROMDIR entries are individually LZ-decoded. The complete bank fingerprint uses decoded header plus exactly the declared body, not the archive's compressed bytes. The driver does not guess pairing by proximity: the EE uploads BOOTB to SPU2 address `0x5010` and OSDDB to `0x85010`, copies the headers to IOP memory, and registers those headers against those addresses with RPC command `0x5006`.

[V] In 1.00 J, both banks are inline in **OSDSYS**, not OSDSND. The plain OSDSYS ELF is at rom0 file `+0xECF70`. The B03 header is module file `+0x22580` / absolute file `+0x10F4F0` / VA `0x221580`; its body is module `+0x22A80` / file `+0x10F9F0` / VA `0x221A80`. The B05 header is module `+0x7A680` / file `+0x1675F0` / VA `0x279680`; its body is module `+0x7AA40` / file `+0x1679B0` / VA `0x279A40`. Each body starts at `header_start + align_up(parsed_header_size, 64)`. All referenced samples parse cleanly there, and B03 is exactly equal to the later named BOOT pair. The init/open calls are at OSDSYS VA `0x20047C`, `0x20049C`, and `0x2004CC..0x20059C`; its SPU addresses remain `0x5010` and `0x85010`.

[V] B01 is inline in every **PS2LOGO** build. It follows its derived `0x1E0` header at `header_start + 0x200`; the `0x20` gap is alignment, not bank content. Its body has `0x2C90` bytes; the sample ends after `0x2C80`, leaving a 16-byte guard. **2.00 uses a compressed executable; after unpacking, its bank is byte-identical to all earlier builds.** No new B01 program bank appears in 2.00.

| ROMVER | PS2LOGO header / body in module file or decoded view | Header / body VA | View |
|---|---|---|---|
| 0100JC20000117 | `+0x26100 / +0x26300` | `0x225100 / 0x225300` | plain ELF file, multiple PT_LOAD segments |
| 0150AD20001228 | `+0x391C0 / +0x393C0` | `0x1381C0 / 0x1383C0` | plain ELF file |
| 0160EC20011004 | `+0x2BC40 / +0x2BE40` | `0x12AC40 / 0x12AE40` | plain ELF file |
| 0160AC20020207 | `+0x2BC40 / +0x2BE40` | `0x12AC40 / 0x12AE40` | plain ELF file |
| 0160AC20020319 | `+0x2BC40 / +0x2BE40` | `0x12AC40 / 0x12AE40` | plain ELF file |
| 0200EC20040614 | decoded `+0x2B5C0 / +0x2B7C0` | `0x12B5C0 / 0x12B7C0` | LZ input at rom0 file `+0x32CCE0`, consuming `0x18C5B` bytes |

[V] In the 2.00 decoded PS2LOGO, VA `0x1073E8..0x107458` copies those exact body/header pointers with lengths `0x2C90` and `0x1E0`. Init at `0x107458` then registers them with `0x5005(body, header, 0x5010)`. VA `0x1075C8..0x10763C` issues five `0x5200(bank, effect_index)` commands, indices 0 through 4. This is the actual game-logo chord; the asset named `SNDLOGOS` is a different OSD sequence.

### VAB and standalone VAG

[V] B02 is a version-6 `pBAV` with declared length `0xBA80`. The header is 32 bytes, followed by 128 16-byte program records, four 512-byte tone blocks, and the 512-byte VAG length table: body starts at `+0x1220`. Entries 1..3 declare lengths `0x34A0`, `0x1CA0`, and `0x5720` in eight-byte units; they sum to `0xA860`, exactly matching the declared whole-bank extent. It has four populated programs (0..3), two tones each. The same **complete header and body** occurs in every valid standalone PS1 shell, POPS shell, and PS2 `LOGO` shell, including the devkit. A `pBAV` literal inside a compressed LOGO or packed PS1 shell is not itself another valid VAB.

[V] There is one standalone VAG cluster, **S01**, named `1KSine.48`, in the rom0 `TESTSPU` IOP module. Its big-endian VAG version is **3**, its header is `0x30`, its declared payload is `0x4C0`, and its declared sample rate is **48,000 Hz**. It decodes to 2,128 samples / **0.044333 s**, with a loop; the complete container is `0x4F0` bytes. It exists from the supplied 1.50 devkit through 2.00, and is absent from the supplied 1.00 rom0s. Its file/VA locations are included in the TSV and generated appendix. [I] The label and TESTSPU context support “1 kHz manufacturing-test sine”; this is a diagnostic tone rather than a soundtrack.

[V] The `[h]` SCPH-5000 file is 526,083 bytes rather than 524,288, has 1,794 LF bytes all preceded by CR, and has an apparent `pBAV` at file `+0x543A2`. Its header declares `0xBA80` but the shifted length table derives `0xD280`; it fails validation. Removing every CR immediately before LF still does **not** reproduce the good Japanese 2.2 file byte-for-byte. This inventory does not turn damaged bytes into an invented new bank. [I] Show this dump as damaged/unavailable and use the user's separate valid 2.2 J dump for that bank. It is a unique input anomaly, not a unique verified sound.

## Every stored score, its bank, and its role

[V] All eight named SSsq assets are present unchanged in all six supplied PS2 ROMVER identities, including 1.00's inline set. They all play on **B03, bank slot 0**. In 2.00, the sound asset name table is at decoded OSDSYS VA `0x27B504`, with BOOTH/B at indices 8/9, BOOTS through TM60S at 10..14, OSDDH/B at 15/16, LOGOS/WARNS/RCLKS at 17..19. The copy code at `0x200250..0x2004B4` maps the sequence assets into IOP-buffer offsets `0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x7000, 0x8000, 0x9000`. Calls at `0x2005D8..0x2006AC` register **all eight** with the same bank-id variable via `0x5009`. OSD bank slot 1, B04/B05, is for direct effects, not these sequences. Sequence bytes carry no self-contained bank identity.

[V] The SSsq true extent is the 16-byte header, sixteen 16-byte channel records, and event stream through `FF 2F 00`. Post-terminator allocation/alignment bytes are excluded. All these sequences use resolution **480 ticks per quarter note**. “Channels” below are zero-based channels that actually have note-ons, not the sixteen allocated records. Nominal seconds integrate delta ticks and tempo. Driver seconds are the repository's 20.12 fixed-point scheduler at nominal 60 updates/s; they include rounding to the next update. Neither column includes sample release/reverb tails or any external stop command.

| Cluster / aliases | Bytes | Nominal seconds / 60-Hz driver seconds | BPM | Note channels | Note-ons | Actual programs used | Proposed role and evidence |
|---|---:|---:|---:|---|---:|---|---|
| Q01 `SNDBOOTS`, slot 0 | `0x1CC` | 9.166667 / 9.166667 | 90 | 0..6 | 8 | 0,1,50,51,53,61,62 | [V] Normal boot opening: 2.00 VA `0x216590` tail-calls the queue with `(0x5014,0)` when opening state is 1. |
| Q02 `SNDTNNLS`, slot 1 | `0x1A2` | 1.250000 / 1.250000 | 90 | 0,1,2 | 4 | 2,50,52 | [V] Opening-to-menu transition: play calls at `0x21A880` and `0x21A940`. [I] “Tunnel transition” is a useful label. |
| Q03 `SNDCLOKS`, slot 2 | `0x64C` | 300.000000 / 300.000000 to stored end | 120 | 0..6 | 126 | 40,42,50,51,52,60,70 | [V] Clock/main-menu ambience: play calls at `0x2299CC`, `0x229B44`, and `0x22E94C`; stop calls in `0x2210A8`. Infinite loop; see below. [I] “Browser ambience” is a broad colloquial label; the verified start sites are in the clock/main-menu code, not the separate card-browser thread. |
| Q04 `SNDTM30S`, slot 3; `SNDLOGOS`, slot 5 | `0x130` | 0.672222 / 0.683333 | 90 | 1 | 1 | 60 | [V] Two registrations of **identical complete sequence bytes**. [I] “30-second timer / OSD logo cue”; their names suggest intended roles, but no actual play call was found. |
| Q05 `SNDTM60S`, slot 4 | `0x130` | 0.672222 / 0.683333 | 90 | 1 | 1 | 70 | [V] Registered as slot 4. [I] “60-second timer cue”; no actual play call was found. The sound itself lasts under a second, not 60 seconds. |
| Q06 `SNDWARNS`, slot 6 | `0x64D` | 327.272727 / 327.283333 to stored end | 110 | 0..6 | 126 | 40,42,50,51,52,60,70 | [V] Alternative/warning opening: `0x216584` queues `(0x5014,6)` when opening state is 4. [I] User label “Warning ambience”. Infinite loop. |
| Q07 `SNDRCLKS`, slot 7 | `0x1A2` | 1.194444 / 1.200000 | 90 | 0,1,2 | 4 | 100,101,102 | [V] Transition selected for disc types `0x6C..0x6E` at `0x21A8FC`; boot is then stopped with release `0x11`. [I] “CD-media transition”; it is not another five-minute clock loop. The expansion of “RCLK” is unestablished. |

[V] Every sequence's only tempo value is its initial BPM; the tempo events repeat that value rather than introducing a second tempo. Q03 sets NRPN MSB controller 99 to 20 at tick 18,240 (19.000 s), sets data-entry controller 6 to **127**, and closes the loop with NRPN 30 at tick 288,000. Q06 similarly starts its loop at tick 17,280 (19.636364 s), sets 127, and closes at 288,000. The OSDSND controller logic treats 127 as forever (IOP code at VA `0x19B9C`, ELF file `+0x19C3C`). Therefore the console score has **no finite natural end**: Q03 has roughly a 19-second introduction followed by a 281-second repeated region; Q06 roughly 19.636 seconds plus a 307.636-second repeated region. The listed stored ends are one linear traversal, not the native soundtrack length. [I] Actual loop wall time also depends on driver update timing and the precise saved event position.

[V] A scan of direct and register-indirect command sites in each decoded OSDSYS and 1.00 plug-in recovers play uses of slots 0,1,2,6,7. In 1.00, `MOPEN` play sites are VA `0x500374` (slot 6), `0x500388` (slot 0), `0x5041A8` (slot 7), and `0x5041EC` (slot 1); `MCLOCK` uses slot 2 at `0x6006A0`. The loader registers the same inline eight sequences. **The “timers 30/60 s” names are not proof of active console timer sounds.** [I] Treat slots 3/4/5 as recoverable, registered cues with unverified trigger roles. The bounded constant scan is not an exhaustive reachability proof for runtime-generated commands; do not claim these cues can never be played. Their existence does not establish a hidden timer feature.

[V] `SNDLOGOS`/Q04 is not PS2LOGO's B01 chord: the former references B03 program 60 and is registered in OSDSYS; the latter invokes five B01 effect entries directly. Merging them by the human word “logo” would erase a real sound.

### PS1 note-table scores

[V] Q08 is the **licence-screen note table**, 19 events (10 note-ons), true extent `0xA0` including the eight-byte record whose time field is 9999. It is byte-identical across all 24 valid shell locations, including POPS and all PS2 `LOGO` builds, and uses B02 programs 0 and 3. Its last timestamp is 50, giving **0.833333 s at the nominal 60-field clock** from the start of that table. It is a custom `{u32 field, program, note, velocity, pan}` table, not a MIDI/SEQ stream; there is no BPM or MIDI-channel field.

[V] Q09 is the **Sony Computer Entertainment intro**, 40 events (20 note-ons), extent `0x148` including the time-9999 record, byte-identical in all 14 standalone PS1/PSone locations. It uses B02 programs 1 and 3. Last table time is 265: **4.416667 s if imported on an unmodified nominal 60-field table clock**. It is absent from both the supplied POPS shell and every PS2 LOGO shell. This absence is verified by scanning each executable shell's note tables and the expected first record, rather than looking only in compressed file bytes.

[I] A faithful “PlayStation licence sting” preset should include the code-issued drone/key-off prelude, not just Q08. Existing `ps1.rs::scheduled_sound_events` issues program 2/note 31 and program 0/note 36 at the fade start, keys them off after 31 fields, then starts Q08. At nominal NTSC timing that makes the final table event **(31 + 50)/60 = 1.350 s** from the fade's start, before release/reverb tails. PAL scales table fields by 5/6 and uses its video clock, rather than a MIDI tempo. The drone prelude is repository-recovered behavior, not an extra stored SEQ; keep its evidence separate from the verified table bytes.

[I] Q09's native clock also differs from simply waiting for timestamp 265: the known shell fires its t=0 sweep before the visual intro, jumps its clock to 89 NTSC / 74 PAL, and advances the arpeggio while the diamond scene runs. Existing `notes/research/ps1_shell_scenes.md` gives about 249/209 display fields (4.15/4.18 s) for that visual intro, and early 1.0 has a shorter initial black phase. These are prior code interpretations, not measured sample durations in this survey. Offer both “table import” and a console timing recipe; do not silently present 4.416667 s as the exact audible coldboot length.

### The tenth musical item: PS2 game-logo recipe

[V] R01 is the five B01 effects keyed by the 116-byte function at decoded 2.00 PS2LOGO `+0x75C8` / VA `0x1075C8`, with calls at `0x1075DC`, `0x1075F0`, `0x107604`, `0x107618`, and tail jump `0x107634`. There is no accompanying SSsq sequence. The effects share one 0.452063-second source sample but run with five different voices: pitches 3090, 3046, 2298, 4096, 3068 (SPU units), with left-only, right-only, and three centre voices; effect 4 sends to reverb. [I] A portable score recipe “PS2 game-logo chord” is an appropriate tenth menu item. Its event duration can be zero (five triggers together); its audio length requires envelopes, pitch conversion, reverb and a defined tail threshold, so there is no honest single exact seconds value to invent. The physical RPC calls occur sequentially rather than at one observed simultaneous hardware timestamp.

[V] The same five indexed play commands are present in each other supplied PS2LOGO build: first/last trigger VA `0x205B6C..0x205BC4` in 1.00, `0x10783C..0x107894` in 1.50, and `0x10766C..0x1076C4` in all three 1.60 identities. This verifies R01's supplied 1.00→2.00 carrier span independently of its shared bank.

## What is truly distinct for the music maker

[V] The native-program catalogue is **two banks / 21 programs**, B02's 0..3 and B03's 17 sparse program IDs. None is discarded merely because the boot score does not use it. B01/B04/B05 have **zero native programs**; counting their effect entries as programs would conflate driver paths. [I] Also expose their direct effect presets as playable one-shot/loop instruments. There are **19 distinct native effect-entry definitions** (B01's 5 + B04's 14; B05's 12 duplicate B04) plus the standalone sine: **41 addressable program/effect/sample choices** in all. Stereo pairs and layered combinations can be additional labelled recipes, not reductions in the underlying catalogue.

[V] Byte-distinct *stored music* is **K=9**, Q01..Q09. Eight PS2 names are seven byte clusters, plus two PS1 tables. [I] The user-facing repertoire can be **10 pieces** when R01, the verified executable-triggered logo chord, is represented as a score recipe. A faithful PS1 licence recipe wraps Q08 rather than adding an eleventh piece. Keep both TM30S and LOGOS as discoverable role aliases under Q04.

[V] First/last below always means **first/last within the supplied corpus**, not first/last firmware Sony ever released. Family timelines are separate: PS1 4.5, PS2 2.00, and PSP POPS 660 are not one version ordering. The SCPH-1000 input has no tail version; its 1.0 J attribution uses the supplied model label and the matching shell generation [I]. Its header date bytes are `22 09 94 19` [V]. POPS's “4.5 ... J” embedded version text describes that PS1 compatibility image; it is not a native 2000 PSP firmware release date.

| Distinct item | First supplied carrier | Last supplied carrier | Unique to one supplied version? |
|---|---|---|---|
| B02, its four programs and three samples; Q08 | PS1 SCPH-1000 shell; PS2 1.00 J | PSone 4.5 A; PS2 2.00 E; POPS 660 also | No |
| Q09 SCE intro | PS1 SCPH-1000 shell | PSone 4.5 A | Standalone PS1 family only; absent POPS/PS2 |
| B01 logo, B03 boot, all seven PS2 score clusters Q01..Q07 | PS2 1.00 J | PS2 2.00 E | No |
| B05 twelve-entry effect-bank header variant | PS2 1.00 J | PS2 1.00 J | Yes, that version only; no unique sample or effect definition |
| B04 fourteen-entry bank; effect definitions 12/13 | PS2 1.50 A devkit | PS2 2.00 E | No; their earliest supplied carrier is the devkit |
| S01 TESTSPU sine | PS2 1.50 A devkit | PS2 2.00 E | No; absent in the 1.00 dumps |

[V] There is **no additional verified bank, sample or score exclusive to the supplied 2.00 E rom0**, nor a sound asset exclusive to the 1.50 devkit. The 1.00 inline effect header is its one exclusive valid complete-bank cluster, despite its shared audio payload. The bad `[h]` dump is the exclusive structural anomaly. Identical retail aliases (the two 1.00 J files, two 1.60 E files, two 1.60 A February files, and the two 3.0 A PS1 files) do not inflate content counts.

## ROM1, ROM2, EROM, and the DVD player

[V] The supplied 70004 `.ROM1` and `.ROM2` are **byte-identical 524,288-byte files**, with the same ROMDIR and driver modules, including LIBSD at file `+0x55480` (`0x7325` bytes) and SDRDRV at `+0x5C7B0` (`0x23C9` bytes). The separate `bios/rom1.bin` is 199,680 bytes, with LIBSD at `+0x25B70` and SDRDRV at `+0x2BF80`. These are driver libraries, not evidence of resident instrument banks. No valid SShd/SSsq/VAB/VAG/SEQ signature occurs in any of those auxiliary files or their decoded views.

[V] EROM is 3,145,728 bytes. All **40 gzip members were independently inflated and CRC/trailer checked by zlib**, consuming 2,378,402 stored bytes and producing **11,009,004 bytes** in total. None of the inflated members or raw EROM contains even a candidate for the six scanned sound signatures. Members 0..37 have repeated image/resource descriptor layouts; member 38 (input `+0x2B1768`, stored 317,180 bytes, inflated 3,776,964 bytes) includes executable/library data; member 39 (input `+0x2FFC4C`, stored 536 bytes, inflated 3,832 bytes) is the small final data member. Every member offset/extent appears in the TSV.

[I] These results support **“no discoverable resident DVD menu sound bank in these auxiliary dumps”**, not “the DVD player is incapable of audio.” LIBSD/SDRDRV and MPEG decoding relate to runtime disc playback; the existence of a codec/driver is not an additional shipped song. There is no verified DVD menu sample or music to add to the recovered library. Unmarked PCM, procedural tones, or unknown resource encodings remain outside the negative signature proof; further work would require tracing DVD-player voice transfers back to concrete buffers. That limitation does not justify treating random instruction bytes as extra sample sets.

## PS3: protected PUP versus user-supplied plaintext resources

[V] Independently rereading `bios/PS3UPDAT.PUP` confirms the structural findings in `repo/notes/research/ps3_boot.md`: 206,167,196 bytes, nine PUP entries, and a 194,426,880-byte outer tar at `+0x5B644C` containing **50 regular members**, all beginning with `SCE\0`: 48 type-3 packages and two type-2 revocation containers. The first numbered dev_flash package starts at `+0x1A7A44C`; its protected payload boundary is recorded in the prior note as member `+0x300`. The outer member names/headers are readable; inner dev_flash sound files are not exposed by this parser.

[I] Confirming that prior note's boundary: **no plaintext coldboot/XMB/SND0 sound is reachable by the available unprotected PUP/tar readers**. Package metadata is not recovered AC3/AT3/VAG audio. Nothing in this inventory decrypts the protected packages. This is an access/format finding, not a global absence claim about PS3 firmware. No plaintext PS3 resource directory was supplied.

The following formats and estimates are [I], summarizing `repo/notes/research/ps3_boot.md`'s system-resource and decoder sections. “Small/medium/large” describes work categories, not a promised calendar estimate.

| User-supplied plaintext source | Audio carried | Decoder effort |
|---|---|---|
| `vsh/resource/coldboot_stereo.ac3` / `coldboot_multi.ac3` | AC-3 coldboot recording, commonly stereo or six channels | Small metadata/syncframe reader; large new conformant AC-3 decoder. Medium integration if an approved existing decoder is used, including channel routing, downmix, resampling and licensing/dependency review. It does not reuse PS-ADPCM. |
| `SND0.AT3` from a readable title/PBP presentation | Usually RIFF/WAVE wrapping ATRAC-family audio, including ATRAC3 | Small container/metadata reader; large codec implementation, or medium integration of a suitable existing decoder. Choose codec by WAVE format fields, not `.AT3` extension; ATRAC3 and ATRAC3plus support are separate capabilities. This is title/tile music, not necessarily system boot audio. |
| VAG sound entries in a plaintext RCO, e.g. `system_plugin.rco` | Shell cursor/confirm/cancel/error/trophy tones; possible per-channel streams | Small VAG wrapper, length/rate checks and playback adapter can reuse `sound::decode_adpcm`; medium RCO indexing/decompression and multichannel assembly. The parent RCO supplies channel structure; do not assume every entry is one mono VAG file. |

[I] A pre-recorded AC3/AT3 stream contributes a track/sample asset, not native MIDI-style programs. Avoid inventing “PS3 instruments” by silently slicing it into notes. A user can explicitly derive samples; record that transformation separately. VAGs can be offered as pitched sampler instruments with declared source rate and transparent tuning assumptions.

## Recommended `ps2kit::library` shape

All proposals in this section are [I]. The implementation can remain free of Sony audio, bank hashes, sample hashes, and reference fixtures.

1. **Separate content identity, provenance, and role.** A `Library` owns locally computed `BankId`, `SampleId`, `ScoreId` and `InstrumentId`; its `SourceOccurrence` records input-file identity, console, version text read from the input, ROMDIR path, transformation chain, parsed offsets/extents and evidence. A human `RoleLabel` such as “PS2 boot ambience” or an observed `SNDBOOTS` alias is searchable metadata, not a content identity. Several source occurrences and role labels can resolve to one local asset.
2. **Fingerprint only parsed content on the user's machine.** Use domain-separated, length-framed hashing of a bank's actual header and declared body (or equivalent VAB extent), and of a sequence through its terminator. Do not hash a fixed look-ahead window, compressed archive, mutable live IOP buffer, or neighbouring padding. Preserve the raw-byte identity separately from any semantic identity: B04/B05 are distinct complete banks with shared samples and twelve shared effect definitions. No shipped table says which digest means “Sony boot bank”.
3. **Make instrument references explicitly bank-scoped.** An `InstrumentRef` is `(local_bank_fingerprint, native_program_id)` for B02/B03; an `EffectRef` is `(local_bank_fingerprint, native_effect_index)` for B01/B04/B05. A `SampleRef` also carries encoding, source rate, channel/stream layout, and parsed extent. Human labels and structural source hints assist rebinding. Program 0 in PS1 and program 0 in PS2 are independent instruments; sparse PS2 IDs 100..102 must survive import.
4. **Provide playable adapters without fabricating native programs.** Represent `Program`, `DirectEffect`, and `Sample` as kinds of a common auditionable instrument interface with capability metadata (key range, root/fine tuning, layers/splits, ADSR, loop, pan/reverb). Expose the 21 programs, 19 distinct effect definitions, and sine. Combining left/right effect pairs or emulating R01 is a recipe that references the underlying assets. Preserve non-program effects in enumeration; the current SoundBank program map misses them.
5. **Require score-to-bank bindings.** An SSsq score needs the driver registration binding recovered from the same input. PS1 tables need B02 plus an explicit field-clock/timing recipe. `Score` variants can be `Sequence`, `FieldNoteTable`, `TriggerRecipe`, and `AudioTrack`; neither SSsq bytes nor “logo” in a name reliably select a bank. A score identity may be shared while its native playback recipe or role differs. Store loop start/end/count and separate finite linear traversal from unbounded playback; audition/export requests must specify a duration for infinite loops.
6. **Expose evidence and availability as ordinary data.** Return `verified role`, `inferred label`, `registered cue / trigger unknown`, `unsupported codec`, `damaged source`, and `missing binding` states. The menu should still offer unused recoverable cues and program 41. Version spans are derived from the currently indexed files, per console family; adding/removing a dump updates them. Decoder support is independent of source ownership and role knowledge.
7. **Keep portable project references honest.** A user-created project may store the fingerprints computed from that user's files plus a friendly name, expected format/rate, console/version hints and native index. On another machine, resolve against its local library, compare content, and report a missing dependency if unmatched. Rebinding by label needs an explicit candidate choice when multiple variants exist. The product ships parsers, structural locators, optional role rules and synthetic tests; it ships no Sony hash catalogue or BIOS/sample bytes. Derived user samples/projects are user outputs, not application resources.

[I] Practical import order: unify all five bank clusters and S01 with direct-effect support; enumerate all eight PS2 names with Q04 aliases; add Q08/Q09 and R01 as clocked recipes; then support plaintext VAG-in-RCO, followed by optional AC3/ATRAC decoders. Synth fidelity work should preserve the actual driver clock, velocity/pan/pitch tables and reverb assumptions without preventing a complete searchable content catalogue.

## Machine-readable handoff and reproduction

[V] `inventory.tsv` is UTF-8 with a header row. It contains source/input rows; bank, standalone-sample, stored-score and recipe occurrences; one row for every referenced sample occurrence; deduplicated native-program and effect definitions; gzip-member extents; and rejected signature candidates. The `cluster` field is a local equality class; `parent_cluster` binds a sample/program/effect to its bank. The `versions`, `first_carrier`, and `last_carrier` fields describe this corpus. Empty numeric fields mean not applicable/unknown, not zero. A bank's `size_bytes` is logical header-plus-body length, while its header/body location fields handle non-contiguous storage. `seconds` means stored sample traversal or linear score traversal, and `loop` identifies unbounded native playback where known.

[V] Research helpers are `sweep.py`, `parse_inventory.py`, `trace.py`, `audit_extra.py`, `emit.py`, and the room-local `validate.rs`/Cargo manifest. `python3 sweep.py`, then `python3 parse_inventory.py`, `python3 trace.py`, `python3 audit_extra.py`, and `python3 emit.py` regenerate the metadata and validation inputs. `cargo run --offline --target-dir target --bin validate` regenerates `validation.txt` when redirected there. Recovered binary views and compiled targets are temporary room-local work products; they are not needed in the report/TSV handoff and should not be distributed as part of a product.

The generated appendices below give exact provenance locations, the full deduplicated native-program/sample lists, effect-index availability, and every EROM gzip extent. All factual table entries are [V]; descriptive program names remain [I].

## Generated appendix A: bank and standalone-sample carriers [V]

Every physical file occurrence is listed; decoded offsets are labelled. A body in a separate named module is non-contiguous with its header.

| Cluster | Input file | Header view + offset | Body view + offset | Input locator |
| --- | --- | --- | --- | --- |
| B01 | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | PS2LOGO 0x391c0 | PS2LOGO 0x393c0 | file bytes |
| S01 | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | TESTSPU 0x990 | TESTSPU 0x9c0 | file bytes |
| B02 | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | LOGO:unpacked 0x126e0 | LOGO:unpacked 0x13900 | LZ input file+0xa484; consumed 83426 |
| B03 | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDBOOTH 0x0 | SNDIMAGE/SNDBOOTB 0x0 | LZ input file+0x1cbfe0; stored 610; consumed 610 |
| B04 | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDOSDDH 0x0 | SNDIMAGE/SNDOSDDB 0x0 | LZ input file+0x2221b0; stored 262; consumed 262 |
| B01 | PS2 Bios 30004R V6 Pal.bin | PS2LOGO 0x2bc40 | PS2LOGO 0x2be40 | file bytes |
| S01 | PS2 Bios 30004R V6 Pal.bin | TESTSPU 0x1370 | TESTSPU 0x13a0 | file bytes |
| B02 | PS2 Bios 30004R V6 Pal.bin | LOGO:unpacked 0x12850 | LOGO:unpacked 0x13a70 | LZ input file+0xa474; consumed 83248 |
| B03 | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDBOOTH 0x0 | SNDIMAGE/SNDBOOTB 0x0 | LZ input file+0x1db090; stored 610; consumed 610 |
| B04 | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDOSDDH 0x0 | SNDIMAGE/SNDOSDDB 0x0 | LZ input file+0x231260; stored 262; consumed 262 |
| B02 | PSXONPSP660.BIN | PS1 shell:unpacked 0x13980 | PS1 shell:unpacked 0x14ba0 | LZ input file+0x181b0; consumed 89955 |
| S01 | SCPH-70004_BIOS_V12_PAL_200.BIN | TESTSPU 0x14e0 | TESTSPU 0x1510 | file bytes |
| B02 | SCPH-70004_BIOS_V12_PAL_200.BIN | LOGO:unpacked 0x12850 | LOGO:unpacked 0x13a70 | LZ input file+0xa5b4; consumed 83248 |
| B03 | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDBOOTH 0x0 | SNDIMAGE/SNDBOOTB 0x0 | LZ input file+0x1fe7b0; stored 610; consumed 610 |
| B04 | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDOSDDH 0x0 | SNDIMAGE/SNDOSDDB 0x0 | LZ input file+0x254980; stored 262; consumed 262 |
| B01 | SCPH-70004_BIOS_V12_PAL_200.BIN | PS2LOGO:unpacked 0x2b5c0 | PS2LOGO:unpacked 0x2b7c0 | LZ input file+0x32cce0; consumed 101467 |
| B02 | Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin | PS1 shell:unpacked 0x4d800 | PS1 shell:unpacked 0x4ea20 | LZ input file+0x1804c; consumed 214200 |
| B01 | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | PS2LOGO 0x2bc40 | PS2LOGO 0x2be40 | file bytes |
| S01 | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | TESTSPU 0x1370 | TESTSPU 0x13a0 | file bytes |
| B02 | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | LOGO:unpacked 0x12850 | LOGO:unpacked 0x13a70 | LZ input file+0xa474; consumed 83248 |
| B03 | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDBOOTH 0x0 | SNDIMAGE/SNDBOOTB 0x0 | LZ input file+0x1db090; stored 610; consumed 610 |
| B04 | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDOSDDH 0x0 | SNDIMAGE/SNDOSDDB 0x0 | LZ input file+0x231260; stored 262; consumed 262 |
| B03 | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x22580 | OSDSYS 0x22a80 | file bytes |
| B05 | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x7a680 | OSDSYS 0x7aa40 | file bytes |
| B01 | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | PS2LOGO 0x26100 | PS2LOGO 0x26300 | file bytes |
| B02 | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | LOGO:unpacked 0x125f0 | LOGO:unpacked 0x13810 | LZ input file+0xa164; consumed 82949 |
| B01 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | PS2LOGO 0x2bc40 | PS2LOGO 0x2be40 | file bytes |
| S01 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | TESTSPU 0x1370 | TESTSPU 0x13a0 | file bytes |
| B02 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | LOGO:unpacked 0x12850 | LOGO:unpacked 0x13a70 | LZ input file+0xa494; consumed 83248 |
| B03 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDBOOTH 0x0 | SNDIMAGE/SNDBOOTB 0x0 | LZ input file+0x1e08b0; stored 610; consumed 610 |
| B04 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDOSDDH 0x0 | SNDIMAGE/SNDOSDDB 0x0 | LZ input file+0x236a80; stored 262; consumed 262 |
| B01 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | PS2LOGO 0x2bc40 | PS2LOGO 0x2be40 | file bytes |
| S01 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | TESTSPU 0x1370 | TESTSPU 0x13a0 | file bytes |
| B02 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | LOGO:unpacked 0x12850 | LOGO:unpacked 0x13a70 | LZ input file+0xa4a4; consumed 83248 |
| B03 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDBOOTH 0x0 | SNDIMAGE/SNDBOOTB 0x0 | LZ input file+0x1e1a60; stored 610; consumed 610 |
| B04 | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDOSDDH 0x0 | SNDIMAGE/SNDOSDDB 0x0 | LZ input file+0x237c30; stored 262; consumed 262 |
| B02 | Sony PlayStation BIOS (E)(v2.0)(1995-05-10)[SCPH-1002].bin | PS1 raw shell 0x36640 | PS1 raw shell 0x37860 | file bytes |
| B02 | Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin | PS1 raw shell 0x369c0 | PS1 raw shell 0x37be0 | file bytes |
| B02 | Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin | PS1 raw shell 0x36b70 | PS1 raw shell 0x37d90 | file bytes |
| B02 | Sony PlayStation BIOS (E)(v4.1)(1997-12-16)[SCPH-7502 + SCPH-9002].bin | PS1 shell:unpacked 0x47bb0 | PS1 shell:unpacked 0x48dd0 | LZ input file+0x1804c; consumed 199844 |
| B02 | Sony PlayStation BIOS (J)(v1.1)(1995-01-22)[SCPH-3000].bin | PS1 raw shell 0x37360 | PS1 raw shell 0x38580 | file bytes |
| B02 | Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin | PS1 raw shell 0x3bdd0 | PS1 raw shell 0x3cff0 | file bytes |
| B02 | Sony PlayStation BIOS (J)(v3.0)(1996-09-09)[SCPH-5500].bin | PS1 raw shell 0x3bda0 | PS1 raw shell 0x3cfc0 | file bytes |
| B02 | Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin | PS1 shell:unpacked 0x4ba60 | PS1 shell:unpacked 0x4cc80 | LZ input file+0x1804c; consumed 204811 |
| B02 | Sony PlayStation BIOS (J)[SCPH-1000].bin | PS1 raw shell 0x38630 | PS1 raw shell 0x39850 | file bytes |
| B02 | Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin | PS1 raw shell 0x39000 | PS1 raw shell 0x3a220 | file bytes |
| B02 | Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin | PS1 shell:unpacked 0x47bb0 | PS1 shell:unpacked 0x48dd0 | LZ input file+0x1804c; consumed 199844 |
| B02 | Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 (1995-12-04)(Sony)(US).bin | PS1 raw shell 0x38fc0 | PS1 raw shell 0x3a1e0 | file bytes |
| B03 | scph10000.bin | OSDSYS 0x22580 | OSDSYS 0x22a80 | file bytes |
| B05 | scph10000.bin | OSDSYS 0x7a680 | OSDSYS 0x7aa40 | file bytes |
| B01 | scph10000.bin | PS2LOGO 0x26100 | PS2LOGO 0x26300 | file bytes |
| B02 | scph10000.bin | LOGO:unpacked 0x125f0 | LOGO:unpacked 0x13810 | LZ input file+0xa164; consumed 82949 |
| B01 | scph39001.bin | PS2LOGO 0x2bc40 | PS2LOGO 0x2be40 | file bytes |
| S01 | scph39001.bin | TESTSPU 0x1370 | TESTSPU 0x13a0 | file bytes |
| B02 | scph39001.bin | LOGO:unpacked 0x12850 | LOGO:unpacked 0x13a70 | LZ input file+0xa494; consumed 83248 |
| B03 | scph39001.bin | SNDIMAGE/SNDBOOTH 0x0 | SNDIMAGE/SNDBOOTB 0x0 | LZ input file+0x1e08b0; stored 610; consumed 610 |
| B04 | scph39001.bin | SNDIMAGE/SNDOSDDH 0x0 | SNDIMAGE/SNDOSDDB 0x0 | LZ input file+0x236a80; stored 262; consumed 262 |
| B02 | scph5501.bin | PS1 raw shell 0x39000 | PS1 raw shell 0x3a220 | file bytes |

## Generated appendix B: every stored score carrier [V]

Each row is one file occurrence and one parsed logical score. The bank binding is B03 for SSsq and B02 for note tables.

| Cluster | Name | Input file | View + offset | VA if known | Parsed bytes |
| --- | --- | --- | --- | --- | --- |
| Q01 | SNDBOOTS | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDBOOTS 0x0 |  | 460 |
| Q02 | SNDTNNLS | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDTNNLS 0x0 |  | 418 |
| Q03 | SNDCLOKS | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDCLOKS 0x0 |  | 1612 |
| Q04 | SNDTM30S | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDTM30S 0x0 |  | 304 |
| Q05 | SNDTM60S | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDTM60S 0x0 |  | 304 |
| Q04 | SNDLOGOS | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDLOGOS 0x0 |  | 304 |
| Q06 | SNDWARNS | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDWARNS 0x0 |  | 1613 |
| Q07 | SNDRCLKS | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | SNDIMAGE/SNDRCLKS 0x0 |  | 418 |
| Q01 | SNDBOOTS | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDBOOTS 0x0 |  | 460 |
| Q02 | SNDTNNLS | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDTNNLS 0x0 |  | 418 |
| Q03 | SNDCLOKS | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDCLOKS 0x0 |  | 1612 |
| Q04 | SNDTM30S | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDTM30S 0x0 |  | 304 |
| Q05 | SNDTM60S | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDTM60S 0x0 |  | 304 |
| Q04 | SNDLOGOS | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDLOGOS 0x0 |  | 304 |
| Q06 | SNDWARNS | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDWARNS 0x0 |  | 1613 |
| Q07 | SNDRCLKS | PS2 Bios 30004R V6 Pal.bin | SNDIMAGE/SNDRCLKS 0x0 |  | 418 |
| Q01 | SNDBOOTS | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDBOOTS 0x0 |  | 460 |
| Q02 | SNDTNNLS | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDTNNLS 0x0 |  | 418 |
| Q03 | SNDCLOKS | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDCLOKS 0x0 |  | 1612 |
| Q04 | SNDTM30S | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDTM30S 0x0 |  | 304 |
| Q05 | SNDTM60S | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDTM60S 0x0 |  | 304 |
| Q04 | SNDLOGOS | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDLOGOS 0x0 |  | 304 |
| Q06 | SNDWARNS | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDWARNS 0x0 |  | 1613 |
| Q07 | SNDRCLKS | SCPH-70004_BIOS_V12_PAL_200.BIN | SNDIMAGE/SNDRCLKS 0x0 |  | 418 |
| Q01 | SNDBOOTS | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDBOOTS 0x0 |  | 460 |
| Q02 | SNDTNNLS | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDTNNLS 0x0 |  | 418 |
| Q03 | SNDCLOKS | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDCLOKS 0x0 |  | 1612 |
| Q04 | SNDTM30S | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDTM30S 0x0 |  | 304 |
| Q05 | SNDTM60S | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDTM60S 0x0 |  | 304 |
| Q04 | SNDLOGOS | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDLOGOS 0x0 |  | 304 |
| Q06 | SNDWARNS | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDWARNS 0x0 |  | 1613 |
| Q07 | SNDRCLKS | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | SNDIMAGE/SNDRCLKS 0x0 |  | 418 |
| Q01 | SNDBOOTS | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x799c0 | 0x2789c0 | 460 |
| Q02 | SNDTNNLS | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x79bc0 | 0x278bc0 | 418 |
| Q03 | SNDCLOKS | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x79d80 | 0x278d80 | 1612 |
| Q04 | SNDTM30S | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x7a400 | 0x279400 | 304 |
| Q05 | SNDTM60S | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x7a540 | 0x279540 | 304 |
| Q04 | SNDLOGOS | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x87ec0 | 0x286ec0 | 304 |
| Q06 | SNDWARNS | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x88000 | 0x287000 | 1613 |
| Q07 | SNDRCLKS | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | OSDSYS 0x88680 | 0x287680 | 418 |
| Q01 | SNDBOOTS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDBOOTS 0x0 |  | 460 |
| Q02 | SNDTNNLS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDTNNLS 0x0 |  | 418 |
| Q03 | SNDCLOKS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDCLOKS 0x0 |  | 1612 |
| Q04 | SNDTM30S | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDTM30S 0x0 |  | 304 |
| Q05 | SNDTM60S | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDTM60S 0x0 |  | 304 |
| Q04 | SNDLOGOS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDLOGOS 0x0 |  | 304 |
| Q06 | SNDWARNS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDWARNS 0x0 |  | 1613 |
| Q07 | SNDRCLKS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | SNDIMAGE/SNDRCLKS 0x0 |  | 418 |
| Q01 | SNDBOOTS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDBOOTS 0x0 |  | 460 |
| Q02 | SNDTNNLS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDTNNLS 0x0 |  | 418 |
| Q03 | SNDCLOKS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDCLOKS 0x0 |  | 1612 |
| Q04 | SNDTM30S | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDTM30S 0x0 |  | 304 |
| Q05 | SNDTM60S | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDTM60S 0x0 |  | 304 |
| Q04 | SNDLOGOS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDLOGOS 0x0 |  | 304 |
| Q06 | SNDWARNS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDWARNS 0x0 |  | 1613 |
| Q07 | SNDRCLKS | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | SNDIMAGE/SNDRCLKS 0x0 |  | 418 |
| Q01 | SNDBOOTS | scph10000.bin | OSDSYS 0x799c0 | 0x2789c0 | 460 |
| Q02 | SNDTNNLS | scph10000.bin | OSDSYS 0x79bc0 | 0x278bc0 | 418 |
| Q03 | SNDCLOKS | scph10000.bin | OSDSYS 0x79d80 | 0x278d80 | 1612 |
| Q04 | SNDTM30S | scph10000.bin | OSDSYS 0x7a400 | 0x279400 | 304 |
| Q05 | SNDTM60S | scph10000.bin | OSDSYS 0x7a540 | 0x279540 | 304 |
| Q04 | SNDLOGOS | scph10000.bin | OSDSYS 0x87ec0 | 0x286ec0 | 304 |
| Q06 | SNDWARNS | scph10000.bin | OSDSYS 0x88000 | 0x287000 | 1613 |
| Q07 | SNDRCLKS | scph10000.bin | OSDSYS 0x88680 | 0x287680 | 418 |
| Q01 | SNDBOOTS | scph39001.bin | SNDIMAGE/SNDBOOTS 0x0 |  | 460 |
| Q02 | SNDTNNLS | scph39001.bin | SNDIMAGE/SNDTNNLS 0x0 |  | 418 |
| Q03 | SNDCLOKS | scph39001.bin | SNDIMAGE/SNDCLOKS 0x0 |  | 1612 |
| Q04 | SNDTM30S | scph39001.bin | SNDIMAGE/SNDTM30S 0x0 |  | 304 |
| Q05 | SNDTM60S | scph39001.bin | SNDIMAGE/SNDTM60S 0x0 |  | 304 |
| Q04 | SNDLOGOS | scph39001.bin | SNDIMAGE/SNDLOGOS 0x0 |  | 304 |
| Q06 | SNDWARNS | scph39001.bin | SNDIMAGE/SNDWARNS 0x0 |  | 1613 |
| Q07 | SNDRCLKS | scph39001.bin | SNDIMAGE/SNDRCLKS 0x0 |  | 418 |
| Q08 | PS1 licence | DTL-H30101/DTL-H30101_USA_Dev_0150_20001228_v4_[CC645DA1].rom0 | LOGO:unpacked 0x1e160 | 0x8004e160 | 160 |
| Q08 | PS1 licence | PS2 Bios 30004R V6 Pal.bin | LOGO:unpacked 0x1e2d0 | 0x8004e2d0 | 160 |
| Q08 | PS1 licence | PSXONPSP660.BIN | PS1 shell:unpacked 0x1f400 | 0x8004f400 | 160 |
| Q08 | PS1 licence | SCPH-70004_BIOS_V12_PAL_200.BIN | LOGO:unpacked 0x1e2d0 | 0x8004e2d0 | 160 |
| Q08 | PS1 licence | Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin | PS1 shell:unpacked 0x593c8 | 0x800893c8 | 160 |
| Q09 | PS1 SCE intro | Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin | PS1 shell:unpacked 0x59280 | 0x80089280 | 328 |
| Q08 | PS1 licence | Sony PlayStation 2 BIOS (E)(v1.6)(2001-10-04)[SCPH30004].bin | LOGO:unpacked 0x1e2d0 | 0x8004e2d0 | 160 |
| Q08 | PS1 licence | Sony PlayStation 2 BIOS (J)(v0.1)(2000-01-17)[SCPH10000].bin | LOGO:unpacked 0x1e070 | 0x8004e070 | 160 |
| Q08 | PS1 licence | Sony PlayStation 2 BIOS (U)(v1.6)(2002-02-07)[SCPH39001].bin | LOGO:unpacked 0x1e2d0 | 0x8004e2d0 | 160 |
| Q08 | PS1 licence | Sony PlayStation 2 BIOS (U)(v1.6)(2002-03-19)[SCPH39004].bin | LOGO:unpacked 0x1e2d0 | 0x8004e2d0 | 160 |
| Q08 | PS1 licence | Sony PlayStation BIOS (E)(v2.0)(1995-05-10)[SCPH-1002].bin | PS1 raw shell 0x42208 | 0x80072208 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (E)(v2.0)(1995-05-10)[SCPH-1002].bin | PS1 raw shell 0x420c0 | 0x800720c0 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin | PS1 raw shell 0x42588 | 0x80072588 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin | PS1 raw shell 0x42440 | 0x80072440 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin | PS1 raw shell 0x42738 | 0x80072738 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin | PS1 raw shell 0x425f0 | 0x800725f0 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (E)(v4.1)(1997-12-16)[SCPH-7502 + SCPH-9002].bin | PS1 shell:unpacked 0x53778 | 0x80083778 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (E)(v4.1)(1997-12-16)[SCPH-7502 + SCPH-9002].bin | PS1 shell:unpacked 0x53630 | 0x80083630 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (J)(v1.1)(1995-01-22)[SCPH-3000].bin | PS1 raw shell 0x42f60 | 0x80072f60 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (J)(v1.1)(1995-01-22)[SCPH-3000].bin | PS1 raw shell 0x42de0 | 0x80072de0 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin | PS1 raw shell 0x47998 | 0x80077998 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin | PS1 raw shell 0x47850 | 0x80077850 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (J)(v3.0)(1996-09-09)[SCPH-5500].bin | PS1 raw shell 0x47968 | 0x80077968 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (J)(v3.0)(1996-09-09)[SCPH-5500].bin | PS1 raw shell 0x47820 | 0x80077820 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin | PS1 shell:unpacked 0x57628 | 0x80087628 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin | PS1 shell:unpacked 0x574e0 | 0x800874e0 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (J)[SCPH-1000].bin | PS1 raw shell 0x44230 | 0x80074230 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (J)[SCPH-1000].bin | PS1 raw shell 0x440b0 | 0x800740b0 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin | PS1 raw shell 0x44bc8 | 0x80074bc8 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin | PS1 raw shell 0x44a80 | 0x80074a80 | 328 |
| Q08 | PS1 licence | Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin | PS1 shell:unpacked 0x53778 | 0x80083778 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin | PS1 shell:unpacked 0x53630 | 0x80083630 | 328 |
| Q08 | PS1 licence | Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 (1995-12-04)(Sony)(US).bin | PS1 raw shell 0x44b88 | 0x80074b88 | 160 |
| Q09 | PS1 SCE intro | Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 (1995-12-04)(Sony)(US).bin | PS1 raw shell 0x44a40 | 0x80074a40 | 328 |
| Q08 | PS1 licence | scph10000.bin | LOGO:unpacked 0x1e070 | 0x8004e070 | 160 |
| Q08 | PS1 licence | scph39001.bin | LOGO:unpacked 0x1e2d0 | 0x8004e2d0 | 160 |
| Q08 | PS1 licence | scph5501.bin | PS1 raw shell 0x44bc8 | 0x80074bc8 | 160 |
| Q09 | PS1 SCE intro | scph5501.bin | PS1 raw shell 0x44a80 | 0x80074a80 | 328 |

## Generated appendix C: full native-program catalogue [V; labels I]

| Bank | Native ID | Suggested label | Tones | Sample clusters | Definition |
| --- | --- | --- | --- | --- | --- |
| B02 | 0 | Licence drone/chime bed | 2 | A03 | kind=layer; native id preserved; roots=72,72 |
| B02 | 1 | SCE rising sweep | 2 | A05 | kind=layer; native id preserved; roots=60,60 |
| B02 | 2 | Licence fade drone | 2 | A04 | kind=layer; native id preserved; roots=54,54 |
| B02 | 3 | Intro/licence arpeggio chime | 2 | A03 | kind=layer; native id preserved; roots=72,72 |
| B03 | 0 | Main layered pad | 3 | A13 | kind=layer; native id preserved; roots=70,70,65 |
| B03 | 1 | Pad | 1 | A13 | kind=split; native id preserved; roots=70 |
| B03 | 2 | Noise | 1 | A10 | kind=split; native id preserved; roots=62 |
| B03 | 40 | Stereo noise | 2 | A10 | kind=layer; native id preserved; roots=62,62 |
| B03 | 41 | Four-layer short periodic tone | 4 | A12 | kind=layer; native id preserved; roots=80,80,80,80 |
| B03 | 42 | Stereo noisy rumble | 2 | A11 | kind=layer; native id preserved; roots=72,72 |
| B03 | 50 | Stereo tonal cluster | 2 | A08 | kind=layer; native id preserved; roots=75,76 |
| B03 | 51 | Three-layer low drone | 3 | A09 | kind=layer; native id preserved; roots=77,78,79 |
| B03 | 52 | Three-layer broadband ambience | 3 | A07 | kind=layer; native id preserved; roots=73,75,70 |
| B03 | 53 | Stereo low drone | 2 | A09 | kind=layer; native id preserved; roots=77,78 |
| B03 | 60 | Three-layer one-shot cue | 3 | A06 | kind=layer; native id preserved; roots=69,81,81 |
| B03 | 61 | Stereo rumble, envelope A | 2 | A06 | kind=layer; native id preserved; roots=62,62 |
| B03 | 62 | Stereo rumble, envelope B | 2 | A06 | kind=layer; native id preserved; roots=62,62 |
| B03 | 70 | Three-layer tonal cue | 3 | A14 | kind=layer; native id preserved; roots=69,69,81 |
| B03 | 100 | Stereo tonal cluster, faster release | 2 | A08 | kind=layer; native id preserved; roots=75,76 |
| B03 | 101 | Broadband ambience, faster release | 3 | A07 | kind=layer; native id preserved; roots=73,75,70 |
| B03 | 102 | Noise, faster release | 1 | A10 | kind=split; native id preserved; roots=62 |

## Generated appendix D: unique parsed sample streams [V]

ADPCM extent stops at the first end flag. VAB allocations and SShd bodies may include additional guard/padding bytes, retained in bank identity. S01 uses 48 kHz; the other streams use the 44.1 kHz reference rate.

| Sample | Representative bank/container | Body offset | Decoded ADPCM bytes | PCM samples | Hz | Seconds | Loops |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A01 | B01 | 0x0 | 11392 | 19936 | 44100 | 0.452063492 | false |
| A02 | S01 | 0x0 | 1216 | 2128 | 48000 | 0.044333333 | true |
| A03 | B02 | 0x0 | 13472 | 23576 | 44100 | 0.534603175 | true |
| A04 | B02 | 0x34a0 | 7328 | 12824 | 44100 | 0.290793651 | false |
| A05 | B02 | 0x5140 | 22304 | 39032 | 44100 | 0.885079365 | true |
| A06 | B03 | 0x0 | 98512 | 172396 | 44100 | 3.909206349 | false |
| A07 | B03 | 0x180e0 | 16720 | 29260 | 44100 | 0.663492063 | true |
| A08 | B03 | 0x1c230 | 14848 | 25984 | 44100 | 0.589206349 | true |
| A09 | B03 | 0x1fc30 | 22960 | 40180 | 44100 | 0.911111111 | true |
| A10 | B03 | 0x255e0 | 22352 | 39116 | 44100 | 0.886984127 | true |
| A11 | B03 | 0x2ad30 | 25312 | 44296 | 44100 | 1.004444444 | true |
| A12 | B03 | 0x31010 | 368 | 644 | 44100 | 0.014603175 | true |
| A13 | B03 | 0x31180 | 108848 | 190484 | 44100 | 4.319365079 | true |
| A14 | B03 | 0x4bab0 | 46176 | 80808 | 44100 | 1.832380952 | true |
| A15 | B04 | 0x0 | 2496 | 4368 | 44100 | 0.099047619 | false |
| A16 | B04 | 0x9d0 | 12928 | 22624 | 44100 | 0.513015873 | false |
| A17 | B04 | 0x3c60 | 16720 | 29260 | 44100 | 0.663492063 | false |
| A18 | B04 | 0x7dc0 | 1024 | 1792 | 44100 | 0.040634921 | false |
| A19 | B04 | 0x81d0 | 9904 | 17332 | 44100 | 0.393015873 | false |
| A20 | B04 | 0xa890 | 4720 | 8260 | 44100 | 0.187301587 | false |
| A21 | B04 | 0xbb10 | 6496 | 11368 | 44100 | 0.257777778 | false |

## Generated appendix E: deduplicated direct-effect definitions [V]

B05 entries 0..11 exactly alias B04 entries 0..11; B04 entries 12/13 are later additions. These are driver-native effects, not program IDs.

| Effect cluster | Native bank:index aliases | Sample | Parameters | First supplied | Last supplied |
| --- | --- | --- | --- | --- | --- |
| E01 | B01:0 | A01 | pitch=3090; ADSR1=0x82ff; ADSR2=0x5fcf; L=28380; R=0; flags=0x0 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E02 | B01:1 | A01 | pitch=3046; ADSR1=0x82ff; ADSR2=0x5fcf; L=0; R=28380; flags=0x0 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E03 | B01:2 | A01 | pitch=2298; ADSR1=0x91ff; ADSR2=0x4e0f; L=27090; R=27090; flags=0x0 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E04 | B01:3 | A01 | pitch=4096; ADSR1=0x82ff; ADSR2=0x5fcf; L=27090; R=27090; flags=0x0 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E05 | B01:4 | A01 | pitch=3068; ADSR1=0x82ff; ADSR2=0x5fcf; L=27090; R=27090; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E06 | B04:0 | B05:0 | A17 | pitch=2055; ADSR1=0xff; ADSR2=0x4ecf; L=18060; R=14502; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E07 | B04:1 | B05:1 | A17 | pitch=2040; ADSR1=0xff; ADSR2=0x4ecf; L=14502; R=18060; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E08 | B04:2 | B05:2 | A16 | pitch=2048; ADSR1=0xff; ADSR2=0x4e8f; L=19702; R=19702; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E09 | B04:3 | B05:3 | A16 | pitch=1534; ADSR1=0xff; ADSR2=0x4e8f; L=7388; R=7388; flags=0x0 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E10 | B04:4 | B05:4 | A19 | pitch=2048; ADSR1=0x31ff; ADSR2=0x4d4f; L=24627; R=24627; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E11 | B04:5 | B05:5 | A20 | pitch=1366; ADSR1=0x9bff; ADSR2=0x4d0f; L=12313; R=12313; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E12 | B04:6 | B05:6 | A18 | pitch=2048; ADSR1=0x25ff; ADSR2=0x5fcf; L=12313; R=12313; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E13 | B04:7 | B05:7 | A19 | pitch=1371; ADSR1=0x2bc0; ADSR2=0x518f; L=23220; R=0; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E14 | B04:8 | B05:8 | A19 | pitch=1361; ADSR1=0x2bc0; ADSR2=0x518f; L=0; R=23220; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E15 | B04:9 | B05:9 | A21 | pitch=2048; ADSR1=0x2fff; ADSR2=0x5fcf; L=16007; R=16007; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E16 | B04:10 | B05:10 | A19 | pitch=861; ADSR1=0x20ff; ADSR2=0x4ccf; L=31277; R=31277; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E17 | B04:11 | B05:11 | A15 | pitch=966; ADSR1=0x2aff; ADSR2=0x4dcf; L=9851; R=9851; flags=0x80 | PS2: 0100JC20000117 | PS2: 0200EC20040614 |
| E18 | B04:12 | A20 | pitch=2298; ADSR1=0x9bff; ADSR2=0x4d0f; L=13545; R=13545; flags=0x80 | PS2: 0150AD20001228 | PS2: 0200EC20040614 |
| E19 | B04:13 | A20 | pitch=1933; ADSR1=0x9bff; ADSR2=0x4d0f; L=9851; R=9851; flags=0x80 | PS2: 0150AD20001228 | PS2: 0200EC20040614 |

## Generated appendix F: all EROM gzip members [V]

| Member | File offset | Stored bytes | Inflated bytes | Validation |
| --- | --- | --- | --- | --- |
| 0 | 0x44eb4 | 3823 | 13676 | passed; zero sound-signature candidates |
| 1 | 0x47a50 | 19526 | 63668 | passed; zero sound-signature candidates |
| 2 | 0x50e64 | 12519 | 55572 | passed; zero sound-signature candidates |
| 3 | 0x5b1e8 | 12437 | 55572 | passed; zero sound-signature candidates |
| 4 | 0x5fab8 | 15420 | 80692 | passed; zero sound-signature candidates |
| 5 | 0x63aa0 | 13725 | 55572 | passed; zero sound-signature candidates |
| 6 | 0x7396c | 13728 | 70644 | passed; zero sound-signature candidates |
| 7 | 0x7a224 | 14910 | 78180 | passed; zero sound-signature candidates |
| 8 | 0x7fa50 | 13302 | 55572 | passed; zero sound-signature candidates |
| 9 | 0x831a4 | 14597 | 73156 | passed; zero sound-signature candidates |
| 10 | 0x86ffc | 13373 | 87348 | passed; zero sound-signature candidates |
| 11 | 0x8d63c | 15070 | 70196 | passed; zero sound-signature candidates |
| 12 | 0x9abf4 | 10218 | 53108 | passed; zero sound-signature candidates |
| 13 | 0x9f94c | 9743 | 42644 | passed; zero sound-signature candidates |
| 14 | 0xa993c | 16937 | 72884 | passed; zero sound-signature candidates |
| 15 | 0xaf350 | 49381 | 194700 | passed; zero sound-signature candidates |
| 16 | 0xbc658 | 41719 | 143868 | passed; zero sound-signature candidates |
| 17 | 0xc85ac | 45958 | 183388 | passed; zero sound-signature candidates |
| 18 | 0xd48fc | 68220 | 299484 | passed; zero sound-signature candidates |
| 19 | 0xe59e0 | 55029 | 212524 | passed; zero sound-signature candidates |
| 20 | 0xf88f8 | 110502 | 360812 | passed; zero sound-signature candidates |
| 21 | 0x114ffc | 103853 | 330604 | passed; zero sound-signature candidates |
| 22 | 0x12ea34 | 125930 | 420252 | passed; zero sound-signature candidates |
| 23 | 0x14da80 | 125028 | 396956 | passed; zero sound-signature candidates |
| 24 | 0x16d388 | 115846 | 393420 | passed; zero sound-signature candidates |
| 25 | 0x189e78 | 125679 | 423564 | passed; zero sound-signature candidates |
| 26 | 0x1b22d8 | 114300 | 368140 | passed; zero sound-signature candidates |
| 27 | 0x1d2f9c | 114356 | 389484 | passed; zero sound-signature candidates |
| 28 | 0x1f1fc4 | 135728 | 478220 | passed; zero sound-signature candidates |
| 29 | 0x213208 | 130647 | 347740 | passed; zero sound-signature candidates |
| 30 | 0x234c30 | 105777 | 349340 | passed; zero sound-signature candidates |
| 31 | 0x250804 | 114628 | 341788 | passed; zero sound-signature candidates |
| 32 | 0x26d164 | 134436 | 511020 | passed; zero sound-signature candidates |
| 33 | 0x28e9f0 | 8529 | 28980 | passed; zero sound-signature candidates |
| 34 | 0x2934ac | 7099 | 22436 | passed; zero sound-signature candidates |
| 35 | 0x299cd0 | 8474 | 31092 | passed; zero sound-signature candidates |
| 36 | 0x2a02c0 | 11062 | 38036 | passed; zero sound-signature candidates |
| 37 | 0x2a45cc | 9177 | 33876 | passed; zero sound-signature candidates |
| 38 | 0x2b1768 | 317180 | 3776964 | passed; zero sound-signature candidates |
| 39 | 0x2ffc4c | 536 | 3832 | passed; zero sound-signature candidates |


## Addendum (2026-10-11): headerless sweep

[V] A second sweep looked for sound that carries no header — runs of ≥ 48 frames matching the
PS-ADPCM frame grammar (shift ≤ 12, filter ≤ 4, known flags, mostly non-zero, ending in an end flag)
— across every file in the corpus, every ROMDIR module (packed modules unpacked, SNDIMAGE members
included), ROM1/ROM2 and all 40 EROM gzip members. It found 23 distinct streams; 22 lie inside the
bank bodies this note already parses (SNDBOOTB, SNDOSDDB, 1.00's inline banks in OSDSYS, PS2LOGO,
TESTSPU, the PS1 VAB in every shell). The 23rd, 2 048 bytes at `PSXONPSP660.BIN` `0x2E120` (POPS's
post-stream build slack), is **not audio**: every frame uses filter 0 with shift 1–2, it decodes to
jagged values on a 2 048-step grid, and it carries loop-body flags (2) without any loop-start flag
(6) — a 16-byte-record table that happens to fit the frame grammar. Conclusion: no audio exists in
the corpus outside the banks above.
