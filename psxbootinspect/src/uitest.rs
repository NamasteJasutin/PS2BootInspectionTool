//! `psxbootinspect --ui <out.png> [--tab music|sound|bios|…] [--size 1400x812] [--script "…"]`:
//! runs the app's real egui interface headless — the same `ui::layout` the window uses, the
//! user's files loaded the same way, sound muted — feeds it a script of input and renders it to
//! PNG with egui-wgpu. Script steps, separated by `;`: `click x,y`, `rclick x,y`,
//! `drag x1,y1 x2,y2`, `wheel x,y dy`, `key space|enter|escape`, `type text`, `frames n`,
//! `shot path.png` (a PNG of that moment; the final frame always goes to `<out.png>`),
//! `save song.psxsong`, `wav song.wav` (the song as the engine renders it), `waitlib` (wait
//! for the Studio's background library scan).

use crate::model::{Model, Tab};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use std::sync::Arc;

struct Harness {
    ctx: egui::Context,
    model: Model,
    size: Vec2,
    renderer: egui_wgpu::Renderer,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    last: Option<egui::FullOutput>,
}

impl Harness {
    fn frame(&mut self, events: Vec<Event>) {
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)), events, ..Default::default() };
        let m = &mut self.model;
        crate::ui::focus_workspace(m);
        m.sync_audio();
        let out = self.ctx.run_ui(input, |root| {
            crate::ui::layout(root, m, |ui, _| { ui.painter().rect_filled(ui.max_rect(), 0.0, egui::Color32::from_rgb(8, 8, 16)); ui.painter().text(ui.max_rect().center(), egui::Align2::CENTER_CENTER, "(boot scene)", egui::FontId::proportional(14.0), egui::Color32::GRAY); });
        });
        for (id, deltas) in &out.textures_delta.set { for delta in deltas { self.renderer.update_texture(&self.device, &self.queue, *id, delta) } }
        if let Some(prev) = self.last.replace(out) { for id in &prev.textures_delta.free { self.renderer.free_texture(id) } }
    }

    fn snapshot(&mut self, path: &str) -> Result<(), String> {
        let out = self.last.as_ref().ok_or("no frame yet")?;
        let (w, h) = (self.size.x as u32, self.size.y as u32);
        let jobs = self.ctx.tessellate(out.shapes.clone(), 1.0);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [w, h], pixels_per_point: 1.0 };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        let extra = self.renderer.update_buffers(&self.device, &self.queue, &mut enc, &jobs, &screen);
        {
            let pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            self.renderer.render(&mut pass, &jobs, &screen);
        }
        let row = (w * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        enc.copy_texture_to_buffer(target.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        self.queue.submit(extra.into_iter().chain([enc.finish()]));
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h { rgba.extend_from_slice(&data[(y * row) as usize..(y * row + w * 4) as usize]) }
        image::save_buffer(path, &rgba, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())
    }
}

fn point(s: &str) -> Result<Pos2, String> {
    let (x, y) = s.split_once(',').ok_or_else(|| format!("bad point {s:?}"))?;
    Ok(Pos2::new(x.trim().parse().map_err(|_| format!("bad x in {s:?}"))?, y.trim().parse().map_err(|_| format!("bad y in {s:?}"))?))
}

fn press(pos: Pos2, button: PointerButton, pressed: bool) -> Event { Event::PointerButton { pos, button, pressed, modifiers: Modifiers::NONE } }

pub fn run(args: &[String]) -> Result<(), String> {
    let mut out = None;
    let (mut tab, mut size, mut script) = (Tab::Music, Vec2::new(1400.0, 812.0), String::new());
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tab" => { i += 1; tab = match args.get(i).map(String::as_str) { Some("boot") => Tab::Boot, Some("save") => Tab::SaveData, Some("disc") => Tab::Disc, Some("bios") => Tab::Bios, Some("lab") => Tab::Lab, Some("sound") => Tab::Sound, Some("fonts") => Tab::Fonts, _ => Tab::Music } }
            "--size" => { i += 1; let (w, h) = args.get(i).and_then(|s| s.split_once('x')).ok_or("--size WxH")?; size = Vec2::new(w.parse().map_err(|_| "bad width")?, h.parse().map_err(|_| "bad height")?) }
            "--script" => { i += 1; script = args.get(i).cloned().unwrap_or_default() }
            other => out = Some(other.to_string()),
        }
        i += 1;
    }
    let out = out.ok_or("usage: --ui <out.png> [--tab …] [--size WxH] [--script \"…\"]")?;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
    let (device, queue) = (Arc::new(device), Arc::new(queue));
    let renderer = egui_wgpu::Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, egui_wgpu::RendererOptions::default());
    let mut model = Model::new();
    model.load_defaults();
    model.sound_volume = 0.0;
    model.tab = tab;
    let mut h = Harness { ctx: egui::Context::default(), model, size, renderer, device, queue, last: None };
    for _ in 0..3 { h.frame(Vec::new()) }
    for step in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (verb, rest) = step.split_once(' ').unwrap_or((step, ""));
        match verb {
            "click" | "rclick" => {
                let p = point(rest)?;
                let b = if verb == "click" { PointerButton::Primary } else { PointerButton::Secondary };
                h.frame(vec![Event::PointerMoved(p)]);
                h.frame(vec![press(p, b, true)]);
                h.frame(vec![press(p, b, false)]);
            }
            "drag" => {
                let (a, b) = rest.split_once(' ').ok_or("drag x1,y1 x2,y2")?;
                let (a, b) = (point(a)?, point(b)?);
                h.frame(vec![Event::PointerMoved(a)]);
                h.frame(vec![press(a, PointerButton::Primary, true)]);
                for k in 1..=6 { h.frame(vec![Event::PointerMoved(a + (b - a) * (k as f32 / 6.0))]) }
                h.frame(vec![press(b, PointerButton::Primary, false)]);
            }
            "wheel" => {
                let (p, dy) = rest.rsplit_once(' ').ok_or("wheel x,y dy")?;
                h.frame(vec![Event::PointerMoved(point(p)?)]);
                h.frame(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: Vec2::new(0.0, dy.parse().map_err(|_| "bad dy")?), modifiers: Modifiers::NONE, phase: egui::TouchPhase::Move }]);
            }
            "key" => {
                let key = match rest { "space" => Key::Space, "enter" => Key::Enter, "escape" => Key::Escape, other => return Err(format!("unknown key {other}")) };
                h.frame(vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
                h.frame(vec![Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
            }
            "type" => h.frame(vec![Event::Text(rest.to_string())]),
            // Blocks until the Studio's background library scan has finished (up to two minutes).
            "waitlib" => {
                h.model.tab = Tab::Music;
                for _ in 0..1200 { h.frame(Vec::new()); if h.model.music.library.is_some() { break } std::thread::sleep(std::time::Duration::from_millis(100)) }
                if h.model.music.library.is_none() { return Err("the library did not load".into()) }
                h.frame(Vec::new());
            }
            "frames" => for _ in 0..rest.parse::<usize>().map_err(|_| "frames n")? { h.frame(Vec::new()) },
            "shot" => { h.frame(Vec::new()); h.snapshot(rest)? }
            "save" => { let song = h.model.music.song.as_ref().ok_or("no song")?; std::fs::write(rest, song.to_text().map_err(|e| e.to_string())?).map_err(|e| e.to_string())? }
            "wav" => {
                let (song, lib) = (h.model.music.song.as_ref().ok_or("no song")?, h.model.music.library.as_ref().ok_or("library not loaded (use waitlib)")?);
                std::fs::write(rest, crate::ui::wav_bytes(&song.render(lib.as_ref()))).map_err(|e| e.to_string())?;
            }
            other => return Err(format!("unknown step {other:?}")),
        }
    }
    h.frame(Vec::new());
    h.snapshot(&out)?;
    let (pos, playing) = h.model.audio.music_state();
    println!("ui: tab {:?}, song {:?}, transport {:.0} frames, playing {playing}", h.model.tab, h.model.music.song.as_ref().map(|s| (&s.name, s.channels.len(), s.bars())), pos);
    Ok(())
}
