//! Identities and chronological differences computed only over caller-supplied ROMs.
//! Evidence: `research/version_evolution.md` §2 and `ps1_version_matrix.md` §§2–3.

use crate::bytes::Bytes;
use crate::locate::{LogoLayout, OsdLayout, ProgramImage};
use crate::ps1bios::{BiosDate, KernelGeneration, Ps1Bios};
use crate::rom::{unpack, RomDir};
use crate::sound::SoundBank;
use crate::{Error, Format, Result};
use std::collections::{BTreeMap, BTreeSet};

/// Console family shared by every member of a set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Family {
    /// Standalone PS1, PSone or POPS BIOS.
    Ps1,
    /// PS2 ROMDIR archive.
    Ps2,
}

/// Coordinate system of a piece of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Space {
    /// Offset in the original file (including ROMDIR module offsets).
    File,
    /// Offset in an unpacked program, or decoded archive member.
    Decoded,
}

/// A byte span in this dump; addresses are observed load addresses, when available.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Evidence {
    /// Module or region containing these bytes.
    pub module: String,
    /// File or decoded-image coordinate system.
    pub space: Space,
    /// Offset in that coordinate system.
    pub offset: usize,
    /// Span length in bytes.
    pub length: usize,
    /// Virtual address of its first byte, if known.
    pub address: Option<usize>,
}

/// ROMDIR EXTINFO fields, parsed following `tools/extinfo.py`'s record format.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExtInfo {
    /// Last type-2 value; major is the high byte, minor the low byte.
    pub version: Option<u16>,
    /// Last type-1 BCD calendar date.
    pub date: Option<BiosDate>,
    /// Last type-3 comment, with trailing NULs removed.
    pub comment: Option<String>,
}

/// One present cell; absence is represented by `None` in [`IdentityRow::cells`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IdentityCell {
    /// Zero-based, row-local identity; equality means identical compared bytes.
    pub cluster: usize,
    /// Number of compared bytes, excluding alignment padding.
    pub size: usize,
    /// Runtime module metadata, independent of the byte identity.
    pub extinfo: ExtInfo,
    /// Location of compared bytes, followed by EXTINFO locations when present.
    pub evidence: Vec<Evidence>,
}

/// What the bytes in a matrix row represent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RowKind {
    /// Entire supplied file, useful for duplicate-file detection.
    Dump,
    /// Composite of separately reported regions; avoids duplicate change captions.
    Aggregate,
    /// A ROMDIR module, including its original packaging.
    Module,
    /// A PS1 ROM region; fixed windows are documented in matrix §2.2.
    Region,
    /// Unpacked executable, before or after loader stores.
    Image,
    /// A located numerical table, texture-name list or sound bank.
    Table,
}

/// One module/region/table across the chronologically sorted dumps.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IdentityRow {
    /// Module or descriptive region/table name.
    pub name: String,
    /// Meaning of the compared bytes.
    pub kind: RowKind,
    /// Cells in [`RomSet::dumps`] order; IDs are local to this row.
    pub cells: Vec<Option<IdentityCell>>,
}

/// A constant word store made by the callable ExecPS2 loader wrapper.
/// `version_evolution.md` §2.7: addresses and values come from instructions, not a list.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PatchedWord {
    /// Address written inside the unpacked OSDSYS.
    pub target: usize,
    /// Word in the original unpacked image (or the preceding store at this address).
    pub old: u32,
    /// Constant stored by the loader.
    pub new: u32,
    /// Minimal disassembly of the replaced instruction.
    pub old_instruction: String,
    /// Minimal disassembly of the stored instruction.
    pub new_instruction: String,
    /// Location of the loader's SW in the original dump.
    pub store: Evidence,
}

/// OSDSYS payload identity and conservatively decoded loader stores.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OsdComparison {
    /// Identity in the `OSDSYS (unpacked)` row.
    pub image_cluster: usize,
    /// Identity after confirmed stores; absent when the wrapper cannot be decoded.
    pub effective_cluster: Option<usize>,
    /// Stores in execution order; an empty list is confirmed only when complete is true.
    pub patches: Vec<PatchedWord>,
    /// Entire called wrapper was recognised, or this is a plain ELF without a stub.
    pub complete: bool,
}

/// Identity read from one supplied dump, including optional locator diagnostics.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DumpIdentity {
    /// Caller-supplied name; never used to infer identity.
    pub name: String,
    /// ROMVER or standalone PS1 version text.
    pub version: String,
    /// ROMVER date, or PS1 shell date with kernel-header fallback.
    pub date: Option<BiosDate>,
    /// PS1 generation established by the existing code-feature reader.
    pub kernel_generation: Option<KernelGeneration>,
    /// OSDSYS loader comparison when it could be unpacked.
    pub osdsys: Option<OsdComparison>,
    /// Missing/unsupported optional comparisons; never silently called identical.
    pub diagnostics: Vec<String>,
}

/// One exact contiguous run of differing bytes (half-open row-relative coordinates).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ByteRange {
    /// First differing byte relative to the compared row.
    pub start: usize,
    /// First byte after the differing run.
    pub end: usize,
    /// Corresponding span in the earlier dump, if present.
    pub before: Option<Evidence>,
    /// Corresponding span in the later dump, if present.
    pub after: Option<Evidence>,
    /// Research function/site hint, explicitly labelled as a reference-site hint.
    pub hint: Option<String>,
}

/// Category of chronological change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChangeKind {
    /// Bytes become present.
    Added,
    /// Bytes cease to be present.
    Removed,
    /// Length and bytes change.
    Resized,
    /// Equal-length bytes change.
    Changed,
    /// EXTINFO changes even if module bytes do not.
    Metadata,
    /// Unpacked payload is identical but loader stores differ.
    Loader,
}

/// An evidence-bearing caption between two consecutive members of a set.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Change {
    /// Earlier index in [`RomSet::dumps`].
    pub before: usize,
    /// Later index in [`RomSet::dumps`].
    pub after: usize,
    /// Row/module involved.
    pub subject: String,
    /// Change category.
    pub kind: ChangeKind,
    /// Caption suitable for the chronoscope.
    pub caption: String,
    /// Count of differing bytes, including a non-shared tail.
    pub changed_bytes: usize,
    /// Exact differing runs for PS1 regions; other rows use the first differing byte.
    pub ranges: Vec<ByteRange>,
    /// Module/table spans or actual loader-store locations supporting the caption.
    pub evidence: Vec<Evidence>,
}

/// Ordered captions, grouped by consecutive pair in chronological order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Changelog {
    /// Captions; indices refer to the owning [`RomSet`].
    pub entries: Vec<Change>,
}

/// A runtime identity matrix; no hashes or identity tables are embedded.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RomSet {
    /// Family of all inputs.
    pub family: Family,
    /// Dumps sorted by bytes-derived date; undated members follow dated ones.
    pub dumps: Vec<DumpIdentity>,
    /// Module, region, executable and table comparisons.
    pub rows: Vec<IdentityRow>,
    /// Consecutive-build captions with evidence.
    pub changelog: Changelog,
}

#[derive(Clone)]
struct Sample {
    bytes: Vec<u8>,
    spans: Vec<Evidence>,
    extinfo: ExtInfo,
    metadata: Vec<Evidence>,
    mapped: bool,
}

impl Sample {
    fn new(bytes: &[u8], span: Evidence) -> Self {
        Self { bytes: bytes.to_vec(), spans: vec![span], extinfo: ExtInfo::default(), metadata: Vec::new(), mapped: true }
    }

    fn at(&self, start: usize, length: usize) -> Option<Evidence> {
        if !self.mapped { return None }
        let mut relative = start;
        for span in &self.spans {
            if relative < span.length {
                return Some(Evidence { offset: span.offset + relative, length: length.min(span.length - relative),
                    address: span.address.map(|a| a + relative), ..span.clone() });
            }
            relative -= span.length;
        }
        None
    }
}

fn file_span(module: &str, offset: usize, length: usize) -> Evidence {
    Evidence { module: module.into(), space: Space::File, offset, length, address: Some(0xBFC00000 + offset) }
}

fn image_span(module: &str, img: &ProgramImage, offset: usize, length: usize) -> Evidence {
    Evidence { module: module.into(), space: Space::Decoded, offset, length, address: Some(img.base() + offset) }
}

fn corrupt(message: &str) -> Error { Error::Corrupt(Format::Bios, message.into()) }

fn date(year: u16, month: u8, day: u8) -> Option<BiosDate> {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month { 2 => if leap { 29 } else { 28 }, 4 | 6 | 9 | 11 => 30, 1..=12 => 31, _ => return None };
    (year != 0 && day != 0 && day <= days).then_some(BiosDate { year, month, day })
}

fn ascii_date(text: &str) -> Option<BiosDate> {
    if text.len() != 8 || !text.bytes().all(|b| b.is_ascii_digit()) { return None }
    date(text[..4].parse().ok()?, text[4..6].parse().ok()?, text[6..].parse().ok()?)
}

fn ps1_date(bios: &Ps1Bios) -> Option<BiosDate> {
    bios.identity().version.split_whitespace().find_map(|s| {
        let fields: Vec<_> = s.split('/').collect();
        if fields.len() != 3 { return None }
        let year: u16 = fields[2].parse().ok()?;
        // Retail shell dates cross 1999→2000; use the conventional two-digit pivot.
        let year = if fields[2].len() == 2 { if year < 70 { 2000 + year } else { 1900 + year } } else { year };
        date(year, fields[0].parse().ok()?, fields[1].parse().ok()?)
    }).or(bios.identity().header_date)
}

fn date_key(d: Option<BiosDate>) -> (u16, u8, u8) { d.map(|d| (d.year, d.month, d.day)).unwrap_or((u16::MAX, 0, 0)) }

// Each directory record owns its u16 EXTINFO length, including alignment entries.
// `tools/extinfo.py`: four-byte headers followed by exactly `size` payload bytes.
fn metadata(rom: &RomDir) -> Result<BTreeMap<String, (ExtInfo, Vec<Evidence>)>> {
    let data = rom.data();
    let mut p = data.windows(10).position(|w| w == b"RESET\0\0\0\0\0").ok_or_else(|| corrupt("missing ROMDIR"))?;
    let ext = rom.entries().into_iter().find(|e| e.0 == "EXTINFO");
    let blob = ext.as_ref().map(|e| rom.module(&e.0)).transpose()?.unwrap_or(&[]);
    let mut cursor = 0usize;
    let mut result = BTreeMap::new();
    loop {
        let entry = data.get(p..p + 16).ok_or_else(|| corrupt("unterminated ROMDIR"))?;
        if entry[0] == 0 { break }
        let name = entry.cstr(0, 10);
        let length = entry.u16(10) as usize;
        let records = blob.get(cursor..cursor + length).ok_or_else(|| corrupt("ROMDIR EXTINFO length exceeds module"))?;
        let mut info = ExtInfo::default();
        let mut q = 0;
        while q < records.len() {
            let header = records.get(q..q + 4).ok_or_else(|| corrupt("truncated EXTINFO header"))?;
            let size = header[2] as usize;
            let payload = records.get(q + 4..q + 4 + size).ok_or_else(|| corrupt("truncated EXTINFO payload"))?;
            match header[3] {
                1 => {
                    if size != 4 { return Err(corrupt("EXTINFO date is not four bytes")) }
                    let bcd = |b: u8| ((b & 15) < 10 && b >> 4 < 10).then_some((b >> 4) * 10 + (b & 15));
                    let parsed = (|| date(u16::from(bcd(payload[3])?) * 100 + u16::from(bcd(payload[2])?), bcd(payload[1])?, bcd(payload[0])?))();
                    info.date = Some(parsed.ok_or_else(|| corrupt("invalid EXTINFO BCD date"))?);
                }
                2 => info.version = Some(header.u16(0)),
                3 => info.comment = Some(String::from_utf8_lossy(payload).trim_end_matches('\0').into()),
                _ => {}, // Unknown and 0x7F records still consume their payload.
            }
            q += 4 + size;
        }
        let evidence = ext.as_ref().filter(|_| length != 0).map(|e| vec![file_span("EXTINFO", e.1 + cursor, length)]).unwrap_or_default();
        result.entry(name).or_insert((info, evidence));
        cursor += length;
        p += 16;
    }
    Ok(result)
}

impl RomSet {
    /// Compares ROMDIR modules, EXTINFO, OSDSYS loaders and located visual/audio tables.
    /// Order comes from ROMVER's date (`version_evolution.md` §2.1), never file names.
    /// Missing optional modules/unsupported locators produce per-dump diagnostics.
    pub fn from_ps2(inputs: &[(&str, RomDir)]) -> Result<Self> {
        let mut members: Vec<_> = inputs.iter().map(|(name, rom)| {
            let version = rom.module("ROMVER").map(|d| d.cstr(0, 14)).unwrap_or_default();
            let build = version.get(6..14).and_then(ascii_date);
            (DumpIdentity { name: (*name).into(), version, date: build, kernel_generation: None, osdsys: None, diagnostics: Vec::new() }, rom)
        }).collect();
        members.sort_by_key(|m| date_key(m.0.date));
        let mut set = Self { family: Family::Ps2, dumps: members.iter().map(|m| m.0.clone()).collect(), rows: Vec::new(), changelog: Changelog::default() };
        let info: Vec<_> = members.iter().map(|m| metadata(m.1)).collect::<Result<_>>()?;
        let names: BTreeSet<_> = members.iter().flat_map(|m| m.1.names().cloned()).collect();
        set.add_row("whole dump", RowKind::Dump, &members.iter().map(|m| Some(Sample::new(m.1.data(), file_span("ROM", 0, m.1.data().len())))).collect::<Vec<_>>());
        for name in names {
            let samples: Vec<_> = members.iter().enumerate().map(|(i, m)| {
                let (_, offset, size) = m.1.entries().into_iter().find(|e| e.0 == name)?;
                let mut sample = Sample::new(m.1.module(&name).ok()?, file_span(&name, offset, size));
                if let Some((fields, evidence)) = info[i].get(&name) { sample.extinfo = fields.clone(); sample.metadata = evidence.clone(); }
                Some(sample)
            }).collect();
            set.add_row(&name, RowKind::Module, &samples);
        }
        let mut derived = Vec::new();
        let (mut images, mut effective) = (Vec::new(), Vec::new());
        for (i, (_, rom)) in members.iter().enumerate() {
            let mut tables = BTreeMap::new();
            match rom.module("OSDSYS").and_then(|m| ProgramImage::from_module(m, 0x200000)) {
                Ok(img) => {
                    let module = rom.module("OSDSYS")?;
                    let offset = rom.entries().iter().find(|e| e.0 == "OSDSYS").map(|e| e.1).unwrap_or(0);
                    let (patches, complete) = loader_patches(module, offset, &img);
                    let mut changed = img.data().to_vec();
                    for patch in &patches {
                        if let Some(o) = img.at(patch.target) { changed[o..o + 4].copy_from_slice(&patch.new.to_le_bytes()); }
                    }
                    images.push(Some(Sample::new(img.data(), image_span("OSDSYS", &img, 0, img.data().len()))));
                    effective.push(complete.then(|| Sample::new(&changed, image_span("OSDSYS", &img, 0, changed.len()))));
                    set.dumps[i].osdsys = Some(OsdComparison { image_cluster: 0, effective_cluster: None, patches, complete });
                    if !complete { set.dumps[i].diagnostics.push("OSDSYS: ExecPS2 wrapper is not a recognised straight-line constant-store routine".into()); }
                    match opening_tables(rom, &img) {
                        Ok(found) => tables.extend(found),
                        Err(e) => set.dumps[i].diagnostics.push(format!("opening: {e}")),
                    }
                    match boot_bank(rom, &img) {
                        Ok(found) => { tables.insert("boot sound bank".into(), found); },
                        Err(e) => set.dumps[i].diagnostics.push(format!("boot sound bank: {e}")),
                    }
                }
                Err(e) => { images.push(None); effective.push(None); set.dumps[i].diagnostics.push(format!("OSDSYS: {e}")); }
            }
            match ribbon_tables(rom) {
                Ok(found) => tables.extend(found),
                Err(e) => set.dumps[i].diagnostics.push(format!("PS2LOGO: {e}")),
            }
            derived.push(tables);
        }
        set.add_row("OSDSYS (unpacked)", RowKind::Image, &images);
        set.add_row("OSDSYS (effective)", RowKind::Image, &effective);
        for i in 0..set.dumps.len() {
            let raw = set.row("OSDSYS (unpacked)").and_then(|r| r.cells[i].as_ref()).map(|c| c.cluster);
            let applied = set.row("OSDSYS (effective)").and_then(|r| r.cells[i].as_ref()).map(|c| c.cluster);
            if let (Some(osd), Some(cluster)) = (&mut set.dumps[i].osdsys, raw) { osd.image_cluster = cluster; osd.effective_cluster = applied; }
        }
        let tables: BTreeSet<_> = derived.iter().flat_map(|m| m.keys().cloned()).collect();
        for name in tables { set.add_row(&name, RowKind::Table, &derived.iter().map(|m| m.get(&name).cloned()).collect::<Vec<_>>()); }
        for i in 1..set.dumps.len() {
            if let (Some(a), Some(b)) = (&set.dumps[i - 1].osdsys, &set.dumps[i].osdsys) {
                let stores = |o: &OsdComparison| o.patches.iter().map(|p| (p.target, p.new)).collect::<Vec<_>>();
                if a.complete && b.complete && a.image_cluster == b.image_cluster && stores(a) != stores(b) {
                    set.changelog.entries.push(Change { before: i - 1, after: i, subject: "OSDSYS loader".into(), kind: ChangeKind::Loader,
                        caption: format!("identical unpacked OSDSYS; loader stores {} → {} words (version_evolution.md §2.7)", a.patches.len(), b.patches.len()),
                        changed_bytes: 0, ranges: Vec::new(), evidence: a.patches.iter().chain(&b.patches).map(|p| p.store.clone()).collect() });
                }
            }
        }
        set.changelog.entries.sort_by_key(|c| c.after);
        Ok(set)
    }

    /// Compares PS1 byte windows from `ps1_version_matrix.md` §2.2, excluding the header
    /// from bootstrap identities. POPS padding remains a real difference, never normalised.
    /// Block A can overlap older J shell tails (§2.1); the row does not presume it is gaiji.
    pub fn from_ps1(inputs: &[(&str, Ps1Bios)]) -> Result<Self> {
        let mut members: Vec<_> = inputs.iter().map(|(name, bios)| (DumpIdentity { name: (*name).into(), version: bios.identity().version.clone(),
            date: ps1_date(bios), kernel_generation: bios.identity().kernel_generation, osdsys: None, diagnostics: Vec::new() }, bios)).collect();
        members.sort_by_key(|m| date_key(m.0.date));
        let mut set = Self { family: Family::Ps1, dumps: members.iter().map(|m| m.0.clone()).collect(), rows: Vec::new(), changelog: Changelog::default() };
        for (name, kind, windows) in [
            ("whole dump", RowKind::Dump, vec![(0, 0x80000)]),
            ("bootstrap + kernel", RowKind::Aggregate, vec![(0, 0x100), (0x180, 0x18000)]),
            ("header", RowKind::Region, vec![(0x100, 0x180)]),
            ("bootstrap", RowKind::Region, vec![(0, 0x100), (0x180, 0x10000)]),
            ("kernel", RowKind::Region, vec![(0x10000, 0x18000)]),
            ("shell", RowKind::Region, vec![(0x18000, 0x64000)]),
            ("block A window", RowKind::Aggregate, vec![(0x64000, 0x66000)]),
            ("block A", RowKind::Region, vec![(0x64000, 0x65CB7)]),
            ("block A tail table", RowKind::Region, vec![(0x65CB7, 0x66000)]),
            ("font", RowKind::Region, vec![(0x66000, 0x7FE70)]),
            ("glyph tail", RowKind::Region, vec![(0x7FE70, 0x7FF30)]),
            ("version tail", RowKind::Region, vec![(0x7FF30, 0x80000)]),
        ] {
            let samples: Vec<_> = members.iter().map(|m| {
                let mut sample = Sample { bytes: Vec::new(), spans: Vec::new(), extinfo: ExtInfo::default(), metadata: Vec::new(), mapped: true };
                for &(start, end) in &windows {
                    sample.bytes.extend_from_slice(m.1.data().get(start..end).ok_or_else(|| corrupt("truncated PS1 comparison region"))?);
                    sample.spans.push(file_span(name, start, end - start));
                }
                Ok(Some(sample))
            }).collect::<Result<_>>()?;
            set.add_row(name, kind, &samples);
        }
        set.changelog.entries.sort_by_key(|c| c.after);
        Ok(set)
    }

    /// Finds a comparison row by its exact name.
    pub fn row(&self, name: &str) -> Option<&IdentityRow> { self.rows.iter().find(|r| r.name == name) }

    fn add_row(&mut self, name: &str, kind: RowKind, samples: &[Option<Sample>]) {
        let mut representatives: Vec<&[u8]> = Vec::new();
        let cells: Vec<_> = samples.iter().map(|s| s.as_ref().map(|s| {
            let cluster = representatives.iter().position(|&b| b == s.bytes).unwrap_or_else(|| { representatives.push(&s.bytes); representatives.len() - 1 });
            IdentityCell { cluster, size: s.bytes.len(), extinfo: s.extinfo.clone(), evidence: s.spans.iter().chain(&s.metadata).cloned().collect() }
        })).collect();
        if !matches!(kind, RowKind::Dump | RowKind::Aggregate) {
            for i in 1..samples.len() {
                let (a, b) = (samples[i - 1].as_ref(), samples[i].as_ref());
                let change = match (a, b) {
                    (None, Some(_)) => Some((ChangeKind::Added, "added".into())),
                    (Some(_), None) => Some((ChangeKind::Removed, "removed".into())),
                    (Some(a), Some(b)) if a.bytes.len() != b.bytes.len() => Some((ChangeKind::Resized, format!("resized {:#x} → {:#x}", a.bytes.len(), b.bytes.len()))),
                    (Some(a), Some(b)) if a.bytes != b.bytes => Some((ChangeKind::Changed, "bytes changed".into())),
                    _ => None,
                };
                if let Some((change, description)) = change {
                    let (count, ranges) = differences(a, b, kind == RowKind::Region);
                    let hints: BTreeSet<_> = ranges.iter().filter_map(|r| r.hint.as_deref()).collect();
                    let hint = if hints.is_empty() { String::new() } else { format!("; {}", hints.into_iter().collect::<Vec<_>>().join("; ")) };
                    self.changelog.entries.push(Change { before: i - 1, after: i, subject: name.into(), kind: change,
                        caption: format!("{name}: {description} ({count} differing bytes){hint}"), changed_bytes: count, ranges,
                        evidence: a.into_iter().chain(b).flat_map(|s| s.spans.clone()).collect() });
                }
                if let (Some(a), Some(b)) = (a, b) {
                    if a.extinfo != b.extinfo {
                        self.changelog.entries.push(Change { before: i - 1, after: i, subject: name.into(), kind: ChangeKind::Metadata,
                            caption: format!("{name}: EXTINFO {} → {}", ext_summary(&a.extinfo), ext_summary(&b.extinfo)), changed_bytes: 0, ranges: Vec::new(),
                            evidence: a.metadata.iter().chain(&b.metadata).cloned().collect() });
                    }
                }
            }
        }
        self.rows.push(IdentityRow { name: name.into(), kind, cells });
    }
}

fn differences(a: Option<&Sample>, b: Option<&Sample>, detailed: bool) -> (usize, Vec<ByteRange>) {
    let (left, right) = (a.map(|s| s.bytes.as_slice()).unwrap_or(&[]), b.map(|s| s.bytes.as_slice()).unwrap_or(&[]));
    let mut ranges = Vec::new();
    let boundaries: BTreeSet<_> = a.into_iter().chain(b).flat_map(|s| s.spans.iter().scan(0usize, |end, e| { *end += e.length; Some(*end) })).collect();
    let (mut count, mut p) = (0, 0);
    while p < left.len().max(right.len()) {
        if left.get(p) == right.get(p) { p += 1; continue }
        let start = p;
        while p < left.len().max(right.len()) && left.get(p) != right.get(p) {
            count += 1; p += 1;
            if detailed && boundaries.contains(&p) { break }
        }
        if detailed || ranges.is_empty() {
            let (before, after) = (a.and_then(|s| s.at(start, p - start)), b.and_then(|s| s.at(start, p - start)));
            let hint = detailed.then(|| after.as_ref().or(before.as_ref()).and_then(reference_hint)).flatten();
            ranges.push(ByteRange { start, end: p, before, after, hint });
        }
    }
    (count, ranges)
}

fn ext_summary(info: &ExtInfo) -> String {
    let version = info.version.map(|v| format!("{}.{:02}", v >> 8, v & 255)).unwrap_or_else(|| "unknown".into());
    let date = info.date.map(|d| format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)).unwrap_or_else(|| "undated".into());
    format!("v{version} {date} {:?}", info.comment)
}

// These are reference-site annotations, not function locators. They never assign identity.
// The actual differing file spans accompany each hint (`ps1_version_matrix.md` §3.2–3.3).
fn reference_hint(e: &Evidence) -> Option<String> {
    if e.module != "bootstrap" && e.module != "kernel" { return None }
    let sites = [(0x00000, 0x00050, "reset ROM/RAM timing and early delay"), (0x045BC, 0x045D4, "cache initialisation"),
        (0x06680, 0x06700, "SetMem RAM_SIZE"), (0x068DC, 0x06950, "kernel default configuration"),
        (0x0D4F0, 0x0D570, "GetSystemInfo"), (0x0D570, 0x0D850, "pre-exec ReadTOC/GetID"),
        (0x0D850, 0x0D880, "pre-exec RAM wipe"), (0x0E1C8, 0x10000, "ROM monitor removal / padding"),
        (0x16F50, 0x18000, "ROM monitor command tables / padding")];
    sites.into_iter().find(|&(start, end, _)| e.offset < end && e.offset + e.length > start)
        .map(|(_, _, name)| format!("reference-site hint: {name} (ps1_version_matrix.md §§3.2–3.3)"))
}

fn opening_tables(rom: &RomDir, osd: &ProgramImage) -> Result<BTreeMap<String, Sample>> {
    let mut img = osd.clone();
    let mut module = "OSDSYS".to_string();
    if !OsdLayout::has_asset_table(&img) {
        if let Ok(descriptor) = rom.module("OSOPEN") {
            let text = std::str::from_utf8(descriptor).map_err(|_| corrupt("invalid OSOPEN text"))?;
            let mut lines = text.trim_end_matches('\0').lines();
            let version = lines.next().ok_or_else(|| corrupt("missing OSOPEN version"))?;
            let name = lines.next().ok_or_else(|| corrupt("missing OSOPEN module"))?;
            let base = lines.next().and_then(|s| u32::from_str_radix(s, 16).ok()).ok_or_else(|| corrupt("invalid OSOPEN base"))?;
            if version.parse::<u32>().is_err() || name.is_empty() || name.len() > 10 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { return Err(corrupt("invalid OSOPEN descriptor")) }
            module = name.into();
            img = ProgramImage::from_module(rom.module(name)?, base as usize)?;
        }
    }
    let lay = OsdLayout::discover(&img)?;
    let mut tables = BTreeMap::new();
    // `version_evolution.md` §2.2: compare all stored components, including vector w.
    for (name, offset, length) in [("opening tower grid", lay.tower_positions, 126 * 16), ("opening slots", lay.slot_table, 126 * 8),
        ("opening growth fill", lay.fill_table, 14 * 4), ("opening growth size", lay.size_table, 14 * 4),
        ("opening orbs", lay.orb_colours, 4 * 16), ("opening cubes", lay.cube_positions, 5 * 16), ("opening prisms", lay.prism_positions, 5 * 16)] {
        let bytes = img.data().get(offset..offset + length).ok_or_else(|| corrupt("located opening table exceeds image"))?;
        tables.insert(name.into(), Sample::new(bytes, image_span(&module, &img, offset, length)));
    }
    // Archive membership is distinct from the opening's texture descriptors (§2.3).
    if let Ok(tex) = rom.module("TEXIMAGE") {
        let archive = RomDir::new(tex.to_vec())?;
        let mut names: Vec<_> = archive.names().filter(|n| n.starts_with("TEX")).cloned().collect();
        names.sort();
        let bytes = names.join("\0").into_bytes();
        let span = Evidence { module: "TEXIMAGE texture names".into(), space: Space::Decoded, offset: 0, length: 0, address: None };
        // Spans point to actual directory records, rather than the canonical sorted list.
        let mut sample = Sample::new(&bytes, span);
        sample.mapped = false;
        let base = archive.data().windows(10).position(|w| w == b"RESET\0\0\0\0\0").ok_or_else(|| corrupt("missing TEXIMAGE ROMDIR"))?;
        sample.spans.clear();
        for (i, entry) in archive.data()[base..].chunks_exact(16).take_while(|e| e[0] != 0).enumerate() {
            if entry.starts_with(b"TEX") { sample.spans.push(Evidence { module: "TEXIMAGE ROMDIR".into(), space: Space::Decoded, offset: base + i * 16, length: 10, address: None }); }
        }
        tables.insert("texture list".into(), sample);
    } else {
        // Inline entries have no names; report their count/formats/dimensions honestly.
        let mut bytes = Vec::new();
        for i in 0..lay.texture_count {
            let o = lay.texture_table + i * lay.texture_stride;
            for field in [0x18, 0x1C, 0x20, 0x28] { bytes.extend_from_slice(&img.data()[o + field..o + field + 4]); }
        }
        let mut sample = Sample::new(&bytes, image_span(&module, &img, lay.texture_table, lay.texture_count * lay.texture_stride));
        sample.mapped = false;
        tables.insert("texture list".into(), sample);
    }
    Ok(tables)
}

fn ribbon_tables(rom: &RomDir) -> Result<BTreeMap<String, Sample>> {
    let img = ProgramImage::from_module(rom.module("PS2LOGO")?, 0x100000)?;
    let lay = LogoLayout::discover(&img)?;
    let mut tables = BTreeMap::new();
    for (name, address, length) in [("ribbon constants", lay.ribbon_delta, 24), ("ribbon multipliers", lay.ribbon_multipliers, 20)] {
        let offset = img.at(address).ok_or_else(|| corrupt("ribbon address outside image"))?;
        let bytes = img.data().get(offset..offset + length).ok_or_else(|| corrupt("truncated ribbon constants"))?;
        tables.insert(name.into(), Sample::new(bytes, image_span("PS2LOGO", &img, offset, length)));
    }
    Ok(tables)
}

// Bank comparison includes both header and ADPCM bytes, without rendering PCM.
// 1.00's matching inline bank is in OSDSYS, despite evolution §2.4's OSDSND attribution;
// see `sound.rs::embedded_boot_sounds` and `ps2_100.rs::sound_and_logo_100_load`.
fn boot_bank(rom: &RomDir, img: &ProgramImage) -> Result<Sample> {
    if let Ok(sound) = rom.module("SNDIMAGE") {
        let archive = RomDir::new(sound.to_vec())?;
        let mut bytes = Vec::new();
        let mut spans = Vec::new();
        let header = unpack(archive.module("SNDBOOTH")?, 0)?;
        let body = unpack(archive.module("SNDBOOTB")?, 0)?;
        // The archived header has two trailing alignment bytes; compare the bank's
        // declared extent with the inline header, not the archive-member padding.
        let header = header.get(..header.u32(0) as usize).filter(|h| h.len() >= 0x30).ok_or_else(|| corrupt("invalid boot bank header size"))?;
        let body = body.get(..header.u32(4) as usize).ok_or_else(|| corrupt("invalid boot bank body size"))?;
        SoundBank::new(header, body.to_vec())?;
        for (name, asset) in [("SNDBOOTH", header), ("SNDBOOTB", body)] {
            bytes.extend_from_slice(asset);
            spans.push(Evidence { module: format!("SNDIMAGE/{name}"), space: Space::Decoded, offset: 0, length: asset.len(), address: None });
        }
        return Ok(Sample { bytes, spans, extinfo: ExtInfo::default(), metadata: Vec::new(), mapped: true });
    }
    let d = img.data();
    img.find_all(b"SShd", 4).into_iter().find_map(|signature| {
        let p = signature.checked_sub(12)?;
        let (head, body) = (d.u32(p) as usize, d.u32(p + 4) as usize);
        if !(0x30..=0x10000).contains(&head) || body == 0 || body > 64 << 20 { return None }
        let header = d.get(p..p.checked_add(head)?)?;
        let start = p.checked_add((head + 63) & !63)?;
        let samples = d.get(start..start.checked_add(body)?)?;
        let bank = SoundBank::new(header, samples.to_vec()).ok()?;
        if bank.programs.is_empty() { return None }
        let mut bytes = header.to_vec();
        bytes.extend_from_slice(samples);
        Some(Sample { bytes, spans: vec![image_span("OSDSYS", img, p, head), image_span("OSDSYS", img, start, body)], extinfo: ExtInfo::default(), metadata: Vec::new(), mapped: true })
    }).ok_or_else(|| corrupt("no bounded inline boot sound bank"))
}

fn instruction(word: u32, address: usize) -> String {
    let (op, rs, rt) = (word >> 26, (word >> 21) & 31, (word >> 16) & 31);
    let branch = address.wrapping_add(4).wrapping_add_signed((word as i16 as isize) * 4);
    match (op, word) {
        (_, 0) => "nop".into(),
        (4, _) if rs == 0 && rt == 0 => format!("b {branch:#x}"),
        (4 | 5 | 20 | 21, _) => format!("{} r{rs},r{rt},{branch:#x}", match op { 4 => "beq", 5 => "bne", 20 => "beql", _ => "bnel" }),
        (6 | 7 | 22 | 23, _) => format!("{} r{rs},{branch:#x}", match op { 6 => "blez", 7 => "bgtz", 22 => "blezl", _ => "bgtzl" }),
        (1, _) => format!("{} r{rs},{branch:#x}", match rt { 0 => "bltz", 1 => "bgez", 2 => "bltzl", 3 => "bgezl", 16 => "bltzal", 17 => "bgezal", _ => "regimm" }),
        (2 | 3, _) => format!("{} {:#x}", if op == 3 { "jal" } else { "j" }, ((address + 4) & 0xF0000000) | ((word as usize & 0x03FFFFFF) << 2)),
        (43, _) => format!("sw r{rt},{}(r{rs})", word as i16),
        _ => format!("word {word:#010x}"),
    }
}

// Locate the syscall-7 wrapper in ELF segments, validate a JAL to its entry, then evaluate
// only its straight-line constants and stores. Unknown control flow yields incomplete,
// never a guessed zero-patch result (`version_evolution.md` §2.7 / osdsys_hooks.md §6.1).
fn loader_patches(module: &[u8], file_offset: usize, img: &ProgramImage) -> (Vec<PatchedWord>, bool) {
    if !module.starts_with(b"\x7fELF") { return (Vec::new(), false) }
    let (ph, stride, count) = (module.u32(0x1C) as usize, module.u16(0x2A) as usize, module.u16(0x2C) as usize);
    if stride < 32 || ph.checked_add(stride * count).is_none_or(|end| end > module.len()) { return (Vec::new(), false) }
    for n in 0..count {
        let header = ph + n * stride;
        if module.u32(header) != 1 { continue }
        let (start, base, length) = (module.u32(header + 4) as usize, module.u32(header + 8) as usize, module.u32(header + 16) as usize);
        let Some(end) = start.checked_add(length).filter(|&e| e <= module.len()) else { continue };
        // Plain ELF's loaded image already contains the code; no compressed loader exists.
        if base == img.base() && module.get(start..end) == img.data().get(..length) { return (Vec::new(), true) }
        for syscall in (start..end.saturating_sub(12)).step_by(4) {
            if module.u32(syscall) != 0x24030007 || module.u32(syscall + 4) != 0x0000000C || module.u32(syscall + 8) != 0x03E00008 { continue }
            let entry = (start..syscall).step_by(4).rev().find(|&p| module.u32(p) == 0x03E00008).map(|p| p + 8).unwrap_or(start);
            if entry > syscall || syscall - entry > 1024 { continue }
            let address = base + entry - start;
            let call = 0x0C000000 | ((address as u32 >> 2) & 0x03FFFFFF);
            if !(start..entry).step_by(4).any(|p| module.u32(p) == call) { continue }
            let mut registers = [None; 32];
            registers[0] = Some(0u32);
            let mut patches: Vec<PatchedWord> = Vec::new();
            let mut complete = true;
            for p in (entry..syscall).step_by(4) {
                let word = module.u32(p);
                let (op, rs, rt, imm) = (word >> 26, ((word >> 21) & 31) as usize, ((word >> 16) & 31) as usize, word as u16);
                match op {
                    15 if rs == 0 => registers[rt] = Some(u32::from(imm) << 16),
                    9 => registers[rt] = registers[rs].map(|v| v.wrapping_add(imm as i16 as u32)),
                    13 => registers[rt] = registers[rs].map(|v| v | u32::from(imm)),
                    43 => {
                        let values = registers[rs].zip(registers[rt]);
                        if let Some((target, value)) = values {
                            let target = target.wrapping_add(imm as i16 as u32) as usize;
                            if let Some(offset) = img.at(target).filter(|&o| o.is_multiple_of(4) && o + 4 <= img.data().len()) {
                                let old = patches.iter().rev().find(|p| p.target == target).map(|p| p.new).unwrap_or_else(|| img.data().u32(offset));
                                patches.push(PatchedWord { target, old, new: value, old_instruction: instruction(old, target), new_instruction: instruction(value, target),
                                    store: Evidence { module: "OSDSYS loader".into(), space: Space::File, offset: file_offset + p, length: 4, address: Some(base + p - start) } });
                            } else { complete = false; }
                        } else { complete = false; }
                    }
                    0 if word == 0 => {},
                    _ => { complete = false; break }
                }
                registers[0] = Some(0);
            }
            return (patches, complete);
        }
    }
    (Vec::new(), false)
}

/// Spreadsheet-style cluster label (`A`…`Z`, `AA`…), local to a single matrix row.
pub fn cluster_label(mut cluster: usize) -> String {
    let mut chars = Vec::new();
    loop {
        chars.push((b'A' + (cluster % 26) as u8) as char);
        if cluster < 26 { break }
        cluster = cluster / 26 - 1;
    }
    chars.into_iter().rev().collect()
}
