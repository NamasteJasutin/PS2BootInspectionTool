# Thinker Report T3: Influences & Unexplored Boot Research

## Executive Summary
This report investigates two fronts for the **PSX Boot Inspection Tool**:
1. **Motion and structural choreographies** from other boot sequences that can be adapted using **only the user's data** (play-history towers, save icons, ROM textures, SPU sounds), while strictly avoiding proprietary trade dress.
2. **Untapped realities from the boot ROM research**—poetic quirks, dead monitors, load-time hot-patches, checksum rejections, and hardware narrations—and how to translate them into interactive experiences.
3. A ranked portfolio of **22 concrete features** evaluated by Delight ÷ Effort, highlighting the top three initial builds.

---

## Part A — Boot Choreography Influences (Motion & Structure Only)

Homages must borrow purely kinetic geometry, particle physics, acoustic timings, and input chords—never proprietary artwork, logos, or audio trademarks.

### 1. GameCube: Multi-Pad Chords & Polyhedral Stepped Bounces
- **Motion/Structure Idea**: The boot sequence's timing and mood are governed by held controller inputs (e.g. holding 'Z' on pad 1 triggers squeaking toy samples; holding 'Z' on all 4 pads invokes kabuki clappers). Mechanically, a tumbling polyhedral bounding box rolls across a planar surface, each facet impact triggering an acoustic impulse that synchronises the camera's stepped easing.
- **Engine Mapping**: Map keyboard chords (or gamepads) at boot to swap SPU-2 soundfonts using the user's own BIOS VAB programs (e.g. SCPH-10000 `rom0:OSDSND` or PS1 VAB program 1 sweep vs program 0 orchestra hit). The camera steps through 90° orbital ticks around the user's save-history towers, each tick ringing a pitch-shifted note from the user's ROM chime.
- **Trade Dress vs Respectful Nod**: Avoid the purple cube, rolling 'G' path, and spinning child's block motif. A respectful nod captures the rhythmic stepped deceleration and the delight of hidden controller chords revoicing the synth.

### 2. Nintendo 64: Isometric Monogram Spin & Edge Specular Sweeps
- **Motion/Structure Idea**: A continuous isometric 3D rotation of an extruded central letterform, illuminated by an orbiting directional light source whose steep grazing angle highlights geometry bevels and edges before camera convergence.
- **Engine Mapping**: Group the user's top four most-played save towers into an extruded geometric monogram at the coordinate origin. Orbit a grazing specular light source while slowly rotating the assembly in isometric projection (`ortho` camera), catching highlights along the tower bevels.
- **Trade Dress vs Respectful Nod**: Strictly avoid the 64-facet N logo or primary colour faces. The structural idea is purely isometric rotation with sharp rim-lighting on the user's tallest save monoliths.

### 3. Wii U: Clustered Plazas ("WaraWara") & Activity Swarms
- **Motion/Structure Idea**: The camera begins in an elevated celestial overcast, then swoops vertically downward into bustling, separated plazas where avatar swarms gather under floating topical billboards, accompanied by a distant murmur of crowd audio.
- **Engine Mapping**: Spatial partitioning of save towers into "Publisher Plazas" (SLUS, SCES, SLES, SCPS clusters). As the camera cranes down from cloud altitude, ambient sound generates an aggregate granular murmur synthesized from the user's save icon titles and launch frequencies.
- **Trade Dress vs Respectful Nod**: Avoid Mii models, speech bubbles, and sterile white UI placards. The homage borrows only the macro-to-micro descent and spatial grouping of save data by publisher lineage.

### 4. Neo Geo CD: Radial Seek Radar & Stepper Cadence
- **Motion/Structure Idea**: A high-contrast radial beam sweeps 360° across a circular display, revealing data nodes in angular sequence. The sweep angular velocity locks to the rhythmic seek pulses of the optical drive stepper motor.
- **Engine Mapping**: Arrange towers in a circular ring (`TowerLayout::Ring`). A luminous radial radar wedge sweeps the circle; as the beam intersects each tower, it triggers an optical seek click (from the CDVD driver's step timings) and highlights the save title.
- **Trade Dress vs Respectful Nod**: Avoid SNK logos, juggling monkey animations, or Neo Geo brand marks. The homage is the mechanical synchronisation of drive seek latency with circular visual discovery.

### 5. Game Boy / GBA: Cascade Drop, Checksum Settle & Palette Shifting
- **Motion/Structure Idea**: A high-altitude vertical drop that decelerates rapidly, bouncing into position; if the cartridge header checksum verifies, an audible chime rings and colour blooms outward. Holding button combinations during boot shifts the hardware palette registers.
- **Engine Mapping**: Save towers drop from off-screen into the grid like falling pistons, settling with an elastic bounce. When the boot disc passes `sceCdReadKey` [V], a chime reverberates and the shader shifts from monochrome amber to full colour. Holding keys (e.g. 1–9) during boot remaps the shader palette lookup table.
- **Trade Dress vs Respectful Nod**: Avoid scrolling Nintendo banners, pixelated font trade dress, or GBA startup sweeps. The homage is the gravitational drop-and-lock settling and pad-controlled palette registers.

### 6. Atari Jaguar: Red Wireframe Grid Surge & Low-Frequency Swell
- **Motion/Structure Idea**: The camera hurtles forward across an infinite illuminated wireframe ground plane at breakneck velocity, accompanied by a rising low-frequency resonant drone that peaks as a polyhedral silhouette is illuminated.
- **Engine Mapping**: Enable `Wireframe` shader mode across the floor grid and save towers. Drive the camera on a high-speed low-altitude dash down the central corridor of the tower grid while the audio engine plays a pitch-dropped resonant SPU drone derived from the PS2 boot chord fundamental.
- **Trade Dress vs Respectful Nod**: Avoid the red jaguar feline roar, 3D textured cat head, or Atari logotypes. The homage is the hyper-speed wireframe corridor fly-through and bass resonance.

### 7. Amiga Kickstart (1.3/2.0): Stepper Motor Cadence & Media-Wait Pulses
- **Motion/Structure Idea**: A stark visual waiting loop governed by the physical periodic polling rhythm of a floppy drive track-0 sensor (`click... click... whirr`), waiting for media insertion.
- **Engine Mapping**: When the scenario is set to "No Disc / Tray Open", the engine renders a resting tower silhouette pulsing in sync with the CDVD drive's tray polling interval (S-cmd `0x05` / `0x1F402005` status checks [V]). Each poll emits a soft mechanical stepper tick through the audio mixer.
- **Trade Dress vs Respectful Nod**: Avoid the iconic Amiga hand holding a 3.5" floppy disk or the Kickstart 2.0 purple disk graphic. The homage is the visceral, physical cadence of an idling optical drive mechanism pacing the UI.

### 8. Early PC BIOS POST: Sequential Memory Count-Up & Diagnostic Beeps
- **Motion/Structure Idea**: Monospace hardware diagnostic scan where memory addresses increment rapidly in high-contrast monospace text, each subsystem check marked by an audible square-wave beep before the video mode switches.
- **Engine Mapping**: Prior to the opening scene, an optional "POST Diagnostics" overlay streams the actual RDRAM verification count (`# Total accessable memory size: 32 MB` [V, `kernel_eeload_hooks.md` §1.2]) and POST byte progression (`0x1F802041` [V]) with synthesised 800 Hz diagnostic beeps.
- **Trade Dress vs Respectful Nod**: Avoid IBM, AMI, or Award copyright headers. The homage is the brutalist, functional transparency of hardware counting RAM and checking subsystem registers before aesthetic hand-off.

### 9. Classic Demoscene: Sine-Wave Plasma & Copper Raster Gradients
- **Motion/Structure Idea**: Amiga Copper-style horizontal scanline color gradients oscillating across the screen height, while a real-time sine-wave heightmap displaces vertex meshes in sync with tracker music tempo.
- **Engine Mapping**: Modulate the background atmospheric fog with horizontal gradient bands that gently cycle in tone. Displace the heights of save towers using a composite 2D sine wave `y = y_base + A * sin(x * w1 + t) * cos(z * w2 + t)`, animating the skyline like a liquid surface.
- **Trade Dress vs Respectful Nod**: Pure algorithmic graphics technique originating from public demoscene culture; carries zero corporate trade dress risk.

### 10. PlayStation 4/5: Luminous Vacuum Aurora & Breathing Dust
- **Motion/Structure Idea**: Organic, flowing ribbon auroras floating in an endless dark vacuum, accompanied by slow-drifting illuminated dust motes whose pulsation rate breathes at the frequency of human resting respiration (0.2 Hz).
- **Engine Mapping**: Expand the PS2 opening orb dust particles into an ambient field of motes that drift around the towers, scaling and brightening to a 0.2 Hz sine wave. Render the PS2 logo ribbons as elongated floating trails weaving through the towers.
- **Trade Dress vs Respectful Nod**: Avoid Sony's DualShock blue color scheme, PS5 gold curved light bars, or PlayStation button scatter. The homage is the tranquil respiration-rate particle pulsation and ribbon trajectories.

---

## Part B — What the Boot Research Presents That Nobody Would Think Of

### 1. The Eternal 1995 Kernel: Dec 4, 1995 Frozen Across Three Generations
- **The Finding**: In 15 PS1 dumps across 6 years, bootstrap and kernel bytes `0x00000..0x17FFF` are byte-identical (`2f471835...`) from Version 2.2 (Dec 1995) to PSone 4.5 (May 2000), and identical in the PSP 6.60 POPS image (`PSXONPSP660.BIN`) [V, `ps1_version_matrix.md` §0]. `GetSystemInfo(0)` returns `0x19951204` on every console manufactured for over a decade [V, `ps1_kernel_boot.md` §1.6]. The PSone and PSP shells merely wear a 1995 kernel underneath.
- **User Experience**: A **"Kernel Time-Capsule"** lens. Clicking the kernel cluster highlights how the code executed on a 2011 PSP Go or a 2000 PSone is running instructions written on Monday, December 4, 1995 by engineer "K.S." (`CEX-3000/1001/1002 by K.S.` [V]). A visual timeline slider scrubs 16 years while the kernel byte strip remains utterly frozen, contrasting with the shell versions evolving above it.

### 2. The PCSX2 NVM Checksum Rejection & The Zero-State Ghost Boot
- **The Finding**: PCSX2's default generated NVM files calculate an invalid 7-bit checksum for OSD config block area 1 (`chk = sum & 0x7F` at byte 15) [V, `rom1_erom_nvm.md` §3]. Real IOP CDVDMAN rejects this block upon boot, causing retail OSDSYS to ignore the default settings entirely and fall back to hardcoded zeroes [V]. Emulated boots unknowingly run in an uninitialised default fallback state.
- **User Experience**: A **"Checksum Divergence" Toggle** in the NVM Inspector. The user can flip between PCSX2's invalid checksum and a corrected MechaCon checksum. The UI dynamically highlights how OSDSYS reacts: triggering the first-boot setup wizard (bit `0x11.7` [V, `osdsys_hooks.md` §4.1]), altering language selection, or silencing the SPDIF digital sound command (`0x1031` [V]).

### 3. The SCPH-30004R Load-Time Mutator: 1.60E's 8-Word Hot-Patch
- **The Finding**: European SCPH-30004R (ROM 1.60E) unpacks an OSDSYS image byte-identical to 1.60A, but its loader stub at file `0x230..0x2A3` writes 8 MIPS words directly into decompressed RAM before `ExecPS2` [V, `osdsys_hooks.md` §6.1]. These patches replace branch instructions with unconditional jumps (`0x2022F4`, `0x20230C`), completely neutering `sceCdReadKey` illegal-disc detection, bypassing `BOOT2` title-ID validation (`0x2024E4`), and routing missing `SYSTEM.CNF` straight into the DVD player (`0x20237C`) [V].
- **User Experience**: A **"Stub Mutator Live Diff"**. In the Scenario Bar, clicking "Apply 1.60E Hot-Patches" shows the disassembly mutating in real-time. An invalid or mismatched disc that triggers the red warning screen on a 1.60A console suddenly bypasses copy-protection and launches into the game or DVD player on 1.60E.

### 4. Boot Narration & The Audible POST Stepper (`0x1F802041`)
- **The Finding**: PS1 boot writes single-byte diagnostic codes to hardware register `0x1F802041` across every stage: `F -> E -> 1 -> 2 -> 3 -> [1 3 4 5 6 2] -> 4 -> 5 -> 6 -> 7 -> [0 1 2 5] -> 8 -> 9` [V, `ps1_kernel_boot.md` §1.3]. Meanwhile, retail kernels call `printf` across boot stages, routed to a dummy TTY device (`A0:99`, `k:2870` [V]) that discards the characters.
- **User Experience**: A **"Boot Telemetry Narration"** overlay. As the boot sequence plays, the app prints the silenced TTY log in real-time (`PS-X Realtime Kernel Ver.2.5`, `KERNEL SETUP!`, `Configuration : EvCB 0x04`, `TCB 0x10`, `BOOT = cdrom:\...` [V]). Simultaneously, each POST byte transition emits an authentic diagnostic tick, letting the user literally *hear* the BIOS progressing through hardware setup.

### 5. The 40-Entry Root Directory Cliff & SYSTEM.CNF Prefix Parsing
- **The Finding**: The PS1 BIOS ISO9660 reader (`rom:BFC07700`) reads exactly **one 2048-byte sector** for the root directory and stores at most **40 file records** [V, `ps1_kernel_boot.md` §3.1]. If `SYSTEM.CNF` or the boot executable is file #41, the console hangs in an infinite loop with POST byte `0xF` [V]. Furthermore, `SYSTEM.CNF` parser uses line prefixes rather than exact keys (`BOOT2` satisfies `BOOT`) and parses values as un-prefixed hexadecimal (`TCB = 10` allocates 16 blocks) [V, `ps1_kernel_boot.md` §4].
- **User Experience**: A **"Disc Hand-Off Linter"** overlay. The user inspects their disc image: if `SYSTEM.CNF` sits past sector offset 2048 or directory record 40, the tool renders a red warning banner: *"Invisible to BIOS Root Sector (Entry #42 > 40)"*. A parser simulator demonstrates how `TCB = 10` is silently ingested as 16 threads.

### 6. The MechaCon Physical Wobble Error (`0x30 / 0x37`) & The Red Screen Gate
- **The Finding**: On PS2, the iconic "red screen" warning is not an OSDSYS visual check; it is triggered when EELOAD receives return code `-2` from XLOADFILE (`el:82548` [V, `kernel_eeload_hooks.md` §2.2]). XLOADFILE checks `cdrom0:sce_dev5` via `sceCdReadKey(0, 0, 0x4B)` [V, `kernel_eeload_hooks.md` §2.3]. Error byte `0x30` or `0x37` from the MechaCon registers physical track wobble failure (anti-piracy check), returning `-2` and invoking `BootIllegal` [V].
- **User Experience**: A **"MechaCon Wobble Simulator"**. In the Disc Tab, the user toggles wobble validation. When corrupted wobble is simulated, the telemetry card traces the exact execution chain: `sce_dev5 -> ioctl2(0x10020) -> error byte 0x37 -> return -2 -> EELOAD BootIllegal -> OSDSYS ctx[0x74] -> Warning Scene` [V].

### 7. 1.00 J Serial Narration & The Memory Card Replaceable Browser
- **The Finding**: SCPH-10000 (1.00J) EELOAD outputs verbose execution logs directly to EE SIO UART (`0x1000F180`) on every boot, narrating `# Loader 'rom0:OSDSYS':pc=00200008` and `# LoadExec '%s':pc=%08x` [V, `kernel_eeload_hooks.md` §2.2]. Furthermore, 1.00J OSDSYS checks `mc?:/BIEXEC-SYSTEM/OSBROWS` at boot, parsing a 20-byte text descriptor to replace the entire browser module (`MBROWS`) with a card-resident KELF at any chosen RAM address [V, `osdsys_hooks.md` §0].
- **User Experience**: A **"1.00 J Serial Teletype Terminal"** window. The user watches the boot sequence through the green phosphor glow of the serial port as the EE outputs its raw load logs. An interactive module loader shows how dropping an `OSBROWS` descriptor into a virtual memory card hijacks the OSD browser before retail FreeMCBoot mechanisms were ever conceived.

### 8. The Ghost ROM Monitors (PS1 1.0 J Monitor 2.3 & PS2 TBIN Monitor 2.6)
- **The Finding**: SCPH-1000 (1.0 J) contains a full ~2,000-instruction interactive ROM debug monitor (`exec`, `load`, `sector`, `mem`, `pad`, `led`, `dbc` commands) [V, `ps1_version_matrix.md` §0, §2]. Stripped in 2.2, its command table still survives as dead text in the kernel shadow [V]. Similarly, the PS2's `TBIN` module (the IOP's PS1-mode bootstrap) contains ROM monitor 2.6 (1999) [V, `ps1_version_matrix.md` §1].
- **User Experience**: A **"Ghost Monitor Console"**. The app provides an interactive prompt that revives the dead commands against the user's loaded BIOS image. Users can type `mem <addr>`, `sector <lba>`, or `exec` to query the BIOS exactly as a Sony hardware engineer did on a development board in 1994.

### 9. PS1DRV Hidden Per-Title Compatibility Tables & `TITLE.DB`
- **The Finding**: The PS2's PS1 driver (`rom0:PS1DRV`) contains hardcoded per-title compatibility tables (182 game records in 2.00E, 71 in 1.60A, 175 in 1.00J) encoding custom GPU/SPU timing fixes [V, `games_probe.md` §3]. Additionally, it checks memory cards for a `TITLE.DB` database and parses hidden `PSD1.0.0` directive lines in `SYSTEM.CNF` [V, `games_probe.md` §3].
- **User Experience**: A **"PS1DRV Title Matcher"** panel. Inserting any PS1 disc checks its serial against the user's specific PS2 BIOS table. If matched, the tool highlights the exact patch flags applied (e.g. CD speed clamping, texture filtering overrides). Users can also inject an alternate `TITLE.DB` to simulate community patches.

### 10. The Mirrored ROM2 & The 40 Gzip Secrets of EROM
- **The Finding**: In European slim consoles (SCPH-70004), `ROM2` is a byte-for-byte duplicate mirror of `ROM1` (`82dc50fa...`) because the hardware lacks a ROM2 chip, causing the SSBUS chip select to float [V, `rom1_erom_nvm.md` §0]. Meanwhile, the mysterious 3 MB `EROM` image is not encrypted: it contains **40 standard CRC-valid gzip archives** containing the complete unencrypted DVD Player executable at `0x200000`, parental controls, and EEPROM modules [V, `rom1_erom_nvm.md` §2].
- **User Experience**: An **"EROM Decompressor & Hardware Mirror Explorer"**. An interactive archive inspector unpacks all 40 gzip streams live from the user's EROM dump, extracting DVD player ELF strings and parental control tables without needing MagicGate keys. A bus schematic visualises why ROM2 reflects ROM1.

---

## Part C — Ranked Concrete Feature List

Features are ranked in descending order of **Delight ÷ Effort** (high-impact, low-cost features first).

### 1. Boot Telemetry Narration & POST Stepper
- **What User Sees**: A toggleable side HUD displaying silenced kernel `printf` strings timed to boot frames, accompanied by soft synthesized diagnostic ticks matching POST byte transitions (`0x1F802041`).
- **Reuses**: Existing timeline scrubber, SPU mixer/synth, hand-off card.
- **New Data/Code**: Table of PS1/PS2 kernel format strings and POST step sequence mappings (`ps1_kernel_boot.md` §1.3).
- **Effort**: **S**
- **Stance Check**: Fully compliant. Replays plaintext strings from user's dump and public register sequences; zero proprietary code shipped.

### 2. Multi-Chord SPU Boot Easter Eggs (GameCube Homage)
- **What User Sees**: Holding key combinations (e.g. Z, K, 1–4) at startup triggers alternate boot soundscapes generated from different VAB programs inside their own BIOS (e.g. Program 1 sweep vs Program 0 hits, pitch shifts).
- **Reuses**: Existing CPAL audio pipeline, SPU-2 / VAB parser, sound synthesis worker.
- **New Data/Code**: Input chord listener mapped to SPU note/program triggers; preset instrument chord tables.
- **Effort**: **S**
- **Stance Check**: Fully compliant. Uses only the user's BIOS VAB audio waveforms and generic input chords.

### 3. Disc Hand-Off Linter & 40-Entry Cliff
- **What User Sees**: In the Disc Tab, a diagnostic card audits `SYSTEM.CNF` grammar (prefix match, un-prefixed hex) and verifies whether the boot executable sits within the root directory's first 2048 bytes (40-entry limit).
- **Reuses**: Disc parser, ISO9660 PVD reader, UI diagnostic cards.
- **New Data/Code**: Linter validation rules from `ps1_kernel_boot.md` §3.1 & §4; directory entry index counter.
- **Effort**: **S**
- **Stance Check**: Pure algorithmic analysis of user's disc image against kernel constraints.

### 4. 1.60E Stub Mutator Live Diff (SCPH-30004R)
- **What User Sees**: In the Scenario Bar, a "1.60E Stub Hot-Patch" toggle mutates the OSDSYS launch code in real-time, showing how 8 patched MIPS words allow illegal/unmatched discs to launch without warnings.
- **Reuses**: Scenario bar, OSDSYS decompile viewer, hand-off verdict engine.
- **New Data/Code**: 8 patch words from `osdsys_hooks.md` §6.1; conditional branch override logic.
- **Effort**: **S**
- **Stance Check**: Pure code disassembly analysis derived from user's ROM stub bytes.

### 5. NVM Checksum Rejection & Zero-State Ghost Boot
- **What User Sees**: In the NVM Inspector, a checksum audit flags invalid PCSX2 7-bit checksums, with a toggle to preview how the BIOS falls back to hardcoded zeroes vs real MechaCon state.
- **Reuses**: NVM viewer tab, OSD configuration model.
- **New Data/Code**: 7-bit checksum algorithm (`chk = sum & 0x7F`) from `rom1_erom_nvm.md` §3; fallback state simulator.
- **Effort**: **S**
- **Stance Check**: Algorithmic check on user's NVM dump.

### 6. MechaCon Physical Wobble Error Simulator
- **What User Sees**: A toggle in the Disc Tab to inject error bytes `0x30` / `0x37` into `sceCdReadKey`, demonstrating the exact hardware gate that triggers the red warning screen.
- **Reuses**: Warning screen renderer, Scenario bar, hand-off flow.
- **New Data/Code**: Hand-off step for MechaCon wobble verification (`kernel_eeload_hooks.md` §2.3).
- **Effort**: **S**
- **Stance Check**: Verification model of hardware S-commands.

### 7. Radial Seek Radar & Stepper Cadence (Neo Geo CD Homage)
- **What User Sees**: In `TowerLayout::Ring`, a 360° radar sweep illuminates save towers in radial succession, accompanied by drive stepper motor click pulses pacing the rotation.
- **Reuses**: Ring layout renderer, audio mixer tick generator, tower highlight shaders.
- **New Data/Code**: Radar sweep shader uniform, CDVD step cadence acoustic timings.
- **Effort**: **S**
- **Stance Check**: Kinematic sweep geometry and synthetic stepper audio; zero proprietary art.

### 8. Cascade Drop & Checksum Palette Modulation (GBA Homage)
- **What User Sees**: Save towers drop into the scene with an elastic bounce. Holding number keys at boot modulates the UI/tower shader colour palette dynamically.
- **Reuses**: Tower physics/positioning, shader uniform buffers, keyboard input handler.
- **New Data/Code**: Elastic spring physics interpolation for tower Z-drop; 8 procedural palette lookup tables.
- **Effort**: **S**
- **Stance Check**: Motion and shader math; no Nintendo trade dress.

### 9. 1.00 J Serial Teletype Terminal
- **What User Sees**: A retro teletype terminal displaying the exact EE SIO UART serial feed emitted by SCPH-10000 EELOAD on cold boot.
- **Reuses**: Boot logging UI pane, timeline scrubber.
- **New Data/Code**: 1.00 J UART string table and format generator (`kernel_eeload_hooks.md` §2.2).
- **Effort**: **S**
- **Stance Check**: Derives text from user's 1.00J ROM image.

### 10. PS1DRV Compatibility Table Inspector
- **What User Sees**: A table view listing the 182 / 71 / 175 game titles patched by the user's PS2 BIOS, with active match highlighting when a disc is loaded.
- **Reuses**: BIOS inspection tab, disc serial matching.
- **New Data/Code**: Parser for PS1DRV embedded string table (`games_probe.md` §3).
- **Effort**: **S**
- **Stance Check**: Parses user's own `rom0:PS1DRV` module.

### 11. Wireframe Corridor Fly-Through (Atari Jaguar Homage)
- **What User Sees**: A camera flight mode rushing at breakneck speed down the central avenue of wireframe towers over an infinite neon grid, backed by an SPU resonant drone.
- **Reuses**: Camera path system, wireframe rendering pipeline, SPU audio engine.
- **New Data/Code**: Linear high-velocity camera path; ground plane infinite grid shader.
- **Effort**: **M**
- **Stance Check**: Abstract wireframe aesthetic and camera motion; no Jaguar trademarks.

### 12. Publisher Plaza Clusters & Audio Swarms (Wii U Homage)
- **What User Sees**: Save towers spatially grouped into regional/publisher plazas (SLUS, SCES, SLES, SCPS). Camera descends from clouds into active plazas buzzing with granular audio.
- **Reuses**: Tower layout engine, cloud fog shaders, audio mixer.
- **New Data/Code**: Plaza clustering layout algorithm; granular audio synthesis from save title text.
- **Effort**: **M**
- **Stance Check**: Spatial clustering and synthetic sound; no Wii U assets.

### 13. EROM 40-Member Gzip Inspector
- **What User Sees**: In the BIOS tab, an EROM explorer lists and inflates all 40 unencrypted gzip members, revealing DVD player version strings and parental control tables.
- **Reuses**: BIOS module tree, hex viewer.
- **New Data/Code**: Gzip stream decompressor over raw EROM image (`rom1_erom_nvm.md` §2).
- **Effort**: **M**
- **Stance Check**: Uses standard gzip decompression on unencrypted user data; zero MagicGate bypass.

### 14. Kernel Time-Capsule Matrix (1995-12-04 Frozen Code)
- **What User Sees**: A side-by-side binary and structural diff view showing that the PS1 kernel across 15 retail dumps, PSone, and PSP POPS is byte-identical from Dec 1995.
- **Reuses**: Two-dump diff viewer, ROM layout strip.
- **New Data/Code**: Byte cluster classification and `GetSystemInfo` verification logic.
- **Effort**: **M**
- **Stance Check**: Comparative analysis of user's own BIOS files.

### 15. Demoscene Sine-Wave Liquid Skyline
- **What User Sees**: Save towers undulate gracefully in real-time like a liquid surface displaced by composite sine waves, backdropped by cycling horizontal Copper raster bars.
- **Reuses**: Tower vertex buffer updates, background gradient renderer.
- **New Data/Code**: GPU vertex shader height displacement uniform; Copper gradient generator.
- **Effort**: **M**
- **Stance Check**: Public mathematical demoscene technique.

### 16. Ghost ROM Monitor Interactive Console
- **What User Sees**: An interactive command prompt emulating the dead 1994 ROM monitor (`mem`, `sector`, `load`) against the loaded BIOS and disc.
- **Reuses**: Memory map inspector, disc sector viewer, UI console pane.
- **New Data/Code**: Text command parser and dispatcher based on `ps1_kernel_boot.md` §1.4.
- **Effort**: **M**
- **Stance Check**: Clean-room emulator of documented monitor CLI.

### 17. 1.00 J Memory Card Browser Hijack Simulator (`OSBROWS`)
- **What User Sees**: An interactive card injection simulator showing how placing an `OSBROWS` descriptor file on a memory card redirects the 1.00J OSDSYS browser module at boot.
- **Reuses**: Memory card explorer, OSD module layout viewer.
- **New Data/Code**: Descriptor parser and module slot override logic (`osdsys_hooks.md` §0).
- **Effort**: **M**
- **Stance Check**: Recreates documented Sony descriptor protocol on user card data.

### 18. Stepper Cadence "Waiting for Media" Loop (Amiga Homage)
- **What User Sees**: When "No Disc" is selected, towers pulse faintly in rhythm with the CDVD drive's periodic polling interval, accompanied by mechanical stepper pulses.
- **Reuses**: Scenario bar, SPU audio engine, tower lighting animation.
- **New Data/Code**: Periodic polling timer linked to S-cmd `0x05` frequency; stepper sample synthesis.
- **Effort**: **M**
- **Stance Check**: Mechanical rhythm and lighting pulse; zero Amiga art.

### 19. Isometric Monogram Spin & Edge Specular Sweeps (N64 Homage)
- **What User Sees**: An orthographic camera mode spinning the top four save towers around the origin while a steep grazing light illuminates their geometric bevels.
- **Reuses**: Camera system (`ortho` projection), lighting shaders, tower layout.
- **New Data/Code**: Monogram grouping layout; rotating directional specular light pass.
- **Effort**: **M**
- **Stance Check**: Geometric rotation and lighting; no proprietary N-logo shape.

### 20. Luminous Vacuum Aurora & Breathing Dust (PS4/PS5 Homage)
- **What User Sees**: Floating ribbon auroras weave among the save towers while ambient dust particles pulse gently at human resting breathing frequency (0.2 Hz).
- **Reuses**: Ribbon renderer, particle system, atmospheric fog.
- **New Data/Code**: 0.2 Hz harmonic scale/opacity particle curve; ribbon spline path generator.
- **Effort**: **M**
- **Stance Check**: Particle physics and harmonic timing; no PlayStation trademarked symbols.

### 21. Early PC POST Memory Countdown Overlay
- **What User Sees**: Prior to boot, a brutalist monospace diagnostic counter scrolls through detected RDRAM sizes and checks hardware registers with square-wave beeps.
- **Reuses**: Text renderer, SPU synth.
- **New Data/Code**: POST sequence state machine; square-wave beep tone audio generator.
- **Effort**: **M**
- **Stance Check**: Generic functional diagnostic overlay.

### 22. Full Director Mode Spline Editor with Homage Presets
- **What User Sees**: A Catmull-Rom spline keyframe timeline permitting custom camera paths, with one-click presets for all console homage choreographies.
- **Reuses**: Camera system, timeline scrubber, file import/export.
- **New Data/Code**: Spline curve evaluation, JSON keyframe schema, trajectory preset library.
- **Effort**: **L**
- **Stance Check**: Pure camera kinematic data.

---

## The Top Three to Build First & Why

1. **Boot Telemetry Narration & POST Stepper (#1)**
   - *Why*: Delivers immediate emotional and intellectual delight for very low effort (**S**). Turning silent kernel `printf` strings and POST byte writes (`0x1F802041`) into a synchronised visual/acoustic narration transforms the boot from a passive replay into an intelligible engineering event.
2. **Disc Hand-Off Linter & 40-Entry Cliff (#3)**
   - *Why*: Solves a concrete, mystifying historical failure mode with high utility (**S**). Explaining why discs fail to boot due to root-directory sector limits (entry #41 invisible) or `SYSTEM.CNF` prefix quirks bridges the gap between inspection and actionable disc auditing.
3. **Multi-Chord SPU Boot Easter Eggs (#2)**
   - *Why*: Pure delight and engine flex (**S**). GameCube demonstrated how hidden input chords create unforgettable tactile boot experiences. Mapping keyboard chords to revoice the startup chime using different programs from the user's own BIOS VAB banks makes the tool deeply playful while respecting Sony assets.
