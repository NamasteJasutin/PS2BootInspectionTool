//! The PlayStation 1 kernel boot path, SYSTEM.CNF grammar, and executable validation.
//!
//! Reconstructs the boot decisions of the real PS1 kernel as documented in
//! `notes/research/ps1_kernel_boot.md`:
//! - SYSTEM.CNF parsing grammar: line-prefix keys, hex numbers without prefix, default to 0,
//!   and RAM 0x180 argument copy (`rom:BFC008A0`, `rom:BFC00944`, `rom:BFC00B7C`, §4.1).
//! - SYSTEM.CNF linter: identifies surprises such as decimal numbers read as hex, `BOOT2`
//!   lines matching `BOOT`, missing keys, and trailing arguments (§4.1, §9.5).
//! - PS-X EXE header and loadability checks: `t_addr`, `t_size % 0x800 == 0`, stack handling,
//!   and unchecked magic (`rom:BFC03A18`, `rom:BFC03CF0`, §4.2).
//! - Root directory lookup: 1-sector read limit and 40-entry table capacity (`rom:BFC07700`, §3.1).
//! - Ordered boot step sequence from shell exit to `DoExecute` (§8, §3.4, §7).

use std::fmt;

/// Maximum bytes of SYSTEM.CNF read by the kernel into `0xA000B070` (`ps1_kernel_boot.md` §4.1).
pub const SYSTEM_CNF_MAX_BYTES: usize = 0x800;

/// Maximum length of the argument string copied to RAM 0x00000180 (`rom:BFC00D20`, `ps1_kernel_boot.md` §4.1).
pub const RAM_ARGUMENT_MAX_BYTES: usize = 0x80;

/// Default BIOS stack pointer inherited when `STACK = 0` or missing (`rom:BFC0E14C`, `rom:BFC03CF0`, `ps1_kernel_boot.md` §4.2).
pub const BIOS_DEFAULT_STACK: u32 = 0x801FFF00;

/// ROM default TCB count applied during initial kernel setup (`rom:BFC0E14C`, `ps1_kernel_boot.md` §1.2).
pub const ROM_DEFAULT_TCB: u32 = 4;

/// ROM default EVENT count applied during initial kernel setup (`rom:BFC0E14C`, `ps1_kernel_boot.md` §1.2).
pub const ROM_DEFAULT_EVENT: u32 = 0x10;

/// How a field value was derived during SYSTEM.CNF parsing (`rom:BFC008A0`, `ps1_kernel_boot.md` §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldDerivation {
    /// Parsed from a line in SYSTEM.CNF.
    Parsed {
        /// 0-indexed line index where the key prefix matched.
        line_index: usize,
        /// The raw line text verbatim.
        raw_line: String,
        /// The raw token extracted after `=` before conversion.
        raw_token: String,
    },
    /// Key was missing: the kernel sets the field to 0 rather than the ROM default (`rom:BFC008A0`, §4.1).
    DefaultZero,
    /// The first line that starts with the key's letters continues with something other than
    /// a space or `=` (e.g. `BOOT2 = …` for `BOOT`, `TCBX = 5` for `TCB`). The kernel stops at
    /// that line and leaves the field unset, so any later proper line is never read
    /// (`rom:BFC00A64` / `rom:BFC00C38`: `bne v0, '='` → return).
    Shadowed {
        /// 0-indexed line index of the shadowing line.
        line_index: usize,
        /// The shadowing line verbatim.
        raw_line: String,
    },
    /// Key was missing or empty: no boot executable was specified.
    NotPresent,
}

impl FieldDerivation {
    /// True if the field was parsed from a line in the file.
    #[must_use]
    pub fn is_parsed(&self) -> bool {
        matches!(self, Self::Parsed { .. })
    }
}

/// A parsed PlayStation 1 `SYSTEM.CNF` file according to the kernel's real grammar (`rom:BFC008A0`, `ps1_kernel_boot.md` §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1SystemCnf {
    /// The raw lines of the file as read (up to 2048 bytes).
    pub raw_lines: Vec<String>,
    /// Thread Control Blocks (kernel address `0xA000B940`, `rom:BFC0DD10`).
    pub tcb: u32,
    /// Event Control Blocks (kernel address `0xA000B944`, `rom:BFC0DD14`).
    pub event: u32,
    /// Stack pointer (kernel address `0xA000B948`, `rom:BFC0DD1C`). 0 means inherit BIOS stack (0x801FFF00).
    pub stack: u32,
    /// Boot executable path (kernel address `0xA000B8B0`, `rom:BFC0DD24`).
    pub boot: Option<String>,
    /// Trailing argument copied to RAM 0x00000180 (at most 128 bytes, `rom:BFC00D20`).
    pub argument: Vec<u8>,
    /// Derivation of `tcb`.
    pub tcb_derivation: FieldDerivation,
    /// Derivation of `event`.
    pub event_derivation: FieldDerivation,
    /// Derivation of `stack`.
    pub stack_derivation: FieldDerivation,
    /// Derivation of `boot`.
    pub boot_derivation: FieldDerivation,
    /// Whether the original input file exceeded the 2048-byte read limit (`rom:BFC0D890`).
    pub was_truncated: bool,
    /// Total input byte count before truncation.
    pub total_input_bytes: usize,
}

impl Ps1SystemCnf {
    /// Parses a `SYSTEM.CNF` file following the kernel's exact rules (`rom:BFC008A0`, `ps1_kernel_boot.md` §4.1):
    /// - At most 2048 (0x800) bytes are read into RAM (`0xA000B070`).
    /// - The three numeric config words are zeroed first; missing keys remain 0.
    /// - Keys are found with a case-sensitive `strncmp(line, key, strlen(key))` at each line
    ///   start; the **first** such line decides: it must continue with spaces and `=`, otherwise
    ///   the field stays unset ([`FieldDerivation::Shadowed`]). A NUL byte ends the text.
    /// - Numbers are parsed as hex without prefix (`TCB = 10` is 16; `0x` stops at 0).
    /// - `STACK = 0` means "inherit the BIOS stack" (0x801FFF00) at `DoExecute`.
    /// - For `BOOT`, the first whitespace-delimited token is the boot path.
    /// - Any text following the boot path is `strncpy`ed into RAM 0x00000180 (up to 128 bytes).
    /// - Carriage returns (`\r`) are tolerated as whitespace.
    #[must_use]
    pub fn parse(text: &[u8]) -> Self {
        let total_input_bytes = text.len();
        let was_truncated = text.len() > SYSTEM_CNF_MAX_BYTES;
        let slice = &text[..text.len().min(SYSTEM_CNF_MAX_BYTES)];

        let raw_lines: Vec<String> = slice
            .split(|&b| b == b'\n')
            .map(|l| {
                let trimmed = if l.ends_with(b"\r") { &l[..l.len() - 1] } else { l };
                String::from_utf8_lossy(trimmed).into_owned()
            })
            .collect();

        // Parse TCB, EVENT, STACK
        let (tcb, tcb_derivation) = parse_numeric_key(slice, b"TCB");
        let (event, event_derivation) = parse_numeric_key(slice, b"EVENT");
        let (stack, stack_derivation) = parse_numeric_key(slice, b"STACK");

        // Parse BOOT
        let (boot, argument, boot_derivation) = parse_boot_key(slice);

        Self {
            raw_lines,
            tcb,
            event,
            stack,
            boot,
            argument,
            tcb_derivation,
            event_derivation,
            stack_derivation,
            boot_derivation,
            was_truncated,
            total_input_bytes,
        }
    }

    /// Evaluates all lints and potential surprises in this `SYSTEM.CNF` (`ps1_kernel_boot.md` §4.1, §9.5).
    #[must_use]
    pub fn lints(&self) -> Vec<CnfLint> {
        let mut lints = Vec::new();

        if self.was_truncated {
            lints.push(CnfLint::TruncatedFile {
                total_bytes: self.total_input_bytes,
            });
        }

        // Numeric checks for TCB, EVENT, STACK
        lint_numeric_field("TCB", ROM_DEFAULT_TCB, &self.tcb_derivation, self.tcb, &mut lints);
        lint_numeric_field("EVENT", ROM_DEFAULT_EVENT, &self.event_derivation, self.event, &mut lints);
        lint_numeric_field("STACK", BIOS_DEFAULT_STACK, &self.stack_derivation, self.stack, &mut lints);

        if self.stack == 0 && self.stack_derivation.is_parsed() {
            lints.push(CnfLint::ExplicitZeroStack);
        }

        // BOOT checks
        match &self.boot_derivation {
            FieldDerivation::Parsed { .. } => {}
            FieldDerivation::Shadowed { raw_line, .. } => lints.push(CnfLint::KeyShadowed { key: "BOOT", line: raw_line.clone() }),
            FieldDerivation::NotPresent | FieldDerivation::DefaultZero => {
                lints.push(CnfLint::MissingBoot);
            }
        }

        if let Some(boot_path) = &self.boot {
            if !boot_path.contains(';') {
                lints.push(CnfLint::MissingVersionSuffix {
                    path: boot_path.clone(),
                });
            }
            if !boot_path.to_ascii_lowercase().starts_with("cdrom:") {
                lints.push(CnfLint::NonCdromDevice {
                    path: boot_path.clone(),
                });
            }
        }

        if !self.argument.is_empty() {
            lints.push(CnfLint::TrailingArgument {
                argument: String::from_utf8_lossy(&self.argument).into_owned(),
            });
        }

        lints
    }

    /// Returns the argument string copied to RAM 0x180 as UTF-8 lossy text.
    #[must_use]
    pub fn argument_str(&self) -> String {
        String::from_utf8_lossy(&self.argument).into_owned()
    }
}

/// Helper for ctype whitespace check as defined in PS1 kernel table `rom:BFC0DDB1` (`ps1_kernel_boot.md` §4.1).
#[inline]
fn is_ps1_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Helper for ctype hex digit check (`ctype bits 0x44` in `rom:BFC0DDB1`).
#[inline]
fn hex_digit_val(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u32),
        b'a'..=b'f' => Some((b - b'a' + 10) as u32),
        b'A'..=b'F' => Some((b - b'A' + 10) as u32),
        _ => None,
    }
}

/// Where the kernel's line scanner (`rom:BFC00944`, `rom:BFC00B7C`) lands for `key`.
enum KeyMatch<'a> {
    /// No line starts with the key.
    Absent,
    /// The first line starting with the key continues with something other than space or `=`.
    Shadowed { line_index: usize, raw_line: String },
    /// `line[value..]` follows `key`, spaces, `=` and spaces.
    Value { line_index: usize, line: &'a [u8], value: usize },
}

/// The kernel's scan: a case-sensitive `strncmp` at each line start (NUL ends the text); the
/// first matching line must continue with space-class bytes and `=` or the search ends there.
fn find_key<'a>(slice: &'a [u8], key: &[u8]) -> KeyMatch<'a> {
    let text = &slice[..slice.iter().position(|&b| b == 0).unwrap_or(slice.len())];
    for (line_index, line) in text.split(|&b| b == b'\n').enumerate() {
        if !line.starts_with(key) { continue }
        let mut p = key.len();
        while p < line.len() && is_ps1_space(line[p]) { p += 1 }
        if line.get(p) != Some(&b'=') {
            return KeyMatch::Shadowed { line_index, raw_line: String::from_utf8_lossy(line).trim_end_matches('\r').to_string() };
        }
        p += 1;
        while p < line.len() && is_ps1_space(line[p]) { p += 1 }
        return KeyMatch::Value { line_index, line, value: p };
    }
    KeyMatch::Absent
}

/// Parses a numeric key (`TCB`, `EVENT`, `STACK`) line by line (`rom:BFC00944`, `ps1_kernel_boot.md` §4.1).
fn parse_numeric_key(slice: &[u8], key: &[u8]) -> (u32, FieldDerivation) {
    let (line_index, line, mut p) = match find_key(slice, key) {
        KeyMatch::Absent => return (0, FieldDerivation::DefaultZero),
        KeyMatch::Shadowed { line_index, raw_line } => return (0, FieldDerivation::Shadowed { line_index, raw_line }),
        KeyMatch::Value { line_index, line, value } => (line_index, line, value),
    };
    let raw_token = String::from_utf8_lossy(&line[p..]).trim_end_matches(['\r', ' ']).to_string();
    // Hex digits without prefix, stopping at the first other byte: `0x10` reads as 0.
    let mut val = 0u32;
    while let Some(digit) = line.get(p).and_then(|&b| hex_digit_val(b)) {
        val = (val << 4) | digit;
        p += 1;
    }
    let raw_line = String::from_utf8_lossy(line).trim_end_matches('\r').to_string();
    (val, FieldDerivation::Parsed { line_index, raw_line, raw_token })
}

/// Parses the `BOOT` line (`rom:BFC00B7C`, `ps1_kernel_boot.md` §4.1).
fn parse_boot_key(slice: &[u8]) -> (Option<String>, Vec<u8>, FieldDerivation) {
    let (line_index, line, mut p) = match find_key(slice, b"BOOT") {
        KeyMatch::Absent => return (None, Vec::new(), FieldDerivation::NotPresent),
        KeyMatch::Shadowed { line_index, raw_line } => return (None, Vec::new(), FieldDerivation::Shadowed { line_index, raw_line }),
        KeyMatch::Value { line_index, line, value } => (line_index, line, value),
    };
        let boot_val_start = p;
        // The boot file name runs until the first space-class byte (or CR/end of line)
        while p < line.len() && !is_ps1_space(line[p]) {
            p += 1;
        }

        let boot_file = String::from_utf8_lossy(&line[boot_val_start..p]).into_owned();

        // Trailing text copied to RAM 0x180: whatever follows the first space-class byte
        let mut argument = Vec::new();
        if p < line.len() {
            // In kernel rom:BFC00D20: strncpy(0x180, token_end + 1, 128)
            let arg_start = p + 1;
            if arg_start < line.len() {
                let arg_slice = &line[arg_start..];
                // Strip trailing CR
                let arg_end = if arg_slice.ends_with(b"\r") {
                    arg_slice.len() - 1
                } else {
                    arg_slice.len()
                };
                let len = arg_end.min(RAM_ARGUMENT_MAX_BYTES);
                argument.extend_from_slice(&arg_slice[..len]);
            }
        }

        let raw_token = boot_file.clone();
    let raw_line = String::from_utf8_lossy(line).trim_end_matches('\r').to_string();
    (Some(boot_file), argument, FieldDerivation::Parsed { line_index, raw_line, raw_token })
}

fn lint_numeric_field(
    name: &'static str,
    rom_default: u32,
    derivation: &FieldDerivation,
    value: u32,
    lints: &mut Vec<CnfLint>,
) {
    match derivation {
        FieldDerivation::Parsed { raw_token, .. } => {
            if raw_token.starts_with("0x") || raw_token.starts_with("0X") {
                lints.push(CnfLint::HexPrefix {
                    key: name.to_string(),
                    raw: raw_token.clone(),
                });
            } else if raw_token.len() >= 2 && raw_token.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(dec_val) = raw_token.parse::<u32>() {
                    if dec_val != value {
                        lints.push(CnfLint::DecimalInterpretedAsHex {
                            key: name.to_string(),
                            raw: raw_token.clone(),
                            parsed_hex: value,
                            decimal_value: dec_val,
                        });
                    }
                }
            }
        }
        FieldDerivation::Shadowed { raw_line, .. } => {
            lints.push(CnfLint::KeyShadowed { key: name, line: raw_line.clone() });
            lints.push(CnfLint::MissingKey { key: name, rom_default });
        }
        FieldDerivation::DefaultZero | FieldDerivation::NotPresent => {
            lints.push(CnfLint::MissingKey {
                key: name,
                rom_default,
            });
        }
    }
}

/// A surprise or quirk detected in `SYSTEM.CNF` (`ps1_kernel_boot.md` §4.1, §9.5).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CnfLint {
    /// Decimal-looking number parsed as hex without prefix (e.g. `TCB = 10` is 16).
    DecimalInterpretedAsHex {
        /// The config key name (`TCB`, `EVENT`, `STACK`).
        key: String,
        /// The raw string token.
        raw: String,
        /// The value parsed in hex.
        parsed_hex: u32,
        /// The value if interpreted as decimal.
        decimal_value: u32,
    },
    /// Value had a `0x` prefix; kernel stops parsing at 'x' resulting in 0.
    HexPrefix {
        /// The config key name.
        key: String,
        /// The raw string token.
        raw: String,
    },
    /// The first line starting with the key's letters is not `key =` (e.g. `BOOT2 = …`), so the
    /// kernel stops there and the field stays unset (`rom:BFC00A64`, `rom:BFC00C38`).
    KeyShadowed {
        /// The key the line hides.
        key: &'static str,
        /// The shadowing line verbatim.
        line: String,
    },
    /// Missing numeric key defaulting to 0 rather than ROM default.
    MissingKey {
        /// The key name.
        key: &'static str,
        /// The ROM default that was NOT used.
        rom_default: u32,
    },
    /// Trailing argument present on the `BOOT` line (copied to RAM 0x180).
    TrailingArgument {
        /// The argument text.
        argument: String,
    },
    /// Boot path lacks ISO 9660 version suffix `;1`.
    MissingVersionSuffix {
        /// The boot path.
        path: String,
    },
    /// Boot path does not begin with `cdrom:`.
    NonCdromDevice {
        /// The boot path.
        path: String,
    },
    /// Missing `BOOT` line (kernel falls back to `cdrom:PSX.EXE;1`).
    MissingBoot,
    /// `STACK` was explicitly set to 0.
    ExplicitZeroStack,
    /// File exceeded 2048 bytes and was truncated by the kernel.
    TruncatedFile {
        /// Total file bytes.
        total_bytes: usize,
    },
}

impl fmt::Display for CnfLint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecimalInterpretedAsHex { key, raw, parsed_hex, decimal_value } => {
                write!(f, "'{key} = {raw}' looks like decimal {decimal_value}, but the kernel reads hex without prefix resulting in 0x{raw} = {parsed_hex} (ps1_kernel_boot.md §4.1)")
            }
            Self::HexPrefix { key, raw } => {
                write!(f, "'{key} = {raw}' uses a '0x' prefix; the kernel parses hex digits without prefix and stops at 'x', yielding 0 (rom:BFC00944, ps1_kernel_boot.md §4.1)")
            }
            Self::KeyShadowed { key, line } => {
                write!(f, "\"{line}\" is the first line starting with {key}, but it does not continue with '=': the kernel stops at it and never reads a later {key} line, so {key} stays unset (rom:BFC00A64 / rom:BFC00C38; corrects ps1_kernel_boot.md §4.1)")
            }
            Self::MissingKey { key: "STACK", rom_default } => {
                write!(f, "missing 'STACK' key: kernel sets STACK to 0, inheriting the BIOS stack 0x{rom_default:08X} at DoExecute (rom:BFC03CF0, ps1_kernel_boot.md §4.1)")
            }
            Self::MissingKey { key, rom_default } => {
                write!(f, "missing '{key}' key: kernel sets {key} to 0 rather than the ROM default {rom_default} (rom:BFC008A0, ps1_kernel_boot.md §4.1)")
            }
            Self::TrailingArgument { argument } => {
                write!(f, "trailing argument \"{argument}\" after boot file name is copied to RAM 0x00000180 (rom:BFC00D20, ps1_kernel_boot.md §4.1)")
            }
            Self::MissingVersionSuffix { path } => {
                write!(f, "boot path \"{path}\" lacks ISO 9660 version suffix ';1'; kernel dev_cd_open appends it (rom:BFC0E2E8, ps1_kernel_boot.md §3.1)")
            }
            Self::NonCdromDevice { path } => {
                write!(f, "boot path \"{path}\" does not begin with PS1 device 'cdrom:'; kernel only registers 'cdrom:' at boot (rom:BFC0E2F0, ps1_kernel_boot.md §1.1)")
            }
            Self::MissingBoot => {
                f.write_str("missing 'BOOT' line: kernel falls back to 'cdrom:PSX.EXE;1' with ROM defaults (ps1_kernel_boot.md §3.5)")
            }
            Self::ExplicitZeroStack => {
                f.write_str("'STACK = 0' leaves the kernel's BIOS stack 0x801FFF00 in place at DoExecute (rom:BFC03CF0, ps1_kernel_boot.md §4.2)")
            }
            Self::TruncatedFile { total_bytes } => {
                write!(f, "SYSTEM.CNF is {total_bytes} bytes; kernel reads at most 2048 bytes (0x800) into 0xA000B070 (ps1_kernel_boot.md §4.1)")
            }
        }
    }
}

/// The outcome of kernel executable validation (`LoadExeFile`, `rom:BFC03A18`, `ps1_kernel_boot.md` §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExeOutcome {
    /// File is shorter than 2048 bytes: `LoadExeFile` returns 0 -> `'B', 0x38A` hang.
    HeaderTruncated,
    /// `t_size % 0x800 != 0`: `dev_cd_read` returns EINVAL (-1), nothing is loaded, jumps into unloaded RAM at `pc0`.
    UnalignedLoadsNothing,
    /// Claimed text segment extends beyond the physical file size.
    TextExtendsPastFile,
    /// Valid sector-aligned executable that loads normally and jumps to `pc0`.
    Bootable,
}

impl fmt::Display for ExeOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderTruncated => f.write_str("header shorter than 2048 bytes: LoadExeFile returns 0 -> SystemError('B', 0x38A) hang"),
            Self::UnalignedLoadsNothing => f.write_str("t_size not a multiple of 0x800: dev_cd_read fails (EINVAL), read result ignored, jumps to unloaded memory at pc0"),
            Self::TextExtendsPastFile => f.write_str("header text segment extends past physical file: dev_cd_read clamps or reads past EOF"),
            Self::Bootable => f.write_str("sector-aligned text loads into RAM; DoExecute jumps to pc0"),
        }
    }
}

/// PlayStation 1 executable loadability analysis (`rom:BFC03A18`, `rom:BFC03CF0`, `ps1_kernel_boot.md` §4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1ExeCheck {
    /// Total file size in bytes.
    pub file_size: usize,
    /// True if the file has at least 2048 bytes for the header.
    pub header_readable: bool,
    /// Program counter (`pc0` at header +0x10).
    pub pc0: u32,
    /// Global pointer (`gp0` at header +0x14).
    pub gp0: u32,
    /// Text RAM destination (`t_addr` at header +0x18).
    pub t_addr: u32,
    /// Text size in bytes (`t_size` at header +0x1C).
    pub t_size: u32,
    /// BSS destination (`b_addr` at header +0x28).
    pub b_addr: u32,
    /// BSS size in bytes (`b_size` at header +0x2C, zero-filled by `DoExecute`).
    pub b_size: u32,
    /// Stack base (`s_addr` at header +0x30).
    pub s_addr: u32,
    /// Stack size (`s_size` at header +0x34).
    pub s_size: u32,
    /// Initial stack pointer: `s_addr + s_size` if `s_addr != 0`, otherwise None (inherits BIOS stack).
    pub sp: Option<u32>,
    /// Sector alignment: `t_size % 0x800 == 0`.
    pub t_size_sector_aligned: bool,
    /// The kernel always ignores the read return value (`rom:BFC03A18`). Always true.
    pub read_result_ignored: bool,
    /// Whether `2048 + t_size > file_size`.
    pub text_extends_past_file: bool,
    /// Whether the header starts with `"PS-X EXE"` (kernel never validates this).
    pub has_psx_magic: bool,
    /// True if the kernel validates the magic (always false: `ps1_kernel_boot.md` §4.2).
    pub magic_checked_by_kernel: bool,
    /// The overall verdict.
    pub outcome: ExeOutcome,
}

impl Ps1ExeCheck {
    /// Evaluates an executable header and file size using the kernel's real loader logic (`rom:BFC03A18`, `ps1_kernel_boot.md` §4.2):
    /// - Only requires that 2048 (0x800) bytes were read.
    /// - "PS-X EXE" magic, SCE region text, and checksum are never checked.
    /// - `dev_cd_read` requires `t_size % 0x800 == 0` (else EINVAL and nothing is loaded).
    /// - Return value of read is ignored.
    /// - Stack is set to `s_addr + s_size` only if `s_addr != 0`.
    #[must_use]
    pub fn judge(header: &[u8], file_size: usize) -> Self {
        if header.len() < 0x800 || file_size < 0x800 {
            return Self {
                file_size,
                header_readable: false,
                pc0: 0,
                gp0: 0,
                t_addr: 0,
                t_size: 0,
                b_addr: 0,
                b_size: 0,
                s_addr: 0,
                s_size: 0,
                sp: None,
                t_size_sector_aligned: false,
                read_result_ignored: true,
                text_extends_past_file: false,
                has_psx_magic: false,
                magic_checked_by_kernel: false,
                outcome: ExeOutcome::HeaderTruncated,
            };
        }

        let has_psx_magic = header.len() >= 8 && &header[..8] == b"PS-X EXE";
        let read_u32 = |off: usize| -> u32 {
            u32::from_le_bytes(header[off..off + 4].try_into().unwrap_or([0; 4]))
        };

        let pc0 = read_u32(0x10);
        let gp0 = read_u32(0x14);
        let t_addr = read_u32(0x18);
        let t_size = read_u32(0x1C);
        let b_addr = read_u32(0x28);
        let b_size = read_u32(0x2C);
        let s_addr = read_u32(0x30);
        let s_size = read_u32(0x34);

        let sp = if s_addr != 0 {
            Some(s_addr.wrapping_add(s_size))
        } else {
            None
        };

        let t_size_sector_aligned = t_size % 0x800 == 0;
        let text_extends_past_file = (2048usize).saturating_add(t_size as usize) > file_size;

        let outcome = if !t_size_sector_aligned {
            ExeOutcome::UnalignedLoadsNothing
        } else if text_extends_past_file {
            ExeOutcome::TextExtendsPastFile
        } else {
            ExeOutcome::Bootable
        };

        Self {
            file_size,
            header_readable: true,
            pc0,
            gp0,
            t_addr,
            t_size,
            b_addr,
            b_size,
            s_addr,
            s_size,
            sp,
            t_size_sector_aligned,
            read_result_ignored: true,
            text_extends_past_file,
            has_psx_magic,
            magic_checked_by_kernel: false,
            outcome,
        }
    }
}

impl fmt::Display for Ps1ExeCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.header_readable {
            return write!(f, "PS-X EXE check: file size {} < 2048 bytes (LoadExeFile fails)", self.file_size);
        }
        write!(
            f,
            "PS-X EXE check: PC0 0x{:08X}, GP0 0x{:08X}, T_ADDR 0x{:08X}, T_SIZE 0x{:08X} ({} bytes, {}), SP {}, magic checked: false; outcome: {}",
            self.pc0,
            self.gp0,
            self.t_addr,
            self.t_size,
            self.t_size,
            if self.t_size_sector_aligned { "sector aligned" } else { "NOT sector aligned" },
            self.sp.map(|s| format!("0x{s:08X}")).unwrap_or_else(|| "inherited".into()),
            self.outcome,
        )
    }
}

/// Root directory lookup analysis comparing the kernel's 1-sector/40-entry loader against a full walk (`rom:BFC07700`, `ps1_kernel_boot.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ps1RootLookup {
    /// Normalized search target name with `;1` (e.g. `"SYSTEM.CNF;1"`).
    pub target_name: String,
    /// Found under the kernel's rules: first sector (2048 bytes) and within the first 40 entries.
    pub found_by_kernel: bool,
    /// Found anywhere in the root directory extent.
    pub found_in_full_directory: bool,
    /// 0-indexed entry record index where the file was found.
    pub entry_index: Option<usize>,
    /// Byte offset within `root_dir` where the record starts.
    pub byte_offset: Option<usize>,
    /// Sector index within `root_dir` (0 = first sector).
    pub sector_index: Option<usize>,
    /// LBA extent recorded in the directory record.
    pub lba: Option<u32>,
    /// Data length recorded in the directory record.
    pub size: Option<u32>,
    /// Total valid directory records in the first sector.
    pub first_sector_entries: usize,
    /// Total valid directory records across the entire directory extent.
    pub total_entries: usize,
    /// True if the record sits in the first sector but past entry 40 (kernel misses it).
    pub exceeds_40_entries: bool,
    /// True if the record sits beyond the first 2048 bytes (kernel misses it).
    pub exceeds_one_sector: bool,
}

impl Ps1RootLookup {
    /// Evaluates whether `name` is found under the kernel's directory rules (`rom:BFC07700`, `ps1_kernel_boot.md` §3.1):
    /// - The kernel reads **one sector only** (first 2048 bytes).
    /// - It keeps at most **40 entries** (`0xA00091F0..95B0`).
    /// - Directory matching is exact except `?` wildcard.
    /// - The target name is upper-cased and `;1` is appended if no `;` is present (`rom:BFC0E2E8`).
    #[must_use]
    pub fn judge(root_dir: &[u8], name: &str) -> Self {
        let clean_name = name.rsplit(['\\', '/', ':']).next().unwrap_or(name);
        let mut target_name = clean_name.to_ascii_uppercase();
        if !target_name.contains(';') {
            target_name.push_str(";1");
        }

        let mut p = 0;
        let mut entry_index = 0;
        let mut first_sector_entries = 0;
        let mut total_entries = 0;

        let mut found_by_kernel = false;
        let mut found_in_full_directory = false;
        let mut found_entry_index = None;
        let mut found_byte_offset = None;
        let mut found_sector_index = None;
        let mut found_lba = None;
        let mut found_size = None;
        let mut exceeds_40_entries = false;
        let mut exceeds_one_sector = false;

        while p + 33 <= root_dir.len() {
            let len = root_dir[p] as usize;
            if len == 0 {
                // Sector padding in ISO 9660: advance to start of next 2048-byte sector
                p = (p / 2048 + 1) * 2048;
                continue;
            }
            if p + len > root_dir.len() {
                break;
            }

            let name_len = root_dir[p + 32] as usize;
            if p + 33 + name_len > p + len {
                break;
            }

            let entry_name_bytes = &root_dir[p + 33..p + 33 + name_len];
            let entry_name = String::from_utf8_lossy(entry_name_bytes);
            let sector_idx = p / 2048;
            let in_first_sector = sector_idx == 0;

            if in_first_sector {
                first_sector_entries += 1;
            }
            total_entries += 1;

            if entry_name.eq_ignore_ascii_case(&target_name) && !found_in_full_directory {
                found_in_full_directory = true;
                found_entry_index = Some(entry_index);
                found_byte_offset = Some(p);
                found_sector_index = Some(sector_idx);

                let lba = u32::from_le_bytes(root_dir[p + 2..p + 6].try_into().unwrap_or([0; 4]));
                let size = u32::from_le_bytes(root_dir[p + 10..p + 14].try_into().unwrap_or([0; 4]));
                found_lba = Some(lba);
                found_size = Some(size);

                if in_first_sector && entry_index < 40 {
                    found_by_kernel = true;
                } else if in_first_sector && entry_index >= 40 {
                    exceeds_40_entries = true;
                } else {
                    exceeds_one_sector = true;
                }
            }

            entry_index += 1;
            p += len;
        }

        Self {
            target_name,
            found_by_kernel,
            found_in_full_directory,
            entry_index: found_entry_index,
            byte_offset: found_byte_offset,
            sector_index: found_sector_index,
            lba: found_lba,
            size: found_size,
            first_sector_entries,
            total_entries,
            exceeds_40_entries,
            exceeds_one_sector,
        }
    }
}

impl fmt::Display for Ps1RootLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.found_by_kernel {
            write!(
                f,
                "'{}' found by kernel at entry {} (offset 0x{:04X}, sector 0, LBA {}, {} bytes)",
                self.target_name,
                self.entry_index.unwrap_or(0),
                self.byte_offset.unwrap_or(0),
                self.lba.unwrap_or(0),
                self.size.unwrap_or(0),
            )
        } else if self.exceeds_40_entries {
            write!(
                f,
                "'{}' found at entry {} (exceeds the kernel's 40-entry table capacity at 0xA00091F0; kernel misses it)",
                self.target_name,
                self.entry_index.unwrap_or(0),
            )
        } else if self.exceeds_one_sector {
            write!(
                f,
                "'{}' found at offset 0x{:04X} in sector {} (exceeds the kernel's 1-sector read limit in rom:BFC07700; kernel misses it)",
                self.target_name,
                self.byte_offset.unwrap_or(0),
                self.sector_index.unwrap_or(0),
            )
        } else {
            write!(f, "'{}' not found in root directory ({} entries searched)", self.target_name, self.total_entries)
        }
    }
}

/// One step in the PS1 kernel hand-off from shell exit to `DoExecute` (`notes/research/ps1_kernel_boot.md` §8).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Ps1BootStep {
    /// Assumption regarding the drive's GetID verdict (§7, §8 step 8, §9 item 8).
    DriveAssumption {
        /// The assumed SCEx region marker.
        region: &'static str,
    },
    /// Shell licence decision and exit via `jr ra` (POST 5, §8 step 11).
    ShellExit {
        /// Whether this shell checks the licence text/logo.
        checks_licence: Option<bool>,
    },
    /// CD-ROM re-initialisation (`CdInit`, rom:BFC073A0, POST 8, §8 step 12).
    CdInit,
    /// Root directory lookup for `SYSTEM.CNF;1` (`rom:BFC07700`, §3.1).
    SystemCnfLookup {
        /// Lookup outcome.
        lookup: Option<Box<Ps1RootLookup>>,
    },
    /// SYSTEM.CNF read and parse (POST 9, `rom:BFC008A0`, §4.1).
    SystemCnf {
        /// Parsed SYSTEM.CNF if file was present.
        cnf: Option<Box<Ps1SystemCnf>>,
        /// Boot executable path determined.
        boot_path: String,
        /// Configured TCB count.
        tcb: u32,
        /// Configured EVENT count.
        event: u32,
        /// Configured STACK address.
        stack: u32,
        /// Argument copied to RAM 0x180.
        argument: Vec<u8>,
    },
    /// Kernel Setup 2 and user RAM wipe (`rom:BFC06F28`, `rom:BFC0D850`, §8 step 14).
    KernelSetup2 {
        /// TCB count configured.
        tcb: u32,
        /// EVENT count configured.
        event: u32,
        /// Stack top configured.
        stack: u32,
    },
    /// Root directory lookup for the boot executable (`rom:BFC07700`, §3.1).
    BootFileLookup {
        /// File name looked up.
        file_name: String,
        /// Lookup outcome.
        lookup: Option<Box<Ps1RootLookup>>,
    },
    /// LoadExeFile reading the 2048-byte header (`rom:BFC03A18`, §4.2).
    ExeHeader {
        /// Path opened.
        path: String,
        /// Header check analysis.
        check: Option<Box<Ps1ExeCheck>>,
    },
    /// dev_cd_read loading the text segment into RAM (`rom:BFC07A04`, §4.2).
    ExeTextLoad {
        /// Destination RAM address.
        t_addr: u32,
        /// Size in bytes.
        t_size: u32,
        /// Sector aligned (`t_size % 0x800 == 0`).
        sector_aligned: bool,
        /// The kernel ignores the read return value.
        read_result_ignored: bool,
        /// Text extends past file size.
        extends_past_file: bool,
    },
    /// Pre-exec ReadTOC 0x1E and GetID 0x1A anti-swap check (`rom:BFC0D570`, §3.4).
    PreExecCheck {
        /// Enabled if controller BIOS date >= 95/07/06 (`0xA000DFFC != 0`).
        enabled: bool,
    },
    /// Kernel DoExecute jumping to the executable (`rom:BFC03CF0`, §4.2).
    DoExecute {
        /// Initial program counter (`pc0`).
        pc0: u32,
        /// Initial global pointer (`gp0`).
        gp0: u32,
        /// Initial stack pointer (`sp`).
        sp: u32,
        /// True if BIOS default stack 0x801FFF00 was inherited.
        stack_inherited: bool,
        /// BSS address zero-filled.
        b_addr: u32,
        /// BSS size zero-filled.
        b_size: u32,
    },
    /// Boot failure hang (`SystemErrorBootOrDiskFailure`, rom:BFC0D8D0, §1.2).
    BootFailed {
        /// Error stage code ('B' or 'D', hex error code).
        code: (char, u16),
        /// Detailed explanation.
        reason: String,
    },
}

impl Ps1BootStep {
    /// The component performing this step (`ps1_kernel_boot.md` §8).
    #[must_use]
    pub fn who(&self) -> &'static str {
        match self {
            Self::DriveAssumption { .. } => "CD controller (drive firmware)",
            Self::ShellExit { .. } => "PS1 shell (0x80030000)",
            Self::CdInit => "Bootstrap CdInit (rom:BFC073A0)",
            Self::SystemCnfLookup { .. } => "CD-ROM filesystem (rom:BFC07700)",
            Self::SystemCnf { .. } => "Bootstrap SYSTEM.CNF (rom:BFC008A0)",
            Self::KernelSetup2 { .. } => "Kernel Setup 2 (rom:BFC06F28)",
            Self::BootFileLookup { .. } => "CD-ROM filesystem (rom:BFC07700)",
            Self::ExeHeader { .. } => "Bootstrap LoadExeFile (rom:BFC03A18)",
            Self::ExeTextLoad { .. } => "CD-ROM dev_cd_read (rom:BFC07A04)",
            Self::PreExecCheck { .. } => "Bootstrap Pre-Exec (rom:BFC0D570)",
            Self::DoExecute { .. } => "Kernel DoExecute (rom:BFC03CF0)",
            Self::BootFailed { .. } => "Kernel SystemError (rom:BFC0D8D0)",
        }
    }
}

impl fmt::Display for Ps1BootStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DriveAssumption { region } => {
                write!(
                    f,
                    "assuming the drive reports this disc as licensed ({region}): the wobble-groove SCEx handshake is performed by sub-CPU firmware and cannot be derived from disc/BIOS bytes alone (ps1_kernel_boot.md §7)"
                )
            }
            Self::ShellExit { checks_licence: Some(true) } => {
                f.write_str("shell verifies sector-4 licence text and logo blob before returning (POST 5, jr ra to bootstrap at POST 8)")
            }
            Self::ShellExit { checks_licence: Some(false) } => {
                f.write_str("shell displays sector-4 licence text and logo without checking (retail A console behaviour; POST 5, jr ra to bootstrap at POST 8)")
            }
            Self::ShellExit { checks_licence: None } => {
                f.write_str("shell exits to bootstrap at POST 8 (licence verification policy not specified)")
            }
            Self::CdInit => {
                f.write_str("CD re-initialisation (POST 8): CdInit (rom:BFC073A0) opens CD events, verifies PVD 'CD001' at LBA 16, XORs path table into disc identity 0xA0009D7C; banner 'BOOTSTRAP LOADER Type C Ver 2.1   03-JUL-1994'")
            }
            Self::SystemCnfLookup { lookup: Some(lookup) } => {
                write!(f, "root directory lookup for cdrom:SYSTEM.CNF;1 (rom:BFC07700): {lookup}")
            }
            Self::SystemCnfLookup { lookup: None } => {
                f.write_str("root directory lookup for cdrom:SYSTEM.CNF;1 (rom:BFC07700: 1 sector, <= 40 entries)")
            }
            Self::SystemCnf { cnf: Some(_), boot_path, tcb, event, stack, argument } => {
                write!(
                    f,
                    "read cdrom:SYSTEM.CNF;1 (POST 9, rom:BFC008A0): BOOT = {boot_path}, TCB = {tcb}, EVENT = {event}, STACK = 0x{stack:08X}{}",
                    if argument.is_empty() {
                        String::new()
                    } else {
                        format!("; argument \"{}\" copied to RAM 0x180", String::from_utf8_lossy(argument))
                    }
                )
            }
            Self::SystemCnf { cnf: None, boot_path, .. } => {
                write!(
                    f,
                    "no SYSTEM.CNF on disc: kernel falls back to {boot_path} with ROM defaults TCB 4, EVENT 0x10, STACK 0x801FFF00 (ps1_kernel_boot.md §3.5)"
                )
            }
            Self::KernelSetup2 { tcb, event, stack } => {
                write!(
                    f,
                    "second KERNEL SETUP (rom:BFC06F28): alloc {tcb} TCBs, {event} EvCBs, stack 0x{stack:08X}; wipe user RAM 0xA0010000..0x{stack:08X} (rom:BFC0D850)"
                )
            }
            Self::BootFileLookup { file_name, lookup: Some(lookup) } => {
                write!(f, "root directory lookup for {file_name}: {lookup}")
            }
            Self::BootFileLookup { file_name, lookup: None } => {
                write!(f, "root directory lookup for {file_name} (rom:BFC07700: 1 sector, <= 40 entries)")
            }
            Self::ExeHeader { path, check: Some(check) } => {
                write!(f, "LoadExeFile (rom:BFC03A18): read 2048-byte header for {path} into 0xA000B870; {check}")
            }
            Self::ExeHeader { path, check: None } => {
                write!(f, "LoadExeFile (rom:BFC03A18): read 2048-byte header for {path} into 0xA000B870; 'PS-X EXE' magic is never validated")
            }
            Self::ExeTextLoad { t_addr, t_size, sector_aligned, extends_past_file, .. } => {
                write!(
                    f,
                    "dev_cd_read (rom:BFC07A04): read {t_size} bytes to RAM 0x{t_addr:08X}; sector alignment {}{}; read result ignored by kernel",
                    if *sector_aligned { "valid (multiple of 0x800)" } else { "INVALID (EINVAL, nothing loaded)" },
                    if *extends_past_file { " (extends past physical file)" } else { "" },
                )
            }
            Self::PreExecCheck { enabled } => {
                write!(
                    f,
                    "pre-exec disc check (rom:BFC0D570): ReadTOC 0x1E and GetID 0x1A via CD registers 0x1F801800..03; status & 0x1D == 0 required (anti-swap check{})",
                    if *enabled { " enabled" } else { " disabled" },
                )
            }
            Self::DoExecute { pc0, gp0, sp, stack_inherited, b_addr, b_size } => {
                write!(
                    f,
                    "DoExecute (rom:BFC03CF0): clear BSS 0x{b_addr:08X}..0x{:08X}, SP = 0x{sp:08X}{}, GP = 0x{gp0:08X}, jalr PC0 0x{pc0:08X} with argc 1, argv 0",
                    b_addr.wrapping_add(*b_size),
                    if *stack_inherited { " (inherited BIOS stack)" } else { "" },
                )
            }
            Self::BootFailed { code: (stage, code), reason } => {
                write!(f, "SystemErrorBootOrDiskFailure('{stage}', 0x{code:03X}) (hang, POST F): {reason}")
            }
        }
    }
}

/// Generates the ordered list of hand-off steps from shell exit to game execution (`ps1_kernel_boot.md` §8).
///
/// # Arguments
/// - `system_cnf`: Raw bytes of `SYSTEM.CNF` if present.
/// - `exe_header`: `(header_bytes, file_size)` of the boot executable if available.
/// - `root_dir`: Raw bytes of the root directory extent if available.
/// - `shell_checks_licence`: Whether the shell verifies the licence text and logo (per shell version/region).
#[must_use]
pub fn ps1_boot_steps(
    system_cnf: Option<&[u8]>,
    exe_header: Option<(&[u8], usize)>,
    root_dir: Option<&[u8]>,
    shell_checks_licence: Option<bool>,
) -> Vec<Ps1BootStep> {
    let mut steps = Vec::new();

    // 1. Drive assumption (§7, §8 step 8, §9 item 8)
    steps.push(Ps1BootStep::DriveAssumption { region: "SCEA" });

    // 2. Shell exit (§8 step 11)
    steps.push(Ps1BootStep::ShellExit {
        checks_licence: shell_checks_licence,
    });

    // 3. CD re-initialisation (POST 8, §8 step 12)
    steps.push(Ps1BootStep::CdInit);

    // 4. Root directory lookup for SYSTEM.CNF (POST 9, §3.1)
    let cnf_lookup = root_dir.map(|dir| Box::new(Ps1RootLookup::judge(dir, "SYSTEM.CNF")));
    steps.push(Ps1BootStep::SystemCnfLookup { lookup: cnf_lookup });

    // 5. SYSTEM.CNF read / parse (POST 9, §4.1)
    let (cnf_opt, boot_path, tcb, event, stack, argument) = if let Some(cnf_bytes) = system_cnf {
        let cnf = Ps1SystemCnf::parse(cnf_bytes);
        let path = cnf.boot.clone().unwrap_or_else(|| "cdrom:PSX.EXE;1".to_string());
        let t = cnf.tcb;
        let e = cnf.event;
        let s = cnf.stack;
        let a = cnf.argument.clone();
        (Some(cnf), path, t, e, s, a)
    } else {
        ("cdrom:PSX.EXE;1".to_string(), ROM_DEFAULT_TCB, ROM_DEFAULT_EVENT, BIOS_DEFAULT_STACK)
            .into_fallback()
    };

    steps.push(Ps1BootStep::SystemCnf {
        cnf: cnf_opt.map(Box::new),
        boot_path: boot_path.clone(),
        tcb,
        event,
        stack,
        argument,
    });

    // 6. Kernel Setup 2 and user RAM wipe (§8 step 14)
    steps.push(Ps1BootStep::KernelSetup2 { tcb, event, stack });

    // 7. Root directory lookup for boot executable (§3.1, §8 step 14)
    let boot_file_name = boot_path.rsplit(['\\', '/', ':']).next().unwrap_or(&boot_path).to_string();
    let exe_lookup = root_dir.map(|dir| Ps1RootLookup::judge(dir, &boot_file_name));

    if let Some(lookup) = &exe_lookup {
        steps.push(Ps1BootStep::BootFileLookup {
            file_name: boot_file_name.clone(),
            lookup: Some(Box::new(lookup.clone())),
        });
        if !lookup.found_by_kernel {
            steps.push(Ps1BootStep::BootFailed {
                code: ('B', 0x38A),
                reason: format!("boot file '{boot_file_name}' not found by kernel in root directory (LoadExeFile returned 0)"),
            });
            return steps;
        }
    } else {
        steps.push(Ps1BootStep::BootFileLookup {
            file_name: boot_file_name.clone(),
            lookup: None,
        });
    }

    // 8. LoadExeFile header check (§4.2, §8 step 14)
    let exe_check = exe_header.map(|(hdr, size)| Ps1ExeCheck::judge(hdr, size));
    steps.push(Ps1BootStep::ExeHeader {
        path: boot_path,
        check: exe_check.as_ref().map(|c| Box::new(c.clone())),
    });

    if let Some(check) = &exe_check {
        if !check.header_readable {
            steps.push(Ps1BootStep::BootFailed {
                code: ('B', 0x38A),
                reason: format!("executable file size {} < 2048 bytes (LoadExeFile returned 0)", check.file_size),
            });
            return steps;
        }

        // 9. dev_cd_read text load (§4.2, §8 step 14)
        steps.push(Ps1BootStep::ExeTextLoad {
            t_addr: check.t_addr,
            t_size: check.t_size,
            sector_aligned: check.t_size_sector_aligned,
            read_result_ignored: true,
            extends_past_file: check.text_extends_past_file,
        });

        // 10. Pre-exec ReadTOC + GetID check (§3.4, §8 step 15)
        steps.push(Ps1BootStep::PreExecCheck { enabled: true });

        // 11. DoExecute (§4.2, §8 step 16)
        let effective_sp = if stack != 0 { stack } else { BIOS_DEFAULT_STACK };
        let stack_inherited = stack == 0;
        steps.push(Ps1BootStep::DoExecute {
            pc0: check.pc0,
            gp0: check.gp0,
            sp: effective_sp,
            stack_inherited,
            b_addr: check.b_addr,
            b_size: check.b_size,
        });
    } else {
        // Without header bytes, report the standard flow with generic parameters
        steps.push(Ps1BootStep::PreExecCheck { enabled: true });
        let effective_sp = if stack != 0 { stack } else { BIOS_DEFAULT_STACK };
        steps.push(Ps1BootStep::DoExecute {
            pc0: 0x80010000,
            gp0: 0,
            sp: effective_sp,
            stack_inherited: stack == 0,
            b_addr: 0,
            b_size: 0,
        });
    }

    steps
}

trait FallbackExt {
    fn into_fallback(self) -> (Option<Ps1SystemCnf>, String, u32, u32, u32, Vec<u8>);
}

impl FallbackExt for (String, u32, u32, u32) {
    fn into_fallback(self) -> (Option<Ps1SystemCnf>, String, u32, u32, u32, Vec<u8>) {
        (None, self.0, self.1, self.2, self.3, Vec::new())
    }
}
