//! Runtime sound catalogue built only from the caller's files.
//!
//! `notes/research/sound_inventory.md` §§"Banks, sample sets, and exact pairing",
//! "Every stored score, its bank, and its role", "What is truly distinct for the music maker" and
//! "Recommended ps2kit::library shape" describe the recovered formats and bindings.
//! Five audition banks include TESTSPU; the byte-distinct 1.00 menu header is retained
//! as a resolvable variant rather than another twelve choices. Fingerprints are local,
//! domain-separated, length-framed FNV-1a-128 hashes, not a shipped Sony hash catalogue
//! or a security primitive. Deduplication additionally compares complete parsed bytes.

use crate::bench::{Character, SoundBench, Style};
use crate::bytes::Bytes;
use crate::locate::ProgramImage;
use crate::ps1::{NoteEvent, Ps1Shell, Vab};
use crate::ps1bios::Ps1Bios;
use crate::ps1intro::SceIntro;
use crate::rom::{unpack, RomDir};
use crate::song::{BankRef, Channel, Instrument, InstrumentRef, Kit, Note, Pattern, Song};
use crate::sound::{decode_adpcm, DriverTables, EventKind, SoundSequence};
use crate::{Error, Format, Result, VideoMode};
use std::path::{Path, PathBuf};

/// Portable bank dependency; equality uses the fingerprint, independently of its label.
#[derive(Debug, Clone, Eq)]
#[non_exhaustive]
pub struct BankKey {
    /// Hex content hash computed at runtime; empty denotes the legacy song default.
    pub fingerprint: String,
    /// Human label saved with a song so missing dependencies remain understandable.
    pub label: String,
}

impl PartialEq for BankKey {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}
impl std::hash::Hash for BankKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.fingerprint.hash(state);
    }
}
impl Default for BankKey {
    fn default() -> Self {
        Self::new("", "Song default bank")
    }
}
impl BankKey {
    /// Creates a dependency; the song validator checks its bounded fingerprint and label.
    pub fn new(fingerprint: impl Into<String>, label: impl Into<String>) -> Self {
        Self { fingerprint: fingerprint.into(), label: label.into() }
    }
    /// Whether this is the default binding migrated from version-1/2 song files.
    pub fn is_default(&self) -> bool {
        self.fingerprint.is_empty()
    }
    pub(crate) fn legacy_label(bank: BankRef) -> &'static str {
        match bank {
            BankRef::Ps1 => "PS1 shell VAB",
            BankRef::Ps2 => "PS2 boot bank",
            BankRef::Ps2Logo => "PS2 logo bank",
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if (!self.is_default()
            && (self.fingerprint.len() != 32 || !self.fingerprint.bytes().all(|c| c.is_ascii_hexdigit())))
            || self.label.is_empty()
            || self.label.len() > 1024
        {
            return Err(Error::Corrupt(Format::Sequence, "invalid bank fingerprint or label".into()));
        }
        Ok(())
    }
}

/// A resolved bank and its optional locally derived percussion kit.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ResolvedBank<'a> {
    /// Native/sample renderer owning the caller's decoded audio.
    pub bench: &'a SoundBench,
    /// Optional measured, sample-derived kit from that bank.
    pub kit: Option<&'a Kit>,
}

/// Resolves portable dependencies; legacy defaults also receive their old bank kind.
pub trait BankResolver {
    /// Returns `None` when this machine lacks the requested bank.
    fn resolve(&self, key: &BankKey, default: BankRef) -> Option<ResolvedBank<'_>>;
}
impl BankResolver for SoundBench {
    fn resolve(&self, key: &BankKey, default: BankRef) -> Option<ResolvedBank<'_>> {
        ((key.is_default() && self.bank == default) || self.key.as_ref().is_some_and(|k| !key.is_default() && k == key))
            .then_some(ResolvedBank { bench: self, kit: None })
    }
}

/// Console family carrying an asset; shared PS1 audio can have sources from both families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Console {
    /// Standalone PS1, PSone and POPS shell.
    Ps1,
    /// PS2 ROMDIR image, including its PS1-mode shell.
    Ps2,
}

/// Bank role/format; effect entries remain effects rather than invented MIDI programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BankKind {
    /// B03: boot/ambience instruments.
    Ps2Boot,
    /// B02: shared PS1 shell VAB.
    Ps1Shell,
    /// B01: game-logo chord effects.
    Ps2Logo,
    /// B04/B05: menu effects with fourteen/twelve-entry headers.
    Ps2Menu,
    /// S01: standalone manufacturing-test VAG.
    TestSine,
}

/// Source coordinates in an input module or explicitly labelled decompressed view.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Provenance {
    /// Input path; kept in the library, never embedded in song dependencies.
    pub file: PathBuf,
    /// ROMDIR path, or standalone shell view name.
    pub module: String,
    /// Header/table offset within that module/view, not a guessed absolute file offset.
    pub offset: usize,
    /// Version/ROMVER read from this input, empty when unknown.
    pub version: String,
    /// Whether coordinates refer to decoded rather than stored bytes.
    pub decoded: bool,
    /// Body module when distinct from the header module.
    pub body_module: Option<String>,
    /// Body offset in its module/view, when applicable.
    pub body_offset: Option<usize>,
}

/// A byte-distinct complete bank sharing the primary bank's existing effect choices.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BankVariant {
    /// Its own complete-header/body fingerprint (B05 differs from B04).
    pub key: BankKey,
    /// Files containing this exact variant.
    pub sources: Vec<Provenance>,
    /// Renderer retaining precisely the variant's native entries.
    pub bench: SoundBench,
    content: Vec<u8>,
}

/// One distinct audition bank, with all carriers and any shared-audio header variants.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LibraryBank {
    /// Complete parsed-content identity and human name.
    pub key: BankKey,
    /// Native console of this bank's playback format.
    pub console: Console,
    /// Recovered role/format.
    pub kind: BankKind,
    /// Every file occurrence, including the separately identified header variants.
    pub sources: Vec<Provenance>,
    /// Renderer for the fullest available bank (B04 when present).
    pub bench: SoundBench,
    /// Locally measured percussion recipes; omitted when no lane can be derived.
    pub kit: Option<Kit>,
    /// Header variants that add no distinct playable choices.
    pub variants: Vec<BankVariant>,
    header: Vec<u8>,
    body: Vec<u8>,
}

/// Evidence for a piece's role; trigger names alone never establish playback use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Evidence {
    /// A recovered call site or stored table verifies the role \[V\].
    Verified,
    /// A descriptive audition/timing interpretation \[I\].
    Inferred,
    /// Stored and registered \[V\], but no actual trigger role was verified.
    RegisteredCue,
}

/// A console timing arrangement for the same stored PS1 table, not an extra piece.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TimingVariant {
    /// Display name including video mode and source version.
    pub label: String,
    /// Input whose shell code provided the timing recipe.
    pub source: Provenance,
    /// Repository-recovered scene schedule \[I\], with native bank binding.
    pub song: Song,
}

#[derive(Debug, Clone)]
enum Score {
    Sequence(Box<SoundSequence>),
    Table(Vec<NoteEvent>),
    LogoChord,
}

/// One stored music cluster or executable chord recipe, with searchable native aliases.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LibraryPiece {
    /// Hash of exact table/sequence bytes through the terminator, or of the chord recipe.
    pub key: String,
    /// User-facing name, following the inventory's role distinctions.
    pub label: String,
    /// Inventory role text; unverified timer roles explicitly say "registered cue".
    pub role: String,
    /// Strength of evidence behind that role.
    pub evidence: Evidence,
    /// Console that uses the score (Q08 also appears in PS2 PS1-mode).
    pub console: Console,
    /// Required bank fingerprint and human label.
    pub bank: BankKey,
    /// Every carrier/alias occurrence.
    pub sources: Vec<Provenance>,
    /// Nominal seconds for one stored traversal; excludes release tails/external stops.
    /// R01 uses zero for the simultaneous trigger recipe, not an exact audible duration.
    pub length_s: f64,
    /// Native asset names and inventory cluster label, including Q04's two aliases.
    pub aliases: Vec<String>,
    /// Table-clock alternatives, including the licence drone prelude and SCE scene clock.
    pub console_timing: Vec<TimingVariant>,
    /// Whether console playback repeats indefinitely (Q03/Q06).
    pub loops: bool,
    score: Score,
    content: Vec<u8>,
}

impl LibraryPiece {
    /// Imports the table or one complete SSsq traversal, retaining its loop markers.
    /// PS1 console timing, including Q08's drone prelude, is offered in `console_timing`.
    pub fn to_song(&self, library: &SoundLibrary) -> Result<Song> {
        let resolved = library
            .resolve(&self.bank, BankRef::Ps2)
            .ok_or_else(|| Error::NotFound(Format::SoundBank, self.bank.label.clone()))?;
        let mut song = match &self.score {
            Score::Sequence(seq) => Song::from_sequence(seq, &self.label, 127)?,
            Score::Table(events) => Song::from_ps1_note_table(events, &self.label)?,
            Score::LogoChord => {
                // [V] PS2LOGO issues effects 0..4; sound_inventory.md,
                // "The tenth musical item: PS2 game-logo recipe". Simultaneous
                // editor triggers and a finite audition tail are choices [I].
                let mut song = Song::new(&self.label, BankRef::Ps2Logo);
                for e in resolved.bench.effects.iter().filter(|e| e.id < 5) {
                    let mut channel =
                        Channel::with_instrument(format!("Logo effect {}", e.id), Instrument::Effect(e.id));
                    let mut pattern = Pattern::new(song.ticks_per_bar());
                    pattern.notes.push(Note::new(0, 1, 60, 127));
                    channel.patterns.push(pattern);
                    channel.arrangement.push(Some(0));
                    song.channels.push(channel);
                }
                song.end_tick = Some(1920);
                song
            }
        };
        bind_song(&mut song, &self.bank);
        song.validate()?;
        Ok(song)
    }

    /// Imports a particular shell's console timing variant of this same stored table.
    pub fn to_song_variant(&self, library: &SoundLibrary, index: usize) -> Result<Song> {
        if library.resolve(&self.bank, BankRef::Ps1).is_none() {
            return Err(Error::NotFound(Format::SoundBank, self.bank.label.clone()));
        }
        self.console_timing
            .get(index)
            .map(|v| v.song.clone())
            .ok_or_else(|| Error::NotFound(Format::Sequence, "console timing variant".into()))
    }
}

/// Style rows of bank-scoped native program/effect/sample IDs and measured character.
pub type LibraryStyles = Vec<(Style, Vec<(BankKey, i32, Character)>)>;

/// Runtime catalogue, tolerant of absent/unreadable/damaged individual inputs.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SoundLibrary {
    /// Five audition groups in the surveyed corpus, including the loose test sine.
    pub banks: Vec<LibraryBank>,
    /// Nine stored-score clusters plus the PS2 logo chord recipe.
    pub pieces: Vec<LibraryPiece>,
    /// Unreadable/damaged assets and recoverable items outside the documented inventory.
    pub diagnostics: Vec<String>,
    // Version-2 PS2 songs address the old combined boot/menu bench's sample indices
    // and effect IDs. Retain that renderer solely for legacy default dependencies.
    legacy_ps2: Option<SoundBench>,
}

impl BankResolver for SoundLibrary {
    fn resolve(&self, key: &BankKey, default: BankRef) -> Option<ResolvedBank<'_>> {
        if key.is_default() && default == BankRef::Ps2 {
            if let Some(bench) = &self.legacy_ps2 {
                return Some(ResolvedBank { bench, kit: None });
            }
        }
        for bank in &self.banks {
            if (key.is_default()
                && bank.kind
                    == match default {
                        BankRef::Ps1 => BankKind::Ps1Shell,
                        BankRef::Ps2 => BankKind::Ps2Boot,
                        BankRef::Ps2Logo => BankKind::Ps2Logo,
                    })
                || (!key.is_default() && bank.key == *key)
            {
                return Some(ResolvedBank { bench: &bank.bench, kit: bank.kit.as_ref() });
            }
            if let Some(variant) = bank.variants.iter().find(|v| !key.is_default() && v.key == *key) {
                return Some(ResolvedBank { bench: &variant.bench, kit: None });
            }
        }
        None
    }
}

impl SoundLibrary {
    /// Scans caller-provided BIOS/ROMDIR files; failures are recorded per source.
    /// Auxiliary ROM1/ROM2/EROM carry no recovered sound in the inventory (§"ROM1,
    /// ROM2, EROM, and the DVD player"); no PS3 protected payload is decoded.
    pub fn scan(files: &[PathBuf]) -> Self {
        let mut library = Self::default();
        let mut files = files.to_vec();
        files.sort();
        files.dedup();
        for file in files {
            let result = std::fs::read(&file).map_err(Error::from).and_then(|data| library.scan_file(&file, data));
            if let Err(e) = result {
                library.diagnostics.push(format!("{}: {e}", file.display()));
            }
        }
        library.banks.sort_by_key(|b| match b.kind {
            BankKind::Ps2Boot => 0,
            BankKind::Ps1Shell => 1,
            BankKind::Ps2Menu => 2,
            BankKind::Ps2Logo => 3,
            BankKind::TestSine => 4,
        });
        library.pieces.sort_by(|a, b| a.aliases.first().cmp(&b.aliases.first()));
        for bank in &mut library.banks {
            let kit = Kit::derive(&bank.bench);
            if !kit.pieces.is_empty() {
                bank.kit = Some(kit);
            }
        }
        if let Some(boot) = library.banks.iter().find(|b| b.kind == BankKind::Ps2Boot) {
            let mut legacy = boot.bench.clone();
            legacy.key = None;
            if let Some(menu) = library.banks.iter().find(|b| b.kind == BankKind::Ps2Menu) {
                if let Err(e) = legacy.add_effects(&menu.header, &menu.body) {
                    library.diagnostics.push(format!("Legacy PS2 default binding: {e}"));
                }
            }
            library.legacy_ps2 = Some(legacy);
        }
        library
    }

    /// Finds a primary bank by exact content key; header variants resolve via `resolve`.
    pub fn bank(&self, key: &BankKey) -> Option<&LibraryBank> {
        self.banks.iter().find(|b| b.key == *key)
    }

    /// Finds a piece by fingerprint, native alias, inventory ID or display label.
    pub fn piece(&self, name: &str) -> Option<&LibraryPiece> {
        self.pieces.iter().find(|p| {
            p.key == name
                || p.label.eq_ignore_ascii_case(name)
                || p.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
        })
    }

    /// Native programs, effects and loose samples; B05's shared entries appear once.
    pub fn choices(&self) -> Vec<InstrumentRef> {
        self.banks
            .iter()
            .flat_map(|b| {
                let mut choices: Vec<_> = b
                    .bench
                    .programs
                    .iter()
                    .filter_map(|p| u8::try_from(p.id).ok())
                    .map(|p| InstrumentRef::new(b.key.clone(), Instrument::Program(p)))
                    .collect();
                choices.extend(
                    b.bench.effects.iter().map(|e| InstrumentRef::new(b.key.clone(), Instrument::Effect(e.id))),
                );
                if b.kind == BankKind::TestSine {
                    choices.extend(
                        b.bench.samples.iter().map(|s| InstrumentRef::new(b.key.clone(), Instrument::Sample(s.index))),
                    );
                }
                choices
            })
            .collect()
    }

    /// Style catalogue across all choices, keeping native IDs scoped to their banks.
    /// The integer is a program ID, effect index or loose sample index, per bank kind.
    pub fn by_style(&self) -> LibraryStyles {
        let mut rows: LibraryStyles = Style::ALL.into_iter().map(|style| (style, Vec::new())).collect();
        for bank in &self.banks {
            let programs = bank.bench.programs.iter().filter_map(|p| bank.bench.character(p.id).map(|c| (p.id, c)));
            let effects =
                bank.bench.effects.iter().filter_map(|e| bank.bench.effect_character(e.id).map(|c| (e.id as i32, c)));
            let samples = bank
                .bench
                .samples
                .iter()
                .filter(|_| bank.kind == BankKind::TestSine)
                .filter_map(|s| bank.bench.sample_character(s.index).map(|c| (s.index as i32, c)));
            for (id, character) in programs.chain(effects).chain(samples) {
                if let Some((_, row)) = rows.iter_mut().find(|r| r.0 == character.style) {
                    row.push((bank.key.clone(), id, character));
                }
            }
        }
        rows
    }

    fn scan_file(&mut self, file: &Path, data: Vec<u8>) -> Result<()> {
        if Ps1Bios::detect(&data) {
            let bios = Ps1Bios::new(data)?;
            let image = bios.shell_image()?;
            let version = if bios.identity().version.is_empty() {
                bios.identity().model.clone()
            } else {
                bios.identity().version.clone()
            };
            let decoded = bios.identity().shell_storage != crate::ps1bios::ShellStorage::Raw;
            let source = provenance(file, "PS1 shell", &version, 0, decoded);
            let shell = Ps1Shell::load_ps1(&bios).ok();
            let intro = SceIntro::locate(&image, bios.identity()).ok();
            self.scan_shell(&image, source, shell.as_ref(), intro.as_ref())?;
        } else if data.starts_with(b"SCE\0") || data.starts_with(b"SCEUF") {
            // No interpretation of protected PS3 payloads.
        } else if let Ok(rom) = RomDir::new(data.clone()) {
            let version = rom
                .module("ROMVER")
                .map(|d| String::from_utf8_lossy(d).trim_end_matches('\0').trim().to_string())
                .unwrap_or_default();
            let tables = rom.module("OSDSND").ok().and_then(DriverTables::from_driver);
            if let Ok(snd) = rom.module("SNDIMAGE") {
                match RomDir::new(snd.to_vec())
                    .and_then(|archive| self.scan_sound_archive(file, &version, &archive, tables.clone()))
                {
                    Ok(()) => {}
                    Err(e) => self.diagnostics.push(format!("{} SNDIMAGE: {e}", file.display())),
                }
            } else if let Ok(osd) = rom.module("OSDSYS") {
                self.scan_inline(file, "OSDSYS", &version, osd, tables.clone(), true);
            }
            if let Ok(logo) = rom.module("PS2LOGO") {
                self.scan_inline(file, "PS2LOGO", &version, logo, tables.clone(), false);
            }
            if let Ok(logo) = rom.module("LOGO") {
                match unpack(logo, 0x54) {
                    Ok(image) => {
                        let source = provenance(file, "LOGO", &version, 0, true);
                        let shell = Ps1Shell::load(&rom).ok();
                        if let Err(e) = self.scan_shell(&image, source, shell.as_ref(), None) {
                            self.diagnostics.push(format!("{} LOGO: {e}", file.display()));
                        }
                    }
                    Err(e) => self.diagnostics.push(format!("{} LOGO: {e}", file.display())),
                }
            }
            if let Ok(test) = rom.module("TESTSPU") {
                self.scan_vags(test, provenance(file, "TESTSPU", &version, 0, false));
            }
        } else {
            if data.windows(20).any(|w| w == b"PS-X Realtime Kernel") && crate::ps1bios::audit(&data).is_corrupt() {
                return Err(corrupt("damaged standalone PS1 source; unavailable"));
            }
            // Standalone synthetic/parser inputs are useful for offline validation.
            self.scan_inline(file, "file", "", &data, None, false);
            self.scan_vags(&data, provenance(file, "file", "", 0, false));
            if data.starts_with(b"pBAV") {
                self.scan_shell(&data, provenance(file, "file", "", 0, false), None, None)?;
            }
        }
        Ok(())
    }

    fn scan_sound_archive(
        &mut self,
        file: &Path,
        version: &str,
        archive: &RomDir,
        tables: Option<DriverTables>,
    ) -> Result<()> {
        let asset = |name: &str| unpack(archive.module(name)?, 0);
        let header = asset("SNDBOOTH")?;
        let body = asset("SNDBOOTB")?;
        let mut source = provenance(file, "SNDIMAGE/SNDBOOTH", version, 0, true);
        source.body_module = Some("SNDIMAGE/SNDBOOTB".into());
        source.body_offset = Some(0);
        let key = self.add_sshd(&header, &body, BankKind::Ps2Boot, source, tables.clone())?;
        if let (Ok(header), Ok(body)) = (asset("SNDOSDDH"), asset("SNDOSDDB")) {
            let mut source = provenance(file, "SNDIMAGE/SNDOSDDH", version, 0, true);
            source.body_module = Some("SNDIMAGE/SNDOSDDB".into());
            source.body_offset = Some(0);
            self.add_sshd(&header, &body, BankKind::Ps2Menu, source, tables)?;
        }
        for name in SEQUENCES {
            match asset(name).and_then(|data| {
                self.add_sequence(
                    &data,
                    name,
                    key.clone(),
                    provenance(file, &format!("SNDIMAGE/{name}"), version, 0, true),
                )
            }) {
                Ok(()) => {}
                Err(e) => self.diagnostics.push(format!("{} {name}: {e}", file.display())),
            }
        }
        Ok(())
    }

    fn scan_inline(
        &mut self,
        file: &Path,
        module: &str,
        version: &str,
        stored: &[u8],
        tables: Option<DriverTables>,
        sequences: bool,
    ) {
        let raw_banks = inline_banks(stored);
        let (data, decoded, banks) = if !raw_banks.is_empty() || !stored.starts_with(b"\x7fELF") {
            (stored.to_vec(), false, raw_banks)
        } else {
            match ProgramImage::from_module(stored, if module == "OSDSYS" { 0x200000 } else { 0x100000 }) {
                Ok(img) => {
                    let banks = inline_banks(img.data());
                    (img.data().to_vec(), true, banks)
                }
                Err(_) => return,
            }
        };
        let mut boot = None;
        for (start, header_size, body_start, body_size) in banks {
            let header = &data[start..start + header_size];
            let body = &data[body_start..body_start + body_size];
            let kind = if header.u32(0x10) != u32::MAX {
                BankKind::Ps2Boot
            } else if module == "PS2LOGO" || effect_count(header) == Some(5) {
                BankKind::Ps2Logo
            } else {
                BankKind::Ps2Menu
            };
            let mut source = provenance(file, module, version, start, decoded);
            source.body_offset = Some(body_start);
            match self.add_sshd(header, body, kind, source.clone(), tables.clone()) {
                Ok(key) if kind == BankKind::Ps2Boot => boot = Some(key),
                Ok(key) if kind == BankKind::Ps2Logo => self.add_chord(key, source),
                Ok(_) => {}
                Err(e) => self.diagnostics.push(format!("{} {module}: {e}", file.display())),
            }
        }
        if sequences {
            if let Some(key) = boot {
                let starts: Vec<_> = data
                    .windows(4)
                    .enumerate()
                    .filter(|(_, w)| *w == b"SSsq")
                    .filter_map(|(i, _)| i.checked_sub(12))
                    .filter(|&i| sequence_extent(&data[i..]).is_some())
                    .collect();
                if starts.len() != SEQUENCES.len() {
                    self.diagnostics.push(format!(
                        "{} {module}: found {} inline scores, expected eight",
                        file.display(),
                        starts.len()
                    ));
                }
                for (&start, name) in starts.iter().zip(SEQUENCES) {
                    if let Err(e) = self.add_sequence(
                        &data[start..],
                        name,
                        key.clone(),
                        provenance(file, module, version, start, decoded),
                    ) {
                        self.diagnostics.push(format!("{} {module}/{name}: {e}", file.display()));
                    }
                }
            }
        }
    }

    fn add_sshd(
        &mut self,
        header: &[u8],
        body: &[u8],
        kind: BankKind,
        source: Provenance,
        tables: Option<DriverTables>,
    ) -> Result<BankKey> {
        let size = header_extent(header).ok_or_else(|| corrupt("invalid SShd header extent"))?;
        let header = &header[..size];
        let body_size = header.u32(4) as usize;
        let body = body.get(..body_size).ok_or_else(|| corrupt("truncated SShd body"))?;
        let mut bench = SoundBench::from_parts(header, body, tables)?;
        for p in &bench.programs {
            for t in &p.tones {
                if t.sample_index >= bench.samples.len() {
                    return Err(corrupt("missing program sample"));
                }
            }
        }
        let native = crate::sound::SoundBank::new(header, body.to_vec())?;
        for p in native.programs.values() {
            for t in &p.tones {
                if !valid_adpcm(body, t.sample_offset) {
                    return Err(corrupt("invalid program ADPCM stream"));
                }
            }
        }
        if let Some(count) = effect_count(header) {
            let table = header.u32(0x2C) as usize;
            for i in 0..count {
                if !valid_adpcm(body, header.u32(table + 0x20 + i * 0x40 + 16) as usize) {
                    return Err(corrupt("invalid effect ADPCM stream"));
                }
            }
        }
        if kind == BankKind::Ps2Logo {
            bench.bank = BankRef::Ps2Logo;
        }
        let label = match kind {
            BankKind::Ps2Boot => "PS2 boot bank",
            BankKind::Ps2Logo => "PS2 logo bank",
            _ => "PS2 menu effects",
        };
        Ok(self.add_bank(header, body, bench, kind, label, source))
    }

    fn add_bank(
        &mut self,
        header: &[u8],
        body: &[u8],
        mut bench: SoundBench,
        kind: BankKind,
        label: &str,
        source: Provenance,
    ) -> BankKey {
        let console = if kind == BankKind::Ps1Shell { Console::Ps1 } else { Console::Ps2 };
        let key = BankKey::new(fingerprint("bank", &[header, body]), label);
        bench.key = Some(key.clone());
        let content: Vec<_> = header.iter().chain(body).copied().collect();
        for bank in &mut self.banks {
            if bank.header == header && bank.body == body {
                bank.sources.push(source);
                return bank.key.clone();
            }
            if let Some(v) = bank.variants.iter_mut().find(|v| v.content == content) {
                v.sources.push(source.clone());
                bank.sources.push(source);
                return v.key.clone();
            }
            // [V] B04/B05: identical body/master/first twelve effect records, different
            // complete headers; sound_inventory.md §"Banks, sample sets, and exact pairing".
            if kind == BankKind::Ps2Menu
                && bank.kind == kind
                && bank.body == body
                && shared_menu_headers(&bank.header, header)
            {
                let mut key = key;
                if bench.effects.len() < bank.bench.effects.len() {
                    key.label = "PS2 1.00 J menu effects (12-entry header)".into();
                    bench.key = Some(key.clone());
                    bank.variants.push(BankVariant { key: key.clone(), sources: vec![source.clone()], bench, content });
                } else {
                    let mut old_key = bank.key.clone();
                    old_key.label = "PS2 1.00 J menu effects (12-entry header)".into();
                    let mut old_bench = bank.bench.clone();
                    old_bench.key = Some(old_key.clone());
                    bank.variants.push(BankVariant {
                        key: old_key,
                        sources: bank.sources.clone(),
                        bench: old_bench,
                        content: bank.header.iter().chain(&bank.body).copied().collect(),
                    });
                    bank.header = header.to_vec();
                    bank.key = key.clone();
                    bank.bench = bench;
                }
                bank.sources.push(source);
                return key;
            }
        }
        self.banks.push(LibraryBank {
            key: key.clone(),
            console,
            kind,
            sources: vec![source],
            bench,
            kit: None,
            variants: Vec::new(),
            header: header.to_vec(),
            body: body.to_vec(),
        });
        key
    }

    fn scan_shell(
        &mut self,
        data: &[u8],
        source: Provenance,
        shell: Option<&Ps1Shell>,
        intro: Option<&SceIntro>,
    ) -> Result<()> {
        let mut key = None;
        for (offset, _) in data.windows(4).enumerate().filter(|(_, w)| *w == b"pBAV") {
            let Some((header_size, size)) = vab_extent(&data[offset..]) else { continue };
            let bytes = &data[offset..offset + size];
            let vab = Vab::parse(bytes)?;
            let mut occurrence = source.clone();
            occurrence.offset = offset;
            occurrence.body_offset = Some(offset + header_size);
            let found = self.add_bank(
                &bytes[..header_size],
                &bytes[header_size..],
                SoundBench::ps1(&vab),
                BankKind::Ps1Shell,
                "PS1 shell VAB",
                occurrence,
            );
            key = Some(found);
        }
        let key = key.ok_or_else(|| corrupt("no valid complete shell VAB (damaged/unavailable source)"))?;
        for (offset, events, size) in note_tables(data) {
            let (id, label, role) = match events.len() {
                19 if events.iter().all(|e| matches!(e.prog, 0 | 3)) => {
                    ("Q08", "PlayStation licence sting", "licence-screen note table")
                }
                40 if events.iter().all(|e| matches!(e.prog, 1 | 3)) => {
                    ("Q09", "Sony Computer Entertainment intro", "Sony Computer Entertainment intro table")
                }
                _ => {
                    self.diagnostics.push(format!(
                        "{} {}: unlisted note table at {offset:#x} ({} events)",
                        source.file.display(),
                        source.module,
                        events.len()
                    ));
                    continue;
                }
            };
            let mut occurrence = source.clone();
            occurrence.offset = offset;
            let content = data[offset..offset + size].to_vec();
            let length_s = f64::from(events.last().map_or(0, |e| e.time)) / 60.0;
            let mut variants = Vec::new();
            for video in [VideoMode::Ntsc, VideoMode::Pal] {
                let song = if id == "Q08" {
                    shell.and_then(|s| Song::from_ps1_licence(s, video).ok())
                } else {
                    intro.and_then(|s| Song::from_ps1_intro(s, video).ok())
                };
                if let Some(mut song) = song {
                    bind_song(&mut song, &key);
                    variants.push(TimingVariant {
                        label: format!("Console timing {video:?} ({}) [I]", source.version),
                        source: occurrence.clone(),
                        song,
                    });
                }
            }
            self.add_piece(LibraryPiece {
                key: fingerprint("table", &[&content]),
                label: label.into(),
                role: role.into(),
                evidence: Evidence::Verified,
                console: Console::Ps1,
                bank: key.clone(),
                sources: vec![occurrence],
                length_s,
                aliases: vec![id.into()],
                console_timing: variants,
                loops: false,
                score: Score::Table(events),
                content,
            });
        }
        Ok(())
    }

    fn scan_vags(&mut self, data: &[u8], source: Provenance) {
        for (offset, _) in data.windows(4).enumerate().filter(|(_, w)| *w == b"VAGp") {
            let d = &data[offset..];
            if d.len() < 0x30 {
                continue;
            }
            let size = u32::from_be_bytes(d[12..16].try_into().unwrap_or_default()) as usize;
            let rate = u32::from_be_bytes(d[16..20].try_into().unwrap_or_default());
            let Some(body) = d.get(0x30..0x30usize.saturating_add(size)) else { continue };
            if !(1..=192000).contains(&rate) || size == 0 || !size.is_multiple_of(16) || !valid_adpcm(body, 0) {
                continue;
            }
            let (pcm, lp) = decode_adpcm(body, 0);
            let bench = SoundBench::from_sample(pcm, lp, rate);
            let mut occurrence = source.clone();
            occurrence.offset = offset;
            occurrence.body_offset = Some(offset + 0x30);
            self.add_bank(&d[..0x30], body, bench, BankKind::TestSine, "TESTSPU sine", occurrence);
            if source.module != "TESTSPU" && source.module != "file" {
                self.diagnostics.push(format!("{}: unlisted loose VAG at {offset:#x}", source.file.display()));
            }
        }
    }

    fn add_sequence(&mut self, data: &[u8], name: &str, bank: BankKey, source: Provenance) -> Result<()> {
        let size = sequence_extent(data)
            .ok_or_else(|| Error::Corrupt(Format::Sequence, "missing/truncated SSsq terminator".into()))?;
        let content = data[..size].to_vec();
        let seq = SoundSequence::new(&content)?;
        let (id, label, role, evidence) = sequence_role(name);
        let length_s = seq.events.last().map_or(0.0, |e| e.tick as f64 * 60.0 / seq.bpm as f64 / seq.resolution as f64);
        let loops = seq.events.iter().any(|e| e.kind == EventKind::Control(99, 20));
        self.add_piece(LibraryPiece {
            key: fingerprint("sequence", &[&content]),
            label: label.into(),
            role: role.into(),
            evidence,
            console: Console::Ps2,
            bank,
            sources: vec![source],
            length_s,
            aliases: vec![id.into(), name.into()],
            console_timing: Vec::new(),
            loops,
            score: Score::Sequence(Box::new(seq)),
            content,
        });
        Ok(())
    }
    fn add_chord(&mut self, bank: BankKey, source: Provenance) {
        let content = b"simultaneous direct effects 0..4".to_vec();
        self.add_piece(LibraryPiece {
            key: fingerprint("recipe", &[bank.fingerprint.as_bytes(), &content]),
            label: "PS2 game-logo five-effect chord".into(),
            role: "PS2 logo's five-effect chord".into(),
            evidence: Evidence::Verified,
            console: Console::Ps2,
            bank,
            sources: vec![source],
            length_s: 0.0,
            aliases: vec!["R01".into()],
            console_timing: Vec::new(),
            loops: false,
            score: Score::LogoChord,
            content,
        });
    }
    fn add_piece(&mut self, piece: LibraryPiece) {
        if let Some(old) =
            self.pieces.iter_mut().find(|p| p.bank == piece.bank && p.content == piece.content && p.key == piece.key)
        {
            old.sources.extend(piece.sources);
            for alias in piece.aliases {
                if !old.aliases.contains(&alias) {
                    old.aliases.push(alias);
                }
            }
            old.console_timing.extend(piece.console_timing);
        } else {
            self.pieces.push(piece);
        }
    }
}

// [V]/[I] Roles are the table in sound_inventory.md §"Every stored score, its bank,
// and its role". Names determine registrations, not alleged unverified timer triggers.
const SEQUENCES: [&str; 8] =
    ["SNDBOOTS", "SNDTNNLS", "SNDCLOKS", "SNDTM30S", "SNDTM60S", "SNDLOGOS", "SNDWARNS", "SNDRCLKS"];
fn sequence_role(name: &str) -> (&str, &str, &str, Evidence) {
    match name {
        "SNDBOOTS" => ("Q01", "Normal boot opening", "Normal boot opening", Evidence::Verified),
        "SNDTNNLS" => ("Q02", "Tunnel transition [I]", "Opening-to-menu transition", Evidence::Verified),
        "SNDCLOKS" => ("Q03", "Clock/main-menu ambience", "Clock/main-menu ambience", Evidence::Verified),
        "SNDTM30S" | "SNDLOGOS" => (
            "Q04",
            "SNDTM30S / SNDLOGOS registered cue",
            "registered cue; trigger role unverified",
            Evidence::RegisteredCue,
        ),
        "SNDTM60S" => {
            ("Q05", "SNDTM60S registered cue", "registered cue; trigger role unverified", Evidence::RegisteredCue)
        }
        "SNDWARNS" => ("Q06", "Warning ambience [I]", "Alternative/warning opening", Evidence::Verified),
        _ => ("Q07", "CD-media transition [I]", "Transition selected for disc types 0x6C..0x6E", Evidence::Verified),
    }
}
fn bind_song(song: &mut Song, key: &BankKey) {
    for c in &mut song.channels {
        c.instrument.bank = key.clone();
    }
}
fn provenance(file: &Path, module: &str, version: &str, offset: usize, decoded: bool) -> Provenance {
    Provenance {
        file: file.into(),
        module: module.into(),
        offset,
        version: version.into(),
        decoded,
        body_module: None,
        body_offset: None,
    }
}
fn corrupt(message: &str) -> Error {
    Error::Corrupt(Format::SoundBank, message.into())
}
fn fingerprint(domain: &str, parts: &[&[u8]]) -> String {
    let mut hash = 0x6c62272e07bb014262b821756295c58d_u128;
    for part in std::iter::once(domain.as_bytes()).chain(parts.iter().copied()) {
        for byte in (part.len() as u64).to_le_bytes().iter().chain(part) {
            hash ^= u128::from(*byte);
            hash = hash.wrapping_mul(0x0000000001000000000000000000013b);
        }
    }
    format!("{hash:032x}")
}
fn effect_count(header: &[u8]) -> Option<usize> {
    if header.len() < 0x30 {
        return None;
    }
    let table = header.u32(0x2C) as usize;
    let prefix = header.get(table..table.checked_add(8)?)?;
    let count = (prefix.u32(4) as usize).checked_add(1)?;
    (count <= 8192).then_some(count)
}
fn header_extent(data: &[u8]) -> Option<usize> {
    if data.len() < 0x30 || &data[12..16] != b"SShd" {
        return None;
    }
    let size = if data.u32(0) == u32::MAX {
        (data.u32(0x2C) as usize).checked_add(0x20)?.checked_add(effect_count(data)?.checked_mul(0x40)?)?
    } else {
        data.u32(0) as usize
    };
    let header = data.get(..size)?;
    if !(0x31..=0x10000).contains(&size) {
        return None;
    }
    let base = header.u32(0x10) as usize;
    if base != u32::MAX as usize {
        let max = header.get(base..base.checked_add(2)?)?.u16(0) as usize;
        if max > 127 || base.checked_add(2 + (max + 1) * 2)? > size {
            return None;
        }
        for p in 0..=max {
            let offset = header.u16(base + 2 + p * 2) as usize;
            if offset == 0xFFFF {
                continue;
            }
            let program = base.checked_add(offset)?;
            let prefix = header.get(program..program.checked_add(8)?)?;
            let tones = if prefix[0] == 255 { 0 } else { usize::from(prefix[0] & 127) + 1 };
            if program.checked_add(8 + tones * 16)? > size {
                return None;
            }
        }
    }
    if header.u32(0x2C) != u32::MAX
        && (header.u32(0x2C) as usize).checked_add(0x20 + effect_count(header)? * 0x40)? > size
    {
        return None;
    }
    Some(size)
}
fn inline_banks(data: &[u8]) -> Vec<(usize, usize, usize, usize)> {
    data.windows(4)
        .enumerate()
        .filter(|(_, w)| *w == b"SShd")
        .filter_map(|(signature, _)| {
            let start = signature.checked_sub(12)?;
            let header_size = header_extent(&data[start..])?;
            let body_size = data[start..].u32(4) as usize;
            if body_size == 0 || body_size > 64 << 20 {
                return None;
            }
            let body_start = start.checked_add((header_size + 63) & !63)?;
            data.get(body_start..body_start.checked_add(body_size)?)?;
            Some((start, header_size, body_start, body_size))
        })
        .collect()
}
fn shared_menu_headers(a: &[u8], b: &[u8]) -> bool {
    let (Some(ac), Some(bc)) = (effect_count(a), effect_count(b)) else { return false };
    if !matches!((ac, bc), (12, 14) | (14, 12)) {
        return false;
    }
    let (at, bt) = (a.u32(0x2C) as usize, b.u32(0x2C) as usize);
    a.get(at..at + 4) == b.get(bt..bt + 4)
        && a.get(at + 0x20..at + 0x20 + 12 * 0x40) == b.get(bt + 0x20..bt + 0x20 + 12 * 0x40)
}
fn valid_adpcm(body: &[u8], offset: usize) -> bool {
    let Some(data) = body.get(offset..) else { return false };
    for block in data.chunks_exact(16) {
        if block[0] >> 4 > 4 || block[0] & 15 > 12 || block[1] > 7 {
            return false;
        }
        if block[1] & 1 != 0 {
            return true;
        }
    }
    false
}
fn vab_extent(data: &[u8]) -> Option<(usize, usize)> {
    if data.len() < 32 || &data[..4] != b"pBAV" {
        return None;
    }
    let (programs, samples) = (data.u16(18) as usize, data.u16(22) as usize);
    if programs == 0 || programs > 128 || samples == 0 || samples > 254 {
        return None;
    }
    let table = 32 + 128 * 16 + programs * 512;
    let header = table + 512;
    data.get(..header)?;
    let mut end = header;
    for i in 1..=samples {
        let size = data.u16(table + i * 2) as usize * 8;
        if size == 0 || !size.is_multiple_of(16) {
            return None;
        }
        let sample = data.get(end..end.checked_add(size)?)?;
        if !valid_adpcm(sample, 0) {
            return None;
        }
        end += size;
    }
    (end == data.u32(12) as usize).then_some((header, end))
}
fn note_tables(data: &[u8]) -> Vec<(usize, Vec<NoteEvent>, usize)> {
    let mut result = Vec::new();
    for end in (0..data.len().saturating_sub(7)).step_by(4).filter(|&i| data.u32(i) == 9999) {
        for count in [19, 40] {
            let Some(start) = end.checked_sub(count * 8) else { continue };
            let mut events: Vec<NoteEvent> = Vec::new();
            for chunk in data[start..end].chunks_exact(8) {
                let e = NoteEvent { time: chunk.u32(0), prog: chunk[4], note: chunk[5], vel: chunk[6], pan: chunk[7] };
                if e.time >= 9999
                    || e.prog > 3
                    || e.note > 127
                    || e.vel > 127
                    || e.pan > 127
                    || events.last().is_some_and(|p| p.time > e.time)
                {
                    break;
                }
                events.push(e);
            }
            if events.len() == count && events.first().is_some_and(|e| e.time == 0 && e.vel > 0) {
                result.push((start, events, count * 8 + 8));
            }
        }
    }
    result
}
// SSsq is MIDI-like but has two-byte BPM events, one-byte coarse bends, and no
// initial delta. Compute the true terminator extent; never hash allocation padding.
fn sequence_extent(data: &[u8]) -> Option<usize> {
    if data.len() <= 0x110 || &data[12..16] != b"SSsq" {
        return None;
    }
    let (mut pos, mut status) = (0x110, 0u8);
    for _ in 0..32768 {
        let byte = *data.get(pos)?;
        if byte & 128 != 0 {
            status = byte;
            pos += 1;
        }
        let size = match status {
            0x80..=0xBF => 2,
            0xC0..=0xCF | 0xE0..=0xEF => 1,
            0xFF => match data.get(pos)? {
                0x2F => {
                    if data.get(pos + 1)? != &0 {
                        return None;
                    }
                    return Some(pos + 2);
                }
                0x51 => 3,
                _ => return None,
            },
            _ => return None,
        };
        data.get(pos..pos.checked_add(size)?)?;
        pos += size;
        let mut finished = false;
        for _ in 0..4 {
            let byte = *data.get(pos)?;
            pos += 1;
            if byte & 128 == 0 {
                finished = true;
                break;
            }
        }
        if !finished {
            return None;
        }
    }
    None
}
