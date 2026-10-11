//! Validated native sound extraction from the caller's ISO 9660 disc files.
//!
//! VAB/VAG and SShd pairing: notes/research/sound_inventory.md §§"VAB and
//! standalone VAG", "SShd boundaries", appendix D. Archives are searched without
//! decompressing or guessing undocumented packers. PS1 SEQ follows
//! [PSX-SPX](https://psx-spx.consoledev.net/cdromfileformats/#cdrom-file-audio-sequences-seqsep-sony).
//! HD/BD is inferred, synthetically tested, and retains native program chunks;
//! audition exposes Vagi samples without claiming CSL program emulation.
//! No audio, disc-specific offsets, or file hashes are supplied by the crate.

use crate::bench::SoundBench;
use crate::bytes::Bytes;
use crate::banks::{fingerprint, header_extent, sequence_extent};
use crate::probe::{DiscFileEntry, DiscFiles};
use crate::sound::{decode_adpcm, EventKind, Program, ProgramKind, SoundBank, SoundSequence, Tone};
use crate::{Error, Format, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const MARKER: &str = "# PSX Boot Inspection Tool soundfonts v1";
const LIMIT: usize = 64 << 20;

/// Native sound container; body companions belong to the same asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssetKind {
    /// PS1 VAB, combined even when read from separate VH/VB files.
    Vab,
    /// PS1 MIDI-like sequence, with no guessed bank binding.
    Seq,
    /// Standalone mono SPU ADPCM sample.
    Vag,
    /// OSD SShd header and headerless body.
    SShd,
    /// OSD SSsq sequence, with no guessed bank binding.
    SSsq,
    /// Inferred PS2 CSL IECS header and BD body.
    Hd,
    /// RIFF WAVE integer PCM audio.
    Wave,
}
impl AssetKind {
    /// All reported formats, in stable display order.
    pub const ALL: [Self; 7] = [
        Self::Vab,
        Self::Seq,
        Self::Vag,
        Self::SShd,
        Self::SSsq,
        Self::Hd,
        Self::Wave,
    ];
    /// Format name used in the index and CLI.
    pub fn name(self) -> &'static str {
        match self {
            Self::Vab => "VAB",
            Self::Seq => "SEQ",
            Self::Vag => "VAG",
            Self::SShd => "SShd",
            Self::SSsq => "SSsq",
            Self::Hd => "HD",
            Self::Wave => "WAVE",
        }
    }
    fn filename(self, number: usize) -> String {
        let (prefix, extension) = match self {
            Self::Vab => ("bank", "vab"),
            Self::Seq => ("seq", "seq"),
            Self::Vag => ("sample", "vag"),
            Self::SShd => ("bank", "sshd"),
            Self::SSsq => ("seq", "sssq"),
            Self::Hd => ("bank", "hd"),
            Self::Wave => ("audio", "wav"),
        };
        format!("{prefix}-{number:04}.{extension}")
    }
    fn companion(self, file: &str) -> String {
        match self {
            Self::SShd => file.replace(".sshd", ".ssbd"),
            Self::Hd => file.replace(".hd", ".bd"),
            _ => String::new(),
        }
    }
}

/// Coordinates of one validated occurrence, relative to files in the image.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AssetSource {
    /// ISO path including its version suffix.
    pub file: String,
    /// Container start, or header start for split banks.
    pub offset: usize,
    /// Parsed bytes in this file, excluding alignment.
    pub size: usize,
    /// Body source when separate from the container/header.
    pub body_file: Option<String>,
    /// Body offset within that ISO file.
    pub body_offset: Option<usize>,
    /// Body extent retained in the output.
    pub body_size: usize,
}

/// Parsed counts and duration; no descriptive instrument names are invented.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct AssetSummary {
    /// Populated native programs, zero for sample-only assets.
    pub programs: usize,
    /// Populated native tone/sample parameter records.
    pub tones: usize,
    /// Validated sample streams.
    pub samples: usize,
    /// Sum of sample seconds, or sequence traversal seconds.
    pub length_s: f64,
}

/// One byte-distinct validated asset, with all identical occurrences retained.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExtractedAsset {
    /// Native container format.
    pub kind: AssetKind,
    /// Every occurrence within this disc.
    pub sources: Vec<AssetSource>,
    /// Parsed counts and duration.
    pub summary: AssetSummary,
    /// Complete native bytes (combined VH/VB for VAB).
    pub data: Vec<u8>,
    /// Native headerless body for SShd or HD.
    pub body: Vec<u8>,
}

/// Signature hit rejected by a bounded parser.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RejectedHit {
    /// Expected container format.
    pub kind: AssetKind,
    /// ISO source path.
    pub file: String,
    /// Signature offset (SShd/SSsq are twelve bytes into their headers).
    pub offset: usize,
    /// Validation failure; never silently repaired.
    pub reason: String,
}

/// Occurrence counts; validated includes duplicates, unique counts written assets.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct KindCounts {
    /// Matching signatures encountered.
    pub found: usize,
    /// Completely parsed and bounded candidates.
    pub validated: usize,
    /// Failed candidates.
    pub rejected: usize,
    /// Byte-distinct assets retained.
    pub unique: usize,
}

/// Scan result owning only bytes read from this caller's disc.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Extraction {
    /// Input image or cue path.
    pub image: PathBuf,
    /// Disc family inferred from SYSTEM.CNF, executable or native sound format.
    pub console: String,
    /// Image filename title, falling back to the ISO volume ID.
    pub title: String,
    /// ISO primary volume ID.
    pub volume_id: String,
    /// Boot title ID, empty when this disc provides none.
    pub title_id: String,
    /// Byte-distinct validated native assets.
    pub assets: Vec<ExtractedAsset>,
    /// Every rejected signature and its reason.
    pub rejected: Vec<RejectedHit>,
    /// Read failures for individual ISO files.
    pub diagnostics: Vec<String>,
    counts: HashMap<AssetKind, KindCounts>,
}

/// Written native set ready for SoundLibrary::scan_with_soundfonts.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExtractedSet {
    /// Replaced or newly created set folder.
    pub folder: PathBuf,
    /// Native asset paths, including body companions.
    pub files: Vec<PathBuf>,
    /// Number of byte-distinct assets.
    pub assets: usize,
}

/// Stateless scanner; opens caller-owned disc images through DiscFiles.
pub struct Extractor;
impl Extractor {
    /// Walks every ISO file and searches its contents, including packed archives.
    /// CD-DA tracks are outside the ISO tree and are never treated as instruments.
    pub fn scan(image_path: impl AsRef<Path>) -> Result<Extraction> {
        let image = image_path.as_ref();
        let mut disc = DiscFiles::open(image)?;
        let entries: Vec<_> = disc.list().iter().filter(|e| !e.is_directory).cloned().collect();
        let volume_id = disc.volume_id().to_string();
        let mut title = image
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(at) = title.to_ascii_lowercase().find(" (track ") {
            title.truncate(at);
        }
        if title.is_empty() {
            title.clone_from(&volume_id);
        }
        let mut extraction = Extraction {
            image: image.to_path_buf(),
            console: "Disc".into(),
            title,
            volume_id,
            title_id: String::new(),
            assets: Vec::new(),
            rejected: Vec::new(),
            diagnostics: Vec::new(),
            counts: HashMap::new(),
        };
        if let Ok(cnf) = disc.read("SYSTEM.CNF") {
            for line in String::from_utf8_lossy(&cnf).lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                if key.eq_ignore_ascii_case("BOOT2") || key.eq_ignore_ascii_case("BOOT") {
                    extraction.console = if key.eq_ignore_ascii_case("BOOT2") {
                        "PS2"
                    } else {
                        "PS1"
                    }
                    .into();
                    let name = value.trim().rsplit(['/', '\\', ':']).next().unwrap_or("");
                    let id = name.split(';').next().unwrap_or("");
                    if !id.eq_ignore_ascii_case("PSX.EXE") {
                        extraction.title_id = id.to_string();
                    }
                    break;
                }
            }
        }
        if extraction.console == "Disc" && entries.iter().any(|e| e.path.eq_ignore_ascii_case("/PSX.EXE;1")) {
            extraction.console = "PS1".into();
        }
        let mut identities: HashMap<String, Vec<usize>> = HashMap::new();
        for entry in &entries {
            let data = match disc.read(&entry.path) {
                Ok(data) if data.len() == entry.size => data,
                Ok(_) => {
                    extraction
                        .diagnostics
                        .push(format!("{}: truncated ISO extent", entry.path));
                    continue;
                }
                Err(e) => {
                    extraction.diagnostics.push(format!("{}: {e}", entry.path));
                    continue;
                }
            };
            for (offset, sig) in data.windows(4).enumerate() {
                let kind = match sig {
                    b"pBAV" => AssetKind::Vab,
                    b"pQES" => AssetKind::Seq,
                    b"VAGp" => AssetKind::Vag,
                    b"SShd" => AssetKind::SShd,
                    b"SSsq" => AssetKind::SSsq,
                    b"RIFF" => AssetKind::Wave,
                    b"IECS"
                        if !matches!(
                            data.get(offset + 4..offset + 8),
                            Some(b"daeH" | b"igaV" | b"lpmS" | b"tesS" | b"gorP")
                        ) =>
                    {
                        AssetKind::Hd
                    }
                    _ => continue,
                };
                extraction.counts.entry(kind).or_default().found += 1;
                match candidate(kind, &data, offset, entry, &entries, &mut disc) {
                    Ok(asset) => {
                        extraction.counts.entry(kind).or_default().validated += 1;
                        let key = fingerprint(kind.name(), &[&asset.data, &asset.body]);
                        let identical = identities.get(&key).and_then(|indexes| {
                            indexes.iter().copied().find(|&i| {
                                extraction.assets[i].data == asset.data && extraction.assets[i].body == asset.body
                            })
                        });
                        if let Some(i) = identical {
                            extraction.assets[i].sources.extend(asset.sources);
                        } else {
                            extraction.counts.entry(kind).or_default().unique += 1;
                            identities.entry(key).or_default().push(extraction.assets.len());
                            extraction.assets.push(asset);
                        }
                    }
                    Err(e) => {
                        extraction.counts.entry(kind).or_default().rejected += 1;
                        extraction.rejected.push(RejectedHit {
                            kind,
                            file: entry.path.clone(),
                            offset,
                            reason: e.to_string(),
                        });
                    }
                }
            }
        }
        if extraction.console == "Disc" {
            extraction.console = if extraction
                .assets
                .iter()
                .any(|a| matches!(a.kind, AssetKind::SShd | AssetKind::Hd | AssetKind::SSsq))
            {
                "PS2"
            } else if extraction.assets.iter().any(|a| a.kind == AssetKind::Vab) {
                "PS1"
            } else {
                "Disc"
            }
            .into();
        }
        Ok(extraction)
    }
}

impl Extraction {
    /// Candidate and distinct-content counts for one format.
    pub fn counts(&self, kind: AssetKind) -> KindCounts {
        self.counts.get(&kind).cloned().unwrap_or_default()
    }

    /// Writes native files and provenance; replaces an owned set on rerun.
    /// Existing unmarked folders and symlink destinations are refused.
    pub fn write(&self, soundfonts_dir: impl AsRef<Path>) -> Result<ExtractedSet> {
        let root = soundfonts_dir.as_ref();
        std::fs::create_dir_all(root)?;
        let name = format!(
            "{} - {} [{}]",
            safe_name(&self.console),
            safe_name(&self.title),
            safe_name(if self.title_id.is_empty() {
                "unknown"
            } else {
                &self.title_id
            })
        );
        let folder = root.join(name);
        if let Ok(meta) = std::fs::symlink_metadata(&folder) {
            if !meta.is_dir()
                || meta.file_type().is_symlink()
                || !std::fs::read_to_string(folder.join("index.tsv")).is_ok_and(|s| s.lines().next() == Some(MARKER))
            {
                return Err(bad("destination is not an owned extracted set"));
            }
        }
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let token = format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let staging = root.join(format!(".psx-extract-{token}"));
        let backup = root.join(format!(".psx-backup-{token}"));
        std::fs::create_dir(&staging)?;
        let (names, count) = match self.write_files(&staging) {
            Ok(result) => result,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(e);
            }
        };
        let replaced = folder.exists();
        if replaced {
            if let Err(e) = std::fs::rename(&folder, &backup) {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(e.into());
            }
        }
        if let Err(e) = std::fs::rename(&staging, &folder) {
            if replaced {
                let _ = std::fs::rename(&backup, &folder);
            }
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e.into());
        }
        if replaced {
            std::fs::remove_dir_all(&backup)?;
        }
        Ok(ExtractedSet {
            folder: folder.clone(),
            files: names.into_iter().map(|n| folder.join(n)).collect(),
            assets: count,
        })
    }

    fn write_files(&self, folder: &Path) -> Result<(Vec<String>, usize)> {
        let mut index = format!(
            "{MARKER}\n# title\t{}\n# console\t{}\n# volume\t{}\n# title_id\t{}\n# image\t{}\n",
            field(&self.title),
            field(&self.console),
            field(&self.volume_id),
            field(&self.title_id),
            field(&self.image.to_string_lossy())
        );
        index.push_str("kind\tfile\tsource\toffset\tsize\tprograms\ttones\tsamples\tlength_s\tbody_file\tbody_source\tbody_offset\tbody_size\n");
        let mut counters = HashMap::new();
        let mut names = Vec::new();
        for asset in &self.assets {
            let count = counters.entry(asset.kind).or_insert(0);
            *count += 1;
            let name = asset.kind.filename(*count);
            std::fs::write(folder.join(&name), &asset.data)?;
            names.push(name.clone());
            let body_name = asset.kind.companion(&name);
            if !body_name.is_empty() {
                std::fs::write(folder.join(&body_name), &asset.body)?;
                names.push(body_name.clone());
            }
            for source in &asset.sources {
                index.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.9}\t{}\t{}\t{}\t{}\n",
                    asset.kind.name(),
                    name,
                    field(&source.file),
                    source.offset,
                    source.size,
                    asset.summary.programs,
                    asset.summary.tones,
                    asset.summary.samples,
                    asset.summary.length_s,
                    body_name,
                    field(source.body_file.as_deref().unwrap_or("")),
                    source.body_offset.map(|n| n.to_string()).unwrap_or_default(),
                    source.body_size
                ));
            }
        }
        std::fs::write(folder.join("index.tsv"), index)?;
        let readme = format!("{} — {}\nDisc image: {}\nVolume: {}\nTitle ID: {}\nDate: {} UTC\n\nextracted locally from your own disc by the PSX Boot Inspection Tool; this folder is yours\n\nNative files retain the source bytes. index.tsv records offsets relative to ISO files,\nnot raw BIN sectors. Duplicate content keeps every occurrence. SEQ/SSsq files have no\nguessed bank binding. HD/BD interpretation is inferred; audition exposes Vagi samples.\n",
            field(&self.console), field(&self.title), field(&self.image.to_string_lossy()), field(&self.volume_id),
            field(&self.title_id), utc_date());
        std::fs::write(folder.join("README.txt"), readme)?;
        if !self.rejected.is_empty() || !self.diagnostics.is_empty() {
            let mut rejected = String::from("kind\tsource\toffset\treason\n");
            for hit in &self.rejected {
                rejected.push_str(&format!(
                    "{}\t{}\t{}\t{}\n",
                    hit.kind.name(),
                    field(&hit.file),
                    hit.offset,
                    field(&hit.reason)
                ));
            }
            for diagnostic in &self.diagnostics {
                rejected.push_str(&format!("read\t\t\t{}\n", field(diagnostic)));
            }
            std::fs::write(folder.join("rejected.tsv"), rejected)?;
        }
        Ok((names, self.assets.len()))
    }
}

fn candidate(
    kind: AssetKind,
    data: &[u8],
    signature: usize,
    entry: &DiscFileEntry,
    entries: &[DiscFileEntry],
    disc: &mut DiscFiles,
) -> Result<ExtractedAsset> {
    let start = if matches!(kind, AssetKind::SShd | AssetKind::SSsq) {
        signature
            .checked_sub(12)
            .ok_or_else(|| bad("signature before header prefix"))?
    } else {
        signature
    };
    let bytes = &data[start..];
    let mut source = AssetSource {
        file: entry.path.clone(),
        offset: start,
        size: 0,
        body_file: None,
        body_offset: None,
        body_size: 0,
    };
    let (native, body, summary) = match kind {
        AssetKind::Vab => {
            let (header, body_size) = vab_sizes(bytes)?;
            let header_bytes = &bytes[..header];
            let (body, file, offset) = if extension(&entry.path) == "VH" && start == 0 {
                paired_body(entry, entries, disc, &["VB"], body_size)?
            } else {
                let mut found = None;
                // [I] Archive alignment: try bounded adjacent/padded starts, each
                // requiring a complete parsed bank. [V] the caller's Tekken ANNA.ARC
                // has its VH at +0x13800 and VB at +0x16000, with 0xAB padding.
                for align in [1, 16, 64, 2048] {
                    let end = (start + header).div_ceil(align) * align - start;
                    if let Some(body) = bytes.get(end..end + body_size) {
                        let combined: Vec<_> = header_bytes.iter().chain(body).copied().collect();
                        if validate_vab(&combined).is_ok() {
                            found = Some((body.to_vec(), entry.path.clone(), start + end));
                            break;
                        }
                    }
                }
                if let Some(body) = found {
                    body
                } else {
                    match paired_body(entry, entries, disc, &["VB"], body_size) {
                        Ok(body) => body,
                        Err(_) => return Err(bad("no validated adjacent/aligned/named VAB body")),
                    }
                }
            };
            let combined: Vec<_> = header_bytes.iter().chain(&body).copied().collect();
            let summary = validate_vab(&combined)?;
            source.size = if file == entry.path && offset == start + header {
                header + body_size
            } else {
                header
            };
            source.body_file = Some(file);
            source.body_offset = Some(offset);
            source.body_size = body_size;
            (combined, Vec::new(), summary)
        }
        AssetKind::SShd | AssetKind::Hd => {
            let (header_size, body_size) = if kind == AssetKind::SShd {
                (
                    header_extent(bytes).ok_or_else(|| bad("invalid SShd tables/extent"))?,
                    bytes.u32(4) as usize,
                )
            } else {
                hd_sizes(bytes)?
            };
            if body_size == 0 || body_size > LIMIT {
                return Err(bad("invalid bank body size"));
            }
            let exts: &[&str] = if kind == AssetKind::SShd {
                &["SSBD", "BD"]
            } else {
                &["BD"]
            };
            let paired = if start == 0 {
                paired_body(entry, entries, disc, exts, body_size).ok()
            } else {
                None
            };
            let (body, file, offset) = if let Some(pair) = paired {
                pair
            } else {
                let mut found = None;
                // Inventory: inline SShd bodies are 64-byte aligned. Unpadded files
                // are accepted only if their sample references actually validate.
                for end in [header_size.div_ceil(64) * 64, header_size] {
                    if let Some(body) = bytes.get(end..end + body_size) {
                        let result = if kind == AssetKind::SShd {
                            validate_sshd(&bytes[..header_size], body)
                        } else {
                            validate_hd(&bytes[..header_size], body)
                        };
                        if result.is_ok() {
                            found = Some((body.to_vec(), entry.path.clone(), start + end));
                            break;
                        }
                    }
                }
                found.ok_or_else(|| bad("missing or invalid inline/named bank body"))?
            };
            let native = bytes[..header_size].to_vec();
            let summary = if kind == AssetKind::SShd {
                validate_sshd(&native, &body)?
            } else {
                validate_hd(&native, &body)?
            };
            source.size = header_size;
            source.body_file = Some(file);
            source.body_offset = Some(offset);
            source.body_size = body_size;
            (native, body, summary)
        }
        AssetKind::Vag => {
            let (size, rate) = vag_extent(bytes)?;
            let frames = adpcm_frames(&bytes[0x30..size])?;
            (
                bytes[..size].to_vec(),
                Vec::new(),
                AssetSummary {
                    samples: 1,
                    length_s: (frames * 28) as f64 / rate as f64,
                    ..Default::default()
                },
            )
        }
        AssetKind::Wave => {
            let wave = wave_parse(bytes)?;
            let summary = AssetSummary {
                samples: 1,
                length_s: wave.frames as f64 / wave.rate as f64,
                ..Default::default()
            };
            (bytes[..wave.size].to_vec(), Vec::new(), summary)
        }
        AssetKind::Seq => {
            let (size, seconds) = seq_extent(bytes)?;
            (
                bytes[..size].to_vec(),
                Vec::new(),
                AssetSummary {
                    length_s: seconds,
                    ..Default::default()
                },
            )
        }
        AssetKind::SSsq => {
            let size = sequence_extent(bytes).ok_or_else(|| bad("truncated/invalid SSsq events or terminator"))?;
            let seq = SoundSequence::new(&bytes[..size])?;
            if seq.volume > 127
                || seq
                    .channels
                    .iter()
                    .any(|c| c.program > 127 || c.volume > 127 || c.pan > 127 || c.bend > 127)
                || seq.events.iter().any(|e| match e.kind {
                    EventKind::NoteOn { note, velocity } => note > 127 || velocity > 127,
                    EventKind::NoteOff { note } => note > 127,
                    EventKind::Program(p) | EventKind::Bend(p) => p > 127,
                    EventKind::Control(a, b) => a > 127 || b > 127,
                    _ => false,
                })
            {
                return Err(bad("invalid SSsq MIDI/channel values"));
            }
            let length_s = seq.events.last().map_or(0.0, |e| e.update as f64 / 60.0);
            (
                bytes[..size].to_vec(),
                Vec::new(),
                AssetSummary {
                    length_s,
                    ..Default::default()
                },
            )
        }
    };
    if source.size == 0 {
        source.size = native.len();
    }
    Ok(ExtractedAsset {
        kind,
        sources: vec![source],
        summary,
        data: native,
        body,
    })
}
fn extension(name: &str) -> String {
    name.split(';')
        .next()
        .unwrap_or(name)
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
}
fn stem(name: &str) -> String {
    let name = name.split(';').next().unwrap_or(name);
    name.rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .to_ascii_uppercase()
}
fn paired_body(
    entry: &DiscFileEntry,
    entries: &[DiscFileEntry],
    disc: &mut DiscFiles,
    exts: &[&str],
    size: usize,
) -> Result<(Vec<u8>, String, usize)> {
    let matching: Vec<_> = entries
        .iter()
        .filter(|e| exts.contains(&extension(&e.path).as_str()) && e.size == size)
        .collect();
    let named: Vec<_> = matching
        .iter()
        .filter(|e| stem(&e.path) == stem(&entry.path))
        .copied()
        .collect();
    let pair = if named.len() == 1 {
        named[0]
    } else if named.is_empty() && matching.len() == 1 {
        matching[0]
    } else {
        return Err(bad("missing or ambiguous body pair by name/size"));
    };
    let data = disc.read(&pair.path)?;
    if data.len() != size {
        return Err(bad("truncated paired body"));
    }
    Ok((data, pair.path.clone(), 0))
}
fn bad(reason: impl Into<String>) -> Error {
    Error::Corrupt(Format::SoundBank, reason.into())
}
fn region(data: &[u8], offset: usize, size: usize) -> Result<&[u8]> {
    let end = offset.checked_add(size).ok_or_else(|| bad("extent overflow"))?;
    data.get(offset..end).ok_or_else(|| bad("extent outside file"))
}
fn be16(data: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([data.u8(at), data.u8(at + 1)])
}
fn be32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([data.u8(at), data.u8(at + 1), data.u8(at + 2), data.u8(at + 3)])
}

/// Strict ADPCM extent within its allocation; trailing guard bytes are retained.
/// sound_inventory.md appendix D: streams stop at the first end flag.
fn adpcm_frames(bytes: &[u8]) -> Result<usize> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(16) {
        return Err(bad("unaligned/empty ADPCM allocation"));
    }
    for (i, frame) in bytes.chunks_exact(16).enumerate() {
        if frame[0] >> 4 > 4 || frame[0] & 15 > 12 || frame[1] > 7 {
            return Err(bad(format!("invalid ADPCM frame {i}")));
        }
        if frame[1] & 1 != 0 {
            return Ok(i + 1);
        }
    }
    Err(bad("ADPCM stream has no end frame in its allocation"))
}
/// VAB header/table extents: sound_inventory.md §"VAB and standalone VAG".
fn vab_sizes(bytes: &[u8]) -> Result<(usize, usize)> {
    region(bytes, 0, 32)?;
    if !bytes.starts_with(b"pBAV") || !(4..=7).contains(&bytes.u32(4)) {
        return Err(bad("unsupported/invalid VAB version"));
    }
    let (programs, samples) = (bytes.u16(18) as usize, bytes.u16(22) as usize);
    if !(1..=128).contains(&programs) || !(1..=254).contains(&samples) {
        return Err(bad("implausible VAB counts"));
    }
    let table = 32 + 128 * 16 + programs * 512;
    let header = table + 512;
    region(bytes, 0, header)?;
    let mut body = 0;
    for i in 1..=samples {
        let size = bytes.u16(table + i * 2) as usize * 8;
        if size == 0 || !size.is_multiple_of(16) {
            return Err(bad("invalid VAB sample allocation"));
        }
        body += size;
    }
    if body > LIMIT {
        return Err(bad("VAB body exceeds allocation limit"));
    }
    if header + body != bytes.u32(12) as usize {
        return Err(bad("VAB declared size disagrees with sample table"));
    }
    Ok((header, body))
}
fn validate_vab(bytes: &[u8]) -> Result<AssetSummary> {
    let (header, body) = vab_sizes(bytes)?;
    region(bytes, header, body)?;
    if bytes.len() != header + body {
        return Err(bad("VAB has trailing container bytes"));
    }
    let n = bytes.u16(18) as usize;
    let samples = bytes.u16(22) as usize;
    let mut summary = AssetSummary {
        programs: n,
        samples,
        ..Default::default()
    };
    let mut populated = 0;
    for i in 0..128 {
        let p = 32 + i * 16;
        let count = bytes[p] as usize;
        if count > 16 || bytes[p + 1] > 127 {
            return Err(bad("invalid VAB program"));
        }
        if count == 0 {
            continue;
        }
        if populated >= n {
            return Err(bad("VAB populated program count disagrees"));
        }
        for t in 0..count {
            let at = 32 + 128 * 16 + populated * 512 + t * 32;
            let tone = region(bytes, at, 32)?;
            if tone.u16(22) == 0
                || tone.u16(22) as usize > samples
                || tone[2] > 127
                || tone[3] > 127
                || tone[4] > 127
                || tone[6] > tone[7]
                || tone[7] > 127
            {
                return Err(bad("invalid VAB tone range/sample reference"));
            }
        }
        summary.tones += count;
        populated += 1;
    }
    if populated != n || summary.tones != bytes.u16(20) as usize || bytes[24] > 127 || bytes[25] > 127 {
        return Err(bad("VAB tone count/master disagrees"));
    }
    let table = header - 512;
    let mut at = header;
    for i in 1..=samples {
        let size = bytes.u16(table + i * 2) as usize * 8;
        let frames = adpcm_frames(region(bytes, at, size)?)?;
        summary.length_s += (frames * 28) as f64 / 44100.0;
        at += size;
    }
    Ok(summary)
}
/// Native tables and referenced streams: sound_inventory.md §"SShd boundaries".
fn validate_sshd(header: &[u8], body: &[u8]) -> Result<AssetSummary> {
    if header_extent(header) != Some(header.len()) || header.u32(4) as usize != body.len() {
        return Err(bad("invalid SShd header/body extents"));
    }
    let velocity = header.u32(0x14) as usize;
    if velocity != u32::MAX as usize {
        region(header, velocity, 130)?;
    }
    let bank = SoundBank::new(header, body.to_vec())?;
    let mut summary = AssetSummary {
        programs: bank.programs.len(),
        ..Default::default()
    };
    let mut offsets = HashSet::new();
    for program in bank.programs.values() {
        for tone in &program.tones {
            if tone.low > tone.high || tone.high > 127 || tone.root > 127 || tone.volume > 127 || tone.pan > 127 {
                return Err(bad("invalid SShd tone"));
            }
            offsets.insert(tone.sample_offset);
            summary.tones += 1;
        }
    }
    let effects = header.u32(0x2c) as usize;
    if effects != u32::MAX as usize {
        let count = header.u32(effects + 4) as usize + 1;
        for i in 0..count {
            offsets.insert(header.u32(effects + 0x20 + i * 64 + 16) as usize);
        }
    }
    if offsets.is_empty() {
        return Err(bad("SShd has no sample references"));
    }
    let mut offsets: Vec<_> = offsets.into_iter().collect();
    offsets.sort_unstable();
    for (i, &offset) in offsets.iter().enumerate() {
        if !offset.is_multiple_of(16) {
            return Err(bad("unaligned SShd sample offset"));
        }
        let end = offsets.get(i + 1).copied().unwrap_or(body.len());
        let size = end.checked_sub(offset).ok_or_else(|| bad("SShd sample outside body"))?;
        summary.length_s += (adpcm_frames(region(body, offset, size)?)? * 28) as f64 / 44100.0;
    }
    summary.samples = offsets.len();
    SoundBench::from_parts(header, body, None)?;
    Ok(summary)
}
/// Mono VAG extents/rate: sound_inventory.md §"VAB and standalone VAG".
fn vag_extent(bytes: &[u8]) -> Result<(usize, u32)> {
    region(bytes, 0, 0x30)?;
    let size = be32(bytes, 12) as usize;
    let rate = be32(bytes, 16);
    if !bytes.starts_with(b"VAGp")
        || !matches!(be32(bytes, 4), 2 | 3 | 0x20)
        || size == 0
        || size > LIMIT
        || !(1000..=192000).contains(&rate)
    {
        return Err(bad("invalid VAG version/size/rate"));
    }
    region(bytes, 0x30, size)?;
    adpcm_frames(&bytes[0x30..0x30 + size])?;
    Ok((0x30 + size, rate))
}

/// \[I\] CSL chunk/offset tables: Nisto's primary implementation,
/// https://github.com/Nisto/ps2-bankmod/blob/master/ps2-bankmod.py,
/// and VGMTrans SonyPS2InstrSet.cpp parseHeader/parseInstrPointers:
/// https://github.com/vgmtrans/vgmtrans/blob/master/src/main/formats/SonyPS2/SonyPS2InstrSet.cpp.
/// No corresponding bank was found on the supplied discs.
fn hd_sizes(bytes: &[u8]) -> Result<(usize, usize)> {
    region(bytes, 0, 0x50)?;
    if !bytes.starts_with(b"IECSsreV") || bytes.u32(8) != 16 || &bytes[16..24] != b"IECSdaeH" || bytes.u32(24) < 36 {
        return Err(bad("invalid IECS Vers/Head"));
    }
    let header = bytes.u32(28) as usize;
    let body = bytes.u32(32) as usize;
    if !(0x50..=LIMIT).contains(&header) || body == 0 || body > LIMIT {
        return Err(bad("invalid HD/BD size"));
    }
    region(bytes, 0, header)?;
    region(bytes, 16, bytes.u32(24) as usize)?;
    Ok((header, body))
}
fn hd_chunk<'a>(header: &'a [u8], at: usize, tag: &[u8]) -> Result<&'a [u8]> {
    region(header, at, 16)?;
    let chunk = region(header, at, header.u32(at + 8) as usize)?;
    if chunk.len() < 16 || !chunk.starts_with(b"IECS") || &chunk[4..8] != tag {
        return Err(bad("invalid IECS chunk"));
    }
    let count = chunk
        .u32(12)
        .checked_add(1)
        .ok_or_else(|| bad("HD chunk count overflow"))? as usize;
    if count > 8192 {
        return Err(bad("implausible HD chunk count"));
    }
    region(chunk, 16, count * 4)?;
    Ok(chunk)
}
fn hd_param(chunk: &[u8], index: usize, min: usize) -> Result<&[u8]> {
    if index > chunk.u32(12) as usize {
        return Err(bad("HD parameter index out of range"));
    }
    let at = chunk.u32(16 + index * 4) as usize;
    let count = chunk.u32(12) as usize + 1;
    if at < 16 + count * 4 {
        return Err(bad("HD parameter overlaps table"));
    }
    region(chunk, at, min)?;
    Ok(&chunk[at..])
}
fn hd_vagi(header: &[u8], body: &[u8]) -> Result<Vec<(usize, usize, u32)>> {
    let chunk = hd_chunk(header, header.u32(48) as usize, b"igaV")?;
    let mut samples = Vec::new();
    for i in 0..=chunk.u32(12) as usize {
        let record = hd_param(chunk, i, 8)?;
        let start = record.u32(0) as usize;
        let rate = record.u16(4) as u32;
        if !(1000..=192000).contains(&rate) || !start.is_multiple_of(16) || start >= body.len() {
            return Err(bad("invalid HD Vagi offset/rate"));
        }
        samples.push((start, 0, rate));
    }
    for i in 0..samples.len() {
        let end = samples.get(i + 1).map_or(body.len(), |s| s.0);
        let (start, size, _) = &mut samples[i];
        *size = end
            .checked_sub(*start)
            .filter(|n| *n != 0)
            .ok_or_else(|| bad("nonascending HD Vagi offsets"))?;
        adpcm_frames(region(body, *start, *size)?)?;
    }
    Ok(samples)
}
fn validate_hd(header: &[u8], body: &[u8]) -> Result<AssetSummary> {
    if hd_sizes(header)? != (header.len(), body.len()) {
        return Err(bad("HD/BD extent mismatch"));
    }
    let samples = hd_vagi(header, body)?;
    let prog = hd_chunk(header, header.u32(36) as usize, b"gorP")?;
    let sset = hd_chunk(header, header.u32(40) as usize, b"tesS")?;
    let smpl = hd_chunk(header, header.u32(44) as usize, b"lpmS")?;
    let mut ranges: Vec<_> = [(36, prog), (40, sset), (44, smpl)]
        .into_iter()
        .map(|(ptr, chunk)| (header.u32(ptr) as usize, chunk.len()))
        .collect();
    let vagi = hd_chunk(header, header.u32(48) as usize, b"igaV")?;
    ranges.push((header.u32(48) as usize, vagi.len()));
    ranges.sort_unstable();
    let mut previous = 16 + header.u32(24) as usize;
    for (start, size) in ranges {
        if start < previous {
            return Err(bad("overlapping IECS chunks"));
        }
        previous = start + size;
    }
    let mut summary = AssetSummary {
        samples: samples.len(),
        ..Default::default()
    };
    for i in 0..=smpl.u32(12) as usize {
        if smpl.u32(16 + i * 4) == u32::MAX {
            continue;
        }
        let p = hd_param(smpl, i, 42)?;
        if p.u16(0) as usize >= samples.len() {
            return Err(bad("HD Smpl references absent Vagi"));
        }
        summary.tones += 1;
    }
    for i in 0..=sset.u32(12) as usize {
        if sset.u32(16 + i * 4) == u32::MAX {
            continue;
        }
        let p = hd_param(sset, i, 4)?;
        region(p, 4, p[3] as usize * 2)?;
        for j in 0..p[3] as usize {
            hd_param(smpl, p.u16(4 + j * 2) as usize, 42)?;
        }
    }
    for i in 0..=prog.u32(12) as usize {
        if prog.u32(16 + i * 4) == u32::MAX {
            continue;
        }
        let p = hd_param(prog, i, 36)?;
        if p[5] != 20 {
            return Err(bad("unsupported HD split size"));
        }
        let splits = region(p, p.u32(0) as usize, p[4] as usize * 20)?;
        for split in splits.chunks_exact(20) {
            hd_param(sset, split.u16(0) as usize, 4)?;
        }
        summary.programs += 1;
    }
    for (start, size, rate) in samples {
        summary.length_s += (adpcm_frames(region(body, start, size)?)? * 28) as f64 / rate as f64;
    }
    Ok(summary)
}

// [I] Integer PCM RIFF/WAVE framing: Microsoft RIFF documentation.
// https://learn.microsoft.com/en-us/windows/win32/xaudio2/resource-interchange-file-format--riff-
struct Wave<'a> {
    size: usize,
    rate: u32,
    bits: usize,
    channels: usize,
    frames: usize,
    pcm: &'a [u8],
}
fn wave_parse(bytes: &[u8]) -> Result<Wave<'_>> {
    region(bytes, 0, 12)?;
    if !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WAVE" {
        return Err(bad("RIFF is not WAVE"));
    }
    let size = (bytes.u32(4) as usize)
        .checked_add(8)
        .ok_or_else(|| bad("RIFF size overflow"))?;
    if size > LIMIT {
        return Err(bad("WAVE exceeds sample allocation limit"));
    }
    let wave = region(bytes, 0, size)?;
    let (mut at, mut fmt, mut pcm) = (12, None, None);
    while at < size {
        let prefix = region(wave, at, 8)?;
        let n = prefix.u32(4) as usize;
        let chunk = region(wave, at + 8, n)?;
        match &prefix[..4] {
            b"fmt " if fmt.is_none() => fmt = Some(chunk),
            b"data" if pcm.is_none() => pcm = Some(chunk),
            b"fmt " | b"data" => return Err(bad("duplicate WAVE fmt/data chunk")),
            _ => {}
        }
        at = at
            .checked_add(8 + n + (n & 1))
            .ok_or_else(|| bad("RIFF chunk overflow"))?;
        if at > size {
            return Err(bad("missing RIFF chunk padding"));
        }
    }
    let fmt = fmt.ok_or_else(|| bad("WAVE has no fmt"))?;
    region(fmt, 0, 16)?;
    let channels = fmt.u16(2) as usize;
    let rate = fmt.u32(4);
    let bits = fmt.u16(14) as usize;
    if fmt.u16(0) != 1
        || !(1..=2).contains(&channels)
        || !matches!(bits, 8 | 16 | 24 | 32)
        || !(1000..=192000).contains(&rate)
    {
        return Err(bad("WAVE is not supported integer PCM"));
    }
    let align = channels * bits / 8;
    if fmt.u16(12) as usize != align || fmt.u32(8) as u64 != rate as u64 * align as u64 {
        return Err(bad("WAVE block alignment/byte rate mismatch"));
    }
    let pcm = pcm.ok_or_else(|| bad("WAVE has no data"))?;
    if pcm.is_empty() || !pcm.len().is_multiple_of(align) {
        return Err(bad("WAVE data has partial frames"));
    }
    Ok(Wave {
        size,
        rate,
        channels,
        bits,
        frames: pcm.len() / align,
        pcm,
    })
}

// [I] PSX-SPX §"SEQ - Single Sequence": BE clocks and MIDI events.
// Running status, variable lengths and the terminator are bounded to one traversal.
fn seq_extent(bytes: &[u8]) -> Result<(usize, f64)> {
    region(bytes, 0, 15)?;
    let resolution = be16(bytes, 8);
    let mut tempo = ((bytes[10] as u32) << 16) | ((bytes[11] as u32) << 8) | bytes[12] as u32;
    if !bytes.starts_with(b"pQES")
        || be32(bytes, 4) != 1
        || resolution == 0
        || tempo == 0
        || bytes[13] == 0
        || bytes[14] > 7
    {
        return Err(bad("invalid SEQ version/clock"));
    }
    let (mut at, mut status, mut seconds) = (15, 0u8, 0.0);
    for _ in 0..1_000_000 {
        let delta = vlq(bytes, &mut at)?;
        seconds += delta as f64 * tempo as f64 / 1_000_000.0 / resolution as f64;
        let byte = *bytes.get(at).ok_or_else(|| bad("truncated SEQ event"))?;
        if byte & 128 != 0 {
            status = byte;
            at += 1;
        }
        if status == 0xff {
            let meta = *bytes.get(at).ok_or_else(|| bad("truncated SEQ meta"))?;
            at += 1;
            let n = vlq(bytes, &mut at)? as usize;
            let payload = region(bytes, at, n)?;
            at += n;
            if meta == 0x2f {
                if n != 0 {
                    return Err(bad("invalid SEQ end event"));
                }
                return Ok((at, seconds));
            }
            if meta == 0x51 {
                if n != 3 {
                    return Err(bad("invalid SEQ tempo event"));
                }
                tempo = ((payload[0] as u32) << 16) | ((payload[1] as u32) << 8) | payload[2] as u32;
                if tempo == 0 {
                    return Err(bad("zero SEQ tempo"));
                }
            }
            status = 0;
        } else if status == 0xf0 || status == 0xf7 {
            let n = vlq(bytes, &mut at)? as usize;
            region(bytes, at, n)?;
            at += n;
            status = 0;
        } else {
            let n = match status {
                0x80..=0xbf | 0xe0..=0xef => 2,
                0xc0..=0xdf => 1,
                _ => return Err(bad("invalid SEQ status")),
            };
            if region(bytes, at, n)?.iter().any(|v| *v > 127) {
                return Err(bad("invalid SEQ data byte"));
            }
            at += n;
        }
    }
    Err(bad("SEQ exceeds event limit"))
}
fn vlq(bytes: &[u8], at: &mut usize) -> Result<u32> {
    let mut value = 0;
    for _ in 0..4 {
        let byte = *bytes.get(*at).ok_or_else(|| bad("truncated variable length value"))?;
        *at += 1;
        value = (value << 7) | (byte & 127) as u32;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err(bad("variable length value exceeds four bytes"))
}

/// A playable bench for one extracted bank (VAB, SShd + body, or HD/BD), parsed and bounded again.
pub fn asset_bench(kind: AssetKind, data: &[u8], body: &[u8]) -> Result<SoundBench> {
    match kind {
        AssetKind::Vab => {
            validate_vab(data)?;
            let (header, _) = vab_sizes(data)?;
            let mut offsets = vec![0];
            let mut at = 0;
            for i in 1..=data.u16(22) as usize {
                offsets.push(at);
                at += data.u16(header - 512 + i * 2) as usize * 8;
            }
            let mut programs = HashMap::new();
            let mut populated = 0;
            // Sparse program IDs and packed tone blocks: PSX-SPX §VAB Header;
            // sound_inventory.md §VAB and standalone VAG verifies the shell subset.
            for id in 0..128 {
                let p = 32 + id * 16;
                if data[p] == 0 {
                    continue;
                }
                let mut tones = Vec::new();
                for t in 0..data[p] as usize {
                    let o = 32 + 128 * 16 + populated * 512 + t * 32;
                    tones.push(Tone {
                        low: data[o + 6] as i32,
                        high: data[o + 7] as i32,
                        root: data[o + 4] as i32,
                        fine: data[o + 5] as i8 as i32,
                        sample_offset: offsets[data.u16(o + 22) as usize],
                        adsr1: data.u16(o + 16),
                        adsr2: data.u16(o + 18),
                        volume: (data[o + 2] as i32 * data[24] as i32 / 127),
                        pan: data[o + 3] as i32,
                        bend_range: 2,
                        flags: 0,
                    });
                }
                programs.insert(
                    id as i32,
                    Program {
                        kind: ProgramKind::Layer,
                        volume: data[p + 1] as i32,
                        tones,
                    },
                );
                populated += 1;
            }
            Ok(SoundBench::from_bank(
                SoundBank::from_native(programs, data[header..].to_vec()),
                None,
            ))
        }
        AssetKind::SShd => {
            validate_sshd(data, body)?;
            SoundBench::from_parts(data, body, None)
        }
        AssetKind::Hd => {
            validate_hd(data, body)?;
            let samples = hd_vagi(data, body)?
                .into_iter()
                .map(|(start, size, rate)| {
                    let (pcm, lp) = decode_adpcm(&body[start..start + size], 0);
                    (pcm, lp, rate)
                })
                .collect();
            SoundBench::from_samples(samples)
        }
        _ => Err(bad("asset is not a bank")),
    }
}
/// One extracted loose sample (VAG or RIFF WAVE): mono PCM, loop start, declared sample rate.
pub fn asset_sample(kind: AssetKind, data: &[u8]) -> Result<(Vec<f32>, Option<usize>, u32)> {
    match kind {
        AssetKind::Vag => {
            let (size, rate) = vag_extent(data)?;
            if size != data.len() {
                return Err(bad("VAG has trailing container bytes"));
            }
            let (pcm, lp) = decode_adpcm(&data[0x30..size], 0);
            Ok((pcm, lp, rate))
        }
        AssetKind::Wave => {
            let wave = wave_parse(data)?;
            if wave.size != data.len() {
                return Err(bad("WAVE has trailing container bytes"));
            }
            let mut pcm = Vec::with_capacity(wave.frames);
            for frame in wave.pcm.chunks_exact(wave.channels * wave.bits / 8) {
                let sum: f32 = frame
                    .chunks_exact(wave.bits / 8)
                    .map(|sample| match wave.bits {
                        8 => (sample[0] as f32 - 128.0) * 256.0,
                        16 => sample.i16(0) as f32,
                        24 => (((sample.u32(0) << 8) as i32) >> 8) as f32 / 256.0,
                        32 => sample.i32(0) as f32 / 65536.0,
                        _ => 0.0,
                    })
                    .sum();
                pcm.push(sum / wave.channels as f32);
            }
            Ok((pcm, None, wave.rate))
        }
        _ => Err(bad("asset is not a loose sample")),
    }
}

/// An extracted set's `index.tsv`: the disc title and one row per occurrence of an asset.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SetIndex {
    /// The disc title recorded when the set was written.
    pub title: String,
    /// Every occurrence of every native file, in index order.
    pub rows: Vec<SetRow>,
}
/// One `index.tsv` row: the native file, its body companion (SShd/HD) and where it came from.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SetRow {
    /// Asset format.
    pub kind: AssetKind,
    /// Native file name inside the set folder.
    pub file: String,
    /// Body companion file name, empty when the asset is self-contained.
    pub body_file: String,
    /// Source coordinates on the disc.
    pub source: AssetSource,
}
/// Reads one native file of a set: a bounded regular file, never through a symlink.
pub fn read_native(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > LIMIT as u64 {
        return Err(bad("native asset is not a bounded regular file"));
    }
    Ok(std::fs::read(path)?)
}
/// Reads a set folder's `index.tsv` written by [`Extraction::write`].
pub fn read_index(folder: &Path) -> Result<SetIndex> {
    let text = std::fs::read_to_string(folder.join("index.tsv"))?;
    if text.lines().next() != Some(MARKER) {
        return Err(bad("unrecognised soundfonts index"));
    }
    let mut index = SetIndex {
        title: String::new(),
        rows: Vec::new(),
    };
    for line in text.lines().skip(1) {
        if let Some(title) = line.strip_prefix("# title\t") {
            index.title = title.to_string();
            continue;
        }
        if line.starts_with('#') || line.starts_with("kind\t") || line.is_empty() {
            continue;
        }
        let cols: Vec<_> = line.split('\t').collect();
        if cols.len() != 13 {
            return Err(bad("invalid index row"));
        }
        let kind = AssetKind::ALL
            .into_iter()
            .find(|k| k.name() == cols[0])
            .ok_or_else(|| bad("unknown index kind"))?;
        for name in [cols[1], cols[9]].into_iter().filter(|n| !n.is_empty()) {
            if Path::new(name).components().count() != 1 || name.contains(['/', '\\']) || name == "." || name == ".." {
                return Err(bad("unsafe native filename"));
            }
        }
        let number = |s: &str| s.parse::<usize>().map_err(|_| bad("invalid index coordinate"));
        index.rows.push(SetRow {
            kind,
            file: cols[1].into(),
            body_file: cols[9].into(),
            source: AssetSource {
                file: cols[2].into(),
                offset: number(cols[3])?,
                size: number(cols[4])?,
                body_file: (!cols[10].is_empty()).then(|| cols[10].into()),
                body_offset: if cols[11].is_empty() {
                    None
                } else {
                    Some(number(cols[11])?)
                },
                body_size: number(cols[12])?,
            },
        });
    }
    if index.title.is_empty() {
        index.title = folder
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    Ok(index)
}
fn field(value: &str) -> String {
    value.replace(['\t', '\r', '\n', '\0'], " ")
}
fn safe_name(value: &str) -> String {
    let result: String = value
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .scan(0usize, |length, c| {
            *length += c.len_utf8();
            (*length <= 80).then_some(c)
        })
        .collect();
    let result = result.trim().trim_matches('.');
    if result.is_empty() {
        "unknown".into()
    } else {
        result.into()
    }
}
fn utc_date() -> String {
    // Gregorian civil date from days since Unix epoch.
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86400;
    let z = days as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Splits an extracted bank into its header and body bytes (the parts its fingerprint covers).
pub fn bank_parts<'a>(kind: AssetKind, data: &'a [u8], body: &'a [u8]) -> Result<(&'a [u8], &'a [u8])> {
    if kind == AssetKind::Vab {
        let (header, _) = vab_sizes(data)?;
        Ok((&data[..header], &data[header..]))
    } else {
        Ok((data, body))
    }
}
