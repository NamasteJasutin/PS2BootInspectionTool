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

#[derive(Debug, Clone)]
pub struct DiscImage {
    pub path: PathBuf,
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
    pub fn disc_state_code(&self) -> u32 { if self.is_dvd() { 0x6E } else { 0x6C } }
    /// Title ID as the history file and the browser spell it (`SLES_530.64`).
    pub fn title_id(&self) -> Option<String> { self.boot_elf.as_ref().map(|b| b.file_name.split(';').next().unwrap_or("").to_string()) }

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
        let dir = sector(root_lba, (root_len + 2047).div_ceil(2048).max(1))?;
        let mut entries: Vec<(String, usize, usize)> = Vec::new();
        let mut p = 0;
        while p < dir.len().min(root_len) {
            let len = dir.u8(p) as usize;
            if len == 0 { p = (p / 2048 + 1) * 2048; continue }
            let name_len = dir.u8(p + 32) as usize;
            let name = String::from_utf8_lossy(&dir[p + 33..(p + 33 + name_len).min(dir.len())]).into_owned();
            entries.push((name, dir.u32(p + 2) as usize, dir.u32(p + 10) as usize));
            p += len;
        }
        let mut system_cnf = HashMap::new();
        let mut text = String::new();
        if let Some(e) = entries.iter().find(|e| e.0.to_uppercase().starts_with("SYSTEM.CNF")) {
            let raw = sector(e.1, e.2.div_ceil(2048).max(1))?;
            text = String::from_utf8_lossy(&raw[..e.2.min(1023).min(raw.len())]).into_owned();
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    system_cnf.insert(k.trim().to_uppercase(), v.trim().to_string());
                }
            }
        }
        let mut boot_elf = None;
        if let Some(boot) = system_cnf.get("BOOT2") {
            let file = boot.rsplit(|c| c == '\\' || c == ':' || c == '/').next().unwrap_or("").to_string();
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
        let logo_region = DiscLogo::read(path).ok().and_then(|l| l.region);
        Ok(Self { path: path.to_path_buf(), volume_id, sector_count, system_cnf, system_cnf_text: text, boot_elf, logo_region, raw_sectors })
    }
}

#[derive(Debug, Clone)]
pub struct HandoffStep {
    pub who: String,
    pub what: String,
}

/// The hand-off the console would perform after the logo, as a list of steps the app can
/// show instead of doing. Every line names the function in the notes that does it.
pub fn handoff_steps(disc: Option<&DiscImage>, history: &PlayHistory, video: VideoMode, rom_region: Option<char>) -> Vec<HandoffStep> {
    let step = |who: &str, what: String| HandoffStep { who: who.into(), what };
    let Some(d) = disc else {
        return vec![step("OSDSYS", "No disc: OpeningDecideNext → ctx[0x5E8] = 2, the clock/main-menu module is woken (not re-created here).".into())];
    };
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
            format!("{id}: count {} → {c}{}; written to mc0:/B?DATA-SYSTEM/history", r.count, if c >= 14 && (c - 14) % 10 == 0 { ", a new random tower bit is added" } else { "" })
        }
    };
    s.push(step("OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)", what));
    s.push(step("OSDSYS", format!("shutdown of subsystems (0x2021E8), then LoadExecPS2(\"rom0:PS2LOGO\", argc 1, argv {{\"{boot2}\"}})")));
    s.push(step("KERNEL KLoadExec", "HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)".into()));
    s.push(step("PS2LOGO", "rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode".into()));
    s.push(step("PS2LOGO LoadImage (0x101540)", format!("sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; checksum {}", d.logo_region.map(|r| format!("matches region {r}")).unwrap_or_else(|| "does not match E/J (A/C consoles skip it)".into()))));
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
