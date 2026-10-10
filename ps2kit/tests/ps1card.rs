//! Tests for the PlayStation 1 memory card reader (`ps2kit::ps1card`).
//!
//! Includes synthetic memory card construction (header, 3 saves with icons,
//! multi-block chains, deleted entries with undelete evaluation, DexDrive .gme container,
//! synthetic BPLAYSTATION SoundScope score per hidden_input.md §9.2), checksum verification,
//! Shift-JIS title decoding, fuzz testing against malformed inputs, and an optional
//! real-card oracle test enabled via `PS1CARD`.

use ps2kit::ps1card::{
    decode_sjis, frame_checksum, PerformanceEvent, Ps1Card, SoundScopePerformance,
    GME_HEADER_SIZE, GME_MAGIC, PS1_BLOCK_SIZE, PS1_CARD_SIZE, PS1_FRAME_SIZE,
    SOUNDSCOPE_MAX_EVENTS, SOUNDSCOPE_VERSION,
};
use ps2kit::Region;

/// Encodes an ASCII string as full-width Shift-JIS characters.
fn encode_fullwidth_sjis(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in s.chars() {
        match c {
            ' ' | '　' => {
                out.push(0x81);
                out.push(0x40);
            }
            '0'..='9' => {
                out.push(0x82);
                out.push(0x4F + (c as u8 - b'0'));
            }
            '０'..='９' => {
                out.push(0x82);
                out.push(0x4F + (c as u32 - 0xFF10) as u8);
            }
            'A'..='Z' => {
                out.push(0x82);
                out.push(0x60 + (c as u8 - b'A'));
            }
            'Ａ'..='Ｚ' => {
                out.push(0x82);
                out.push(0x60 + (c as u32 - 0xFF21) as u8);
            }
            'a'..='z' => {
                out.push(0x82);
                out.push(0x81 + (c as u8 - b'a'));
            }
            'ａ'..='ｚ' => {
                out.push(0x82);
                out.push(0x81 + (c as u32 - 0xFF41) as u8);
            }
            '.' | '．' => {
                out.push(0x81);
                out.push(0x44);
            }
            '-' | '－' => {
                out.push(0x81);
                out.push(0x7C);
            }
            _ => {
                out.push(c as u8);
            }
        }
    }
    out
}

/// Creates a blank 128 KiB raw memory card with valid header frame and formatted directory frames.
fn create_blank_card() -> Vec<u8> {
    let mut card = vec![0u8; PS1_CARD_SIZE];

    // Frame 0: Header frame (MC magic, 0x0E checksum at byte 127)
    card[0] = b'M';
    card[1] = b'C';
    card[127] = frame_checksum(&card[..128]);

    // Frames 1..=15: Free formatted directory frames (state 0xA0)
    for i in 1..=15 {
        let off = i * PS1_FRAME_SIZE;
        card[off] = 0xA0; // FreeFormatted
        card[off + 8] = 0xFF; // next block = -1
        card[off + 9] = 0xFF;
        card[off + 127] = frame_checksum(&card[off..off + PS1_FRAME_SIZE]);
    }

    // Frames 16..=35: Broken sector list (all 0xFFFFFFFF = none)
    for i in 16..=35 {
        let off = i * PS1_FRAME_SIZE;
        card[off..off + 4].copy_from_slice(&[0xFF; 4]);
        card[off + 127] = frame_checksum(&card[off..off + PS1_FRAME_SIZE]);
    }

    card
}

/// Configures a save file across the specified blocks on a memory card.
#[allow(clippy::too_many_arguments)]
fn add_save_to_card(
    card: &mut [u8],
    blocks: &[usize],
    filename: &str,
    title_sjis: &[u8],
    icon_flag: u8,
    clut: &[u16; 16],
    icons: &[&[u8; 128]],
    is_deleted: bool,
    custom_payload: Option<&[u8]>,
) {
    let block_count = blocks.len();
    assert!(block_count >= 1);

    for (idx, &blk) in blocks.iter().enumerate() {
        let dir_off = blk * PS1_FRAME_SIZE;
        let is_first = idx == 0;
        let is_last = idx == block_count - 1;

        let state = if is_deleted {
            if is_first {
                0xA1
            } else if is_last {
                0xA3
            } else {
                0xA2
            }
        } else if is_first {
            0x51
        } else if is_last {
            0x53
        } else {
            0x52
        };

        card[dir_off] = state;

        if is_first {
            let file_size = (block_count * PS1_BLOCK_SIZE) as u32;
            card[dir_off + 4..dir_off + 8].copy_from_slice(&file_size.to_le_bytes());

            let next = if block_count > 1 {
                (blocks[1] - 1) as u16
            } else {
                0xFFFF
            };
            card[dir_off + 8..dir_off + 10].copy_from_slice(&next.to_le_bytes());

            let fn_bytes = filename.as_bytes();
            let fn_len = fn_bytes.len().min(20);
            card[dir_off + 10..dir_off + 10 + fn_len].copy_from_slice(&fn_bytes[..fn_len]);
        } else if is_last {
            card[dir_off + 8..dir_off + 10].copy_from_slice(&0xFFFFu16.to_le_bytes());
        } else {
            let next = (blocks[idx + 1] - 1) as u16;
            card[dir_off + 8..dir_off + 10].copy_from_slice(&next.to_le_bytes());
        }

        card[dir_off + 127] = frame_checksum(&card[dir_off..dir_off + PS1_FRAME_SIZE]);
    }

    // Write title frame in Frame 0 of first data block
    let data_blk_off = blocks[0] * PS1_BLOCK_SIZE;
    card[data_blk_off] = b'S';
    card[data_blk_off + 1] = b'C';
    card[data_blk_off + 2] = icon_flag;
    card[data_blk_off + 3] = block_count as u8;

    let title_len = title_sjis.len().min(64);
    card[data_blk_off + 4..data_blk_off + 4 + title_len].copy_from_slice(&title_sjis[..title_len]);

    // 16-color CLUT at 0x60
    for (i, &col) in clut.iter().enumerate() {
        card[data_blk_off + 0x60 + i * 2..data_blk_off + 0x60 + i * 2 + 2]
            .copy_from_slice(&col.to_le_bytes());
    }

    // Icon frames (Frame 1..=N)
    for (fi, &icon_data) in icons.iter().enumerate() {
        let icon_frame_off = data_blk_off + (fi + 1) * PS1_FRAME_SIZE;
        card[icon_frame_off..icon_frame_off + PS1_FRAME_SIZE].copy_from_slice(icon_data);
    }

    // Custom payload (e.g. SoundScope) at offset 0x200
    if let Some(payload) = custom_payload {
        card[data_blk_off + 0x200..data_blk_off + 0x200 + payload.len()].copy_from_slice(payload);
    }
}

/// Builds a synthetic SoundScope score payload (version, checksum, events).
fn build_soundscope_payload(events: &[PerformanceEvent], initial_scale: u32, initial_variant: u32) -> Vec<u8> {
    let mut payload = vec![0u8; 0x1E80 - 0x200]; // 0x1C80 bytes

    // Version at 0x00
    payload[0..4].copy_from_slice(&SOUNDSCOPE_VERSION.to_le_bytes());

    // Count at 0x08
    let count = events.len() as u32;
    payload[8..12].copy_from_slice(&count.to_le_bytes());

    // Initial scale and variant
    payload[12..16].copy_from_slice(&initial_scale.to_le_bytes());
    payload[16..20].copy_from_slice(&initial_variant.to_le_bytes());

    // Events at offset 0x60
    for (i, ev) in events.iter().enumerate().take(SOUNDSCOPE_MAX_EVENTS) {
        let slot = 0x60 + i * 8;
        payload[slot..slot + 4].copy_from_slice(&ev.field.to_le_bytes());
        payload[slot + 4..slot + 6].copy_from_slice(&ev.duration.to_le_bytes());
        payload[slot + 6..slot + 8].copy_from_slice(&ev.buttons.to_le_bytes());
    }

    // Checksum at 0x04: wrapping sum of frame u32 + buttons u16 across all 900 slots
    let mut calc_sum = 0u32;
    for i in 0..SOUNDSCOPE_MAX_EVENTS {
        let slot = 0x60 + i * 8;
        let frame = u32::from_le_bytes(payload[slot..slot + 4].try_into().unwrap());
        let buttons = u16::from_le_bytes(payload[slot + 6..slot + 8].try_into().unwrap());
        calc_sum = calc_sum.wrapping_add(frame).wrapping_add(buttons as u32);
    }
    payload[4..8].copy_from_slice(&calc_sum.to_le_bytes());

    payload
}

/// Wraps raw memory card bytes into an InterAct DexDrive `.gme` container.
fn wrap_dexdrive_gme(raw_card: &[u8]) -> Vec<u8> {
    let mut gme = vec![0u8; GME_HEADER_SIZE + raw_card.len()];
    gme[..GME_MAGIC.len()].copy_from_slice(GME_MAGIC);
    gme[GME_MAGIC.len()] = 0; // NUL terminator
    gme[0x10..0x15].copy_from_slice(&[0, 0, 1, 0, 1]);
    gme[GME_HEADER_SIZE..].copy_from_slice(raw_card);
    gme
}

#[test]
fn synthetic_card_parsing_and_saves() {
    let mut card_bytes = create_blank_card();

    // Palette: entry 0 transparent (0), entry 1 red (0x001F), entry 2 green (0x03E0)
    let mut clut = [0u16; 16];
    clut[1] = 0x001F; // Red (R=31, G=0, B=0)
    clut[2] = 0x03E0; // Green (R=0, G=31, B=0)

    let icon1 = [0x11u8; 128]; // All index 1 pixels (red)
    let icon2 = [0x22u8; 128]; // All index 2 pixels (green)

    // Save 1: 1 block, 1 static icon, Japanese region
    let title1 = encode_fullwidth_sjis("Ｔｅｋｋｅｎ　３");
    add_save_to_card(
        &mut card_bytes,
        &[1],
        "BISLPS-01234TEKKEN",
        &title1,
        0x11,
        &clut,
        &[&icon1],
        false,
        None,
    );

    // Save 2: 2-block multi-block chain, 2 animated icon frames, American region
    let title2 = encode_fullwidth_sjis("Ｆｉｎａｌ　Ｆａｎｔａｓｙ");
    add_save_to_card(
        &mut card_bytes,
        &[2, 3],
        "BASLUS-00001FF7",
        &title2,
        0x12,
        &clut,
        &[&icon1, &icon2],
        false,
        None,
    );

    // Save 3: SoundScope performance BPLAYSTATION (notes/research/hidden_input.md §9.2)
    let title3 = encode_fullwidth_sjis("ＳｏｕｎｄＳｃｏｐｅ");
    let perf_events = vec![
        PerformanceEvent::new(0, 60, 0x0080, None), // Square
        PerformanceEvent::new(60, 30, 0x8000, None), // Left (preset decrement)
        PerformanceEvent::new(90, 45, 0x2000, None), // Right (preset increment)
    ];
    let perf_payload = build_soundscope_payload(&perf_events, 0x100, 5);
    add_save_to_card(
        &mut card_bytes,
        &[4],
        "BPLAYSTATION",
        &title3,
        0x11,
        &clut,
        &[&icon1],
        false,
        Some(&perf_payload),
    );

    // Save 4: Deleted save (recoverable by undelete)
    let title4 = encode_fullwidth_sjis("Ｃｒａｓｈ");
    add_save_to_card(
        &mut card_bytes,
        &[5],
        "BESLES-00002CRASH",
        &title4,
        0x11,
        &clut,
        &[&icon1],
        true,
        None,
    );

    // Save 5: Deleted save (blocked because its next pointer points to live block 1)
    let title5 = encode_fullwidth_sjis("Ｂｌｏｃｋｅｄ");
    let dir6 = 6 * PS1_FRAME_SIZE;
    card_bytes[dir6] = 0xA1;
    card_bytes[dir6 + 4..dir6 + 8].copy_from_slice(&(16384u32).to_le_bytes());
    card_bytes[dir6 + 8..dir6 + 10].copy_from_slice(&0u16.to_le_bytes()); // next block index 0 = block 1
    let fn5 = b"BESLES-00003BLOCKED";
    card_bytes[dir6 + 10..dir6 + 10 + fn5.len()].copy_from_slice(fn5);
    card_bytes[dir6 + 127] = frame_checksum(&card_bytes[dir6..dir6 + PS1_FRAME_SIZE]);

    let blk6 = 6 * PS1_BLOCK_SIZE;
    card_bytes[blk6] = b'S';
    card_bytes[blk6 + 1] = b'C';
    card_bytes[blk6 + 2] = 0x12;
    card_bytes[blk6 + 3] = 2;
    card_bytes[blk6 + 4..blk6 + 4 + title5.len()].copy_from_slice(&title5);

    let card = Ps1Card::parse(&card_bytes).expect("synthetic card must parse");

    // Header & directory checksums
    assert!(card.header_checksum_valid);
    assert!(card.directory_checksums_valid);
    assert_eq!(card.broken_frames.len(), 0);

    // 3 live saves
    assert_eq!(card.saves.len(), 3);

    // Save 1 checks
    let s1 = &card.saves[0];
    assert_eq!(s1.filename, "BISLPS-01234TEKKEN");
    assert_eq!(s1.region, Some(Region::Japan));
    assert_eq!(s1.product, "SLPS-01234");
    assert_eq!(s1.name, "TEKKEN");
    assert_eq!(s1.title, "Ｔｅｋｋｅｎ　３");
    assert_eq!(s1.blocks, vec![1]);
    assert_eq!(s1.icon_frames.len(), 1);
    assert_eq!(s1.icon_flag, 0x11);
    // Pixel check: red color [255, 0, 0, 255]
    assert_eq!(s1.icon_frames[0][0..4], [255, 0, 0, 255]);

    // Save 2 checks (multi-block)
    let s2 = &card.saves[1];
    assert_eq!(s2.filename, "BASLUS-00001FF7");
    assert_eq!(s2.region, Some(Region::America));
    assert_eq!(s2.product, "SLUS-00001");
    assert_eq!(s2.title, "Ｆｉｎａｌ　Ｆａｎｔａｓｙ");
    assert_eq!(s2.blocks, vec![2, 3]);
    assert_eq!(s2.icon_frames.len(), 2);
    assert_eq!(s2.icon_flag, 0x12);
    assert_eq!(s2.file_size, 16384);
    // Second icon frame is green [0, 255, 0, 255]
    assert_eq!(s2.icon_frames[1][0..4], [0, 255, 0, 255]);

    // Save 3 checks (SoundScope performance)
    let s3 = &card.saves[2];
    assert_eq!(s3.filename, "BPLAYSTATION");
    assert_eq!(s3.title, "ＳｏｕｎｄＳｃｏｐｅ");
    assert_eq!(s3.blocks, vec![4]);

    let perfs = card.soundscope_performances();
    assert_eq!(perfs.len(), 1);
    let p = &perfs[0];
    assert_eq!(p.version, SOUNDSCOPE_VERSION);
    assert!(p.checksum_valid);
    assert_eq!(p.recorded_count, 3);
    assert_eq!(p.initial_scale, 0x100);
    assert_eq!(p.initial_variant, 5);
    assert_eq!(p.events.len(), 3);
    assert_eq!(p.duration_fields(), 135); // 90 + 45 = 135

    // Presets tracked: start 5, Left -> 4, Right -> 5
    assert_eq!(p.events[0].preset, Some(5));
    assert_eq!(p.events[1].preset, Some(4));
    assert_eq!(p.events[2].preset, Some(5));

    // Deleted entries & undelete evaluation (notes/research/hidden_input.md §6)
    let deleted = card.deleted();
    assert_eq!(deleted.len(), 2);

    // Save 4 is recoverable
    let d1 = &deleted[0];
    assert_eq!(d1.save.filename, "BESLES-00002CRASH");
    assert!(d1.recoverable);
    assert_eq!(d1.failure_reason, None);
    assert_eq!(d1.state_transitions, vec![(5, 0xA1, 0x51)]);

    // Save 5 is unrecoverable (chain reuses live block 1)
    let d2 = &deleted[1];
    assert_eq!(d2.save.filename, "BESLES-00003BLOCKED");
    assert!(!d2.recoverable);
    assert!(d2.failure_reason.as_ref().unwrap().contains("block 1"));
}

#[test]
fn dexdrive_gme_container_parses_identically() {
    let mut card_bytes = create_blank_card();
    let clut = [0u16; 16];
    let icon = [0u8; 128];
    let title = encode_fullwidth_sjis("ＧＭＥ　Ｔｅｓｔ");

    add_save_to_card(
        &mut card_bytes,
        &[1],
        "BASLUS-09999GMETEST",
        &title,
        0x11,
        &clut,
        &[&icon],
        false,
        None,
    );

    let gme_bytes = wrap_dexdrive_gme(&card_bytes);
    let parsed_gme = Ps1Card::parse(&gme_bytes).expect("DexDrive .gme must parse");
    let parsed_raw = Ps1Card::parse(&card_bytes).expect("Raw card must parse");

    assert_eq!(parsed_gme.saves.len(), 1);
    assert_eq!(parsed_gme.saves[0].filename, parsed_raw.saves[0].filename);
    assert_eq!(parsed_gme.saves[0].title, parsed_raw.saves[0].title);
    assert_eq!(parsed_gme.saves[0].blocks, parsed_raw.saves[0].blocks);
    assert_eq!(parsed_gme.raw, parsed_raw.raw);
}

#[test]
fn checksum_validation_and_tampering() {
    let mut card_bytes = create_blank_card();
    assert!(Ps1Card::parse(&card_bytes).unwrap().header_checksum_valid);
    assert!(Ps1Card::parse(&card_bytes).unwrap().directory_checksums_valid);

    // Corrupt header frame checksum (Frame 0, byte 127)
    card_bytes[127] ^= 0xFF;
    let card_bad_header = Ps1Card::parse(&card_bytes).unwrap();
    assert!(!card_bad_header.header_checksum_valid);
    assert!(card_bad_header.directory_checksums_valid);

    // Restore header checksum and corrupt directory frame 1 checksum
    card_bytes[127] ^= 0xFF;
    card_bytes[PS1_FRAME_SIZE + 127] ^= 0x42;
    let card_bad_dir = Ps1Card::parse(&card_bytes).unwrap();
    assert!(card_bad_dir.header_checksum_valid);
    assert!(!card_bad_dir.directory_checksums_valid);
}

#[test]
fn soundscope_checksum_validation() {
    let events = vec![PerformanceEvent::new(0, 10, 0x0100, None)];
    let mut payload = build_soundscope_payload(&events, 0x100, 0);

    let perf_valid = SoundScopePerformance::parse(&payload).expect("valid score");
    assert!(perf_valid.checksum_valid);

    // Tamper with checksum at offset 4
    payload[4] ^= 0x01;
    let perf_tampered = SoundScopePerformance::parse(&payload).expect("parses even if invalid");
    assert!(!perf_tampered.checksum_valid);
}

#[test]
fn sjis_decoding_fullwidth_and_ascii() {
    // Test full-width ASCII range: digits, uppercase, lowercase, space, punctuation
    let raw = [
        0x82, 0x60, // 'Ａ'
        0x82, 0x79, // 'Ｚ'
        0x81, 0x40, // '　'
        0x82, 0x81, // 'ａ'
        0x82, 0x9A, // 'ｚ'
        0x81, 0x40, // '　'
        0x82, 0x4F, // '０'
        0x82, 0x58, // '９'
        0x81, 0x44, // '．'
        0x81, 0x7C, // '－'
    ];
    let decoded = decode_sjis(&raw);
    assert_eq!(decoded, "ＡＺ　ａｚ　０９．－");

    // Standard ASCII fallback
    let ascii = b"Standard ASCII Title\0";
    assert_eq!(decode_sjis(ascii), "Standard ASCII Title");

    // NUL byte termination
    let with_nul = [0x82, 0x60, 0x00, 0x82, 0x61];
    assert_eq!(decode_sjis(&with_nul), "Ａ");
}

#[test]
fn malformed_inputs_never_panic() {
    let valid = create_blank_card();

    // Truncated cards
    assert!(Ps1Card::parse(&valid[..100]).is_err());
    assert!(Ps1Card::parse(&valid[..1000]).is_err());
    assert!(Ps1Card::parse(&valid[..131_071]).is_err());

    // Bad magic
    let mut bad_magic = valid.clone();
    bad_magic[0] = b'X';
    assert!(Ps1Card::parse(&bad_magic).is_err());

    // Truncated .gme container
    let gme = wrap_dexdrive_gme(&valid);
    assert!(Ps1Card::parse(&gme[..GME_HEADER_SIZE + 100]).is_err());

    // Fuzz 3000 mutations
    let mut rng = ps2kit::history::SplitMix(42);
    let mut parse_success = 0;

    for round in 0..3000 {
        let mut d = valid.clone();
        for _ in 0..1 + round % 8 {
            let idx = (rng.next_u64() as usize) % d.len();
            d[idx] = rng.next_u64() as u8;
        }
        if round % 5 == 0 {
            d.truncate((rng.next_u64() as usize) % d.len());
        }

        let Ok(card) = Ps1Card::parse(&d) else {
            continue;
        };
        parse_success += 1;

        // Exercise all readers on the parsed mutated card
        let _ = card.deleted();
        let _ = card.soundscope_performances();
        for s in &card.saves {
            let _ = s.block_count();
            let _ = decode_sjis(&s.title_bytes);
        }
    }

    assert!(parse_success > 500, "expected mutations preserving size and magic to parse");
}

#[test]
fn real_ps1_card_oracle() {
    let Ok(env_path) = std::env::var("PS1CARD") else {
        // Silently skip when no real card is supplied
        return;
    };
    let Ok(bytes) = std::fs::read(&env_path) else {
        return;
    };
    let card = Ps1Card::parse(&bytes).expect("PS1CARD must parse");
    println!("Real PS1 card from {}: {} saves", env_path, card.saves.len());
}
