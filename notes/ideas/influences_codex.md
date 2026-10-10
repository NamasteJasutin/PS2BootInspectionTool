# T3 — boot as a promise, a threshold, and a memory of what survived

[I] Recommendation: borrow the grammar of waiting, revealing and yielding control; let unusual ROM behaviour supply the subject.
[I] The strongest experiences here make a small hidden rule visible through the user's own skyline, sound or disc.
[I] This is a thinker report: no product code, commits, network access or writes through `repo/` or `bios/`.
[I] Historical startup references below are public-knowledge recollections, not verified recordings or claims about exact timings.
[I] Every proposed interaction, homage judgment and effort estimate is an inference/design proposal.
[V] The verification ledger below identifies the dumps/decompilations actually read, their builds and their addresses.
[I] An evidence identifier such as E03 carries its full build/address citation into subsequent references.
[I] ROM offsets are file offsets; PS1 ROM virtual addresses are `0xBFC00000 + offset`; shell addresses require unpacking when stated.
[I] Current engine reuse was checked in `ps2kit/src/{history,sim,disc,ps1,logo,sound}.rs` and app `model.rs`, `renderer.rs`, `audio.rs`.
[I] The capability inventory is a reading of this checkout, not a claim that proposed features already exist.

## What is already spoken for

[I] I read both rounds of the research README, skimmed all eight survey notes, and read the campaign and 64 options.
[I] Options 29–37 already cover tunnels, converging ribbons, step orbits, grids, spiral dives, waves and polygon assembly.
[I] Options 3–8, 23–24 and 39–49 already cover scrubbing, comparison, provenance, logs, alternate history and inspection panels.
[I] The proposals here are specific new behaviours inside those surfaces, not replacement names for those options.
[I] For example, “eviction scan” exposes a persistent date floor during traversal; it does not re-propose an eviction simulator.
[I] “Kernel island” fixes identical bytes spatially while surrounding shell bytes change; it does not re-propose a generic diff tab.
[I] “Stereo lantern” transfers the user's existing audio balance to attention in space; it does not re-propose microphone-reactive towers.
[I] Keep the console reconstruction available as its own mode; label these treatments “interpretation” at entry and in exports.

## Verification ledger

- **E01 [V] — Persistent kernel window.** PS1 2.2 J SCPH-5000, 3.0 A `scph5501.bin`, PSone 4.5 A SCPH-101: ROM `[0x10000,0x18000)` is byte-identical; SHA-256 prefix `a9102fd421bd6a88`; date word at `0x100` is `0x19951204` in all three.
- **E02 [V] — POPS changes filler, not those older code bytes.** `PSXONPSP660.BIN` versus 3.0 A: 10,864 differences below ROM `0x18000`, confined to `[0xE630,0x10000)` and `[0x16F60,0x18000)`; equal outside those windows. POPS shell stream at `0x181B0` unpacks to `0x20F40` bytes.
- **E03 [V] — Eviction has state.** PS2 2.00 E SCPH-70004 OSDSYS [`HistoryUpdate`, `0x201E98`](repo/analysis/osdsys_named/c/00201e98.c): 21 slots; count floor falls without resetting date floor; date comparison casts through signed `short`. [`HistoryDateStamp`, `0x201E68`](repo/analysis/osdsys_named/c/00201e68.c) packs `day + month*0x20 + (year-2000)*0x200`, retaining the low 16 bits.
- **E04 [V] — Saturation is conditional.** Same build/function: full six-bit tower mask uses count limit `0x3F`, then index 7; matching records still receive a new date. With all 21 nonempty records at index 7, an unfamiliar title is not inserted. The non-full-mask branch has a separate `0x7F` clamp.
- **E05 [V] — A small view of a directory.** PS1 3.0 A [`0xBFC07700`](repo/analysis/ps1_ghidra/us30/c/bfc07700.c): one sector read; entries copied into `[0xA00091F0,0xA00095B0)` at 24 bytes each; termination also at buffer `0xA000B870`. This yields both a 40-record capacity and a 2,048-byte visibility limit.
- **E06 [V] — Prefix recognition is not assignment acceptance.** PS1 3.0 A [`0xBFC00944`](repo/analysis/ps1_ghidra/us30/c/bfc00944.c), [`0xBFC00B7C`](repo/analysis/ps1_ghidra/us30/c/bfc00b7c.c): first line-prefix match ends search; advance exactly key length, skip whitespace, require `=`. `BOOT2`/`TCBX` stop there without assigning; a later valid key is shadowed.
- **E07 [V] — Zero and surviving arguments.** PS1 3.0 A [`0xBFC008A0`](repo/analysis/ps1_ghidra/us30/c/bfc008a0.c) clears three config words and RAM `0x180`; `0xBFC00944` accumulates hexadecimal digits; `0xBFC00B7C` copies up to `0x80` bytes after the boot filename into RAM `0x180`.
- **E08 [V] — Loading and reading have different success criteria.** PS1 3.0 A [`0xBFC03A18`](repo/analysis/ps1_ghidra/us30/c/bfc03a18.c) ignores the payload read result; [`0xBFC07A04`](repo/analysis/ps1_ghidra/us30/c/bfc07a04.c) rejects a read size or position not aligned to `0x800`.
- **E09 [V] — POST is a register write, not a progress percentage.** PS1 3.0 A [`0xBFC01A60`](repo/analysis/ps1_ghidra/us30/c/bfc01a60.c) writes `0x1F802041`; boot main [`0xBFC067E8`](repo/analysis/ps1_ghidra/us30/c/bfc067e8.c) calls it for stages 1 through 9 around distinct operations.
- **E10 [V] — There is another disc gate after the shell.** PS1 3.0 A [`0xBFC0D570`](repo/analysis/ps1_ghidra/us30/c/bfc0d570.c) tests `0xA000DFFC`, calls `0xBFC0D72C` and `0xBFC0D7BC`, and routes either negative result to error `0x38B` before execute.
- **E11 [V] — A host chooses the picture's author.** POPS shell [`0x80030F50`](repo/analysis/ps1_ghidra/psp_shell/c/80030f50.c) reads one sector at LBA 4 for text; [`0x80030580`](repo/analysis/ps1_ghidra/psp_shell/c/80030580.c) builds the model from shell address `0x800406A0`. Retail 3.0 A `0x8003EC40` also reads seven sectors from LBA 5 for the model buffer.
- **E12 [V] — Display and comparison are separable.** PS1 3.0 A [`shell_entry`, `0x80030000`](repo/analysis/ps1_ghidra/shell_u30/c/80030000.c) sets check flag `0x80079E2C` to zero; `0x8003EC40` guards the `0x3278`-byte logo comparison with that flag. PSone 4.5 A `0x80030A6C` sets its flag then clears it for letter `A`.
- **E13 [V] — POPS adds a hold.** POPS shell entry `0x80030000` calls [`0x80030060`](repo/analysis/ps1_ghidra/psp_shell/c/80030060.c) with `0x78`; the helper waits 120 times through its field-wait call.
- **E14 [V] — Two event clocks, one sample bank.** PS1 3.0 A SCE table at shell `0x80074A80` / ROM `0x5CA80`, licence table at shell `0x80074BC8` / ROM `0x5CBC8`, VAB at shell `0x80069000`; first SCE event is program 1, note 36, velocity 127, pan 80. `0x8003F940` resets its clock to 89 NTSC or 74 PAL; `0x8003FA5C` dispatches due events and increments the clock.
- **E15 [V] — Eight changes after decompression.** PS2 1.60 E SCPH-30004R OSDSYS at ROM `0x34B4E0`, 1.60 A SCPH-39001 at `0x350E50`: unpacked images equal, length `0xA7EB4`; E stub file offsets `0x248..0x290` store to `0x20237C`, `0x2026E0`, `0x20274C`, `0x202460`, `0x2025C4`, `0x2024E4`, `0x2022F4`, `0x20230C`.
- **E16 [V] — Checksum and retry, separately verified.** PS2 1.60 retail XCDVDMAN [`module +0x72F0`](repo/analysis/hidden/xcdvdman_ret160/c/000072f0.c) compares the full eight-bit sum of 15 bytes to byte 15. PS2 2.00 E [`ReadConfigBlock`, `0x203DA8`](repo/analysis/osdsys_named/c/00203da8.c) repeats while returned status has bits `0x81`.
- **E17 [V] — The supplied settings blocks currently pass.** SCPH-70004 2.00 E NVM `0x2C0..0x2CF`: computed/stored `0x97`; SCPH-39001 1.60 A NVM `0x310..0x31F`: `0xE0`; SCPH-30004R 1.60 E NVM same offsets: `0xBF`. All three pass the eight-bit sum.
- **E18 [V] — Seven characters route different consumers.** SCPH-70004 2.00 E NVM `0x180..0x18B` contains `EEengEE`; OSDSYS `0x205160` replaces four `OSDVER` question marks from returned region bytes; `0x2052D0` returns its DVD suffix from `0x27B3F8`. File `0x186` is the final `E` in this observed block.
- **E19 [V] — The display letter can differ.** SCPH-70004 ROM1 `DVDIDO` at file `0x770` says `3.10O`, while `DVDVERO` at `0x7F0` says `3.10A`; the E pair at `0x760`/`0x7E0` both says `3.10E`. ROM2 equals ROM1 across the full supplied 512 KiB files.
- **E20 [V] — Plain islands in EROM.** SCPH-70004 EROM: 40 complete, CRC-valid gzip members, first at `0x44EB4`, last at `0x2FFC4C`; total inflated bytes **11,009,004**. This check establishes gzip validity, not the meaning of intervening bytes.
- **E21 [V] — A font survives across hosts.** ROM `[0x66000,0x7FE70)` equal in sampled PS1 2.2 J, 3.0 A, PSone 4.5 A, POPS and PS2 2.00 E `KROM` at ROM `0x66000`, length `0x19E70`.
- **E22 [V] — A newline insertion pattern.** Clean and `[h]` SCPH-5000 2.2 J first differ at ROM/file `0x24A`: clean starts `0A 24`, altered `0D 0A 24`; sizes 524,288 versus 526,083; altered file has 1,794 CRLF pairs and no lone LF. Text-mode handling as the cause is inferred [I].
- **E23 [V] — Dead monitor words have a home.** SCPH-1000 1.0 J ROM `0x16F94` contains `exec`, `0x1701C` contains `sector`, `0x1723C` contains `dbc`; kernel entry at RAM `0x500` zeroes `[0x7460,0x8920)`, covering their copied positions. Presence does not establish an installed monitor.
- **E24 [V] — Container visibility is finite.** Supplied PS3 4.82 PUP `0x18`: nine top-level entries; version at `0x290`; entry `0x300` points to tar at `0x5B644C`, where `tarfile` enumerates **50 regular files**. Nothing here establishes a boot-asset location.
- **E25 [V] — Four bytes repeat.** Supplied SCPH-39001, SCPH-30004R and SCPH-70004 MEC files are each `03 06 02 00` at offset zero. Their equality is verified; calling them emulator-created relies on the archived survey's PCSX2 source reading [I].

[I] Reproduction: [verify_evidence.py](verify_evidence.py) reads through the links without writing extracted assets; [evidence_checks.json](evidence_checks.json) records results.
[I] The scratch parser/eviction traces transcribe narrow control-flow cases; they are not console execution or a general emulator.
[I] Three corrections for the coordinator: E06 contradicts “BOOT2 satisfies BOOT” as an accepted assignment; E20 contradicts the roughly 21 MB inflated total; E24 contradicts the 48-member update-tar count.
[I] The archived PCSX2 default-checksum finding remains useful, but its original external source is unavailable here and no running emulator was observed.
[V] E17 verifies that the supplied nonzero NVM settings blocks are already repaired/valid; do not present them as checksum failures.

## Part A — motion and structure survey

[I] Each row separates the remembered motif, the reusable grammar, a mapping to owned data, and a resemblance to avoid.
[I] “Nod” means the idea can stand on its own in this app; “avoid” is a design judgment about conspicuous identity cues, not legal advice.
[I] Version-specific sequences vary; these are reference families, not promises that every machine shows the described motif.

| Reference [I] | Motion or structure to borrow [I] | Our user's material / existing parts [I] | Nod / avoid [I] |
|---|---|---|---|
| Sega Saturn: assembly and dimensional arrival | Delay the viewer's understanding until projected fragments resolve into a stable object; use an occluding foreground pass. | Camera crosses actual tower silhouettes, then settles on a selected record; no flying-piece rebuild required. | Nod: occlusion becoming legibility. Avoid: Saturn wordmark, orbit emblem, blue scene and matched assembly timing. |
| Dreamcast: sparse events with meaningful pauses | Let an initial small event promise a larger arrival; preserve a quiet interval after it. | One history tower catches the user's chime onset; the rest become visible only as its release decays. | Nod: point → expectation → field. Avoid: spiral trace, orange/blue brand palette and original sound contour; deeper than option 33's spiral dive. |
| Nintendo 64: game-specific 3D openings | A familiar object can stop functioning as a flat emblem once the camera looks around it. No universal N64 cinema is assumed. | Begin with a selected save's tower edge-on; reveal its actual volume and nearby records with the free-camera machinery. | Nod: perspective changes what a shape means. Avoid: multicolour N, rotating N silhouette or treating a game's intro as system ROM behaviour. |
| GameCube: play and alternate startup flavours | A short, self-contained ritual can tolerate a deliberate user-selected variation while keeping its destination. | A held pointer/button sets the duration of anticipation before the user's skyline reveal; show the hold meter openly. | Nod: playful variation with a stable ending. Avoid: purple cube route, logo outline, original alternate voices and copied hidden combination. |
| Wii: calm invitation | End the reveal at a stable, usable state instead of spending the entire animation budget before interaction. | Towers stop drifting when the loaded-input card becomes actionable; focus a selected history entry. | Nod: readiness feels like settling. Avoid: channel tiles, white menu choreography and exact health-page arrangement; no repeat of option 32. |
| Wii U: continuity into a destination | The surrounding atmosphere can continue while the destination becomes interactive. | Keep clouds/orbs moving under a save-data graph as the camera eases to rest; graph interactions need not restart the loop. | Nod: persistence across a boundary. Avoid: Mii scenes, rounded icon plaza and Nintendo typography. |
| Xbox 2001: organic energy under restraint | Use a small region of pressure and release, with the camera watching instead of diving through a branded aperture. | Stereo energy in the user's BIOS sound brightens two ordinary existing orbs near the selected tower. | Nod: stored energy becomes attention. Avoid: green membrane, X opening and the sound's recognisable growl; no repeat of option 29. |
| Xbox 360: convergence and coherence | Several asynchronous sources can arrive at agreement before a final still frame. | ROM, card and disc readiness indicators settle beside the skyline, only for inputs actually loaded. | Nod: independent arrivals forming one verdict. Avoid: spherical Xbox badge, green trails and copied ribbon trajectories; deeper than option 30. |
| PSP: lightness and atmospheric drift | Separate slow background motion from crisp foreground interaction; phase continuity matters more than a wave shape. | Hold cloud phase while save titles or the hand-off card change; interpolate camera velocity through the transition. | Nod: a persistent world underneath actions. Avoid: recognisable XMB wave, horizontal icon system and bundled PSP sounds. |
| PS3: broad musical arrival | Place the visual resolution at the end of a musical phrase, and allow a long low-motion tail. | Existing rendered BIOS PCM determines a release envelope; camera settles into an owned tower field before the tail ends. | Nod: phrase → repose. Avoid: PS3 coldboot title, matching wave shader, font and orchestral imitation. |
| PS Vita: layered depth and inertia | Foreground attention and background motion may have different response times. | Selected tower's highlight follows immediately; cloud drift and camera catch up with a bounded lag. | Nod: depth through differing inertia. Avoid: floating bubbles, page peel and Vita home-screen organisation. |
| PS4 / PS5: threshold into a personal session | Treat acknowledging readiness as a meaningful transition, not an excuse for another logo spectacle. | User confirms a selected scenario; the source-derived hand-off sentence becomes the stationary destination. | Nod: resolve → choose → enter. Avoid: blue particle funnel, branded button prompt, profile-avatar layout and PS5 light vocabulary as a package. |
| 3DO: staged theatrical reveal | Withhold the final subject while a sequence of viewpoints prepares its scale. | Begin very close to a tower face, pull back to a group, then show the full card skyline; title remains visible. | Nod: changing scale tells a story. Avoid: 3DO letters, tumbling brand shapes and original choreography. |
| Neo Geo CD: waiting has its own identity | Give indeterminate waiting a bounded repeatable gesture instead of a false percentage. | A single orb repeats a short loop while a real disc parse runs; parsed facts replace the loop when ready. | Nod: honest loop with an exit. Avoid: monkey mascot, juggling objects and SNK chime. |
| Sega CD: recursive space / showcase energy | Let a camera transition connect two representations of the same data rather than introduce another unrelated scene. | A title ID in the record table becomes the selected tower label as the camera moves into the field. | Nod: table → world continuity. Avoid: rotating Sega wordmark, chequered floor and original music. |
| Atari Jaguar: one emphatic arrival | An extremely short action can work if the arrival is conclusive and followed by quiet. | Selected save's title appears once, followed by a stationary tower and provenance; use the BIOS sound already loaded. | Nod: decisiveness. Avoid: animal, red claw language, jaguar cry or branded letter movement. |
| Game Boy: alignment as reassurance | A descending element meets a fixed datum; arrival means alignment succeeded. | A chosen save title approaches its own date/count row, settles, then opens the corresponding tower view. | Nod: registration/alignment. Avoid: Nintendo text drop, pale green palette and two-note chime imitation. |
| Game Boy Advance: gathering into a compact mark | A distributed field can briefly organise around a small focal subject before releasing it. | Nearby history towers provide changing parallax around one actual title; keep tower locations unchanged. | Nod: attention contracts, then releases. Avoid: coloured Nintendo letters, matching scatter timing or sound. |
| Nintendo DS: two spaces share one clock | A single event can be understood across two adjacent views without both views being the same representation. | Audio waveform on one side, tower attention on the other; linked cursor uses the existing sequence clock. | Nod: complementary views. Avoid: dual-screen hardware frame, Nintendo symbols and health-screen recreation. |
| Switch: a concise junction | A small joining gesture can signal that two independent inputs now belong to one action. | Card record and disc ID move into one plain source card when they match; show absence or ambiguity honestly. | Nod: inputs meeting. Avoid: paired Joy-Con silhouettes, red field, Switch logo and its click; distinct from option 36's snap cuts. |
| Early PC BIOS POST | A sequence of tests can narrate capability before promising readiness; counts need not be decoration. | POST writes become individual beads with operation labels; local parse progress is a separate channel. | Nod: earned readiness. Avoid: a fake RAM test, invented hardware readings or an OEM POST-screen copy. |
| Amiga Kickstart: wait as an affordance | A static request can still make the next action obvious and inviting. | Missing BIOS/card/disc is represented by the app's own ordinary file slot beside the scene. | Nod: waiting invites the useful action. Avoid: hand-and-disk illustration, bouncing checkmark or historical screen replica. |
| Mac startup chime | Sound can assert continuity before much is visible; its release can carry the transition. | Map the user's boot PCM release to camera braking, with a quiet visual substitute when muted. | Nod: continuity announced by owned audio. Avoid: borrowed chime, Apple silhouette or deliberately similar intervals/timbre. |
| Demoscene intros | Temporal structure, register-sized constraints and multiple readings of one signal can be the subject. | A note/pan event moves attention; the same event becomes a byte/address label on inspection. | Nod: show the mechanism's beauty. Avoid: another group's production, tracker module, scroller art or claims of cycle accuracy. |

### Three rules that make the homages more than camera presets

[I] First, choose a grammar by the data available: sparse card → anticipation; dense card → selective attention; missing disc → waiting affordance.
[I] Do not automatically supply a replacement mascot, reference-console colour or sound when the user's inputs are absent.
[I] Second, use the user's clock: note events or envelope landmarks anchor motion; historical recordings supply no copied timestamps.
[I] A chime's loudness is not evidence of successful disc authentication, so readiness indicators must follow actual app facts.
[I] Third, use geometry honestly: existing towers remain records, orbs remain atmosphere, and user-owned logos retain their real source.
[I] A generic cube rolling into an implied GameCube logo is still a conspicuous imitation even if textured with card data.
[I] Likewise, arranging PS2 ribbons into an X or a Dreamcast spiral uses an owned asset to copy someone else's identity.
[I] Prefer names describing the action—“settle on release”, “held anticipation”, “table into skyline”—with reference consoles in optional notes.
[I] Offer a lower-motion version of each treatment: stationary camera, bounded opacity change and the same event labels.
[I] No new image assets, console audio packs or logo models are needed for these proposals.

## Part B — things the research lets a user feel

### B01 — a machine can look new while carrying an old centre

[V] E01/E02 show identical sampled kernel bytes beneath changing shells and later host packaging; POPS changes padding in the old region.
[I] Fix the equal kernel window at the centre of a small visual and slide loaded shells around it; clicking a byte gives both file offsets. [I] The pleasure is continuity: the visible era changes, a verified piece does not. Dates alone must not establish identity. [I] This extends revision diff with spatial invariance; it does not imply every PS1 version has identical bootstrap bytes.

### B02 — a “least played, oldest” story hides a procedural memory

[V] E03 keeps an old date floor when a smaller count arrives; a later tie can therefore select a newer member of the lowest-count group.
[I] Animate a cursor across the 21 rows; two small gauges carry count floor and date floor, while the victim outline follows actual control flow. [I] A three-row teaching case `(8,33), (2,97), (2,65)` selects row 1, although row 2 has the older date of the two count-2 records. [I] Let users compare a mathematical minimum with the console scan on a scratch history; this is more instructive than ghost towers alone.

### B03 — the skyline can keep time after it stops growing

[V] E04 updates a matching title's date even when its normal full-mask count has saturated; fully index-7 history refuses an unfamiliar insertion.
[I] Play the next launch: height stays fixed while the date plaque changes; a new title arrives at the edge and is shown as “not recorded”. [I] This makes the limit emotionally legible without pretending an absent record proves the game was never played. [I] Distinguish six-bit-mask saturation, index-7 status and malformed/non-full-mask records; “all counts are 63” is insufficient evidence.

### B04 — the future can sort before the past

[V] E03 compares through signed `short`; the same build's `HistoryDateStamp`, `0x201E68`, packs the year displacement into bits 9 upward.
[I] Encoded 2063-12-31 is `0x7F9F`; 2064-01-01 is `0x8021`, interpreted as −32,735 in that comparison. [I] A scratch date dial crosses the seam; the newest plaque suddenly becomes the earliest candidate in an equal-count pair. [I] Label the calendar consequence inferred from the verified arithmetic; do not claim an observed console RTC can be set to 2064.

### B05 — a valid filesystem can be invisible to a small reader

[V] E05 bounds directory visibility by both bytes and parsed records, including records the loop actually counts rather than just useful filenames.
[I] Draw the actual directory as a byte strip with a movable 2 KB window and forty slots; distinguish byte-limit loss from slot-limit loss. [I] Reorder a scratch directory entry and watch the boot filename enter or leave the window; do not alter the disc image. [I] This teaches a physical constraint that a green “ISO is valid” badge misses; it is an experience inside the planned disc audit.

### B06 — a near-match can silence a real instruction

[V] E06: `BOOT2 = wrong` before `BOOT = right` ends BOOT's prefix search at line 1 but fails the `=` test at the `2`.
[I] Show “prefix found” and “assignment accepted” as separate steps; the caret stops on `2` and the later line dims as unreachable to this search. [I] Move the valid BOOT line earlier in a scratch editor and the result changes; `TCBX` similarly shadows a later `TCB`. [I] This corrects the survey's acceptance claim and gives R4 builders a concrete case to verify before adopting the note's grammar.

### B07 — absence, zero and defaults are different events

[V] E07 clears the three words before parsing, reads bare hexadecimal digits, and preserves trailing BOOT material in RAM `0x180`.
[I] Offer a three-state row: absent config, present config with absent key, explicit zero; show the resulting word and whether a value was assigned. [I] For `TCB = 10`, sixteen small cells fill; the text-to-number step supplies the explanation without a paragraph of parser jargon. [I] An argument's journey can be followed to a small RAM destination card; its presence does not prove a game consumes it.

### B08 — “opened successfully” need not mean “loaded successfully”

[V] E08 distinguishes the outer loader's success path from a rejected unaligned payload read whose return value is ignored.
[I] A segment starts to descend toward memory, then remains outlined when the read fails; the attempted execute arrow still advances. [I] Show “read rejected; execute still attempted”, with exact size modulo 2 KB, instead of a reassuring executable-header badge. [I] Keep this a data-flow illustration; no machine execution or prediction of the resulting instruction stream is needed.

### B09 — boot has punctuation, not a universal metronome

[V] E09 verifies ordered POST writes around operations, rather than measured durations or a percentage scale.
[I] Make a bead for every write occurrence; repeated numeric values remain separate events with their own call-site label. [I] Let a user tap forward event by event; optional pulses use an explicitly chosen BIOS instrument and synthetic timing. [I] This extends the planned narration with a tactile rhythm; never call it a recording of serial traffic or real POST timing.

### B10 — the reassuring picture comes before the final gate

[V] E10 places a conditional second check before execute, after the shell return path represented by the boot main.
[I] Hold the licence screen while an ordinary app arrow approaches a second gate; choose pass/fail for the declared drive response. [I] A displayed logo can remain visible while the later path fails; the graphic is reassurance, not proof that hand-off occurred. [I] Reuse Scenario and hand-off facts; expose the controller-dependent flag as an assumption, with the code branch beside it.

### B11 — the same emblem can have different authors

[V] E11 shows retail reading disc model sectors and POPS choosing the embedded shell model while retaining disc text.
[I] Two small badges beside one rendered model say “model from disc” or “model from BIOS”; selecting a host changes the source edge. [I] If both loaded models are identical, explicitly show equality and leave the picture still; a provenance change is already the surprise. [I] An optional user-supplied alternate model is a labelled illustration only; never fabricate a discovery from a real disc.

### B12 — presentation is not comparison

[V] E12 shows a retail 3.0 A shell with a comparison loop present but flag zero; PSone 4.5 uses a letter-driven flag.
[I] “Read”, “show”, and “compare” are three separate lamps; a displayed logo lights the first two without necessarily lighting the third. [I] Clicking “compare” follows the flag store and branch, giving users a reason they can inspect rather than a version-number superstition. [I] This is a compact extension to the licence-policy card, not a synthetic region-bypass feature.

### B13 — a pause can be a host's contribution

[V] E13 adds 120 waits in POPS; E14 resets the SCE event clock to 89/74 after its first dispatch rather than replaying all intervening fields.
[I] Show waits as a held segment and event-clock jumps as a discontinuity; do not squeeze both into “animation duration”. [I] A small rail has display field, note-table tick and app playback seconds; seeking highlights their relationship. [I] This helps explain why copying a note list literally can produce the right pitches at the wrong moment.

### B14 — eight tiny instructions can change a shared program's promises

[V] E15 verifies identical unpacked images and the E loader's eight stores, including replacing `0x2024E4` with zero.
[I] Stage the comparison in three views: packed source, common unpacked image, then eight bright marks in execution order. [I] Each mark links to the affected hand-off decision; explanations of the patch purpose should retain the survey's evidence/interpretation split. [I] This is a reveal of process, beyond option 40's byte diff; it avoids guessing why Sony made the change.

### B15 — a setting exists in bytes before the machine accepts it

[V] E16 supplies checksum rejection and retry mechanics; E17 shows that the supplied current blocks pass.
[I] In a scratch block, remove the checksum's high bit and watch identical settings diverge at the checksum gate; restore it to recover acceptance. [I] The archived PCSX2-default example can be an explicitly attributed inferred scenario; the later zero-result/wizard chain is not runtime-verified here. [I] The app should distinguish stored block, accepted block and resulting interpretation, and stop at unknown response behaviour.

### B16 — a region is several routes, not one colour

[V] E18/E19 provide `EEengEE`, question-mark replacement, a DVD suffix return and an O-versus-A display/identity discrepancy.
[I] Give each observed character its own consumer edge; hovering file `0x186` highlights DVD entry selection rather than recolouring everything. [I] Selecting the O entry shows `3.10O` as the lookup/comparison identity and `3.10A` as display text, with both file offsets. [I] This extends NVM inspection with a small routing performance and avoids turning the last display letter into a region verdict.

### B17 — one physical-looking file can be another file's reflection

[V] E19 verifies whole-file ROM1/ROM2 equality for this supplied set, not the chip configuration of every model.
[I] Collapse the two rectangles into a mirrored pair; “same bytes” is the label, while the dumper-window explanation stays inferred. [I] A missing EROM should remain separately missing even when ROM2 is present; file count is not capability count. [I] This gives the existing completeness check a memorable visual without inventing another ROM explorer.

### B18 — the old script is still there after the performance stops

[V] E23 finds monitor words in the oldest dump and a clearing range that covers their copied RAM positions.
[I] Animate those words arriving in the kernel shadow and disappearing under the clear; leave their ROM locations available to inspect. [I] Offer a “preserve shadow” illustration, openly counterfactual, that reveals text only and never pretends to install executable monitor commands. [I] This makes dead data feel archaeological rather than promising hidden buttons or a serial loader.

### B19 — type can outlive the interface that used it

[V] E21 verifies equal sampled glyph bytes across five inputs, including different hosts and console generations.
[I] Render a user-entered supported glyph once; relay provenance labels between equal loaded font windows without changing its pixels. [I] Let the user inspect a missing codepoint as missing; font equality is established by local byte comparison rather than a shipped fingerprint catalogue. [I] This gives the font atelier a cross-generation encounter rather than another contact sheet.

### B20 — extraction boundaries can be beautiful without pretending to know contents

[V] E20 supplies forty valid gzip islands; E24 supplies a readable outer PUP table and a fifty-file update tar.
[I] Use crisp byte intervals to show what is parsed, inflated or merely opaque; high entropy alone never earns an “encrypted” label. [I] Tapping a verified EROM member expands its measured compressed and inflated lengths; PUP intervals stop where plaintext parsing stops. [I] These are byte maps inside BIOS/PUP inspection, never a guessed PS3 splash, decrypted-content browser or authentication claim.

### B21 — an apparently exotic dump can carry a mundane accident

[V] E22 verifies a newline insertion at `0x24A`, a size change and CRLF density; the archived note reports additional hand-edited strings [I here].
[I] Keep instruction-word boundaries stationary while an inserted CR pushes all following bytes sideways; the intact header remains visually deceptive. [I] Animate one boundary failure, not an automatic “repair”; retain the original input and mark any normalisation only as an illustration. [I] This extends audit with a causal explanation and does not convert a damaged file into a purported working BIOS.

## Reuse and limits the coordinator can plan around

[I] Ready surfaces: history records, `PlayHistory::launch` effects, date/count graphs, tower tint, selected tower, Scenario, hand-off sentences and the sequence clock.
[I] Ready rendering primitives: textured/untextured quads, lines, billboards, glass, existing orbs/clouds, per-path focus/distance/zoom and the PS1 model path.
[I] Ready audio inputs: rendered stereo PCM, arranged clips, per-channel RMS/peaks, waveform and spectral analysis; motion can be driven without microphone input.
[I] A pose or visibility treatment requires new animation plumbing; a path dial by itself cannot implement object deformation, joints or rigid-body physics.
[I] Save-title typography requires decoding card filenames/titles where unavailable; do not infer a commercial title from a bare history ID.
[I] Treat card dates as last-launch dates, and saturated counts as bounded records; do not reconstruct an entire personal timeline from them.
[I] New trace readers should live near the data readers, but this report does not prescribe product edits while builders own that work.
[I] For unfamiliar BIOS builds, unmatched trace sites must show “not located”; do not silently apply a known-build address table.
[I] Drive outcomes, controller dates and host responses are declared inputs whenever the loaded files cannot establish them.
[I] Exact console frame rates do not establish the duration of filesystem reads, device readiness or any chosen sonification.
[I] No proposal requires fetching cover art, shipping BIOS assets, decrypting SCE packages or adding an EE/IOP emulator.

## Part C — ranked concrete features

[I] Ranking uses delight 1–5 divided by effort weights S=1, M=3, L=8; ties favour current engine reuse and clearer source evidence.
[I] S means a bounded addition, M a new reader/rendering path, L a substantial subsystem; these are relative estimates, not delivery promises.
[I] Features dependent on R2/R4/R6 say so; feature cost is the addition, with those existing campaign foundations assumed where named.
[I] **Build first: #1, #2, #3.** #1 exposes a verified surprise using an existing simulator; #2 turns existing owned PCM into a new kind of delight; #3 supplies a correction and demonstrable grammar case for builders already doing R4.
[I] If R4's parser interface is not yet available, #3 can remain a small educational trace; it must not claim to have judged an entire disc.
[I] Every card below is a proposal [I]; cited E identifiers identify the verified material it would display.

### 1. ★ Watch the victim change — B02; delight 5, S, score 5

- **Sees:** a scan cursor, persistent count/date gauges and the selected victim; a clearly labelled ideal-sort toggle reveals the disagreement.
- **Reuses:** alternate-history launch/undo, record table, tower highlight and `LaunchEffect`; extends options 23–24.
- **Needs:** per-row decision snapshots matching E03; a three-row scratch demonstration and a current-card trace.
- **Stance:** user's records or openly synthetic scratch rows; date-floor behaviour cites PS2 2.00 E `0x201E98`; no card writes.

### 2. ★ Stereo lantern — A Xbox/DS/demoscene; delight 5, S, score 5

- **Sees:** two existing orbs brighten with left/right BIOS audio energy; attention passes between nearby towers as channel balance changes.
- **Reuses:** `Analysis::measure`, stereo RMS/peaks, sequence clock, orb billboards and selected tower.
- **Needs:** bounded amplitude-to-opacity mapping, attack/release smoothing and seek-stable measurement; muted mode retains a readable rail.
- **Stance:** only PCM rendered from the user's BIOS; generic paired lights, original PS2 world, no green membrane, foreign chime or microphone.

### 3. ★ The line that silenced BOOT — B06; delight 5, S, score 5

- **Sees:** caret stops at `BOOT2`'s `2`; “prefix found” lights, “assigned” stays dark, later BOOT is shadowed; reorder two scratch lines to recover it.
- **Reuses:** Disc tab's SYSTEM.CNF text, hand-off card and R4 parser work; extends option 49 with a concrete corrected rule.
- **Needs:** E06's recogniser/assignment trace and an R4-facing case for malformed-prefix shadowing; complete-disc verdict waits for R4.
- **Stance:** source text or labelled sample; no executable/disc modification; cites PS1 3.0 A `0xBFC00B7C`, not the erroneous survey sentence.

### 4. Last date, same height — B03; delight 4, S, score 4

- **Sees:** a saturated matching record changes date without growing; all-index-7 scratch history declines an unfamiliar title at the skyline edge.
- **Reuses:** `PlayHistory::launch`, launch effects, undo, tower renderer and dated record table.
- **Needs:** two explicit animations for date-only update and dropped insertion; distinguish mask, count and index conditions from E04.
- **Stance:** counterfactual launches are labelled; no assertion of lifetime launch totals or never-played games; no card writes.

### 5. One record, three scales — A N64/3DO; delight 4, S, score 4

- **Sees:** one tower edge, its group, then the full history field; its ID/date remain anchored through the pullback.
- **Reuses:** free/scripted camera poses, crane, focus/zoom dials, tower selection and existing layout.
- **Needs:** three bounded camera stages and a persistent egui label; no new model or physical tower motion.
- **Stance:** user's tower geometry and real record; no N emblem, 3DO shapes, borrowed timing or implied N64 BIOS movie.

### 6. Settle on the release — A Dreamcast/Mac/PS3; delight 4, S, score 4

- **Sees:** the world resolves, the camera brakes while the user's chime decays, and the final pose stays useful for inspection.
- **Reuses:** stereo PCM/clips, analysis, current camera paths and timeline.
- **Needs:** offline release-landmark estimate, bounded easing and a visual timing control for silence; timing is an interpretation.
- **Stance:** no reference-console audio, spiral, wave or wordmark; measured audio energy never stands for drive authentication.

### 7. Hold the anticipation — A GameCube; delight 4, S, score 4

- **Sees:** openly displayed hold duration changes a pre-reveal pause or camera tempo, then converges to the same selected record.
- **Reuses:** playback rate/clock, camera dials, egui input and current scene.
- **Needs:** capped gesture duration, deterministic replay value and stationary reduced-motion variant.
- **Stance:** app interaction explicitly labelled; no claim of a PS1/PS2 hidden button, copied GameCube combo, cube route or alternate voices.

### 8. Kernel island — B01; delight 4, S, score 4

- **Sees:** equal loaded kernel byte windows stay fixed while differing shell strips orbit around the centre; header and padding have separate treatment.
- **Reuses:** BIOS identity/layout surfaces from R2/R6, ordinary egui geometry and local input bytes.
- **Needs:** range equality and differences computed across loaded files, with E01/E02 as the teaching example; baseline R2/R6 assumed.
- **Stance:** no shipped identity hashes, global sameness claim or copied console visuals; exact compared windows are visible.

### 9. Who supplied the picture? — B11; delight 4, S, score 4

- **Sees:** source edges switch between disc model and BIOS model while disc text stays attached to its own source; equality may leave pixels still.
- **Reuses:** existing PS1 model renderer and disc logo data; R2/R3 standalone/POPS front ends when available.
- **Needs:** actual model-origin field and a small source diagram; E11 supplies the comparison; no new host emulation.
- **Stance:** all displayed models/text come from loaded inputs; alternate user models are explicitly illustrative, never a manufactured disc finding.

### 10. Glyph relay — B19; delight 4, S, score 4

- **Sees:** one supported user-entered glyph stays unchanged while provenance passes between equal font ranges from different loaded hosts.
- **Reuses:** KROM decoder/font atelier foundation, BIOS input selection and egui display.
- **Needs:** runtime range equality, glyph-offset lookup and a small relay animation; font atelier foundation assumed.
- **Stance:** glyph bytes extracted at runtime; missing codepoints stay missing; no font pack or catalogue of expected Sony hashes.

### 11. Read / show / compare — B12; delight 3, S, score 3

- **Sees:** three lamps independently explain why a visible logo may be read and shown without comparison.
- **Reuses:** PS1 licence screen, verdict card, Scenario and R3's per-shell policy.
- **Needs:** policy-to-operation flags and links to the located flag store/compare branch from E12; R3 foundation assumed.
- **Stance:** source-derived policy only; a version label alone never establishes the flag; unknown shells remain unknown.

### 12. Letter meets its row — A Game Boy/GBA; delight 3, S, score 3

- **Sees:** selected history ID or decoded save title settles against its actual date/count row, then the corresponding tower gains focus.
- **Reuses:** record table, current labels, tower picking and camera focus.
- **Needs:** one text-position interpolation and selection continuity; title decode is optional, ID remains an honest fallback.
- **Stance:** user's text, neutral app typography and BIOS audio if chosen; no Nintendo wordmark, palette or copied two-note confirmation.

### 13. The year where “new” becomes “early” — B04; delight 3, S, score 3

- **Sees:** scratch calendar crosses 2063/2064; unsigned date rises while signed comparison falls; the candidate outline jumps.
- **Reuses:** alternate-history dates, record table, E03 scan and tower highlight.
- **Needs:** signed/unsigned date display and a two-record teaching case; calendar consequence remains inferred.
- **Stance:** never promises a console RTC range or observed future behaviour; original dates and card remain unchanged.

### 14. POST as beads — B09 / A PC/demoscene; delight 3, S, score 3

- **Sees:** one bead per write occurrence with stage/address labels; event stepping works even when the same code appears again.
- **Reuses:** hand-off timeline, current highlighted fact and BIOS-tab detail; extends planned boot narration.
- **Needs:** build-located write-event list from E09 and optional chosen-instrument pulses with clearly synthetic timing.
- **Stance:** reconstructed static path, not captured serial traffic, hardware progress percentage or a real-time POST recording.

### 15. A pause has an owner — B13; delight 3, S, score 3

- **Sees:** a POPS “120 waits” span and separate display-field/note-tick/app-time cursors, including the intro clock discontinuity.
- **Reuses:** sequence clock, scrubber, PS1 note playback and R3/R5 extracted timeline foundations.
- **Needs:** host hold metadata and event-clock transform from E13/E14; baseline R3/R5 assumed.
- **Stance:** exact waits and tick reset cited; elapsed seconds depend on declared rate; no claim to measure a real drive's waiting time.

### 16. Display letter versus route letter — B16; delight 3, S, score 3

- **Sees:** hovering NVM's DVD suffix highlights its selected ROM1 entry; O's identity `3.10O` sits beside display `3.10A`.
- **Reuses:** BIOS ROMDIR table, planned NVM/DVD panel, existing text/line primitives.
- **Needs:** two string roles and located consumer edge, from E18/E19; NVM inspector foundation assumed.
- **Stance:** labels preserve distinct sources and semantics; neither display suffix nor absent input earns a universal region verdict.

### 17. Mirror, then missing — B17; delight 3, S, score 3

- **Sees:** equal supplied ROM1/ROM2 rectangles fold together; EROM presence remains an independent slot, so duplication adds no invented capability.
- **Reuses:** BIOS-set completeness foundation, module list and byte comparison.
- **Needs:** whole-file equality predicate and a small folding animation, with differing files left separate.
- **Stance:** “same bytes” verified locally; hardware/mapping explanation labelled inferred; no automatic deletion or reassignment of files.

### 18. The newline that moved a machine — B21; delight 3, S, score 3

- **Sees:** inserted CR at `0x24A` pushes bytes across fixed instruction boundaries while the header still looks plausible.
- **Reuses:** R2 audit, BIOS detail/hex display and egui animation.
- **Needs:** earliest divergence, CRLF-density evidence and a local two-file alignment illustration from E22; R2 assumed.
- **Stance:** input remains untouched; no repaired-ROM export, bootability guarantee or claim that every CRLF pair is corruption.

### 19. Ready means settled — A Wii/Wii U/Xbox 360; delight 3, S, score 3

- **Sees:** real ROM/card/disc parse completions settle their ordinary source indicators; clouds keep moving as the destination becomes clickable.
- **Reuses:** background loading state, egui input cards, clouds and camera easing.
- **Needs:** independent task-ready events and transition continuity; optional inputs are marked optional rather than required for success.
- **Stance:** no channels, Mii plaza or Xbox sphere; readiness belongs to app parsing, never claimed hardware boot success.

### 20. Forty seats, one sector — B05; delight 5, M, score 1.67

- **Sees:** actual directory bytes fill a 2 KB aperture and up to forty record seats; moving a scratch entry demonstrates two independent limits.
- **Reuses:** disc ISO reader, directory entries, hand-off card and R4 audit work.
- **Needs:** original record order/byte offsets, BIOS visibility traversal and scratch permutation; dots/special records count as the loop counts them.
- **Stance:** illustrates the loaded image's constraints; no disc rewrite, invented filenames or claim that every valid ISO is bootable.

### 21. Eight marks before the opening — B14; delight 5, M, score 1.67

- **Sees:** identical unpacked OSD first, then eight stores light their target decisions before the ordinary boot starts.
- **Reuses:** ROM unpacker, module-aware comparison, hand-off sources and timeline.
- **Needs:** loader-store recogniser and patched virtual-image view from E15; branch effects require checked instruction/control-flow interpretation.
- **Stance:** patches only the inspection model in memory; runtime source addresses shown; motive remains unknown rather than guessed.

### 22. Same settings, different acceptance — B15; delight 5, M, score 1.67

- **Sees:** scratch checksum loses its high bit; stored settings stay identical while acceptance diverges; observed valid input starts green.
- **Reuses:** NVM inspector foundation, scenario-like scratch state and small data-flow overlays.
- **Needs:** eight-bit checksum trace, retry/status model and explicit unknown-response stop; E16/E17 are the verified core.
- **Stance:** no NVM writes or present-day PCSX2 claim; archived default chain labelled inferred, not runtime-verified or universally applicable.

### 23. Beyond the reassuring picture — B10; delight 4, M, score 1.33

- **Sees:** licence screen holds while a later gate receives an assumed pass/fail response; verdict and picture can diverge.
- **Reuses:** PS1 licence renderer, Scenario, hand-off card and R4 gate facts.
- **Needs:** conditional pre-exec gate event and declared controller/drive inputs, grounded in E10.
- **Stance:** no SCEx/wobble verdict invented from an image; no region-bypass claim; branching is an explanation with supplied assumptions.

### 24. Load arrow outruns the bytes — B08; delight 4, M, score 1.33

- **Sees:** unaligned payload remains an outline while execution is still attempted; header acceptance and payload transfer have separate labels.
- **Reuses:** ELF/EXE segment facts, hand-off card and planned memory-map surface.
- **Needs:** PS1 read-result/ignored-result trace and size-alignment check from E08, plus an ordinary memory-strip animation.
- **Stance:** static data flow only; no emulation, arbitrary executable running or prediction of instructions from unloaded memory.

### 25. Words before the clear — B18; delight 4, M, score 1.33

- **Sees:** oldest ROM's monitor words arrive in a RAM shadow and vanish under the verified clear; inspect their surviving ROM locations.
- **Reuses:** BIOS text/hex inspection, ROM-to-RAM copy facts and egui overlays.
- **Needs:** readable-word extraction, copied-address mapping and clear-range trace from E23; optional labelled preserve-shadow illustration.
- **Stance:** never treats dead strings as installed commands; no executable monitor, serial session or unverified hidden input.

### 26. Two spaces, one event — A DS; delight 4, M, score 1.33

- **Sees:** linked waveform and selected-tower viewport respond to one sequence cursor, letting a stereo event be heard, measured and spatially felt.
- **Reuses:** audio visualiser, sequence clock, tower viewport, selection and #2's mapping.
- **Needs:** second viewport/layout and consistent time/selection handling; no new models or audio required.
- **Stance:** plain app panes; no DS bezel, health layout or Nintendo icon; visual correspondence is labelled interpretation.

### 27. Table enters the world — A Sega CD/Switch; delight 4, M, score 1.33

- **Sees:** selected table entry maintains its identity while becoming a tower label; a matching loaded disc adds a real source edge at the landing.
- **Reuses:** record selection, tower picking/camera, disc ID and existing hand-off card.
- **Needs:** coordinated screen/world projection and stable label anchoring; unmatched IDs remain visibly unmatched.
- **Stance:** user's identifiers and owned geometry; no chequered floor, Sega logo, Joy-Con shape, red field or copied click.

### 28. Zeros, words and the surviving argument — B07; delight 3, M, score 1

- **Sees:** absent/present/explicit-zero config cases produce different word/assignment states; `10` fills sixteen cells; BOOT tail reaches RAM `0x180`.
- **Reuses:** Disc SYSTEM.CNF display, R4 parser and memory-map/hand-off foundations.
- **Needs:** config-presence semantics, hex-digit trace, stack-path context and bounded argument view from E07; show unknown consumers honestly.
- **Stance:** scratch edits remain in app memory; presence of an argument is not proof of game use; no “helpful” rewrite of the disc.

### 29. Islands that really open — B20; delight 3, M, score 1

- **Sees:** EROM intervals expand only after a CRC-valid inflate; PUP outer/tar intervals remain crisp and stop at the parsed boundary.
- **Reuses:** ROM/BIOS inspection, planned EROM explorer/PUP identity card and ordinary interval charts.
- **Needs:** bounded gzip-member enumeration and compressed/inflated sizes; PUP visibility labels from actual reads; E20/E24 establish examples.
- **Stance:** no decryption, keys, guessed encrypted contents or fake PS3 startup; entropy is a measurement with no automatic semantic verdict.

### 30. A bounded waiting ritual — A Neo Geo CD/Kickstart; delight 2, M, score 0.67

- **Sees:** one existing orb repeats a short gesture while a real parse is pending; a missing file slot remains an obvious invitation to load it.
- **Reuses:** asynchronous loading, orbs, file selection and current input-source cards.
- **Needs:** task-progress/pending/error events, cancellable loop and a static low-motion alternative; completion never waits for a decorative lap.
- **Stance:** no mascot, hand/disk drawing, copied loading music or false percentage; no performance delay introduced to manufacture nostalgia.
