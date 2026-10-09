//! The PlayStation 1 licence screen as the PS2's PS1 shell draws it (`notes/ps1_boot.md`):
//! the logo fades in over 31 fields through the GTE depth cue, then the "PlayStation"
//! wordmark ramps up over 30 fields while the licence text, the TM mark and the drive's
//! region letters pop in; then everything holds while the game loads.

use crate::renderer::{Batch, Blend, DepthAction, RenderOptions, Renderer, Vertex, ViewCamera};

/// The GTE's camera-space units of the PS1 logo, scaled to this renderer's world so that the
/// logo (translation z 5888) sits at about z = 184 for a free camera.
pub const PS1_WORLD_SCALE: f32 = 1.0 / 32.0;
use glam::{Vec2, Vec3, Vec4};
use ps2kit::ps1::{LicenceTimeline, Ps1Shell, Tmd, TEXT_ROW_HEIGHT};
use ps2kit::VideoMode;

/// The screen's text and bitmaps, laid out as the shell would for one licence line.
pub struct Ps1Layout {
    pub line1: (f32, f32, usize),  // x, y, width
    pub line2: (f32, f32, usize),
    pub sce: (f32, f32, usize),
    pub show_id: bool,
}

impl Renderer {
    /// Uploads the wordmark, TM mark and the rendered text for a licence line.
    pub fn set_ps1_assets(&mut self, shell: &Ps1Shell, licence: &str) -> Ps1Layout {
        self.upload_texture("ps1wordmark", shell.wordmark.width, shell.wordmark.height, &shell.wordmark.rgba, false);
        self.upload_texture("ps1tm", shell.tm.width, shell.tm.height, &shell.tm.rgba, false);
        let upload_text = |r: &mut Renderer, name: &str, text: &str| -> usize {
            let (w, mask) = shell.font.render(text);
            let rgba: Vec<u8> = mask.iter().flat_map(|&m| [255, 255, 255, m]).collect();
            r.upload_texture(name, w, TEXT_ROW_HEIGHT, &rgba, false);
            w
        };
        // The shell draws the line in 16-character pieces at tabled x positions; rendering the
        // whole line proportionally from the first piece's x gives the same picture.
        let (l1, l2) = licence.split_at(licence.len().min(32));
        let w1 = upload_text(self, "ps1line1", l1);
        let w2 = upload_text(self, "ps1line2", l2);
        let id = shell.sce_id();
        let w3 = upload_text(self, "ps1sce", &id);
        let ((x1, y1), (x2, y2)) = Ps1Shell::text_layout(licence.len());
        Ps1Layout { line1: (x1, y1, w1), line2: (x2, y2, w2), sce: (288.0, 396.0, w3), show_id: true }
    }

    /// One field of the licence screen, `field` counted from the first logo frame.
    pub fn render_ps1_licence(&mut self, enc: &mut wgpu::CommandEncoder, field: f32, shell: &Ps1Shell, logo: &Tmd, layout: &Ps1Layout, video: VideoMode, free: Option<ViewCamera>, _options: &RenderOptions) {
        self.video = video;
        let field = field.max(0.0) as usize;
        let fade = field.min(LicenceTimeline::FADE_FIELDS - 1);
        // The shell draws a 640×480 interlaced frame; one field of it is y/2, and the PAL
        // display window sits 27 lines (13 field lines) lower.
        let yoff = if video == VideoMode::Pal { 13.5 } else { 0.0 };
        let half = video.field_height() / 2.0;
        let ndc = |x: f32, y: f32| Vec4::new(x / 320.0 - 1.0, 1.0 - (y / 2.0 + yoff) / half, 0.0, 1.0);

        let mut verts: Vec<Vertex> = Vec::new();
        let mut batches = Vec::new();
        Renderer::full_quad(&mut verts, Vec4::new(0.0, 0.0, 0.0, 1.0));
        batches.push(Batch::new("white", Blend::Opaque, 0..6));

        // The logo: flat triangles, already sorted far to near — or, under a free camera, the
        // same lit triangles in camera space, scaled into this world and projected by that camera.
        let start = verts.len();
        if let Some(view) = &free {
            let mut tris = shell.lit_triangles(logo, fade);
            let to_world = |p: [f32; 3]| Vec3::new(p[0], p[1], p[2]) * PS1_WORLD_SCALE;
            let depth = |t: &ps2kit::ps1::LitTri| t.xyz.iter().map(|&p| (to_world(p) - view.position).length()).sum::<f32>();
            tris.sort_by(|a, b| depth(b).total_cmp(&depth(a)));
            for t in tris {
                let c = Vec4::new(t.colour[0], t.colour[1], t.colour[2], 1.0);
                let q: Vec<Vertex> = t.xyz.iter().map(|&p| Vertex { pos: view.project(to_world(p)), uv: Vec2::ZERO, pad: Vec2::ZERO, color: c }).collect();
                if q.iter().all(|v| v.pos.w > 1.0) { verts.extend_from_slice(&q) }
            }
        } else {
            for t in shell.project(logo, fade) {
                let c = Vec4::new(t.colour[0], t.colour[1], t.colour[2], 1.0);
                for p in t.xy {
                    verts.push(Vertex { pos: ndc(p[0], p[1]), uv: Vec2::ZERO, pad: Vec2::ZERO, color: c });
                }
            }
        }
        batches.push(Batch::new("white", Blend::Opaque, start..verts.len()));

        if field >= LicenceTimeline::FADE_FIELDS {
            let level = LicenceTimeline::wordmark_level(field - LicenceTimeline::FADE_FIELDS);
            let sprite = |r: &Renderer, verts: &mut Vec<Vertex>, batches: &mut Vec<Batch>, tex: &'static str, x: f32, y: f32, w: f32, h: f32, colour: Vec4| {
                let start = verts.len();
                r.sprite(verts, x, y / 2.0 + yoff, w, h / 2.0, 0.0, 0.0, w, h, (w, h), colour);
                batches.push(Batch::new(tex, Blend::Alpha, start..verts.len()));
            };
            let grey = 0xDAD6 & 31; // 15-bit text colour, r = g = b = 22
            let text = Vec4::new(grey as f32 / 31.0, grey as f32 / 31.0, grey as f32 / 31.0, 1.0);
            let white = Vec4::ONE;
            sprite(self, &mut verts, &mut batches, "ps1wordmark", 228.0, 280.0, shell.wordmark.width as f32, shell.wordmark.height as f32, Vec4::new(level, level, level, 1.0));
            if layout.show_id {
                sprite(self, &mut verts, &mut batches, "ps1tm", 354.0, 401.0, shell.tm.width as f32, shell.tm.height as f32, white);
                sprite(self, &mut verts, &mut batches, "ps1sce", layout.sce.0, layout.sce.1, layout.sce.2 as f32, TEXT_ROW_HEIGHT as f32, text);
            }
            sprite(self, &mut verts, &mut batches, "ps1line1", layout.line1.0, layout.line1.1, layout.line1.2 as f32, TEXT_ROW_HEIGHT as f32, text);
            sprite(self, &mut verts, &mut batches, "ps1line2", layout.line2.0, layout.line2.1, layout.line2.2 as f32, TEXT_ROW_HEIGHT as f32, text);
        }
        let fmt = self.format_of("scene");
        self.pass(enc, "scene", true, DepthAction::None, |r, rp| r.draw(rp, &verts, &batches, fmt, false));
    }
}
