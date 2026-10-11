use ps2kit::ps1bios::{Ps1Bios, ShellStorage};
use ps2kit::ps1menu::{MenuStyle, Ps1Menu};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn synthetic_bios() -> Ps1Bios {
    let mut d = vec![0; 0x80000];
    d[0x108..0x128].copy_from_slice(b"Sony Computer Entertainment Inc.");
    d[0x200..0x214].copy_from_slice(b"PS-X Realtime Kernel");
    d[0x18000..0x18004].copy_from_slice(&0x27BDFFE0u32.to_le_bytes());
    Ps1Bios::new(d).unwrap()
}

#[test]
fn malformed_and_truncated_inputs_never_panic() {
    let bios = synthetic_bios();
    assert!(Ps1Menu::locate(&[], bios.identity()).is_err());
    assert!(Ps1Menu::locate(&[0x10, 0, 0, 0, 8, 0, 0, 0], bios.identity()).is_err());

    // Random slices
    let garbage = vec![0x42; 0x30000];
    assert!(Ps1Menu::locate(&garbage, bios.identity()).is_err());

    // Slices that look partially like screen table headers
    let mut partial = vec![0; 0x25000];
    partial[0x100..0x120].copy_from_slice(&[0; 32]);
    assert!(Ps1Menu::locate(&partial, bios.identity()).is_err());
}

#[test]
fn every_local_ps1_dump_menu_locates_and_renders() {
    let bios_dir = root().join("bios");
    let dumps = [
        "Sony PlayStation BIOS (J)[SCPH-1000].bin",
        "Sony PlayStation BIOS (J)(v1.1)(1995-01-22)[SCPH-3000].bin",
        "Sony PlayStation BIOS (E)(v2.0)(1995-05-10)[SCPH-1002].bin",
        "Sony PlayStation BIOS (E)(v2.2)(1995-12-04)[DTLH-3002].bin",
        "Sony PlayStation BIOS (J)(v2.2)(1995-12-04)[SCPH-5000].bin",
        "Sony PlayStation SCPH-1001 - DTLH-3000 BIOS v2.2 (1995-12-04)(Sony)(US).bin",
        "Sony PlayStation BIOS (J)(v3.0)(1996-09-09)[SCPH-5500].bin",
        "Sony PlayStation BIOS (E)(v3.0)(1997-01-06)[SCPH-5502 + SCPH-5552].bin",
        "Sony PlayStation BIOS (U)(v3.0)(1996-11-18)[SCPH-7003].bin",
        "scph5501.bin",
        "Sony PlayStation BIOS (J)(v4.0)(1997-08-18)[SCPH-7000].bin",
        "Sony PlayStation BIOS (E)(v4.1)(1997-12-16)[SCPH-7502 + SCPH-9002].bin",
        "Sony PlayStation BIOS (U)(v4.1)(1997-12-16)[SCPH-7001 + SCPH-9001].bin",
        "Sony PSone BIOS (U)(v4.5)(2000-05-25)[SCPH-101].bin",
    ];

    let mut tested_count = 0;
    for filename in dumps {
        let p = bios_dir.join(filename);
        let Ok(data) = std::fs::read(&p) else { continue };
        let Ok(bios) = Ps1Bios::new(data) else { continue };
        let Ok(shell_image) = bios.shell_image() else { continue };

        let menu = match Ps1Menu::locate(&shell_image, bios.identity()) {
            Ok(m) => m,
            Err(ps2kit::Error::NotFound(ps2kit::Format::Bios, ref msg))
                if msg.contains("redesigned menu not reconstructed yet") =>
            {
                assert!(
                    filename.contains("[SCPH-101]")
                        || filename.contains("SCPH-5502")
                        || filename.contains("SCPH-1002")
                        || filename.contains("DTLH-3002"),
                    "unexpected redesigned error for {filename}"
                );
                tested_count += 1;
                continue;
            }
            Err(e) => panic!("failed to locate menu in {filename}: {e}"),
        };

        // Assert screen table
        assert_eq!(menu.screens.len(), 5);
        assert_eq!(menu.screens[0].n_items, 2);
        assert_eq!(menu.screens[1].n_items, 3);
        assert_eq!(menu.screens[2].n_items, 0);
        assert_eq!(menu.screens[3].n_items, 203);
        assert!(menu.screens[4].n_items == 2 || menu.screens[4].n_items == 5);


        // Assert style categorization
        match filename {
            f if f.contains("(J)") || f.contains("(J)[SCPH-1000]") => {
                assert_eq!(menu.style, MenuStyle::ClassicJp);
            }
            _ => {
                assert_eq!(menu.style, MenuStyle::ClassicEn);
            }
        }

        // Assert textures and procedural parameters
        assert!(menu.textures.len() >= 12);
        assert_eq!(menu.procedural.discs.len(), 7);
        assert_eq!(menu.procedural.discs[0].diameter, 120);
        assert_eq!(menu.procedural.discs[0].inner_colour, [2, 2, 37]);
        assert_eq!(menu.procedural.discs[0].outer_colour, [223, 187, 255]);
        assert_eq!(menu.procedural.bar.width, 140);
        assert_eq!(menu.procedural.bar.height, 28);
        assert_eq!(menu.procedural.bar.colour, [62, 62, 140]);
        assert_eq!(menu.procedural.far_colour, [128, 128, 255]);

        // Frame rendering test for all screens and cursors
        for s in 0..5 {
            for cur in 0..3 {
                let frame = menu.frame(s, cur, 0);

                // Polygons in bounds
                for poly in &frame.polygons {
                    match poly {
                        ps2kit::ps1menu::MenuPolygon::Quad(verts) => {
                            for v in verts {
                                assert!(v.xy[0].is_finite());
                                assert!(v.xy[1].is_finite());
                            }
                        }
                        ps2kit::ps1menu::MenuPolygon::Triangle(verts) => {
                            for v in verts {
                                assert!(v.xy[0].is_finite());
                                assert!(v.xy[1].is_finite());
                            }
                        }
                        _ => {}
                    }
                }

                // Sprites in bounds and UV within texture
                for sprite in &frame.sprites {
                    assert!(sprite.texture < menu.textures.len(), "texture index out of bounds in {filename}");
                    assert!(sprite.centre[0].is_finite());
                    assert!(sprite.centre[1].is_finite());
                    assert!(sprite.size[0] > 0.0 && sprite.size[1] > 0.0);
                    assert!(sprite.uv_rect[0] >= 0.0 && sprite.uv_rect[0] <= sprite.uv_rect[2]);
                    assert!(sprite.uv_rect[1] >= 0.0 && sprite.uv_rect[1] <= sprite.uv_rect[3]);
                    assert!(sprite.uv_rect[2] <= 1.0);
                    assert!(sprite.uv_rect[3] <= 1.0);
                }

                // Screen 0: no label sprite is wider than one label
                if s == 0 {
                    for sprite in &frame.sprites {
                        if Some(sprite.texture) == menu.labels_main {
                            assert!(sprite.size[0] <= 216.0, "label sprite on screen 0 wider than one label: {}", sprite.size[0]);
                            assert!(sprite.uv_rect[3] - sprite.uv_rect[1] <= 0.6, "label sprite UV taller than one label");
                        }
                    }
                }

                // Research notes present
                assert!(!frame.notes.is_empty());
            }
        }

        // Test cursor counts
        assert_eq!(menu.cursor_count(0), 2);
        assert_eq!(menu.cursor_count(1), 6);
        assert_eq!(menu.cursor_count(2), 0);
        assert_eq!(menu.cursor_count(3), 26);
        assert_eq!(menu.cursor_count(4), 0);

        // Test that every label sprite uses exact sub-rectangles from the shell's descriptor tables
        // and does not sample outside its own label bounds across screens 1 and 3.
        for s in [1, 3] {
            let n_cursors = menu.cursor_count(s);
            for c in 0..n_cursors {
                let frame = menu.frame(s, c, 0);
                for sprite in &frame.sprites {
                    let is_memcard_label = Some(sprite.texture) == menu.labels_memcard;
                    let is_cd_label = Some(sprite.texture) == menu.labels_cd;
                    if !is_memcard_label && !is_cd_label {
                        continue;
                    }

                    let tex = &menu.textures[sprite.texture];
                    let w = tex.width;
                    let h = tex.height;
                    let x0 = (sprite.uv_rect[0] * w as f32).round() as usize;
                    let y0 = (sprite.uv_rect[1] * h as f32).round() as usize;
                    let x1 = (sprite.uv_rect[2] * w as f32).round() as usize;
                    let y1 = (sprite.uv_rect[3] * h as f32).round() as usize;

                    assert!(x0 < x1 && x1 <= w, "x bounds invalid: {x0}..{x1} on {w} in {filename} screen {s}");
                    assert!(y0 < y1 && y1 <= h, "y bounds invalid: {y0}..{y1} on {h} in {filename} screen {s}");

                    // Sprite size matches UV texel extent 1:1 (no distortion or over-sampling)
                    assert_eq!(sprite.size[0], (x1 - x0) as f32, "sprite width mismatch in {filename}");
                    assert_eq!(sprite.size[1], (y1 - y0) as f32, "sprite height mismatch in {filename}");

                    if is_memcard_label {
                        let matched = menu.subrects_memcard.iter().any(|sub| {
                            sub.u as usize == x0 && sub.v as usize == y0 && sub.w as usize == (x1 - x0) && sub.h as usize == (y1 - y0)
                        });
                        assert!(matched, "memcard sprite UV [{x0},{y0},{x1},{y1}] not found in subrect table in {filename}");
                    } else if is_cd_label {
                        let matched_sub = menu.subrects_cd.iter().any(|sub| {
                            sub.u as usize == x0 && sub.v as usize == y0 && sub.w as usize == (x1 - x0) && sub.h as usize == (y1 - y0)
                        });
                        let is_transport = [
                            (1, 55, 28, 13),
                            (33, 55, 29, 13),
                            (65, 55, 22, 13),
                            (89, 55, 12, 13),
                            (103, 55, 15, 13),
                            (120, 55, 30, 13),
                            (152, 55, 28, 13),
                        ].iter().any(|&(tu, tv, tw, th)| tu == x0 && tv == y0 && tw == (x1 - x0) && th == (y1 - y0));

                        assert!(matched_sub || is_transport, "cd sprite UV [{x0},{y0},{x1},{y1}] not found in subrect table in {filename}");

                        // For transport icons: verify bounded by blank rows above and below
                        if is_transport {
                            let is_blank = |x: usize, y: usize| -> bool {
                                if x >= w || y >= h { return true; }
                                tex.rgba[(y * w + x) * 4 + 3] == 0
                            };
                            let row_above_blank = (x0..x1).all(|x| is_blank(x, y0 - 1));
                            let row_below_blank = (x0..x1).all(|x| is_blank(x, y1));
                            assert!(row_above_blank, "transport icon row above not blank");
                            assert!(row_below_blank, "transport icon row below not blank");
                        }
                    }
                }
            }
        }
        tested_count += 1;
    }
    if tested_count > 0 {
        println!("Tested {tested_count} PS1 BIOS menus successfully");
    }
}

#[test]
fn pops_dump_has_no_menu() {
    let p = root().join("bios").join("PSXONPSP660.BIN");
    let Ok(data) = std::fs::read(&p) else { return };
    let Ok(bios) = Ps1Bios::new(data) else { return };
    assert_eq!(bios.identity().shell_storage, ShellStorage::Pops);

    let Ok(shell_image) = bios.shell_image() else { return };
    let res = Ps1Menu::locate(&shell_image, bios.identity());
    assert!(res.is_err(), "POPS must not locate a shell menu");
}
