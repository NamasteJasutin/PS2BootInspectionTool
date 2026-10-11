//! Disc and game executable probe: inspects ISO 9660 directory structures,
//! PS2 boot ELFs, IOP modules, IOPRP images, PS1 EXEs, and SYSTEM.CNF files
//! to reveal how a game talks to the console.
//!
//! Documented in `notes/research/games_probe.md`:
//! - Multi-level ISO 9660 directory traversal via [`DiscFiles`].
//! - PS2 boot ELF inspection ([`ElfProbe`]): program segments, SDK version stamps,
//!   IOP/IRX device references, IOPRP module lists and versions, EE syscalls actually called,
//!   libcdvd commands, hard-coded video modes, and libmc/libpad version gates (§1.1–§1.8).
//! - PS1 executable inspection ([`Ps1ExeProbe`]): header validation, PsyQ library stamps,
//!   and BIOS A0/B0/C0 vector calls used (§2.1–§2.2).
//! - `SYSTEM.CNF` key audit ([`audit_system_cnf`]): identifies who reads each key
//!   (OSDSYS, PS1 kernel/TBIN, PS1DRV, or nobody) and why (§1.1, §2.1, §3.3).

use crate::bytes::Bytes;
use crate::rom::RomDir;
use crate::sectors::SectorReader;
use crate::{Error, Format, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::Path;

// ============================================================================
// 1. ISO 9660 Filesystem Traversal (DiscFiles)
// ============================================================================

/// An entry in an ISO 9660 filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DiscFileEntry {
    /// Normalized path starting with `/` (e.g. `"/IRX/IOPRP165.IMG;1"`).
    pub path: String,
    /// Logical block address (first sector).
    pub lba: usize,
    /// File size in bytes.
    pub size: usize,
    /// True if this entry is a directory.
    pub is_directory: bool,
}

/// Directory reader for an ISO 9660 filesystem traversing all levels via [`SectorReader`].
#[derive(Debug)]
pub struct DiscFiles {
    reader: SectorReader,
    entries: Vec<DiscFileEntry>,
    volume_id: String,
}

impl DiscFiles {
    /// Opens an `.iso`, `.bin`, `.img`, or `.cue` file and indexes all directories.
    ///
    /// Fails with [`Error::Corrupt`] if the disc has no valid primary volume descriptor.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut reader = SectorReader::open(path)?;
        let pvd = reader.read(16, 1)?;
        if pvd.len() < 2048 || &pvd[1..6] != b"CD001" {
            return Err(Error::Corrupt(
                Format::DiscImage,
                "not an ISO 9660 image (no primary volume descriptor)".into(),
            ));
        }

        let root_lba = pvd.u32(156 + 2) as usize;
        let root_len = pvd.u32(156 + 10) as usize;

        let mut entries = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back((String::new(), root_lba, root_len));

        let mut visited_lbas = HashSet::new();

        while let Some((dir_path, lba, length)) = queue.pop_front() {
            if !visited_lbas.insert(lba) {
                continue;
            }
            let sec_count = length.div_ceil(2048).clamp(1, 256);
            let dir_data = reader.read(lba, sec_count)?;
            let max_len = dir_data.len().min(length);

            let mut p = 0;
            while p + 33 <= max_len {
                let rec_len = dir_data[p] as usize;
                if rec_len == 0 {
                    // Padded to next 2048-byte sector boundary
                    p = (p / 2048 + 1) * 2048;
                    continue;
                }
                if p + rec_len > dir_data.len() {
                    break;
                }

                let ent_lba = dir_data.u32(p + 2) as usize;
                let ent_size = dir_data.u32(p + 10) as usize;
                let flags = dir_data[p + 25];
                let is_dir = (flags & 2) != 0;
                let name_len = dir_data[p + 32] as usize;

                if p + 33 + name_len <= p + rec_len {
                    let name_bytes = &dir_data[p + 33..p + 33 + name_len];
                    if name_bytes != [0] && name_bytes != [1] {
                        let name = String::from_utf8_lossy(name_bytes);
                        let full_path = if dir_path.is_empty() {
                            format!("/{name}")
                        } else {
                            format!("{dir_path}/{name}")
                        };

                        entries.push(DiscFileEntry {
                            path: full_path.clone(),
                            lba: ent_lba,
                            size: ent_size,
                            is_directory: is_dir,
                        });

                        if is_dir {
                            queue.push_back((full_path, ent_lba, ent_size));
                        }
                    }
                }
                p += rec_len;
            }
        }

        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { reader, entries, volume_id: pvd.cstr(40, 32).trim().to_string() })
    }

    pub(crate) fn volume_id(&self) -> &str { &self.volume_id }

    /// List of all files and directories indexed on the disc.
    #[must_use]
    pub fn list(&self) -> &[DiscFileEntry] {
        &self.entries
    }

    /// Reads the complete byte content of a file identified by path.
    ///
    /// Accepts paths with or without leading `/`, `\`, `cdrom:`, `cdrom0:`,
    /// or trailing ISO version `;1` suffixes, case-insensitively.
    pub fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let norm_req = Self::normalize_path(path);
        let entry = self
            .entries
            .iter()
            .find(|e| !e.is_directory && Self::normalize_path(&e.path) == norm_req)
            .ok_or_else(|| Error::NotFound(Format::DiscImage, path.to_string()))?;

        let count = entry.size.div_ceil(2048).max(1);
        let mut data = self.reader.read(entry.lba, count)?;
        data.truncate(entry.size);
        Ok(data)
    }

    /// Normalizes a disc path for loose matching.
    fn normalize_path(p: &str) -> String {
        let mut s = p.trim();
        if let Some(rest) = s.strip_prefix("cdrom0:").or_else(|| s.strip_prefix("cdrom:")) {
            s = rest;
        }
        let s = s.trim_start_matches(['/', '\\']);
        let s = s.replace('\\', "/");
        let s = s.split(';').next().unwrap_or(&s);
        s.to_ascii_uppercase()
    }
}

// ============================================================================
// 2. PS2 Boot ELF Probe (ElfProbe)
// ============================================================================

/// One `PT_LOAD` segment of the ELF program header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProbeSegment {
    /// Segment type (1 = PT_LOAD).
    pub seg_type: u32,
    /// Offset within the file.
    pub file_offset: usize,
    /// Virtual load address in memory.
    pub vaddr: u32,
    /// Physical address.
    pub paddr: u32,
    /// Size in the file.
    pub file_size: usize,
    /// Size in memory.
    pub mem_size: usize,
    /// Segment flags (PF_R = 4, PF_W = 2, PF_X = 1).
    pub flags: u32,
    /// Alignment.
    pub align: u32,
}

/// An SDK or library build stamp found in the executable.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SdkStamp {
    /// Byte offset in the executable file.
    pub offset: usize,
    /// The stamp string (e.g. `"PsIIlibcdvd 1650"`, `"libdma  1620"`).
    pub stamp: String,
}

/// An IOP module or IOPRP image referenced in the ELF text/data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReferencedModule {
    /// Byte offset where the string begins.
    pub offset: usize,
    /// Path or name referenced (e.g. `"cdrom:\\IRX\\IOPRP165.IMG;1"`).
    pub path: String,
}

/// An EE kernel syscall site in the executable.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyscallSite {
    /// Syscall number as loaded into `$v1` (negative for interrupt-context variants).
    pub number: i32,
    /// Canonical name if known from the EE kernel table (`games_probe.md §1.5`).
    pub name: Option<&'static str>,
    /// Virtual address of the syscall stub.
    pub stub_vaddr: u32,
    /// File offset of the syscall stub.
    pub stub_offset: usize,
    /// Number of call sites (`jal` or `j`) jumping to this stub.
    pub call_count: usize,
    /// File offsets of the instructions that call this stub.
    pub caller_offsets: Vec<usize>,
}

/// Minimum IOP module version gate checked by libmc or libpad (`games_probe.md §1.7`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VersionGate {
    /// Module name (e.g. `"mcserv"`, `"mcman"`, `"padman"`).
    pub module: String,
    /// Minimum required version (e.g. 0x205 for 2.05).
    pub min_version: u16,
    /// Byte offset in the file where the check occurs.
    pub offset: usize,
    /// Virtual address where the instruction sits.
    pub vaddr: u32,
}

/// Libcdvd command usage observed in the executable (`games_probe.md §1.4`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CdvdUsage {
    /// Function or command name (e.g. `"sceCdMmode"`).
    pub name: String,
    /// File offset of the call site or function.
    pub offset: usize,
    /// Detailed observation.
    pub details: String,
}

/// Detected hard-coded video mode (`games_probe.md §1.6`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DetectedVideoMode {
    /// NTSC (mode 2).
    Ntsc,
    /// PAL (mode 3).
    Pal,
}

impl fmt::Display for DetectedVideoMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ntsc => f.write_str("NTSC"),
            Self::Pal => f.write_str("PAL"),
        }
    }
}

/// Complete inspection report for a PS2 boot ELF.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ElfProbe {
    /// Program entry point (`e_entry`).
    pub entry: u32,
    /// `PT_LOAD` program segments.
    pub segments: Vec<ProbeSegment>,
    /// SDK library build stamps.
    pub sdk_stamps: Vec<SdkStamp>,
    /// Referenced IRX module and IOPRP image paths.
    pub referenced_modules: Vec<ReferencedModule>,
    /// EE syscalls with at least one active caller (`call_count > 0`).
    pub called_syscalls: Vec<SyscallSite>,
    /// EE syscalls linked in library stubs but never invoked (`call_count == 0`).
    pub uncalled_syscalls: Vec<SyscallSite>,
    /// Libmc and libpad minimum module version gates.
    pub version_gates: Vec<VersionGate>,
    /// Hard-coded video mode detected from `sceGsResetGraph`, if present.
    pub video_mode: Option<(DetectedVideoMode, usize)>,
    /// Libcdvd command usage summary.
    pub cdvd_usage: Vec<CdvdUsage>,
}

impl ElfProbe {
    /// Inspects a PS2 boot ELF executable.
    pub fn inspect(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 52 || bytes[..4] != [0x7F, b'E', b'L', b'F'] {
            return Err(Error::Corrupt(
                Format::DiscImage,
                "not an ELF executable (bad magic)".into(),
            ));
        }

        let entry = bytes.u32(0x18);
        let phoff = bytes.u32(0x1C) as usize;
        let phentsize = bytes.u16(0x2A) as usize;
        let phnum = bytes.u16(0x2C) as usize;

        let mut segments = Vec::new();
        for i in 0..phnum {
            let ph = phoff + i * phentsize;
            if ph + 32 > bytes.len() {
                break;
            }
            let p_type = bytes.u32(ph);
            let p_offset = bytes.u32(ph + 4) as usize;
            let p_vaddr = bytes.u32(ph + 8);
            let p_paddr = bytes.u32(ph + 12);
            let p_filesz = bytes.u32(ph + 16) as usize;
            let p_memsz = bytes.u32(ph + 20) as usize;
            let p_flags = bytes.u32(ph + 24);
            let p_align = bytes.u32(ph + 28);

            segments.push(ProbeSegment {
                seg_type: p_type,
                file_offset: p_offset,
                vaddr: p_vaddr,
                paddr: p_paddr,
                file_size: p_filesz,
                mem_size: p_memsz,
                flags: p_flags,
                align: p_align,
            });
        }

        let file_to_vaddr = |foff: usize| -> Option<u32> {
            for s in &segments {
                if s.seg_type == 1 && s.file_offset <= foff && foff < s.file_offset + s.file_size {
                    return Some(s.vaddr + (foff - s.file_offset) as u32);
                }
            }
            None
        };

        // 1. Scan SDK stamps
        let sdk_stamps = Self::scan_sdk_stamps(bytes);

        // 2. Scan referenced IRX / IOPRP modules
        let referenced_modules = Self::scan_referenced_modules(bytes);

        // 3. Scan EE syscalls (stubs + callers)
        let (called_syscalls, uncalled_syscalls) = Self::scan_syscalls(bytes, &segments);

        // 4. Scan version gates (libmc/libpad)
        let version_gates = Self::scan_version_gates(bytes, &file_to_vaddr);

        // 5. Detect hard-coded video mode
        let video_mode = Self::detect_video_mode(bytes, &segments);

        // 6. Scan libcdvd usage
        let cdvd_usage = Self::scan_cdvd_usage(bytes, &segments);

        Ok(Self {
            entry,
            segments,
            sdk_stamps,
            referenced_modules,
            called_syscalls,
            uncalled_syscalls,
            version_gates,
            video_mode,
            cdvd_usage,
        })
    }

    /// The SDK library stamps in any byte range (an ELF, an IRX), in file order.
    #[must_use]
    pub fn sdk_stamps_in(bytes: &[u8]) -> Vec<SdkStamp> { Self::scan_sdk_stamps(bytes) }

    /// SDK library stamps have one shape (`games_probe.md` §1.1): `PsIIlib` or `lib`, a lowercase
    /// library name, optional spaces, then a four-digit version (`PsIIlibcdvd 1650`,
    /// `PsIIlibrspu21620`, `libdma  1620`); stamps are often packed back to back. Anything else
    /// that merely starts with "lib" (UI text such as "libre en la MEMORY CARD") is not a stamp.
    fn scan_sdk_stamps(bytes: &[u8]) -> Vec<SdkStamp> {
        let mut stamps = Vec::new();
        let mut p = 0;
        while p + 8 <= bytes.len() {
            match Self::stamp_at(bytes, p) {
                Some((len, stamp)) => { stamps.push(SdkStamp { offset: p, stamp }); p += len }
                None => p += 1,
            }
        }
        stamps
    }

    /// The stamp starting at `p`, as `(byte length, normalised text)`.
    fn stamp_at(b: &[u8], p: usize) -> Option<(usize, String)> {
        let prefix: &[u8] = if b[p..].starts_with(b"PsIIlib") { b"PsIIlib" } else if b[p..].starts_with(b"lib") { b"lib" } else { return None };
        let mut q = p + prefix.len();
        let token_start = q;
        while q < b.len() && (b[q].is_ascii_lowercase() || b[q].is_ascii_digit()) { q += 1 }
        let token = &b[token_start..q];
        let mut r = q;
        while r < b.len() && b[r] == b' ' { r += 1 }
        let digits = |s: &[u8]| s.len() == 4 && s.iter().all(u8::is_ascii_digit);
        let (name, version, end) = if r > q && r + 4 <= b.len() && digits(&b[r..r + 4]) && b.get(r + 4).is_none_or(|c| !c.is_ascii_digit()) {
            (token, &b[r..r + 4], r + 4)
        } else if token.len() >= 6 && digits(&token[token.len() - 4..]) {
            (&token[..token.len() - 4], &token[token.len() - 4..], q)
        } else {
            return None;
        };
        if name.len() < 2 || !name[0].is_ascii_lowercase() { return None }
        let text = format!("{}{} {}", String::from_utf8_lossy(prefix), String::from_utf8_lossy(name), String::from_utf8_lossy(version));
        Some((end - p, text))
    }

    fn scan_referenced_modules(bytes: &[u8]) -> Vec<ReferencedModule> {
        let mut modules = Vec::new();
        let mut seen = HashSet::new();

        // Scan for device paths like cdrom:\IRX\*.IRX;1 or rom0:UDNL
        let needles: &[&[u8]] = &[b".IRX", b".irx", b".IMG", b".img", b"rom0:UDNL"];
        for needle in needles {
            let mut pos = 0;
            while let Some(rel) = bytes[pos..].windows(needle.len()).position(|w| w == *needle) {
                let idx = pos + rel;
                // Walk backward to find the start of the string
                let mut start = idx;
                while start > 0 && bytes[start - 1] > 0x20 && bytes[start - 1] < 0x7F {
                    start -= 1;
                }
                // Walk forward to find the end of the string
                let mut end = idx + needle.len();
                while end < bytes.len() && bytes[end] > 0x20 && bytes[end] < 0x7F {
                    end += 1;
                }
                let s = String::from_utf8_lossy(&bytes[start..end]).into_owned();
                if (s.contains(':') || s.contains('/') || s.contains('\\')) && seen.insert(s.clone()) {
                    modules.push(ReferencedModule { offset: start, path: s });
                }
                pos = idx + needle.len();
            }
        }
        modules.sort_by_key(|m| m.offset);
        modules
    }

    fn scan_syscalls(
        bytes: &[u8],
        segments: &[ProbeSegment],
    ) -> (Vec<SyscallSite>, Vec<SyscallSite>) {
        let file_to_vaddr = |foff: usize| -> Option<u32> {
            for s in segments {
                if s.seg_type == 1 && s.file_offset <= foff && foff < s.file_offset + s.file_size {
                    return Some(s.vaddr + (foff - s.file_offset) as u32);
                }
            }
            None
        };

        // Map: stub_vaddr -> (sys_num, stub_file_offset)
        let mut stubs = HashMap::new();
        for i in (4..bytes.len().saturating_sub(4)).step_by(4) {
            let word = bytes.u32(i);
            if word == 0x0000_000C {
                // syscall
                let prev = bytes.u32(i - 4);
                let v1_val = if (prev & 0xFFFF_0000) == 0x2403_0000 {
                    // addiu $v1, $zero, imm (signed)
                    Some((prev & 0xFFFF) as i16 as i32)
                } else if (prev & 0xFFFF_0000) == 0x3403_0000 {
                    // ori $v1, $zero, imm (unsigned)
                    Some((prev & 0xFFFF) as i32)
                } else {
                    None
                };

                if let Some(num) = v1_val {
                    if let Some(stub_va) = file_to_vaddr(i - 4) {
                        stubs.insert(stub_va, (num, i - 4));
                    }
                }
            }
        }

        // Scan callers: jal or j
        let mut callers = HashMap::new();
        for &stub_va in stubs.keys() {
            callers.insert(stub_va, Vec::new());
        }

        for s in segments {
            if s.seg_type != 1 {
                continue;
            }
            let seg_end = (s.file_offset + s.file_size).min(bytes.len());
            for i in (s.file_offset..seg_end.saturating_sub(4)).step_by(4) {
                let word = bytes.u32(i);
                let op = word >> 26;
                if op == 2 || op == 3 {
                    // j or jal
                    if let Some(curr_va) = file_to_vaddr(i) {
                        let target_va = (curr_va & 0xF000_0000) | ((word & 0x03FF_FFFF) << 2);
                        if let Some(list) = callers.get_mut(&target_va) {
                            list.push(i);
                        }
                    }
                }
            }
        }

        let mut called = Vec::new();
        let mut uncalled = Vec::new();

        for (&stub_va, &(num, stub_off)) in &stubs {
            let caller_offsets = callers.remove(&stub_va).unwrap_or_default();
            let count = caller_offsets.len();
            let site = SyscallSite {
                number: num,
                name: ee_syscall_name(num),
                stub_vaddr: stub_va,
                stub_offset: stub_off,
                call_count: count,
                caller_offsets,
            };
            if count > 0 {
                called.push(site);
            } else {
                uncalled.push(site);
            }
        }

        called.sort_by_key(|s| s.number);
        uncalled.sort_by_key(|s| s.number);
        (called, uncalled)
    }

    fn scan_version_gates(
        bytes: &[u8],
        file_to_vaddr: &impl Fn(usize) -> Option<u32>,
    ) -> Vec<VersionGate> {
        let mut gates = Vec::new();
        for i in (0..bytes.len().saturating_sub(4)).step_by(4) {
            let inst = bytes.u32(i);
            let op = inst >> 26;
            if op == 0x0A {
                // slti $rt, $rs, imm
                let imm = (inst & 0xFFFF) as u16;
                let module = match imm {
                    0x205 => Some("mcserv"),
                    0x206 => Some("mcman"),
                    0x204 => Some("padman"),
                    _ => None,
                };
                if let Some(m) = module {
                    let va = file_to_vaddr(i).unwrap_or(0);
                    gates.push(VersionGate {
                        module: m.to_string(),
                        min_version: imm,
                        offset: i,
                        vaddr: va,
                    });
                }
            }
        }
        gates.sort_by_key(|g| g.offset);
        gates
    }

    fn detect_video_mode(bytes: &[u8], segments: &[ProbeSegment]) -> Option<(DetectedVideoMode, usize)> {
        let file_to_vaddr = |foff: usize| -> Option<u32> {
            for s in segments {
                if s.seg_type == 1 && s.file_offset <= foff && foff < s.file_offset + s.file_size {
                    return Some(s.vaddr + (foff - s.file_offset) as u32);
                }
            }
            None
        };

        // Scan for calls where $a2 (register 6) is loaded with 2 (NTSC) or 3 (PAL)
        // preceding a jal instruction.
        for s in segments {
            if s.seg_type != 1 {
                continue;
            }
            let seg_end = (s.file_offset + s.file_size).min(bytes.len());
            for i in (s.file_offset..seg_end.saturating_sub(4)).step_by(4) {
                let inst = bytes.u32(i);
                if (inst >> 26) == 3 {
                    // jal
                    // Look at previous instructions (up to 4 back) and the delay slot
                    let mut found_mode = None;
                    let check_inst = |inst: u32| -> Option<DetectedVideoMode> {
                        let op = inst >> 26;
                        let rt = (inst >> 16) & 0x1F;
                        let imm = inst & 0xFFFF;
                        if (op == 9 || op == 13) && rt == 6 {
                            // addiu $a2, $zero, imm or ori $a2, $zero, imm
                            if imm == 2 {
                                return Some(DetectedVideoMode::Ntsc);
                            }
                            if imm == 3 {
                                return Some(DetectedVideoMode::Pal);
                            }
                        }
                        None
                    };

                    for back in 1..=4 {
                        if i >= back * 4 {
                            if let Some(m) = check_inst(bytes.u32(i - back * 4)) {
                                found_mode = Some((m, i));
                                break;
                            }
                        }
                    }
                    if found_mode.is_none() && i + 4 < seg_end {
                        if let Some(m) = check_inst(bytes.u32(i + 4)) {
                            found_mode = Some((m, i));
                        }
                    }

                    if let Some((mode, off)) = found_mode {
                        // Confirm if this call site is inside ResetGraph / ResetScreen logic
                        let curr_va = file_to_vaddr(off).unwrap_or(0);
                        if (0x398000..0x3A0000).contains(&curr_va) {
                            return Some((mode, off));
                        }
                    }
                }
            }
        }
        None
    }

    fn scan_cdvd_usage(bytes: &[u8], segments: &[ProbeSegment]) -> Vec<CdvdUsage> {
        let mut usage = Vec::new();
        let file_to_vaddr = |foff: usize| -> Option<u32> {
            for s in segments {
                if s.seg_type == 1 && s.file_offset <= foff && foff < s.file_offset + s.file_size {
                    return Some(s.vaddr + (foff - s.file_offset) as u32);
                }
            }
            None
        };

        // Look for sceCdInit and sceCdMmode calls at start-up
        for s in segments {
            if s.seg_type != 1 {
                continue;
            }
            let seg_end = (s.file_offset + s.file_size).min(bytes.len());
            for i in (s.file_offset..seg_end.saturating_sub(4)).step_by(4) {
                let inst = bytes.u32(i);
                if (inst >> 26) == 3 {
                    // jal
                    if let Some(va) = file_to_vaddr(i) {
                        let target = (va & 0xF000_0000) | ((inst & 0x03FF_FFFF) << 2);
                        if target == 0x3F4D28 {
                            usage.push(CdvdUsage {
                                name: "sceCdInit".into(),
                                offset: i,
                                details: "binds S-cmd server 0x80000592, fno 0".into(),
                            });
                        } else if target == 0x3F5620 {
                            usage.push(CdvdUsage {
                                name: "sceCdMmode".into(),
                                offset: i,
                                details: "media mode CD (S-cmd fno 0x22, SCECdCD = 1)".into(),
                            });
                        }
                    }
                }
            }
        }

        // Trace strings
        if bytes.windows(14).any(|w| w == b"Scmd fail sema") {
            usage.push(CdvdUsage {
                name: "libcdvd (Scmd)".into(),
                offset: 0,
                details: "S-command RPC dispatcher present".into(),
            });
        }
        if bytes.windows(14).any(|w| w == b"Ncmd fail sema") {
            usage.push(CdvdUsage {
                name: "libcdvd (Ncmd)".into(),
                offset: 0,
                details: "N-command RPC dispatcher present".into(),
            });
        }
        usage
    }
}

/// Canonical EE syscall names from `games_probe.md §1.5` and standard EE kernel table.
fn ee_syscall_name(num: i32) -> Option<&'static str> {
    Some(match num {
        1 => "ResetEE",
        2 => "SetGsCrt",
        4 => "Exit",
        5 => "_LoadExecPS2",
        6 => "ExecPS2",
        7 => "SetVCommon",
        0x10 => "AddIntcHandler",
        0x11 => "RemoveIntcHandler",
        0x12 => "AddDmacHandler",
        0x13 => "RemoveDmacHandler",
        0x14 | -0x14 => "EnableIntc",
        0x15 | -0x15 => "DisableIntc",
        0x16 | -0x16 => "EnableDmac",
        0x17 | -0x17 => "DisableDmac",
        0x18 | -0x18 => "SetAlarm",
        0x19 | -0x19 => "ReleaseAlarm",
        0x20 => "CreateThread",
        0x21 => "DeleteThread",
        0x22 => "StartThread",
        0x23 => "ExitThread",
        0x24 => "ExitDeleteThread",
        0x25 => "TerminateThread",
        0x26 | -0x26 => "iTerminateThread",
        0x27 => "DisableDispatchThread",
        0x28 => "EnableDispatchThread",
        0x29 => "ChangeThreadPriority",
        0x2A | -0x2A => "iChangeThreadPriority",
        0x2B => "RotateThreadReadyQueue",
        0x2C | -0x2C => "iRotateThreadReadyQueue",
        0x2D => "ReleaseWaitThread",
        0x2E | -0x2E => "iReleaseWaitThread",
        0x2F => "GetThreadId",
        0x30 => "ReferThreadStatus",
        0x31 | -0x31 => "iReferThreadStatus",
        0x32 => "SleepThread",
        0x33 => "WakeupThread",
        0x34 | -0x34 => "iWakeupThread",
        0x35 => "CancelWakeupThread",
        0x36 | -0x36 => "iCancelWakeupThread",
        0x37 => "SuspendThread",
        0x38 | -0x38 => "iSuspendThread",
        0x39 => "ResumeThread",
        0x3A | -0x3A => "iResumeThread",
        0x3C => "InitMainThread",
        0x3D => "InitHeap",
        0x3E => "EndOfHeap",
        0x40 => "CreateSema",
        0x41 => "DeleteSema",
        0x42 => "SignalSema",
        0x43 | -0x43 => "iSignalSema",
        0x44 => "WaitSema",
        0x45 => "PollSema",
        0x46 | -0x46 => "iPollSema",
        0x47 => "ReferSemaStatus",
        0x48 | -0x48 => "iReferSemaStatus",
        0x4B => "GetOsdConfigParam",
        0x4C => "SetOsdConfigParam",
        0x4D => "GetGsVParam",
        0x4E => "SetGsVParam",
        0x64 => "FlushCache",
        0x67 | -0x67 => "GetCop0",
        0x6B => "sceSifStopDma",
        0x70 => "GsGetIMR",
        0x71 => "GsPutIMR",
        0x73 => "SetVSyncFlag",
        0x74 => "SetSyscall",
        0x75 => "_print",
        0x76 => "SifDmaInit",
        0x77 | -0x77 => "SifSetDma",
        0x78 | -0x78 => "SifDmaStat",
        0x79 => "SifSetDChain",
        0x7A => "SifSetReg",
        0x7B => "SifGetReg",
        0x7C => "Deci2Call",
        0x7D => "PSMode",
        0x7E => "MachineType",
        0x7F => "GetMemorySize",
        _ => return None,
    })
}

// ============================================================================
// 3. IOPRP & IRX Probe (IoprpReport, IrxProbe)
// ============================================================================

/// Metadata of a module stored inside an IOPRP image (`games_probe.md §1.8`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IoprpModule {
    /// Module name (e.g. `"SIFCMD"`, `"CDVDMAN"`).
    pub name: String,
    /// Offset within the image.
    pub offset: usize,
    /// Size in bytes.
    pub size: usize,
    /// Version string from EXTINFO (type 2), if present.
    pub version: Option<String>,
    /// Build date string from EXTINFO (type 1), if present.
    pub date: Option<String>,
    /// Comment string from EXTINFO (type 3), if present.
    pub comment: Option<String>,
}

/// Inspection report for an `IOPRP*.IMG` image (`games_probe.md §1.3`, §1.8).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IoprpReport {
    /// Build comment embedded in ROMDIR EXTINFO (e.g. `"xokano@rel-linux/~/tmp_165"`).
    pub romdir_comment: Option<String>,
    /// List of modules contained in the image.
    pub modules: Vec<IoprpModule>,
}

impl IoprpReport {
    /// Parses an `IOPRP*.IMG` container using ROMDIR and EXTINFO metadata.
    pub fn inspect(data: &[u8]) -> Result<Self> {
        let romdir = RomDir::new(data.to_vec())?;
        let base = data
            .windows(10)
            .position(|w| w == b"RESET\0\0\0\0\0")
            .ok_or_else(|| Error::NotA(Format::Bios, "no ROMDIR table in IOPRP image".into()))?;

        // Parse ROMDIR entries with extinfo lengths: (name, off, size, ext)
        let mut raw_entries = Vec::new();
        let mut off = 0;
        let mut p = base;
        while p + 16 <= data.len() && data[p] != 0 {
            let name = data.cstr(p, 10);
            let ext = data.u16(p + 10) as usize;
            let size = data.u32(p + 12) as usize;
            raw_entries.push((name, off, size, ext));
            off += (size + 15) & !15;
            p += 16;
        }

        let ext_off = raw_entries
            .iter()
            .find(|(n, _, _, _)| n == "EXTINFO")
            .map(|(_, o, _, _)| *o)
            .unwrap_or(0);

        let mut modules = Vec::new();
        let mut ep = ext_off;
        let mut romdir_comment = None;

        for (name, o, s, ext) in raw_entries {
            let mut date = None;
            let mut version = None;
            let mut comment = None;

            if ext > 0 && ep + ext <= data.len() {
                let mut bp = ep;
                let bend = ep + ext;
                while bp + 4 <= bend {
                    let val = data.u16(bp);
                    let rec_size = data[bp + 2] as usize;
                    let typ = data[bp + 3];
                    bp += 4;
                    if bp + rec_size > bend {
                        break;
                    }
                    let rec_data = &data[bp..bp + rec_size];
                    bp += rec_size;
                    match typ {
                        1 if rec_size >= 4 => {
                            // Date in BCD: YYYY-MM-DD
                            date = Some(format!(
                                "{:02X}{:02X}-{:02X}-{:02X}",
                                rec_data[3], rec_data[2], rec_data[1], rec_data[0]
                            ));
                        }
                        2 => {
                            let maj = val >> 8;
                            let min = val & 0xFF;
                            version = Some(format!("{maj}.{min:02}"));
                        }
                        3 => {
                            let text = String::from_utf8_lossy(rec_data)
                                .trim_end_matches('\0')
                                .to_string();
                            comment = Some(text);
                        }
                        _ => {}
                    }
                }
            }
            ep += ext;

            if name == "ROMDIR" {
                romdir_comment = comment.clone();
            }

            if romdir.module(&name).is_ok() {
                modules.push(IoprpModule {
                    name,
                    offset: o,
                    size: s,
                    version,
                    date,
                    comment,
                });
            }
        }

        Ok(Self {
            romdir_comment,
            modules,
        })
    }
}

/// Information extracted from an `.IRX` module file (`.iopmod` section).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IrxProbe {
    /// Module name (e.g. `"sio2man"`, `"mcman_tool"`).
    pub module_name: String,
    /// Module version code (e.g. 0x0201 for 2.01).
    pub version: u16,
    /// Formatted version string (e.g. `"2.01"`).
    pub version_string: String,
}

impl IrxProbe {
    /// Inspects an `.IRX` ELF binary for its `.iopmod` metadata.
    pub fn inspect(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 52 || bytes[..4] != [0x7F, b'E', b'L', b'F'] {
            return Err(Error::Corrupt(Format::DiscImage, "not an IRX ELF".into()));
        }

        let e_shoff = bytes.u32(0x20) as usize;
        let e_shentsize = bytes.u16(0x2E) as usize;
        let e_shnum = bytes.u16(0x30) as usize;
        let e_shstrndx = bytes.u16(0x32) as usize;

        if e_shstrndx >= e_shnum || e_shoff + (e_shstrndx + 1) * e_shentsize > bytes.len() {
            return Err(Error::Corrupt(Format::DiscImage, "bad section header table".into()));
        }

        let sh_str_hdr = e_shoff + e_shstrndx * e_shentsize;
        let str_off = bytes.u32(sh_str_hdr + 0x10) as usize;

        for i in 0..e_shnum {
            let sh = e_shoff + i * e_shentsize;
            if sh + 0x28 > bytes.len() {
                break;
            }
            let name_idx = bytes.u32(sh) as usize;
            if str_off + name_idx < bytes.len() {
                let sec_name = bytes.cstr(str_off + name_idx, 32);
                if sec_name == ".iopmod" {
                    let off = bytes.u32(sh + 0x10) as usize;
                    let size = bytes.u32(sh + 0x14) as usize;
                    if off + 26 <= bytes.len() && size >= 26 {
                        let ver = bytes.u16(off + 24);
                        let name = bytes.cstr(off + 26, size.saturating_sub(26));
                        let maj = ver >> 8;
                        let min = ver & 0xFF;
                        return Ok(Self {
                            module_name: name,
                            version: ver,
                            version_string: format!("{maj}.{min:02}"),
                        });
                    }
                }
            }
        }

        Err(Error::NotFound(Format::DiscImage, "no .iopmod section found".into()))
    }
}

// ============================================================================
// 4. PS1 Executable Probe (Ps1ExeProbe)
// ============================================================================

/// A PlayStation 1 BIOS call site found in a PS-X EXE (`games_probe.md §2.2`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1BiosCall {
    /// BIOS vector table: 0xA0, 0xB0, or 0xC0.
    pub table: u8,
    /// Function number in the table (e.g. 0x39 for InitHeap).
    pub function: u8,
    /// Well-known name from `games_probe.md §2.2`.
    pub name: Option<&'static str>,
    /// File offset of the `jr $t2` call site.
    pub offset: usize,
}

/// Inspection report for a PlayStation 1 executable.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1ExeProbe {
    /// Loadability check from `ps1boot::Ps1ExeCheck`.
    pub check: crate::ps1boot::Ps1ExeCheck,
    /// PsyQ and RCS version stamps found in the file.
    pub stamps: Vec<SdkStamp>,
    /// BIOS calls used in the executable.
    pub bios_calls: Vec<Ps1BiosCall>,
    /// Whether the dead SCE kernel bootstrap ROM probe (`0xBFC00100` / `0xBFC0E228`) is present.
    pub dead_rom_probe_present: bool,
}

impl Ps1ExeProbe {
    /// Inspects a PlayStation 1 executable.
    pub fn inspect(bytes: &[u8]) -> Result<Self> {
        let check = crate::ps1boot::Ps1ExeCheck::judge(bytes, bytes.len());

        // 1. Scan for stamps ($Id:, PsyQ, Sony Computer Entertainment)
        let mut stamps = Vec::new();
        let mut pos = 0;
        while let Some(rel) = bytes[pos..].windows(5).position(|w| w == b"$Id: ") {
            let idx = pos + rel;
            let rem = &bytes[idx + 5..];
            let end_rel = rem
                .iter()
                .position(|&b| b == b'$' || b == 0 || b == b'\r' || b == b'\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            let end = idx + 5 + end_rel;
            let s = String::from_utf8_lossy(&bytes[idx..end.min(idx + 120)]).into_owned();
            stamps.push(SdkStamp { offset: idx, stamp: s });
            pos = end.max(idx + 5);
        }

        // 2. Scan BIOS calls: jr $t2 (0x01400008) preceded by li $t2, 0xA0/B0/C0 and li $t1, n
        let mut bios_calls = Vec::new();
        for i in (4..bytes.len().saturating_sub(4)).step_by(4) {
            let word = bytes.u32(i);
            if word == 0x0140_0008 {
                // jr $t2
                let prev = bytes.u32(i - 4);
                let table_val = if (prev & 0xFFFF_FF00) == 0x240A_0000 || (prev & 0xFFFF_FF00) == 0x340A_0000 {
                    // addiu $t2, $zero, table or ori $t2, $zero, table
                    Some((prev & 0xFF) as u8)
                } else {
                    None
                };

                if let Some(tbl) = table_val {
                    if tbl == 0xA0 || tbl == 0xB0 || tbl == 0xC0 {
                        // Look for li $t1, n in delay slot (i + 4) or prior instruction
                        let next = bytes.u32(i + 4);
                        let func_val = if (next & 0xFFFF_FF00) == 0x2409_0000 || (next & 0xFFFF_FF00) == 0x3409_0000 {
                            Some((next & 0xFF) as u8)
                        } else if i >= 8 {
                            let p2 = bytes.u32(i - 8);
                            if (p2 & 0xFFFF_FF00) == 0x2409_0000 || (p2 & 0xFFFF_FF00) == 0x3409_0000 {
                                Some((p2 & 0xFF) as u8)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(fno) = func_val {
                            bios_calls.push(Ps1BiosCall {
                                table: tbl,
                                function: fno,
                                name: ps1_bios_call_name(tbl, fno),
                                offset: i,
                            });
                        }
                    }
                }
            }
        }

        // 3. Dead ROM probe check (games_probe.md §2.2):
        // Checks for the sys.c 1.129 routine loading *(0xBFC00100) and *(0xBFC00104),
        // comparing with 0x19940728 and 0x2000, and jumping to 0xBFC0E228.
        let mut has_bfc0 = false;
        let mut has_e228 = false;
        let mut has_1994 = false;
        for i in (0..bytes.len().saturating_sub(4)).step_by(4) {
            let w = bytes.u32(i);
            let op = w >> 26;
            let imm = (w & 0xFFFF) as u16;
            if op == 0x0F && imm == 0xBFC0 {
                has_bfc0 = true;
            }
            if imm == 0xE228 {
                has_e228 = true;
            }
            if op == 0x0F && imm == 0x1994 {
                has_1994 = true;
            }
        }
        let dead_rom_probe_present = has_bfc0 && has_e228 && has_1994;

        Ok(Self {
            check,
            stamps,
            bios_calls,
            dead_rom_probe_present,
        })
    }
}

/// Canonical PS1 BIOS call names from `games_probe.md §2.2`.
fn ps1_bios_call_name(table: u8, function: u8) -> Option<&'static str> {
    Some(match (table, function) {
        // Table A0
        (0xA0, 0x13) => "SaveState",
        (0xA0, 0x15) => "strcat",
        (0xA0, 0x17) => "strcmp",
        (0xA0, 0x18) => "strncmp",
        (0xA0, 0x19) => "strcpy",
        (0xA0, 0x2B) => "memset",
        (0xA0, 0x2F) => "rand",
        (0xA0, 0x30) => "srand",
        (0xA0, 0x39) => "InitHeap",
        (0xA0, 0x44) => "FlushCache",
        (0xA0, 0x49) => "GPU_cw",
        (0xA0, 0x70) => "_bu_init",
        (0xA0, 0x72) => "_96_remove",
        (0xA0, 0xA0) => "WarmBoot",
        (0xA0, 0xAC) => "_card_async_load_directory",
        (0xA0, 0xB4) => "GetSystemInfo",
        (0xA0, 0x9D) => "GetConf",

        // Table B0
        (0xB0, 0x07) => "DeliverEvent",
        (0xB0, 0x08) => "OpenEvent",
        (0xB0, 0x09) => "TestEvent",
        (0xB0, 0x0A) => "CloseEvent",
        (0xB0, 0x0B) => "EnableEvent",
        (0xB0, 0x0C) => "DisableEvent",
        (0xB0, 0x0D) => "WaitEvent",
        (0xB0, 0x12) => "InitPad",
        (0xB0, 0x13) => "StartPad",
        (0xB0, 0x14) => "StopPad",
        (0xB0, 0x15) => "OutdatedPadGetButtons",
        (0xB0, 0x16) => "OutdatedPadInitAndStart",
        (0xB0, 0x17) => "ReturnFromException",
        (0xB0, 0x19) => "SetCustomExitFromException",
        (0xB0, 0x32) => "FileOpen",
        (0xB0, 0x34) => "FileRead",
        (0xB0, 0x35) => "FileWrite",
        (0xB0, 0x36) => "FileClose",
        (0xB0, 0x38) => "exit",
        (0xB0, 0x41) => "FormatDevice",
        (0xB0, 0x42) => "firstfile",
        (0xB0, 0x4A) => "InitCard",
        (0xB0, 0x4B) => "StartCard",
        (0xB0, 0x4E) => "write_card_sector",
        (0xB0, 0x50) => "allow_new_card",
        (0xB0, 0x56) => "GetC0Table",
        (0xB0, 0x57) => "GetB0Table",
        (0xB0, 0x5B) => "ChangeClearPad",

        // Table C0
        (0xC0, 0x0A) => "ChangeClearRCnt",

        _ => return None,
    })
}

// ============================================================================
// 5. SYSTEM.CNF Key Audit (CnfKeyAudit)
// ============================================================================

/// Console component that consults a `SYSTEM.CNF` key (`games_probe.md §1.1`, §2.1, §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CnfKeyReader {
    /// OSDSYS (checks disc ID, launches `rom0:PS2LOGO` / `rom0:PS1DRV`).
    Osdsys,
    /// PlayStation 1 kernel / TBIN (sets execution parameters in RAM).
    Ps1Kernel,
    /// PlayStation Driver (`rom0:PS1DRV`) compatibility layer.
    Ps1Drv,
    /// Not consulted by any ROM component (orphaned / unused).
    Nobody,
    /// Key not recognized in PlayStation specifications.
    Unknown,
}

impl fmt::Display for CnfKeyReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Osdsys => f.write_str("OSDSYS"),
            Self::Ps1Kernel => f.write_str("PS1 kernel / TBIN"),
            Self::Ps1Drv => f.write_str("PS1DRV"),
            Self::Nobody => f.write_str("nobody"),
            Self::Unknown => f.write_str("unknown"),
        }
    }
}

/// Audit record for one key found in a disc's `SYSTEM.CNF` file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CnfKeyAudit {
    /// Key name as specified (e.g. `"BOOT2"`, `"VER"`, `"VMODE"`).
    pub key: String,
    /// Verbatim value token.
    pub value: String,
    /// Subsystems that read this key.
    pub readers: Vec<CnfKeyReader>,
    /// Architectural rationale citing research notes.
    pub explanation: &'static str,
}

/// Audits a `SYSTEM.CNF` file, reporting which console components read each key and why.
#[must_use]
pub fn audit_system_cnf(text: &str) -> Vec<CnfKeyAudit> {
    let mut audits = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim().to_ascii_uppercase();
            let val = v.trim().to_string();

            let (readers, explanation) = match key.as_str() {
                "BOOT2" => (
                    vec![CnfKeyReader::Osdsys],
                    "OSDSYS reads BOOT2, checks file name against MechaCon disc ID, and launches rom0:PS2LOGO (osdsys_hooks.md §2, games_probe.md §1.1)",
                ),
                "VER" => (
                    vec![CnfKeyReader::Osdsys],
                    "OSDSYS reads VER for PS2 discs, and passes it as argv[2] to PS1DRV for PS1 discs (osdsys_hooks.md §2, games_probe.md §1.1)",
                ),
                "VMODE" => (
                    vec![CnfKeyReader::Nobody],
                    "Nobody reads VMODE: never consulted by OSDSYS, PS2LOGO, EELOAD, or KERNEL; video mode is determined by the game ELF itself (games_probe.md §1.1, §1.6)",
                ),
                "BOOT" => (
                    vec![CnfKeyReader::Osdsys, CnfKeyReader::Ps1Kernel],
                    "OSDSYS reads BOOT to derive title ID; PS1 kernel / TBIN reads BOOT into A0:0000B8B0 as the boot executable (ps1_kernel_boot.md §4.1, games_probe.md §2.1)",
                ),
                "TCB" => (
                    vec![CnfKeyReader::Ps1Kernel],
                    "PS1 kernel / TBIN sets Thread Control Blocks count at A0:0000B940 (hex without prefix; ps1_kernel_boot.md §4.1, games_probe.md §2.1)",
                ),
                "EVENT" => (
                    vec![CnfKeyReader::Ps1Kernel],
                    "PS1 kernel / TBIN sets Event Control Blocks count at A0:0000B944 (hex without prefix; ps1_kernel_boot.md §4.1, games_probe.md §2.1)",
                ),
                "STACK" => (
                    vec![CnfKeyReader::Ps1Kernel],
                    "PS1 kernel / TBIN sets stack pointer at A0:0000B948; 0 inherits BIOS default stack 0x801FFF00 (ps1_kernel_boot.md §4.1, games_probe.md §2.1)",
                ),
                "PSD1.0.0" => (
                    vec![CnfKeyReader::Ps1Drv],
                    "PS1DRV reads comma-separated override parameters p1..p8 from cdrom0:\\SYSTEM.CNF;1 to override compatibility table (games_probe.md §3.3)",
                ),
                _ => (
                    vec![CnfKeyReader::Unknown],
                    "Unrecognized SYSTEM.CNF key",
                ),
            };

            audits.push(CnfKeyAudit {
                key,
                value: val,
                readers,
                explanation,
            });
        }
    }
    audits
}

// ============================================================================
// 6. Unified Disc Probe (DiscProbe)
// ============================================================================

/// Complete probe inspection of a game disc image.
#[derive(Debug)]
#[non_exhaustive]
pub struct DiscProbe {
    /// Disc kind (PS2, PS1, or Unknown).
    pub kind: crate::disc::DiscKind,
    /// Indexed files on the disc.
    pub files: Vec<DiscFileEntry>,
    /// Audit of the disc's `SYSTEM.CNF` file.
    pub cnf_audit: Vec<CnfKeyAudit>,
    /// Inspection of the PS2 boot ELF, if applicable.
    pub elf_probe: Option<ElfProbe>,
    /// Report on the IOPRP image if referenced and found on the disc.
    pub ioprp_report: Option<IoprpReport>,
    /// IRX modules found on the disc.
    pub irx_modules: Vec<IrxProbe>,
    /// Inspection of the PS1 boot executable, if applicable.
    pub ps1_probe: Option<Ps1ExeProbe>,
}

impl DiscProbe {
    /// Inspects a disc image file (`.iso`, `.bin`, `.cue`, or `.img`).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut files = DiscFiles::open(path)?;

        let mut cnf_text = String::new();
        if let Ok(data) = files.read("SYSTEM.CNF") {
            cnf_text = String::from_utf8_lossy(&data).into_owned();
        }

        let cnf_audit = audit_system_cnf(&cnf_text);

        let has_boot2 = cnf_audit.iter().any(|a| a.key == "BOOT2");
        let has_boot = cnf_audit.iter().any(|a| a.key == "BOOT");

        let kind = if has_boot2 {
            crate::disc::DiscKind::Ps2
        } else if has_boot || files.list().iter().any(|f| f.path.to_ascii_uppercase().contains("PSX.EXE")) {
            crate::disc::DiscKind::Ps1
        } else {
            crate::disc::DiscKind::Unknown
        };

        let mut elf_probe = None;
        let mut ioprp_report = None;
        let mut irx_modules = Vec::new();
        let mut ps1_probe = None;

        if kind == crate::disc::DiscKind::Ps2 {
            if let Some(boot2) = cnf_audit.iter().find(|a| a.key == "BOOT2") {
                if let Ok(elf_bytes) = files.read(&boot2.value) {
                    if let Ok(probe) = ElfProbe::inspect(&elf_bytes) {
                        // Check if an IOPRP image was referenced and is on disc
                        for m in &probe.referenced_modules {
                            if m.path.to_ascii_uppercase().contains(".IMG") {
                                if let Ok(ioprp_bytes) = files.read(&m.path) {
                                    if let Ok(report) = IoprpReport::inspect(&ioprp_bytes) {
                                        ioprp_report = Some(report);
                                        break;
                                    }
                                }
                            }
                        }
                        elf_probe = Some(probe);
                    }
                }
            }

            // Probe all IRX files on disc
            for f in files.list().to_vec() {
                if f.path.to_ascii_uppercase().ends_with(".IRX;1") || f.path.to_ascii_uppercase().ends_with(".IRX") {
                    if let Ok(data) = files.read(&f.path) {
                        if let Ok(irx) = IrxProbe::inspect(&data) {
                            irx_modules.push(irx);
                        }
                    }
                }
            }
        } else if kind == crate::disc::DiscKind::Ps1 {
            let boot_path = cnf_audit
                .iter()
                .find(|a| a.key == "BOOT")
                .map(|a| a.value.as_str())
                .unwrap_or("PSX.EXE;1");

            if let Ok(exe_bytes) = files.read(boot_path) {
                if let Ok(probe) = Ps1ExeProbe::inspect(&exe_bytes) {
                    ps1_probe = Some(probe);
                }
            }
        }

        Ok(Self {
            kind,
            files: files.list().to_vec(),
            cnf_audit,
            elf_probe,
            ioprp_report,
            irx_modules,
            ps1_probe,
        })
    }
}
