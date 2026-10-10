//! Persistent GPU models from the user's card. Format facts: `ps2kit::icon`;
//! fitting, turning and tower placement are inspection aids, beyond the console.

use crate::renderer::{Renderer, ViewCamera};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use ps2kit::history::PlayHistory;
use ps2kit::icon::{IconModel, SaveIcon};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

const DRAWS: u64 = 1024;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Attributes { normal: Vec4, uv: Vec4, colour: Vec4 }

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    transform: Mat4,
    rotation: Mat4,
    directions: [Vec4; 3],
    colours: [Vec4; 3],
    ambient: Vec4,
    weights: [Vec4; 2],
    shape_count: u32,
    pad: [u32; 3],
}

struct GpuIcon {
    attributes: wgpu::Buffer,
    _positions: wgpu::Buffer,
    bind: wgpu::BindGroup,
    texture: String,
    model: IconModel,
    directions: [Vec4; 3],
    colours: [Vec4; 3],
    ambient: Vec4,
    fit: Mat4,
}

/// Buffers and pipeline retained across frames; model/texture bytes upload only on load.
pub(crate) struct CardIcons {
    models: Vec<GpuIcon>,
    titles: HashMap<String, usize>,
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    stride: u64,
    next: u64,
}

impl CardIcons {
    pub(crate) fn begin_frame(&mut self) { self.next = 0 }
}

impl Renderer {
    /// Renders a single card icon close-up for headless visual inspection.
    pub fn render_icon_preview(&mut self, enc: &mut wgpu::CommandEncoder, icon: usize, frame: f32, video: ps2kit::VideoMode) {
        let camera = ViewCamera { position: Vec3::new(0.0, 0.0, -18.0), forward: Vec3::Z, up: Vec3::Y, video, zoom: 1.0 };
        self.pass(enc, "scene", true, crate::renderer::DepthAction::Clear, |r, rp| {
            r.draw_icon(rp, icon, Vec3::ZERO, 6.0, frame, frame * std::f32::consts::TAU / (video.fps() * 20.0), false, &camera);
        });
    }

    /// Uploads every save folder's own model and texture once, replacing the previous card.
    /// Duplicate title IDs remain individually drawable; tower tags use the first save.
    pub fn set_card_icons(&mut self, saves: &[SaveIcon]) {
        if let Some(old) = self.card_icons.take() {
            for m in old.models { self.remove_texture(&m.texture); }
        }
        let entry = |binding, ty, visibility| wgpu::BindGroupLayoutEntry { binding, visibility, ty, count: None };
        let model_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("icon model"), entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64) }, wgpu::ShaderStages::VERTEX),
                entry(1, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, wgpu::ShaderStages::VERTEX),
            ],
        });
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("icons"), bind_group_layouts: &[Some(&self.bind_layout), Some(&model_layout)], immediate_size: 0 });
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("icons"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lit card icons"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vertex"), compilation_options: Default::default(), buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 48, step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
            })] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fragment"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let alignment = self.device.limits().min_uniform_buffer_offset_alignment as u64;
        let stride = (std::mem::size_of::<Uniforms>() as u64).div_ceil(alignment) * alignment;
        let uniforms = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("icon frame transforms"), size: stride * DRAWS, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut models = Vec::new();
        let mut titles = HashMap::new();
        for (i, save) in saves.iter().enumerate() {
            let m = &save.list_icon;
            if m.vertices.is_empty() || m.shapes.is_empty() || m.shapes.len() > 8 || m.shapes.iter().any(|s| s.len() != m.vertices.len()) { continue }
            let attributes: Vec<Attributes> = m.vertices.iter().map(|v| Attributes {
                normal: v.normal, uv: Vec4::new(v.uv.x, v.uv.y, 0.0, 0.0),
                // [I] GS modulation uses 128 as neutral; alpha is ignored (IconLoader.h).
                colour: Vec4::new(v.rgba[0] as f32 / 128.0, v.rgba[1] as f32 / 128.0, v.rgba[2] as f32 / 128.0, 1.0),
            }).collect();
            let positions: Vec<Vec4> = (0..m.vertices.len()).flat_map(|v| m.shapes.iter().map(move |s| s[v])).collect();
            let attributes = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("icon attributes"), contents: bytemuck::cast_slice(&attributes), usage: wgpu::BufferUsages::VERTEX });
            let positions = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("icon morph positions"), contents: bytemuck::cast_slice(&positions), usage: wgpu::BufferUsages::STORAGE });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("icon transform and shapes"), layout: &model_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &uniforms, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64) }) },
                wgpu::BindGroupEntry { binding: 1, resource: positions.as_entire_binding() },
            ] });
            let texture = format!("card-icon-{i}");
            if m.texture.len() == 128 * 128 * 4 { self.upload_texture(&texture, 128, 128, &m.texture, true) }
            else { self.upload_texture(&texture, 1, 1, &[255; 4], false) }
            let (lo, hi) = m.bounds();
            let fit = Mat4::from_scale(Vec3::splat(1.0 / (hi - lo).max_element().max(0.001))) * Mat4::from_translation(-(lo + hi) * 0.5);
            if !save.title_id.is_empty() { titles.entry(save.title_id.clone()).or_insert(models.len()); }
            models.push(GpuIcon { attributes, _positions: positions, bind, texture, model: m.clone(), directions: save.icon_sys.light_directions, colours: save.icon_sys.light_colours, ambient: save.icon_sys.ambient, fit });
        }
        self.card_icons = Some(CardIcons { models, titles, pipeline, uniforms, stride, next: 0 });
    }

    /// Associates history record slots with titles; call whenever the history changes.
    pub fn set_icon_history(&mut self, history: &PlayHistory) {
        self.icon_history = history.records.iter().map(|r| r.name.clone()).collect();
    }

    /// Looks up a loaded icon by history title ID; duplicate saves select the first.
    pub fn icon_for_title(&self, title_id: &str) -> Option<usize> { self.card_icons.as_ref()?.titles.get(title_id).copied() }

    /// Draws a fitted icon at `position`, with longest extent `scale` world units.
    /// Icon +Y is down; `face_camera` keeps thin title plaques readable while turning.
    /// `turn` is radians about the icon's Y axis; facing the camera is an inspection aid.
    /// Requires an RGBA8 pass with the shared depth attachment, after `begin_frame`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_icon(&mut self, rp: &mut wgpu::RenderPass, icon: usize, position: Vec3, scale: f32, frame: f32, turn: f32, face_camera: bool, camera: &ViewCamera) {
        let Some(icons) = &self.card_icons else { return };
        let Some(model) = icons.models.get(icon) else { return };
        if icons.next >= DRAWS || !scale.is_finite() || scale <= 0.0 || !position.is_finite() || !frame.is_finite() || !turn.is_finite() { return }
        let texture = model.texture.clone();
        let Some(texture_bind) = self.bind_group(&texture, true) else { return };
        let Some(icons) = &mut self.card_icons else { return };
        let model = &icons.models[icon];
        let (bx, by, bz) = camera.basis();
        let axes = if face_camera { Mat4::from_cols(bx.extend(0.0), by.extend(0.0), bz.extend(0.0), Vec4::W) } else { Mat4::IDENTITY };
        let rotation = axes * Mat4::from_rotation_y(turn);
        let world = Mat4::from_translation(position) * rotation * Mat4::from_scale(Vec3::splat(scale)) * model.fit;
        let view = Mat4::from_cols(bx.extend(0.0), by.extend(0.0), bz.extend(0.0), Vec4::W).transpose() * Mat4::from_translation(-camera.position);
        let projection = Mat4::from_cols(Vec4::new(3.2 * camera.zoom, 0.0, 0.0, 0.0), Vec4::new(0.0, -camera.y_scale() * camera.zoom, 0.0, 0.0), Vec4::new(0.0, 0.0, 4000.0 / 3999.0, 1.0), Vec4::new(0.0, 0.0, -4000.0 / 3999.0, 0.0));
        let weights = model.model.shape_weights(frame);
        let mut packed = [0.0; 8];
        packed[..weights.len()].copy_from_slice(&weights);
        let uniforms = Uniforms { transform: projection * view * world, rotation: Mat4::from_rotation_y(turn), directions: model.directions, colours: model.colours, ambient: model.ambient, weights: [Vec4::from_slice(&packed[..4]), Vec4::from_slice(&packed[4..])], shape_count: weights.len() as u32, pad: [0; 3] };
        let offset = icons.next * icons.stride;
        icons.next += 1;
        self.queue.write_buffer(&icons.uniforms, offset, bytemuck::bytes_of(&uniforms));
        rp.set_pipeline(&icons.pipeline);
        rp.set_bind_group(0, &texture_bind, &[]);
        rp.set_bind_group(1, &model.bind, &[offset as u32]);
        rp.set_vertex_buffer(0, model.attributes.slice(..));
        rp.draw(0..model.model.vertices.len() as u32, 0..1);
    }
}

const SHADER: &str = r#"
struct Uniforms {
    transform: mat4x4<f32>, rotation: mat4x4<f32>,
    directions: array<vec4<f32>, 3>, colours: array<vec4<f32>, 3>, ambient: vec4<f32>,
    weights: array<vec4<f32>, 2>, shape_count: u32, pad0: u32, pad1: u32, pad2: u32,
};
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(1) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(1) var<storage, read> positions: array<vec4<f32>>;
struct Out { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) colour: vec3<f32> };
@vertex fn vertex(@builtin(vertex_index) v: u32, @location(0) normal: vec4<f32>, @location(1) uv: vec4<f32>, @location(2) colour: vec4<f32>) -> Out {
    var p = vec3<f32>(0.0);
    for (var s = 0u; s < u.shape_count; s++) { p += positions[v * u.shape_count + s].xyz * u.weights[s / 4u][s % 4u]; }
    let rotated = (u.rotation * vec4<f32>(normal.xyz, 0.0)).xyz;
    let n = rotated / max(length(rotated), 0.00001);
    var light = max(u.ambient.xyz, vec3<f32>(0.0));
    for (var i = 0u; i < 3u; i++) {
        let direction = u.directions[i].xyz / max(length(u.directions[i].xyz), 0.00001);
        light += max(dot(n, -direction), 0.0) * max(u.colours[i].xyz, vec3<f32>(0.0));
    }
    var o: Out;
    o.position = u.transform * vec4<f32>(p, 1.0); o.uv = uv.xy; o.colour = colour.xyz * clamp(light, vec3<f32>(0.0), vec3<f32>(2.0));
    return o;
}
@fragment fn fragment(i: Out) -> @location(0) vec4<f32> {
    // Browser icons are opaque: stored vertex/texture alpha may be zero throughout.
    return vec4<f32>(textureSample(tex, samp, i.uv).rgb * i.colour, 1.0);
}
"#;
