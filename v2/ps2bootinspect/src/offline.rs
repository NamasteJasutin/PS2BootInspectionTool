//! `ps2bootinspect --render <frame> <out.png> [--bios f] [--card f | --titles N --launches N]
//!  [--scene boot|warning|logo|full] [--pal] [--iso f] [--disc s] [--exit s] [--free x,y,z,yaw,pitch]`
//! renders one frame without a window (headless wgpu).

use crate::model::{FreeCamera, Model};
use crate::renderer::Renderer;
use glam::Vec3;
use ps2kit::history::PlayHistory;
use ps2kit::memcard::MemoryCard;
use ps2kit::sim::{SceneKind, Segment, VideoMode};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub fn run(args: &[String]) -> Result<(), String> {
    let mut positional = Vec::new();
    let mut named: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--pal" { named.insert("video".into(), "pal".into()); }
        else if let Some(k) = a.strip_prefix("--") { if i + 1 < args.len() { named.insert(k.into(), args[i + 1].clone()); i += 1 } }
        else { positional.push(a.clone()) }
        i += 1;
    }
    if positional.len() != 2 { return Err("usage: --render <frame> <out.png> --bios <file> [--scene boot|warning|logo|full] [--pal] ...".into()) }
    let frame: f32 = positional[0].parse().map_err(|_| "bad frame")?;
    let mut m = Model::new();
    let bios = named.get("bios").ok_or("--bios is required")?;
    m.load_bios(Path::new(bios), false);
    if m.assets.is_none() { return Err(m.bios_status) }
    if let Some(card) = named.get("card") {
        let c = MemoryCard::open(Path::new(card)).map_err(|e| e.to_string())?;
        m.history = PlayHistory::from_card(&c).map_err(|e| e.to_string())?;
    } else if let Some(t) = named.get("titles") {
        let n: usize = t.parse().map_err(|_| "bad --titles")?;
        let l: u32 = named.get("launches").and_then(|v| v.parse().ok()).unwrap_or(30);
        m.history = PlayHistory::synthetic(&vec![l; n], &[], 1);
    }
    m.history_source = crate::model::HistorySource::Custom;
    if let Some(a) = &m.assets { m.scene = ps2kit::sim::OpeningScene::new(a, &m.history) }
    if let Some(iso) = named.get("iso") { m.load_disc(Path::new(iso), false) }
    m.video = if named.get("video").map(String::as_str) == Some("pal") { VideoMode::Pal } else { VideoMode::Ntsc };
    m.disc_seconds = named.get("disc").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    m.warning_exit_seconds = named.get("exit").and_then(|v| v.parse().ok()).unwrap_or(10.0);
    m.scene_kind = match named.get("scene").map(String::as_str) { Some("warning") => SceneKind::Warning, Some("logo") => SceneKind::Logo, Some("full") => SceneKind::Full, _ => SceneKind::Boot };
    m.rebuild_timeline();
    let free = named.get("free").and_then(|f| {
        let v: Vec<f32> = f.split(',').filter_map(|x| x.parse().ok()).collect();
        (v.len() == 5).then(|| {
            let mut cam = FreeCamera { pivot: Vec3::ZERO, distance: 1.0, yaw: v[3], pitch: v[4] };
            cam.pivot = Vec3::new(v[0], v[1], v[2]) + cam.forward();       // position given, looking along yaw/pitch
            cam.view(m.video)
        })
    });

    // Headless device.
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
    let (device, queue) = (Arc::new(device), Arc::new(queue));
    let mut r = Renderer::new(device.clone(), queue.clone());
    r.set_assets(m.assets.as_ref().unwrap());
    r.set_logo_bitmap(m.logo_bitmap().as_ref());
    let mut enc = device.create_command_encoder(&Default::default());
    r.begin_frame();
    let assets = m.assets.as_ref().unwrap();
    let warning_tex = format!("TEXOPNG{}", m.language);
    match m.scene_kind {
        SceneKind::Boot => r.render_opening(&mut enc, frame, &m.scene, assets, &m.timeline, free, &m.options),
        SceneKind::Warning => r.render_warning(&mut enc, frame, assets, &m.timeline, free, &m.options, &warning_tex),
        SceneKind::Logo => { let anim = m.logo_animation().ok_or("no PS2LOGO")?; r.render_logo(&mut enc, frame, &anim, &m.options) }
        SceneKind::Full => {
            let (span, local) = m.sequence.span_at(frame.max(0.0) as usize);
            match span.segment {
                Segment::Opening => r.render_opening(&mut enc, local as f32, &m.scene, assets, &m.sequence.opening, free, &m.options),
                Segment::Logo => { let anim = m.logo_animation().ok_or("no PS2LOGO")?; r.render_logo(&mut enc, local as f32, &anim, &m.options) }
                _ => r.pass(&mut enc, "scene", true, crate::renderer::DepthAction::None, |_, _| {}),
            }
        }
    }
    // Read back.
    let (w, h) = r.size;
    let bytes_per_row = (w * 4 + 255) & !255;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bytes_per_row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(
        r.targets["scene"].0.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res| { let _ = tx.send(res); });
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h { rgba.extend_from_slice(&data[(y * bytes_per_row) as usize..(y * bytes_per_row + w * 4) as usize]) }
    for px in rgba.chunks_mut(4) { px[3] = 255 }
    image::save_buffer(&positional[1], &rgba, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())?;
    m.frame = frame;
    let c = m.camera();
    println!("frame {frame}: camera z {:.3}, {} towers, scene ends at frame {}", c.z, m.scene.towers.len(), m.end_frame());
    Ok(())
}
