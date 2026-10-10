//! Standalone PS1 SCE intro. Geometry, ordering and CLUT steps come from ps2kit.

use crate::renderer::{Batch, Blend, DepthAction, Renderer, Vertex};
use glam::{Vec2, Vec4};
use ps2kit::ps1intro::{SceFrame, SceIntro, ScePolygon, SceVertex};
use ps2kit::VideoMode;

/// Texture names for every uploaded logotype and integer CLUT interpolation step.
pub struct SceTextures {
    names: Vec<Vec<String>>,
}

impl Renderer {
    /// Uploads the dump's logotypes at every CLUT step once; retain the returned handle.
    pub fn set_sce_assets(&mut self, intro: &SceIntro) -> SceTextures {
        let steps = intro.timeline(VideoMode::Ntsc).text_fade;
        let mut names = Vec::new();
        for texture in 0..intro.logotypes.len() {
            let mut levels = Vec::new();
            for level in 0..=steps {
                let Some(tim) = intro.faded_logotype(texture, level) else { continue };
                let name = format!("sce{texture}clut{level}");
                self.upload_texture(&name, tim.width, tim.height, &tim.rgba, false);
                levels.push(name);
            }
            names.push(levels);
        }
        SceTextures { names }
    }

    /// Draws the supplied frame into `scene`; `field` is the sequence's intro field.
    /// PS1 drawing coordinates are interlaced 640×480, unlike the PS2 opening's fields.
    /// PAL displays that drawing with x=3/y=27 in 512 lines (scenes §1).
    pub fn render_sce_intro(&mut self, enc: &mut wgpu::CommandEncoder, _field: f32, frame: &SceFrame, tex: &SceTextures, video: VideoMode) {
        self.video = video;
        let (xoff, yoff, height) = if video == VideoMode::Pal { (3.0, 27.0, 512.0) } else { (0.0, 0.0, 480.0) };
        let ndc = |xy: [f32; 2]| Vec4::new((xy[0] + xoff) / 320.0 - 1.0, 1.0 - (xy[1] + yoff) * 2.0 / height, 0.0, 1.0);
        let vertex = |v: SceVertex| Vertex { pos: ndc(v.xy), uv: Vec2::ZERO, pad: Vec2::ZERO,
            color: Vec4::new(v.colour[0] as f32 / 255.0, v.colour[1] as f32 / 255.0, v.colour[2] as f32 / 255.0, 1.0) };
        let mut verts = Vec::new();
        let mut batches = Vec::new();
        let grey = frame.grey as f32 / 255.0;
        Self::full_quad(&mut verts, Vec4::new(grey, grey, grey, 1.0));
        for polygon in &frame.polygons {
            match polygon {
                ScePolygon::Quad(v) => verts.extend([vertex(v[0]), vertex(v[1]), vertex(v[2]), vertex(v[1]), vertex(v[2]), vertex(v[3])]),
                ScePolygon::Triangle(v) => verts.extend(v.iter().copied().map(vertex)),
                _ => {}
            }
        }
        // v_main/f_tex already interpolate per-vertex colour; white supplies no texture.
        batches.push(Batch::new("white", Blend::Opaque, 0..verts.len()));
        for sprite in &frame.sprites {
            let Some(name) = tex.names.get(sprite.texture).and_then(|levels| levels.get(sprite.clut_level)) else { continue };
            let start = verts.len();
            let [w, h] = sprite.size;
            let [cx, cy] = sprite.centre;
            let lo = ndc([cx - w / 2.0, cy + h / 2.0]);
            let hi = ndc([cx + w / 2.0, cy - h / 2.0]);
            Self::quad(&mut verts, lo.x, lo.y, hi.x, hi.y, 0.0, 0.0, 1.0, 1.0, Vec4::ONE);
            batches.push(Batch::named(name.clone(), Blend::Alpha, start..verts.len()));
        }
        let fmt = self.format_of("scene");
        self.pass(enc, "scene", true, DepthAction::None, |r, rp| r.draw(rp, &verts, &batches, fmt, false));
    }
}
