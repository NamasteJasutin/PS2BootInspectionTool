//! What the console learns about a game disc on the way to launching it, read from a plain
//! ISO image the way OSDSYS and the kernel would, and the hand-off it would perform.

use crate::bytes::Bytes;
use crate::history::{PlayHistory, Record};
use crate::logo::{DiscLogo, LogoAnimation, LogoMaster};
use crate::{Error, Format, Region, Result, VideoMode};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// One `PT_LOAD` segment of the boot ELF's program header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ElfSegment {
    /// Where the segment is loaded.
    pub vaddr: u32,
    /// Bytes read from the file.
    pub file_size: u32,
    /// Bytes occupied in memory (the excess over `file_size` is zeroed).
    pub mem_size: u32,
}

/// The executable `BOOT2` names, as EELOAD would load it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BootElf {
    /// The `BOOT2` value as written in `SYSTEM.CNF` (`cdrom0:\SLES_530.64;1`).
    pub path: String,
    /// The directory entry's name, with its `;1` version.
    pub file_name: String,
    /// First sector of the file.
    pub lba: usize,
    /// File size in bytes.
    pub size: usize,
    /// `e_entry`, where ExecPS2 jumps.
    pub entry: u32,
    /// The `PT_LOAD` segments, at most 16.
    pub segments: Vec<ElfSegment>,
    /// `e_machine` is MIPS (8); false means the header was not understood.
    pub is_mips: bool,
}

/// What the drive's disc-type register would say about the disc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1Exe {
    /// As written in `SYSTEM.CNF` `BOOT`, or `cdrom:\PSX.EXE;1` when there is none.
    pub path: String,
    /// The directory entry's name, with its `;1` version.
    pub file_name: String,
    /// First sector of the file.
    pub lba: usize,
    /// File size in bytes, header included.
    pub size: usize,
    /// Initial program counter.
    pub initial_pc: u32,
    /// Initial global pointer.
    pub initial_gp: u32,
    /// Where the text is loaded.
    pub text_addr: u32,
    /// Bytes of text.
    pub text_size: u32,
    /// Initial stack pointer.
    pub stack: u32,
    /// The "Sony Computer Entertainment Inc. for … area" marker at 0x4C, if present.
    pub marker: String,
}

/// The licence sector a PlayStation disc carries (sector 4 of the system area) and whether
/// the logo data in sectors 5–15 is present.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1Licence {
    /// The line as the shell copies it: from byte 0 to the first newline or NUL, at most 72
    /// characters, spaces and all (it is compared and drawn verbatim).
    pub line: String,
    /// The same with the padding collapsed, for display.
    pub text: String,
    /// Japan, America or Europe from the licence text; `None` when it names none.
    pub region: Option<Region>,
    /// Sectors 5–15 hold data (the logo model); false means the licence screen shows no logo.
    pub logo_sectors_present: bool,
    /// The 0x3278 bytes of sectors 5–11 the shell compares with its own copy of the logo.
    pub logo_data: Vec<u8>,
}

/// What the PS1 shell inside the console does with a disc's licence sector and logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Ps1Verdict {
    /// `A` consoles never check: the disc's text and logo are shown as they are.
    NotChecked,
    /// The logo matches (or was not compared) and the licence line is the one this console accepts.
    Accepted,
    /// The logo differs from the shell's copy: `SystemErrorBootOrDiskFailure`, a black screen for ever.
    LogoMismatch,
    /// The licence line is not the one this console accepts: the shell re-reads the disc for ever, black screen.
    TextMismatch,
}

impl Ps1Verdict {
    /// The rule the shell applies from the console's region (`notes/ps1_boot.md` §5): Europe
    /// accepts the 70-character "Euro pe" or 67-character "(Europe)" line, America checks
    /// nothing, every other region accepts the 64-character "Inc." line. The logo is compared
    /// first when `shell_logo` is given. `None` when the console region or the licence is unknown.
    #[must_use]
    pub fn judge(console: Option<Region>, licence: Option<&Ps1Licence>, shell_logo: Option<&[u8]>) -> Option<Self> {
        let region = console?;
        if region == Region::America { return Some(Self::NotChecked) }
        let lic = licence?;
        if let Some(copy) = shell_logo {
            if lic.logo_data != copy { return Some(Self::LogoMismatch) }
        }
        let line = lic.line.as_str();
        let ok = match region {
            Region::Europe => (line.len() == 70 && line.ends_with("Euro pe   ")) || (line.len() == 67 && line.ends_with("(Europe)")),
            _ => line.len() == 64 && line.ends_with("Inc."),
        };
        Some(if ok { Self::Accepted } else { Self::TextMismatch })
    }
}

/// Everything OSDSYS and the kernel read from a disc before launching it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DiscImage {
    /// The path [`open`](Self::open) was given.
    pub path: PathBuf,
    /// What the drive would report the disc as.
    pub kind: DiscKind,
    /// The PS-X EXE of a PlayStation disc, if it was found.
    pub ps1_exe: Option<Ps1Exe>,
    /// The licence sector of a PlayStation disc, if it carries one.
    pub ps1_licence: Option<Ps1Licence>,
    /// `VER` from SYSTEM.CNF (PS1 discs: passed to PS1DRV as the second argument).
    pub version: Option<String>,
    /// The cue sheet lists audio tracks (register 0x11/0x13: "with CD-DA").
    pub has_cdda: bool,
    /// The primary volume descriptor's volume identifier, trimmed.
    pub volume_id: String,
    /// Volume space size in 2048-byte sectors.
    pub sector_count: usize,
    /// `SYSTEM.CNF` as `key = value` pairs; keys are upper-cased, values trimmed.
    pub system_cnf: HashMap<String, String>,
    /// `SYSTEM.CNF` verbatim (at most the 1023 bytes OSDSYS reads), empty if absent.
    pub system_cnf_text: String,
    /// The `BOOT2` executable of a PS2 disc, if it was found.
    pub boot_elf: Option<BootElf>,
    /// The master the logo sectors' checksum matched (PS2 discs only).
    pub logo_region: Option<LogoMaster>,
    /// Raw 2352-byte sectors (a CD rip) rather than a plain ISO.
    pub raw_sectors: bool,
}

impl DiscImage {
    /// The volume's size in bytes.
    #[must_use]
    pub fn byte_size(&self) -> usize { self.sector_count * 2048 }
    /// CD if the volume fits a CD (OSDSYS's state 0x6C), otherwise DVD (0x6E).
    #[must_use]
    pub fn is_dvd(&self) -> bool { self.byte_size() > 800 << 20 }
    /// OSDSYS's disc state after identification: 0x6A/0x6B PlayStation CD (with CD-DA),
    /// 0x6C/0x6D PS2 CD (with CD-DA), 0x6E PS2 DVD, 0x69 unknown media.
    #[must_use]
    pub fn disc_state_code(&self) -> u32 {
        match self.kind {
            DiscKind::Ps1 => if self.has_cdda { 0x6B } else { 0x6A },
            DiscKind::Ps2 => if self.is_dvd() { 0x6E } else if self.has_cdda { 0x6D } else { 0x6C },
            DiscKind::Unknown => 0x69,
        }
    }
    /// The drive's disc-type register value behind [`disc_state_code`](Self::disc_state_code).
    #[must_use]
    pub fn disc_type_register(&self) -> u8 {
        match self.kind {
            DiscKind::Ps1 => if self.has_cdda { 0x11 } else { 0x10 },
            DiscKind::Ps2 => if self.is_dvd() { 0x14 } else if self.has_cdda { 0x13 } else { 0x12 },
            DiscKind::Unknown => 0x05,
        }
    }
    /// Title ID as the history file and the browser spell it (`SLES_530.64`). For a PS1 disc
    /// OSDSYS takes the file name of the `BOOT` line; `PSX.EXE` becomes `???`.
    #[must_use]
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

    /// Reads the volume descriptor, the root directory, `SYSTEM.CNF`, the boot executable's
    /// header and (PS2) the logo sectors or (PS1) the licence sector from an `.iso`, `.bin`
    /// or `.cue`. Fails with [`Error::Corrupt`] when there is no ISO 9660 volume.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut reader = crate::sectors::SectorReader::open(path)?;
        let raw_sectors = reader.is_raw();
        let mut sector = |n: usize, count: usize| -> Result<Vec<u8>> { reader.read(n, count) };
        let pvd = sector(16, 1)?;
        if pvd.len() < 2048 || &pvd[1..6] != b"CD001" {
            return Err(Error::Corrupt(Format::DiscImage, "not an ISO 9660 image (no primary volume descriptor)".into()));
        }
        let volume_id = pvd.cstr(40, 32).trim().to_string();
        let sector_count = pvd.u32(80) as usize;
        let (root_lba, root_len) = (pvd.u32(156 + 2) as usize, pvd.u32(156 + 10) as usize);
        // A root directory is a few sectors; a corrupt length must not turn into a 4 GB read.
        let root_sectors = root_len.div_ceil(2048).max(1);
        if root_sectors > 256 {
            return Err(Error::Corrupt(Format::DiscImage, "root directory length is implausible".into()));
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
                    let region = if text.contains("Amer") { Some(Region::America) } else if text.contains("Euro") { Some(Region::Europe) } else if text.contains("Inc.") { Some(Region::Japan) } else { None };
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

/// Whether OSDSYS lets a PS2 disc through its region check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RegionVerdict {
    /// Disc and console regions agree.
    Accepted,
    /// They differ: a real console shows the warning scene (state 0x74).
    Rejected,
    /// One of the two regions is unknown, so nothing can be said.
    NotChecked,
}

impl RegionVerdict {
    /// OSDSYS's rule: the title ID's region must be the console's (Japanese titles play on
    /// `J` and `H` consoles, both [`Region::Japan`]); either side unknown → `NotChecked`.
    #[must_use]
    pub fn judge(disc: Option<Region>, console: Option<Region>) -> Self {
        match (disc, console) {
            (Some(d), Some(c)) if d == c => Self::Accepted,
            (Some(_), Some(_)) => Self::Rejected,
            _ => Self::NotChecked,
        }
    }
}

/// Whether PS2LOGO accepts a disc's logo data on a console of a given region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LogoVerdict {
    /// `A` and `C` consoles never check the checksum.
    NotChecked,
    /// The checksum is the one this console expects: the animation plays.
    Accepted,
    /// The checksum is wrong: the animation is skipped and PS2LOGO exits straight to the game.
    Mismatch,
}

impl LogoVerdict {
    /// PS2LOGO's rule: `E` consoles want the E master, `J`/`H` consoles the J/A master, `A`
    /// and `C` consoles do not look; `None` when the console region is unknown.
    #[must_use]
    pub fn judge(console: Option<Region>, master: Option<LogoMaster>) -> Option<Self> {
        Some(match console? {
            Region::America | Region::China => Self::NotChecked,
            Region::Europe if master == Some(LogoMaster::Europe) => Self::Accepted,
            Region::Japan if master == Some(LogoMaster::JapanAmerica) => Self::Accepted,
            _ => Self::Mismatch,
        })
    }
}

/// What the PS1 shell finds in a PlayStation disc's logo sectors (5–11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Ps1LogoState {
    /// The sectors are blank: the licence screen would show no logo.
    Absent,
    /// Data is there but was not compared (no shell copy given).
    Present,
    /// Byte for byte the shell's own copy.
    Identical,
    /// Differs from the shell's copy.
    Differs,
}

/// The program OSDSYS hands the console to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BootModule {
    /// `rom0:PS2LOGO`, for a PS2 disc.
    Ps2Logo,
    /// `rom0:PS1DRV`, for a PlayStation disc.
    Ps1Drv,
}

impl fmt::Display for BootModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self { Self::Ps2Logo => "rom0:PS2LOGO", Self::Ps1Drv => "rom0:PS1DRV" })
    }
}

/// One step of the hand-off, as [`handoff_steps`] lists them. Each carries what the console
/// would know at that point; [`who`](Self::who) names the component acting and `Display`
/// gives the sentence the app shows for it, both with the function addresses of the notes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum HandoffStep {
    /// No disc: OpeningDecideNext wakes the clock / main-menu module.
    NoDisc,
    /// Neither `SYSTEM.CNF` nor a PlayStation licence sector: unknown media, left to the browser.
    UnknownMedia,
    /// The drive reports a disc that is neither a PlayStation nor a PlayStation 2 disc
    /// (state 0x74): the warning scene follows the opening.
    IllegalDisc,
    /// OSDSYS compares the title ID's region with the console's.
    RegionCheck {
        /// The disc's title ID (`?` when unknown).
        title_id: String,
        /// The region the title ID implies.
        disc: Option<Region>,
        /// The console's region.
        console: Option<Region>,
        /// The outcome.
        verdict: RegionVerdict,
    },
    /// The drive identifies a PS2 disc and OSDSYS reads its disc key for the title ID.
    Ps2DiscIdentified {
        /// OSDSYS's disc state (0x6C, 0x6D or 0x6E).
        state: u32,
        /// DVD rather than CD.
        dvd: bool,
        /// The title ID.
        title_id: String,
    },
    /// The drive identifies a PlayStation disc.
    Ps1DiscIdentified {
        /// The disc-type register (0x10 or 0x11).
        register: u8,
        /// OSDSYS's disc state (0x6A or 0x6B).
        state: u32,
        /// The cue sheet lists audio tracks.
        cdda: bool,
    },
    /// OpeningDecideNext latches the launch request: `ctx[0x14]` = 0 PS2 DVD, 1 PS2 CD, 2 PlayStation disc.
    LaunchRequest {
        /// The disc kind ([`DiscKind::Ps2`] or [`DiscKind::Ps1`]).
        kind: DiscKind,
        /// DVD rather than CD (PS2 only).
        dvd: bool,
    },
    /// OSDSYS reads `SYSTEM.CNF` of a PS2 disc and checks `BOOT2` against the disc ID.
    SystemCnf {
        /// The `BOOT2` value.
        boot2: String,
    },
    /// OSDSYS reads `SYSTEM.CNF` of a PlayStation disc for the `BOOT` line and `VER`.
    Ps1SystemCnf {
        /// The `BOOT` line, `None` when the disc has no `SYSTEM.CNF` (PSX.EXE is booted);
        /// `?` when the file it names is not on the disc.
        boot: Option<String>,
        /// The `VER` value.
        version: Option<String>,
        /// The title ID OSDSYS records (`???` for PSX.EXE).
        title_id: String,
    },
    /// OSDSYS updates the play history and saves it to the memory card.
    HistoryUpdate {
        /// The title ID.
        title_id: String,
        /// The record as it was, `None` for a first launch.
        before: Option<Record>,
        /// The record as it is saved (see [`PlayHistory::next_record`]).
        after: Record,
        /// This launch plants a new tower.
        new_tower: bool,
    },
    /// OSDSYS shuts its subsystems down and calls `LoadExecPS2`.
    LoadExec {
        /// The program.
        module: BootModule,
        /// Its arguments.
        argv: Vec<String>,
    },
    /// The kernel restarts the hardware and EELOAD loads the program.
    KernelLoadExec {
        /// The program.
        module: BootModule,
    },
    /// PS2LOGO initialises: region from `ROMVER`, sound driver and bank, GS mode.
    Ps2LogoInit,
    /// PS2LOGO reads the logo sectors and checksums them.
    LogoCheck {
        /// The master the checksum matched.
        master: Option<LogoMaster>,
        /// The outcome, `None` when the console region is unknown.
        verdict: Option<LogoVerdict>,
    },
    /// PS2LOGO plays the chime and the animation, then holds the last frame.
    Ps2LogoPlays {
        /// The video mode, which sets the animated fields.
        video: VideoMode,
    },
    /// PS2LOGO quits and the kernel loads the boot ELF.
    LoadElf {
        /// The ELF.
        elf: BootElf,
    },
    /// EELOAD has the ELF in memory and ExecPS2 jumps to its entry.
    ExecElf {
        /// The ELF.
        elf: BootElf,
    },
    /// One `PT_LOAD` segment of the boot ELF.
    ElfSegment {
        /// Index in the program header.
        index: usize,
        /// The segment.
        segment: ElfSegment,
    },
    /// The PS1 shell checks the licence line and the logo against the console's rules.
    Ps1RegionCheck {
        /// The region the licence sector names.
        licence: Option<Region>,
        /// The console's region.
        console: Option<Region>,
        /// The outcome (see [`Ps1Verdict::judge`]).
        verdict: Option<Ps1Verdict>,
    },
    /// PS1DRV brings up the PS1 environment and TBIN loads the PS1 shell.
    Ps1DrvBoot,
    /// The PS1 shell reads the system area (sectors 4–15).
    Ps1SystemArea {
        /// The licence sector, `None` when it carries no licence text.
        licence: Option<Ps1Licence>,
        /// What the logo sectors hold.
        logo: Ps1LogoState,
    },
    /// The PS1 shell shows the licence screen.
    Ps1LicenceScreen {
        /// The video mode.
        video: VideoMode,
        /// Fields after the text phase until the note table ends.
        tail_fields: usize,
        /// Fields from the first logo frame until the note table ends.
        total_fields: usize,
    },
    /// The PS1 shell starts the PS-X EXE.
    LoadPsxExe {
        /// The executable, `None` when the `BOOT` file was not found or is not a PS-X EXE.
        exe: Option<Ps1Exe>,
    },
    /// The game takes over; the app ends its sequence here.
    Stop {
        /// A PlayStation game rather than a PS2 one.
        ps1: bool,
    },
}

impl HandoffStep {
    /// The component performing the step, as the hand-off card labels it (`OSDSYS Launch
    /// (0x203970 → 0x202AB0)`, `PS2LOGO`, `— stop —`, ...).
    #[must_use]
    pub fn who(&self) -> String {
        match self {
            Self::NoDisc | Self::IllegalDisc | Self::LoadExec { .. } => "OSDSYS".into(),
            Self::UnknownMedia | Self::Ps2DiscIdentified { .. } | Self::Ps1DiscIdentified { .. } => "CDVD (disc thread 0x20F478)".into(),
            Self::RegionCheck { .. } | Self::Ps1RegionCheck { .. } => "Region".into(),
            Self::LaunchRequest { .. } => "OSDSYS OpeningDecideNext (0x2165A0)".into(),
            Self::SystemCnf { .. } => "OSDSYS Launch (0x203970 → 0x202AB0)".into(),
            Self::Ps1SystemCnf { .. } => "OSDSYS LaunchPs1Disc (0x202D50 → Ps1GetBootId 0x203390)".into(),
            Self::HistoryUpdate { .. } => "OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)".into(),
            Self::KernelLoadExec { .. } => "KERNEL KLoadExec".into(),
            Self::Ps2LogoInit | Self::Ps2LogoPlays { .. } => "PS2LOGO".into(),
            Self::LogoCheck { .. } => "PS2LOGO LoadImage (0x101540)".into(),
            Self::LoadElf { .. } => "PS2LOGO → KERNEL".into(),
            Self::ExecElf { .. } => "EELOAD / kernel".into(),
            Self::ElfSegment { index, .. } => format!("  segment {index}"),
            Self::Ps1DrvBoot => "PS1DRV / TBIN".into(),
            Self::Ps1SystemArea { .. } => "PS1 shell: system area".into(),
            Self::Ps1LicenceScreen { .. } => "PS1 shell: licence screen".into(),
            Self::LoadPsxExe { .. } => "PS1 shell → PS-X EXE".into(),
            Self::Stop { .. } => "— stop —".into(),
        }
    }
}

/// The region names OSDSYS's check is described with: a disc is "USA" or "Japan/Asia", a
/// console "USA" or "Japan".
fn ps2_region_name(region: Region, disc: bool) -> &'static str {
    match region {
        Region::Japan => if disc { "Japan/Asia" } else { "Japan" },
        Region::America => "USA",
        Region::Europe => "Europe",
        Region::China => "China",
        Region::Asia => "Asia",
    }
}

impl fmt::Display for HandoffStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDisc => f.write_str("No disc: OpeningDecideNext → ctx[0x5E8] = 2, the clock/main-menu module is woken (not re-created here)."),
            Self::IllegalDisc => f.write_str("the drive reports an illegal disc (state 0x74): OpeningDecideNext → ctx[0x5E8] = 4, the opening module plays its warning scene (\"Please insert a PlayStation or PlayStation 2 format disc\") until the drive reports a change"),
            Self::UnknownMedia => f.write_str("the disc has no SYSTEM.CNF and no PlayStation licence sector: the drive reports it as unknown media (register 0x05, state 0x69); OpeningDecideNext leaves it to the browser, which shows it as a data disc"),
            Self::RegionCheck { title_id, disc, console, verdict } => match (disc, console, verdict) {
                (Some(d), Some(c), RegionVerdict::Accepted) => write!(f, "disc {title_id} is {}, console ROM is {}: a real console accepts it", ps2_region_name(*d, true), ps2_region_name(*c, false)),
                (Some(d), Some(c), RegionVerdict::Rejected) => write!(f, "disc {title_id} is {}, console ROM is {}: a real console would reject it (state 0x74, warning scene)", ps2_region_name(*d, true), ps2_region_name(*c, false)),
                (d, c, _) => write!(f, "disc region {}, console region {} (not checked)", d.map(|r| ps2_region_name(r, true)).unwrap_or("unknown"), c.map(|r| ps2_region_name(r, false)).unwrap_or("unknown")),
            },
            Self::Ps2DiscIdentified { state, dvd, title_id } => write!(f, "disc type register → state 0x{state:02X} (PlayStation 2 {}); Ps2DiscVerifyAndGetId reads the disc key twice → title ID {title_id}", if *dvd { "DVD" } else { "CD" }),
            Self::Ps1DiscIdentified { register, state, cdda } => write!(f, "disc type register 0x{register:02X} → state 0x{state:02X} (PlayStation CD{})", if *cdda { " with CD-DA" } else { "" }),
            Self::LaunchRequest { kind: DiscKind::Ps1, .. } => f.write_str("latched state → ctx[0x14] = 2 (launch request: PlayStation disc)"),
            Self::LaunchRequest { dvd, .. } => write!(f, "latched state → ctx[0x14] = {} (launch request: PS2 {})", if *dvd { 0 } else { 1 }, if *dvd { "DVD" } else { "CD" }),
            Self::SystemCnf { boot2 } => write!(f, "DiscThreadEnable(0); read cdrom0:\\SYSTEM.CNF;1 → BOOT2 = {boot2}; file name must match the first 10 characters of the disc ID"),
            Self::Ps1SystemCnf { boot: Some(boot), version, title_id } => write!(f, "SYSTEM.CNF BOOT = {boot}; VER = {}; title ID = the file name between the last \\ or : and ; → {title_id}{}", version.as_deref().unwrap_or("(none)"), if title_id == "???" { " (PSX.EXE is recorded as \"???\")" } else { "" }),
            Self::Ps1SystemCnf { boot: None, .. } => f.write_str("no SYSTEM.CNF on the disc: the shell boots cdrom:\\PSX.EXE;1 and OSDSYS records the title as \"???\""),
            Self::HistoryUpdate { title_id, before: None, .. } => write!(f, "new record for {title_id}: count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history"),
            Self::HistoryUpdate { title_id, before: Some(r), after, new_tower } => write!(f, "{title_id}: count {} → {}{}; written to mc0:/B?DATA-SYSTEM/history", r.count, after.count, if *new_tower { ", a new random tower bit is added" } else { "" }),
            Self::LoadExec { module, argv } => write!(f, "shutdown of subsystems (0x2021E8), then LoadExecPS2(\"{module}\", argc {}, argv {{{}}})", argv.len(), argv.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ")),
            Self::KernelLoadExec { module: BootModule::Ps2Logo } => f.write_str("HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)"),
            Self::KernelLoadExec { module } => write!(f, "HardwareRestart, EELOAD re-copied to 0x82000, loads {module}"),
            Self::Ps2LogoInit => f.write_str("rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode"),
            Self::LogoCheck { master, verdict } => {
                let master = master.map(|m| format!("the {m} master")).unwrap_or_else(|| "neither known master".into());
                let verdict = match verdict {
                    Some(LogoVerdict::NotChecked) => "this console never checks it",
                    Some(LogoVerdict::Accepted) => "accepted",
                    Some(LogoVerdict::Mismatch) => "mismatch: the animation is skipped and PS2LOGO exits straight to the game",
                    None => "console region unknown",
                };
                write!(f, "sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; logo data matches {master}; checksum {verdict}")
            }
            Self::Ps2LogoPlays { video } => {
                let fields = LogoAnimation::animated_fields(*video);
                write!(f, "chime (cmd 0x5200 ×5), animation fields {}–{}, then the last frame is held for {} fields", fields.start(), fields.end(), LogoAnimation::HOLD_FIELDS)
            }
            Self::LoadElf { elf } => write!(f, "SPU quit; LoadExecPS2(\"{}\", …) → KLoadExec → EELOAD LoadElfAll: {} at LBA {}, {} bytes", elf.path, elf.file_name, elf.lba, elf.size),
            Self::ExecElf { elf } => {
                let total: u64 = elf.segments.iter().map(|s| s.mem_size as u64).sum();
                write!(f, "ELF {}: {} PT_LOAD segment(s), {total} bytes in memory, entry 0x{:08X} — ExecPS2 jumps there with argv[0] = the boot path", if elf.is_mips { "(MIPS R5900)" } else { "(unexpected machine)" }, elf.segments.len(), elf.entry)
            }
            Self::ElfSegment { segment, .. } => write!(f, "vaddr 0x{:08X}, {} bytes from file, {} bytes in memory{}", segment.vaddr, segment.file_size, segment.mem_size, if segment.mem_size > segment.file_size { " (bss zeroed)" } else { "" }),
            Self::Ps1RegionCheck { licence, console, verdict } => match (licence, console, verdict) {
                (_, Some(c), Some(Ps1Verdict::NotChecked)) => write!(f, "console ROM is {c}: an A console's PS1 shell checks neither the licence line nor the logo — the disc is shown as it is"),
                (Some(l), Some(c), Some(Ps1Verdict::Accepted)) => write!(f, "licence sector says {l}, console ROM is {c}: the PS1 shell accepts the line and the logo"),
                (Some(l), Some(c), Some(Ps1Verdict::TextMismatch)) => write!(f, "licence sector says {l}, console ROM is {c}: the PS1 shell would re-read the disc for ever on a black screen (the tool does not enforce region locks)"),
                (_, Some(c), Some(Ps1Verdict::LogoMismatch)) => write!(f, "console ROM is {c}: the logo in sectors 5–11 differs from the shell's copy — SystemErrorBootOrDiskFailure, black screen (the tool draws the disc's logo anyway)"),
                (l, c, _) => write!(f, "licence region {}, console region {} (not checked)", l.map(|r| r.to_string()).unwrap_or_else(|| "unknown".into()), c.map(|r| r.to_string()).unwrap_or_else(|| "unknown".into())),
            },
            Self::Ps1DrvBoot => f.write_str("the EE side of PlayStation compatibility: the IOP is rebooted in PS1 mode and TBIN loads rom0:LOGO — the PS1 BIOS shell (stub + LZ stream → 0x30000, i.e. the PS1's 0x80030000)"),
            Self::Ps1SystemArea { licence: Some(l), logo } => write!(f, "sectors 4–15 read; licence line {} characters: \"{}\"; logo TMD in sectors 5–11 {}", l.line.len(), l.text, match logo {
                Ps1LogoState::Absent => "absent (the licence screen would show no logo)",
                Ps1LogoState::Identical => "identical to the shell's own copy",
                Ps1LogoState::Differs => "differs from the shell's copy",
                Ps1LogoState::Present => "present",
            }),
            Self::Ps1SystemArea { licence: None, .. } => f.write_str("sectors 4–15 carry no licence text: an original PS1 shell reports \"Not PS Disk\""),
            Self::Ps1LicenceScreen { video, tail_fields, total_fields } => write!(f, "no Sony Computer Entertainment screen in this shell; the logo fades in over 31 fields through the GTE depth cue under two drone notes, then the wordmark ramps up over 30 fields with the licence line, the TM mark and the drive's SCE letters while the ascending chime plays, {tail_fields} more fields until the note table ends: {total_fields} fields ≈ {:.2} s, then the screen stays while the game loads", *total_fields as f32 / video.fps()),
            Self::LoadPsxExe { exe: Some(b) } => write!(f, "{} at LBA {}, {} bytes: text {} bytes to 0x{:08X}, initial PC 0x{:08X}, GP 0x{:08X}, SP 0x{:08X}{}", b.file_name, b.lba, b.size, b.text_size, b.text_addr, b.initial_pc, b.initial_gp, b.stack, if b.marker.is_empty() { String::new() } else { format!("; header marker \"{}\"", b.marker) }),
            Self::LoadPsxExe { exe: None } => f.write_str("the BOOT file was not found on the disc (or is not a PS-X EXE); the shell would fail to start it"),
            Self::Stop { ps1: false } => f.write_str("This is where the game takes over the console. The app ends the sequence here."),
            Self::Stop { ps1: true } => f.write_str("This is where the PlayStation game takes over. The app ends the sequence here."),
        }
    }
}

/// The hand-off the console would perform after the logo, as a list of steps the app can
/// show instead of doing. `console` is the console's region (from `ROMVER`); `ps1_shell_logo`
/// is the BIOS's own copy of the PS1 logo (`ps1::Ps1Shell::logo_bytes`), used to say whether
/// the PS1 shell would accept a PlayStation disc; `None` skips that comparison.
#[must_use]
pub fn handoff_steps(disc: Option<&DiscImage>, history: &PlayHistory, video: VideoMode, console: Option<Region>, ps1_shell_logo: Option<&[u8]>) -> Vec<HandoffStep> {
    let Some(d) = disc else { return vec![HandoffStep::NoDisc] };
    if d.kind == DiscKind::Ps1 { return ps1_handoff_steps(d, history, video, console, ps1_shell_logo) }
    if d.kind == DiscKind::Unknown { return vec![HandoffStep::UnknownMedia] }
    let id = d.title_id().unwrap_or_else(|| "?".into());
    let boot2 = d.system_cnf.get("BOOT2").cloned().unwrap_or_default();
    let disc_region = Region::from_title_id(&id);
    let mut s = vec![
        HandoffStep::RegionCheck { title_id: id.clone(), disc: disc_region, console, verdict: RegionVerdict::judge(disc_region, console) },
        HandoffStep::Ps2DiscIdentified { state: d.disc_state_code(), dvd: d.is_dvd(), title_id: id.clone() },
        HandoffStep::LaunchRequest { kind: DiscKind::Ps2, dvd: d.is_dvd() },
        HandoffStep::SystemCnf { boot2: boot2.clone() },
        history_update(history, &id),
        HandoffStep::LoadExec { module: BootModule::Ps2Logo, argv: vec![boot2] },
        HandoffStep::KernelLoadExec { module: BootModule::Ps2Logo },
        HandoffStep::Ps2LogoInit,
        HandoffStep::LogoCheck { master: d.logo_region, verdict: LogoVerdict::judge(console, d.logo_region) },
        HandoffStep::Ps2LogoPlays { video },
    ];
    if let Some(b) = &d.boot_elf {
        s.push(HandoffStep::LoadElf { elf: b.clone() });
        s.push(HandoffStep::ExecElf { elf: b.clone() });
        for (i, seg) in b.segments.iter().enumerate() {
            s.push(HandoffStep::ElfSegment { index: i, segment: seg.clone() });
        }
    }
    s.push(HandoffStep::Stop { ps1: false });
    s
}

/// Where the boot leads, read off the hand-off facts: no disc or unknown media → the menu;
/// an illegal disc or a rejected region → the warning scene (unless `enforce` is false, when
/// the region lock is ignored as the tool always did); a PlayStation disc → the licence
/// screen; otherwise the logo and the game.
#[must_use]
pub fn boot_outcome(steps: &[HandoffStep], enforce: bool) -> crate::sim::BootOutcome {
    use crate::sim::BootOutcome;
    let mut outcome = BootOutcome::Game;
    for s in steps {
        match s {
            HandoffStep::NoDisc | HandoffStep::UnknownMedia => return BootOutcome::Menu,
            HandoffStep::IllegalDisc => return BootOutcome::Warning,
            HandoffStep::RegionCheck { verdict: RegionVerdict::Rejected, .. } if enforce => return BootOutcome::Warning,
            HandoffStep::Ps1DiscIdentified { .. } => outcome = BootOutcome::Ps1Game,
            _ => {}
        }
    }
    outcome
}

fn history_update(history: &PlayHistory, title_id: &str) -> HandoffStep {
    let before = history.records.iter().find(|r| r.name == title_id).cloned();
    let (after, new_tower) = history.next_record(title_id);
    HandoffStep::HistoryUpdate { title_id: title_id.into(), before, after, new_tower }
}

/// The hand-off for a PlayStation 1 disc: OSDSYS records it and starts `rom0:PS1DRV`, which
/// brings up the PS1 environment; the PS1 shell (`rom0:LOGO`, see notes/hidden_features.md §6
/// and notes/ps1_boot.md) shows its own screens and runs the PS-X EXE.
fn ps1_handoff_steps(d: &DiscImage, history: &PlayHistory, video: VideoMode, console: Option<Region>, shell_logo: Option<&[u8]>) -> Vec<HandoffStep> {
    let id = d.title_id().unwrap_or_else(|| "???".into());
    let lic = d.ps1_licence.as_ref();
    let verdict = Ps1Verdict::judge(console, lic, shell_logo);
    let logo = match (lic.map(|l| l.logo_sectors_present), shell_logo) {
        (Some(false), _) => Ps1LogoState::Absent,
        (Some(true), Some(c)) if lic.is_some_and(|l| c == l.logo_data.as_slice()) => Ps1LogoState::Identical,
        (Some(true), Some(_)) => Ps1LogoState::Differs,
        _ => Ps1LogoState::Present,
    };
    let (tail_fields, total_fields) = if video == VideoMode::Pal { (13, 74) } else { (22, 83) };
    let s = vec![
        HandoffStep::Ps1RegionCheck { licence: lic.and_then(|l| l.region), console, verdict },
        HandoffStep::Ps1DiscIdentified { register: d.disc_type_register(), state: d.disc_state_code(), cdda: d.has_cdda },
        HandoffStep::LaunchRequest { kind: DiscKind::Ps1, dvd: false },
        HandoffStep::Ps1SystemCnf {
            boot: d.system_cnf.contains_key("BOOT").then(|| d.ps1_exe.as_ref().map(|b| b.path.clone()).unwrap_or_else(|| "?".into())),
            version: d.version.clone(),
            title_id: id.clone(),
        },
        history_update(history, &id),
        HandoffStep::LoadExec { module: BootModule::Ps1Drv, argv: vec![id.clone(), d.version.clone().unwrap_or_default()] },
        HandoffStep::KernelLoadExec { module: BootModule::Ps1Drv },
        HandoffStep::Ps1DrvBoot,
        HandoffStep::Ps1SystemArea { licence: lic.cloned(), logo },
        HandoffStep::Ps1LicenceScreen { video, tail_fields, total_fields },
        HandoffStep::LoadPsxExe { exe: d.ps1_exe.clone() },
        HandoffStep::Stop { ps1: true },
    ];
    s
}
