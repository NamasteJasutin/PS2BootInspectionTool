//! Synthetic icon formats and mutation tests; real cards are opt-in via PS2_CARD.

use ps2kit::history::PlayHistory;
use ps2kit::icon::{IconModel, IconSys, SaveIcon};
use ps2kit::memcard::MemoryCard;
use ps2kit::{Vec2, Vec4};

fn word(b: &mut Vec<u8>, v: u32) { b.extend_from_slice(&v.to_le_bytes()) }
fn float(b: &mut Vec<u8>, v: f32) { word(b, v.to_bits()) }
fn short(b: &mut Vec<u8>, v: i16) { b.extend_from_slice(&v.to_le_bytes()) }
fn u32_at(b: &[u8], o: usize) -> u32 { u32::from_le_bytes(b[o..o + 4].try_into().expect("test field")) }
fn f32_at(b: &[u8], o: usize) -> f32 { f32::from_bits(u32_at(b, o)) }
fn fixed_at(b: &[u8], o: usize) -> f32 { i16::from_le_bytes([b[o], b[o + 1]]) as f32 / 4096.0 }

fn descriptor() -> Vec<u8> {
    let mut b = vec![0; 964];
    b[..4].copy_from_slice(b"PS2D");
    b[6..8].copy_from_slice(&4u16.to_le_bytes());
    b[12..16].copy_from_slice(&96u32.to_le_bytes());
    for (i, c) in b[16..80].chunks_exact_mut(4).enumerate() { c.copy_from_slice(&(i as u32).to_le_bytes()) }
    for (i, c) in b[80..192].chunks_exact_mut(4).enumerate() { c.copy_from_slice(&(i as f32 / 32.0).to_le_bytes()) }
    b[192..200].copy_from_slice(&[0x82, 0x60, 0x82, 0x61, 0x82, 0x62, 0x82, 0x63]); // ＡＢ / ＣＤ
    for o in [260, 324, 388] { b[o..o + 9].copy_from_slice(b"model.ico") }
    b
}

fn model(flags: u32) -> Vec<u8> {
    let mut b = Vec::new();
    word(&mut b, 0x10000); word(&mut b, 2); word(&mut b, flags); float(&mut b, 3.0); word(&mut b, 3);
    for i in 0..3 {
        for s in 0..2 { for v in [i * 1024, -4096 + s * 4096, 2048, 1024] { short(&mut b, v) } }
        for v in [0, 0, -4096, 0, -128, 4096] { short(&mut b, v) }
        b.extend_from_slice(&[128, 64, 255, 0]);
    }
    word(&mut b, 1); word(&mut b, 20); float(&mut b, 1.0); word(&mut b, 0); word(&mut b, 2);
    for shape in 0..2 {
        word(&mut b, shape); word(&mut b, 2);
        float(&mut b, 0.0); float(&mut b, (1 - shape) as f32);
        float(&mut b, 10.0); float(&mut b, shape as f32);
    }
    if flags & 4 != 0 {
        if flags & 8 != 0 {
            let mut packed = Vec::new();
            short(&mut packed, -2); short(&mut packed, 0x001f); short(&mut packed, 0x03e0);
            short(&mut packed, 16382); short(&mut packed, 0xfc00u16 as i16);
            word(&mut b, packed.len() as u32); b.extend_from_slice(&packed);
        } else {
            for i in 0..16384 { short(&mut b, if i == 0 { 0x001f } else if i == 1 { 0x03e0 } else { 0xfc00u16 as i16 }) }
        }
    }
    b
}

#[test]
fn metadata_and_shift_jis_byte_split() {
    let s = IconSys::parse(&descriptor()).expect("synthetic descriptor");
    assert_eq!(s.title_lines, ["ＡＢ", "ＣＤ"]);
    assert_eq!(s.title_offset, 4);
    assert_eq!(s.background_alpha, 96);
    assert_eq!(s.background_colours[3], [12, 13, 14, 15]);
    assert_eq!(s.light_directions[0], Vec4::new(0.0, 1.0 / 32.0, 2.0 / 32.0, 3.0 / 32.0));
    assert_eq!(s.ambient.w, 27.0 / 32.0);
    assert_eq!(s.list_file, "model.ico");
    assert_eq!(s.copy_file, s.list_file);
    assert_eq!(s.delete_file, s.list_file);
}

#[test]
fn raw_rle_and_morph_animation_agree() {
    let raw = IconModel::parse(&model(7)).expect("raw");
    let rle = IconModel::parse(&model(15)).expect("RLE");
    assert_eq!(raw.texture, rle.texture);
    assert_eq!(raw.texture.len(), 128 * 128 * 4);
    assert_eq!(&raw.texture[..12], &[255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 255]);
    assert_eq!(raw.back_face, 3.0); // Real files contradict the alleged fixed 1.0.
    assert_eq!(raw.vertices[0].uv, Vec2::new(-128.0 / 4096.0, 1.0));
    assert_eq!(raw.vertices[0].normal, Vec4::new(0.0, 0.0, -1.0, 0.0));
    assert_eq!(raw.vertices[0].rgba, [128, 64, 255, 0]);
    assert_eq!(raw.shapes[0][0], Vec4::new(0.0, -1.0, 0.5, 0.25));
    assert_eq!(raw.shapes[1][0].y, 0.0);
    assert_eq!(raw.shape_weights(0.0), [1.0, 0.0]);
    assert_eq!(raw.shape_weights(5.0), [0.5, 0.5]);
    assert_eq!(raw.shape_weights(10.0), [0.0, 1.0]);
    assert_eq!(raw.shape_weights(25.0), [0.5, 0.5]);
    assert_eq!(raw.bounds().0.y, -1.0);
    assert_eq!(raw.bounds().1.y, 0.0);
    assert!(IconModel::parse(&model(3)).expect("untextured").texture.is_empty());
}

#[test]
fn bad_lengths_counts_floats_runs_and_paths_report_errors() {
    let sys = descriptor();
    for n in 0..sys.len() { assert!(IconSys::parse(&sys[..n]).is_err(), "sys prefix {n}") }
    for flags in [3, 7, 15] {
        let data = model(flags);
        for n in (0..data.len()).filter(|n| *n < 240 || n % 97 == 0 || *n == data.len() - 1) { assert!(IconModel::parse(&data[..n]).is_err(), "model prefix {n}") }
        for (offset, value) in [(0, 0), (4, 0), (4, u32::MAX), (12, f32::NAN.to_bits()), (16, 4), (16, u32::MAX), (116, 2), (120, 0), (124, f32::INFINITY.to_bits()), (132, u32::MAX), (136, 2), (140, u32::MAX), (144, f32::NAN.to_bits()), (144, (-1.0f32).to_bits()), (148, (-1.0f32).to_bits())] {
            let mut b = data.clone(); b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(IconModel::parse(&b).is_err(), "invalid field {offset}");
        }
    }
    for filename in ["../evil.ico", "/evil", "..", ".", "C:\\evil.ico", ""] {
        let mut b = sys.clone(); b[260..324].fill(0); b[260..260 + filename.len()].copy_from_slice(filename.as_bytes());
        assert!(IconSys::parse(&b).is_err());
    }
    let mut b = sys.clone(); b[6..8].copy_from_slice(&69u16.to_le_bytes()); assert!(IconSys::parse(&b).is_err());
    let mut b = sys.clone(); b[260..324].fill(b'x'); assert!(IconSys::parse(&b).is_err());
    let mut b = sys; b[80..84].copy_from_slice(&f32::NAN.to_le_bytes()); assert!(IconSys::parse(&b).is_err());
    // RLE length starts at 184; payload starts at 188.
    for run in [0i16, i16::MIN] { let mut b = model(15); b[188..190].copy_from_slice(&run.to_le_bytes()); assert!(IconModel::parse(&b).is_err()) }
    let mut b = model(15); b[184..188].copy_from_slice(&u32::MAX.to_le_bytes()); assert!(IconModel::parse(&b).is_err());
    let mut b = model(15); b[194..196].copy_from_slice(&16381i16.to_le_bytes()); assert!(IconModel::parse(&b).is_err());
}

#[test]
fn mutation_fuzz_never_panics() {
    let mut seed = 0x51a6_7901u64;
    for original in [descriptor(), model(3), model(7), model(15)] {
        for _ in 0..1200 {
            let mut b = original.clone();
            for _ in 0..4 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let limit = if seed & 1 == 0 { b.len().min(200) } else { b.len() };
                let i = (seed >> 8) as usize % limit;
                b[i] ^= (seed >> 40) as u8;
            }
            assert!(std::panic::catch_unwind(|| { let _ = IconSys::parse(&b); if let Ok(m) = IconModel::parse(&b) { let _ = m.shape_weights(123.0); let _ = m.bounds(); } }).is_ok());
        }
    }
}

#[test]
fn folder_card_keeps_duplicate_saves_and_skips_corruption() {
    let root = std::path::Path::new("/tmp/b4a-icons").join(format!("icon-test-{}", std::process::id()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
    std::fs::create_dir_all(&root).expect("synthetic card directory");
    let _cleanup = Cleanup(root.clone());
    for name in ["BESLES-12345A", "BESLES-12345B", "BESLES-54321BAD", "BOTHER", "Bあいうえお"] {
        let dir = root.join(name); std::fs::create_dir(&dir).expect("save folder");
        std::fs::write(dir.join("icon.sys"), descriptor()).expect("synthetic descriptor");
        std::fs::write(dir.join("model.ico"), if name.ends_with("BAD") { vec![0] } else { model(7) }).expect("synthetic model");
    }
    let card = MemoryCard::open(&root).expect("folder card");
    let saves = SaveIcon::from_card(&card);
    assert_eq!(saves.len(), 4);
    assert_eq!(saves.iter().filter(|s| s.title_id == "SLES_123.45").count(), 2);
    assert_eq!(saves.iter().filter(|s| s.title_id.is_empty()).count(), 2);
    assert!(saves.iter().all(|s| s.title == "ＡＢ\nＣＤ"));
    assert!(SaveIcon::read_folder(&card, "BESLES-54321BAD").is_err());
    assert!(SaveIcon::read_folder(&card, "../other").is_err());
}

#[test]
fn real_card_fields_and_folder_mapping() {
    let Some(path) = std::env::var_os("PS2_CARD") else { return };
    let card = MemoryCard::open(path).expect("PS2_CARD must name a formatted PS2 card");
    let saves = SaveIcon::from_card(&card);
    let titles = PlayHistory::titles_with_saves(&card);
    let mut descriptors = 0;
    for entry in card.list(&[]).expect("root").into_iter().filter(|e| e.is_directory) {
        let Ok(data) = card.read_file(&[&entry.name, "icon.sys"]) else { continue };
        descriptors += 1;
        let save = SaveIcon::read_folder(&card, &entry.name).expect("real icon");
        let s = &save.icon_sys;
        assert_eq!(s.file_type, u16::from_le_bytes([data[4], data[5]]));
        assert_eq!(s.title_offset, u16::from_le_bytes([data[6], data[7]]));
        assert_eq!(s.reserved, u32_at(&data, 8));
        assert_eq!(s.background_alpha, u32_at(&data, 12));
        for i in 0..16 { assert_eq!(s.background_colours[i / 4][i % 4], u32_at(&data, 16 + i * 4)) }
        let lights: Vec<Vec4> = s.light_directions.iter().chain(&s.light_colours).copied().chain([s.ambient]).collect();
        for (i, v) in lights.iter().enumerate() { for j in 0..4 { assert_eq!(v[j], f32_at(&data, 80 + i * 16 + j * 4)) } }
        assert_eq!(s.title_sjis, data[192..260]);
        for (name, offset) in [(&s.list_file, 260), (&s.copy_file, 324), (&s.delete_file, 388)] {
            let end = data[offset..offset + 64].iter().position(|&b| b == 0).expect("terminated filename");
            assert_eq!(name.as_bytes(), &data[offset..offset + end]);
        }
        if !save.title_id.is_empty() { assert!(titles.contains(&save.title_id)) }
        let raw = card.read_file(&[&entry.name, &s.list_file]).expect("model bytes");
        let m = &save.list_icon;
        assert_eq!(m.shapes.len(), u32_at(&raw, 4) as usize);
        assert_eq!(m.vertices.len(), u32_at(&raw, 16) as usize);
        assert_eq!(m.texture_type, u32_at(&raw, 8));
        assert_eq!(m.back_face, f32_at(&raw, 12));
        let stride = m.shapes.len() * 8 + 16;
        for (i, vertex) in m.vertices.iter().enumerate() {
            let offset = 20 + i * stride;
            for (j, shape) in m.shapes.iter().enumerate() { for (c, value) in shape[i].to_array().iter().enumerate() { assert_eq!(*value, fixed_at(&raw, offset + j * 8 + c * 2)) } }
            let offset = offset + m.shapes.len() * 8;
            for c in 0..4 { assert_eq!(vertex.normal[c], fixed_at(&raw, offset + c * 2)) }
            assert_eq!(vertex.uv, Vec2::new(fixed_at(&raw, offset + 8), fixed_at(&raw, offset + 10)));
            assert_eq!(vertex.rgba, raw[offset + 12..offset + 16]);
        }
        let mut offset = 20 + m.vertices.len() * stride;
        assert_eq!(u32_at(&raw, offset), 1);
        assert_eq!(m.frame_length, u32_at(&raw, offset + 4));
        assert_eq!(m.animation_speed, f32_at(&raw, offset + 8));
        assert_eq!(m.play_offset, u32_at(&raw, offset + 12));
        assert_eq!(m.frames.len(), u32_at(&raw, offset + 16) as usize);
        offset += 20;
        for track in &m.frames {
            assert_eq!(track.shape_id, u32_at(&raw, offset)); assert_eq!(track.keys.len(), u32_at(&raw, offset + 4) as usize); offset += 8;
            for key in &track.keys { assert_eq!(key.time, f32_at(&raw, offset)); assert_eq!(key.weight, f32_at(&raw, offset + 4)); offset += 8; }
        }
        if m.texture_type & 8 != 0 { assert_eq!(offset + 4 + u32_at(&raw, offset) as usize, raw.len()) }
        else if m.texture_type & 4 != 0 { assert_eq!(offset + 32768, raw.len()) }
        else { assert_eq!(offset, raw.len()) }
        assert_eq!(m.texture.len(), if m.texture_type & 4 != 0 { 65536 } else { 0 });
        // Independently expand the card's texture stream and compare every RGB5551 pixel.
        if m.texture_type & 4 != 0 {
            let mut packed_pixels = Vec::new();
            if m.texture_type & 8 != 0 {
                let end = offset + 4 + u32_at(&raw, offset) as usize;
                let mut cursor = offset + 4;
                while cursor < end {
                    let run = u16::from_le_bytes([raw[cursor], raw[cursor + 1]]); cursor += 2;
                    if run & 0x8000 == 0 {
                        let value = u16::from_le_bytes([raw[cursor], raw[cursor + 1]]); cursor += 2;
                        packed_pixels.extend(std::iter::repeat_n(value, run as usize));
                    } else {
                        for _ in 0..run.wrapping_neg() { packed_pixels.push(u16::from_le_bytes([raw[cursor], raw[cursor + 1]])); cursor += 2; }
                    }
                }
                assert_eq!(cursor, end);
            } else { packed_pixels.extend(raw[offset..].chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]]))) }
            assert_eq!(packed_pixels.len(), 16384);
            for (p, rgba) in packed_pixels.iter().zip(m.texture.chunks_exact(4)) {
                for (c, byte) in rgba[..3].iter().enumerate() { let v = ((p >> (5 * c)) & 31) as u8; assert_eq!(*byte, (v << 3) | (v >> 2)); }
                assert_eq!(rgba[3], if p & 0x8000 == 0 { 0 } else { 255 });
            }
        }
        println!("{}: {} vertices / {} shapes / flags {} / texture@{offset:#x} / {}", save.folder, m.vertices.len(), m.shapes.len(), m.texture_type, save.title.replace('\n', " / "));
    }
    assert_eq!(saves.len(), descriptors);
}
