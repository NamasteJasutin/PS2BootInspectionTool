//! The tower scene, stage by stage as the console draws it (`notes/opening.md` §5).

use crate::renderer::*;
use glam::{Mat3, Quat, Vec2, Vec3, Vec4};
use ps2kit::bios::OpeningAssets;
use ps2kit::sim::{motion, OpeningScene, Timeline};

const TOWER_FACES: [([(f32, f32, f32); 4], Vec3); 5] = [
    ([(2.0, -2.0, -1.0), (-2.0, -2.0, -1.0), (2.0, 2.0, -1.0), (-2.0, 2.0, -1.0)], Vec3::new(0.0, 0.0, 1.0)), // near cap
    ([(2.0, -2.0, -1.0), (2.0, -2.0, 1.0), (-2.0, -2.0, -1.0), (-2.0, -2.0, 1.0)], Vec3::new(0.0, 1.0, 0.0)),
    ([(2.0, 2.0, 1.0), (2.0, 2.0, -1.0), (-2.0, 2.0, 1.0), (-2.0, 2.0, -1.0)], Vec3::new(0.0, -1.0, 0.0)),
    ([(2.0, -2.0, 1.0), (2.0, 2.0, 1.0), (2.0, -2.0, -1.0), (2.0, 2.0, -1.0)], Vec3::new(-1.0, 0.0, 0.0)),
    ([(-2.0, -2.0, -1.0), (-2.0, 2.0, -1.0), (-2.0, -2.0, 1.0), (-2.0, 2.0, 1.0)], Vec3::new(1.0, 0.0, 0.0)),
];
const TOWER_UV: [Vec2; 4] = [Vec2::new(0.01, 0.01), Vec2::new(0.24, 0.01), Vec2::new(0.01, 0.24), Vec2::new(0.24, 0.24)];

pub const CUBE_CORNERS: [Vec3; 8] = [
    Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, -1.0, -1.0), Vec3::new(-1.0, 1.0, -1.0), Vec3::new(1.0, 1.0, -1.0),
    Vec3::new(-1.0, -1.0, 1.0), Vec3::new(1.0, -1.0, 1.0), Vec3::new(-1.0, 1.0, 1.0), Vec3::new(1.0, 1.0, 1.0),
];
pub const CUBE_FACES: [([usize; 4], Vec3); 6] = [
    ([0, 1, 2, 3], Vec3::new(0.0, 0.0, -1.0)), ([5, 4, 7, 6], Vec3::new(0.0, 0.0, 1.0)), ([4, 0, 6, 2], Vec3::new(-1.0, 0.0, 0.0)),
    ([2, 3, 6, 7], Vec3::new(0.0, 1.0, 0.0)), ([1, 5, 3, 7], Vec3::new(1.0, 0.0, 0.0)), ([4, 5, 0, 1], Vec3::new(0.0, -1.0, 0.0)),
];

pub fn euler(r: Vec3) -> Mat3 {
    Mat3::from_quat(Quat::from_axis_angle(Vec3::X, r.x)) * Mat3::from_quat(Quat::from_axis_angle(Vec3::Y, r.y)) * Mat3::from_quat(Quat::from_axis_angle(Vec3::Z, r.z))
}

pub struct GlassParams {
    pub half: f32,
    pub tint: Vec3,
    pub reflect_tint: Vec3,
    pub refraction: f32,
    pub reflection: (f32, f32),
}

/// Where tower `i` of `n` stands at `frame`: on the console's grid, or (beyond the PS2) on a
/// ring or spiral around the camera's axis, revolving at `options.revolve` turns per second.
pub fn tower_place(options: &RenderOptions, i: usize, n: usize, t: &ps2kit::sim::Tower, frame: f32, fps: f32) -> Vec3 {
    use std::f32::consts::TAU;
    let n = n.max(1) as f32;
    let turn = options.revolve * frame / fps * TAU;
    let (angle, radius) = match options.layout {
        TowerLayout::Console => return t.centre,
        TowerLayout::Ring => (TAU * i as f32 / n + turn, options.ring_radius),
        TowerLayout::Spiral => (TAU * 2.5 * i as f32 / n + turn, options.ring_radius * (0.35 + 0.65 * i as f32 / n)),
    };
    Vec3::new(radius * angle.cos(), radius * angle.sin(), t.centre.z)
}

impl Renderer {
    /// Renders one frame of the opening into the "scene" target.
    pub fn render_opening(&mut self, enc: &mut wgpu::CommandEncoder, frame: f32, scene: &OpeningScene, assets: &OpeningAssets, timeline: &Timeline, free: Option<ViewCamera>, options: &RenderOptions) {
        self.video = timeline.video;
        self.logo_cached_field = None;
        if frame < 0.0 {
            self.pass(enc, "scene", true, DepthAction::None, |_, _| {});
            return;
        }
        let scripted = timeline.camera(frame);
        let camera = free.unwrap_or_else(|| ViewCamera::scripted(scripted, timeline.video));
        let camera_at = |f: f32| free.unwrap_or_else(|| ViewCamera::scripted(timeline.camera(f), timeline.video));
        let rgba8 = wgpu::TextureFormat::Rgba8Unorm;

        // 1. Towers, blended over the previous frames: frame = 0.375 * current + 0.625 * previous.
        let history = if options.trails { 12 } else { 1 };
        let mut first = true;
        for k in (0..history).rev() {
            let f = frame - k as f32;
            if f < 0.0 && k != 0 { continue }
            let mut verts = Vec::new();
            if options.towers && f >= 0.0 { self.towers(&mut verts, scene, f, &camera_at(f), options) }
            let n = verts.len();
            self.pass(enc, "sub", true, DepthAction::Clear, |r, rp| r.draw(rp, &verts, &[Batch::new("TEXOWAL0", Blend::Opaque, 0..n).depth(DepthMode::Write).repeat()], rgba8, true));
            let weight = if history == 1 { 1.0 } else if k == history - 1 { 0.625f32.powi(k as i32) } else { 0.375 * 0.625f32.powi(k as i32) };
            let mut quad = Vec::new();
            Self::full_quad(&mut quad, Vec4::splat(weight));
            let fmt = self.format_of("accum");
            self.pass(enc, "accum", first, DepthAction::None, |r, rp| r.draw(rp, &quad, &[Batch::new("@sub", Blend::Add, 0..6)], fmt, false));
            first = false;
        }

        // 2. Composite, fog, orbs. Depth comes from the current frame's towers.
        let mut verts = Vec::new();
        let mut batches = Vec::new();
        {
            let start = verts.len();
            Self::full_quad(&mut verts, Vec4::ONE);
            batches.push(Batch::new("@accum", Blend::Opaque, start..verts.len()));
        }
        if options.fog {
            for (i, tex) in ["TEXOFOG4", "TEXOFOG2", "TEXOFOG1", "TEXOFOG4", "TEXOFOG2", "TEXOFOG1"].iter().enumerate() {
                let start = verts.len();
                self.fog_layer(&mut verts, i, frame, &camera);
                if verts.len() > start { batches.push(Batch::new(tex, Blend::Add, start..verts.len()).depth(DepthMode::Test).repeat()) }
            }
        }
        let near_objects = scripted.z < 73.0;
        if options.orbs && near_objects {
            let start = verts.len();
            self.orbs(&mut verts, frame, options.orb_seed, assets.orb_colours(), &camera);
            if verts.len() > start { batches.push(Batch::new("TEXOCRBL", Blend::AddSrcAlpha, start..verts.len())) }
            let start = verts.len();
            self.orb_trails(&mut verts, frame, options.orb_seed, assets.orb_colours(), &camera_at);
            if verts.len() > start { batches.push(Batch::new("white", Blend::AddSrcAlpha, start..verts.len())) }
        }
        self.pass(enc, "scene", true, DepthAction::Load, |r, rp| r.draw(rp, &verts, &batches, rgba8, true));

        // 3. Glass cubes: back faces into a copy of the frame, front faces refract that copy.
        if options.glass && near_objects {
            self.blit(enc, "scene", "copy");
            for front in [false, true] {
                let mut gv = Vec::new();
                for (i, p) in assets.cube_positions().iter().enumerate() {
                    let params = GlassParams { half: 1.8, tint: Vec3::new(112.0, 112.0, 152.0) / 128.0, reflect_tint: Vec3::ONE, refraction: 1.0, reflection: (0.25, 0.5) };
                    self.glass_cube(&mut gv, Vec3::new(p.x * 3.5, p.y * 3.5, p.z * -15.0 + 150.0), motion::cube_rotation(i, frame), &params, &camera, front);
                }
                if gv.is_empty() { continue }
                let (target, source) = if front { ("scene", "copy") } else { ("copy", "scene") };
                self.pass(enc, target, false, DepthAction::None, |r, rp| r.draw_glass(rp, &gv, source, "TEXOREF", "TEXOBLP", rgba8));
            }
        }

        // 4. Defocus: squeeze the picture into its top-left corner and stretch it back.
        if options.defocus {
            let n = motion::defocus_passes(scripted.z);
            let fh = self.video.field_height();
            for i in 0..n {
                let shrink = (i * (n - 1)) as f32;
                let fx = (640.0 * 7.0 / 8.0 - 1.0 - shrink) / 640.0;
                let fy = (fh * 7.0 / 8.0 - 1.0 - shrink) / fh;
                let (mut down, mut up) = (Vec::new(), Vec::new());
                Self::quad(&mut down, -1.0, 1.0 - 2.0 * fy, -1.0 + 2.0 * fx, 1.0, 0.0, 0.0, 1.0, 1.0, Vec4::ONE);
                Self::quad(&mut up, -1.0, -1.0, 1.0, 1.0, 0.0, 0.0, fx, fy, Vec4::ONE);
                self.pass(enc, "temp", true, DepthAction::None, |r, rp| r.draw(rp, &down, &[Batch::new("@scene", Blend::Opaque, 0..6)], rgba8, false));
                self.pass(enc, "scene", false, DepthAction::None, |r, rp| r.draw(rp, &up, &[Batch::new("@temp", Blend::Opaque, 0..6)], rgba8, false));
            }
        }

        // 5. Overlays: fade, lettering, letterbox.
        let mut verts = Vec::new();
        let mut batches = Vec::new();
        if options.fade {
            let a = if frame < 2.0 { 1.0 } else { motion::fade_alpha(scripted.z) };
            if a > 0.0 {
                let start = verts.len();
                Self::full_quad(&mut verts, Vec4::new(0.0, 0.0, 0.0, a));
                batches.push(Batch::new("white", Blend::Alpha, start..verts.len()));
            }
        }
        if options.lettering {
            let a = motion::lettering_alpha(frame - timeline.frame_passing(18.0) as f32);
            if a > 0.0 {
                let pal = self.video == ps2kit::VideoMode::Pal;
                let (y, h) = if pal { (120.0, 18.0) } else { (105.0, 16.0) };
                let start = verts.len();
                let c = Vec4::new(1.0, 1.0, 1.0, a);
                self.sprite(&mut verts, 120.0, y, 256.0, h, 0.0, 1.0, 256.0, 30.0, (256.0, 64.0), c);
                self.sprite(&mut verts, 326.0, y, 256.0, h, 0.0, 33.0, 256.0, 30.0, (256.0, 64.0), c);
                batches.push(Batch::new("TEXOSCE", Blend::Alpha, start..verts.len()));
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

        // 6. Study aid: the scripted camera's route, depth-tested against the towers.
        if options.camera_path {
            let mut lines = Vec::new();
            self.camera_path(&mut lines, timeline, frame, &camera, free.is_some());
            let n = lines.len();
            self.pass(enc, "scene", false, DepthAction::Load, |r, rp| r.draw(rp, &lines, &[Batch::new("white", Blend::Alpha, 0..n).depth(DepthMode::Test)], rgba8, true));
        }
    }

    fn towers(&self, out: &mut Vec<Vertex>, scene: &OpeningScene, frame: f32, camera: &ViewCamera, options: &RenderOptions) {
        let lights: [(Vec3, f32); 3] = [(Vec3::new(0.0, 0.0, 1.0), 1.0), (Vec3::new(-0.5, -0.5, 0.0).normalize(), 0.8), (Vec3::new(0.5, 0.5, 0.0).normalize(), 0.8)];
        let sway = motion::sway(frame);
        let wrap = options.colour_wrap;
        let n = scene.towers.len();
        for (i, t) in scene.towers.iter().enumerate() {
            let centre = tower_place(options, i, n, t, frame, camera.video.fps());
            let mut tint = if options.tint == TowerTint::Console { Vec3::ONE } else { self.tower_tints.get(i).copied().unwrap_or(Vec3::ONE) };
            if self.highlight == Some(t.record) { tint = Vec3::new(1.6, 1.4, 0.6) }
            let angle = t.quarter_turns as f32 * std::f32::consts::FRAC_PI_2 + if t.growing { sway } else { 0.0 };
            let (c, s) = (angle.cos(), angle.sin());
            let rotate = |v: Vec3| Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z);
            for (fi, (corners, normal)) in TOWER_FACES.iter().enumerate() {
                let n = rotate(*normal);
                let lit = 0.4 + lights.iter().map(|(d, k)| n.dot(*d).max(0.0) * k).sum::<f32>();
                let mut quad = [Vertex { pos: Vec4::ZERO, uv: Vec2::ZERO, pad: Vec2::ZERO, color: Vec4::ONE }; 4];
                for (vi, corner) in corners.iter().enumerate() {
                    let local = Vec3::new(corner.0, corner.1, corner.2 * t.half_length);
                    let far = if options.solid_towers { 0.55 } else { 0.0 };
                    let base = if corner.2 > 0.0 { t.brightness * far } else if fi == 0 { t.brightness } else { t.brightness * 0.8 };
                    let value = (base * lit) as i32;
                    let value = if wrap { value & 255 } else { value.min(255) };
                    let shade = value as f32 / 128.0;
                    let colour = tint * shade;
                    quad[vi] = Vertex { pos: camera.project(centre + rotate(local)), uv: TOWER_UV[vi] + Vec2::splat(t.uv_offset), pad: Vec2::ZERO, color: Vec4::new(colour.x, colour.y, colour.z, 1.0) };
                }
                out.extend_from_slice(&[quad[0], quad[1], quad[2], quad[2], quad[1], quad[3]]);
            }
        }
    }

    fn fog_layer(&self, out: &mut Vec<Vertex>, layer: usize, frame: f32, camera: &ViewCamera) {
        let radius = 5202f32.sqrt();
        let z = 134.0 - 5.0 * layer as f32;
        let scroll = motion::fog_scroll(layer, frame);
        let vertex = |a: i32, b: i32| {
            let d = (-5.1 - ((a - 8) as f32 * 6.0 + 3.0)).hypot(-((b - 8) as f32 * 6.0 + 3.0));
            let c = ((radius - 4.0 * d) * 96.0 / radius).clamp(0.0, 127.0) as i32;
            let k = 0x14 as f32 / 128.0 / 128.0;
            Vertex { pos: camera.project(Vec3::new(a as f32 * 6.0 - 46.0, b as f32 * 6.0 - 48.0, z)), uv: Vec2::new(a as f32 * 0.5 - scroll, b as f32 * 0.5), pad: Vec2::ZERO, color: Vec4::new((c / 4) as f32 * k, (c * 2 / 5) as f32 * k, c as f32 * k, 1.0) }
        };
        for b in 0..16 {
            for a in 0..16 {
                let q = [vertex(a, b), vertex(a + 1, b), vertex(a, b + 1), vertex(a + 1, b + 1)];
                if q.iter().all(|v| v.color.z == 0.0) { continue }
                out.extend_from_slice(&[q[0], q[1], q[2], q[2], q[1], q[3]]);
            }
        }
    }

    fn orbs(&self, out: &mut Vec<Vertex>, frame: f32, seed: f32, colours: &[Vec3], camera: &ViewCamera) {
        for i in 0..4 {
            for ghost in 1..=4 {
                let f = frame - (4 - ghost) as f32;
                if f < 0.0 { continue }
                let p = motion::orb_position(i, f, seed);
                let rgb = colours[i] * 0.5 / 128.0;
                Self::billboard(out, p, 0.8, Vec4::new(rgb.x, rgb.y, rgb.z, (24 * ghost / 5) as f32 / 128.0), camera);
                Self::billboard(out, p, 0.25, Vec4::new(1.0, 1.0, 1.0, (12 * ghost / 5) as f32 / 128.0), camera);
            }
        }
    }

    /// The console keeps the last 128 screen positions of each orb and joins every eighth
    /// one with a line that fades towards the tail.
    fn orb_trails(&self, out: &mut Vec<Vertex>, frame: f32, seed: f32, colours: &[Vec3], camera_at: &dyn Fn(f32) -> ViewCamera) {
        let length = 127.0f32;
        let half = Vec2::new(1.0 / 640.0, 1.0 / 448.0);
        for i in 0..4 {
            let mut points: Vec<(Vec2, Vec4)> = Vec::new();
            for k in (0..128).step_by(8) {
                let f = frame - k as f32;
                if f < 0.0 { break }
                let clip = camera_at(f).project(motion::orb_position(i, f, seed));
                if clip.w <= 1.0 { break }
                let fade = ((length - k as f32) * 64.0 / length).floor();
                let rgb = colours[i] * fade / 128.0 / 255.0;
                points.push((Vec2::new(clip.x, clip.y) / clip.w, Vec4::new(rgb.x, rgb.y, rgb.z, (fade * 2.0 / 128.0).min(1.0))));
            }
            for w in points.windows(2) {
                let ((p0, c0), (p1, c1)) = (w[0], w[1]);
                let dir = p1 - p0;
                if dir.length_squared() <= 0.0 { continue }
                let n = Vec2::new(-dir.y * 448.0 / 640.0, dir.x * 640.0 / 448.0).normalize() * half * 2.0;
                let v = |p: Vec2, c: Vec4| Vertex { pos: Vec4::new(p.x, p.y, 0.0, 1.0), uv: Vec2::ZERO, pad: Vec2::ZERO, color: c };
                out.extend_from_slice(&[v(p0 - n, c0), v(p0 + n, c0), v(p1 - n, c1), v(p1 - n, c1), v(p0 + n, c0), v(p1 + n, c1)]);
            }
        }
    }

    /// Approximation of the console's ten-pass glass: per face, a refracted copy of the frame
    /// multiplied by `tint`, plus the reflection map (times `reflect_tint`) masked by noise.
    pub fn glass_cube(&self, out: &mut Vec<GlassVertex>, centre: Vec3, r: Vec3, p: &GlassParams, camera: &ViewCamera, front: bool) {
        let rot = euler(r);
        let world: Vec<Vec3> = CUBE_CORNERS.iter().map(|c| centre + rot * (*c * p.half)).collect();
        let clip: Vec<Vec4> = world.iter().map(|w| camera.project(*w)).collect();
        if !clip.iter().all(|c| c.w > 1.0) { return }
        let screen_uv = |c: Vec4| Vec2::new(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
        let centre_uv = screen_uv(camera.project(centre));
        let (bx, by, _) = camera.basis();
        let corners = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 1.0)];
        for (indices, normal) in CUBE_FACES.iter() {
            let n = rot * *normal;
            let face_centre = indices.iter().map(|&i| world[i]).sum::<Vec3>() / 4.0;
            if (n.dot(face_centre - camera.position) < 0.0) != front { continue }
            let n_view = Vec2::new(n.dot(bx), n.dot(by));
            let mut quad = [GlassVertex { pos: Vec4::ZERO, a: Vec4::ZERO, b: Vec4::ZERO, c: Vec4::ZERO, tint: Vec4::ZERO, refl_tint: Vec4::ZERO }; 4];
            for (vi, &ci) in indices.iter().enumerate() {
                let to_eye = (camera.position - world[ci]).normalize();
                let rim = (1.0 - n.dot(to_eye).abs()).powi(2) * 0.5 * 32.0 / 128.0;
                let refract = n_view * 2.0 * p.refraction / clip[ci].w;
                let reflect_scale = if front { 0.5 } else { -0.25 };
                let refl_uv = corners[vi] * 0.5 + Vec2::splat(0.25) + n_view * reflect_scale;
                let noise_shift = if front { 0.0075 } else { 0.00375 };
                quad[vi] = GlassVertex {
                    pos: clip[ci],
                    a: Vec4::new(refract.x, refract.y, refl_uv.x, refl_uv.y),
                    b: Vec4::new(corners[vi].x + noise_shift, corners[vi].y - noise_shift, if front { -0.084 } else { 0.0 }, rim),
                    c: Vec4::new(centre_uv.x, centre_uv.y, if front { p.reflection.1 } else { p.reflection.0 }, 0.0),
                    tint: p.tint.extend(1.0),
                    refl_tint: p.reflect_tint.extend(1.0),
                };
            }
            out.extend_from_slice(&[quad[0], quad[1], quad[2], quad[2], quad[1], quad[3]]);
        }
    }

    /// The scripted camera's route: spine, rungs every 10 frames pointing screen-up (the roll
    /// shows as a twist), gates where events fire, and the frustum at the current frame.
    pub fn camera_path(&self, out: &mut Vec<Vertex>, timeline: &Timeline, frame: f32, camera: &ViewCamera, show_frustum: bool) {
        let video = timeline.video;
        let travelled = Vec4::new(1.0, 0.82, 0.25, 0.95);
        let ahead = Vec4::new(1.0, 0.82, 0.25, 0.35);
        let now = frame as usize;
        let states = timeline.states();
        for f in 0..timeline.end_frame() {
            self.line(out, states[f].position(), states[f + 1].position(), if f < now { travelled } else { ahead }, 3.0, camera);
        }
        for f in (0..=timeline.end_frame()).step_by(10) {
            let cam = ViewCamera::scripted(states[f], video);
            let (bx, by, _) = cam.basis();
            let second = f % 60 == 0;
            let colour = Vec4::new(1.0, 1.0, 1.0, if f <= now { 0.9 } else { 0.35 });
            self.line(out, cam.position, cam.position - by * if second { 2.4 } else { 1.2 }, colour, if second { 3.0 } else { 2.0 }, camera);
            self.line(out, cam.position - bx * 0.6, cam.position + bx * 0.6, colour, 2.0, camera);
        }
        let gates = [
            (timeline.frame_passing(18.0), Vec3::new(0.3, 0.9, 1.0)), (timeline.dive_frame, Vec3::new(0.3, 1.0, 0.4)),
            (timeline.frame_passing(56.0), Vec3::new(1.0, 0.4, 1.0)), (timeline.frame_passing(72.0), Vec3::new(1.0, 0.55, 0.15)),
            (timeline.end_frame(), Vec3::new(1.0, 0.25, 0.25)),
        ];
        for (f, rgb) in gates {
            let cam = ViewCamera::scripted(states[f.min(timeline.end_frame())], video);
            let (bx, by, _) = cam.basis();
            let r = 3.0;
            let c = [cam.position - bx * r - by * r, cam.position + bx * r - by * r, cam.position + bx * r + by * r, cam.position - bx * r + by * r];
            for i in 0..4 { self.line(out, c[i], c[(i + 1) % 4], rgb.extend(0.9), 3.0, camera) }
        }
        if !show_frustum { return }
        let cam = ViewCamera::scripted(timeline.camera(frame), video);
        let (bx, by, bz) = cam.basis();
        let depth = 10.0;
        let (half_w, half_h) = (depth / 3.2, depth / cam.y_scale());
        let centre = cam.position + bz * depth;
        let corners = [centre - bx * half_w - by * half_h, centre + bx * half_w - by * half_h, centre + bx * half_w + by * half_h, centre - bx * half_w + by * half_h];
        for i in 0..4 {
            self.line(out, cam.position, corners[i], Vec4::ONE, 3.0, camera);
            self.line(out, corners[i], corners[(i + 1) % 4], Vec4::ONE, 3.0, camera);
        }
        let apex = centre - by * (half_h * 1.35);
        self.line(out, corners[0], apex, Vec4::ONE, 3.0, camera);
        self.line(out, corners[1], apex, Vec4::ONE, 3.0, camera);
    }
}
