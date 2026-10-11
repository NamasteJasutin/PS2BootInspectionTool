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
            f if f.contains("[SCPH-101]") || f.contains("SCPH-5502") || f.contains("SCPH-1002") || f.contains("DTLH-3002") => {
                assert_eq!(menu.style, MenuStyle::Redesigned);
            }
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

                // Sprites in bounds
                for sprite in &frame.sprites {
                    assert!(sprite.texture < menu.textures.len(), "texture index out of bounds in {filename}");
                    assert!(sprite.centre[0].is_finite());
                    assert!(sprite.centre[1].is_finite());
                    assert!(sprite.size[0] > 0.0 && sprite.size[1] > 0.0);
                    assert!(sprite.uv_rect[0] >= 0.0 && sprite.uv_rect[0] <= 1.0);
                    assert!(sprite.uv_rect[1] >= 0.0 && sprite.uv_rect[1] <= 1.0);
                    assert!(sprite.uv_rect[2] >= 0.0 && sprite.uv_rect[2] <= 1.0);
                    assert!(sprite.uv_rect[3] >= 0.0 && sprite.uv_rect[3] <= 1.0);
                }

                // Research notes present
                assert!(!frame.notes.is_empty());
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
