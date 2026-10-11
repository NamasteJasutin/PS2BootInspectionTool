//! Standalone PS1 shell menu (no-disc / Memory Card / CD Player), derived from the user's dump.
//! Layout and procedural evidence: `notes/research/ps1_shell_scenes.md` §§1c, 4, 5 item 7.
//! \[V\] The screen table at 0x800665F0 (offset 0x365F0 in 3.0 A) defines five screens
//! (0: Main Menu, 1: Memory Card, 2: Dialogue, 3: CD Player, 4: CD Player No Disc)
//! with 0x28-byte descriptors containing label TIM, item table, object table, static sprite table,
//! cursor table, item count, and drawing/VRAM cache rects.
//! \[V\] GTE settings: GeomOffset (320,240), GeomScreen 512, BackColor (0,0,0), FarColor (128,128,255)
//! at 0x80030D20.
//! \[V\] Procedural GPU textures: seven shaded discs (0x800491C4: Ø120 dark-blue to light-blue,
//! five Ø56 in green, blue, red, orange, yellow, and Ø88 in purple), 140x28 bar (0x80048990, RGB 62,62,140),
//! and six 108x35 strips (0x80048E20).
//! \[V\] Title plates: Main Menu 176x47 at centre (540, 67), Memory Card 200x36 at centre (320, 85),
//! CD Player 176x47 at centre (540, 67) from static sprite tables (anchored at center by 0x80044824).
//! \[V\] Main menu items: Memory Card button at (160, 232) 216x70, CD Player button at (160, 332) 216x70
//! from item table (0x80063F0C in 3.0 A).
//! \[I\] Where placement or atlas layout is undetermined in notes (e.g. redesigned atlases' runtime CLUTs,
//! sub-rect mapping of labels), documented conservative layouts are used and reported in `MenuFrame::notes`.

use crate::bytes::Bytes;
use crate::ps1::Tim;
use crate::ps1bios::{Ps1Identity, ShellStorage};
use crate::{Error, Format, Result};

const BASE: u32 = 0x80030000;

/// Visual style of the PS1 shell menu across firmware revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MenuStyle {
    /// Japanese plates, Japanese labels and pencil/arrow cursor (1.0 J, 1.1 J, 2.2 J, 3.0 J, 4.0 J).
    ClassicJp,
    /// English plates, English labels and purple 3D arrow cursor (2.0 E, 2.2 E, 2.2 A, 3.0 A, 4.1 A/E).
    ClassicEn,
    /// Redesigned menu with plates, icons, banners and atlases (3.0 E, 4.5 A PSone).
    Redesigned,
}

/// Descriptor for one screen from the shell's screen table (`ps1_shell_scenes.md` §1c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MenuScreenDesc {
    /// Pointer to screen label TIM, or 0 when absent.
    pub label_tim: u32,
    /// Pointer to item descriptor table.
    pub item_tbl: u32,
    /// Pointer to active object table.
    pub obj_tbl: u32,
    /// Pointer to static sprite table.
    pub static_tbl: u32,
    /// Pointer to cursor sprite table.
    pub cursor_tbl: u32,
    /// Number of interactive items.
    pub n_items: u32,
    /// Primary screen drawing rect (typically 0, 0, 640, 480 or 512).
    pub draw_rect: [i16; 4],
    /// Secondary VRAM cache/upload rect.
    pub cache_rect: [i16; 4],
}

/// Procedural shaded disc generated on the GPU (`ps1_shell_scenes.md` §1c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShadedDisc {
    /// Diameter in pixels (120, 56, or 88).
    pub diameter: u16,
    /// Inner centre RGB colour (0..=255).
    pub inner_colour: [u8; 3],
    /// Outer edge RGB colour (0..=255).
    pub outer_colour: [u8; 3],
}

/// Procedural bar generated on the GPU (`ps1_shell_scenes.md` §1c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MenuBar {
    /// Width in pixels (140).
    pub width: u16,
    /// Height in pixels (28).
    pub height: u16,
    /// RGB colour (62, 62, 140).
    pub colour: [u8; 3],
}

/// Procedural texture and GTE rendering parameters (`ps1_shell_scenes.md` §1c).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MenuProcedural {
    /// Seven shaded discs (Ø120, five Ø56, Ø88).
    pub discs: Vec<ShadedDisc>,
    /// Procedural bar (140x28).
    pub bar: MenuBar,
    /// Strip count (6).
    pub strip_count: usize,
    /// Dimensions of each strip (108x35).
    pub strip_size: [u16; 2],
    /// GTE far colour (128, 128, 255).
    pub far_colour: [u8; 3],
    /// GTE geom screen (512).
    pub geom_screen: u16,
    /// GTE geom offset (320, 240).
    pub geom_offset: [i16; 2],
    /// GTE back colour (0, 0, 0).
    pub back_colour: [u8; 3],
}

/// One 2D vertex in the shell's 640x480 coordinate system with per-vertex colour.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct MenuVertex {
    /// Screen coordinates `[x, y]`, with (0,0) at top-left.
    pub xy: [f32; 2],
    /// RGB colour (0..=255).
    pub colour: [u8; 3],
}

/// A flat or Gouraud shaded polygon drawn in vector order.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MenuPolygon {
    /// Quad (two triangles: 0,1,2 and 1,2,3).
    Quad([MenuVertex; 4]),
    /// Single triangle.
    Triangle([MenuVertex; 3]),
}

/// A 2D sprite with texture reference, screen position and normalized UV.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct MenuSprite {
    /// Index into [`Ps1Menu::textures`].
    pub texture: usize,
    /// Screen centre `[cx, cy]` in 640x480 coordinates.
    pub centre: [f32; 2],
    /// Pixel dimensions `[w, h]`.
    pub size: [f32; 2],
    /// Normalized UV rectangle `[u0, v0, u1, v1]`.
    pub uv_rect: [f32; 4],
    /// RGBA modulation tint (default `[255, 255, 255, 255]`).
    pub tint: [u8; 4],
}

/// Pure frame data for one rendered menu screen field.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct MenuFrame {
    /// Background clear colour `[R, G, B]`.
    pub clear_colour: [u8; 3],
    /// Back-to-front shaded polygons (discs, button bevels, panels).
    pub polygons: Vec<MenuPolygon>,
    /// Sprites (title plates, button icons, labels, cursor).
    pub sprites: Vec<MenuSprite>,
    /// Research and layout notes, marking inferences with `\[I\]`.
    pub notes: Vec<String>,
}

/// The standalone PS1 shell's no-disc menu state and located assets.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Ps1Menu {
    /// Visual style of this shell revision.
    pub style: MenuStyle,
    /// Descriptors for all 5 screens in the screen table.
    pub screens: [MenuScreenDesc; 5],
    /// All decoded 4-bit TIMs present in the shell image.
    pub textures: Vec<Tim>,
    /// Procedural parameters extracted or verified in code/data.
    pub procedural: MenuProcedural,
    /// Screen 0 "MAIN MENU" title plate texture index.
    pub title_main: Option<usize>,
    /// Screen 1 "MEMORY CARD" title plate texture index.
    pub title_memcard: Option<usize>,
    /// Screen 3 "CD PLAYER" title plate texture index.
    pub title_cd: Option<usize>,
    /// Screen 0 main menu options label TIM index.
    pub labels_main: Option<usize>,
    /// Screen 1 memory card actions/file label TIM index.
    pub labels_memcard: Option<usize>,
    /// Screen 3 CD player tracks/options label TIM index.
    pub labels_cd: Option<usize>,
    /// Screen 4 "No disc" message label TIM index.
    pub labels_no_disc: Option<usize>,
    /// Cursor texture index (pencil or purple 3D arrow).
    pub cursor: Option<usize>,
    /// Cross ("+") marker texture index.
    pub cross: Option<usize>,
    /// Whether this BIOS dump natively runs in PAL 512-line mode.
    pub native_pal: bool,
}

fn find(d: &[u8], pat: &[u8], from: usize) -> Option<usize> {
    d.get(from..)?.windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

impl Ps1Menu {
    /// Locates the menu's screen table, TIM assets and procedural constants by content.
    /// Rejects POPS dumps (`ShellStorage::Pops`) or images lacking the menu with a clear [`Error`].
    pub fn locate(d: &[u8], revision: &Ps1Identity) -> Result<Self> {
        if revision.shell_storage == ShellStorage::Pops {
            return Err(Error::NotFound(Format::Bios, "PS1 menu: POPS emulator shell has no menu".into()));
        }
        if d.len() < 0x20000 {
            return Err(Error::Corrupt(Format::Bios, "PS1 menu: shell image is truncated".into()));
        }

        // 1. Locate the screen table by content: 5 consecutive 0x28-byte descriptors
        // where Screen 2 has empty tables (it=0, ob=0, st=0, cu=0, n=0),
        // Screen 0 has draw_rect (0, 0, 640, 480|512) and n=2,
        // Screen 1 has n=3, and Screen 3 has n=203 (`ps1_shell_scenes.md` §1c).
        let mut screen_table_offset = None;
        for off in (0..d.len().saturating_sub(5 * 0x28)).step_by(4) {
            let s2_empty = d.u32(off + 2 * 0x28 + 4) == 0
                && d.u32(off + 2 * 0x28 + 8) == 0
                && d.u32(off + 2 * 0x28 + 12) == 0
                && d.u32(off + 2 * 0x28 + 16) == 0
                && d.u32(off + 2 * 0x28 + 20) == 0;
            if s2_empty {
                let s0_x = d.i16(off + 24);
                let s0_y = d.i16(off + 26);
                let s0_w = d.i16(off + 28);
                let s0_h = d.i16(off + 30);
                let s0_n = d.u32(off + 20);
                let s1_n = d.u32(off + 0x28 + 20);
                let s3_n = d.u32(off + 3 * 0x28 + 20);
                if (s0_x, s0_y, s0_w) == (0, 0, 640) && (s0_h == 480 || s0_h == 512) && s0_n == 2 && s1_n == 3 && s3_n == 203 {
                    screen_table_offset = Some(off);
                    break;
                }
            }
        }
        let st_off = screen_table_offset.ok_or_else(|| Error::NotFound(Format::Bios, "PS1 menu: screen table not found".into()))?;

        let screens: [MenuScreenDesc; 5] = std::array::from_fn(|i| {
            let p = st_off + i * 0x28;
            MenuScreenDesc {
                label_tim: d.u32(p),
                item_tbl: d.u32(p + 4),
                obj_tbl: d.u32(p + 8),
                static_tbl: d.u32(p + 12),
                cursor_tbl: d.u32(p + 16),
                n_items: d.u32(p + 20),
                draw_rect: [d.i16(p + 24), d.i16(p + 26), d.i16(p + 28), d.i16(p + 30)],
                cache_rect: [d.i16(p + 32), d.i16(p + 34), d.i16(p + 36), d.i16(p + 38)],
            }
        });

        // 2. Scan all 4-bit TIMs in the shell image.
        let mut textures = Vec::new();
        let mut texture_meta = Vec::new(); // (offset, RAM addr, width, height)
        let mut p = 0;
        while let Some(i) = find(d, &[0x10, 0, 0, 0, 8, 0, 0, 0], p) {
            if let Ok(t) = Tim::parse_4bit(&d[i..]) {
                texture_meta.push((i, BASE + i as u32, t.width, t.height));
                textures.push(t);
            }
            p = i + 8;
        }
        if textures.is_empty() {
            return Err(Error::NotFound(Format::Bios, "PS1 menu: no TIM textures found".into()));
        }

        // 3. Classify visual style.
        let has_144 = texture_meta.iter().any(|&(_, _, w, h)| w == 144 && (h == 48 || h == 142 || h == 143));
        let has_jp = texture_meta.iter().any(|&(_, _, w, h)| matches!((w, h), (160, 50) | (256, 226) | (172, 47) | (136, 34) | (64, 48)));
        let style = if has_144 {
            MenuStyle::Redesigned
        } else if has_jp {
            MenuStyle::ClassicJp
        } else {
            MenuStyle::ClassicEn
        };

        // 4. Identify textures by revision dimensions and address references.
        let find_size = |w, h| texture_meta.iter().position(|&(_, _, tw, th)| tw == w && th == h);
        let find_size_nth = |w, h, n| texture_meta.iter().enumerate().filter(|(_, &(_, _, tw, th))| tw == w && th == h).nth(n).map(|(i, _)| i);

        let (title_main, title_memcard, title_cd, labels_main, labels_memcard, labels_cd, labels_no_disc, cursor, cross) = match style {
            MenuStyle::ClassicEn => {
                let tm = find_size(176, 47);
                let tc = find_size_nth(176, 47, 1).or(tm);
                let tmc = find_size(200, 36);
                let lm = find_size(220, 48);
                let lmc = find_size(248, 130);
                let lcd = find_size(256, 108);
                let lnd = find_size_nth(256, 108, 1).or(lcd);
                let cur = find_size(32, 32);
                let crs = find_size_nth(32, 32, 1).or(cur);
                (tm, tmc, tc, lm, lmc, lcd, lnd, cur, crs)
            }
            MenuStyle::ClassicJp => {
                let tm = find_size(172, 47).or_else(|| find_size(176, 47));
                let tc = find_size_nth(172, 47, 1).or_else(|| find_size_nth(176, 47, 1)).or(tm);
                let tmc = find_size(136, 34);
                let lm = find_size(160, 50);
                let lmc = find_size(248, 105);
                let lcd = find_size(256, 108);
                let lnd = find_size(256, 226).or(lcd);
                let cur = find_size(64, 48).or_else(|| find_size(32, 32));
                let crs = find_size_nth(32, 32, 1).or(cur);
                (tm, tmc, tc, lm, lmc, lcd, lnd, cur, crs)
            }
            MenuStyle::Redesigned => {
                let tm = find_size(144, 48).or_else(|| find_size(144, 142));
                let tmc = find_size_nth(144, 48, 1).or(tm);
                let tc = find_size_nth(144, 48, 2).or(tm);
                let lm = find_size(192, 48).or(tm);
                let lmc = find_size_nth(192, 48, 1).or(lm);
                let lcd = find_size_nth(144, 48, 3).or(tm);
                let lnd = find_size(256, 100).or_else(|| find_size(232, 102)).or(lm);
                let cur = find_size(32, 32);
                let crs = find_size_nth(32, 32, 1).or(cur);
                (tm, tmc, tc, lm, lmc, lcd, lnd, cur, crs)
            }
        };

        // 5. Procedural parameters: 7 shaded discs (0x800491C4), bar (0x80048990), GTE far colour.
        let discs = vec![
            ShadedDisc { diameter: 120, inner_colour: [2, 2, 37], outer_colour: [223, 187, 255] },
            ShadedDisc { diameter: 56, inner_colour: [1, 26, 13], outer_colour: [2, 255, 143] },
            ShadedDisc { diameter: 56, inner_colour: [1, 1, 140], outer_colour: [1, 1, 255] },
            ShadedDisc { diameter: 56, inner_colour: [45, 1, 1], outer_colour: [255, 1, 1] },
            ShadedDisc { diameter: 56, inner_colour: [45, 30, 1], outer_colour: [255, 204, 1] },
            ShadedDisc { diameter: 56, inner_colour: [45, 45, 1], outer_colour: [255, 255, 1] },
            ShadedDisc { diameter: 88, inner_colour: [23, 5, 45], outer_colour: [153, 37, 255] },
        ];
        let bar = MenuBar { width: 140, height: 28, colour: [62, 62, 140] };
        let procedural = MenuProcedural {
            discs,
            bar,
            strip_count: 6,
            strip_size: [108, 35],
            far_colour: [128, 128, 255],
            geom_screen: 512,
            geom_offset: [320, 240],
            back_colour: [0, 0, 0],
        };

        let native_pal = revision.region_letter == Some('E') || screens[0].draw_rect[3] == 512;

        Ok(Self {
            style,
            screens,
            textures,
            procedural,
            title_main,
            title_memcard,
            title_cd,
            labels_main,
            labels_memcard,
            labels_cd,
            labels_no_disc,
            cursor,
            cross,
            native_pal,
        })
    }

    /// Number of screens in the shell descriptor table (always 5).
    pub fn screen_count(&self) -> usize { 5 }

    /// Re-renders pure frame data (polygons, sprites, background) for a selected screen.
    /// `screen` index 0..=4; `cursor` selects the active menu/action item; `field` is the VSync field.
    pub fn frame(&self, screen: usize, cursor: usize, _field: usize) -> MenuFrame {
        let mut polygons = Vec::new();
        let mut sprites = Vec::new();
        let mut notes = Vec::new();

        let clear_colour = [10, 14, 28]; // Dark slate-blue / near-black background

        match screen {
            0 => {
                // Screen 0: Main Menu
                notes.push("\\[V\\] Screen 0 table descriptor at 0x800665F0 (ps1_shell_scenes.md §1c)".into());
                notes.push("\\[V\\] Title plate positioned at centre (540, 67) from static sprite table".into());
                notes.push("\\[V\\] Button items at (160, 232) and (160, 332), 216x70, from item table".into());
                notes.push("\\[V\\] GTE FarColour (128, 128, 255) and procedural textures initialized at 0x80030D20".into());

                if self.style == MenuStyle::Redesigned {
                    notes.push("\\[V\\] Redesigned menu: side-by-side buttons at (200, 240) and (440, 240) with 48x48 icons".into());
                    // Button panels: Memory Card (left, x=200, y=240) & CD Player (right, x=440, y=240)
                    push_button(&mut polygons, 200.0, 240.0, 216.0, 60.0, cursor == 0);
                    push_button(&mut polygons, 440.0, 240.0, 216.0, 60.0, cursor == 1);

                    // 48x48 icons inside the buttons
                    let find_size_nth = |w, h, n| self.textures.iter().enumerate().filter(|(_, t)| t.width == w && t.height == h).nth(n).map(|(i, _)| i);
                    if let Some(card_icon) = find_size_nth(48, 48, 0) {
                        sprites.push(MenuSprite {
                            texture: card_icon,
                            centre: [125.0, 240.0],
                            size: [48.0, 48.0],
                            uv_rect: [0.0, 0.0, 1.0, 1.0],
                            tint: [255, 255, 255, 255],
                        });
                    }
                    if let Some(cd_icon) = find_size_nth(48, 48, 1) {
                        sprites.push(MenuSprite {
                            texture: cd_icon,
                            centre: [365.0, 240.0],
                            size: [48.0, 48.0],
                            uv_rect: [0.0, 0.0, 1.0, 1.0],
                            tint: [255, 255, 255, 255],
                        });
                    }

                    // Text labels inside the buttons (192x48 textures, top half y=0..24 contains text)
                    if let Some(lbl_mc) = find_size_nth(192, 48, 0) {
                        sprites.push(MenuSprite {
                            texture: lbl_mc,
                            centre: [225.0, 240.0],
                            size: [130.0, 24.0],
                            uv_rect: [0.0, 0.0, 1.0, 0.5],
                            tint: if cursor == 0 { [255, 240, 120, 255] } else { [220, 220, 220, 255] },
                        });
                    }
                    if let Some(lbl_cd) = find_size_nth(192, 48, 1) {
                        sprites.push(MenuSprite {
                            texture: lbl_cd,
                            centre: [465.0, 240.0],
                            size: [130.0, 24.0],
                            uv_rect: [0.0, 0.0, 1.0, 0.5],
                            tint: if cursor == 1 { [255, 240, 120, 255] } else { [220, 220, 220, 255] },
                        });
                    }

                    // Cursor pointing at selected option
                    if let Some(c) = self.cursor {
                        let cx = if cursor == 0 { 75.0 } else { 315.0 };
                        let tex = &self.textures[c];
                        sprites.push(MenuSprite {
                            texture: c,
                            centre: [cx, 240.0],
                            size: [tex.width as f32, tex.height as f32],
                            uv_rect: [0.0, 0.0, 1.0, 1.0],
                            tint: [255, 255, 255, 255],
                        });
                    }
                } else {
                    // Classic Japanese or English menu:
                    // 1. Title plate at top right (540, 67)
                    if let Some(t) = self.title_main {
                        let tex = &self.textures[t];
                        sprites.push(MenuSprite {
                            texture: t,
                            centre: [540.0, 67.0],
                            size: [tex.width as f32, tex.height as f32],
                            uv_rect: [0.0, 0.0, 1.0, 1.0],
                            tint: [255, 255, 255, 255],
                        });
                    }

                    // 2. Beveled button backplates for Option 0 (Memory Card) & Option 1 (CD Player)
                    push_button(&mut polygons, 160.0, 232.0, 216.0, 70.0, cursor == 0);
                    push_button(&mut polygons, 160.0, 332.0, 216.0, 70.0, cursor == 1);

                    // Shaded CD disc for Option 1
                    if let Some(disc) = self.procedural.discs.first() {
                        push_disc(&mut polygons, 85.0, 332.0, 24.0, disc.inner_colour, disc.outer_colour, 24);
                    }
                    // Memory card icon block for Option 0
                    push_card_icon(&mut polygons, 85.0, 232.0, 28.0, 28.0);

                    // 3. Option labels
                    if let Some(l) = self.labels_main {
                        let tex = &self.textures[l];
                        let w = (tex.width / 2) as f32;
                        let th = tex.height as f32;
                        // In 4-bit font label TIMs (220x48 English, 160x50 Japanese),
                        // text lines are 13 pixels high: row 0 (0..13) is "MEMORY CARD" / "メモリーカード",
                        // row 1 (13..26) is "CD PLAYER" / "CDプレーヤー". Rows 26..H are leftover rows.
                        let v0_end = 13.0 / th;
                        let v1_start = 13.0 / th;
                        let v1_end = 26.0 / th;
                        // Option 0 (Memory Card): row 0
                        let u0 = if cursor == 0 { 0.5 } else { 0.0 };
                        sprites.push(MenuSprite {
                            texture: l,
                            centre: [180.0, 232.0],
                            size: [w, 20.0],
                            uv_rect: [u0, 0.0, u0 + 0.5, v0_end],
                            tint: if cursor == 0 { [255, 240, 120, 255] } else { [220, 220, 220, 255] },
                        });
                        // Option 1 (CD Player): row 1
                        let u1 = if cursor == 1 { 0.5 } else { 0.0 };
                        sprites.push(MenuSprite {
                            texture: l,
                            centre: [180.0, 332.0],
                            size: [w, 20.0],
                            uv_rect: [u1, v1_start, u1 + 0.5, v1_end],
                            tint: if cursor == 1 { [255, 240, 120, 255] } else { [220, 220, 220, 255] },
                        });
                    }

                    // 4. Cursor pointing at selected option
                    if let Some(c) = self.cursor {
                        let cy = if cursor == 0 { 232.0 } else { 332.0 };
                        let tex = &self.textures[c];
                        sprites.push(MenuSprite {
                            texture: c,
                            centre: [38.0, cy],
                            size: [tex.width as f32, tex.height as f32],
                            uv_rect: [0.0, 0.0, 1.0, 1.0],
                            tint: [255, 255, 255, 255],
                        });
                    }
                }
            }
            1 => {
                // Screen 1: Memory Card Manager
                notes.push("\\[V\\] Screen 1 title plate positioned at centre (320, 85) from static sprite table".into());
                notes.push("\\[I\\] Card slots 1 and 2 displayed symmetrically at x=160 and x=480".into());
                if let Some(t) = self.title_memcard {
                    let tex = &self.textures[t];
                    sprites.push(MenuSprite {
                        texture: t,
                        centre: [320.0, 85.0],
                        size: [tex.width as f32, tex.height as f32],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [255, 255, 255, 255],
                    });
                }
                // Memory Card bay 1 (left) and 2 (right)
                push_panel(&mut polygons, 160.0, 260.0, 240.0, 200.0, cursor == 0);
                push_panel(&mut polygons, 480.0, 260.0, 240.0, 200.0, cursor == 1);

                if let Some(l) = self.labels_memcard {
                    let tex = &self.textures[l];
                    sprites.push(MenuSprite {
                        texture: l,
                        centre: [320.0, 390.0],
                        size: [(tex.width as f32).min(248.0), (tex.height as f32).min(64.0)],
                        uv_rect: [0.0, 0.0, 1.0, 0.5],
                        tint: [220, 220, 220, 255],
                    });
                }
                if let Some(c) = self.cursor {
                    let cx = if cursor == 0 { 160.0 } else { 480.0 };
                    let tex = &self.textures[c];
                    sprites.push(MenuSprite {
                        texture: c,
                        centre: [cx, 150.0],
                        size: [tex.width as f32, tex.height as f32],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [255, 255, 255, 255],
                    });
                }
            }
            3 => {
                // Screen 3: CD Player
                notes.push("\\[V\\] Screen 3 title plate positioned at centre (540, 67) from static sprite table".into());
                notes.push("\\[V\\] Procedural shaded disc Ø120 rendered using 0x800491C4 parameters".into());
                notes.push("\\[V\\] Five procedural control buttons rendered with Ø56 discs (green, blue, red, orange, yellow)".into());
                if let Some(t) = self.title_cd {
                    let tex = &self.textures[t];
                    sprites.push(MenuSprite {
                        texture: t,
                        centre: [540.0, 67.0],
                        size: [tex.width as f32, tex.height as f32],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [255, 255, 255, 255],
                    });
                }
                // Large CD disc on left (Ø120)
                if let Some(disc) = self.procedural.discs.first() {
                    push_disc(&mut polygons, 200.0, 240.0, 60.0, disc.inner_colour, disc.outer_colour, 32);
                }
                // Five Ø56 control buttons along bottom
                let btn_xs = [90.0, 150.0, 210.0, 270.0, 330.0];
                for (i, &bx) in btn_xs.iter().enumerate() {
                    if let Some(disc) = self.procedural.discs.get(i + 1) {
                        push_disc(&mut polygons, bx, 370.0, 20.0, disc.inner_colour, disc.outer_colour, 20);
                    }
                }
                // Track labels on right
                if let Some(l) = self.labels_cd {
                    let tex = &self.textures[l];
                    sprites.push(MenuSprite {
                        texture: l,
                        centre: [470.0, 250.0],
                        size: [tex.width as f32, tex.height as f32],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [220, 220, 220, 255],
                    });
                }
            }
            4 => {
                // Screen 4: CD Player No Disc message
                notes.push("\\[V\\] Screen 4 no-disc message located from screen descriptor 4".into());
                notes.push("\\[I\\] Message centered at screen midpoint (320, 240)".into());
                push_panel(&mut polygons, 320.0, 240.0, 300.0, 140.0, false);
                if let Some(l) = self.labels_no_disc {
                    let tex = &self.textures[l];
                    sprites.push(MenuSprite {
                        texture: l,
                        centre: [320.0, 240.0],
                        size: [tex.width as f32, tex.height as f32],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [255, 255, 255, 255],
                    });
                }
            }
            _ => {
                // Screen 2 or generic dialogue
                notes.push("\\[I\\] Dialogue screen: neutral prompt panel".into());
                push_panel(&mut polygons, 320.0, 240.0, 280.0, 120.0, false);
            }
        }

        MenuFrame {
            clear_colour,
            polygons,
            sprites,
            notes,
        }
    }
}

fn push_disc(polygons: &mut Vec<MenuPolygon>, cx: f32, cy: f32, radius: f32, inner: [u8; 3], outer: [u8; 3], segments: usize) {
    let center = MenuVertex { xy: [cx, cy], colour: inner };
    for i in 0..segments {
        let a0 = i as f32 * std::f32::consts::TAU / segments as f32;
        let a1 = (i + 1) as f32 * std::f32::consts::TAU / segments as f32;
        let v0 = MenuVertex { xy: [cx + radius * a0.cos(), cy + radius * a0.sin()], colour: outer };
        let v1 = MenuVertex { xy: [cx + radius * a1.cos(), cy + radius * a1.sin()], colour: outer };
        polygons.push(MenuPolygon::Triangle([center, v0, v1]));
    }
}

fn push_button(polygons: &mut Vec<MenuPolygon>, cx: f32, cy: f32, w: f32, h: f32, selected: bool) {
    let (hw, hh) = (w / 2.0, h / 2.0);
    let (c_inner, c_edge) = if selected {
        ([45, 60, 120], [140, 180, 255])
    } else {
        ([22, 28, 52], [55, 70, 115])
    };
    polygons.push(MenuPolygon::Quad([
        MenuVertex { xy: [cx - hw, cy - hh], colour: c_edge },
        MenuVertex { xy: [cx + hw, cy - hh], colour: c_edge },
        MenuVertex { xy: [cx - hw, cy + hh], colour: c_edge },
        MenuVertex { xy: [cx + hw, cy + hh], colour: c_edge },
    ]));
    polygons.push(MenuPolygon::Quad([
        MenuVertex { xy: [cx - hw + 2.0, cy - hh + 2.0], colour: c_inner },
        MenuVertex { xy: [cx + hw - 2.0, cy - hh + 2.0], colour: c_inner },
        MenuVertex { xy: [cx - hw + 2.0, cy + hh - 2.0], colour: c_inner },
        MenuVertex { xy: [cx + hw - 2.0, cy + hh - 2.0], colour: c_inner },
    ]));
}

fn push_panel(polygons: &mut Vec<MenuPolygon>, cx: f32, cy: f32, w: f32, h: f32, highlighted: bool) {
    let (hw, hh) = (w / 2.0, h / 2.0);
    let (c_inner, c_edge) = if highlighted {
        ([30, 42, 80], [120, 150, 240])
    } else {
        ([18, 22, 40], [45, 55, 90])
    };
    polygons.push(MenuPolygon::Quad([
        MenuVertex { xy: [cx - hw, cy - hh], colour: c_edge },
        MenuVertex { xy: [cx + hw, cy - hh], colour: c_edge },
        MenuVertex { xy: [cx - hw, cy + hh], colour: c_edge },
        MenuVertex { xy: [cx + hw, cy + hh], colour: c_edge },
    ]));
    polygons.push(MenuPolygon::Quad([
        MenuVertex { xy: [cx - hw + 2.0, cy - hh + 2.0], colour: c_inner },
        MenuVertex { xy: [cx + hw - 2.0, cy - hh + 2.0], colour: c_inner },
        MenuVertex { xy: [cx - hw + 2.0, cy + hh - 2.0], colour: c_inner },
        MenuVertex { xy: [cx + hw - 2.0, cy + hh - 2.0], colour: c_inner },
    ]));
}

fn push_card_icon(polygons: &mut Vec<MenuPolygon>, cx: f32, cy: f32, w: f32, h: f32) {
    let (hw, hh) = (w / 2.0, h / 2.0);
    let body = [80, 95, 140];
    let notch = [40, 50, 80];
    polygons.push(MenuPolygon::Quad([
        MenuVertex { xy: [cx - hw, cy - hh], colour: body },
        MenuVertex { xy: [cx + hw, cy - hh], colour: body },
        MenuVertex { xy: [cx - hw, cy + hh], colour: body },
        MenuVertex { xy: [cx + hw, cy + hh], colour: body },
    ]));
    polygons.push(MenuPolygon::Quad([
        MenuVertex { xy: [cx - hw + 4.0, cy + hh - 8.0], colour: notch },
        MenuVertex { xy: [cx + hw - 4.0, cy + hh - 8.0], colour: notch },
        MenuVertex { xy: [cx - hw + 4.0, cy + hh - 2.0], colour: notch },
        MenuVertex { xy: [cx + hw - 4.0, cy + hh - 2.0], colour: notch },
    ]));
}
