//! wgpu plumbing shared by the scenes: offscreen targets, pipelines per (format, blend,
//! depth), one bump-allocated vertex buffer per frame, and the GS-style helpers the
//! scenes build their frames from.

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3, Vec4};
use ps2kit::bios::OpeningAssets;
use ps2kit::sim::CameraState;
use ps2kit::VideoMode;
use std::collections::HashMap;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: Vec4,
    pub uv: Vec2,
    pub pad: Vec2,
    pub color: Vec4,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GlassVertex {
    pub pos: Vec4,
    pub a: Vec4,        // refraction offset (uv), reflection uv
    pub b: Vec4,        // noise uv, magnification, rim
    pub c: Vec4,        // projected cube centre (uv), reflection strength
    pub tint: Vec4,     // multiplies the refracted frame
    pub refl_tint: Vec4,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Blend {
    Opaque,
    Add,
    AddSrcAlpha,
    Alpha,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DepthMode {
    None,
    Test,
    Write,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DepthAction {
    None,
    Clear,
    Load,
}

pub struct Batch {
    pub texture: &'static str,
    pub texture_owned: Option<String>,
    pub blend: Blend,
    pub depth: DepthMode,
    pub repeat_uv: bool,
    pub range: std::ops::Range<u32>,
}

impl Batch {
    pub fn new(texture: &'static str, blend: Blend, range: std::ops::Range<usize>) -> Self {
        Self { texture, texture_owned: None, blend, depth: DepthMode::None, repeat_uv: false, range: range.start as u32..range.end as u32 }
    }
    pub fn named(texture: String, blend: Blend, range: std::ops::Range<usize>) -> Self {
        Self { texture: "", texture_owned: Some(texture), blend, depth: DepthMode::None, repeat_uv: false, range: range.start as u32..range.end as u32 }
    }
    pub fn depth(mut self, d: DepthMode) -> Self { self.depth = d; self }
    pub fn repeat(mut self) -> Self { self.repeat_uv = true; self }
    fn name(&self) -> &str { self.texture_owned.as_deref().unwrap_or(self.texture) }
}

/// Where the towers stand: on the console's grid, or rearranged (beyond the PS2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TowerLayout { Console, Ring, Spiral }

impl TowerLayout {
    pub const ALL: [Self; 3] = [Self::Console, Self::Ring, Self::Spiral];
    pub fn name(self) -> &'static str { match self { Self::Console => "the console's grid", Self::Ring => "a ring around the camera path", Self::Spiral => "a spiral" } }
}

/// How the towers are coloured: as the console does, or by a fact of their record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TowerTint { Console, Count, Age, Region, Publisher }

impl TowerTint {
    pub const ALL: [Self; 5] = [Self::Console, Self::Count, Self::Age, Self::Region, Self::Publisher];
    pub fn name(self) -> &'static str { match self { Self::Console => "as the console", Self::Count => "by launch count", Self::Age => "by last launch", Self::Region => "by title region", Self::Publisher => "by publisher family" } }
}

/// What to draw; every stage of the console's frame can be switched off for study.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RenderOptions {
    /// Tower placement (beyond the PS2 when not `Console`).
    pub layout: TowerLayout,
    /// Turns of the ring/spiral per second (0 = still).
    pub revolve: f32,
    /// Ring radius in world units.
    pub ring_radius: f32,
    /// Tower colouring; the colours themselves are the renderer's `tower_tints`.
    pub tint: TowerTint,
    /// Light the towers along their whole length (the console lights only the near cap, so
    /// from any other viewpoint they fade to black).
    pub solid_towers: bool,
    pub towers: bool,
    pub trails: bool,
    pub fog: bool,
    pub orbs: bool,
    pub glass: bool,
    pub defocus: bool,
    pub fade: bool,
    pub lettering: bool,
    pub letterbox: bool,
    pub colour_wrap: bool,
    pub camera_path: bool,
    pub orb_seed: f32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { layout: TowerLayout::Console, revolve: 0.0, ring_radius: 34.0, tint: TowerTint::Console, solid_towers: false, towers: true, trails: true, fog: true, orbs: true, glass: true, defocus: true, fade: true, lettering: true, letterbox: true, colour_wrap: false, camera_path: false, orb_seed: 4000.0 }
    }
}

/// A camera in the opening's world: +Z is into the screen, +Y is down the screen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ViewCamera {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub video: VideoMode,
}

impl ViewCamera {
    pub fn scripted(s: CameraState, video: VideoMode) -> Self {
        Self { position: s.position(), forward: s.forward(), up: s.up(), video }
    }

    /// Screen-right, screen-down and view axes (the console's camera-matrix construction).
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let z = self.forward.normalize();
        let x = self.up.cross(z).normalize();
        (x, z.cross(x), z)
    }

    /// Vertical clip-space scale: screen distance 1024 × line-aspect factor / half field height.
    pub fn y_scale(&self) -> f32 { 1024.0 * self.video.aspect_y() / (self.video.field_height() / 2.0) }

    /// Clip-space position reproducing the console projection on a 4:3 picture.
    pub fn project(&self, p: Vec3) -> Vec4 {
        let (bx, by, bz) = self.basis();
        let d = p - self.position;
        let v = Vec3::new(d.dot(bx), d.dot(by), d.dot(bz));
        let (near, far) = (1.0f32, 4000.0f32);
        Vec4::new(v.x * 3.2, -v.y * self.y_scale(), (v.z - near) * far / (far - near), v.z)
    }
}

pub struct Renderer {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    shader: wgpu::ShaderModule,
    pipelines: HashMap<(bool, wgpu::TextureFormat, bool, Blend, DepthMode), wgpu::RenderPipeline>,
    layout: wgpu::PipelineLayout,
    glass_layout: wgpu::PipelineLayout,
    bind_layout: wgpu::BindGroupLayout,
    glass_bind_layout: wgpu::BindGroupLayout,
    sampler_clamp: wgpu::Sampler,
    sampler_repeat: wgpu::Sampler,
    pub textures: HashMap<String, (wgpu::Texture, wgpu::TextureView)>,
    pub targets: HashMap<String, (wgpu::Texture, wgpu::TextureView)>,
    bind_cache: HashMap<(String, bool), wgpu::BindGroup>,
    pub size: (u32, u32),
    vertex_buffer: wgpu::Buffer,
    vertex_offset: u64,
    staging: Vec<u8>,
    pub assets_loaded: bool,
    pub video: VideoMode,
    // per-scene state kept across frames
    pub logo_cached_field: Option<i32>,
    /// One colour multiplier per tower of the opening scene (empty = none); see [`TowerTint`].
    pub tower_tints: Vec<Vec3>,
    /// The history record whose towers are drawn highlighted (the inspector's hover).
    pub highlight: Option<usize>,
}

const VERTEX_BUFFER_SIZE: u64 = 24 << 20;

impl Renderer {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("scene"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let tex_entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty, count: None };
        let texture_ty = wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false };
        let sampler_ty = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("tex"), entries: &[tex_entry(0, texture_ty), tex_entry(1, sampler_ty)] });
        let glass_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glass"),
            entries: &[tex_entry(0, texture_ty), tex_entry(1, texture_ty), tex_entry(2, texture_ty), tex_entry(3, sampler_ty), tex_entry(4, sampler_ty)],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bind_layout)], immediate_size: 0 });
        let glass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&glass_bind_layout)], immediate_size: 0 });
        let sampler = |mode: wgpu::AddressMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: mode, address_mode_v: mode, address_mode_w: mode,
                mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("frame vertices"), size: VERTEX_BUFFER_SIZE, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut r = Self {
            sampler_clamp: sampler(wgpu::AddressMode::ClampToEdge),
            sampler_repeat: sampler(wgpu::AddressMode::Repeat),
            device, queue, shader, pipelines: HashMap::new(), layout, glass_layout, bind_layout, glass_bind_layout,
            textures: HashMap::new(), targets: HashMap::new(), bind_cache: HashMap::new(), size: (0, 0),
            vertex_buffer, vertex_offset: 0, staging: Vec::new(), assets_loaded: false, video: VideoMode::Ntsc, logo_cached_field: None, tower_tints: Vec::new(), highlight: None,
        };
        r.upload_texture("white", 1, 1, &[255, 255, 255, 255], false);
        r.resize(1280, 960);
        r
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.size == (width, height) { return }
        self.size = (width, height);
        for name in ["sub", "scene", "copy", "temp", "logoPrev"] {
            self.make_target(name, wgpu::TextureFormat::Rgba8Unorm, width, height);
        }
        self.make_target("accum", wgpu::TextureFormat::Rgba16Float, width, height);
        self.make_target("depth", wgpu::TextureFormat::Depth32Float, width, height);
        let scale = width as f32 / 640.0;
        self.make_target("blurA", wgpu::TextureFormat::Rgba8Unorm, (384.0 * scale) as u32, (96.0 * scale) as u32);
        self.make_target("blurB", wgpu::TextureFormat::Rgba8Unorm, (384.0 * scale) as u32, (96.0 * scale) as u32);
        self.bind_cache.clear();
        self.logo_cached_field = None;
    }

    pub fn make_target(&mut self, name: &str, format: wgpu::TextureFormat, width: u32, height: u32) {
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(name), size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let v = t.create_view(&Default::default());
        self.targets.insert(name.to_string(), (t, v));
        self.bind_cache.retain(|k, _| k.0 != name);
    }

    pub fn upload_texture(&mut self, name: &str, width: usize, height: usize, rgba: &[u8], mips: bool) {
        let levels = if mips { (width.max(height) as f32).log2().floor() as u32 + 1 } else { 1 };
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(name), size: wgpu::Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 }, mip_level_count: levels, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        // Mip levels are generated on the CPU with a box filter (the console does the same in 16 bit).
        let (mut w, mut h, mut data) = (width, height, rgba.to_vec());
        for level in 0..levels {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &t, mip_level: level, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w as u32 * 4), rows_per_image: Some(h as u32) },
                wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
            );
            if w <= 1 || h <= 1 { break }
            let (nw, nh) = (w / 2, h / 2);
            let mut next = vec![0u8; nw * nh * 4];
            for y in 0..nh { for x in 0..nw { for c in 0..4 {
                let s = |dx: usize, dy: usize| data[((2 * y + dy) * w + 2 * x + dx) * 4 + c] as u32;
                next[(y * nw + x) * 4 + c] = ((s(0, 0) + s(1, 0) + s(0, 1) + s(1, 1)) / 4) as u8;
            }}}
            w = nw; h = nh; data = next;
        }
        let v = t.create_view(&Default::default());
        self.textures.insert(name.to_string(), (t, v));
        self.bind_cache.retain(|k, _| k.0 != name);
    }

    pub fn set_assets(&mut self, assets: &OpeningAssets) {
        for t in assets.textures() {
            self.upload_texture(&t.name, t.width, t.height, &t.rgba, t.mip_levels > 0);
        }
        self.assets_loaded = true;
    }

    fn view(&self, name: &str) -> Option<&wgpu::TextureView> {
        if let Some(n) = name.strip_prefix('@') { self.targets.get(n).map(|t| &t.1) } else { self.textures.get(name).or_else(|| self.textures.get("white")).map(|t| &t.1) }
    }

    fn bind_group(&mut self, name: &str, repeat: bool) -> Option<wgpu::BindGroup> {
        let key = (name.to_string(), repeat);
        if let Some(b) = self.bind_cache.get(&key) { return Some(b.clone()) }
        let view = self.view(name)?;
        let sampler = if repeat { &self.sampler_repeat } else { &self.sampler_clamp };
        let b = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &self.bind_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }],
        });
        self.bind_cache.insert(key, b.clone());
        Some(b)
    }

    fn pipeline(&mut self, glass: bool, format: wgpu::TextureFormat, has_depth: bool, blend: Blend, depth: DepthMode) -> wgpu::RenderPipeline {
        let key = (glass, format, has_depth, blend, depth);
        if let Some(p) = self.pipelines.get(&key) { return p.clone() }
        let blend_state = match blend {
            Blend::Opaque => None,
            Blend::Add => Some(wgpu::BlendState { color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add }, alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add } }),
            Blend::AddSrcAlpha => Some(wgpu::BlendState { color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add }, alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add } }),
            Blend::Alpha => Some(wgpu::BlendState { color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add }, alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add } }),
        };
        let (vs, fs, stride, attrs): (&str, &str, u64, Vec<wgpu::VertexAttribute>) = if glass {
            ("v_glass", "f_glass", 96, (0..6).map(|i| wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: i * 16, shader_location: i as u32 }).collect())
        } else {
            ("v_main", "f_tex", 48, vec![
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 16, shader_location: 1 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 2 },
            ])
        };
        let p = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(if glass { &self.glass_layout } else { &self.layout }),
            vertex: wgpu::VertexState { module: &self.shader, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: stride, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })] },
            fragment: Some(wgpu::FragmentState { module: &self.shader, entry_point: Some(fs), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: blend_state, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: has_depth.then(|| wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(depth == DepthMode::Write),
                depth_compare: Some(if depth == DepthMode::None { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::LessEqual }),
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(key, p.clone());
        p
    }

    /// Call at the start of a frame: resets the bump allocator.
    pub fn begin_frame(&mut self) {
        self.vertex_offset = 0;
        self.staging.clear();
    }

    /// Copies vertex bytes into the frame buffer and returns the byte offset.
    fn upload(&mut self, bytes: &[u8]) -> Option<u64> {
        let aligned = (self.vertex_offset + 255) & !255;
        if aligned + bytes.len() as u64 > VERTEX_BUFFER_SIZE { return None }
        self.queue.write_buffer(&self.vertex_buffer, aligned, bytes);
        self.vertex_offset = aligned + bytes.len() as u64;
        Some(aligned)
    }

    /// A render pass over a target, with optional depth, running `body` inside.
    pub fn pass(&mut self, enc: &mut wgpu::CommandEncoder, color: &str, clear: bool, depth: DepthAction, body: impl FnOnce(&mut Self, &mut wgpu::RenderPass)) {
        let Some(target) = self.targets.get(color) else { return };
        let target_view = target.1.clone();
        let depth_view = (depth != DepthAction::None).then(|| self.targets["depth"].1.clone());
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(color),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations { load: if clear { wgpu::LoadOp::Clear(wgpu::Color::BLACK) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: depth_view.as_ref().map(|v| wgpu::RenderPassDepthStencilAttachment {
                view: v,
                depth_ops: Some(wgpu::Operations { load: if depth == DepthAction::Clear { wgpu::LoadOp::Clear(1.0) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        }).forget_lifetime();
        body(self, &mut rp);
    }

    pub fn format_of(&self, target: &str) -> wgpu::TextureFormat {
        self.targets.get(target).map(|t| t.0.format()).unwrap_or(wgpu::TextureFormat::Rgba8Unorm)
    }

    /// Draws batches of triangles from `verts` into the current pass.
    pub fn draw(&mut self, rp: &mut wgpu::RenderPass, verts: &[Vertex], batches: &[Batch], format: wgpu::TextureFormat, has_depth: bool) {
        if verts.is_empty() { return }
        let Some(offset) = self.upload(bytemuck::cast_slice(verts)) else { return };
        let buffer = self.vertex_buffer.clone();
        rp.set_vertex_buffer(0, buffer.slice(offset..offset + (verts.len() * 48) as u64));
        for b in batches.iter().filter(|b| !b.range.is_empty()) {
            let pipeline = self.pipeline(false, format, has_depth, b.blend, if has_depth { b.depth } else { DepthMode::None });
            let Some(bind) = self.bind_group(b.name(), b.repeat_uv) else { continue };
            rp.set_pipeline(&pipeline);
            rp.set_bind_group(0, &bind, &[]);
            rp.draw(b.range.clone(), 0..1);
        }
    }

    pub fn draw_glass(&mut self, rp: &mut wgpu::RenderPass, verts: &[GlassVertex], source: &str, refl: &str, noise: &str, format: wgpu::TextureFormat) {
        if verts.is_empty() { return }
        let Some(offset) = self.upload(bytemuck::cast_slice(verts)) else { return };
        let buffer = self.vertex_buffer.clone();
        let pipeline = self.pipeline(true, format, false, Blend::Opaque, DepthMode::None);
        let (Some(src), Some(r), Some(n)) = (self.targets.get(source).map(|t| &t.1), self.view(refl), self.view(noise)) else { return };
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &self.glass_bind_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(r) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(n) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler_clamp) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.sampler_repeat) },
            ],
        });
        rp.set_pipeline(&pipeline);
        rp.set_bind_group(0, &bind, &[]);
        rp.set_vertex_buffer(0, buffer.slice(offset..offset + (verts.len() * 96) as u64));
        rp.draw(0..verts.len() as u32, 0..1);
    }

    pub fn blit(&self, enc: &mut wgpu::CommandEncoder, from: &str, to: &str) {
        let (Some(a), Some(b)) = (self.targets.get(from), self.targets.get(to)) else { return };
        enc.copy_texture_to_texture(a.0.as_image_copy(), b.0.as_image_copy(), a.0.size());
    }

    // MARK: geometry helpers (NDC, uv origin top-left)

    pub fn quad(out: &mut Vec<Vertex>, x0: f32, y0: f32, x1: f32, y1: f32, u0: f32, v0: f32, u1: f32, v1: f32, color: Vec4) {
        let v = |x, y, u, w| Vertex { pos: Vec4::new(x, y, 0.0, 1.0), uv: Vec2::new(u, w), pad: Vec2::ZERO, color };
        let (tl, tr, bl, br) = (v(x0, y1, u0, v0), v(x1, y1, u1, v0), v(x0, y0, u0, v1), v(x1, y0, u1, v1));
        out.extend_from_slice(&[tl, tr, bl, bl, tr, br]);
    }

    pub fn full_quad(out: &mut Vec<Vertex>, color: Vec4) { Self::quad(out, -1.0, -1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, color) }

    /// A sprite in the console's 640 × field-height coordinates.
    pub fn sprite(&self, out: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32, u: f32, v: f32, uw: f32, vh: f32, tex: (f32, f32), color: Vec4) {
        let half = self.video.field_height() / 2.0;
        Self::quad(out, x / 320.0 - 1.0, 1.0 - (y + h) / half, (x + w) / 320.0 - 1.0, 1.0 - y / half, u / tex.0, v / tex.1, (u + uw) / tex.0, (v + vh) / tex.1, color);
    }

    /// Black bars that leave a 16:9 window in the current field.
    pub fn letterbox(&self, out: &mut Vec<Vertex>) {
        let h = self.video.field_height();
        let visible = (640.0 * 9.0 * self.video.aspect_y() / 16.0).floor();
        let bar = ((h - visible + 1.0) / 2.0).floor();
        let black = Vec4::new(0.0, 0.0, 0.0, 1.0);
        self.sprite(out, 0.0, 0.0, 640.0, bar, 0.0, 0.0, 1.0, 1.0, (1.0, 1.0), black);
        self.sprite(out, 0.0, h - bar, 640.0, bar, 0.0, 0.0, 1.0, 1.0, (1.0, 1.0), black);
    }

    pub fn billboard(out: &mut Vec<Vertex>, centre: Vec3, half: f32, color: Vec4, camera: &ViewCamera) {
        let (bx, by, _) = camera.basis();
        let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
        let q: Vec<Vertex> = corners.iter().map(|&(cx, cy)| Vertex { pos: camera.project(centre + bx * (cx * half) + by * (cy * half)), uv: Vec2::new((cx + 1.0) / 2.0, (cy + 1.0) / 2.0), pad: Vec2::ZERO, color }).collect();
        if q.iter().all(|v| v.pos.w > 1.0) {
            out.extend_from_slice(&[q[0], q[1], q[2], q[2], q[1], q[3]]);
        }
    }

    /// A world-space line drawn `width` target pixels wide, near-clipped.
    pub fn line(&self, out: &mut Vec<Vertex>, a: Vec3, b: Vec3, color: Vec4, width: f32, camera: &ViewCamera) {
        let (mut ca, mut cb) = (camera.project(a), camera.project(b));
        let near = 1.05;
        if ca.w < near && cb.w < near { return }
        if ca.w < near { ca = cb + (ca - cb) * ((cb.w - near) / (cb.w - ca.w)) }
        if cb.w < near { cb = ca + (cb - ca) * ((ca.w - near) / (ca.w - cb.w)) }
        let pa = ca.truncate() / ca.w;
        let pb = cb.truncate() / cb.w;
        let pixel = Vec2::new(2.0 / self.size.0 as f32, 2.0 / self.size.1 as f32);
        let d = Vec2::new(pb.x - pa.x, pb.y - pa.y) / pixel;
        if d.length_squared() < 1e-6 { return }
        let n = Vec2::new(-d.y, d.x).normalize() * pixel * width * 0.5;
        let v = |p: Vec3, s: f32| Vertex { pos: Vec4::new(p.x + n.x * s, p.y + n.y * s, p.z, 1.0), uv: Vec2::ZERO, pad: Vec2::ZERO, color };
        out.extend_from_slice(&[v(pa, -1.0), v(pa, 1.0), v(pb, -1.0), v(pb, -1.0), v(pa, 1.0), v(pb, 1.0)]);
    }
}

pub const SHADER: &str = r#"
struct VIn { @location(0) pos: vec4<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32> };
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };

@vertex fn v_main(v: VIn) -> VOut {
    var o: VOut;
    o.pos = v.pos; o.uv = v.uv; o.color = v.color;
    return o;
}

@group(0) @binding(0) var t_tex: texture_2d<f32>;
@group(0) @binding(1) var s_tex: sampler;

@fragment fn f_tex(i: VOut) -> @location(0) vec4<f32> {
    return textureSample(t_tex, s_tex, i.uv) * i.color;
}

struct GIn { @location(0) pos: vec4<f32>, @location(1) a: vec4<f32>, @location(2) b: vec4<f32>, @location(3) c: vec4<f32>, @location(4) tint: vec4<f32>, @location(5) refl_tint: vec4<f32> };
struct GOut { @builtin(position) pos: vec4<f32>, @location(0) a: vec4<f32>, @location(1) b: vec4<f32>, @location(2) c: vec4<f32>, @location(3) tint: vec4<f32>, @location(4) refl_tint: vec4<f32> };

@vertex fn v_glass(v: GIn) -> GOut {
    var o: GOut;
    o.pos = v.pos; o.a = v.a; o.b = v.b; o.c = v.c; o.tint = v.tint; o.refl_tint = v.refl_tint;
    return o;
}

@group(0) @binding(0) var g_scene: texture_2d<f32>;
@group(0) @binding(1) var g_refl: texture_2d<f32>;
@group(0) @binding(2) var g_noise: texture_2d<f32>;
@group(0) @binding(3) var g_clamp: sampler;
@group(0) @binding(4) var g_repeat: sampler;

@fragment fn f_glass(i: GOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(g_scene));
    var uv = i.pos.xy / size;
    uv = uv + (uv - i.c.xy) * i.b.z + i.a.xy;
    var col = textureSample(g_scene, g_clamp, uv).rgb * i.tint.rgb + vec3<f32>(i.b.w);
    col = col + textureSample(g_refl, g_repeat, i.a.zw).rgb * i.refl_tint.rgb * textureSample(g_noise, g_repeat, i.b.xy).a * 2.0 * i.c.z;
    return vec4<f32>(col, 1.0);
}
"#;
