//! What the console learns about a game disc on the way to launching it, read from a plain
//! ISO image the way OSDSYS and the kernel would, and the hand-off it would perform.

use crate::bytes::Bytes;
use crate::history::PlayHistory;
use crate::logo::DiscLogo;
use crate::sim::VideoMode;
use crate::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ElfSegment {
    pub vaddr: u32,
    pub file_size: u32,
    pub mem_size: u32,
}

#[derive(Debug, Clone)]
pub struct BootElf {
    pub path: String,      // as written in SYSTEM.CNF
    pub file_name: String,
    pub lba: usize,
    pub size: usize,
    pub entry: u32,
    pub segments: Vec<ElfSegment>,
    pub is_mips: bool,
}

/// What the drive's disc-type register would say about the disc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscKind {
    /// `SYSTEM.CNF` has `BOOT2` (register 0x12/0x14, states 0x6C/0x6E).
    Ps2,
    /// `SYSTEM.CNF` has `BOOT` only, or no `SYSTEM.CNF` but a PlayStation licence sector
    /// (register 0x10/0x11, states 0x6A/0x6B).
    Ps1,
    /// Neither: an ISO the console would not launch.
    Unknown,
}

/// A PlayStation 1 boot executable ("PS-X EXE" header, 2048 bytes before the text).
#[derive(Debug, Clone)]
pub struct Ps1Exe {
    pub path: String,      // as written in SYSTEM.CNF `BOOT`, or `cdrom:\PSX.EXE;1`
    pub file_name: String,
    pub lba: usize,
    pub size: usize,
    pub initial_pc: u32,
    pub initial_gp: u32,
    pub text_addr: u32,
    pub text_size: u32,
    pub stack: u32,
    /// The "Sony Computer Entertainment Inc. for … area" marker at 0x4C, if present.
    pub marker: String,
}

/// The licence sector a PlayStation disc carries (sector 4 of the system area) and whether
/// the logo data in sectors 5–15 is present.
#[derive(Debug, Clone)]
pub struct Ps1Licence {
    /// The line as the shell copies it: from byte 0 to the first newline or NUL, at most 72
    /// characters, spaces and all (it is compared and drawn verbatim).
    pub line: String,
    /// The same with the padding collapsed, for display.
    pub text: String,
    /// "Japan", "America" or "Europe" from the licence text.
    pub region: Option<&'static str>,
    pub logo_sectors_present: bool,
    /// The 0x3278 bytes of sectors 5–11 the shell compares with its own copy of the logo.
    pub logo_data: Vec<u8>,
}

/// What the PS1 shell inside the console does with a disc's licence sector and logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps1Verdict {
    /// `A` consoles never check: the disc's text and logo are shown as they are.
    NotChecked,
    Accepted,
    /// The logo differs from the shell's copy: `SystemErrorBootOrDiskFailure`, a black screen for ever.
    LogoMismatch,
    /// The licence line is not the one this console accepts: the shell re-reads the disc for ever, black screen.
    TextMismatch,
}

impl Ps1Verdict {
    /// The rule the shell applies from the ROM's region letter (`notes/ps1_boot.md` §5): `E`
    /// accepts the 70-character "Euro pe" or 67-character "(Europe)" line, `A` checks nothing,
    /// every other letter accepts the 64-character "Inc." line. The logo is compared first.
    pub fn judge(rom_region: Option<char>, licence: Option<&Ps1Licence>, shell_logo: Option<&[u8]>) -> Option<Self> {
        let region = rom_region?;
        if region == 'A' { return Some(Self::NotChecked) }
        let lic = licence?;
        if let Some(copy) = shell_logo {
            if lic.logo_data != copy { return Some(Self::LogoMismatch) }
        }
        let line = lic.line.as_str();
        let ok = match region {
            'E' => (line.len() == 70 && line.ends_with("Euro pe   ")) || (line.len() == 67 && line.ends_with("(Europe)")),
            _ => line.len() == 64 && line.ends_with("Inc."),
        };
        Some(if ok { Self::Accepted } else { Self::TextMismatch })
    }
}

#[derive(Debug, Clone)]
pub struct DiscImage {
    pub path: PathBuf,
    pub kind: DiscKind,
    pub ps1_exe: Option<Ps1Exe>,
    pub ps1_licence: Option<Ps1Licence>,
    /// `VER` from SYSTEM.CNF (PS1 discs: passed to PS1DRV as the second argument).
    pub version: Option<String>,
    /// The cue sheet lists audio tracks (register 0x11/0x13: "with CD-DA").
    pub has_cdda: bool,
    pub volume_id: String,
    pub sector_count: usize,
    pub system_cnf: HashMap<String, String>,
    pub system_cnf_text: String,
    pub boot_elf: Option<BootElf>,
    pub logo_region: Option<&'static str>,
    /// Raw 2352-byte sectors (a CD rip) rather than a plain ISO.
    pub raw_sectors: bool,
}

impl DiscImage {
    pub fn byte_size(&self) -> usize { self.sector_count * 2048 }
    /// CD if the volume fits a CD (OSDSYS's state 0x6C), otherwise DVD (0x6E).
    pub fn is_dvd(&self) -> bool { self.byte_size() > 800 << 20 }
    pub fn disc_state_code(&self) -> u32 {
        match self.kind {
            DiscKind::Ps1 => if self.has_cdda { 0x6B } else { 0x6A },
            DiscKind::Ps2 => if self.is_dvd() { 0x6E } else if self.has_cdda { 0x6D } else { 0x6C },
            DiscKind::Unknown => 0x69,
        }
    }
    /// The drive's disc-type register value behind [`disc_state_code`](Self::disc_state_code).
    pub fn disc_type_register(&self) -> u8 {
        match self.kind {
            DiscKind::Ps1 => if self.has_cdda { 0x11 } else { 0x10 },
            DiscKind::Ps2 => if self.is_dvd() { 0x14 } else if self.has_cdda { 0x13 } else { 0x12 },
            DiscKind::Unknown => 0x05,
        }
    }
    /// Title ID as the history file and the browser spell it (`SLES_530.64`). For a PS1 disc
    /// OSDSYS takes the file name of the `BOOT` line; `PSX.EXE` becomes `???`.
    pub fn title_id(&self) -> Option<String> {
        match self.kind {
            DiscKind::Ps2 => self.boot_elf.as_ref().map(|b| b.file_name.split(';').next().unwrap_or("").to_string()),
            DiscKind::Ps1 => self.ps1_exe.as_ref().map(|b| {
                let name = b.file_name.split(';').next().unwrap_or("").to_string();
                if name.eq_ignore_ascii_case("PSX.EXE") { "???".to_string() } else { name }
            }),
            DiscKind::Unknown => None,
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        let mut reader = crate::sectors::SectorReader::open(path)?;
        let raw_sectors = reader.is_raw();
        let mut sector = |n: usize, count: usize| -> Result<Vec<u8>> { reader.read(n, count) };
        let pvd = sector(16, 1)?;
        if pvd.len() < 2048 || &pvd[1..6] != b"CD001" {
            return Err(Error::Corrupt("not an ISO 9660 image (no primary volume descriptor)".into()));
        }
        let volume_id = pvd.cstr(40, 32).trim().to_string();
        let sector_count = pvd.u32(80) as usize;
        let (root_lba, root_len) = (pvd.u32(156 + 2) as usize, pvd.u32(156 + 10) as usize);
        // A root directory is a few sectors; a corrupt length must not turn into a 4 GB read.
        let root_sectors = root_len.div_ceil(2048).max(1);
        if root_sectors > 256 {
            return Err(Error::Corrupt("root directory length is implausible".into()));
        }
        let dir = sector(root_lba, root_sectors)?;
        let mut entries: Vec<(String, usize, usize)> = Vec::new();
        let mut p = 0;
        while p + 33 <= dir.len().min(root_len) {
            let len = dir.u8(p) as usize;
            if len == 0 { p = (p / 2048 + 1) * 2048; continue }
            let name_len = dir.u8(p + 32) as usize;
            let Some(name) = dir.get(p + 33..(p + 33 + name_len).min(dir.len())) else { break };
            entries.push((String::from_utf8_lossy(name).into_owned(), dir.u32(p + 2) as usize, dir.u32(p + 10) as usize));
            p += len;
        }
        let mut system_cnf = HashMap::new();
        let mut text = String::new();
        if let Some(e) = entries.iter().find(|e| e.0.to_uppercase().starts_with("SYSTEM.CNF")) {
            // OSDSYS reads at most 1023 bytes of it: one sector.
            let raw = sector(e.1, 1)?;
            text = String::from_utf8_lossy(&raw[..e.2.min(1023).min(raw.len())]).into_owned();
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    system_cnf.insert(k.trim().to_uppercase(), v.trim().to_string());
                }
            }
        }
        let has_cdda = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue"))
            && std::fs::read_to_string(path).map(|t| t.lines().any(|l| l.trim().starts_with("TRACK") && l.contains("AUDIO"))).unwrap_or(false);
        let version = system_cnf.get("VER").cloned();
        // A PlayStation disc: `BOOT` (no `BOOT2`), or no SYSTEM.CNF at all but the licence
        // sector; the executable then defaults to PSX.EXE.
        let mut ps1_exe = None;
        let mut ps1_licence = None;
        let ps1_boot = if system_cnf.contains_key("BOOT2") { None } else { system_cnf.get("BOOT").cloned().or_else(|| if entries.is_empty() { None } else { Some("cdrom:\\PSX.EXE;1".to_string()) }) };
        if let Some(boot) = &ps1_boot {
            let file = boot.rsplit(['\\', ':', '/']).next().unwrap_or("").to_string();
            if let Some(e) = entries.iter().find(|e| e.0.eq_ignore_ascii_case(&file)) {
                let hdr = sector(e.1, 1)?;
                if hdr.len() >= 0x80 && &hdr[..8] == b"PS-X EXE" {
                    ps1_exe = Some(Ps1Exe {
                        path: boot.clone(), file_name: e.0.clone(), lba: e.1, size: e.2,
                        initial_pc: hdr.u32(0x10), initial_gp: hdr.u32(0x14), text_addr: hdr.u32(0x18), text_size: hdr.u32(0x1C), stack: hdr.u32(0x30),
                        marker: hdr.cstr(0x4C, 0x7B4 - 0x4C).trim().to_string(),
                    });
                }
            }
            let lic = sector(4, 12).unwrap_or_default();
            if lic.len() >= 2048 {
                let data = &lic[..2048];
                let line: String = data.iter().take(72).take_while(|&&b| b != 0 && b != b'\n').map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }).collect();
                if line.contains("Licensed") {
                    let text = line.split_whitespace().collect::<Vec<_>>().join(" ");
                    let region = if text.contains("Amer") { Some("America") } else if text.contains("Euro") { Some("Europe") } else if text.contains("Inc.") { Some("Japan") } else { None };
                    let logo_sectors_present = lic.len() > 2048 && lic[2048..].iter().any(|&b| b != 0);
                    let logo_data = lic.get(2048..2048 + 0x3278).map(|b| b.to_vec()).unwrap_or_default();
                    ps1_licence = Some(Ps1Licence { line, text, region, logo_sectors_present, logo_data });
                }
            }
        }
        let kind = if system_cnf.contains_key("BOOT2") { DiscKind::Ps2 } else if ps1_exe.is_some() || ps1_licence.is_some() { DiscKind::Ps1 } else { DiscKind::Unknown };
        let mut boot_elf = None;
        if let Some(boot) = system_cnf.get("BOOT2") {
            let file = boot.rsplit(['\\', ':', '/']).next().unwrap_or("").to_string();
            if let Some(e) = entries.iter().find(|e| e.0.eq_ignore_ascii_case(&file)) {
                let hdr = sector(e.1, 2)?;
                let (mut segments, mut entry, mut is_mips) = (Vec::new(), 0, false);
                if hdr.len() >= 52 && hdr[..4] == [0x7F, b'E', b'L', b'F'] {
                    entry = hdr.u32(0x18);
                    is_mips = hdr.u16(0x12) == 8;
                    let (phoff, phentsize, phnum) = (hdr.u32(0x1C) as usize, hdr.u16(0x2A) as usize, hdr.u16(0x2C) as usize);
                    for i in 0..phnum.min(16) {
                        let o = phoff + i * phentsize;
                        if o + 32 > hdr.len() { break }
                        if hdr.u32(o) == 1 {
                            segments.push(ElfSegment { vaddr: hdr.u32(o + 8), file_size: hdr.u32(o + 16), mem_size: hdr.u32(o + 20) });
                        }
                    }
                }
                boot_elf = Some(BootElf { path: boot.clone(), file_name: e.0.clone(), lba: e.1, size: e.2, entry, segments, is_mips });
            }
        }
        let logo_region = if kind == DiscKind::Ps2 { DiscLogo::read(path).ok().and_then(|l| l.region) } else { None };
        Ok(Self { path: path.to_path_buf(), kind, ps1_exe, ps1_licence, version, has_cdda, volume_id, sector_count, system_cnf, system_cnf_text: text, boot_elf, logo_region, raw_sectors })
    }
}

#[derive(Debug, Clone)]
pub struct HandoffStep {
    pub who: String,
    pub what: String,
}

/// The hand-off the console would perform after the logo, as a list of steps the app can
/// show instead of doing. Every line names the function in the notes that does it.
/// `ps1_shell_logo` is the BIOS's own copy of the PS1 logo (`ps1::Ps1Shell::logo_bytes`), used
/// to say whether the PS1 shell would accept a PlayStation disc; `None` skips that comparison.
pub fn handoff_steps(disc: Option<&DiscImage>, history: &PlayHistory, video: VideoMode, rom_region: Option<char>, ps1_shell_logo: Option<&[u8]>) -> Vec<HandoffStep> {
    let step = |who: &str, what: String| HandoffStep { who: who.into(), what };
    let Some(d) = disc else {
        return vec![step("OSDSYS", "No disc: OpeningDecideNext → ctx[0x5E8] = 2, the clock/main-menu module is woken (not re-created here).".into())];
    };
    if d.kind == DiscKind::Ps1 { return ps1_handoff_steps(d, history, video, rom_region, ps1_shell_logo) }
    if d.kind == DiscKind::Unknown {
        return vec![step("CDVD (disc thread 0x20F478)", "the disc has no SYSTEM.CNF and no PlayStation licence sector: the drive reports it as unknown media (register 0x05, state 0x69); OpeningDecideNext leaves it to the browser, which shows it as a data disc".into())];
    }
    let id = d.title_id().unwrap_or_else(|| "?".into());
    let kind = if d.is_dvd() { "DVD" } else { "CD" };
    let boot2 = d.system_cnf.get("BOOT2").cloned().unwrap_or_default();
    let console = rom_region.map(|r| match r { 'J' | 'H' => "Japan", 'A' => "USA", 'E' => "Europe", 'C' => "China", _ => "unknown" });
    let disc_region = match id.get(..4) { Some("SLES") | Some("SCES") => Some("Europe"), Some("SLUS") | Some("SCUS") => Some("USA"), Some("SLPS") | Some("SCPS") | Some("SLPM") | Some("SCPM") | Some("SLKA") => Some("Japan/Asia"), Some("SCAJ") => Some("Asia"), _ => None };
    let mut s = vec![
        step("Region", match (disc_region, console) {
            (Some(d), Some(c)) if d == c || (d == "Japan/Asia" && c == "Japan") => format!("disc {id} is {d}, console ROM is {c}: a real console accepts it"),
            (Some(d), Some(c)) => format!("disc {id} is {d}, console ROM is {c}: a real console would reject it (state 0x74, warning scene). The tool does not enforce region locks."),
            (d, c) => format!("disc region {}, console region {} (not checked)", d.unwrap_or("unknown"), c.unwrap_or("unknown")),
        }),
        step("CDVD (disc thread 0x20F478)", format!("disc type register → state 0x{:02X} (PlayStation 2 {kind}); Ps2DiscVerifyAndGetId reads the disc key twice → title ID {id}", d.disc_state_code())),
        step("OSDSYS OpeningDecideNext (0x2165A0)", format!("latched state → ctx[0x14] = {} (launch request: PS2 {kind})", if d.is_dvd() { 0 } else { 1 })),
        step("OSDSYS Launch (0x203970 → 0x202AB0)", format!("DiscThreadEnable(0); read cdrom0:\\SYSTEM.CNF;1 → BOOT2 = {boot2}; file name must match the first 10 characters of the disc ID")),
    ];
    let rec = history.records.iter().find(|r| r.name == id);
    let what = match rec {
        None => format!("new record for {id}: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history"),
        Some(r) => {
            let c = r.count as u32 + 1;
            format!("{id}: count {} → {c}{}; written to mc0:/B?DATA-SYSTEM/history", r.count, if c >= 14 && (c - 14).is_multiple_of(10) { ", a new random tower bit is added" } else { "" })
        }
    };
    s.push(step("OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)", what));
    s.push(step("OSDSYS", format!("shutdown of subsystems (0x2021E8), then LoadExecPS2(\"rom0:PS2LOGO\", argc 1, argv {{\"{boot2}\"}})")));
    s.push(step("KERNEL KLoadExec", "HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)".into()));
    s.push(step("PS2LOGO", "rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode".into()));
    // The logo bitmap is mastered per region: E discs carry the PAL logo, J and A discs share
    // the same one, so a US disc matches the "J" checksum. Only J/H and E consoles check it.
    let master = d.logo_region.map(|r| if r == "J" { "the J/A master".to_string() } else { format!("the {r} master") }).unwrap_or_else(|| "neither known master".into());
    let verdict = match rom_region {
        Some('A') | Some('C') => "this console never checks it".to_string(),
        Some('E') if d.logo_region == Some("E") => "accepted".into(),
        Some('J') | Some('H') if d.logo_region == Some("J") => "accepted".into(),
        Some(_) => "mismatch: the animation is skipped and PS2LOGO exits straight to the game".into(),
        None => "console region unknown".into(),
    };
    s.push(step("PS2LOGO LoadImage (0x101540)", format!("sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches {master}; checksum {verdict}")));
    s.push(step("PS2LOGO", format!("chime (cmd 0x5200 ×5), animation fields {}, then the last frame is held for 120 fields", if video == VideoMode::Pal { "14–35" } else { "17–42" })));
    if let Some(b) = &d.boot_elf {
        let total: u64 = b.segments.iter().map(|s| s.mem_size as u64).sum();
        s.push(step("PS2LOGO → KERNEL", format!("SPU quit; LoadExecPS2(\"{}\", …) → KLoadExec → EELOAD LoadElfAll: {} at LBA {}, {} bytes", b.path, b.file_name, b.lba, b.size)));
        s.push(step("EELOAD / kernel", format!("ELF {}: {} PT_LOAD segment(s), {total} bytes in memory, entry 0x{:08X} — ExecPS2 jumps there with argv[0] = the boot path", if b.is_mips { "(MIPS R5900)" } else { "(unexpected machine)" }, b.segments.len(), b.entry)));
        for (i, seg) in b.segments.iter().enumerate() {
            s.push(step(&format!("  segment {i}"), format!("vaddr 0x{:08X}, {} bytes from file, {} bytes in memory{}", seg.vaddr, seg.file_size, seg.mem_size, if seg.mem_size > seg.file_size { " (bss zeroed)" } else { "" })));
        }
    }
    s.push(step("— stop —", "This is where the game takes over the console. The app ends the sequence here.".into()));
    s
}

/// The hand-off for a PlayStation 1 disc: OSDSYS records it and starts `rom0:PS1DRV`, which
/// brings up the PS1 environment; the PS1 shell (`rom0:LOGO`, see notes/hidden_features.md §6
/// and notes/ps1_boot.md) shows its own screens and runs the PS-X EXE.
fn ps1_handoff_steps(d: &DiscImage, history: &PlayHistory, video: VideoMode, rom_region: Option<char>, shell_logo: Option<&[u8]>) -> Vec<HandoffStep> {
    let step = |who: &str, what: String| HandoffStep { who: who.into(), what };
    let id = d.title_id().unwrap_or_else(|| "???".into());
    let console = rom_region.map(|r| match r { 'J' | 'H' => "Japan", 'A' => "America", 'E' => "Europe", 'C' => "China", _ => "unknown" });
    let lic = d.ps1_licence.as_ref();
    let verdict = Ps1Verdict::judge(rom_region, lic, shell_logo);
    let mut s = vec![
        step("Region", match (lic.and_then(|l| l.region), console, verdict) {
            (_, Some(c), Some(Ps1Verdict::NotChecked)) => format!("console ROM is {c}: an A console's PS1 shell checks neither the licence line nor the logo — the disc is shown as it is"),
            (Some(l), Some(c), Some(Ps1Verdict::Accepted)) => format!("licence sector says {l}, console ROM is {c}: the PS1 shell accepts the line and the logo"),
            (Some(l), Some(c), Some(Ps1Verdict::TextMismatch)) => format!("licence sector says {l}, console ROM is {c}: the PS1 shell would re-read the disc for ever on a black screen (the tool does not enforce region locks)"),
            (_, Some(c), Some(Ps1Verdict::LogoMismatch)) => format!("console ROM is {c}: the logo in sectors 5–11 differs from the shell's copy — SystemErrorBootOrDiskFailure, black screen (the tool draws the disc's logo anyway)"),
            (l, c, _) => format!("licence region {}, console region {} (not checked)", l.unwrap_or("unknown"), c.unwrap_or("unknown")),
        }),
        step("CDVD (disc thread 0x20F478)", format!("disc type register 0x{:02X} → state 0x{:02X} (PlayStation CD{})", d.disc_type_register(), d.disc_state_code(), if d.has_cdda { " with CD-DA" } else { "" })),
        step("OSDSYS OpeningDecideNext (0x2165A0)", "latched state → ctx[0x14] = 2 (launch request: PlayStation disc)".into()),
        step("OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)", if d.system_cnf.contains_key("BOOT") {
            format!("SYSTEM.CNF BOOT = {}; VER = {}; title ID = the file name between the last \\ or : and ; → {id}{}", d.ps1_exe.as_ref().map(|b| b.path.as_str()).unwrap_or("?"), d.version.as_deref().unwrap_or("(none)"), if id == "???" { " (PSX.EXE is recorded as \"???\")" } else { "" })
        } else {
            "no SYSTEM.CNF on the disc: the shell boots cdrom:\\PSX.EXE;1 and OSDSYS records the title as \"???\"".to_string()
        }),
    ];
    let rec = history.records.iter().find(|r| r.name == id);
    s.push(step("OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)", match rec {
        None => format!("new record for {id}: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history"),
        Some(r) => { let c = r.count as u32 + 1; format!("{id}: count {} → {c}{}; written to mc0:/B?DATA-SYSTEM/history", r.count, if c >= 14 && (c - 14).is_multiple_of(10) { ", a new random tower bit is added" } else { "" }) }
    }));
    s.push(step("OSDSYS", format!("shutdown of subsystems (0x2021E8), then LoadExecPS2(\"rom0:PS1DRV\", argc 2, argv {{\"{id}\", \"{}\"}})", d.version.as_deref().unwrap_or(""))));
    s.push(step("KERNEL KLoadExec", "HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS1DRV".into()));
    s.push(step("PS1DRV / TBIN", "the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)".into()));
    match lic {
        Some(l) => s.push(step("PS1 shell: system area", format!("sectors 4–15 read; licence line {} characters: \"{}\"; logo TMD in sectors 5–11 {}", l.line.len(), l.text, match (l.logo_sectors_present, shell_logo) { (false, _) => "absent (the licence screen would show no logo)".to_string(), (true, Some(c)) if c == l.logo_data.as_slice() => "identical to the shell's own copy".into(), (true, Some(_)) => "differs from the shell's copy".into(), (true, None) => "present".into() }))),
        None => s.push(step("PS1 shell: system area", "sectors 4–15 carry no licence text: an original PS1 shell reports \"Not PS Disk\"".into())),
    }
    let (tail, total) = if video == VideoMode::Pal { (13, 74) } else { (22, 83) };
    s.push(step("PS1 shell: licence screen", format!("no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, {tail} more fields until the note table ends: {total} fields ≈ {:.2} s, then the screen stays while the game loads", total as f32 / video.fps())));
    if let Some(b) = &d.ps1_exe {
        s.push(step("PS1 shell → PS-X EXE", format!("{} at LBA {}, {} bytes: text {} bytes to 0x{:08X}, initial PC 0x{:08X}, GP 0x{:08X}, SP 0x{:08X}{}", b.file_name, b.lba, b.size, b.text_size, b.text_addr, b.initial_pc, b.initial_gp, b.stack, if b.marker.is_empty() { String::new() } else { format!("; header marker \"{}\"", b.marker) })));
    } else {
        s.push(step("PS1 shell → PS-X EXE", "the BOOT file was not found on the disc (or is not a PS-X EXE); the shell would fail to start it".into()));
    }
    s.push(step("— stop —", "This is where the PlayStation game takes over. The app ends the sequence here.".into()));
    s
}
