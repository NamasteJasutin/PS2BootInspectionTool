//! The red "Please insert a PlayStation or PlayStation 2 format disc" screen
//! (`notes/opening_scene1.md`).

use super::opening::{euler, GlassParams};
use crate::renderer::*;
use glam::{Vec2, Vec3, Vec4};
use ps2kit::bios::OpeningAssets;
use ps2kit::history::SplitMix;
use ps2kit::sim::{motion, Timeline, VideoMode};

impl Renderer {
    pub fn render_warning(&mut self, enc: &mut wgpu::CommandEncoder, frame: f32, assets: &OpeningAssets, timeline: &Timeline, free: Option<ViewCamera>, options: &RenderOptions, warning_texture: &str) {
        self.video = timeline.video;
        self.logo_cached_field = None;
        let rgba8 = wgpu::TextureFormat::Rgba8Unorm;
        if frame < 0.0 {
            self.pass(enc, "scene", true, DepthAction::None, |_, _| {});
            return;
        }
        let scripted = timeline.camera(frame);
        let camera = free.unwrap_or_else(|| ViewCamera::scripted(scripted, timeline.video));
        let camera_at = |f: f32| free.unwrap_or_else(|| ViewCamera::scripted(timeline.camera(f), timeline.video));

        // 1. The light source (seven spinning discs + flares), fed back at 87.5 % per frame.
        let history = if options.trails { 20 } else { 1 };
        let mut first = true;
        for k in (0..history).rev() {
            let f = frame - k as f32;
            if f < 1.0 && k != 0 { continue }
            let mut verts = Vec::new();
            let mut batches = Vec::new();
            if options.orbs && f >= 1.0 {
                let b = motion::warning_brightness(timeline.camera(f).z);
                let start = verts.len();
                Self::light_discs(&mut verts, f, b, &camera_at(f));
                batches.push(Batch::new("white", Blend::Add, start..verts.len()));
                let start = verts.len();
                self.flares(&mut verts, f, b, &camera_at(f));
                batches.push(Batch::new("TEXOFLAR", Blend::Add, start..verts.len()));
            }
            self.pass(enc, "sub", true, DepthAction::Clear, |r, rp| r.draw(rp, &verts, &batches, rgba8, true));
            let weight = if history == 1 { 1.0 } else if k == history - 1 { 0.875f32.powi(k as i32) } else { 0.125 * 0.875f32.powi(k as i32) };
            let mut quad = Vec::new();
            Self::full_quad(&mut quad, Vec4::splat(weight));
            let fmt = self.format_of("accum");
            self.pass(enc, "accum", first, DepthAction::None, |r, rp| r.draw(rp, &quad, &[Batch::new("@sub", Blend::Add, 0..6)], fmt, false));
            first = false;
        }

        // 2. Composite, then the smoke.
        let mut verts = Vec::new();
        let mut batches = Vec::new();
        Self::full_quad(&mut verts, Vec4::ONE);
        batches.push(Batch::new("@accum", Blend::Opaque, 0..6));
        if options.fog {
            let start = verts.len();
            Self::smoke(&mut verts, frame, scripted.z, &camera);
            if verts.len() > start { batches.push(Batch::new("TEXOFOG0", Blend::Add, start..verts.len())) }
        }
        self.pass(enc, "scene", true, DepthAction::Load, |r, rp| r.draw(rp, &verts, &batches, rgba8, true));

        // 3. Prisms: the glass renderer with red reflections.
        if options.glass {
            self.blit(enc, "scene", "copy");
            for front in [false, true] {
                let mut gv = Vec::new();
                for (i, p) in assets.prism_positions.iter().enumerate() {
                    let params = GlassParams { half: 1.2, tint: Vec3::ONE, reflect_tint: Vec3::new(1.0, 1.0 / 3.0, 1.0 / 3.0), refraction: 0.8, reflection: (0.0, 0.5) };
                    self.glass_cube(&mut gv, Vec3::new(p.x, p.y, (p.z - 2.5) * 128.0 + 788.0), motion::prism_rotation(i, frame), &params, &camera, front);
                }
                if gv.is_empty() { continue }
                let (target, source) = if front { ("scene", "copy") } else { ("copy", "scene") };
                self.pass(enc, target, false, DepthAction::None, |r, rp| r.draw_glass(rp, &gv, source, "TEXOREF", "TEXOBLP", rgba8));
            }
        }

        // 4. Overlays: fade in from black, the text, the exit fade, letterbox.
        let mut verts = Vec::new();
        let mut batches = Vec::new();
        let exit_frames = (frame - timeline.dive_frame as f32).max(0.0);
        if options.fade {
            let mut a = if frame < 1.0 { 1.0 } else { motion::warning_fade_in(scripted.z) };
            if frame >= timeline.dive_frame as f32 { a = a.max((exit_frames / 128.0).min(1.0)) }
            if a > 0.0 {
                let start = verts.len();
                Self::full_quad(&mut verts, Vec4::new(0.0, 0.0, 0.0, a));
                batches.push(Batch::new("white", Blend::Alpha, start..verts.len()));
            }
        }
        if options.lettering {
            let since = frame - timeline.frame_passing(800.0) as f32;
            let mut counter = since.clamp(0.0, 112.0);
            if frame >= timeline.dive_frame as f32 { counter = (counter - exit_frames).max(0.0) }
            let a = counter / 128.0;
            if a > 0.0 && self.textures.contains_key(warning_texture) {
                let k = if self.video == VideoMode::Pal { 0.526271 / 0.457627 } else { 1.0 };
                let start = verts.len();
                self.sprite(&mut verts, 64.0, 88.0 * k, 512.0, 64.0 * k, 0.0, 0.0, 512.0, 128.0, (512.0, 128.0), Vec4::new(a, a, a, 1.0));
                batches.push(Batch::named(warning_texture.to_string(), Blend::Add, start..verts.len()));
            }
        }
        if options.letterbox {
            let start = verts.len();
            self.letterbox(&mut verts);
            batches.push(Batch::new("white", Blend::Opaque, start..verts.len()));
        }
        if !batches.is_empty() {
            self.pass(enc, "scene", false, DepthAction::None, |r, rp| r.draw(rp, &verts, &batches, rgba8, false));
        }
        if options.camera_path {
            let mut lines = Vec::new();
            self.camera_path(&mut lines, timeline, frame, &camera, free.is_some());
            let n = lines.len();
            self.pass(enc, "scene", false, DepthAction::Load, |r, rp| r.draw(rp, &lines, &[Batch::new("white", Blend::Alpha, 0..n).depth(DepthMode::Test)], rgba8, true));
        }
    }

    /// Seven discs in two sizes, each a 16-segment fan with a coloured centre and black rim.
    fn light_discs(out: &mut Vec<Vertex>, frame: f32, b: f32, camera: &ViewCamera) {
        let scale = (b / 64.0).min(1.0);
        let sets: [(Box<dyn Fn(f32) -> f32>, Vec3); 2] = [(Box::new(|_| 8.0), Vec3::new(0x80 as f32, 0x10 as f32, 0x28 as f32)), (Box::new(|i| 28.0 + 8.0 * i), Vec3::new(0x30 as f32, 0x08 as f32, 0x0C as f32))];
        for (radius, colour) in sets.iter() {
            let c = *colour * scale / 255.0;
            for i in 0..7 {
                let (position, r) = motion::warning_disc(i, frame);
                let rot = euler(r);
                let centre = camera.project(position);
                if centre.w <= 1.0 { continue }
                let rim: Vec<Vec4> = (0..=16).map(|k| { let a = (k % 16) as f32 * 2.0 * std::f32::consts::PI / 16.0; camera.project(position + rot * Vec3::new(a.cos() * radius(i as f32), a.sin() * radius(i as f32), 0.0)) }).collect();
                if !rim.iter().all(|p| p.w > 1.0) { continue }
                let cv = Vertex { pos: centre, uv: Vec2::ZERO, pad: Vec2::ZERO, color: c.extend(1.0) };
                let black = Vec4::new(0.0, 0.0, 0.0, 1.0);
                for k in 0..16 {
                    out.push(cv);
                    out.push(Vertex { pos: rim[k], uv: Vec2::ZERO, pad: Vec2::ZERO, color: black });
                    out.push(Vertex { pos: rim[k + 1], uv: Vec2::ZERO, pad: Vec2::ZERO, color: black });
                }
            }
        }
    }

    /// Five additive flare sprites around the light source's centre.
    fn flares(&self, out: &mut Vec<Vertex>, frame: f32, b: f32, camera: &ViewCamera) {
        let n = ((b / 16.0) as i32).min(8) as f32;
        let angle = ((frame as i32 & 31) as f32 + 49.0) * 0.1;
        let centre = Vec3::new(0.196 * angle.cos(), 0.196 * angle.sin(), 1160.0);
        let clip = camera.project(centre);
        if clip.w <= 1.0 { return }
        let half = self.video.field_height() / 2.0;
        let (sx, sy) = (clip.x / clip.w * 320.0, -clip.y / clip.w * half);
        let red = Vec3::new(0x80 as f32, 0x40 as f32, 0x40 as f32);
        let flares = [(1.0f32, 0x70 as f32, n + 2.0, red), (1.0, 0xAA as f32, n + 2.0, red), (1.0, 0x100 as f32, n + 2.0, red), (1.0, 0x1C0 as f32, n + 2.0, red), (0.9, (b / 8.0 + 420.0).floor(), (3.0 * n + b / 2.0).min(255.0), Vec3::new(0x80 as f32, 0x70 as f32, 0x60 as f32))];
        for (scale, hf, fix, colour) in flares {
            let k = colour / 128.0 * (fix / 128.0);
            self.sprite(out, sx * scale + 320.0 - hf, sy * scale + half - hf / 2.0, 2.0 * hf, hf, 0.0, 0.0, 128.0, 128.0, (128.0, 128.0), k.extend(1.0));
        }
    }

    /// 128 drifting puffs: 3x3 quads of ±15 units with a bright centre vertex, recycled in a
    /// band from 32 to 805 units in front of the camera.
    fn smoke(out: &mut Vec<Vertex>, frame: f32, camera_z: f32, camera: &ViewCamera) {
        let mut rng = SplitMix(0x5EED);
        for k in 0..128usize {
            let x = ((rng.next() % 4800) as i32 - 2400) as f32 * 0.01;
            let y = ((rng.next() % 4800) as i32 - 2400) as f32 * 0.01;
            let z0 = 477.0 + (rng.next() % 805) as f32;
            let r = (64 + rng.next() % 64) as f32;
            let speed = ((k + 1) as f32 * 0.02 + 1.2).floor();
            let band = 805.0 - 32.0;
            let rel = (z0 - 672.0 - 32.0 - speed * frame).rem_euclid(band);
            let z = camera_z + 32.0 + rel;
            let fade = ((camera_z + 805.0 - z) / 3.0).min((z - camera_z - 32.0) / 3.0).min(64.0);
            if fade <= 0.0 { continue }
            let fix = fade / 128.0;
            let centre_colour = Vec4::new(r, 0.75 * r, 0.75 * r, 128.0) / 128.0 * fix;
            let v = |ox: f32, oy: f32, u: f32, w: f32, centre: bool| Vertex { pos: camera.project(Vec3::new(x + ox, y + oy, z)), uv: Vec2::new(u, w), pad: Vec2::ZERO, color: if centre { centre_colour } else { Vec4::new(0.0, 0.0, 0.0, 1.0) } };
            let g = [-15.0f32, 0.0, 15.0];
            for qy in 0..2 {
                for qx in 0..2 {
                    let q = [v(g[qx], g[qy], qx as f32 * 0.5, qy as f32 * 0.5, qx == 1 && qy == 1), v(g[qx + 1], g[qy], (qx + 1) as f32 * 0.5, qy as f32 * 0.5, qx == 0 && qy == 1), v(g[qx], g[qy + 1], qx as f32 * 0.5, (qy + 1) as f32 * 0.5, qx == 1 && qy == 0), v(g[qx + 1], g[qy + 1], (qx + 1) as f32 * 0.5, (qy + 1) as f32 * 0.5, qx == 0 && qy == 0)];
                    if !q.iter().all(|p| p.pos.w > 1.0) { continue }
                    out.extend_from_slice(&[q[0], q[1], q[2], q[2], q[1], q[3]]);
                }
            }
        }
    }
}
