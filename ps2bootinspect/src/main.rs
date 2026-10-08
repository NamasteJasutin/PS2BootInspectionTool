//! PS2 Boot Inspection Tool — cross-platform app (egui + wgpu).

mod audio;
mod model;
mod offline;
mod renderer;
mod scenes;
mod ui;

use eframe::egui;
use std::sync::Arc;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--render") {
        if let Err(e) = offline::run(&args[2..]) { eprintln!("error: {e}"); std::process::exit(1) }
        return Ok(());
    }
    let icon = image::load_from_memory(include_bytes!("../../docs/icon.png")).ok().map(|i| {
        let rgba = i.to_rgba8();
        egui::IconData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() }
    });
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1400.0, 860.0]).with_min_inner_size([1000.0, 600.0]).with_title("PS2 Boot Inspection Tool");
    if let Some(icon) = icon { viewport = viewport.with_icon(std::sync::Arc::new(icon)) }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("PS2 Boot Inspection Tool", options, Box::new(|cc| Ok(Box::new(ui::App::new(cc)))))
}

/// The scene texture shown inside egui, re-registered whenever it is recreated.
pub struct SceneView {
    pub texture_id: egui::TextureId,
    pub size: (u32, u32),
}

impl SceneView {
    pub fn register(rs: &eframe::egui_wgpu::RenderState, view: &wgpu::TextureView, size: (u32, u32)) -> Self {
        let id = rs.renderer.write().register_native_texture(&rs.device, view, wgpu::FilterMode::Linear);
        Self { texture_id: id, size }
    }
}

pub fn arc_device(rs: &eframe::egui_wgpu::RenderState) -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    (Arc::new(rs.device.clone()), Arc::new(rs.queue.clone()))
}
