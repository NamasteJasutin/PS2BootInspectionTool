//! The "PlayStation 2" logo (`rom0:PS2LOGO`) in its 640x512 buffer coordinates, field by
//! field; fields depend on the previous one, so they are produced in order and cached.

use crate::renderer::*;
use glam::{Vec2, Vec4};
use ps2kit::logo::{LogoAnimation, LogoBitmap};
use ps2kit::sim::VideoMode;

const FRAME: (f32, f32) = (640.0, 512.0);
const REGION: (f32, f32) = (384.0, 96.0);
const REGION_ORIGIN: (f32, f32) = (120.5, 200.5);

impl Renderer {
    pub fn set_logo_bitmap(&mut self, bitmap: Option<&LogoBitmap>) {
        self.logo_cached_field = None;
        let Some(b) = bitmap else { return };
        let mut rgba = vec![0u8; 512 * 128 * 4];
        for y in 0..b.height { for x in 0..b.width {
            let v = b.grey[y * b.width + x];
            let o = (y * 512 + x) * 4;
            rgba[o..o + 4].copy_from_slice(&[v, v, v, v]);
        }}
        self.upload_texture("logo", 512, 128, &rgba, false);
    }

    /// Field `f` (0 = first drawn field). Returns without work when already rendered.
    pub fn render_logo(&mut self, enc: &mut wgpu::CommandEncoder, frame: f32, anim: &LogoAnimation, options: &RenderOptions) {
        self.video = anim.video;
        let animated = (anim.last_field() - anim.first_field()) as i32;
        let f = (frame as i32).clamp(0, animated);
        if self.logo_cached_field == Some(f) { return }
        let start = match self.logo_cached_field {
            Some(c) if c < f => c + 1,
            _ => { self.pass(enc, "logoPrev", true, DepthAction::None, |_, _| {}); 0 }
        };
        for field in start..=f {
            self.logo_field(enc, anim.first_field() + field, anim, options);
            self.blit(enc, "scene", "logoPrev");
        }
        self.logo_cached_field = Some(f);
    }

    /// A textured quad between two targets, each addressed in its own logical pixel space.
    fn resample(&mut self, enc: &mut wgpu::CommandEncoder, from: &'static str, from_size: (f32, f32), to: &str, to_size: (f32, f32), src: (f32, f32, f32, f32), dst: (f32, f32, f32, f32), clear: bool, color: Vec4, blend: Blend) {
        let mut v = Vec::new();
        Self::quad(&mut v, dst.0 / to_size.0 * 2.0 - 1.0, 1.0 - dst.3 / to_size.1 * 2.0, dst.2 / to_size.0 * 2.0 - 1.0, 1.0 - dst.1 / to_size.1 * 2.0, src.0 / from_size.0, src.1 / from_size.1, src.2 / from_size.0, src.3 / from_size.1, color);
        let fmt = self.format_of(to);
        let name = format!("@{from}");
        self.pass(enc, to, clear, DepthAction::None, |r, rp| r.draw(rp, &v, &[Batch::named(name, blend, 0..6)], fmt, false));
    }

    fn logo_field(&mut self, enc: &mut wgpu::CommandEncoder, t: i32, anim: &LogoAnimation, options: &RenderOptions) {
        let pal = anim.video == VideoMode::Pal;
        let rgba8 = wgpu::TextureFormat::Rgba8Unorm;
        // 1. The logo bitmap, opaque.
        let mut v = Vec::new();
        let (rx, ry, rw, rh) = anim.logo_rect();
        if options.towers {
            Self::quad(&mut v, rx / 320.0 - 1.0, 1.0 - (ry + rh) / 256.0, (rx + rw) / 320.0 - 1.0, 1.0 - ry / 256.0, 0.0, 0.0, rw / 512.0, rh / 128.0, Vec4::ONE);
        }
        let n = v.len();
        self.pass(enc, "scene", true, DepthAction::None, |r, rp| r.draw(rp, &v, &[Batch::new("logo", Blend::Opaque, 0..n)], rgba8, false));
        // 2. Progressive blur of the logo neighbourhood on two region-sized textures.
        if options.defocus {
            let n = anim.logo_blur_iterations(t);
            if n > 0 {
                let aux_h = if pal { 84.25 } else { 71.25 };
                let region = (REGION_ORIGIN.0, REGION_ORIGIN.1, REGION_ORIGIN.0 + REGION.0, REGION_ORIGIN.1 + REGION.1);
                let whole = (0.0, 0.0, REGION.0, REGION.1);
                self.resample(enc, "scene", FRAME, "blurA", REGION, region, whole, true, Vec4::ONE, Blend::Opaque);
                for i in 0..n {
                    let s = i as f32 * 0.5;
                    // Symmetric down/up pair: the console's half-pixel offsets would drift here.
                    self.resample(enc, "blurA", REGION, "blurB", REGION, whole, (0.0, 0.0, 239.25 - s, aux_h - s), true, Vec4::ONE, Blend::Opaque);
                    self.resample(enc, "blurB", REGION, "blurA", REGION, (0.0, 0.0, 239.25 - s, aux_h - s), whole, false, Vec4::ONE, Blend::Opaque);
                }
                self.resample(enc, "blurA", REGION, "scene", FRAME, whole, region, false, Vec4::ONE, Blend::Opaque);
            }
        }
        // 3. Layer A, soft blur, layer B, feedback, soft blur.
        if options.orbs { self.logo_layers(enc, anim, t, true) }
        if options.fog && anim.screen_blur_active(t) { self.screen_blur(enc) }
        if options.orbs { self.logo_layers(enc, anim, t, false) }
        if options.trails {
            let a = anim.feedback_alpha(t) as f32 / 128.0;
            if a > 0.0 {
                let k = 0x84 as f32 / 128.0;
                self.resample(enc, "logoPrev", FRAME, "scene", FRAME, (0.5, 0.5, 640.5, 512.5), (2.0, 2.0, 638.0, 510.0), false, Vec4::new(k, k, k, a), Blend::Alpha);
            }
        }
        if options.fog && anim.screen_blur_active(t) { self.screen_blur(enc) }
    }

    fn screen_blur(&mut self, enc: &mut wgpu::CommandEncoder) {
        for i in 0..2 {
            let s = i as f32 * 0.5;
            self.resample(enc, "scene", FRAME, "temp", FRAME, (0.0, 0.0, 640.0, 512.0), (0.0, 0.0, 479.25 - s, 385.25 - s), true, Vec4::ONE, Blend::Opaque);
            self.resample(enc, "temp", FRAME, "scene", FRAME, (0.0, 0.0, 479.25 - s, 385.25 - s), (0.0, 0.0, 640.0, 512.0), false, Vec4::ONE, Blend::Opaque);
        }
    }

    /// Additive vector layers: anti-aliased line strips (type 0) and five-copy ribbons (type 1).
    fn logo_layers(&mut self, enc: &mut wgpu::CommandEncoder, anim: &LogoAnimation, t: i32, layer_a: bool) {
        let mut v: Vec<Vertex> = Vec::new();
        let to_ndc = |p: Vec2| Vec4::new(p.x / 320.0 - 1.0, 1.0 - p.y / 256.0, 0.0, 1.0);
        for index in (0..4).rev() {
            let obj = &anim.objects()[index];
            if !(if layer_a { obj.layer_a } else { obj.layer_b }) { continue }
            let c = anim.colour(obj, t);
            let colour = Vec4::new(c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0);
            if obj.kind == 0 {
                for strip in anim.flatten(&anim.shape(obj, t), None) {
                    let pts: Vec<Vec2> = strip.iter().map(|&p| anim.to_screen(p)).collect();
                    for w in pts.windows(2) {
                        let d = w[1] - w[0];
                        if d.length_squared() < 1e-6 { continue }
                        let n = Vec2::new(-d.y, d.x).normalize() * 0.5;
                        let vert = |p: Vec2| Vertex { pos: to_ndc(p), uv: Vec2::ZERO, pad: Vec2::ZERO, color: colour };
                        v.extend_from_slice(&[vert(w[0] - n), vert(w[0] + n), vert(w[1] - n), vert(w[1] - n), vert(w[0] + n), vert(w[1] + n)]);
                    }
                }
            } else {
                let counts = anim.segment_counts(obj, t);
                let offsets = anim.ribbon_offsets(index);
                let curves: Vec<Vec<Vec<Vec2>>> = offsets.iter().map(|&d| anim.flatten(&anim.shape(obj, t - d), Some(&counts)).iter().map(|pl| pl.iter().map(|&p| anim.to_screen(p)).collect()).collect()).collect();
                for k in 1..5 {
                    let ca = colour * anim.assets.ribbon_multipliers[k - 1];
                    let cb = colour * anim.assets.ribbon_multipliers[k];
                    for (pa, pb) in curves[k - 1].iter().zip(&curves[k]) {
                        let n = pa.len().min(pb.len());
                        if n < 2 { continue }
                        let vert = |p: Vec2, c: Vec4| Vertex { pos: to_ndc(p), uv: Vec2::ZERO, pad: Vec2::ZERO, color: Vec4::new(c.x, c.y, c.z, 1.0) };
                        for i in 0..n - 1 {
                            v.extend_from_slice(&[vert(pa[i], ca), vert(pb[i], cb), vert(pa[i + 1], ca), vert(pa[i + 1], ca), vert(pb[i], cb), vert(pb[i + 1], cb)]);
                        }
                    }
                }
            }
        }
        if v.is_empty() { return }
        let n = v.len();
        self.pass(enc, "scene", false, DepthAction::None, |r, rp| r.draw(rp, &v, &[Batch::new("white", Blend::Add, 0..n)], wgpu::TextureFormat::Rgba8Unorm, false));
    }
}
