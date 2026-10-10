//! File-side predictions of PS2 boot hooks, with research provenance.
//!
//! `osdsys_hooks.md` §§0–7, `kernel_eeload_hooks.md` §§1–2, `games_probe.md` §3
//! and `hidden_input.md` §§3–5 describe the inspected 1.00/1.50/1.60/2.00 builds.
//! Presence is confirmed in the user's unpacked programs, not inferred from ROMVER alone.
//! A firing file probe is an attempted hand-over: live drive status, card authentication
//! and successful executable loading are outside this reader. One supplied card is examined;
//! the other port is not assumed empty. Missing optional inputs mean unknown, not absent hardware.

use crate::bytes::Bytes;
use crate::disc::{DiscImage, DiscKind};
use crate::locate::ProgramImage;
use crate::memcard::MemoryCard;
use crate::nvm::{BiosSet, DvdVersion, Nvm, RomVersion};
use crate::rom::RomDir;
use crate::{Error, Format, Result};
use std::fmt;

/// A documented hook, including unreachable remnants and software launch arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Hook {
    /// Version-selected memory-card system executable.
    SystemUpdate,
    /// Accepted memory-card DVD-player override; launch still needs DVD media.
    CardDvdPlayer,
    /// DVD-player launch from ROM1/EROM.
    RomDvdPlayer,
    /// Sector-zero HDD boot image hand-over.
    HddBoot,
    /// Orphan ROM HDDOSD program.
    HddOsd,
    /// Dead flash-OSD hand-over in the inspected 2.00 build.
    FlashBoot,
    /// EELOAD factory/test-program dispatch from live CDVD status.
    TestMode,
    /// BootIllegal warning argument and its XLOADFILE disc-key producer.
    BootIllegal,
    /// Disc-type dispatch and SYSTEM.CNF hand-over to PS2LOGO or PS1DRV.
    DiscLaunch,
    /// Eight-store SCPH-30004R or three-store March-2002 loader launch relaxations.
    LaunchRelaxations,
    /// 1.00 versioned card-browser descriptor and module replacement.
    CardBrowser,
    /// 1.00 generic launch code -2, for replacement plug-ins.
    GenericLaunch,
    /// PS1-driver card compatibility database.
    TitleDatabase,
    /// PS1-driver SYSTEM.CNF parameter override.
    Ps1Parameters,
    /// Hidden controller-2 Console options page, gated by packed bit 0x13.7.
    HiddenRemoteControl,
    /// Ordinary Remote Control menu item, gated separately by bit 0x13.5.
    RemoteControlItem,
    /// DVD-player progressive-setting clear page.
    ClearProgressive,
    /// First-setup wizard selected by the initialized-state flag.
    FirstSetup,
    /// Incoming argument suppressing the card update probe.
    SkipMc,
    /// Incoming argument suppressing HDD probing.
    SkipHdd,
    /// Incoming argument suppressing sceCdForbidDVDP.
    SkipForbid,
    /// Unreachable or ineffective Initialize launch argument.
    Initialize,
    /// BootError/BootClock/BootBrowser/BootOpening/BootWarning launch vocabulary.
    LaunchArguments,
    /// Dead host helper-module directory branch.
    HostModules,
    /// Conditional Chinese ROM2 font/asset loader.
    ChinaAssets,
    /// Browser aspect recovery input sequence.
    AspectRecovery,
    /// Clock-only display and secondary popup input paths.
    ClockInputs,
    /// Ordinary Version-page Diagnosis drive command.
    Diagnosis,
}

impl fmt::Display for Hook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SystemUpdate => "Memory-card system update", Self::CardDvdPlayer => "Card DVD player",
            Self::RomDvdPlayer => "ROM DVD player", Self::HddBoot => "HDD boot", Self::HddOsd => "Orphan HDDOSD",
            Self::FlashBoot => "Flash OSD boot", Self::TestMode => "TESTMODE", Self::BootIllegal => "BootIllegal",
            Self::DiscLaunch => "Disc boot dispatch", Self::LaunchRelaxations => "Loader disc-check patches", Self::CardBrowser => "Card OSBROWS browser",
            Self::GenericLaunch => "Plug-in generic launch", Self::TitleDatabase => "PS1DRV TITLE.DB",
            Self::Ps1Parameters => "PS1DRV PSD1.0.0", Self::HiddenRemoteControl => "Hidden Remote Control page",
            Self::RemoteControlItem => "Remote Control menu item", Self::ClearProgressive => "Clear Progressive Setting",
            Self::FirstSetup => "First setup", Self::SkipMc => "SkipMc", Self::SkipHdd => "SkipHdd",
            Self::SkipForbid => "SkipForbid", Self::Initialize => "Initialize", Self::LaunchArguments => "OSDSYS launch arguments",
            Self::HostModules => "Host helper modules", Self::ChinaAssets => "China ROM2 assets",
            Self::AspectRecovery => "Aspect recovery", Self::ClockInputs => "Clock menu inputs", Self::Diagnosis => "Diagnosis",
        })
    }
}

/// A prediction about the specified files under ordinary cold boot, with no injected argv/input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// The inspected file-side probe or setting gate is satisfied; execution is not guaranteed.
    Fires,
    /// A required condition fails, or the documented path is dead.
    DoesNotFire,
    /// Missing files or live hardware/software inputs prevent a decision.
    Unknown,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self { Self::Fires => "Fires", Self::DoesNotFire => "Does not fire", Self::Unknown => "Unknown" })
    }
}

/// One hook's outcome and the evidence supporting it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HookFinding {
    /// The hand-over or gate inspected.
    pub hook: Hook,
    /// File-side outcome, retaining runtime uncertainty.
    pub verdict: Verdict,
    /// The condition, inspected paths and any live-input limitations.
    pub because: String,
    /// Address in the named research reference build, followed by note and section.
    /// Located evidence in this user's program is reported in `because` when available.
    pub source: &'static str,
}

/// Version-specific findings, with malformed or unstudied ROMs reported as diagnostics.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HookMap {
    /// Identity read from the supplied ROM's ROMVER entry.
    pub romver: Option<RomVersion>,
    /// One entry per confirmed hook in the studied program family.
    pub findings: Vec<HookFinding>,
    /// Evidence gaps, including unreadable programs and incompatible companion identities.
    pub diagnostics: Vec<String>,
}

impl HookMap {
    /// Inspects one ROM and optional files; ROM1 presence is unknown through this entry point.
    /// For companion-aware DVD gates, use [`inspect_with_bios_set`](Self::inspect_with_bios_set).
    #[must_use]
    pub fn inspect(rom: &RomDir, card: Option<&MemoryCard>, nvm: Option<&Nvm>, disc: Option<&DiscImage>) -> Self {
        Self::inspect_inner(rom, card, nvm, disc, None)
    }

    /// Adds explicitly inspected BIOS companions; a missing ROM1 now closes the DVDID gate.
    /// The explicit `nvm` overrides the set's NVM; absent uses the set's decoded companion.
    /// `rom1_erom_nvm.md` §§1.4, 4–5 describes DVD selection and MechaCon suffix gating.
    #[must_use]
    pub fn inspect_with_bios_set(rom: &RomDir, card: Option<&MemoryCard>, nvm: Option<&Nvm>,
        disc: Option<&DiscImage>, set: &BiosSet) -> Self {
        Self::inspect_inner(rom, card, nvm.or(set.nvm.as_ref()), disc, Some(set))
    }

    fn add(&mut self, hook: Hook, result: (Verdict, String), generation: u16, evidence: Option<(&ProgramImage, &str)>) {
        let (verdict, mut because) = result;
        if let Some((image, text)) = evidence {
            if let Some(offset) = find(image.data(), text.as_bytes()) {
                because.push_str(&format!(" Evidence OSDSYS string at {:#x}.", image.base() + offset));
            }
        }
        self.findings.push(HookFinding { hook, verdict, because, source: source(hook, generation) });
    }

    fn inspect_inner(rom: &RomDir, card: Option<&MemoryCard>, nvm: Option<&Nvm>, disc: Option<&DiscImage>, set: Option<&BiosSet>) -> Self {
        let version = rom.module("ROMVER").and_then(RomVersion::parse);
        let mut map = Self { romver: None, findings: Vec::new(), diagnostics: Vec::new() };
        let v = match version { Ok(v) => v, Err(e) => { map.diagnostics.push(e.to_string()); return map } };
        let generation = match (v.major, v.minor) { (1, 0) => 100, (1, 50) => 150, (1, 60) => 160, (2, 0) => 200,
            _ => { map.diagnostics.push(format!("ROM {} is outside the researched program families; no version-only predictions.", v.text));
                map.romver = Some(v); return map } };
        let nvm = nvm.filter(|n| {
            let matches = n.romver == v;
            if !matches { map.diagnostics.push("NVM was decoded for a different ROMVER; settings ignored.".into()); }
            matches
        });
        let set = set.filter(|s| {
            let matches = s.romver == v;
            if !matches { map.diagnostics.push("BiosSet has a different ROMVER; companions ignored.".into()); }
            matches
        });
        map.romver = Some(v.clone());
        let image = match rom.module("OSDSYS").and_then(|b| ProgramImage::from_module(b, 0x200000)) {
            Ok(i) => i, Err(e) => { map.diagnostics.push(format!("OSDSYS cannot be inspected: {e}")); return map }
        };
        let has = |s: &str| contains(image.data(), s);
        let letter = folder_letter(&v, generation);
        let candidates = update_names(image.data());
        if !candidates.is_empty() {
            let result = match letter {
                Some(l) => card_candidates(card, &format!("B{l}EXEC-SYSTEM"), &candidates,
                    if generation == 100 { "file-open gate only" } else { "readable PS2 card filesystem; PS2-type/formatted gate represented by the parser; no SkipMc on cold boot" }),
                None => unknown("ROMVER region is outside this build's documented folder-letter mapping."),
            };
            map.add(Hook::SystemUpdate, result, generation, Some((&image, &candidates[0])));
        } else { map.diagnostics.push("No system-update filename confirmed in OSDSYS.".into()); }
        if has("dvdplayer.elf") {
            let result = match letter { Some(l) => card_dvd(card, &format!("B{l}EXEC-DVDPLAYER"), generation, set, nvm),
                None => unknown("Cannot resolve the DVD-player card folder for this ROMVER region.") };
            map.add(Hook::CardDvdPlayer, result, generation, Some((&image, "dvdplayer.elf")));
        }
        if has("rom1:DVDID") {
            map.add(Hook::RomDvdPlayer, rom_dvd(set, disc), generation, Some((&image, "rom1:DVDID")));
        }
        if has("HDDLOAD") {
            let result = if generation == 200 && rom.module("XDEV9").is_ok() && has("XDEV9") {
                no("rom0:XDEV9 is present: the ATAD/XFLASH/HDDLOAD probe is skipped; HddBootEnabled stays zero.")
            } else if rom.module("ATAD").is_ok() && rom.module("HDDLOAD").is_ok() {
                unknown("Live ATAD → HDDLOAD probe (no SkipHdd on cold boot); sector-zero boot image/status, ATA unlock and load success need a live HDD. Ready hand-over is ExecPS2(0x100000, gp 0, {rom0:HDDBOOT, OSD argv[1..]}).")
            } else { unknown("HDD probe text exists, but the documented ATAD/HDDLOAD module pair is incomplete; live reachability unverified.") };
            map.add(Hook::HddBoot, result, generation, Some((&image, "HDDLOAD")));
        }
        if rom.module("HDDOSD").is_ok() {
            map.add(Hook::HddOsd, no("ROMDIR has HDDOSD, but no in-ROM caller was found in the researched builds; an HDD program or utility could start it (inferred)."), generation, None);
        }
        if generation == 200 && has("xfrom:XFROMBOOT") {
            let result = if rom.module("XDEV9").is_ok() { no("XDEV9 closes the probe; xfrom: update filenames and XFROMBOOT are dead on this ROM.") }
                else { unknown("Flash hand-over strings present, but the documented XDEV9 dead-path gate is absent; this variant was not traced.") };
            map.add(Hook::FlashBoot, result, generation, Some((&image, "xfrom:XFROMBOOT")));
        }
        let eeload = rom.module("EELOAD").unwrap_or_default();
        if rom.module("TESTMODE").is_ok() && contains(eeload, "rom0:TESTMODE") {
            map.add(Hook::TestMode, unknown("EELOAD tests live CDVD register 0x1F402005 & 0x10; neither NVM nor a disc image supplies that status. Factory provisioning origin is unresolved."), generation, None);
        }
        if has("BootIllegal") {
            let producer = contains(eeload, "BootIllegal") && rom.module("XLOADFILE").is_ok_and(|b| contains(b, "cdrom0:sce_dev5"));
            let result = if !producer { no("OSDSYS accepts BootIllegal, but this ROM's EELOAD/XLOADFILE does not produce it; ordinary cold boot has no injected argument.") }
                else if disc.is_some_and(|d| d.kind == DiscKind::Ps1) { no("Supplied PS1 image dispatches to PS1DRV, not the plain PS2 disc-ELF loader; no BootIllegal argument is supplied.") }
                else { unknown(&format!("{} XLOADFILE's plain cdrom ELF load uses disc type and sceCdReadKey(0,0,0x4B): error 0 continues; 0x30/0x37 can yield -2 → BootIllegal; other errors yield BootError. An image has no live MechaCon error/key verdict. Card -x failures always yield BootError.",
                    disc.map(|d| format!("Image classification {:?}, estimated register {:#04x};", d.kind, d.disc_type_register())).unwrap_or_else(|| "No disc supplied;".into()))) };
            map.add(Hook::BootIllegal, result, generation, Some((&image, "BootIllegal")));
        }
        if generation == 160 {
            let patches = rom.module("OSDSYS").map(|b| launch_patches(b, &image)).unwrap_or_default();
            if matches!(patches.len(), 3 | 8) {
                map.add(Hook::LaunchRelaxations, yes(&format!("Confirmed {} stub stores into unpacked OSDSYS (first target {:#x}); {} Patch values are read from this ROM.", patches.len(), patches[0].0,
                    if patches.len() == 8 { "Launch-time key/ID and PS1/DVD type rechecks are relaxed; SYSTEM.CNF failure can fall back to DVD/PS1." }
                    else { "Launch-time ReadKey/BOOT2-ID failures are ignored; PS1/DVD type rechecks and the ordinary fallback path remain." })), generation, None);
            }
        }
        if has("rom0:PS2LOGO") && has("rom0:PS1DRV") {
            let relaxed = map.findings.iter().any(|f| f.hook == Hook::LaunchRelaxations);
            let result = match disc {
                None => unknown("No disc supplied: live type 0x10/0x11 selects PS1DRV, 0x12/0x13/0x14 PS2LOGO, 0xFE DVD player (2.00 also 0xFC), 0xFD CD player; unknown media stays in the browser."),
                Some(d) if d.kind == DiscKind::Ps1 => unknown(&format!("Image identifies as PS1 (estimated type {:#04x}); BOOT title {}, VER {} go to PS1DRV, with PSX.EXE/MYST/LASTPHOT fallbacks. Live disc type/launch success is not captured by the image.",
                    d.disc_type_register(), d.title_id().unwrap_or_else(|| "unknown".into()), d.version.as_deref().unwrap_or("???"))),
                Some(d) if d.kind == DiscKind::Ps2 => unknown(&format!("Image identifies as PS2 (estimated type {:#04x}); BOOT2 {} → rom0:PS2LOGO. {} Live drive identification and executable loading remain unknown.",
                    d.disc_type_register(), d.system_cnf.get("BOOT2").map(String::as_str).unwrap_or("missing"),
                    if relaxed { "Confirmed loader patches ignore launch-time ReadKey/BOOT2-ID failures." }
                    else { "OSDSYS requires a live ReadKey-derived disc ID matching the BOOT2 filename; image bytes do not supply that key." })),
                Some(_) => unknown("Image filesystem does not identify a PS1/PS2 launch path; actual drive type may select unknown-media, CD or DVD handling and is not captured by the image."),
            };
            map.add(Hook::DiscLaunch, result, generation, Some((&image, "rom0:PS2LOGO")));
        }
        if generation == 100 && has("mc0:/BIEXEC-SYSTEM/OSBROWS") {
            map.add(Hook::CardBrowser, card_browser(card), generation, Some((&image, "mc0:/BIEXEC-SYSTEM/OSBROWS")));
            map.add(Hook::GenericLaunch, no("Dispatcher launch code -2 can LoadExecPS2 an arbitrary plug-in path; no shipped 1.00 module writes -2. A replacement browser would have to request it."), generation, None);
        }
        if let Ok(driver) = rom.module("PS1DRV").and_then(|b| ProgramImage::from_module(b, 0x200000)) {
            if let Some(path) = card_title_path(driver.data()) {
                let result = title_database(card, disc, &path);
                map.add(Hook::TitleDatabase, result, generation, None);
            }
            if contains(driver.data(), "PSD1.0.0") {
                let result = ps1_parameters(disc);
                map.add(Hook::Ps1Parameters, result, generation, None);
            }
        } else { map.diagnostics.push("PS1DRV cannot be inspected; no driver override predictions.".into()); }
        if generation >= 150 && has("Initialize") {
            map.add(Hook::Initialize, no(if generation == 150 { "Only argv[1] is compared; Initialize sets its flag after the config worker/main readers have run (scheduling inference). No cold-boot argument supplies it." }
                else { "Initialize comparison is nested inside the Boot-prefix branch and cannot match; no NVM reset is triggered." }), generation, Some((&image, "Initialize")));
            map.add(Hook::FirstSetup, nvm_gate(nvm, |n| !n.osd.setup_required(), "OSD initialized bit 0x11.7 is set: first-setup wizard suppressed.",
                "OSD initialized bit 0x11.7 is clear: first-setup wizard follows the opening."), generation, None);
        }
        for (hook, word) in [(Hook::SkipMc, "SkipMc"), (Hook::SkipHdd, "SkipHdd"), (Hook::SkipForbid, "SkipForbid")] {
            if has(word) {
                map.add(hook, no("Ordinary cold boot supplies no Skip word. An external ExecOSD caller may pass it; update/HDD hand-overs propagate incoming argv. Retail return helpers pass BootBrowser only; no menu/pad producer found."), generation, Some((&image, word)));
            }
        }
        let words: Vec<_> = ["BootError", "BOOT_ROM_MODULE", "BootClock", "BootBrowser", "BootOpening", "BootWarning"].into_iter().filter(|s| has(s)).collect();
        if !words.is_empty() {
            map.add(Hook::LaunchArguments, no(&format!("Cold boot has no incoming mode/error word. Accepted: {}. Kernel Exit returns with BootBrowser; EELOAD failure supplies BootError <path>; fatal OSD start supplies BOOT_ROM_MODULE. {}", words.join(", "),
                if generation <= 150 { "Only argv[1] is examined." } else { "All argv[1..] are scanned; last Boot word wins." })), generation, None);
        }
        if generation == 200 && hidden_remote_record(&image) {
            map.add(Hook::HiddenRemoteControl, nvm_gate(nvm, |n| !n.osd.unknown_option,
                "Hidden gate packed byte 0x13 bit 7 is clear (not the ordinary-item bit 5).",
                "Hidden gate bit 0x13.7 is set; live sceCdRcBypassCtl status 0x100 can clear it. Version Information → Console and controller 2 slot 0 subset mask 0x090F are also required."), generation, None);
            // A set gate still requires the live command response and menu/controller context (§3.2).
            if let Some(f) = map.findings.last_mut() { if f.verdict == Verdict::Fires { f.verdict = Verdict::Unknown; } }
            map.add(Hook::RemoteControlItem, nvm_gate(nvm, |n| !n.osd.remote_item,
                "Ordinary Remote Control item bit 0x13.5 is clear.",
                "Ordinary-item bit 0x13.5 is set; live sceCdRcBypassCtl status 0x100 can clear availability. Actual Off/On value is separate bit 0x13.6."), generation, None);
            if let Some(f) = map.findings.last_mut() { if f.verdict == Verdict::Fires { f.verdict = Verdict::Unknown; } }
        }
        if generation == 200 && find(image.data(), b"Clear Progressive Setting").is_some() {
            map.add(Hook::ClearProgressive, nvm_gate(nvm, |n| !n.osd.progressive,
                "DVD progressive flag 0x13.4 is clear: option page is hidden.",
                "DVD progressive flag 0x13.4 is set: clear-setting page is offered. Choosing Yes requests config write/flag clear instead of the next DVD launch; completion needs runtime state."), generation, Some((&image, "Clear Progressive Setting")));
        }
        // §1.2's path is assembled from separate host0: and ../modules literals.
        if generation == 200 && has("host0:") && find(image.data(), b"../modules").is_some() {
            map.add(Hook::HostModules, no("Non-rom/mc argv[0] already fails FatalBootError before helper-module lookup; assembled host0:../modules/ path is unreachable."), generation, Some((&image, "host0:")));
        }
        if generation == 200 && has("rom0:ADDROM2") {
            let result = china_assets(&v, nvm, set, rom);
            map.add(Hook::ChinaAssets, result, generation, Some((&image, "rom0:ADDROM2")));
        }
        // Input paths are catalogued, not claimed to execute without a live menu/pad (§§4–5).
        if generation >= 150 && has("Reset") {
            map.add(Hook::AspectRecovery, unknown("Browser input must hold exactly Left + Square + L1/L2/R1/R2 (0x808F) for 121 frames, then Start alone. No live controller/menu state was supplied."), generation, Some((&image, "Reset")));
        }
        if has("BootClock") {
            map.add(Hook::ClockInputs, unknown(if generation == 100 { "MCLOCK Square toggles clock-only display; no secondary Triangle popup in the inspected 1.00 handler. Live menu/pad state required; this does not skip the opening." }
                else { "Clock menu Square toggles clock-only display; Triangle opens the secondary clock popup. Live menu/pad state required; neither skips the opening." }), generation, None);
        }
        if find(image.data(), b"Diagnosis,Off,On").is_some() || (generation == 100 && rom.module("MCLOCK").is_ok()) {
            map.add(Hook::Diagnosis, unknown("Ordinary Version options request temporary drive Diagnosis; On disables auto-adjust. It needs a menu selection and live CDVD command response; it is not a boot-pad unlock."), generation, None);
        }
        map
    }
}

fn yes(s: &str) -> (Verdict, String) { (Verdict::Fires, s.into()) }
fn no(s: &str) -> (Verdict, String) { (Verdict::DoesNotFire, s.into()) }
fn unknown(s: &str) -> (Verdict, String) { (Verdict::Unknown, s.into()) }
fn find(b: &[u8], s: &[u8]) -> Option<usize> { b.windows(s.len()).position(|w| w == s) }
fn contains(b: &[u8], s: &str) -> bool { find(b, &[s.as_bytes(), &[0]].concat()).is_some() }

// osdsys_hooks.md §2: Japan's folder uses I; H uses A, unlike the licence-screen region.
fn folder_letter(v: &RomVersion, generation: u16) -> Option<char> {
    if generation == 100 { return Some('I') }
    match v.region { 'J' => Some('I'), 'A' | 'H' => Some('A'), 'E' => Some('E'), 'C' if generation == 200 => Some('C'), _ => None }
}

// §2.1: read candidate words from the user's program; no shipped filename/version table.
fn update_names(b: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for offset in 0..b.len().saturating_sub(3) {
        if !b[offset..].starts_with(b"osd") { continue }
        let name = b.cstr(offset, 32);
        let Some(stem) = name.strip_suffix(".elf").and_then(|s| s.strip_prefix("osd")) else { continue };
        if (stem == "sys" || stem == "main" || (stem.len() == 3 && stem.bytes().all(|v| v.is_ascii_digit())))
            && !names.contains(&name) { names.push(name); }
    }
    // Numbered candidate precedes the fallback regardless of string-pool order (§2.1).
    names.sort_by_key(|s| s == "osdmain.elf");
    names
}

fn card_candidates(card: Option<&MemoryCard>, folder: &str, names: &[String], gate: &str) -> (Verdict, String) {
    let paths = names.iter().map(|n| format!("{folder}/{n}")).collect::<Vec<_>>().join(" then ");
    let Some(card) = card else { return unknown(&format!("Probe {paths}: no card supplied; {gate}.")) };
    for name in names {
        match card.read_file(&[folder, name]) {
            Ok(_) => return yes(&format!("{folder}/{name} opens; {gate}. Card probe selects this hand-over; KELF authentication/load success remains unknown.")),
            Err(Error::NotFound(..)) => (), Err(e) => return unknown(&format!("Cannot inspect {folder}/{name}: {e}.")),
        }
    }
    no(&format!("Supplied card has none of {paths}; {gate}."))
}

// osdsys_hooks.md §2.2 and rom1_erom_nvm.md §4: prefer the DVD suffix, not the PS1 letter.
fn selected_dvd<'a>(set: &'a BiosSet, nvm: Option<&Nvm>, generation: u16) -> Result<&'a DvdVersion> {
    let rom1 = set.rom1.as_ref().ok_or_else(|| Error::NotFound(Format::Bios, "ROM1 / DVDID".into()))?;
    if generation == 200 && rom1.dvd_versions.iter().any(|v| v.suffix.is_some() && v.id.is_some())
        && (set.mec.is_none() || (set.mec.as_ref().is_some_and(|m| m.version_number() >= 0x060000)
            && nvm.and_then(|n| n.region_params.as_ref()).is_none())) {
        return Err(Error::Corrupt(Format::Bios, "DVDID suffix selection needs decoded region parameters and MEC evidence; absence of an unsuffixed fallback alone does not close the gate".into()))
    }
    let suffix = if generation == 200 {
        if set.mec.as_ref().is_none_or(|m| m.version_number() >= 0x060000) {
            nvm.and_then(|n| n.region_params.as_ref()).map(|p| p.dvd_region)
        } else { None }
    } else { None };
    let selected = suffix.and_then(|s| rom1.dvd_versions.iter().find(|v| v.suffix == Some(s) && v.id.is_some()))
        .or_else(|| rom1.dvd_versions.iter().find(|v| v.suffix.is_none() && v.id.is_some()));
    selected.ok_or_else(|| Error::NotFound(Format::Bios, "selected ROM1 DVDID (including unsuffixed fallback)".into()))
}

fn dvd_id(text: &str) -> Option<(u32, u32, char)> {
    let s = text.trim_matches(['\0', '\n', '\r', ' ']);
    let (&letter, version) = s.as_bytes().split_last()?;
    if !letter.is_ascii_uppercase() { return None }
    let version = std::str::from_utf8(version).ok()?;
    let (major, minor) = version.split_once('.')?;
    if major.is_empty() || minor.is_empty() || !major.bytes().chain(minor.bytes()).all(|v| v.is_ascii_digit()) { return None }
    Some((major.parse().ok()?, minor.parse().ok()?, letter as char))
}

fn card_dvd(card: Option<&MemoryCard>, folder: &str, generation: u16, set: Option<&BiosSet>, nvm: Option<&Nvm>) -> (Verdict, String) {
    let gate = if generation == 100 { "file-open gate only; no ROM1/version/region comparison; selected when DVD media is dispatched" }
        else { "DVD override selection; eventual launch requires live DVD media (0xFE, 2.00 also 0xFC; confirmed 1.60E patches remove the launch recheck)" };
    let executable = card_candidates(card, folder, &["dvdplayer.elf".into()], gate);
    if executable.0 == Verdict::DoesNotFire || generation == 100 { return executable }
    let Some(set) = set else { return unknown(&format!("{} ROM1 DVDID evidence was not supplied; 1.50+ also requires it, same region, strictly newer version and a nonblacklisted display version.", executable.1)) };
    if set.paths.rom1.is_some() && set.rom1.is_none() { return unknown("ROM1 companion is unreadable/malformed; its DVDID gate cannot be evaluated.") }
    let selected = match selected_dvd(set, nvm, generation) { Ok(v) => v,
        Err(Error::NotFound(_, s)) => return no(&format!("{s}: missing ROM1 DVDID leaves region -1 and disables card players. {}", executable.1)),
        Err(e) => return unknown(&e.to_string()),
    };
    let Some(rom_id) = selected.id.as_deref().and_then(dvd_id) else { return unknown("ROM1 DVDID is malformed; cannot evaluate its version/letter gate.") };
    if executable.0 == Verdict::Unknown { return executable }
    let Some(card) = card else { return unknown("No card supplied.") };
    let read = |name| card.read_file(&[folder, name]).map(|b| String::from_utf8_lossy(&b[..b.len().min(31)]).into_owned());
    let id_text = match read("dvdplayer.id") { Ok(t) => t, Err(Error::NotFound(..)) => return no(&format!("{folder}/dvdplayer.id is absent: card-player ID gate fails.")), Err(e) => return unknown(&e.to_string()) };
    let display = match read("dvdplayer-e.ver") { Ok(t) => t, Err(Error::NotFound(..)) => return no(&format!("{folder}/dvdplayer-e.ver is absent: card-player version gate fails.")), Err(e) => return unknown(&e.to_string()) };
    let Some(id) = dvd_id(&id_text) else { return no("Card dvdplayer.id is not major.minor<letter>; override is not accepted.") };
    if display == "1.00\n" || display == "1.01\n" { return no("Card dvdplayer-e.ver is blacklisted (1.00 or 1.01 plus newline).") }
    if id.2 != rom_id.2 || (id.0, id.1) <= (rom_id.0, rom_id.1) {
        return no(&format!("Card DVDID {}.{}{} versus ROM1 {}.{}{}: requires same letter and strictly newer numeric version.", id.0, id.1, id.2, rom_id.0, rom_id.1, rom_id.2))
    }
    yes(&format!("{folder}/dvdplayer.elf opens; card {}.{}{} is newer than ROM1 {}.{}{}, same letter, display version not blacklisted. Override selection fires; actual launch still needs DVD media and successful authenticated loading.", id.0, id.1, id.2, rom_id.0, rom_id.1, rom_id.2))
}

fn rom_dvd(set: Option<&BiosSet>, disc: Option<&DiscImage>) -> (Verdict, String) {
    if let Some(set) = set {
        if set.rom1.as_ref().is_none_or(|r| !r.dvd_boot_modules) || set.erom.as_ref().is_none_or(|e| e.blank) {
            return no("Inspected BIOS set lacks usable ROM1 UDNL/DVDCNF or populated EROM evidence; ROM DVD launch chain is incomplete.")
        }
    } else { return unknown("ROM1/EROM companions not supplied; DVD launch needs their modules and live DVD-Video type 0xFE (2.00 also 0xFC).") }
    if disc.is_some_and(|d| matches!(d.kind, DiscKind::Ps1 | DiscKind::Ps2)) { no("Supplied game image takes PS1/PS2 dispatch, not the DVD-Video launch path. ROM player completeness alone does not launch it.") }
        else { unknown("ROM1/EROM evidence present; DVD-Video type, EROM payload usability and authenticated load success require live drive state. PCSX2 does not load EROM.") }
}

fn card_browser(card: Option<&MemoryCard>) -> (Verdict, String) {
    let Some(card) = card else { return unknown("No card supplied: probe BIEXEC-SYSTEM/OSBROWS descriptor, version >100, then relative browser module. BootError BOOT_ROM_MODULE/osdsys.elf suppresses this probe on restart.") };
    let descriptor = match card.read_file(&["BIEXEC-SYSTEM", "OSBROWS"]) {
        Ok(b) => b, Err(Error::NotFound(..)) => return no("Supplied card has no BIEXEC-SYSTEM/OSBROWS descriptor."), Err(e) => return unknown(&e.to_string()),
    };
    let text = match std::str::from_utf8(&descriptor) { Ok(s) => s, Err(_) => return unknown("OSBROWS descriptor is not readable ASCII text.") };
    let mut lines = text.lines();
    let version = lines.next().and_then(|s| s.trim().parse::<i32>().ok());
    let Some(version) = version else { return unknown("OSBROWS descriptor has no decodable version line.") };
    if version <= 100 { return no(&format!("OSBROWS descriptor version {version} does not exceed the ROM browser's 100.")) }
    let (Some(module), Some(address)) = (lines.next(), lines.next()) else { return unknown("OSBROWS descriptor is truncated (module/address lines missing).") };
    if module.is_empty() || module.contains(['/', '\\', ':']) || u32::from_str_radix(address.trim(), 16).is_err() {
        return unknown("OSBROWS descriptor's module name/load address is malformed or outside the modeled relative-path form.")
    }
    match card.read_file(&["BIEXEC-SYSTEM", module]) {
        Ok(_) => yes(&format!("OSBROWS descriptor version {version} replaces slot 3; BIEXEC-SYSTEM/{module} opens, requested address {}. Authentication/decompression/thread success unknown.", address.trim())),
        Err(Error::NotFound(..)) => no(&format!("OSBROWS descriptor is newer, but its BIEXEC-SYSTEM/{module} payload is missing.")),
        Err(e) => unknown(&e.to_string()),
    }
}

fn card_title_path(b: &[u8]) -> Option<String> {
    for off in 0..b.len().saturating_sub(4) {
        if b[off..].starts_with(b"mc0:") {
            let path = b.cstr(off, 128);
            if path.ends_with("/TITLE.DB") { return Some(path) }
        }
    }
    None
}

// games_probe.md §3.3: first key-prefix line decides; present comma fields overwrite values.
fn parameter_line<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let line = text.split('\0').next()?.lines().find(|line| line.starts_with(key))?;
    let tail = line.get(key.len()..)?.trim_start_matches([' ', '\t']);
    let values = tail.strip_prefix('=')?;
    values.split(',').take(8).any(|s| !s.trim().is_empty()).then_some(values)
}

fn title_database(card: Option<&MemoryCard>, disc: Option<&DiscImage>, path: &str) -> (Verdict, String) {
    if disc.is_some_and(|d| d.kind != DiscKind::Ps1) { return no(&format!("{path} probe belongs to PS1DRV; supplied image does not select PS1 mode.")) }
    let Some(card) = card else { return unknown(&format!("PS1DRV probes {path} then mc1:; no card supplied.")) };
    let components: Vec<_> = path.strip_prefix("mc0:/").unwrap_or(path).split('/').collect();
    let bytes = match card.read_file(&components) { Ok(b) => b, Err(Error::NotFound(..)) => return no(&format!("Supplied card has no {}.", components.join("/"))), Err(e) => return unknown(&e.to_string()) };
    let Some(title) = disc.and_then(DiscImage::title_id) else { return unknown(&format!("{path} opens; PS1 disc/title ID not supplied, so a matching parameter record is unknown.")) };
    let text = String::from_utf8_lossy(&bytes);
    if parameter_line(&text, &title).is_some() { yes(&format!("{path} opens and has parameters for case-sensitive title {title}; PS1DRV applies the provided fields.")) }
        else { no(&format!("{path} opens but has no applicable parameter line for case-sensitive title {title}.")) }
}

fn ps1_parameters(disc: Option<&DiscImage>) -> (Verdict, String) {
    let Some(d) = disc else { return unknown("PS1DRV re-reads SYSTEM.CNF for PSD1.0.0 = up to eight comma fields; no disc supplied.") };
    if d.kind != DiscKind::Ps1 { return no("Supplied image does not select PS1DRV, so PSD1.0.0 is not read.") }
    if parameter_line(&d.system_cnf_text, "PSD1.0.0").is_some() { yes("PS1 SYSTEM.CNF has a PSD1.0.0 parameter line; provided fields override the ROM title table.") }
        else if d.system_cnf_text.len() >= 1023 { unknown("No PSD1.0.0 line in DiscImage's first 1023 bytes; PS1DRV reads up to 2048, so a later override remains unknown.") }
        else { no("PS1 SYSTEM.CNF has no applicable PSD1.0.0 parameter line in the inspected text. DiscImage retains at most 1023 bytes; PS1DRV can read 2048.") }
}

fn nvm_gate(nvm: Option<&Nvm>, closed: impl FnOnce(&Nvm) -> bool, disabled: &str, enabled: &str) -> (Verdict, String) {
    let Some(n) = nvm else { return unknown("No matching decoded NVM supplied; this configuration gate is unknown.") };
    let (verdict, text) = if closed(n) { no(disabled) } else { yes(enabled) };
    let explanation = if n.osd_rejected { " OSD checksum rejected: decoded gate uses documented zero-filled PCSX2 retry behavior, not stored bits." } else { " Accepted area-1 config checksum." };
    (verdict, text + explanation)
}

fn china_assets(v: &RomVersion, nvm: Option<&Nvm>, set: Option<&BiosSet>, rom: &RomDir) -> (Verdict, String) {
    let mec = set.and_then(|s| s.mec.as_ref());
    if mec.is_none_or(|m| m.version_number() >= 0x060000) && nvm.is_none() {
        return unknown("NVM region parameters unavailable; on a supported MechaCon they can replace ROMVER's OSD region and select China assets.")
    }
    let region = if mec.is_some_and(|m| m.version_number() < 0x060000) { v.region }
        else { nvm.and_then(|n| n.region_params.as_ref()).map(|p| p.osd_region).unwrap_or(v.region) };
    if mec.is_none() && region != v.region { return unknown("NVM OSD region differs from ROMVER; live MechaCon region-parameter support decides whether China assets are selected.") }
    if region != 'C' { return no(&format!("Selected OSD region {region} is not China; ADDROM2/font swap gate is closed.")) }
    if rom.module("ADDROM2").is_err() || set.is_some_and(|s| s.paths.rom2.is_none() || s.rom2_mirrors_rom1 == Some(true)) {
        return no("China region selects ADDROM2, but the module or a distinct ROM2 image is missing (a ROM1 mirror supplies no China fonts).")
    }
    unknown("China region selects ADDROM2 and GB18030/FNTASCI2/TEXOPNGM; actual ROM2 font availability/module loading is unverified.")
}

// hidden_input.md §3: registration writes the controller mask into a runtime tail record.
// Locate its immediate-load/store sequence; the BSS record is not a ROM data constant.
fn hidden_remote_record(image: &ProgramImage) -> bool {
    image.data().chunks_exact(4).enumerate().any(|(i, _)| {
        let off = i * 4;
        let word = image.data().u32(off);
        let rt = (word >> 16) & 31;
        matches!(word >> 26, 9 | 13) && (word >> 21) & 31 == 0 && word as u16 == 0x090F && rt != 0
            && (off + 4..(off + 36).min(image.data().len())).step_by(4).any(|p| {
                let store = image.data().u32(p);
                store >> 26 == 43 && (store >> 16) & 31 == rt
            })
    })
}

// osdsys_hooks.md §6.1: evaluate LUI/ADDIU/ORI + SW in the ELF loader stub, taking all
// patch addresses/values from the input. No patch-word table ships. README.md Round three
// adds the three-store March-2002 variant; confirm its branch/nop shape as well as count.
fn launch_patches(module: &[u8], image: &ProgramImage) -> Vec<(usize, u32)> {
    let mut registers = [None; 32];
    registers[0] = Some(0u32);
    let mut patches = Vec::new();
    for offset in (0x80..module.len().min(0x800)).step_by(4) {
        let word = module.u32(offset);
        let (op, rs, rt, immediate) = (word >> 26, ((word >> 21) & 31) as usize, ((word >> 16) & 31) as usize, word as u16);
        match op {
            15 => registers[rt] = Some((immediate as u32) << 16),
            9 => registers[rt] = registers[rs].map(|v| v.wrapping_add(immediate as i16 as u32)),
            13 => registers[rt] = registers[rs].map(|v| v | immediate as u32),
            43 => {
                if let (Some(base), Some(value)) = (registers[rs], registers[rt]) {
                    let address = base.wrapping_add(immediate as i16 as u32) as usize;
                    if image.at(address).is_some_and(|o| o < 0x10000 && o % 4 == 0) && !patches.iter().any(|&(a, _)| a == address) {
                        patches.push((address, value));
                    }
                }
            }
            0 => { let rd = ((word >> 11) & 31) as usize; if rd != 0 { registers[rd] = None; } }
            1..=7 | 40..=46 => (),
            _ => registers[rt] = None,
        }
        registers[0] = Some(0);
    }
    let mut branches = 0;
    let mut calls = 0;
    let mut nops = 0;
    for &(address, value) in &patches {
        let Some(offset) = image.at(address) else { return Vec::new() };
        let original = image.data().u32(offset);
        if value >> 26 == 4 && value & 0x03FF0000 == 0 && matches!(original >> 26, 4..=7 | 20..=23) { branches += 1; }
        else if value >> 26 == 3 && original >> 26 == 3 { calls += 1; }
        else if value == 0 && original >> 26 == 3 { nops += 1; }
        else { return Vec::new() }
    }
    if (branches, calls, nops) == (5, 2, 1) || (branches, calls, nops) == (2, 0, 1) { patches } else { Vec::new() }
}

// Reference addresses are citations, not locators; content locations are reported separately.
fn source(hook: Hook, generation: u16) -> &'static str {
    match hook {
        Hook::SystemUpdate => match generation { 100 => "1.00 OSDSYS 0x2049E0; osdsys_hooks.md §2.1", 150 => "1.50 OSDSYS 0x207340; osdsys_hooks.md §2.1", 160 => "1.60 OSDSYS 0x207200; osdsys_hooks.md §2.1", _ => "2.00 OSDSYS 0x209378; osdsys_hooks.md §2.1" },
        Hook::CardDvdPlayer | Hook::RomDvdPlayer => match generation { 100 => "1.00 OSDSYS 0x202DB0/0x202CD0; osdsys_hooks.md §2.2", 150 | 160 => "1.50/1.60 OSDSYS 0x2049A0/0x202600; osdsys_hooks.md §2.2; rom1_erom_nvm.md §1.4", _ => "2.00 OSDSYS 0x2061E8/0x202E00; osdsys_hooks.md §2.2; rom1_erom_nvm.md §§1.4,4" },
        Hook::HddBoot | Hook::HddOsd => match generation { 150 => "1.50 OSDSYS 0x20774C/0x202D48; osdsys_hooks.md §2.3", 160 => "1.60 OSDSYS 0x207660/0x202D48; osdsys_hooks.md §2.3", _ => "2.00 OSDSYS 0x209888/0x208BB0; osdsys_hooks.md §2.3" },
        Hook::FlashBoot => "2.00 OSDSYS 0x208BB0; osdsys_hooks.md §§2.3,2.4",
        Hook::TestMode => match generation { 150 => "1.50 EELOAD 0x8253C; kernel_eeload_hooks.md §1.4; hidden_input.md §5.4", 160 => "1.60 A/E EELOAD 0x825A8/0x8253C; kernel_eeload_hooks.md §1.4", _ => "2.00 EELOAD 0x8258C; kernel_eeload_hooks.md §1.4" },
        Hook::BootIllegal => "1.60A/2.00 EELOAD 0x82548; XLOADFILE .text+0x1098; kernel_eeload_hooks.md §§2.2,2.3; osdsys_hooks.md §1.1",
        Hook::DiscLaunch => match generation { 100 => "1.00 OSDSYS 0x204080/0x202A38; osdsys_hooks.md §§3,5", 150 => "1.50 OSDSYS 0x206B98; osdsys_hooks.md §§3,5", 160 => "1.60 OSDSYS 0x20B9F8/0x2022A0; osdsys_hooks.md §§3,5", _ => "2.00 OSDSYS 0x20F678/0x202AB0; osdsys_hooks.md §§3,5" },
        Hook::LaunchRelaxations => "1.60E/March-2002 A OSDSYS stub 0x1001B0 onward; osdsys_hooks.md §6.1; README.md Round three (six-week pair)",
        Hook::CardBrowser => "1.00 OSDSYS 0x204838/0x204AF0; osdsys_hooks.md §0",
        Hook::GenericLaunch => "1.00 OSDSYS 0x203310; osdsys_hooks.md §0",
        Hook::TitleDatabase => "1.00/1.60A PS1DRV 0x209F18 (reference); games_probe.md §§3.1,3.3",
        Hook::Ps1Parameters => "1.00/2.00 PS1DRV 0x209EA8/0x20A330; games_probe.md §3.3",
        Hook::HiddenRemoteControl => "2.00 OSDSYS 0x206CC8/0x2266F0/0x20A334; hidden_input.md §§3.1–3.3",
        Hook::RemoteControlItem => "2.00 OSDSYS 0x223758/0x2044A8; hidden_input.md §3.2",
        Hook::ClearProgressive => "2.00 OSDSYS 0x206800/0x225988/0x202E00; hidden_input.md §5.4; osdsys_hooks.md §4.1",
        Hook::FirstSetup => match generation { 150 => "1.50 OSDSYS 0x202D08; osdsys_hooks.md §4.1; hidden_input.md §5.4", 160 => "1.60 OSDSYS main 0x207478; hidden_input.md §5.4", _ => "2.00 OSDSYS 0x203A08/0x209EB8; hidden_input.md §5.4" },
        Hook::SkipMc | Hook::SkipHdd | Hook::SkipForbid => "1.60/2.00 OSDSYS 0x207478/0x209EB8; hidden_input.md §5.3; osdsys_hooks.md §1.2",
        Hook::Initialize => "1.50/1.60/2.00 OSDSYS main 0x2075B8/0x207478/0x209EB8; osdsys_hooks.md §1.2",
        Hook::LaunchArguments => "OSDSYS main: 1.00 0x204D68, 1.50 0x2075B8, 1.60 0x207478, 2.00 0x209EB8; osdsys_hooks.md §1; kernel_eeload_hooks.md §2.1",
        Hook::HostModules => "2.00 OSDSYS 0x209748; osdsys_hooks.md §§1.2,2.4",
        Hook::ChinaAssets => "2.00 OSDSYS 0x206E58; osdsys_hooks.md §2.4; rom1_erom_nvm.md §5",
        Hook::AspectRecovery => "1.50/1.60/2.00 browser 0x23F8A8/0x240178/0x2482B8; hidden_input.md §4",
        Hook::ClockInputs => "1.00/1.50/1.60/2.00 clock 0x606118/0x2270E8/0x2279B8/0x22DC48; hidden_input.md §5.1",
        Hook::Diagnosis => "1.00 worker 0x20E300; 2.00 worker 0x20F9F8; hidden_input.md §5.2; osdsys_hooks.md §4.3",
    }
}
