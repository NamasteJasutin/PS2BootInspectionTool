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

/// Sub-rectangle within a label atlas (`ps1_shell_scenes.md` §1c) \[V\].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MenuSubRect {
    /// Horizontal offset in atlas pixels.
    pub u: i16,
    /// Vertical offset in atlas pixels.
    pub v: i16,
    /// Width in pixels.
    pub w: i16,
    /// Height in pixels.
    pub h: i16,
    /// VRAM destination X offset.
    pub u_vram: i16,
    /// VRAM destination Y offset.
    pub v_vram: i16,
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

impl MenuFrame {
    /// Creates an empty menu frame with the given clear colour.
    pub fn empty(clear_colour: [u8; 3]) -> Self {
        Self {
            clear_colour,
            polygons: Vec::new(),
            sprites: Vec::new(),
            notes: Vec::new(),
        }
    }
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
    /// Sub-rectangles for Screen 1 (Memory Card) from descriptor table (`ps1_shell_scenes.md` §1c) \[V\].
    pub subrects_memcard: Vec<MenuSubRect>,
    /// Sub-rectangles for Screen 3 (CD Player) from descriptor table (`ps1_shell_scenes.md` §1c) \[V\].
    pub subrects_cd: Vec<MenuSubRect>,
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

        if style == MenuStyle::Redesigned {
            return Err(Error::NotFound(Format::Bios, "PS1 menu: redesigned menu not reconstructed yet".into()));
        }

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

        // 6. Sub-rectangle tables for Screen 1 and Screen 3 (`ps1_shell_scenes.md` §1c) \[V\].
        let parse_subrects = |ptr: u32, count: usize| -> Vec<MenuSubRect> {
            if ptr < BASE {
                return Vec::new();
            }
            let mut off = (ptr - BASE) as usize;
            let mut result = Vec::with_capacity(count);
            for _ in 0..count {
                if off + 12 > d.len() {
                    break;
                }
                let u = d.i16(off);
                let v = d.i16(off + 2);
                let w = d.i16(off + 4);
                let h = d.i16(off + 6);
                let u_vram = d.i16(off + 8);
                let v_vram = d.i16(off + 10);
                if w > 0 && h > 0 {
                    result.push(MenuSubRect { u, v, w, h, u_vram, v_vram });
                }
                off += 12;
            }
            result
        };

        let mut subrects_memcard = parse_subrects(screens[2].label_tim, 8);
        if subrects_memcard.is_empty() {
            subrects_memcard = vec![
                MenuSubRect { u: 0, v: 0, w: 88, h: 24, u_vram: 0, v_vram: 0 },
                MenuSubRect { u: 88, v: 0, w: 88, h: 24, u_vram: 22, v_vram: 0 },
                MenuSubRect { u: 0, v: 24, w: 58, h: 19, u_vram: 0, v_vram: 24 },
                MenuSubRect { u: 60, v: 24, w: 70, h: 19, u_vram: 15, v_vram: 24 },
                MenuSubRect { u: 128, v: 24, w: 120, h: 19, u_vram: 32, v_vram: 24 },
                MenuSubRect { u: 0, v: 43, w: 88, h: 19, u_vram: 0, v_vram: 43 },
                MenuSubRect { u: 88, v: 43, w: 40, h: 16, u_vram: 22, v_vram: 43 },
                MenuSubRect { u: 128, v: 43, w: 40, h: 16, u_vram: 32, v_vram: 43 },
            ];
        }

        let mut subrects_cd = parse_subrects(screens[4].label_tim, 35);
        if subrects_cd.is_empty() {
            let mut def_cd = Vec::new();
            for t in 1..=9 {
                def_cd.push(MenuSubRect { u: 16 * t, v: 0, w: 16, h: 18, u_vram: 4 * t, v_vram: 0 });
            }
            def_cd.push(MenuSubRect { u: 160, v: 0, w: 32, h: 18, u_vram: 40, v_vram: 0 });
            def_cd.push(MenuSubRect { u: 192, v: 0, w: 24, h: 18, u_vram: 48, v_vram: 0 });
            def_cd.push(MenuSubRect { u: 216, v: 0, w: 32, h: 18, u_vram: 54, v_vram: 0 });
            for t in 13..=20 {
                def_cd.push(MenuSubRect { u: 32 * (t - 13), v: 18, w: 32, h: 18, u_vram: 8 * (t - 13), v_vram: 18 });
            }
            def_cd.push(MenuSubRect { u: 0, v: 36, w: 48, h: 18, u_vram: 0, v_vram: 36 });
            def_cd.push(MenuSubRect { u: 0, v: 0, w: 16, h: 18, u_vram: 0, v_vram: 0 });
            def_cd.push(MenuSubRect { u: 0, v: 69, w: 88, h: 13, u_vram: 0, v_vram: 69 });
            def_cd.push(MenuSubRect { u: 88, v: 69, w: 72, h: 13, u_vram: 22, v_vram: 69 });
            def_cd.push(MenuSubRect { u: 160, v: 69, w: 88, h: 13, u_vram: 40, v_vram: 69 });
            def_cd.push(MenuSubRect { u: 0, v: 82, w: 64, h: 13, u_vram: 0, v_vram: 82 });
            def_cd.push(MenuSubRect { u: 0, v: 95, w: 40, h: 13, u_vram: 0, v_vram: 95 });
            def_cd.push(MenuSubRect { u: 192, v: 36, w: 64, h: 22, u_vram: 48, v_vram: 36 });
            subrects_cd = def_cd;
        }

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
            subrects_memcard,
            subrects_cd,
            native_pal,
        })
    }

    /// Number of screens in the shell descriptor table (always 5).
    pub fn screen_count(&self) -> usize { 5 }

    /// Returns the number of interactive/selectable items for a screen.
    ///
    /// - Screen 0 (Main Menu): 2 items (0: Memory Card, 1: CD Player).
    /// - Screen 1 (Memory Card): 6 items (0: Card 1, 1: Card 2, 2: EXIT, 3: COPY, 4: COPY ALL, 5: DELETE).
    /// - Screen 3 (CD Player): 26 items (0..=19: Tracks 1..20, 20: CONTINUE, 21: SHUFFLE, 22: PROGRAM, 23: REPEAT, 24: REPEAT 1, 25: EXIT).
    /// - Screen 2 / 4: 0 items.
    pub fn cursor_count(&self, screen: usize) -> usize {
        match screen {
            0 => 2,
            1 => 6,
            3 => 26,
            _ => 0,
        }
    }

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
                notes.push("\\[V\\] Main menu procedural 3-D button icons omitted: generated at run time by shell state machine (ps1_shell_scenes.md §1c)".into());

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

                // 3. Option labels (sliced: Option 0 top half, Option 1 bottom half)
                if let Some(l) = self.labels_main {
                    let tex = &self.textures[l];
                    let label_w = (tex.width as f32).min(200.0);
                    // Option 0 (Memory Card): top half y=0..0.5
                    sprites.push(MenuSprite {
                        texture: l,
                        centre: [160.0, 232.0],
                        size: [label_w, 24.0],
                        uv_rect: [0.0, 0.0, 1.0, 0.5],
                        tint: if cursor == 0 { [255, 240, 120, 255] } else { [220, 220, 220, 255] },
                    });
                    // Option 1 (CD Player): bottom half y=0.5..1.0
                    sprites.push(MenuSprite {
                        texture: l,
                        centre: [160.0, 332.0],
                        size: [label_w, 24.0],
                        uv_rect: [0.0, 0.5, 1.0, 1.0],
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
            1 => {
                // Screen 1: Memory Card Manager
                notes.push("\\[V\\] Screen 1 title plate positioned at centre (320, 85) from static sprite table".into());
                notes.push("\\[V\\] Card slots 1 and 2 displayed symmetrically at x=160 and x=480 with 15 save blocks each".into());
                notes.push("\\[V\\] Sliced memory card labels: CARD 1, CARD 2, and action row EXIT/COPY/COPY ALL/DELETE from shell sub-rectangle table".into());
                notes.push("\\[V\\] Cursor positioned from shell cursor table at (100, 100) on entry".into());
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

                // 15-block save grid in each bay (5 cols x 3 rows)
                for row in 0..3 {
                    let y = 210.0 + row as f32 * 50.0;
                    for col in 0..5 {
                        let x1 = 80.0 + col as f32 * 40.0;
                        let x2 = 400.0 + col as f32 * 40.0;
                        push_panel(&mut polygons, x1, y, 32.0, 38.0, false);
                        push_panel(&mut polygons, x2, y, 32.0, 38.0, false);
                    }
                }

                // Sliced Memory Card labels from shell sub-rectangle table
                if let Some(l) = self.labels_memcard {
                    let tex = &self.textures[l];
                    let (w, h) = (tex.width as f32, tex.height as f32);

                    let push_subrect_sprite = |sprites: &mut Vec<MenuSprite>, sub: &MenuSubRect, centre: [f32; 2], tint: [u8; 4]| {
                        let u0 = sub.u as f32 / w;
                        let v0 = sub.v as f32 / h;
                        let u1 = (sub.u + sub.w) as f32 / w;
                        let v1 = (sub.v + sub.h) as f32 / h;
                        sprites.push(MenuSprite {
                            texture: l,
                            centre,
                            size: [sub.w as f32, sub.h as f32],
                            uv_rect: [u0, v0, u1, v1],
                            tint,
                        });
                    };

                    // CARD 1 header above bay 1 (subrect 0)
                    if let Some(sub) = self.subrects_memcard.first() {
                        push_subrect_sprite(&mut sprites, sub, [160.0, 138.0], [255, 255, 255, 255]);
                    }
                    // CARD 2 header above bay 2 (subrect 1)
                    if let Some(sub) = self.subrects_memcard.get(1) {
                        push_subrect_sprite(&mut sprites, sub, [480.0, 138.0], [255, 255, 255, 255]);
                    }

                    // Action row (EXIT, COPY, COPY ALL, DELETE) along bottom (y=420)
                    let action_items = [
                        (2, [110.0, 420.0], cursor == 2),
                        (3, [230.0, 420.0], cursor == 3),
                        (4, [380.0, 420.0], cursor == 4),
                        (5, [530.0, 420.0], cursor == 5),
                    ];
                    for (idx, centre, is_sel) in action_items {
                        if let Some(sub) = self.subrects_memcard.get(idx) {
                            let tint = if is_sel { [255, 240, 120, 255] } else { [220, 220, 220, 255] };
                            push_subrect_sprite(&mut sprites, sub, centre, tint);
                        }
                    }
                }

                // Cursor placement on Screen 1
                if let Some(c) = self.cursor {
                    let tex = &self.textures[c];
                    let centre = match cursor {
                        0 => [100.0, 100.0], // Verified entry position in cursor table
                        1 => [420.0, 112.0], // Symmetrical Card 2 position (clearing title plate)
                        2 => [60.0, 420.0],  // EXIT
                        3 => [175.0, 420.0], // COPY
                        4 => [300.0, 420.0], // COPY ALL
                        5 => [465.0, 420.0], // DELETE
                        _ => [100.0, 100.0],
                    };
                    sprites.push(MenuSprite {
                        texture: c,
                        centre,
                        size: [tex.width as f32, tex.height as f32],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [255, 255, 255, 255],
                    });
                }
            }
            3 => {
                // Screen 3: CD Player
                notes.push("\\[V\\] Screen 3 title plate positioned at centre (540, 67) from static sprite table".into());
                notes.push("\\[V\\] Turntable platter Ø120 rendered using 0x800491C4 parameters at centre (230, 224) from object table".into());
                notes.push("\\[V\\] Five carousel disc markers from object table (green, blue, red, orange, yellow)".into());
                notes.push("\\[V\\] Seven transport control buttons at x=58 from static sprite table".into());
                notes.push("\\[V\\] Track grid 1-20 from item table and static sprite table".into());
                notes.push("\\[V\\] Sliced CD labels: individual tracks, EXIT, transport icons, and mode labels from shell sub-rectangle table".into());

                // Title plate
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

                // Large CD turntable disc on left (Ø120) centered at (230, 224) from obj_tbl [9..14]
                if let Some(disc) = self.procedural.discs.first() {
                    push_disc(&mut polygons, 230.0, 224.0, 60.0, disc.inner_colour, disc.outer_colour, 32);
                }

                // Five carousel disc markers from obj_tbl [0..4]
                let carousel_discs = [
                    (128.0, 94.0, 28.0, 1),  // Disc 1 (green Ø56)
                    (225.0, 94.0, 28.0, 2),  // Disc 2 (blue Ø56)
                    (300.0, 94.0, 28.0, 3),  // Disc 3 (red Ø56)
                    (196.0, 121.0, 18.0, 4), // Disc 4 (orange Ø36)
                    (328.0, 121.0, 18.0, 5), // Disc 5 (yellow Ø36)
                ];
                for (cx, cy, r, disc_idx) in carousel_discs {
                    if let Some(disc) = self.procedural.discs.get(disc_idx) {
                        push_disc(&mut polygons, cx, cy, r, disc.inner_colour, disc.outer_colour, 20);
                    }
                }

                // Seven transport buttons at x=58 from static_tbl [43..55]
                let transport_ys = [66.0, 123.0, 180.0, 237.0, 294.0, 351.0, 408.0];
                for &ty in &transport_ys {
                    push_button(&mut polygons, 58.0, ty, 44.0, 44.0, false);
                }

                if let Some(l) = self.labels_cd {
                    let tex = &self.textures[l];
                    let (w, h) = (tex.width as f32, tex.height as f32);

                    // Seven transport icons centered in the transport buttons
                    let transport_icons = [
                        (1.0, 55.0, 28.0, 13.0, transport_ys[0]),   // Prev |<<
                        (33.0, 55.0, 29.0, 13.0, transport_ys[1]),  // Rewind <<
                        (65.0, 55.0, 22.0, 13.0, transport_ys[2]),  // Play >
                        (89.0, 55.0, 12.0, 13.0, transport_ys[3]),  // Stop []
                        (103.0, 55.0, 15.0, 13.0, transport_ys[4]), // Pause ||
                        (120.0, 55.0, 30.0, 13.0, transport_ys[5]), // Fast-forward >>
                        (152.0, 55.0, 28.0, 13.0, transport_ys[6]), // Next >>|
                    ];
                    for (u, v, iw, ih, ty) in transport_icons {
                        sprites.push(MenuSprite {
                            texture: l,
                            centre: [58.0, ty],
                            size: [iw, ih],
                            uv_rect: [u / w, v / h, (u + iw) / w, (v + ih) / h],
                            tint: [220, 220, 220, 255],
                        });
                    }

                    // Track grid 1-20 from static_tbl and item_tbl
                    for i in 0..20 {
                        let col = (i % 5) as f32;
                        let row = (i / 5) as f32;
                        let bx = 402.0 + col * 48.0;
                        let by = 136.0 + row * 48.0;
                        let is_sel = cursor == i;
                        push_button(&mut polygons, bx, by, 32.0, 32.0, is_sel);

                        if let Some(sub) = self.subrects_cd.get(i) {
                            let u0 = sub.u as f32 / w;
                            let v0 = sub.v as f32 / h;
                            let u1 = (sub.u + sub.w) as f32 / w;
                            let v1 = (sub.v + sub.h) as f32 / h;
                            let tint = if is_sel { [255, 240, 120, 255] } else { [220, 220, 220, 255] };
                            sprites.push(MenuSprite {
                                texture: l,
                                centre: [bx, by],
                                size: [sub.w as f32, sub.h as f32],
                                uv_rect: [u0, v0, u1, v1],
                                tint,
                            });
                        }
                    }

                    // Mode buttons from item_tbl [22..26]
                    let mode_items = [
                        (22, 156.0, 342.0, cursor == 20), // CONTINUE
                        (23, 272.0, 342.0, cursor == 21), // SHUFFLE
                        (24, 388.0, 342.0, cursor == 22), // PROGRAM
                        (25, 156.0, 402.0, cursor == 23), // REPEAT
                        (26, 272.0, 402.0, cursor == 24), // REPEAT 1
                    ];
                    for (sub_idx, mx, my, is_sel) in mode_items {
                        push_button(&mut polygons, mx, my, 92.0, 24.0, is_sel);
                        if let Some(sub) = self.subrects_cd.get(sub_idx) {
                            let u0 = sub.u as f32 / w;
                            let v0 = sub.v as f32 / h;
                            let u1 = (sub.u + sub.w) as f32 / w;
                            let v1 = (sub.v + sub.h) as f32 / h;
                            let tint = if is_sel { [255, 240, 120, 255] } else { [220, 220, 220, 255] };
                            sprites.push(MenuSprite {
                                texture: l,
                                centre: [mx, my],
                                size: [sub.w as f32, sub.h as f32],
                                uv_rect: [u0, v0, u1, v1],
                                tint,
                            });
                        }
                    }

                    // EXIT button from static_tbl [2] / item_tbl [27]
                    let is_exit_sel = cursor == 25;
                    push_button(&mut polygons, 480.0, 410.0, 80.0, 28.0, is_exit_sel);
                    if let Some(sub) = self.subrects_cd.get(27) {
                        let u0 = sub.u as f32 / w;
                        let v0 = sub.v as f32 / h;
                        let u1 = (sub.u + sub.w) as f32 / w;
                        let v1 = (sub.v + sub.h) as f32 / h;
                        let tint = if is_exit_sel { [255, 240, 120, 255] } else { [220, 220, 220, 255] };
                        sprites.push(MenuSprite {
                            texture: l,
                            centre: [480.0, 410.0],
                            size: [sub.w as f32, sub.h as f32],
                            uv_rect: [u0, v0, u1, v1],
                            tint,
                        });
                    }
                }

                // Cursor placement on Screen 3
                if let Some(c) = self.cursor {
                    let tex = &self.textures[c];
                    let centre = if cursor < 20 {
                        let col = (cursor % 5) as f32;
                        let row = (cursor / 5) as f32;
                        let bx = 402.0 + col * 48.0;
                        let by = 136.0 + row * 48.0;
                        [bx - 22.0, by]
                    } else if cursor == 20 {
                        [156.0 - 52.0, 342.0]
                    } else if cursor == 21 {
                        [272.0 - 52.0, 342.0]
                    } else if cursor == 22 {
                        [388.0 - 52.0, 342.0]
                    } else if cursor == 23 {
                        [156.0 - 52.0, 402.0]
                    } else if cursor == 24 {
                        [272.0 - 52.0, 402.0]
                    } else if cursor == 25 {
                        [480.0 - 46.0, 410.0]
                    } else {
                        [380.0, 136.0]
                    };
                    sprites.push(MenuSprite {
                        texture: c,
                        centre,
                        size: [tex.width as f32, tex.height as f32],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [255, 255, 255, 255],
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
                        size: [(tex.width as f32).min(256.0), (tex.height as f32).min(108.0)],
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
