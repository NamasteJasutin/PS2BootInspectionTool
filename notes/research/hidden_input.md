# T1 — hidden input and non-obvious unlocks in PS2 OSD and PS1 shells

Thinker: Codex. Room: .worktrees/ideas/t1-input. Date: 2026-10-10.
Scope: menu behaviour, conditional options and dormant entry paths; no product changes.

## 1. Findings that change the campaign

[V] PS2 2.00 E has a previously unresolved consumer of configuration byte 0x13 bit 7: it installs a hidden Console options record, unlocked from Version Information by Select + Start + L1 + L2 + R1 + R2 on **controller port 2, slot 0**.
The chain is OSDSYS 0x203A08 → 0x204468 → 0x206CC8 → 0x2266F0 → 0x206AA8.
The resulting option is **Remote Control, Off/On**, not a new boot animation.

[V] PS1 retail shells have **L1 + L2 + R1 + R2 undelete** in the memory-card manager. It restores the shell's remembered last-deleted filename through BIOS B0 function 0x46. Confirmed from 1.0 J 0x8003B9D4 through 4.5 A 0x8003ADF0; complete matrix below.
This is an actual reachable retail operation with a concrete card-directory transition.

[V] PS1 **SoundScope** is reachable with Select in the CD player in the supplied 4.0 J, 4.1 A/E and 4.5 A shells: 0x800349F0, 0x800349D4 and 0x80034D70. Its lesser-known layer records and replays controller performances, then saves/loads
a card file named BPLAYSTATION; 4.5 A 0x800427E0, 0x80042FD8, 0x80043074.

[V] The known PS2 aspect recovery recipe needs **121**, not 120, qualifying frames. It then accepts Start alone without an explicit neutral frame: 1.50 0x23F8A8,
1.60 0x240178, 2.00 0x2482B8. This corrects the earlier survey's timing wording.

[I] The most useful additions are therefore a PS1 undelete experiment, SoundScope performance inspection and a conditional two-controller option demonstration.
These extend the existing menu work without re-proposing generic NVM/TESTMODE inspectors.

## 2. Evidence, build identity and method

[V] All addresses below are virtual addresses in the named module, unless explicitly marked ROM/file offset. PS2 OSD means the unpacked EE image at 0x200000. PS1 shell addresses mean the unpacked shell at 0x80030000.
PS1 ROM addresses beginning BFC are locations in the user's 512 KiB dump.

[V] Local reading covered the eight research surveys, options 1–64, campaign/R8, named 2.00 decompilation, PS1 kernel/shell exports and the actual local dumps.
New working evidence is retained under scratch/; neither repo/ nor bios/ was edited.

[V] scratch/ps2_census.txt records ROMVER and unpacked OSDSYS SHA-256 for nine files. The 1.60 A February, 1.60 A March and both 1.60 E files have identical unpacked OSDSYS hashes: 25a981101c803b7054f69c262d7b6ee86984946190d6717b46a35593900e69fb.
Their OSDSYS ROM starts are respectively file 0x350E50, 0x352000 and 0x34B4E0.
The E loader patches described in osdsys_hooks §6.1 still matter outside these handlers.

| Report tag | Identity read from dump | Address anchor |
|---|---|---|
| 1.00 J | [V] 0100JC20000117; scph10000 and long-name alias | OSDSYS file 0xECF70; main 0x204D68 |
| 1.50 D | [V] 0150AD20001228; DTL-H30101 TEST kit | OSDSYS file 0x349940; main 0x2075B8 |
| 1.60 A Feb | [V] 0160AC20020207; scph39001 and alias | OSDSYS file 0x350E50; main 0x207478 |
| 1.60 A Mar | [V] 0160AC20020319; filename says SCPH39004 | OSDSYS file 0x352000; same unpacked image |
| 1.60 E | [V] 0160EC20011004; two local filenames | OSDSYS file 0x34B4E0; same handlers |
| 2.00 E | [V] 0200EC20040614; SCPH-70004 | OSDSYS file 0x345B80; main 0x209EB8 |

[I] Do not infer the region/model from a download filename: the March file's ROMVER is A, despite the model number in its name. The identity above uses ROMVER.

[V] The 1.00 plugins were extracted through ROMDIR and decompressed as **raw LZ at offset zero**, not as ELF loader stubs. Retained images and entry points: MCLOCK scratch/MCLOCK.bin, base 0x600000, main 0x600628, size 0x377DC;
MBROWS scratch/MBROWS.bin, base 0x7A0000, main 0x7AD9A8, size 0x105B0C;
MOPEN scratch/MOPEN.bin, base 0x500000, main 0x500488, size 0x92C8C.

[V] Fresh Ghidra analysis/export covered these three plugins, 1.00 OSDSYS, 1.50 OSDSYS and shared 1.60 OSDSYS, producing 530/650/402/551/1665/1673 functions. Artifacts: scratch/exports/{mclock,mbrows,mopen,osd100,osd150,osd160}/.
Each contains C exports, functions.tsv and data_xrefs.tsv with virtual addresses.

[I] The supplied headless front-end rejects paths containing the hidden .worktrees segment. A local ProgramDB/AutoAnalysisManager launcher avoided disk projects. scratch/LocalAnalyse.java and local_analyse.py document the workaround.
No retained Ghidra project needs opening; the report's evidence is the exported text.

[V] Additional PS1 shell decompilations filled revision gaps: J1.1, E2.0, E2.2, J3.0 and J4.0 in scratch/exports/ps1_*/. Existing exports supplied other revisions.
scratch/ps1_census.py unpacks compressed late shells using the local core's format.

[V] The supplied A4.1 and E4.1 **entire unpacked shells are byte-identical**: scratch/ps1_census.txt hash prefix 7920530e46981bab; Select handler 0x800349D4. The A3.0 long-name dump and scph5501 unpacked shell are identical as well.
POPS differs substantially despite its kernel's 4.5 J banner.

[I] Negative findings below are scoped to inspected input consumers/call graphs. A missing string or missing decompiler reference alone is not proof of impossibility.
No physical console or emulator execution was performed in this room.

## 3. PS2: hidden port-2 Remote Control options in 2.00 E

### 3.1 Exact trigger and visible result

[V] Open Version Information through the ordinary Triangle path; highlight Console. With the hidden gate enabled, hold **Select + Start + L2 + R2 + L1 + R1** on controller 2, slot 0: active-high mask 0x090F.
OSDSYS 0x2266F0 tests (port2_buttons & required_mask) == required_mask.

[V] There is **no hold timer or ordered sequence** in this handler. The six required bits are a subset test, not equality with the entire pad word.
Up/Down/Triangle/cancel have earlier priority in the normal menu handler.
Use the six buttons alone for a deterministic demonstration; source 0x2266F0.

[V] The hidden record matches GetString(100), the Console title, against the selected ordinary record's title. It lives **after the ordinary list terminator**, so it does
not appear as a separate selectable line. Registration is 0x206CC8.
The option editor opens through 0x224678, with builder 0x206AA8.

[V] GetString(0x67) supplies the Remote Control Off/On options string; 0x206AA8 passes it into the existing option parser 0x225C30.
The data at 0x27B4A0/0x27B4B8 identifies editor strategy 2 and value id 0x12.
This is a real existing options page, not a guessed debug label.

[V] A subtle input-folding detail: 0x208DB8 builds each saved pad word from that pad's high byte but the **accumulated** low byte from earlier polled pads.
Thus Start/Select come from port 2's current packet, while required shoulder bits
can already have been supplied by earlier slots, including port 1.
[I] The six-button chord entirely on port 2 is a sufficient recipe. A split-pad
recipe (port 1 shoulders, port 2 Start+Select) follows from the code, but was not
executed on hardware. Preserve this partial merge in a faithful input experiment.

### 3.2 Three separate configuration bits

| Packed config field | Actual working field | Verified consumer in 2.00 E |
|---|---|---|
| byte 0x13 bit 7 | [V] config word 1 bit 2; getter 0x204468 | Hidden record registration 0x206CC8 |
| byte 0x13 bit 6 | [V] config word 1 bit 3; getter 0x204428 | Actual Off/On value; editor 0x225900 |
| byte 0x13 bit 5 | [V] config word 1 bit 4; getter 0x2044A8 | Ordinary System Configuration item 0x223758 |

[V] UnpackConfig 0x203A08 establishes all three mappings. The decompiler name CfgSetWord1Bit3 at **0x204480 is misleading**:
its code clears/sets mask 0x4, i.e. working **bit 2**, the hidden gate.
0x2044C0 manipulates bit 4; trust masks rather than provisional symbol names.

[V] Value id 0x11 is the working copy of the hidden gate, at 0x374A24. The configuration save/apply path 0x2318D0 preserves that value while changing the
Remote Control setting. I found no menu action that initializes a zero gate to one.

[V] main 0x209EB8 loads config, then issues CDVD S-command RPC fno 0x30 with the inverse of the Remote Control value. If its status includes 0x100, the branch at
0x20A334 clears the hidden gate and ordinary Remote Control availability bit.
Thus stored bit 7 alone does not establish that the hidden menu survives boot.

[I] A conservative reachability statement is: this is an executable retail-ROM menu path conditional on configuration and supported drive-command response.
It is not established as a universally usable retail-controller cheat.
The origin/purpose of the enabled bit remains unresolved; factory provisioning
or another software producer is a hypothesis, not verification.

### 3.3 The supplied NVM and address convention

[V] SCPH-70004_BIOS_V12_PAL_200.NVM file 0x2C0..0x2CF contains 35 21 C0 3C 20 25 00 00 00 00 00 00 00 00 00 97.
The relevant field is **file 0x2C4 = 0x20**: normal item available, hidden gate clear,
Remote Control value clear. It is not file 0x2C3.

[V] ReadConfig 0x25C948 copies records at **15 bytes per record** into the OSD buffer. Raw NVM storage includes the record checksum byte, so packed byte 0x13 maps to
the second raw record's byte 4. Do not confuse packed buffer offsets with file offsets.
The conventional layouts are documented locally; the supplied L1 sample is above.

[I] App presentation: show the hidden page beside three separate gate indicators, then let a virtual pad select controller 2 and hold 0x090F.
Default to the user's actual gate/result facts; a hypothetical gate override belongs
in Scenario, clearly identified as an alternate configuration.
Reuse the future Version Information scene and its dump-derived strings.

[V] Survival: absent in inspected 1.00 MCLOCK Version handler 0x605AC0 and its option registration; absent in 1.50/1.60 Version consumers 0x220D10/0x2215E0.
Present in 2.00 at the addresses above. No retail 1.50 dump was supplied, so the
1.50 column means the actual TEST-kit build, not all version-1.50 consoles.

## 4. PS2: exact aspect recovery state machine

[V] Required held mask is exactly **0x808F**: Left 0x8000 + Square 0x80 + four shoulders 0x000F.
This is checked in the browser, not as a sampled cold-boot combination.
Addresses: 1.50 0x23F8A8; shared 1.60 0x240178; 2.00 0x2482B8.

[V] The count starts at zero, increments on each qualifying frame, and arms when 0x78 < count. Since 0x78 is 120, the first armed count is **121**.
Before arming, another held mask resets the counter; all three handlers agree.

[V] After arming, another mask clears the counter but preserves the armed latch. A subsequent frame with **Start alone = 0x0800** commits the reset.
There is no required intervening zero-button frame in these handlers.
Keeping any extra button held with Start prevents the equality test from succeeding.

[V] Browser context predicates are required too. In 2.00, level 0x2C8DE4 must be zero and transition state 0x2C8D34 must not equal 2; leaving those conditions clears
both count and latch. Source 0x2482B8.
Do not translate the second variable into an asserted “CD player excluded” rule.

[V] The commit sets a 27-frame message state and pauses the browser through 0x248180. 0x2481A8 calls screen-type setter 0x204238(0), queues worker command 0x1A,
waits for completion and resumes. Source 2.00 0x2481A8.
The normal screen-type value zero is the 4:3 choice.

[V] A malformed old screen-type field of 3 causes setter 0x204238 to refuse a request for zero. Therefore a simulator should model the setter's validation too,
rather than claiming that the sequence repairs every possible corrupted config.

[I] With one handler call per frame, 121 frames mean approximately 2.02 seconds at 60 Hz or 2.42 seconds at 50 Hz. The threshold itself is not PAL-adjusted.
A frame-count demonstration is more faithful than a fixed two-second countdown.

[V] No counterpart was located in 1.00 MCLOCK/MBROWS held-button consumers: 0x60A2F0, 0x605AC0, 0x606118, 0x7B1238, 0x7B1A50, 0x7A8B50.
This is a bounded negative finding supported by freshly exported plugin code.

[I] App presentation: a miniature browser/Version experiment can expose count, armed latch and the final message. This is an exact extension to option 45,
not another generic request to implement the entire clock menu.

## 5. PS2: ordinary shortcuts, controller plumbing and non-pad branches

### 5.1 Newly pinned-down 1.00 menu inputs

[V] MCLOCK input decode 0x60A2F0 combines both controller ports, reads only the digital button bytes for acceptable connection states, and produces held/pressed/
released/repeat words at 0x647788/78C/790/794.
Up/Down begin repeating after count >30, then every third frame.

[V] MCLOCK main-menu handler 0x606900 opens Version Information with Triangle. System Configuration handler 0x606118 opens clock-only display with **Square**.
Clock display handler 0x6046E0 watches Square to return.
These are ordinary menu shortcuts, not unlock sequences.

[V] 1.50/1.60 System Configuration handlers 0x2270E8/0x2279B8 retain Square display. Both also accept Triangle on the first selected item for a secondary clock popup.
2.00 equivalents are 0x22DC48 → 0x22CE48 and Triangle → 0x228900.
The popup uses tween 0x293C28; exact subpanel appearance is not reconstructed here.

[V] This corrects menu_survey §1.3's inferred “Display = SELECT” label. The inspected button test is **Square 0x80**, including 1.00 0x606118 and
2.00 0x22DC48/0x22CF20. Do not propagate that earlier inference into controls.

[V] MBROWS 1.00 decode 0x7B1238 merges the same two ports and has distinct held/ new/repeat words at 0x8B5A54/58/60. Main card-grid handling is 0x7B1A50.
0x7A5E48 implements copy/delete confirmations and asynchronous card states;
0x7B0770 implements a format confirmation. No PS1-style undelete hook was found there.

[V] 1.00 CD-player handler 0x7A8B50 offers Start transport, Select command index 0, L1/R1 previous/next and L2/R2 scan controls, with normal input-exclusion masks.
Its many combinations resolve into transport/confirmation states, not a hidden screen.
No SoundScope entry or four-shoulder recovery call was found in these consumers.

[V] 1.00 parent InputThread 0x204308 reads ports (0,0)/(1,0) through 0x20B868 and connection state through 0x20B8F8.
Shared 1.60 InputThread 0x206E00 polls eight port/slot combinations via read
0x24C9A0; 2.00 0x208DB8 reads through 0x2577E8 and retains mixed per-pad words.

[I] State values 2/6 in these PS2 routines are connection states, not pad-model IDs. No analog-stick or pressure-channel unlock was located in the inspected consumers.
A controller lab should preserve this distinction when presenting decoded packets.

### 5.2 Diagnosis and version options

[V] Console/Unit Diagnosis is already an ordinary Version options row, not a combo. In 2.00 its editor/storage chain uses value id 4, RAM 0x1F1234 and request
0x1F13B8; disc worker 0x20F9F8 emits S-command fno 0x19.
Diagnosis On sends zero to the drive's auto-adjust control.

[V] 1.00 counterpart is the option-page editor 0x605008 and parent worker 0x20E300, commands 0x17/0x18. Driver options still flow through config.
These are documented hooks; there is no new secret full-ROM diagnostics page here.

[I] Surface the distinction on existing hand-off facts: “Diagnosis is temporary drive behaviour” is useful; claims that it decrypts, repairs or bypasses a disc are
unsupported by these readers. Avoid making this another proposal for option 44.

[V] The 1.50 D type byte is read by 0x203FC0, returned by 0x2040C8 and
included in the certify block at main 0x207890–0x2078AC, not tested as a menu unlock.
Its only post-reader use found is that getter/certify chain; no D-only menu was located.

### 5.3 Launch arguments

[V] 1.60 main 0x207478 compares argv[1..] to **SkipMc** at 0x2A3B20 and **SkipHdd** at 0x2A3B28, suppressing its update and HDD paths respectively.
2.00 main 0x209EB8 adds **SkipForbid** at 0x2C3C78; SkipMc/SkipHdd strings
are at 0x2C3C68/70. The matching 1.00/1.50 mains have no such words.

[V] The normal 2.00 return helper ExecOsdBootBrowser 0x202258 creates exactly one argument, BootBrowser at 0x2AF1B0, then calls ExecOSD.
It does not manufacture SkipMc/SkipHdd/SkipForbid from controller input.
1.50/1.60 analogous return helper is 0x201CE8.

[V] 1.50 update launcher 0x207340 and 1.60 0x207200 receive the original remaining arguments from main and forward them along their update hand-off.
2.00 update path is 0x209378. This is propagation, not a secret menu producer.

[I] A program that can invoke ExecOSD can supply these words, but that is a software launch condition. I found no retail menu producer or pad sequence for them.
The full surrounding application/update ecosystem was not available for producer tracing.
App: annotate incoming argv on the existing hand-off card, with origin explicit.

### 5.4 TESTMODE, first setup, DVD and other triggers

[V] 2.00 EELOAD main 0x82388 reads CDVD register pointer 0x8FF00 plus 5 and tests bit 0x10; branch 0x8258C loads rom0:TESTMODE instead of rom0:OSDSYS.
This is a hardware/status input, not a verified direct OSD-NVRAM “TESTMODE bit”.

[V] TESTMODE 2.00 pad thread 0x1003C0 accepts Select to advance six pages, Circle to change pattern state, shoulders/video adjustments and Square for a test flag.
Once TESTMODE is already running, these are reachable controls, not its entry trigger.
The inspected normal OSD input consumers do not call this program.

[I] The mapping from a factory jig or persistent firmware setting to CDVD status bit 4 is unresolved here. Option 47 already proposes the TESTMODE dossier;
this report contributes the reachability distinction, not a duplicate feature.

[V] 1.00 has no TESTMODE ROMDIR entry; its extracted EELOAD follows OSDSYS. Direct raw instruction reads also confirm 1.50 probe 0x8253C and 1.60 A probe
0x825A8: lbu +5 followed by andi 0x10, with pointer 0x1F402000.
The 1.60 E EELOAD is byte-identical to 1.50's and shares probe 0x8253C.
Sources: analysis/devkit/eeload_dev.dis and rom_dev/rom_ret160 EELOAD; E dump ROMDIR.

[V] Initial setup is selected by packed config byte 0x11 bit 7 clear, per UnpackConfig 2.00 0x203A08 and main 0x209EB8 → SetupRequired.
1.50/1.60 retain this config-layout flag; 1.00 unpacker 0x203580 lacks it.
The wizard is an initialized-state branch, not an input sequence.

[V] 2.00 DVD options 0x206800 exposes Clear Progressive Setting only when packed byte 0x13 bit 4 is set. Callback 0x225988 sets request 0x1F13C0.
LaunchDvdVideo 0x202E00 branches on it, clears the bit and queues a config write.
The export's later loop prevents asserting a completed player hand-off in that branch.

[I] Full DVD-player controller shortcuts are a separate executable/module scope; the OSD hand-off inspected here supplies launch arguments, not a discovered held-pad
progressive override. Do not import DVD-player folklore as verified OSD behaviour.

[V] 1.00 update descriptor path 0x204838 looks in BIEXEC-SYSTEM for OSBROWS. Later update path builders 1.50 0x207340, 1.60 0x207200, 2.00 0x209378 use
region/version-selected update candidates. Existing surveys already describe these hooks.
No new card filename acting as a secret menu-unlock key was found in this pass.

[V] 2.00 RTC updater 0x232030 uses clock fields and substitutes 2000-01-01 when its clock-status test fails. Clock marker rendering 0x22AC38 uses hour %12.
No holiday/birthday trigger was located in the clock input/render consumers inspected.

[I] This is not a proof that every date-dependent expression in every module is absent. The app should explain clock failure/defaulting if useful, without inventing anniversary
screens from the existence of RTC reads. Language/region differences remain known settings.

## 6. PS1: four-shoulder memory-card undelete

### 6.1 Preconditions and call chain

[V] In 1.0 J, card input callback 0x8003B9D4 requires the manager not busy, no face-button bits (mask 0x00F000F0), no Up/Down bits (0x50005000),
and all four shoulder tests: 0x00080008, 0x00020002, 0x00040004, 0x00010001.
The minimum physical chord is L1 + L2 + R1 + R2.

[V] Left/Right/Start/Select are not explicitly excluded by that callback predicate. This is not “the entire input word must equal 0x000F”.
The duplicated masks accommodate the shell's event representation; they do not prove
that two controllers are required. Digital parser output is a low 16-bit word.

[V] The remembered path must begin bu00: or bu10:; 1.0 J checks RAM 0x80086208. Deletion routine 0x8003D548 saves the chosen filename there after B0 0x45 succeeds.
The shoulder handler passes it into 0x800593C0, the B0 0x46 wrapper.

[V] The shell checks that wrapper's nonzero result, tears down pending card events through 0x8003E464, then rebuilds the manager through 0x80033000(1).
No multi-second hold counter or ordered sequence occurs in the undelete predicate.

[V] B0 table slot 0x46 at RAM 0x874 + 0x46*4 points to 1.0 J RAM 0x37A8. That dispatcher invokes device callback +0x30 and converts device result zero to
API result one. Concrete card callback is ROM **0xBFC0AC78**.
This verifies “undelete”, rather than guessing semantics from a four-button mask.

[V] ROM 0xBFC0AC78 searches deleted directory entries, expects initial state 0xA1 and deleted continuation/final states 0xA2/0xA3, and restores active states
0x51/0x52/0x53. It invokes the directory-write path 0xBFC09818.
It rejects missing/incompatible chains and an already existing live filename.

[I] The practical opportunity ends when the relevant directory/data chain is reused or the shell loses its remembered name. It is not a general filesystem recovery tool.
A read-only app can show how the original operation would alter directory flags.

[V] 4.5 A delete 0x8003C398 remembers the path at RAM 0x80090CF0. Manager initialization/refresh paths 0x8003B048, 0x8003C5C0 and 0x8003CA30 clear it.
Thus a fresh boot cannot recover an arbitrary deleted entry via this shortcut alone.

### 6.2 Version matrix and undo entry addresses

| PS1 supplied build | Card callback | Undelete survives? | SoundScope Select entry |
|---|---|---|---|
| J 1.0 SCPH-1000 | [V] 0x8003B9D4 | [V] yes → B0 0x46 wrapper 0x800593C0 | [V] not in inspected CD/menu graph |
| J 1.1 SCPH-3000 | [V] 0x8003A4E0 | [V] yes → 0x80059000 | [V] not in inspected CD/menu graph |
| E 2.0 SCPH-1002 | [V] 0x8003A78C | [V] yes → 0x8005A020 | [V] not in inspected CD/menu graph |
| J 2.2 SCPH-5000 | [V] 0x8003A644 | [V] yes; same predicate | [V] not in inspected CD/menu graph |
| A 2.2 DTLH-3000-labelled | [V] 0x8003A6D0 | [V] yes; same predicate | [V] not in inspected CD/menu graph |
| E 2.2 DTLH-3002-labelled | [V] 0x8003A7EC | [V] yes → 0x8005A300 | [V] not in inspected CD/menu graph |
| J 3.0 SCPH-5500 | [V] 0x8003A664 | [V] yes; same predicate | [V] not in inspected CD/menu graph |
| A 3.0 / scph5501 | [V] 0x8003A730 | [V] yes → 0x8005AAC0 | [V] not in inspected CD/menu graph |
| E 3.0 SCPH-5502 | [V] 0x8003A96C | [V] yes; same predicate | [V] not in inspected CD/menu graph |
| J 4.0 SCPH-7000 | [V] 0x8003A678 | [V] yes → 0x800682E0 | [V] 0x800349F0 → 0x80049BD0(1) |
| A/E 4.1 | [V] 0x8003A974 | [V] yes; identical A/E shell | [V] 0x800349D4 → 0x8004A6C0(1) |
| A 4.5 PSone | [V] 0x8003ADF0 | [V] yes → 0x80068EB0 | [V] 0x80034D70 → 0x80041DF0(1) |
| POPS PSXONPSP660 | [V] entry 0x80030000/0x80030098 | [V] no corresponding menu path located | [V] no corresponding menu path located |

[I] “Not in inspected graph” is a bounded absence, not a claim about every regional revision ever manufactured. The supplied corpus includes no J/A2.0 or E4.5 shell.
The DTLH-labelled PS1 dumps have retail shell behaviour here; their filenames do not
establish a user-accessible development monitor.

[I] App presentation: select a save in a PS1-card overlay, simulate Delete, then press four shoulders and show the last-name predicate and flag restoration.
Reuse Save-data interaction, BIOS identity and Scenario; implement a PS1 directory
reader because the current PS2 memory-card parser is not already a PS1 filesystem.

## 7. PS1 mouse support exposes a surviving bug-shaped branch

[V] A3.0 packet decoder 0x8003DA08 accepts mouse ID 0x12 and digital type families 0x20/0x40. It returns unsupported for other families, including 0x50/0x70.
The presence of analog hardware therefore does not imply an analog-stick menu unlock.

[V] In A3.0's card screen, one mouse button produces logical Circle, 0x00200020; the other produces Start, 0x08000800. When their low word is 0x0820,
the decoder synthesizes four shoulders 0x000F000F.
This supplies the undelete callback using both mouse buttons.

[V] E2.0 0x8003DBD8, A2.2 0x8003D9D8, E2.2 0x8003DC98 and E3.0 0x8003DE38 retain that 0x820 mouse-chord test.
The early Japanese branches inspected do not expose that same mouse decoder.

[V] J4.0 decoder 0x8003DB18, A/E4.1 0x8003E098 and A4.5 0x8003E418 instead map the second mouse button to **Select 0x01000100** in card/CD screens.
They retain the old comparison against low-word 0x0820.

[V] In those late decoders, possible mouse-button low words are 0, 0x20, 0x100, 0x120. None is 0x820, so the retained four-shoulder synthesis branch cannot
be satisfied by those mappings. The ordinary digital-pad undelete remains intact.

[I] Treat this as a verified unreachable comparison under the current decoder, with an inferred regression history. Do not label it an intentional removed feature
unless a separate changelog or another implementation supports that intention.

[V] A3.0 poller 0x8003DBA4 and A4.5 0x8003E5D8 prefer port 0; they try port 1 only if port 0 returns unsupported/-1.
An idle supported controller on port 0 therefore prevents selecting port 1 here.
This differs from PS2's merged-pad behaviour.

[I] App presentation: show mouse ID, raw bits, translated word and callback outcome side by side across A3.0 and A4.5. This makes a concrete version difference visible
without manufacturing a hidden boot path.

## 8. PS1 SoundScope: entry, controls and 24 selectors

### 8.1 Reachability and version boundary

[V] A4.5 CD-player callback 0x80034D70 checks usable-media flags and shell state 3, then Select (0x01000100) calls 0x80041DF0(1).
No particular track number or special sequence is required by this entry predicate.
The visualiser is a CD-player screen reached after menu initialization.

[V] J4.0 uses 0x800349F0 → 0x80049BD0(1), and A/E4.1 uses 0x800349D4 → 0x8004A6C0(1). J4.0 already has the recording decoder at 0x8004A590.
This establishes the feature in a supplied 4.0 shell, before the 4.1/4.5 shells.

[V] A4.5 entry 0x80041DF0 initializes renderer 0x80045900 and activates flag 0x80124744. Its outer menu loop 0x800302D8 routes frames to 0x80041FD8.
First entry picks a pseudorandom preset; later entry can reuse the remembered preset.
These menu frames, not the cold-boot licence path, read the visualiser controls.

[V] A4.5 selector count 0x80046B88 returns 0x18 = **24**. Left/right 0x80046C14/0x80046BD8 wrap the selector index modulo 24.
Renderer 0x80046418 maps selectors through table 0x8007B374 to dispatch
pointers at 0x8007B3D4. J4.0 count function 0x80041194 also returns 24.

[I] Call them 24 selectable presets, not 24 independently verified renderer engines. The preset-to-renderer mapping and runtime appearance still need a dedicated port.
Static shell assets alone cannot reproduce a performance driven by changing audio/input.

### 8.2 Digital controls, authoritative A4.5 addresses

| Input in SoundScope | Evaluation | Verified effect / source |
|---|---|---|
| Left / Right | [V] changed input word | Previous/next selector; 0x80041FD8 → 0x80046C14/0x80046BD8 |
| Up / Down without Cross | [V] changed input word | Decrease/increase variant; 0x80046CE8/0x80046CB0 |
| Square | [V] held each frame | Phase accumulates +0x400; 0x80046D4C |
| Triangle | [V] held each frame | Set parameter 0x800; release restores gradually; 0x80046D10 |
| Circle | [V] changed input word | Set effect counter to 35; 0x80046E7C → 0x80046418 |
| Cross + Up | [V] held subset 0x1040 | Grow scale, capped 0x300; 0x80046DA8 |
| Cross + Down | [V] held subset 0x4040 | Shrink scale, floored 0x10; 0x80046DF8 |
| R1 / L1 alone | [V] exact 8 / 4 | Advance/back through track list; 0x80038C78/0x80038914 |
| R2 / L2 alone | [V] exact 2 / 1 | CD forward/back commands 4/5; 0x80066464/0x8006650C |
| Start alone | [V] exact 0x800 | Playback toggle branches; 0x80037F5C/0x80038584 |
| Select alone | [V] exact 0x100 | Exit through 0x800427E0 → 0x80041DF0(0) |

[I] The Triangle parameter is plausibly visual persistence/fade, and Circle a short visual transition. The values and calls are verified; exact appearance is not.
Transport names “forward/back” follow their CD command numbers and state paths;
the renderer should not claim a music effect from those transport bits.

[V] A4.5 mouse motion is also consumed in 0x80041FD8: one delta changes phase by delta*0x40, another sets a variant using mouse position.
Mouse Select maps to the exit command. Score recording excludes device type 0x10,
so mouse visual control and digital recording have different coverage.

[I] Reuse the app's audio visualiser/mixer and Scene selection for an eventual dump-derived SoundScope renderer. Keep its first deliverable small: entry, preset
selection and explanatory controls; full fidelity is a larger, separate implementation.

## 9. PS1 SoundScope performance recording and the card file

### 9.1 Exact chords and arming states

[V] A4.5 decoder 0x800427E0 compares exact words **0x48 = Cross+R1** and **0x44 = Cross+L1**, on a changed input word.
A/E4.1 counterpart is 0x8004B0B4; J4.0 counterpart 0x8004A590.

[V] Cross+R1 enters record-arm state 5; complete release moves it to state 6. The first qualifying nonzero visual input, excluding mask 0x80F and Cross alone,
starts recording state 1 with the frame counter zero.
Repeating the chord in states 1/5/6 closes the recording.

[V] Cross+L1 requires a nonempty score, enters replay-arm state 7 and resets its read pointer. Release to 0, Cross alone or L1 alone starts replay state 2.
Repeating the chord in replay/armed-replay cancels replay.

[V] In 0x80041FD8, **Cross+R2 exact 0x42** requests save, provided score count is nonzero; **Cross+L2 exact 0x41** requests load, outside record states.
The shared I/O wrapper 0x80042FD8 actually operates only in idle state 0.
It tries card port 0 first, then port 1 (encoded 0x10) if the first result fails.

[V] The release guards prevent the arming chord from immediately becoming recorded visual input. These are small input state machines, unlike the PS2 hold timer.
Start and shoulder transport also have priority rules separate from visual effects.

### 9.2 Recorded data and validation

[V] A4.5 recorder 0x80042D10 writes records at RAM 0x801249D4: little-endian u32 frame, u16 duration, u16 buttons, eight bytes per slot.
It coalesces compatible adjacent spans and stops when its index exceeds 899.
It records selected digital visual controls; it does not record mouse device 0x10.

[V] Save 0x80043074 writes **0x1E80 bytes** from RAM 0x80124774. The filename at shell 0x8007ACFC/0x8007AD24 is **BPLAYSTATION**.
The title at 0x8007AD0C decodes as full-width “ＳｏｕｎｄＳｃｏｐｅ”.
This identifies the card payload as the visualiser performance facility.

[V] The save begins with SC, sets one-block header information, and supplies dump-derived palette/icon data from 0x8007AAC4/0x8007AAF0/0x8007ABB0/0x8007AC70.
The number of icon frames selected varies with shell region flag 0x80090990.

| Payload-relative offset | A4.5 RAM anchor | Verified field |
|---|---|---|
| 0x000 | [V] 0x80124774 | SC header and icon/header region; save 0x80043074 |
| 0x200 | [V] 0x80124974 | Version constant 0x00010000 |
| 0x204 | [V] 0x80124978 | Checksum from 0x8004340C |
| 0x208 | [V] 0x8012497C | Recorded count |
| 0x20C | [V] 0x80124980 | Initial scale; decoder 0x800427E0 |
| 0x210 | [V] 0x80124984 | Initial variant; decoder 0x800427E0 |
| 0x260 | [V] 0x801249D4 | Eight-byte event slots; recorder 0x80042D10 |

[V] Checksum routine 0x8004340C sums the frame word and button halfword of 900 slots, modulo the machine's 32-bit sum. It does **not** add duration.
Load 0x800432B0 reads the same length, checks version/checksum and clears the
recorded buffer through 0x80042EA8 on the corresponding invalid conditions.

[I] This checksum is an integrity convention, not proof that a duration/count is safe for a host parser. A new importer should bound all counts and frame arithmetic
independently, then present ROM-validation and host-validation outcomes separately.

[V] A/E4.1 analogues are save 0x8004B948, load 0x8004BB84, checksum 0x8004BCE0, recorder 0x8004B5E4 and I/O wrapper 0x8004B8AC.
J4.0 retains filename strings at 0x800797FC/0x80079824/0x80079834 and
the matching control state machine 0x8004A590. No matching POPS facility was located.

[I] The file records **controller performance, not music**. Exact replay requires the visualiser state, preset, audio and frame cadence too.
The inspected saved initial fields establish scale/variant; do not promise that the
card file alone captures every random seed or audio-dependent renderer state.

[I] App presentation: show the file's card icon, timed button spans and validation, then replay inputs through a virtual pad. An initial timeline-only implementation
needs no full PS1 emulator or SoundScope renderer and is independently useful.
A later renderer can make those scores audible/visible with user-selected audio.

## 10. PS1 monitor remnants, POPS and bounded dead ends

[V] J1.0 kernel entry RAM 0x500 starts zeroing the block at RAM 0x7450 (ROM 0xBFC16F50), where the old monitor command strings are stored.
The decompilation's endpoint is poorly typed, but the starting clear is explicit.
The dead command table includes exec/load/sector, DIP/LED and maintenance wording.

[V] J1.0 ROM function 0xBFC0865C formats “DIP SWITCH” through 0xBFC018D0. Its exported call graph lists no caller. Its decompiled argument is not a verified
read of a retail DIP-switch device; avoid treating that remnant as an active hardware probe.

[I] No retail pad/menu path to that monitor was found. A hypothetical injected entry point or expansion program is different from an existing retail trigger.
This does not overturn the previous boot-path survey and need not re-prove its result.

[V] POPS shell entry 0x80030000 runs initialization/licence routines; 0x80030098 calls 0x80030580/0x80030110 and waits on 0x80031304.
Its exports contain no corresponding retail card-manager shoulder callback,
SoundScope state machine or BPLAYSTATION payload. The 4.5 J kernel banner is not
evidence that the PSone no-disc menus survive into POPS.

| Checked candidate | Evidence anchor | Outcome / limit |
|---|---|---|
| Hidden 1.00 Version options chord | [V] MCLOCK 0x605AC0 and registration | No conditional tail-record gate found |
| PS1 undelete copied into PS2 browser | [V] MBROWS 0x7A5E48/0x7B0770/0x7B1A50 | No B0-style recovery operation in these handlers |
| Generic shoulder secret in early PS1 CD UI | [V] J4.0 0x800349F0; A4.5 0x80034D70 and earlier equivalents | Shoulder priority is transport; quartet undo belongs to card screen |
| PS1 mouse secret retained late | [V] J4.0 0x8003DB18; A4.5 0x8003E418 | Stale 0x820 test unreachable from current mappings |
| Analog/pressure menu unlock | [V] PS2 0x232338/0x24D7B8; PS1 0x8003DA08/0x8003E418 | No analog-channel trigger in inspected decoders |
| NVM bit alone opens port-2 option | [V] 2.00 0x203A08/0x209EB8/0x2266F0 | Requires config gate, boot support, menu context and saved input mask |
| Reset needs neutral release | [V] 1.50 0x23F8A8; 2.00 0x2482B8 | No explicit neutral frame; Start alone suffices after arming |
| Date/calendar secret scene | [V] 2.00 0x232030/0x22AC38 | RTC/default/hour behaviour found; no such branch in inspected consumers |
| Skip words produced by retail return | [V] 2.00 0x202258 | Helper supplies BootBrowser only |
| TESTMODE entered by normal-menu chord | [V] EELOAD 0x82388; OSD input consumers | Entry is CDVD status; factory origin not established |
| Retail 1.0 monitor via “dip” string | [V] J1.0 RAM 0x500; ROM 0xBFC0865C | Table clear/orphan handler; no menu route located |
| POPS has all PSone menu tricks | [V] POPS 0x80030000/0x80030098 | Corresponding menu/score paths absent from inspected shell |

[I] Deliberately not repeated: opening-skip and disc-boot pad/DIP/card negatives. Not claimed complete: all DVD-player executables, every IOP peripheral command,
runtime factory-jig behaviour, and regional BIOS revisions absent from the local corpus.

## 11. Consolidated PS2 survival matrix

[I] “—” means no counterpart found in the inspected module/consumer set; it must not become a global statement about unsupplied BIOS versions.

| Trigger / effect | 1.00 J | 1.50 D | 1.60 A/E | 2.00 E |
|---|---|---|---|---|
| Port-2 0x090F hidden Remote Control | [V] —; 0x605AC0 | [V] —; Version 0x220D10 | [V] —; Version 0x2215E0 | [V] gated; 0x206CC8/0x2266F0 |
| Aspect recovery 121 frames → Start | [V] —; plugin pad consumers | [V] 0x23F8A8 | [V] 0x240178 | [V] 0x2482B8 |
| Square clock-only display | [V] 0x606118 | [V] 0x2270E8 | [V] 0x2279B8 | [V] 0x22DC48 |
| Triangle secondary clock popup | [V] — in 0x606118 | [V] 0x2270E8 | [V] 0x2279B8 | [V] 0x22DC48 |
| SkipMc / SkipHdd launch words | [V] —; 0x204D68 | [V] —; 0x2075B8 | [V] 0x207478 | [V] 0x209EB8 |
| SkipForbid launch word | [V] —; same main | [V] —; same main | [V] —; same main | [V] 0x209EB8 |
| Initial-config wizard flag | [V] —; 0x203580 | [V] unpacker 0x202D08 | [V] same layout/main 0x207478 | [V] 0x203A08/0x209EB8 |
| Progressive-clear option bit | [V] — in Version options | [V] — in Version options | [V] — in Version options | [V] 0x206800/0x225988 |
| Factory TESTMODE | [V] absent ROMDIR module | [V] raw probe 0x8253C | [V] raw probes 0x825A8/53C | [V] EELOAD 0x82388 |

## 12. Ranked concrete app features

[I] Ranking weighs novelty, confidence, user-visible payoff and reuse. Each entry is a proposal; factual foundations are the [V] chains above.
No Sony files/assets would be bundled; every displayed native asset comes from the user's dump.

### 1. PS1 “delete, then four shoulders” experiment — effort M

[I] User sees: a PS1-card record disappear, the remembered filename remain, then L1+L2+R1+R2 restore it; side panel shows 0xA1/A2/A3 → 0x51/52/53.
Offer counterexamples: fresh manager state, reused chain and conflicting live filename.
[I] Reuses: Save-data tab interactions, Scenario controls, BIOS identity and provenance links.
[I] New data/code: PS1-card directory parser, last-deletion state, read-only card overlay,
a small B0-undelete model and per-build callback anchors from §6.
[I] Stance check: native card icon/title from the user's card; any shell graphics from
their shell; simulate operations on an overlay and retain the source image untouched.

### 2. SoundScope performance-file inspector and replay timeline — effort M

[I] User sees: detected BPLAYSTATION record, dump-derived icon/title, version/checksum, timed button spans and a moving virtual pad; invalid/truncated payloads explain their failure.
[I] Reuses: Save-data inspector/export UI, Scenario time controls and source-backed facts.
[I] New data/code: bounded 0x1E80 payload reader, §9 offsets/checksum, PS1-card lookup,
frame-span visualization and a replay event stream; no full renderer required initially.
[I] Stance check: only user card data and user BIOS icon/text are decoded; no music
or stock score ships. Export a new report/score only from user-provided or simulated input.

### 3. Conditional PS2 controller-2 Remote Control demonstration — effort M

[I] User sees: Console highlighted, two virtual ports, six-button chord and the hidden Off/On editor; factual gate lights explain actual versus hypothetical availability.
[I] Reuses: BIOS/NVM facts, Scenario, hand-off explanation and upcoming Version scene.
[I] New data/code: packed-to-raw offset mapping, per-pad word support, three-bit gate/value
model, partial low-byte merge, fno-0x30 result and conditional menu logic from §3.
[I] Stance check: derive labels/editor strings from user OSDSYS; hypothetical NVM changes
stay inside Scenario. This is a narrow extension to options 43/45, not a new NVM editor.

### 4. SoundScope virtual-pad scene with score playback — effort L

[I] User sees: Select from the CD-player context enters the dump's 24 selectable presets; pad/mouse controls and a recorded performance modify the running scene.
[I] Reuses: existing audio visualiser, mixer, renderer, Scene selection and Scenario timeline.
[I] New data/code: unpacked-shell tables, preset dispatch/math port, GPU primitive mapping,
audio input model and §8/§9 control states; validate output against a separate reference later.
[I] Stance check: derive native tables/fonts/icons at runtime; use the user's disc/audio,
or silence/user-selected audio. Ship reconstructed algorithms, no Sony audiovisual assets.

### 5. Cross-version controller-packet experiment — effort M

[I] User sees: A3.0 mouse buttons synthesize four shoulders; A4.5 buttons yield 0x120 and fail the stale 0x820 comparison; port preference and unsupported types are visible.
[I] Reuses: BIOS revision selector, Scenario virtual input and hand-off provenance cards.
[I] New data/code: §7 packet translators, one small mouse model, port arbitration and
a trace showing raw bytes → word → callback predicate; no general serial emulator.
[I] Stance check: original explanatory UI and user-dump code facts; no copied controller
artwork is needed. Native shell reproduction is optional, not a dependency.

### 6. Frame-accurate aspect-recovery lesson — effort S

[I] User sees: hold-mask/count/armed state, 120-frame failure, 121-frame success and Start-alone commit, with PAL/NTSC time and malformed-setting counterexamples.
[I] Reuses: Scenario time controls, current aspect presentation and future browser scene.
[I] New data/code: §4 state machine and screen-type setter validation, plus the build matrix;
a diagnostic overlay can precede full native Reset-message rendering.
[I] Stance check: no shipped native message texture; labels from the user's OSD when
rendered. This refines option 45 with a concrete behaviour and corrected timing.

### 7. Trigger-origin sentences on existing hand-off cards — effort S

[I] User sees: “launch argument”, “initialized-state flag”, “DVD setting”, “CDVD status” or “menu input”, each attached to the exact source and reachability limit.
[I] Reuses: existing disc/BIOS hand-off cards, source links and forced Scenario outcomes.
[I] New data/code: a compact per-build trigger catalog for §3–§5 and §10, including
BootBrowser versus Skip words and the unresolved TESTMODE provisioning origin.
[I] Stance check: explanatory facts and user-derived labels only; no new factory boot
simulation implied. Adds precision to options 44/47 and existing hand-offs.
