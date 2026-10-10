//! MechaCon EEPROM and BIOS companions, decoded from the user's files.
//!
//! Field locations and command/checksum rules: `rom1_erom_nvm.md` §§3–5.
//! The IOP checks an **8-bit** sum, not PCSX2's default 7-bit sum. TESTMODE is a
//! live CDVD status bit (`kernel_eeload_hooks.md` §1.4), not an EEPROM field.

use crate::rom::RomDir;
use crate::{Error, Format, Result};
use std::path::{Path, PathBuf};

fn corrupt(message: impl Into<String>) -> Error { Error::Corrupt(Format::Bios, message.into()) }
fn blank(bytes: &[u8]) -> bool { bytes.iter().all(|&b| b == 0) || bytes.iter().all(|&b| b == 0xFF) }

/// ROMVER identity; its decimal minor chooses PCSX2's layout (`rom1_erom_nvm.md` §3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RomVersion {
    /// The fourteen significant ROMVER characters.
    pub text: String,
    /// Decimal major version.
    pub major: u8,
    /// Decimal minor version (70, not hexadecimal 0x70).
    pub minor: u8,
    /// ROMVER's sales-region letter.
    pub region: char,
    /// ROMVER's console-type letter.
    pub console_type: char,
    /// Build date, YYYYMMDD.
    pub date: String,
}

impl RomVersion {
    /// Reads ROMVER, accepting its trailing newline/NUL; rejects truncated or non-ASCII identities.
    pub fn parse(bytes: impl AsRef<[u8]>) -> Result<Self> {
        let b = bytes.as_ref().get(..14).ok_or_else(|| corrupt("truncated ROMVER"))?;
        if !b[..4].iter().chain(&b[6..]).all(u8::is_ascii_digit)
            || !b[4..6].iter().all(u8::is_ascii_uppercase) {
            return Err(corrupt("invalid ROMVER identity"));
        }
        let text = String::from_utf8(b.to_vec()).map_err(|_| corrupt("non-ASCII ROMVER"))?;
        Ok(Self { major: (b[0] - b'0') * 10 + b[1] - b'0', minor: (b[2] - b'0') * 10 + b[3] - b'0',
            region: b[4] as char, console_type: b[5] as char, date: text[6..14].into(), text })
    }

    fn at_least(&self, major: u8, minor: u8) -> bool { (self.major, self.minor) >= (major, minor) }
}

impl AsRef<[u8]> for RomVersion {
    fn as_ref(&self) -> &[u8] { self.text.as_bytes() }
}

/// EEPROM locations, approximated by ROM version, not a guarantee of MechaCon generation.
/// `rom1_erom_nvm.md` §3.2 verifies both layouts in real dumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct NvmLayout {
    /// PCSX2 layout number, 0 before 1.70, otherwise 1.
    pub number: u8,
    /// Bases of areas 0, 1 (OSD) and 2 (DVD settings).
    pub config_bases: [usize; 3],
    /// Sixteen-byte model-name slot.
    pub model_offset: usize,
    /// Eight-byte console-ID slot, excluding unknown trailers.
    pub console_id_offset: usize,
    /// Eight-byte i.LINK-ID slot, excluding unknown trailers.
    pub ilink_id_offset: usize,
}

impl NvmLayout {
    /// Selects the layout using the decimal 1.70 threshold (`rom1_erom_nvm.md` §3.2).
    #[must_use]
    pub fn for_rom(version: &RomVersion) -> Self {
        if version.at_least(1, 70) {
            Self { number: 1, config_bases: [0x270, 0x2B0, 0x200], model_offset: 0x1B0,
                console_id_offset: 0x1F0, ilink_id_offset: 0x1E0 }
        } else {
            Self { number: 0, config_bases: [0x280, 0x300, 0x200], model_offset: 0x1A0,
                console_id_offset: 0x1C8, ilink_id_offset: 0x1C0 }
        }
    }
}

/// Occupancy-based provenance; this is evidence, not authentication (`rom1_erom_nvm.md` §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NvmSource {
    /// Entire image is zero or erased.
    Blank,
    /// Only emulator-populated regions contain data; consistent with PCSX2 fabrication.
    Fabricated,
    /// Calibration/model/key or DVD-setting regions contain data, consistent with a dump.
    Populated,
}

/// Privacy-preserving ID classification (`rom1_erom_nvm.md` §§3.3–3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdState {
    /// All zero or erased.
    Blank,
    /// Present in an otherwise fabricated image; authenticity cannot be established.
    Fabricated,
    /// Nonblank; authenticity cannot be established.
    Present,
}

/// An ID summary with no backing identifier bytes, including in Debug output.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MaskedId {
    /// Presence/fabrication evidence only.
    pub state: IdState,
    /// Twelve asterisks and at most the last two bytes, or a blank/fabricated placeholder.
    pub masked: String,
}

fn masked_id(bytes: &[u8], fabricated: bool) -> MaskedId {
    let state = if blank(bytes) { IdState::Blank } else if fabricated { IdState::Fabricated } else { IdState::Present };
    let masked = match state {
        IdState::Blank => "(blank)".into(),
        IdState::Fabricated => "****************".into(),
        IdState::Present => format!("************{:02X}{:02X}", bytes[6], bytes[7]),
    };
    MaskedId { state, masked }
}

/// One 15-byte config record and its full 8-bit additive checksum (`rom1_erom_nvm.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConfigBlock {
    /// MechaCon config area, 0–2.
    pub area: u8,
    /// Block index within that area.
    pub index: u8,
    /// Location in the 1 KiB image.
    pub offset: usize,
    /// The fifteen bytes returned to the EE, excluding checksum.
    pub data: [u8; 15],
    /// Stored checksum byte.
    pub stored_checksum: u8,
    /// Sum of all fifteen bytes modulo 256.
    pub expected_checksum: u8,
}

impl ConfigBlock {
    /// Whether the IOP accepts this record; erased FF records are not automatically valid.
    #[must_use]
    pub fn valid(&self) -> bool { self.stored_checksum == self.expected_checksum }

    /// Whether the checksum dropped bit 7, like PCSX2's defaults (`rom1_erom_nvm.md` §3.4).
    #[must_use]
    pub fn has_seven_bit_checksum(&self) -> bool {
        !self.valid() && self.stored_checksum == self.expected_checksum & 0x7F
    }
}

/// OSD fields from RPC bytes 0x0F–0x14 (`osdsys_hooks.md` §4.1).
/// Extended fields are ignored by 1.00; raw date/time codes retain their unknown semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OsdSettings {
    /// Whether this ROM supports the later config/wizard fields.
    pub extended: bool,
    /// Digital Out enabled; stored bit 0 is the disable flag.
    pub digital_out: bool,
    /// Screen type: 0 = 4:3, 1 = Full, 2 = 16:9; 3 is reserved.
    pub screen_type: u8,
    /// Component YPbPr rather than RGB output (bit 3).
    pub component_video: bool,
    /// Stored menu-language code, or None for an all-zero/default block.
    /// Later ROMs also validate this against their region; this is not an acceptance verdict.
    pub language: Option<u8>,
    /// Bits 5–7 of byte 0x0F: nonzero selects the extended language encoding.
    pub layout_marker: u8,
    /// OSD initialised flag; absent on 1.00.
    pub initialised: Option<bool>,
    /// Signed eleven-bit time-zone offset in minutes.
    pub timezone_minutes: i16,
    /// Nine-bit time-zone index; >= 0x80 is displayed as custom (`rom1_erom_nvm.md` §3.3).
    pub timezone_index: u16,
    /// Summer-time flag.
    pub summer_time: bool,
    /// Raw time-format bit, retained without assigning unverified display labels.
    pub time_format: u8,
    /// Raw date-format code, 0–3.
    pub date_format: u8,
    /// DVD player's progressive-setting flag (2.00 only).
    pub progressive: bool,
    /// Show the Remote Control menu item (2.00 only).
    pub remote_item: bool,
    /// Remote Control enabled; the command sends its inverse (2.00 only).
    pub remote_enabled: bool,
    /// Byte 0x13 bit 7, value id 0x11; consumer not found in the notes.
    pub unknown_option: bool,
}

impl OsdSettings {
    fn decode(b: &[u8; 15], version: &RomVersion) -> Self {
        let extended = version.at_least(1, 50);
        let tz = (((b[2] & 7) as i16) << 8) | b[3] as i16;
        Self { extended, digital_out: b[0] & 1 == 0, screen_type: (b[0] >> 1) & 3,
            component_video: b[0] & 8 != 0, layout_marker: b[0] >> 5,
            language: if extended && b.iter().all(|&v| v == 0) { None }
                else { Some(if !extended || b[0] & 0xE0 == 0 { (b[0] >> 4) & 1 } else { b[1] & 0x1F }) },
            initialised: extended.then_some(b[2] & 0x80 != 0),
            timezone_minutes: if tz & 0x400 != 0 { tz - 0x800 } else { tz },
            timezone_index: (((b[4] & 1) as u16) << 8) | b[5] as u16,
            summer_time: b[2] & 8 != 0, time_format: (b[2] >> 4) & 1, date_format: (b[2] >> 5) & 3,
            progressive: b[4] & 0x10 != 0, remote_item: b[4] & 0x20 != 0,
            remote_enabled: b[4] & 0x40 != 0, unknown_option: b[4] & 0x80 != 0 }
    }

    /// Whether the first-boot wizard runs; 1.00 has none (`osdsys_hooks.md` §4.1).
    #[must_use]
    pub fn setup_required(&self) -> bool { self.initialised == Some(false) }

    /// Human-readable stored language; the ROM's region validation may substitute its default.
    #[must_use]
    pub fn language_name(&self) -> &'static str {
        match self.language {
            Some(0) => "Japanese", Some(1) => "English", Some(2) => "French", Some(3) => "Spanish",
            Some(4) => "German", Some(5) => "Italian", Some(6) => "Dutch", Some(7) => "Portuguese",
            Some(8) => "Russian", Some(9) => "Korean", Some(10) => "Traditional Chinese",
            Some(11) => "Simplified Chinese", Some(_) => "invalid code (ROM default)", None => "region default",
        }
    }

    /// Screen-type label, retaining reserved values.
    #[must_use]
    pub fn screen_name(&self) -> &'static str {
        match self.screen_type { 0 => "4:3", 1 => "Full", 2 => "16:9", _ => "reserved" }
    }
}

/// Valid PStwo region parameters; older dumps may use a different, undecoded structure.
/// `rom1_erom_nvm.md` §3.3: file byte 0x186, not the PS1 letter, selects DVD suffixes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RegionParams {
    /// Printable seven-character prefix, e.g. ROM/OSD/language/PS1/DVD.
    pub text: String,
    /// ROM region and PS1VER suffix (byte 0).
    pub rom_region: char,
    /// OSD region (byte 1).
    pub osd_region: char,
    /// Three-letter default menu-language code (bytes 2–4).
    pub language: String,
    /// PS1 BIOS's region letter (byte 5); not the PS1VER selector.
    pub ps1_region: char,
    /// DVDID/DVDVER/EROMDRV/DVDPL suffix (byte 6).
    pub dvd_region: char,
}

impl RegionParams {
    fn parse(b: &[u8]) -> Option<Self> {
        if !b[..2].iter().chain(&b[5..7]).all(u8::is_ascii_uppercase)
            || !b[2..5].iter().all(u8::is_ascii_lowercase) { return None }
        Some(Self { text: String::from_utf8_lossy(&b[..7]).into(), rom_region: b[0] as char,
            osd_region: b[1] as char, language: String::from_utf8_lossy(&b[2..5]).into(),
            ps1_region: b[5] as char, dvd_region: b[6] as char })
    }
}

/// A boot consequence and the note supporting it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Prediction {
    /// Consequence, including any hardware/emulator condition.
    pub message: String,
    /// Research note and section; predictions do not claim execution on hardware.
    pub citation: &'static str,
}

fn prediction(message: impl Into<String>, citation: &'static str) -> Prediction {
    Prediction { message: message.into(), citation }
}

/// Decoded EEPROM, with no raw console IDs or key-material region retained.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Nvm {
    /// ROM identity used for interpretation.
    pub romver: RomVersion,
    /// Selected PCSX2-compatible location map.
    pub layout: NvmLayout,
    /// Occupancy evidence, not proof of authenticity.
    pub source: NvmSource,
    /// Areas 0/1/2, respectively 4/2/7 config records (`rom1_erom_nvm.md` §3.2).
    pub blocks: Vec<ConfigBlock>,
    /// OSD fields before checksum rejection.
    pub stored_osd: OsdSettings,
    /// OSD fields with a rejected area-1 read treated as zero for the documented PCSX2 retry path.
    pub osd: OsdSettings,
    /// Whether either OSD config record fails the IOP checksum.
    pub osd_rejected: bool,
    /// Thirty three-bit option values (even = low nibble), from accepted area-1 block 0.
    pub option_nibbles: [u8; 30],
    /// PS1-driver Disc Speed Fast bit; meaning inferred in `osdsys_hooks.md` §4.1.
    pub ps1_fast: bool,
    /// PS1-driver Texture Mapping Smooth bit; meaning inferred in the same section.
    pub ps1_smooth: bool,
    /// Printable model name; blank or malformed names become None.
    pub model: Option<String>,
    /// Recognisable PStwo parameters, otherwise None (including older binary parameters).
    pub region_params: Option<RegionParams>,
    /// Whether the raw twelve-byte region slot contains nonblank, undecoded data.
    pub unknown_region_params: bool,
    /// Masked console-ID presence/fabrication evidence.
    pub console_id: MaskedId,
    /// Masked i.LINK-ID presence/fabrication evidence.
    pub ilink_id: MaskedId,
}

impl Nvm {
    /// Decodes exactly 1024 bytes against ROMVER; checksum failures are findings, not parse errors.
    /// `rom1_erom_nvm.md` §§3.1–3.4. No full IDs or key-material region bytes are retained.
    pub fn parse(bytes: impl AsRef<[u8]>, romver: impl AsRef<[u8]>) -> Result<Self> {
        let b = bytes.as_ref();
        if b.len() != 1024 { return Err(corrupt(format!("NVM must be 1024 bytes, got {}", b.len()))) }
        let romver = RomVersion::parse(romver)?;
        let layout = NvmLayout::for_rom(&romver);
        let mut blocks = Vec::with_capacity(13);
        for (area, count) in [4, 2, 7].into_iter().enumerate() {
            for index in 0..count {
                let offset = layout.config_bases[area] + index * 16;
                let mut data = [0; 15];
                data.copy_from_slice(&b[offset..offset + 15]);
                let expected_checksum = data.iter().fold(0u8, |sum, &v| sum.wrapping_add(v));
                blocks.push(ConfigBlock { area: area as u8, index: index as u8, offset,
                    data, stored_checksum: b[offset + 15], expected_checksum });
            }
        }
        let model_bytes = &b[layout.model_offset..layout.model_offset + 16];
        let name = model_bytes.split(|&v| v == 0).next().unwrap_or_default();
        let model = (!name.is_empty() && name.iter().all(|v| v.is_ascii_graphic() || *v == b' '))
            .then(|| String::from_utf8_lossy(name).into_owned());
        let source = if blank(b) { NvmSource::Blank }
            else if model.is_some() || b[..0x180].iter().any(|&v| v != 0) || b[0x320..].iter().any(|&v| v != 0)
                || blocks.iter().filter(|v| v.area != 1).any(|v| v.data.iter().any(|&v| v != 0)) { NvmSource::Populated }
            else { NvmSource::Fabricated };
        let osd_rejected = !blocks[4].valid() || !blocks[5].valid();
        let options = if osd_rejected { [0; 15] } else { blocks[4].data };
        let effective = if osd_rejected { [0; 15] } else { blocks[5].data };
        let stored_osd = OsdSettings::decode(&blocks[5].data, &romver);
        let osd = OsdSettings::decode(&effective, &romver);
        let mut option_nibbles = [0; 30];
        for (i, value) in option_nibbles.iter_mut().enumerate() { *value = (options[i / 2] >> ((i % 2) * 4)) & 7; }
        let region_params = RegionParams::parse(&b[0x180..0x18C]);
        let unknown_region_params = region_params.is_none() && !blank(&b[0x180..0x18C]);
        let console_id = masked_id(&b[layout.console_id_offset..layout.console_id_offset + 8], source == NvmSource::Fabricated);
        let ilink = &b[layout.ilink_id_offset..layout.ilink_id_offset + 8];
        // Real IDs can share the default's shape: shape alone is not evidence of fabrication.
        let ilink_id = masked_id(ilink, source == NvmSource::Fabricated);
        Ok(Self { romver, layout, source, blocks, stored_osd, osd, osd_rejected, option_nibbles,
            ps1_fast: options[0] & 1 != 0, ps1_smooth: options[0] & 0x10 != 0, model,
            region_params, unknown_region_params, console_id, ilink_id })
    }

    /// TESTMODE dispatch from live CDVD status bit 4, not this NVM (`kernel_eeload_hooks.md` §1.4).
    /// None means unknown status; the 1.00 ROM has no TESTMODE dispatch.
    #[must_use]
    pub fn testmode(&self, cdvd_status: Option<u8>) -> Option<bool> {
        if !self.romver.at_least(1, 50) { Some(false) } else { cdvd_status.map(|v| v & 0x10 != 0) }
    }

    /// Reconstructs EECONF's 0x60-byte allocation before its conditional EEPROM write.
    /// `kernel_eeload_hooks.md` §1.5: six 15-byte records, bit-3 mirror into cache byte 0x0F;
    /// any bad checksum removes the cache pointer. The last six allocation bytes are modelled as zero.
    /// The 1.00/1.50 eeconfig implementation was not traced, so this returns None for those ROMs.
    #[must_use]
    pub fn eeconf_cache(&self) -> Option<[u8; 0x60]> {
        if !self.romver.at_least(1, 60) || self.blocks[..6].iter().any(|v| !v.valid()) { return None }
        let mut out = [0; 0x60];
        for (i, block) in self.blocks[..6].iter().enumerate() { out[i * 15..i * 15 + 15].copy_from_slice(&block.data); }
        out[0x0F] = (out[0x0F] & !8) | (out[0x4B] & 8);
        Some(out)
    }

    /// Boot consequences with unknown MechaCon version; suffix predictions retain that condition.
    #[must_use]
    pub fn predictions(&self) -> Vec<Prediction> { self.predictions_with_mec(None) }

    /// Boot consequences using an optional MEC reply (`rom1_erom_nvm.md` §§3–5).
    /// Invalid-record zeroing describes PCSX2's retry path, not guaranteed real-hardware recovery.
    #[must_use]
    pub fn predictions_with_mec(&self, mec: Option<&Mec>) -> Vec<Prediction> {
        let mut out = Vec::new();
        if self.osd_rejected {
            out.push(prediction("IOP rejects bad OSD checksums; documented PCSX2 retry yields zeros. Effective decode assumes that retry; hardware recovery is unknown.", "rom1_erom_nvm.md §3.4"));
        }
        out.push(prediction(if !self.osd.extended { "No first-boot wizard exists in ROM 1.00." }
            else if self.osd.setup_required() { "First-boot wizard runs after the opening (not initialised)." }
            else { "First-boot wizard skipped (initialised)." }, "osdsys_hooks.md §4.1"));
        out.push(prediction(format!("Menu language: {}{}.", self.osd.language_name(),
            if self.osd.extended { " (subject to the ROM's region validation/default)" } else { "" }), "osdsys_hooks.md §4.1"));
        let model_blocked = mec.is_some_and(|m| m.version_number() < 0x010500);
        out.push(prediction(format!("Version page Console: {}{}.",
            if model_blocked { "Unknown" } else { self.model.as_deref().unwrap_or("Unknown") },
            if model_blocked { " (MechaCon < 1.05.00 blocks the model read)" }
            else if mec.is_none() { " (model read requires MechaCon >= 1.05.00)" } else { "" }), "rom1_erom_nvm.md §§3.1, 3.3"));
        out.push(prediction("OSDSYS reads area 1: S-commands 0x40, 0x41 x2, 0x43; saving config uses 0x40, 0x42, 0x43. Boot also applies S/PDIF.", "rom1_erom_nvm.md §3.1; osdsys_hooks.md §4.1"));
        out.push(prediction(if self.romver.at_least(1, 60) {
            "EECONF reads area 0 x4 and area 1 x2 (0x40/0x41/0x43); CDVD status bit 1 set and bit 2 clear may clear area-1 block 0 with 0x42."
        } else { "Early EECONF's eeconfig implementation/cache was not traced; the later six-record layout is not assumed." }, "kernel_eeload_hooks.md §1.5"));
        if self.romver.at_least(2, 0) {
            let enabled = mec.map(|m| m.version_number() >= 0x060000);
            if enabled != Some(false) {
                out.push(prediction(format!("S-command 0x36 reads region parameters{}.", if enabled.is_none() { " only if MechaCon >= 6.00.00" } else { "" }), "rom1_erom_nvm.md §4"));
                if let Some(p) = &self.region_params {
                    out.push(prediction(format!("{}rom0:PS1VER{}, rom1:DVDID{}, DVDVER{}, EROMDRV{}, erom0:DVDPL{}; missing suffix files fall back to unsuffixed names/DVDELF.",
                        if enabled.is_none() { "With MechaCon >= 6.00.00, selects " } else { "Selects " },
                        p.rom_region, p.dvd_region, p.dvd_region, p.dvd_region, p.dvd_region), "rom1_erom_nvm.md §§3.3, 5"));
                } else {
                    out.push(prediction("No decoded region parameters: ROMVER region and unsuffixed DVD files are the fallback.", "rom1_erom_nvm.md §5"));
                }
            } else {
                out.push(prediction("MechaCon < 6.00.00 closes 0x36: use ROMVER region and unsuffixed DVD files.", "rom1_erom_nvm.md §§4–5"));
            }
            out.push(prediction(format!("Remote Control item {}; RcBypassCtl sends !enabled = {} (RPC fno 0x30, requires MechaCon >= 5.00.00); progressive-clear item {}.",
                if self.osd.remote_item { "shown" } else { "hidden" }, u8::from(!self.osd.remote_enabled),
                if self.osd.progressive { "shown" } else { "hidden" }), "osdsys_hooks.md §4.1; rom1_erom_nvm.md §4"));
        } else if self.romver.at_least(1, 50) {
            out.push(prediction("NVM region parameters ignored; ROMVER supplies the region and DVDID/DVDVER/EROMDRV are unsuffixed.", "rom1_erom_nvm.md §5"));
        } else {
            out.push(prediction("ROM 1.00 ignores ROM1/EROM; DVD player comes from a memory card.", "rom1_erom_nvm.md §5"));
        }
        out.push(prediction(if self.testmode(None) == Some(false) { "TESTMODE is absent in ROM 1.00." }
            else { "TESTMODE needs live CDVD status bit 4; NVM cannot determine it." }, "kernel_eeload_hooks.md §1.4"));
        out.push(prediction("Retail boot does not read console/i.LINK IDs; area 2 belongs to DVD-player settings (inferred).", "rom1_erom_nvm.md §§3.1, 3.3"));
        let dvd_blocks = self.blocks.iter().filter(|b| b.area == 2 && b.valid() && b.data.iter().any(|&v| v != 0)).count();
        if dvd_blocks > 0 {
            out.push(prediction(format!("Area 2 has {dvd_blocks} nonblank valid DVD-setting records: consistent with DVD-player use on a real console (inferred)."), "rom1_erom_nvm.md §3.3"));
        }
        out
    }
}

/// Four-byte MechaCon version reply (`rom1_erom_nvm.md` §4); provenance of real MEC byte order is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Mec {
    /// Status byte; PCSX2 also interprets it as a zone index.
    pub status: u8,
    /// Major, minor, patch bytes in S-command order.
    pub version: [u8; 3],
    /// Matches PCSX2's fabricated 6.02.00 reply, status/zone index 3 (Oceania).
    pub pcsx2_default: bool,
}

/// One XCDVDMAN version gate from `rom1_erom_nvm.md` §4.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MecGate {
    /// Required major/minor/patch bytes.
    pub minimum: [u8; 3],
    /// Command path enabled by the version comparison.
    pub feature: &'static str,
    /// Whether this reply satisfies the comparison; not proof that the hardware implements it.
    pub enabled: bool,
    /// Note citation.
    pub citation: &'static str,
}

impl Mec {
    /// Parses exactly four bytes, without inferring that the reply is a hardware dump.
    pub fn parse(bytes: impl AsRef<[u8]>) -> Result<Self> {
        let b = bytes.as_ref();
        if b.len() != 4 { return Err(corrupt(format!("MEC must be 4 bytes, got {}", b.len()))) }
        Ok(Self { status: b[0], version: [b[1], b[2], b[3]], pcsx2_default: b == [3, 6, 2, 0] })
    }

    /// Packed version used by XCDVDMAN, excluding the status byte.
    #[must_use]
    pub fn version_number(&self) -> u32 { (self.version[0] as u32) << 16 | (self.version[1] as u32) << 8 | self.version[2] as u32 }

    /// Major.minor.patch display string.
    #[must_use]
    pub fn version_string(&self) -> String { format!("{}.{:02}.{:02}", self.version[0], self.version[1], self.version[2]) }

    /// All ten documented comparisons in XCDVDMAN 2.00; earlier drivers expose a subset.
    #[must_use]
    pub fn gates(&self) -> Vec<MecGate> {
        [([1, 5, 0], "model-name read (0x17)"), ([1, 8, 0], "disc-change/tray handling"),
            ([2, 2, 0], "DVD read timing"), ([2, 4, 0], "CancelPOffRdy (0x1B)"),
            ([2, 8, 0], "flag with no known consumer"), ([5, 0, 0], "0x1D/1F/21/22/24/25/26 and N-command path"),
            ([5, 2, 0], "PS1 boot parameters (0x27)"), ([5, 4, 0], "0x28 and temperature (0x03/EF)"),
            ([5, 6, 0], "tray-request test; patch low nibble 1 suppresses TrayReq(1)"),
            ([6, 0, 0], "region parameters (0x36)")].into_iter().map(|(minimum, feature)|
                MecGate { minimum, feature, enabled: self.version >= minimum, citation: "rom1_erom_nvm.md §4" }).collect()
    }
}

/// DVD compare/display strings read directly from ROM1 entries (`rom1_erom_nvm.md` §1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DvdVersion {
    /// Entry suffix, None for the old unsuffixed set.
    pub suffix: Option<char>,
    /// DVDID compare key; None if absent.
    pub id: Option<String>,
    /// DVDVER display string, which may use a different letter; None if absent.
    pub display: Option<String>,
    /// Whether the matching EROMDRV entry exists.
    pub driver_present: bool,
}

/// ROM1 directory evidence; no encrypted modules are decoded.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Rom1Info {
    /// RESET's YYYYMMDD-HHMMSS stamp, when valid (`rom1_erom_nvm.md` §1.1).
    pub stamp: Option<String>,
    /// DVDID/DVDVER/EROMDRV sets discovered by entry names, not a shipped region table.
    pub dvd_versions: Vec<DvdVersion>,
    /// Whether UDNL and DVDCNF exist.
    pub dvd_boot_modules: bool,
}

impl Rom1Info {
    /// Inspects a content-located ROMDIR and its DVD entries.
    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        let rom = RomDir::new(bytes)?;
        rom.module("ROMDIR")?;
        let stamp = rom.module("RESET").ok().and_then(|b| {
            let b = b.get(..15)?;
            (b[8] == b'-' && b[..8].iter().chain(&b[9..]).all(u8::is_ascii_digit))
                .then(|| String::from_utf8_lossy(b).into_owned())
        });
        let mut suffixes = Vec::new();
        for name in rom.names() {
            for prefix in ["DVDID", "DVDVER", "EROMDRV"] {
                if let Some(suffix) = name.strip_prefix(prefix) {
                    if suffix.is_empty() { suffixes.push(None); }
                    else if suffix.len() == 1 && suffix.as_bytes()[0].is_ascii_uppercase() { suffixes.push(suffix.chars().next()); }
                }
            }
        }
        suffixes.sort_unstable();
        suffixes.dedup();
        let text = |name: &str| rom.module(name).ok().map(|b| String::from_utf8_lossy(b).trim_matches(['\0', '\n', '\r']).escape_default().to_string());
        let dvd_versions = suffixes.into_iter().map(|suffix| {
            let s = suffix.map(|c| c.to_string()).unwrap_or_default();
            DvdVersion { suffix, id: text(&format!("DVDID{s}")), display: text(&format!("DVDVER{s}")),
                driver_present: rom.module(&format!("EROMDRV{s}")).is_ok() }
        }).collect();
        Ok(Self { stamp, dvd_versions, dvd_boot_modules: rom.module("UDNL").is_ok() && rom.module("DVDCNF").is_ok() })
    }
}

/// A structurally plausible gzip header in EROM; not proof of a complete member.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GzipHeader {
    /// Offset of the gzip signature.
    pub offset: usize,
    /// Header length, including optional fields.
    pub header_size: usize,
}

/// Plaintext gzip-header evidence in EROM (`rom1_erom_nvm.md` §2.1).
/// No inflater is available in ps2kit: payload boundaries, CRC-32 and ISIZE are **unverified**.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EromInfo {
    /// File length.
    pub size: usize,
    /// Structurally plausible headers, with room for compressed data and a trailer.
    pub gzip_headers: Vec<GzipHeader>,
    /// Gzip signatures whose headers/first DEFLATE block type are malformed or truncated.
    pub malformed_headers: usize,
    /// All-zero or erased file.
    pub blank: bool,
}

impl EromInfo {
    /// Scans plaintext gzip signatures without interpreting encrypted gaps or claiming CRC validation.
    #[must_use]
    pub fn inspect(bytes: &[u8]) -> Self {
        let mut out = Self { size: bytes.len(), gzip_headers: Vec::new(), malformed_headers: 0, blank: blank(bytes) };
        for (offset, sig) in bytes.windows(3).enumerate() {
            if sig != [0x1F, 0x8B, 8] { continue }
            if let Some(header_size) = gzip_header_size(&bytes[offset..]) {
                out.gzip_headers.push(GzipHeader { offset, header_size });
            } else { out.malformed_headers += 1; }
        }
        out
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 { crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1)); }
    }
    !crc
}

// RFC 1952 §§2.3.1–2.3.1.2, only the plaintext header and first DEFLATE block-type bits.
fn gzip_header_size(b: &[u8]) -> Option<usize> {
    let flags = *b.get(3)?;
    if b.len() < 19 || flags & 0xE0 != 0 { return None }
    let mut p: usize = 10;
    if flags & 4 != 0 {
        let len = u16::from_le_bytes([*b.get(p)?, *b.get(p + 1)?]) as usize;
        p = p.checked_add(2 + len)?;
        b.get(..p)?;
    }
    for flag in [8, 16] {
        if flags & flag != 0 { p += b.get(p..)?.iter().position(|&v| v == 0)? + 1; }
    }
    if flags & 2 != 0 {
        let crc = u16::from_le_bytes([*b.get(p)?, *b.get(p + 1)?]);
        if crc != crc32(&b[..p]) as u16 { return None }
        p += 2;
    }
    if p.checked_add(9)? > b.len() || (b[p] >> 1) & 3 == 3 { return None }
    Some(p)
}

/// BIOS and optional companion paths; explicitly supplied paths need not share a basename.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BiosSetPaths {
    /// ROM0 BIOS image.
    pub bios: PathBuf,
    /// EEPROM image.
    pub nvm: Option<PathBuf>,
    /// MechaCon reply.
    pub mec: Option<PathBuf>,
    /// DVD-player module ROM.
    pub rom1: Option<PathBuf>,
    /// Optional Chinese font ROM or dumper mirror.
    pub rom2: Option<PathBuf>,
    /// DVD-player EROM.
    pub erom: Option<PathBuf>,
}

impl BiosSetPaths {
    /// Starts an explicit path set with no companions.
    #[must_use]
    pub fn new(bios: impl AsRef<Path>) -> Self {
        Self { bios: bios.as_ref().into(), nvm: None, mec: None, rom1: None, rom2: None, erom: None }
    }

    /// PCSX2 naming: full filename plus extension first, then same stem, ignoring ASCII case.
    /// `rom1_erom_nvm.md` §1.4; ambiguous case duplicates are reported, not picked arbitrarily.
    pub fn discover(bios: impl AsRef<Path>) -> Result<Self> {
        let bios = bios.as_ref();
        let parent = bios.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        let entries: Vec<PathBuf> = std::fs::read_dir(parent)?.map(|e| e.map(|e| e.path())).collect::<std::io::Result<_>>()?;
        let filename = bios.file_name().ok_or_else(|| corrupt("BIOS path has no filename"))?.to_string_lossy();
        let stem = bios.file_stem().ok_or_else(|| corrupt("BIOS path has no stem"))?.to_string_lossy();
        let companion = |ext: &str| -> Result<Option<PathBuf>> {
            for name in [format!("{filename}.{ext}"), format!("{stem}.{ext}")] {
                let matches: Vec<_> = entries.iter().filter(|p| p.is_file() && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&name))).collect();
                if matches.len() > 1 { return Err(corrupt(format!("ambiguous companion name {name}"))) }
                if let Some(p) = matches.first() { return Ok(Some((*p).clone())) }
            }
            Ok(None)
        };
        Ok(Self { bios: bios.into(), nvm: companion("NVM")?, mec: companion("MEC")?, rom1: companion("ROM1")?,
            rom2: companion("ROM2")?, erom: companion("EROM")? })
    }
}

/// Inspection of ROM0 and all named companions; malformed companions remain reportable findings.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BiosSet {
    /// Paths actually selected by discovery or the explicit caller.
    pub paths: BiosSetPaths,
    /// ROM0 identity.
    pub romver: RomVersion,
    /// Decoded NVM, None if missing/unreadable/malformed (see findings).
    pub nvm: Option<Nvm>,
    /// Decoded MEC reply, None if missing/unreadable/malformed.
    pub mec: Option<Mec>,
    /// Parsed ROM1 directory evidence.
    pub rom1: Option<Rom1Info>,
    /// EROM gzip-header evidence.
    pub erom: Option<EromInfo>,
    /// Byte-for-byte comparison when both ROM1 and ROM2 could be read.
    pub rom2_mirrors_rom1: Option<bool>,
    /// Completeness/provenance findings, each with a citation.
    pub findings: Vec<Prediction>,
}

fn read_companion(path: &Option<PathBuf>, label: &str, findings: &mut Vec<Prediction>) -> Option<Vec<u8>> {
    let path = path.as_ref()?;
    match std::fs::read(path) {
        Ok(b) => Some(b),
        Err(e) => { findings.push(prediction(format!("{label}: cannot read companion ({e})."), "rom1_erom_nvm.md §5")); None }
    }
}

fn decode_companion<T>(bytes: Option<Vec<u8>>, label: &str, findings: &mut Vec<Prediction>,
    parse: impl FnOnce(Vec<u8>) -> Result<T>) -> Option<T> {
    match parse(bytes?) {
        Ok(v) => Some(v),
        Err(e) => { findings.push(prediction(format!("{label}: malformed companion ({e})."), "rom1_erom_nvm.md §§3–5")); None }
    }
}

impl BiosSet {
    /// Discovers companions next to a BIOS, or in a directory containing exactly one PS2 ROM0.
    /// A directory with multiple identities needs an explicit BIOS path.
    pub fn inspect(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bios = if path.is_dir() {
            let mut candidates = Vec::new();
            for entry in std::fs::read_dir(path)? {
                let p = entry?.path();
                if p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bin") || e.eq_ignore_ascii_case("rom0")) {
                    if let Ok(rom) = RomDir::new(std::fs::read(&p)?) {
                        if rom.module("ROMVER").ok().is_some_and(|b| RomVersion::parse(b).is_ok()) { candidates.push(p); }
                    }
                }
            }
            if candidates.len() > 1 { return Err(corrupt("multiple ROM0 images in directory; supply a BIOS file")) }
            candidates.pop().ok_or_else(|| Error::NotFound(Format::Bios, "ROM0 with ROMVER in directory".into()))?
        } else { path.into() };
        Self::inspect_paths(BiosSetPaths::discover(bios)?)
    }

    /// Inspects an explicit set. Only a bad/unreadable ROM0 aborts the whole report.
    pub fn inspect_paths(paths: BiosSetPaths) -> Result<Self> {
        let rom = RomDir::new(std::fs::read(&paths.bios)?)?;
        let romver = RomVersion::parse(rom.module("ROMVER")?)?;
        let mut findings = Vec::new();
        let nvm_bytes = read_companion(&paths.nvm, "NVM", &mut findings);
        let nvm = decode_companion(nvm_bytes, "NVM", &mut findings, |b| Nvm::parse(b, &romver.text));
        let mec_bytes = read_companion(&paths.mec, "MEC", &mut findings);
        let mec = decode_companion(mec_bytes, "MEC", &mut findings, Mec::parse);
        let rom1_bytes = read_companion(&paths.rom1, "ROM1", &mut findings);
        let rom2_bytes = read_companion(&paths.rom2, "ROM2", &mut findings);
        let rom2_mirrors_rom1 = rom1_bytes.as_ref().zip(rom2_bytes.as_ref()).map(|(a, b)| a == b);
        let rom1 = decode_companion(rom1_bytes, "ROM1", &mut findings, Rom1Info::parse);
        let erom = read_companion(&paths.erom, "EROM", &mut findings).map(|b| EromInfo::inspect(&b));
        if nvm.is_none() { findings.push(prediction("NVM unavailable: per-console settings and region parameters cannot be decoded.", "rom1_erom_nvm.md §3")); }
        if !romver.at_least(1, 50) {
            findings.push(prediction("ROM 1.00 needs no ROM1/EROM; DVD player is memory-card-only.", "rom1_erom_nvm.md §5"));
        } else if let Some(r) = &rom1 {
            findings.push(prediction(match &r.stamp {
                Some(stamp) => format!("ROM1 stamp {stamp}; ROM0 date {}: {} (different dates alone do not prove a wrong set).", romver.date,
                    if stamp[..8] == romver.date { "match" } else { "differ" }),
                None => "ROM1 build stamp is absent or unrecognised.".into(),
            }, "rom1_erom_nvm.md §§1.1, 1.4"));
            if !r.dvd_boot_modules { findings.push(prediction("ROM1 lacks UDNL or DVDCNF: DVD IOP boot is incomplete.", "rom1_erom_nvm.md §1.3")); }
            let suffix = if romver.at_least(2, 0) { nvm.as_ref().and_then(|n| n.region_params.as_ref()).map(|p| p.dvd_region) } else { None };
            let selected = suffix.and_then(|s| r.dvd_versions.iter().find(|v| v.suffix == Some(s)))
                .or_else(|| r.dvd_versions.iter().find(|v| v.suffix.is_none()));
            let letters = r.dvd_versions.iter().map(|v| v.suffix.map(|c| c.to_string()).unwrap_or_else(|| "unsuffixed".into())).collect::<Vec<_>>().join(",");
            findings.push(prediction(format!("ROM1 DVD sets [{letters}]; {}", match selected {
                Some(v) => format!("DVD region {}: DVDID={}, DVDVER={}, EROMDRV {}; {}.", suffix.map(|c| c.to_string()).unwrap_or_else(|| "unsuffixed/unknown".into()),
                    v.id.as_deref().unwrap_or("missing"), v.display.as_deref().unwrap_or("missing"),
                    if v.driver_present { "present" } else { "missing" },
                    if v.id.is_some() && v.display.is_some() && v.driver_present { "selected set complete" } else { "selected set incomplete" }),
                None => format!("No complete DVD set for NVM byte 0x186 ({}) or unsuffixed fallback.", suffix.map(|c| c.to_string()).unwrap_or_else(|| "unknown".into())),
            }), "rom1_erom_nvm.md §§1.4, 3.3, 5"));
        } else {
            findings.push(prediction("ROM1 unavailable: ROM DVD player is absent; 1.50+ cannot accept a memory-card DVD player either (no DVDID compare key).", "rom1_erom_nvm.md §1.4"));
        }
        if romver.at_least(1, 50) {
            findings.push(prediction(match &erom {
                None => "EROM unavailable: DVD-player payload is missing.".into(),
                Some(e) => format!("EROM: {} bytes, {} plausible gzip headers, {} malformed; {}. Payload CRC/ISIZE and complete members unverified (no inflater).", e.size,
                    e.gzip_headers.len(), e.malformed_headers, if e.blank { "blank image" } else if e.gzip_headers.is_empty() { "no plaintext gzip evidence" } else { "populated-image evidence" }),
            }, "rom1_erom_nvm.md §2.1"));
        }
        if let Some(mirror) = rom2_mirrors_rom1 {
            findings.push(prediction(if mirror { "ROM2 is byte-identical to ROM1: mirror/copy evidence, not a distinct Chinese font ROM." }
                else { "ROM2 differs from ROM1; this does not establish that it is a usable Chinese font ROM." }, "rom1_erom_nvm.md §§1.1, 5"));
        } else if paths.rom2.is_none() {
            findings.push(prediction("ROM2 absent (required only by the Chinese 2.00 font path, with rom0:ADDROM2).", "rom1_erom_nvm.md §5"));
        }
        let osd_region = if romver.at_least(2, 0) && mec.as_ref().is_some_and(|m| m.version_number() >= 0x060000) {
            nvm.as_ref().and_then(|n| n.region_params.as_ref()).map(|p| p.osd_region).unwrap_or(romver.region)
        } else { romver.region };
        if romver.at_least(2, 0) && osd_region == 'C'
            && (rom.module("ADDROM2").is_err() || rom2_bytes.is_none() || rom2_mirrors_rom1 == Some(true)) {
            findings.push(prediction("Chinese OSD path is incomplete: needs both rom0:ADDROM2 and a distinct ROM2 font image.", "rom1_erom_nvm.md §5"));
        }
        findings.push(prediction(match &mec {
            Some(m) => format!("MEC {}{}; XCDVDMAN 2.00 comparisons: {} open, {} closed; region-params gate {}. Busy bit {}.", m.version_string(),
                if m.pcsx2_default { " = PCSX2 default (status/zone 3, Oceania; not a hardware dump)" } else { " (hardware provenance unknown)" },
                m.gates().iter().filter(|g| g.enabled).count(), m.gates().iter().filter(|g| !g.enabled).count(),
                if m.version_number() >= 0x060000 { "open" } else { "closed" }, if m.status & 0x80 != 0 { "set" } else { "clear" }),
            None => "MEC unavailable: actual MechaCon gates unknown; PCSX2 synthesises its 6.02.00 default when absent.".into(),
        }, "rom1_erom_nvm.md §4"));
        if romver.at_least(1, 50) { findings.push(prediction("PCSX2 does not load EROM; its emulated MechaCon cannot run this DVD-player launch chain.", "rom1_erom_nvm.md §2.1")); }
        Ok(Self { paths, romver, nvm, mec, rom1, erom, rom2_mirrors_rom1, findings })
    }
}
