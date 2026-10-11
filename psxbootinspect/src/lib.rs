//! PSX Boot Inspection Tool — cross-platform app (egui + wgpu): PS1, PS2 and PS3 boots from the
//! user's own files, research instruments, and the sound of every instrument in them.

mod audio;
mod director;
mod model;
mod offline;
mod renderer;
mod scenes;
mod settings;
mod ui;
mod uitest;

use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Set by the headless UI harness (`--ui`): native file dialogs and the file manager never open
/// then, whatever a script clicks — the harness must never reach the user's screen.
pub(crate) static HEADLESS: AtomicBool = AtomicBool::new(false);

pub(crate) fn headless() -> bool { HEADLESS.load(Ordering::Relaxed) }

/// The only way the app opens a native file dialog: a no-op returning `None` under the harness.
pub(crate) trait Dialog {
    fn pick_file_ui(self) -> Option<PathBuf>;
    fn save_file_ui(self) -> Option<PathBuf>;
    fn pick_folder_ui(self) -> Option<PathBuf>;
}

impl Dialog for rfd::FileDialog {
    fn pick_file_ui(self) -> Option<PathBuf> { if headless() { eprintln!("ui: file dialog suppressed (headless)"); None } else { self.pick_file() } }
    fn save_file_ui(self) -> Option<PathBuf> { if headless() { eprintln!("ui: save dialog suppressed (headless)"); None } else { self.save_file() } }
    fn pick_folder_ui(self) -> Option<PathBuf> { if headless() { eprintln!("ui: folder dialog suppressed (headless)"); None } else { self.pick_folder() } }
}

/// Runs the app: the window, or with `--ui`/`--render` the headless harness and renderer.
pub fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--ui") {
        HEADLESS.store(true, Ordering::Relaxed);
        if let Err(e) = uitest::run(&args[2..]) { eprintln!("error: {e}"); std::process::exit(1) }
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--render") {
        if let Err(e) = offline::run(&args[2..]) { eprintln!("error: {e}"); std::process::exit(1) }
        return Ok(());
    }
    let icon = image::load_from_memory(include_bytes!("../../docs/icon.png")).ok().map(|i| {
        let rgba = i.to_rgba8();
        egui::IconData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() }
    });
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1400.0, 860.0]).with_min_inner_size([1000.0, 600.0]).with_title("PSX Boot Inspection Tool");
    if let Some(icon) = icon { viewport = viewport.with_icon(std::sync::Arc::new(icon)) }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("PSX Boot Inspection Tool", options, Box::new(|cc| Ok(Box::new(ui::App::new(cc)))))
}

/// The scene texture shown inside egui, re-registered whenever it is recreated.
pub(crate) struct SceneView {
    pub texture_id: egui::TextureId,
}

impl SceneView {
    pub fn register(rs: &eframe::egui_wgpu::RenderState, view: &wgpu::TextureView) -> Self {
        let id = rs.renderer.write().register_native_texture(&rs.device, view, wgpu::FilterMode::Linear);
        Self { texture_id: id }
    }
}

pub(crate) fn arc_device(rs: &eframe::egui_wgpu::RenderState) -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    (Arc::new(rs.device.clone()), Arc::new(rs.queue.clone()))
}
