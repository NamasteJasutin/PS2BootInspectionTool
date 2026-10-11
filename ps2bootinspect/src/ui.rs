//! The egui application: sidebar, the picture with camera input, visualiser, hand-off card.

use crate::audio::{VisualizerMode, BAND_COUNT};
use crate::model::{outcome_name, video_mode_name, CameraMode, Console, DiscOverride, HistorySource, Model, Scene, Tab, Workspace, LANGUAGES, REGIONS};
use crate::renderer::{Renderer, TowerLayout, TowerTint};
use crate::{arc_device, SceneView};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use glam::Vec3;
use ps2kit::sim::Segment;
use ps2kit::VideoMode;
use std::time::Instant;

pub struct App {
    model: Model,
    renderer: Renderer,
    view: SceneView,
    last: Instant,
    loaded_assets: u32,
    loaded_logo: u32,
    ps1_layout: Option<crate::scenes::ps1::Ps1Layout>,
    sce_textures: Option<crate::scenes::sce_intro::SceTextures>,
    loaded_icons: u32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.as_ref().expect("wgpu");
        let (device, queue) = arc_device(rs);
        let renderer = Renderer::new(device, queue);
        let view = SceneView::register(rs, &renderer.targets["scene"].1, renderer.size);
        let mut model = Model::new();
        model.load_defaults();
        Self { model, renderer, view, last: Instant::now(), loaded_assets: u32::MAX, loaded_logo: u32::MAX, ps1_layout: None, sce_textures: None, loaded_icons: u32::MAX }
    }

    fn render_frame(&mut self, rs: &eframe::egui_wgpu::RenderState) {
        let m = &mut self.model;
        if self.loaded_assets != m.assets_version {
            if let Some(a) = &m.assets { self.renderer.set_assets(a) }
            self.loaded_assets = m.assets_version;
        }
        if self.loaded_logo != m.logo_version {
            let bitmap = m.logo_bitmap();
            self.renderer.set_logo_bitmap(bitmap.as_ref());
            self.ps1_layout = m.ps1_shell.as_ref().filter(|_| m.ps1_active()).map(|s| self.renderer.set_ps1_assets(s, &m.ps1_licence_text()));
            self.sce_textures = m.sce_intro.as_ref().filter(|_| m.console == Console::Ps1).map(|i| self.renderer.set_sce_assets(i));
            self.loaded_logo = m.logo_version;
        }
        // The card's own save icons over their games' towers (Camp 5): re-tagged when the card
        // or the history changes.
        if self.loaded_icons != m.icons_version {
            if !m.card_icons.is_empty() { self.renderer.set_card_icons(&m.card_icons) }
            self.renderer.set_icon_history(&m.history);
            self.loaded_icons = m.icons_version;
        }
        // A standalone PS1 console needs no PS2 BIOS: its scenes are the shell's own.
        let assets = m.assets.as_ref();
        if assets.is_none() && m.ps1_shell.is_none() { return }
        if m.options.tint != crate::renderer::TowerTint::Console { self.renderer.tower_tints = m.tower_tints() } else { self.renderer.tower_tints.clear() }
        self.renderer.highlight = m.hovered_record;
        let mut enc = self.renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("scene") });
        self.renderer.begin_frame();
        let free = m.view_override();
        let warning_tex = format!("TEXOPNG{}", m.language);
        match m.scene_kind {
            Scene::Full => {
                let (span, local) = m.sequence.span_at(m.frame.max(0.0) as usize);
                match span.segment {
                    Segment::Opening => if let Some(assets) = assets { self.renderer.render_opening(&mut enc, local as f32 + m.frame.fract(), &m.scene, assets, &m.sequence.opening, free, &m.options) },
                    Segment::Logo => if let (Some(shell), Some(logo), Some(layout)) = (m.ps1_shell.as_ref().filter(|_| m.ps1_active()), m.ps1_logo_model(), &self.ps1_layout) {
                        self.renderer.render_ps1_licence(&mut enc, local as f32, shell, logo, layout, m.video, free, &m.options)
                    } else if let Some(anim) = m.logo_animation() {
                        self.renderer.render_logo(&mut enc, local as f32, &anim, &m.options);
                        if let Some(view) = &free { self.renderer.picture_plane(&mut enc, "copy", view) }
                    }
                    Segment::Warning => if let Some(assets) = assets { self.renderer.render_warning(&mut enc, local as f32, assets, &m.sequence.warning, free, &m.options, &warning_tex) },
                    Segment::Intro => if let (Some(intro), Some(tex)) = (&m.sce_intro, &self.sce_textures) {
                        let f = intro.frame_for_video(local, m.video);
                        self.renderer.render_sce_intro(&mut enc, local as f32, &f, tex, m.video)
                    },
                    _ => { self.renderer.logo_cached_field = None; self.renderer.pass(&mut enc, "scene", true, crate::renderer::DepthAction::None, |_, _| {}) }
                }
            }
            Scene::Boot => if let Some(assets) = assets { self.renderer.render_opening(&mut enc, m.frame, &m.scene, assets, &m.timeline, free, &m.options) },
            Scene::Warning => if let Some(assets) = assets { self.renderer.render_warning(&mut enc, m.frame, assets, &m.timeline, free, &m.options, &warning_tex) },
            Scene::Logo => if let (Some(shell), Some(logo), Some(layout)) = (m.ps1_shell.as_ref().filter(|_| m.ps1_active()), m.ps1_logo_model(), &self.ps1_layout) {
                self.renderer.render_ps1_licence(&mut enc, m.frame, shell, logo, layout, m.video, free, &m.options)
            } else if let Some(anim) = m.logo_animation() {
                if m.frame >= 0.0 {
                    self.renderer.render_logo(&mut enc, m.frame, &anim, &m.options);
                    if let Some(view) = &free { self.renderer.picture_plane(&mut enc, "copy", view) }
                } else { self.renderer.pass(&mut enc, "scene", true, crate::renderer::DepthAction::None, |_, _| {}) }
            }
        }
        self.renderer.queue.submit([enc.finish()]);
        let _ = rs;
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64().min(0.1);
        self.last = now;
        self.model.poll_sound();
        let music = focus_workspace(&mut self.model);
        self.model.tick(dt);
        self.model.sync_audio();
        let t0 = Instant::now();
        if !music { if let Some(rs) = frame.wgpu_render_state() { self.render_frame(rs) } }
        self.model.cpu_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let view = &self.view;
        layout(root, &mut self.model, |ui, m| picture(ui, m, view));
        ctx.request_repaint();
    }
}

/// The Studio takes the focus: the boot pauses (and resumes where it was on leaving), its audio
/// stops and the 3D scene is not rendered. Returns whether the Studio is open.
pub fn focus_workspace(m: &mut Model) -> bool {
    let studio = Workspace::of(m.tab) == Workspace::Studio;
    if studio != m.music.focused {
        m.music.focused = studio;
        if studio { m.music.resume_boot = m.playing; m.playing = false } else { m.playing = m.music.resume_boot; m.audio.music_play(false) }
    }
    studio
}

/// The window's panels — sidebar and centre — shared by the app and the headless UI harness
/// (`uitest.rs`); `picture` draws the boot scene in the centre outside the Studio.
pub fn layout(root: &mut egui::Ui, m: &mut Model, picture: impl FnOnce(&mut egui::Ui, &mut Model)) {
    egui::Panel::right("sidebar").resizable(true).default_size(330.0).min_size(260.0).show(root, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| sidebar(ui, m));
    });
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(root, |ui| {
        if Workspace::of(m.tab) == Workspace::Studio { music_editor(ui, m) } else { picture(ui, m) }
    });
}

fn picture(ui: &mut egui::Ui, m: &mut Model, view: &SceneView) {
    let avail = ui.available_rect_before_wrap();
    let scale = (avail.width() / 4.0).min(avail.height() / 3.0);
    let size = Vec2::new(4.0 * scale, 3.0 * scale);
    let rect = Rect::from_center_size(avail.center(), size);
    let response = ui.allocate_rect(avail, egui::Sense::click_and_drag());
    ui.painter().image(view.texture_id, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

    // Blender viewport navigation: middle-drag orbits, Shift+middle pans, Ctrl+middle dollies;
    // Alt+left-drag (or plain left-drag) emulates the middle button; the wheel zooms,
    // Shift/Ctrl+wheel pan; 1/3/7 snap views, Home frames the field, "." re-centres.
    if m.free_camera_enabled {
        let video = m.video;
        let mods = ui.input(|i| i.modifiers);
        if response.dragged_by(egui::PointerButton::Middle) || response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            if mods.shift { m.free_camera.pan(d.x, d.y, video) } else if mods.ctrl { m.free_camera.dolly(d.y * 0.02) } else { m.free_camera.orbit(d.x, d.y) }
        }
        if response.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if scroll != Vec2::ZERO {
                if mods.shift { m.free_camera.pan(0.0, -scroll.y * 0.4, video) }
                else if mods.ctrl { m.free_camera.pan((scroll.y + scroll.x) * 0.4, 0.0, video) }
                else { m.free_camera.dolly(scroll.y * 0.1) }
            }
            if zoom != 1.0 { m.free_camera.dolly((zoom - 1.0) * 10.0) }
            ui.input(|i| {
                let opp = i.modifiers.ctrl;
                if i.key_pressed(egui::Key::Num1) { m.free_camera.snap(1, opp) }
                if i.key_pressed(egui::Key::Num3) { m.free_camera.snap(3, opp) }
                if i.key_pressed(egui::Key::Num7) { m.free_camera.snap(7, opp) }
                if i.key_pressed(egui::Key::Home) { m.free_camera.frame_all() }
                if i.key_pressed(egui::Key::Period) { m.free_camera.pivot = Vec3::new(0.0, 0.0, 150.0) }
            });
        }
    }

    // Tower inspector: hover a tower for its record.
    m.hovered_record = None;
    if let Some(pos) = response.hover_pos() {
        if rect.contains(pos) {
            let (x, y) = ((pos.x - rect.min.x) / rect.width(), (pos.y - rect.min.y) / rect.height());
            if let Some(r) = m.pick_tower(x, y, 0.03) {
                m.hovered_record = Some(r);
                if let Some(rec) = m.history.records.get(r) {
                    let towers: String = (0..6).map(|k| if rec.mask >> k & 1 == 1 { if k == rec.index { '▫' } else { '▪' } } else { '·' }).collect();
                    let date = if rec.date == 0 { "no date".to_string() } else { format!("last launch {:04}-{:02}-{:02}", rec.year(), rec.month(), rec.day()) };
                    let growth = if rec.index == 7 { "maxed out".to_string() } else { format!("next tower at {} launches", if rec.count < 14 { 14 } else { 14 + ((rec.count - 14) / 10 + 1) * 10 }) };
                    let text = format!("record {r}: {}\n{}× launched — {towers}  {growth}\n{date}\nhistory slot {r} of 21 → grid cells from the ROM's slot table", rec.name, rec.count);
                    let galley = ui.painter().layout_no_wrap(text, egui::FontId::monospace(11.0), Color32::WHITE);
                    let at = Pos2::new((pos.x + 16.0).min(rect.max.x - galley.size().x - 12.0), (pos.y + 16.0).min(rect.max.y - galley.size().y - 12.0));
                    let bg = Rect::from_min_size(at, galley.size() + Vec2::splat(12.0));
                    ui.painter().rect_filled(bg, 4.0, Color32::from_black_alpha(200));
                    ui.painter().galley(at + Vec2::splat(6.0), galley, Color32::WHITE);
                }
            }
        }
    }

    // Status line and boot-phase readout.
    let c = m.camera();
    let fps = m.timeline.fps();
    let status = if m.scene_kind == Scene::Full {
        let (span, local) = m.sequence.span_at(m.frame.max(0.0) as usize);
        let name = segment_name(span.segment, m.ps1_active());
        format!("{name}  {:5.2} s  (segment frame {local})  camera z {:6.1}  roll {:+.2}   cpu {:4.1} ms", m.frame / fps, c.z, c.roll, m.cpu_ms)
    } else if m.frame < 0.0 {
        format!("power-on {:+5.2} s  (opening starts at 0)   cpu {:4.1} ms", m.frame / fps, m.cpu_ms)
    } else {
        format!("frame {:6.1}  {:5.2} s  camera z {:6.1}  roll {:+.2}  stage {}   cpu {:4.1} ms", m.frame, m.frame / fps, c.z, c.roll, c.stage, m.cpu_ms)
    };
    let painter = ui.painter();
    let mono = egui::FontId::monospace(12.0);
    painter.text(avail.min + Vec2::new(10.0, 8.0), egui::Align2::LEFT_TOP, status, mono.clone(), Color32::from_white_alpha(180));
    if let Some(phase) = m.boot_phase() {
        painter.text(avail.min + Vec2::new(10.0, 26.0), egui::Align2::LEFT_TOP, format!("booting: {phase}"), mono.clone(), Color32::from_rgb(230, 210, 80));
    }
    if m.assets.is_none() && m.ps1_shell.is_none() {
        painter.text(avail.center(), egui::Align2::CENTER_CENTER, "Open your PS2 or PS1 BIOS dump to begin.\nNothing from the BIOS is bundled with this app.", egui::FontId::proportional(16.0), Color32::GRAY);
    }

    // Visualiser strip along the bottom.
    if m.visualizer != VisualizerMode::Off {
        let strip = Rect::from_min_max(Pos2::new(avail.min.x, avail.max.y - 96.0), avail.max);
        painter.rect_filled(strip, 0.0, Color32::from_black_alpha(140));
        let accent = Color32::from_rgb(115, 153, 255);
        let s = &m.snapshot;
        match m.visualizer {
            VisualizerMode::Volume => {
                let rows = [("L", s.rms_db.0, s.peak_db.0, s.hold_db.0), ("R", s.rms_db.1, s.peak_db.1, s.hold_db.1)];
                let (left, right) = (strip.min.x + 36.0, strip.max.x - 16.0);
                let x = |db: f32| left + (right - left) * ((db + 60.0) / 60.0).clamp(0.0, 1.0);
                for (i, (name, rms, peak, hold)) in rows.iter().enumerate() {
                    let y = strip.min.y + 16.0 + i as f32 * 36.0;
                    painter.text(Pos2::new(strip.min.x + 18.0, y + 11.0), egui::Align2::CENTER_CENTER, *name, mono.clone(), Color32::WHITE);
                    painter.rect_filled(Rect::from_min_max(Pos2::new(left, y), Pos2::new(right, y + 22.0)), 2.0, Color32::from_white_alpha(20));
                    painter.rect_filled(Rect::from_min_max(Pos2::new(left, y), Pos2::new(x(*peak), y + 22.0)), 2.0, accent.gamma_multiply(0.45));
                    painter.rect_filled(Rect::from_min_max(Pos2::new(left, y), Pos2::new(x(*rms), y + 22.0)), 2.0, accent);
                    painter.rect_filled(Rect::from_min_max(Pos2::new(x(*hold) - 1.5, y), Pos2::new(x(*hold) + 1.5, y + 22.0)), 0.0, Color32::WHITE);
                }
            }
            VisualizerMode::Wave => {
                let mid = strip.center().y;
                let pts: Vec<Pos2> = s.wave.iter().enumerate().map(|(i, v)| Pos2::new(strip.min.x + strip.width() * i as f32 / (s.wave.len().max(2) - 1) as f32, mid - v * (strip.height() / 2.0 - 4.0))).collect();
                if pts.len() > 1 { painter.add(egui::Shape::line(pts, Stroke::new(1.5, accent))); }
            }
            VisualizerMode::Equalizer => {
                let n = s.bands.len().max(1);
                let gap = 3.0;
                let w = (strip.width() - 16.0 - gap * (BAND_COUNT as f32 - 1.0)) / BAND_COUNT as f32;
                let (top, bottom) = (strip.min.y + 8.0, strip.max.y - 14.0);
                for i in 0..n {
                    let x = strip.min.x + 8.0 + i as f32 * (w + gap);
                    let h = (bottom - top) * s.bands[i];
                    painter.rect_filled(Rect::from_min_max(Pos2::new(x, bottom - h), Pos2::new(x + w, bottom)), 2.0, accent);
                    let ph = (bottom - top) * s.band_peaks[i];
                    painter.rect_filled(Rect::from_min_max(Pos2::new(x, bottom - ph - 2.0), Pos2::new(x + w, bottom - ph)), 0.0, Color32::from_white_alpha(230));
                }
                for (label, i) in [("40 Hz", 0usize), ("300", 11), ("1k", 17), ("3k", 23), ("16k", BAND_COUNT - 1)] {
                    painter.text(Pos2::new(strip.min.x + 8.0 + i as f32 * (w + gap) + w / 2.0, strip.max.y - 6.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(9.0), Color32::from_white_alpha(150));
                }
            }
            VisualizerMode::Off => {}
        }
    }

    // Hand-off card at the end of the full sequence.
    if m.current_segment() == Some(Segment::End) {
        let card = avail.shrink(40.0);
        painter.rect_filled(card, 6.0, Color32::from_black_alpha(190));
        let mut y = card.min.y + 14.0;
        painter.text(Pos2::new(card.min.x + 16.0, y), egui::Align2::LEFT_TOP, "End of the boot sequence — what the console would do now", egui::FontId::proportional(16.0), Color32::WHITE);
        y += 28.0;
        for (who, text) in handoff_lines(m) {
            painter.text(Pos2::new(card.min.x + 236.0, y), egui::Align2::RIGHT_TOP, who, egui::FontId::monospace(11.0), Color32::from_white_alpha(230));
            let galley = painter.layout(text, egui::FontId::monospace(11.0), Color32::WHITE, card.width() - 270.0);
            let h = galley.size().y;
            painter.galley(Pos2::new(card.min.x + 246.0, y), galley, Color32::WHITE);
            y += h + 6.0;
            if y > card.max.y - 20.0 { break }
        }
    }
}

/// The hand-off facts of the console that is booting: OSDSYS's for the PS2, the PS1 kernel's
/// for a standalone PS1 BIOS.
fn handoff_lines(m: &Model) -> Vec<(String, String)> {
    match m.console {
        Console::Ps1 if m.ps1_active() => m.ps1_boot_steps().iter().map(|s| (s.who().to_string(), s.to_string())).collect(),
        Console::Ps1 => vec![("PS1 shell (0x80030000)".into(), "no PlayStation disc in the drive: the shell never returns to the kernel's boot path and stays in its memory-card / CD-player menu (ps1_kernel_boot.md §2–§3)".into())],
        Console::Ps2 => m.handoff_steps().iter().map(|s| (s.who(), s.to_string())).collect(),
    }
}

pub fn segment_name(segment: Segment, ps1: bool) -> &'static str {
    match segment {
        Segment::PowerOn => "power-on",
        Segment::Opening => "ONE: BIOS opening",
        Segment::Handoff => "hand-off",
        Segment::Logo => if ps1 { "TWO: PS1 licence screen" } else { "TWO: disc logo" },
        Segment::Warning => "TWO: warning scene",
        Segment::Menu => "TWO: clock / main menu",
        Segment::End => "end",
        Segment::Intro => "ONE: SCE intro",
        _ => "?",
    }
}

fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(8.0);
    ui.heading(title);
    body(ui);
}

fn file_row(ui: &mut egui::Ui, label: &str, status: &str, directories: bool, filter: &[&str]) -> Option<std::path::PathBuf> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Open…").clicked() {
                let mut d = rfd::FileDialog::new();
                if let Some(p) = Model::pcsx2_dir() { d = d.set_directory(p) }
                if !filter.is_empty() { d = d.add_filter("files", filter) }
                picked = if directories { d.pick_folder().or_else(|| rfd::FileDialog::new().pick_file()) } else { d.pick_file() };
            }
        });
    });
    ui.small(status);
    picked
}

fn sidebar(ui: &mut egui::Ui, m: &mut Model) {
    ui.add_space(6.0);
    // Workspaces first (Boot · Inspect · Studio), then the open workspace's own tabs.
    let current = Workspace::of(m.tab);
    ui.horizontal(|ui| {
        for w in Workspace::ALL {
            let b = egui::Button::selectable(w == current, egui::RichText::new(w.name()).size(15.0).strong()).min_size(Vec2::new(86.0, 28.0));
            if ui.add(b).clicked() && w != current { m.tab = w.tabs()[0] }
        }
    });
    let tabs = Workspace::of(m.tab).tabs();
    if tabs.len() > 1 {
        ui.horizontal_wrapped(|ui| { for t in tabs { ui.selectable_value(&mut m.tab, *t, t.name()); } });
    }
    ui.separator();
    match m.tab {
        Tab::Boot => boot_tab(ui, m),
        Tab::SaveData => save_data_tab(ui, m),
        Tab::Disc => disc_tab(ui, m),
        Tab::Bios => bios_tab(ui, m),
        Tab::Lab => lab_tab(ui, m),
        Tab::Sound => sound_tab(ui, m),
        Tab::Fonts => fonts_tab(ui, m),
        Tab::Music => music_tab(ui, m),
    }
    ui.add_space(12.0);
}

fn boot_tab(ui: &mut egui::Ui, m: &mut Model) {
    section(ui, "Console", |ui| {
        ui.horizontal(|ui| {
            for c in [Console::Ps2, Console::Ps1] {
                let available = match c { Console::Ps2 => m.assets.is_some(), Console::Ps1 => m.ps1_bios.is_some() };
                if ui.add_enabled(available, egui::Button::selectable(m.console == c, c.name())).clicked() { m.set_console(c) }
            }
        });
        // Every BIOS in the folder, per console: switching re-reads everything from that file.
        let want = m.console;
        let current = match want { Console::Ps2 => m.bios_path.clone(), Console::Ps1 => m.ps1_bios_path.clone() };
        let entries: Vec<_> = m.shelf.iter().filter(|e| e.console == want).cloned().collect();
        if entries.len() > 1 {
            let label = entries.iter().find(|e| Some(&e.path) == current.as_ref()).map_or("choose…".to_string(), |e| e.label.clone());
            let mut pick = None;
            egui::ComboBox::from_id_salt("shelf").width(ui.available_width() - 8.0).selected_text(label).show_ui(ui, |ui| {
                for e in &entries { if ui.selectable_label(Some(&e.path) == current.as_ref(), &e.label).clicked() { pick = Some(e.path.clone()) } }
            });
            if let Some(p) = pick { m.load_bios(&p, false) }
        }
        ui.small(match want { Console::Ps2 => m.bios_status.clone(), Console::Ps1 => m.ps1_bios_status.clone() });
    });
    section(ui, "Your files", |ui| {
        if let Some(p) = file_row(ui, "BIOS (PS2 or PS1)", "", false, &["bin", "BIN", "rom"]) { m.load_bios(&p, false) }
        if let Some(p) = file_row(ui, "Memory card", &m.card_status.clone(), true, &["ps2"]) { m.load_card(&p, false) }
        if let Some(p) = file_row(ui, "Game disc image", &m.disc_status.clone(), false, &["iso", "bin", "cue", "img"]) { m.load_disc(&p, false) }
    });
    section(ui, "Scenario", |ui| {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Console region");
            let current = REGIONS.iter().find(|r| r.0 == m.region_override).map(|r| r.1).unwrap_or("?");
            egui::ComboBox::from_id_salt("region").selected_text(current).show_ui(ui, |ui| {
                for (r, name) in REGIONS { changed |= ui.selectable_value(&mut m.region_override, r, name).changed(); }
            });
        });
        ui.horizontal(|ui| {
            ui.label("The drive finds");
            egui::ComboBox::from_id_salt("tray").selected_text(m.disc_override.name()).show_ui(ui, |ui| {
                for d in DiscOverride::ALL { changed |= ui.selectable_value(&mut m.disc_override, d, d.name()).changed(); }
            });
        });
        changed |= ui.checkbox(&mut m.enforce_checks, "Enforce the console's region lock").changed();
        let detected = m.detected_region().map(|r| format!("{r:?}")).unwrap_or_else(|| "unknown".into());
        ui.small(format!("Detected: {detected} console. Outcome: {}.", outcome_name(m.outcome())));
        if changed { m.rebuild_timeline(); m.logo_version += 1; if m.frame > m.end_frame() { m.frame = m.start_frame() } }
    });
    section(ui, "Scene", |ui| {
        let before = m.scene_kind;
        let mut kind = m.scene_kind;
        egui::ComboBox::from_id_salt("scene").selected_text(kind.name()).show_ui(ui, |ui| {
            for s in Scene::ALL { ui.selectable_value(&mut kind, s, s.name()); }
        });
        if kind != before { m.set_scene(kind) }
        let mut rebuild = false;
        match m.scene_kind {
            Scene::Full => {
                ui.small("Phase ONE (BIOS): power-on, the opening. Phase TWO: what the scenario leads to — the hand-off and the disc's logo, the PS1 licence screen, the warning scene, or the menu. The console is never asked to run the game.");
                if matches!(m.sequence.outcome, ps2kit::sim::BootOutcome::Game | ps2kit::sim::BootOutcome::Ps1Game) {
                    rebuild |= ui.add(egui::Slider::new(&mut m.handoff_seconds, 0.2..=4.0).text("hand-off to PS2LOGO (s)")).changed();
                }
            }
            Scene::Logo => { ui.small(if m.ps1_active() { "A PlayStation disc: the licence screen the PS1 shell inside the BIOS (rom0:LOGO) draws — the logo model from the disc's sectors 5–11, the text from the licence sector, the font from rom0:KROM, the chime from the shell's sound bank." } else { "What a licensed disc shows before its game starts: the lettering comes from the disc's first 12 sectors, the animation from rom0:PS2LOGO." }); }
            Scene::Warning | Scene::Boot => {}
        }
        if m.scene_kind == Scene::Warning || (m.scene_kind == Scene::Full && m.sequence.outcome == ps2kit::sim::BootOutcome::Warning) {
            egui::ComboBox::from_id_salt("lang").selected_text(LANGUAGES.iter().find(|l| l.0 == m.language).map(|l| l.1).unwrap_or("")).show_ui(ui, |ui| {
                for (code, name) in LANGUAGES { ui.selectable_value(&mut m.language, code, name); }
            });
            rebuild |= ui.add(egui::Slider::new(&mut m.warning_exit_seconds, 3.0..=60.0).text("drive reports a change after (s)")).changed();
        }
        ui.horizontal(|ui| {
            for v in [VideoMode::Ntsc, VideoMode::Pal] {
                if ui.selectable_label(m.video == v, video_mode_name(v)).clicked() && m.video != v { m.video = v; rebuild = true }
            }
        });
        if rebuild { m.rebuild_timeline() }
    });
    section(ui, "Time", |ui| {
        ui.horizontal(|ui| {
            if ui.button(if m.playing { "Pause" } else { "Play" }).clicked() { m.playing = !m.playing }
            if ui.button("Restart").clicked() { m.frame = m.start_frame(); m.playing = true }
            ui.checkbox(&mut m.looping, "Loop");
            if ui.button("◀").on_hover_text("one field back").clicked() { m.playing = false; m.frame = (m.frame - 1.0).max(m.start_frame()) }
            if ui.button("▶").on_hover_text("one field forward").clicked() { m.playing = false; m.frame = (m.frame + 1.0).min(m.end_frame()) }
        });
        let (start, end) = (m.start_frame(), m.end_frame().max(1.0));
        ui.add(egui::Slider::new(&mut m.frame, start..=end).show_value(false));
        if m.scene_kind == Scene::Full { segment_strip(ui, m) }
        ui.add(egui::Slider::new(&mut m.speed, 0.05..=2.0).text("speed").logarithmic(true));
        let mut rebuild = false;
        rebuild |= ui.add(egui::Slider::new(&mut m.power_on_seconds, 0.0..=6.0).text("power-on black (s)")).changed();
        if matches!(m.scene_kind, Scene::Boot | Scene::Full) {
            rebuild |= ui.add(egui::Slider::new(&mut m.disc_seconds, 0.0..=10.5).text("disc identified after (s)")).changed();
        }
        if rebuild { m.rebuild_timeline() }
    });
    section(ui, "Camera", |ui| {
        ui.horizontal(|ui| {
            ui.label("Path");
            egui::ComboBox::from_id_salt("campath").selected_text(m.camera_mode.name()).show_ui(ui, |ui| {
                for c in CameraMode::ALL { if ui.selectable_value(&mut m.camera_mode, c, c.name()).changed() { m.free_camera_enabled = false; m.options.solid_towers = c != CameraMode::Scripted } }
            });
        });
        ui.small(m.camera_mode.describe());
        if m.camera_mode != CameraMode::Scripted {
            let (mode, logo) = (m.camera_mode, m.in_logo_phase());
            let p = m.path_params_mut();
            ui.add(egui::Slider::new(&mut p.speed, 0.05..=5.0).text("degrees per frame").logarithmic(true));
            ui.add(egui::Slider::new(&mut p.distance, 0.3..=5.0).text("distance (× the console's start)").logarithmic(true));
            if !logo { ui.add(egui::Slider::new(&mut p.focus, 0.0..=1.0).text("aim: floor → highest top")); }
            if matches!(mode, CameraMode::Orbit | CameraMode::FigureEight | CameraMode::Tornado) && !logo { ui.add(egui::Slider::new(&mut p.height, 0.0..=1.5).text("height (× radius)")); }
            ui.add(egui::Slider::new(&mut p.zoom, 0.2..=1.5).text("lens (× the console's)").logarithmic(true));
            if ui.small_button("Reset this path's dials").clicked() { *p = mode.default_params() }
        } else if m.free_camera_enabled { ui.add(egui::Slider::new(&mut m.free_zoom, 0.2..=1.5).text("lens (× the console's)").logarithmic(true)); }
        director_controls(ui, m);
        if ui.checkbox(&mut m.free_camera_enabled, "Free camera").changed() && m.free_camera_enabled { m.reset_free_camera() }
        ui.small("Blender controls: middle-drag (or Alt+drag) orbits, Shift+drag pans, Ctrl+drag dollies; wheel zooms, Shift/Ctrl+wheel pan; 1/3/7 front/right/top (Ctrl for the opposite), Home frames the field, . re-centres.");
        if ui.add_enabled(m.free_camera_enabled, egui::Button::new("Back to the scripted position")).clicked() { m.reset_free_camera() }
        let mut path = m.options.camera_path;
        if ui.checkbox(&mut path, "Show the scripted camera path").changed() { m.set_camera_path(path) }
        if m.options.camera_path {
            ui.small("Route: yellow (bright = travelled); rungs every 10 frames point screen-up. Gates: cyan lettering, green dive, magenta defocus, orange fade, red end.");
            ui.horizontal(|ui| {
                if ui.button("View from the side").clicked() { m.free_camera_enabled = true; m.view_path_from_side() }
                if ui.button("Export CSV…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_file_name("ps2-opening-camera-path.csv").save_file() { let _ = std::fs::write(p, m.camera_path_csv()); }
                }
            });
        }
    });
    section(ui, "Sound", |ui| {
        ui.checkbox(&mut m.sound_enabled, "Boot chime");
        ui.add(egui::Slider::new(&mut m.sound_volume, 0.0..=1.0).text("volume"));
        ui.horizontal(|ui| { for v in VisualizerMode::ALL { ui.selectable_value(&mut m.visualizer, v, v.name()); } });
        ui.small(format!("{} · device {} Hz", m.sound_status, m.audio.device_rate as u32));
    });
    section(ui, "Layers", |ui| {
        let o = &mut m.options;
        if m.scene_kind == Scene::Logo {
            ui.checkbox(&mut o.towers, "Logo bitmap");
            ui.checkbox(&mut o.defocus, "Progressive logo blur");
            ui.checkbox(&mut o.orbs, "Outline and ribbon trails");
            ui.checkbox(&mut o.fog, "Soft-focus passes");
            ui.checkbox(&mut o.trails, "Field feedback glow");
        } else {
            ui.checkbox(&mut o.towers, "Towers");
            ui.checkbox(&mut o.trails, "Frame-to-frame smear");
            ui.checkbox(&mut o.fog, "Fog");
            ui.checkbox(&mut o.orbs, "Light orbs");
            ui.checkbox(&mut o.glass, "Glass cubes (approximate)");
            ui.checkbox(&mut o.defocus, "Defocus during dive");
            ui.checkbox(&mut o.fade, "Fade to black");
            ui.checkbox(&mut o.lettering, "Lettering");
            ui.checkbox(&mut o.letterbox, "Letterbox bars");
            ui.checkbox(&mut o.colour_wrap, "8-bit overflow on tower caps");
            ui.checkbox(&mut o.solid_towers, "Light the towers' sides (beyond the PS2)");
        }
    });
}

/// The segments of the full sequence as a coloured strip under the scrubber, the current
/// frame marked; clicking a segment jumps to its start.
fn segment_strip(ui: &mut egui::Ui, m: &mut Model) {
    let total = m.sequence.total_frames().max(1) as f32;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), egui::Sense::click());
    let painter = ui.painter();
    let colour = |s: Segment| match s {
        Segment::PowerOn | Segment::Handoff => Color32::from_gray(50),
        Segment::Opening => Color32::from_rgb(40, 70, 140),
        Segment::Logo => Color32::from_rgb(60, 120, 90),
        Segment::Warning => Color32::from_rgb(150, 40, 40),
        Segment::Menu => Color32::from_rgb(90, 80, 130),
        _ => Color32::from_gray(30),
    };
    let spans: Vec<_> = m.sequence.spans().to_vec();
    for sp in &spans {
        let x0 = rect.min.x + rect.width() * sp.start as f32 / total;
        let x1 = rect.min.x + rect.width() * (sp.start + sp.length) as f32 / total;
        let r = Rect::from_min_max(Pos2::new(x0, rect.min.y), Pos2::new(x1, rect.max.y));
        painter.rect_filled(r, 2.0, colour(sp.segment));
        if r.width() > 40.0 {
            painter.text(r.center(), egui::Align2::CENTER_CENTER, segment_name(sp.segment, m.ps1_active()).split(": ").last().unwrap_or(""), egui::FontId::proportional(10.0), Color32::from_white_alpha(200));
        }
    }
    let x = rect.min.x + rect.width() * m.frame.max(0.0) / total;
    painter.rect_filled(Rect::from_min_max(Pos2::new(x - 1.0, rect.min.y - 2.0), Pos2::new(x + 1.0, rect.max.y + 2.0)), 0.0, Color32::WHITE);
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let f = ((pos.x - rect.min.x) / rect.width() * total) as usize;
            if let Some(sp) = spans.iter().find(|sp| f >= sp.start && f < sp.start + sp.length) { m.frame = sp.start as f32; m.playing = true }
        }
    }
    if let Some(pos) = response.hover_pos() {
        let f = ((pos.x - rect.min.x) / rect.width() * total) as usize;
        if let Some(sp) = spans.iter().find(|sp| f >= sp.start && f < sp.start + sp.length) {
            response.on_hover_text(format!("{} — frames {}..{} ({:.1} s)", segment_name(sp.segment, m.ps1_active()), sp.start, sp.start + sp.length, sp.length as f32 / m.timeline.fps()));
        }
    }
}

fn save_data_tab(ui: &mut egui::Ui, m: &mut Model) {
    section(ui, "Play history", |ui| {
        let before = m.history_source;
        egui::ComboBox::from_id_salt("history").selected_text(m.history_source.name()).show_ui(ui, |ui| {
            for s in HistorySource::ALL { ui.selectable_value(&mut m.history_source, s, s.name()); }
        });
        let mut changed = before != m.history_source;
        if m.history_source == HistorySource::Custom {
            changed |= ui.add(egui::Slider::new(&mut m.custom_titles, 0..=21).text("titles")).changed();
            changed |= ui.add(egui::Slider::new(&mut m.custom_launches, 1..=64).text("launches each")).changed();
        }
        if m.history_source == HistorySource::Saves { ui.small("Launch counts are invented; only the titles come from the card."); }
        if changed { m.rebuild_history() }
    });
    section(ui, "Towers (beyond the PS2)", |ui| {
        let o = &mut m.options;
        ui.horizontal(|ui| {
            ui.label("Layout");
            egui::ComboBox::from_id_salt("layout").selected_text(o.layout.name()).show_ui(ui, |ui| {
                for l in TowerLayout::ALL { ui.selectable_value(&mut o.layout, l, l.name()); }
            });
        });
        ui.add_enabled(!m.card_icons.is_empty(), egui::Checkbox::new(&mut o.icon_tags, "Tag towers with your card's save icons"))
            .on_disabled_hover_text("Load a PS2 memory card that has saves with icons.");
        if o.layout != TowerLayout::Console {
            ui.add(egui::Slider::new(&mut o.revolve, -0.2..=0.2).text("revolve (turns/s)"));
            ui.add(egui::Slider::new(&mut o.ring_radius, 10.0..=60.0).text("radius"));
        }
        ui.horizontal(|ui| {
            ui.label("Colour");
            egui::ComboBox::from_id_salt("tint").selected_text(o.tint.name()).show_ui(ui, |ui| {
                for t in TowerTint::ALL { ui.selectable_value(&mut o.tint, t, t.name()); }
            });
        });
        ui.small(match o.tint {
            TowerTint::Console => "The console draws every tower in the same grey, lit from the front.",
            TowerTint::Count => "Steel blue → gold with the launch count; maxed-out records pale gold.",
            TowerTint::Age => "Cold for the oldest last launch in the file, warm for the newest; grey without a date.",
            TowerTint::Region => "Blue Europe, red USA, yellow Japan, green Asia/Korea/China — from the title ID's third letter.",
            TowerTint::Publisher => "Gold SC (Sony), blue SL (licensed), grey anything else.",
        });
    });
    section(ui, "Alternate history", |ui| {
        ui.small("Launch a title the way OSDSYS's HistoryUpdate would record it today: counts grow, tower bits are planted at 14/24/34/44/54, a 22nd title evicts the least-played record.");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut m.launch_title).desired_width(110.0).hint_text("SLES_123.45"));
            let id = m.launch_title.trim().to_string();
            if ui.add_enabled(!id.is_empty(), egui::Button::new("Launch")).clicked() { m.launch(&id); }
            if ui.add_enabled(!m.launch_log.is_empty(), egui::Button::new("Undo")).clicked() { m.undo_launch() }
            if ui.add_enabled(!m.launch_log.is_empty(), egui::Button::new("Reset")).clicked() { m.rebuild_history() }
        });
        let known: Vec<String> = m.history.records.iter().filter(|r| !r.is_empty()).map(|r| r.name.clone()).collect();
        ui.horizontal_wrapped(|ui| {
            for name in known { if ui.small_button(&name).on_hover_text("launch again").clicked() { m.launch(&name); } }
        });
        for (what, _) in m.launch_log.iter().rev().take(6) { ui.small(what); }
    });
    section(ui, "Export", |ui| {
        ui.horizontal(|ui| {
            if ui.button("History CSV…").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_file_name("ps2-play-history.csv").save_file() { let _ = std::fs::write(p, m.history_csv()); }
            }
            if ui.button("Hand-off facts…").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_file_name("ps2-handoff.txt").save_file() {
                    let text: String = m.handoff_steps().iter().map(|s| format!("{:<44} {s}\n", s.who())).collect();
                    let _ = std::fs::write(p, text);
                }
            }
            if ui.button("Graphs SVG…").on_hover_text("Launches per title and the Pareto curve, as a vector file").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_file_name("ps2-play-history.svg").save_file() { let _ = std::fs::write(p, history_svg(&m.history.records)); }
            }
        });
    });
    let used: Vec<_> = m.history.records.iter().filter(|r| !r.is_empty()).cloned().collect();
    section(ui, "Launches", |ui| {
        if used.is_empty() { ui.small("No titles: the screen shows no towers."); return }
        let total: u32 = used.iter().map(|r| r.count as u32).sum();
        let max = used.iter().map(|r| r.count).max().unwrap_or(1).max(1) as f32;
        ui.small(format!("{} titles, {total} launches recorded; the file holds 21 records and a count stops at 63.", used.len()));
        let mut sorted = used.clone();
        sorted.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
        let row = 16.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row * sorted.len() as f32 + 4.0), egui::Sense::hover());
        let painter = ui.painter();
        let label_w = 92.0;
        let bar_w = rect.width() - label_w - 36.0;
        for (i, r) in sorted.iter().enumerate() {
            let y = rect.min.y + 2.0 + i as f32 * row;
            painter.text(Pos2::new(rect.min.x, y + row / 2.0), egui::Align2::LEFT_CENTER, &r.name, egui::FontId::monospace(11.0), Color32::from_white_alpha(220));
            let w = bar_w * r.count as f32 / max;
            let x0 = rect.min.x + label_w;
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0, y + 2.0), Pos2::new(x0 + bar_w, y + row - 2.0)), 2.0, Color32::from_white_alpha(12));
            let colour = if r.index == 7 { Color32::from_rgb(230, 190, 80) } else { Color32::from_rgb(115, 153, 255) };
            painter.rect_filled(Rect::from_min_max(Pos2::new(x0, y + 2.0), Pos2::new(x0 + w.max(2.0), y + row - 2.0)), 2.0, colour);
            painter.text(Pos2::new(rect.max.x, y + row / 2.0), egui::Align2::RIGHT_CENTER, format!("{}", r.count), egui::FontId::monospace(11.0), Color32::WHITE);
        }
        ui.small("Gold: maxed-out records (index 7) — their towers no longer grow.");
    });
    section(ui, "Share of play (Pareto)", |ui| {
        let Some(curve) = pareto(used.iter().map(|r| r.count)) else { ui.small("Needs two or more titles."); return };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 84.0), egui::Sense::hover());
        let painter = ui.painter();
        let plot = Rect::from_min_max(rect.min + Vec2::new(28.0, 6.0), rect.max - Vec2::new(8.0, 16.0));
        let at = |i: usize, share: f32| Pos2::new(plot.min.x + plot.width() * i as f32 / curve.len() as f32, plot.max.y - plot.height() * share);
        for level in [0.5, 0.8, 1.0] {
            painter.line_segment([at(0, level), at(curve.len(), level)], Stroke::new(1.0, Color32::from_white_alpha(if level == 0.8 { 70 } else { 30 })));
            painter.text(Pos2::new(rect.min.x, at(0, level).y), egui::Align2::LEFT_CENTER, format!("{:.0}%", level * 100.0), egui::FontId::proportional(9.0), Color32::from_white_alpha(150));
        }
        painter.line_segment([at(0, 0.0), at(curve.len(), 1.0)], Stroke::new(1.0, Color32::from_white_alpha(40)));
        let pts: Vec<Pos2> = std::iter::once(at(0, 0.0)).chain(curve.iter().enumerate().map(|(i, &s)| at(i + 1, s))).collect();
        for p in &pts[1..] { painter.circle_filled(*p, 2.0, Color32::WHITE); }
        painter.add(egui::Shape::line(pts, Stroke::new(2.0, Color32::from_rgb(230, 190, 80))));
        let eighty = curve.iter().position(|&s| s >= 0.8).map_or(curve.len(), |i| i + 1);
        painter.text(Pos2::new(plot.max.x, plot.max.y + 3.0), egui::Align2::RIGHT_TOP, format!("{} titles ranked by launches", curve.len()), egui::FontId::proportional(10.0), Color32::from_white_alpha(170));
        ui.small(format!("{eighty} of {} titles account for 80% of the launches; the diagonal is an even spread.", curve.len()));
    });
    section(ui, "Last launches over time", |ui| {
        let dated: Vec<_> = used.iter().filter(|r| r.date != 0).collect();
        if dated.len() < 2 { ui.small("Needs two or more dated records."); return }
        let days = |r: &ps2kit::history::Record| (r.year() as f32 - 2000.0) * 365.25 + (r.month() as f32 - 1.0) * 30.44 + r.day() as f32;
        let (lo, hi) = dated.iter().fold((f32::MAX, f32::MIN), |(lo, hi), r| (lo.min(days(r)), hi.max(days(r))));
        let span = (hi - lo).max(1.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 64.0), egui::Sense::hover());
        let painter = ui.painter();
        let (x0, x1) = (rect.min.x + 8.0, rect.max.x - 8.0);
        let axis = rect.min.y + 40.0;
        painter.line_segment([Pos2::new(x0, axis), Pos2::new(x1, axis)], Stroke::new(1.0, Color32::from_white_alpha(80)));
        let max = dated.iter().map(|r| r.count).max().unwrap_or(1).max(1) as f32;
        for r in &dated {
            let x = x0 + (x1 - x0) * (days(r) - lo) / span;
            let h = 4.0 + 26.0 * r.count as f32 / max;
            painter.line_segment([Pos2::new(x, axis), Pos2::new(x, axis - h)], Stroke::new(2.0, Color32::from_rgb(115, 153, 255)));
            painter.circle_filled(Pos2::new(x, axis - h), 2.5, Color32::WHITE);
        }
        let first = dated.iter().min_by(|a, b| days(a).total_cmp(&days(b))).unwrap();
        let last = dated.iter().max_by(|a, b| days(a).total_cmp(&days(b))).unwrap();
        painter.text(Pos2::new(x0, axis + 4.0), egui::Align2::LEFT_TOP, format!("{:04}-{:02}-{:02}", first.year(), first.month(), first.day()), egui::FontId::proportional(10.0), Color32::from_white_alpha(170));
        painter.text(Pos2::new(x1, axis + 4.0), egui::Align2::RIGHT_TOP, format!("{:04}-{:02}-{:02}", last.year(), last.month(), last.day()), egui::FontId::proportional(10.0), Color32::from_white_alpha(170));
        ui.small("One bar per record at its last-launch date; height = launch count. The file keeps only the last date, so this is when you last came back to each game.");
    });
    section(ui, "Records", |ui| {
        ui.small("Title, launches, the six tower slots (▪ built, ▫ growing, · empty), last launch.");
        for r in &used {
            let towers: String = (0..6).map(|k| if r.mask >> k & 1 == 1 { if k == r.index { '▫' } else { '▪' } } else { '·' }).collect();
            let date = if r.date == 0 { "—".to_string() } else { format!("{:04}-{:02}-{:02}", r.year(), r.month(), r.day()) };
            ui.monospace(format!("{:<12} {:>3}× {towers} {date}", r.name, r.count));
        }
    });
}

fn disc_tab(ui: &mut egui::Ui, m: &mut Model) {
    section(ui, "Disc", |ui| {
        if let Some(d) = &m.disc {
            ui.small(format!("Volume: {} ({} sectors, {} MiB, {})", d.volume_id, d.sector_count, d.byte_size() >> 20, if d.is_dvd() { "DVD" } else { "CD" }));
            let kind = match d.kind { ps2kit::disc::DiscKind::Ps2 => "PlayStation 2 disc", ps2kit::disc::DiscKind::Ps1 => "PlayStation disc", _ => "not a PlayStation disc" };
            ui.small(format!("{kind}   Title ID: {}   register 0x{:02X} → state 0x{:02X}", d.title_id().unwrap_or_else(|| "—".into()), d.disc_type_register(), d.disc_state_code()));
            if !d.system_cnf_text.trim().is_empty() { ui.monospace(d.system_cnf_text.trim()); }
            if let Some(b) = &d.boot_elf {
                ui.small(format!("Boot ELF: {} — LBA {}, {} bytes, entry 0x{:08X}, {} segment(s)", b.file_name, b.lba, b.size, b.entry, b.segments.len()));
            }
            if let Some(b) = &d.ps1_exe {
                ui.small(format!("PS-X EXE: {} — LBA {}, {} bytes, text 0x{:08X} ({} bytes), PC 0x{:08X}", b.file_name, b.lba, b.size, b.text_addr, b.text_size, b.initial_pc));
            }
            match d.kind {
                ps2kit::disc::DiscKind::Ps2 => { ui.small(d.logo_region.map(|r| format!("Logo sectors: {r} master (checked by J/H and E consoles only)")).unwrap_or_else(|| "Logo sectors: match neither the E nor the J/A master".into())); }
                ps2kit::disc::DiscKind::Ps1 => {
                    ui.small(d.ps1_licence.as_ref().map(|l| format!("Licence sector: \"{}\" ({} chars) — logo data {}", l.text, l.line.len(), if l.logo_sectors_present { "present" } else { "absent" })).unwrap_or_else(|| "Licence sector: none".into()));
                    ui.small(match m.ps1_verdict() {
                        Some(ps2kit::disc::Ps1Verdict::NotChecked) => "This console's PS1 shell does not compare the licence line or the logo (its policy, read from its code).",
                        Some(ps2kit::disc::Ps1Verdict::Accepted) => "This console's PS1 shell accepts the licence line and the logo.",
                        Some(ps2kit::disc::Ps1Verdict::TextMismatch) => "This console's PS1 shell would not accept the licence line (black screen, endless re-read). Shown anyway.",
                        Some(ps2kit::disc::Ps1Verdict::LogoMismatch) => "The logo differs from the shell's copy: the console would hang. Shown anyway.",
                        _ => "Licence check: unknown.",
                    });
                }
                _ => { ui.small("No SYSTEM.CNF and no licence sector."); }
            }
            if let Some(cnf) = m.ps1_system_cnf() {
                ui.small(format!("As the PS1 kernel reads SYSTEM.CNF: BOOT {}, TCB {:#x}, EVENT {:#x}, STACK {:#x}{}", cnf.boot.as_deref().unwrap_or("(unset → PSX.EXE)"), cnf.tcb, cnf.event, cnf.stack,
                    if cnf.argument.is_empty() { String::new() } else { format!(", argument \"{}\" → RAM 0x180", cnf.argument_str()) }));
                for l in cnf.lints() { ui.small(format!("• {l}")); }
            }
        } else {
            ui.small("Open a game disc image (.iso or .cue) to see what the console would load.");
        }
    });
    section(ui, "What the console would do next", |ui| {
        ui.small(format!("Under the scenario: {}.", outcome_name(m.outcome())));
        for (who, text) in handoff_lines(m) {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(egui::RichText::new(who).color(Color32::from_rgb(115, 153, 255)).size(11.0));
                ui.small(text);
            });
        }
    });
}

fn bios_tab(ui: &mut egui::Ui, m: &mut Model) {
    if m.console == Console::Ps1 { ps1_bios_card(ui, m) } else { ps2_bios_card(ui, m) }
    if !m.ps1_set_issues.is_empty() {
        section(ui, "Your PS1 dumps, side by side", |ui| {
            for i in &m.ps1_set_issues {
                ui.small(match i {
                    ps2kit::ps1bios::Ps1SetIssue::Duplicate { first, second } => format!("• {first} and {second} are byte-identical."),
                    ps2kit::ps1bios::Ps1SetIssue::RegionMismatch { file, filename_letter, version_letter } => format!("• {file}: the name says {filename_letter}, the ROM's version string says {version_letter}."),
                    other => format!("• {other:?}"),
                });
            }
        });
    }
    if !m.pup_summary.is_empty() {
        section(ui, "PS3 system update", |ui| { for l in &m.pup_summary { ui.small(l); } });
    }
}

/// A standalone PS1 BIOS, read from the dump (`ps2kit::ps1bios`, `ps1_version_matrix.md`).
fn ps1_bios_card(ui: &mut egui::Ui, m: &mut Model) {
    section(ui, "PS1 ROM", |ui| {
        let Some(b) = &m.ps1_bios else { ui.small("No PS1 BIOS loaded."); return };
        let id = b.identity();
        ui.monospace(if id.version.is_empty() { "no version string (SCPH-1000 era)".to_string() } else { id.version.clone() });
        ui.small(format!("built {}  ·  {}  ·  {}", id.header_date.map(|d| format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)).unwrap_or_else(|| "?".into()), id.model, id.maker));
        ui.small(format!("Kernel: {} — {}", match id.kernel_generation { Some(ps2kit::ps1bios::KernelGeneration::K1) => "K1 (1994–95, A0 table without GetSystemInfo)", Some(ps2kit::ps1bios::KernelGeneration::K2) => "K2 (Dec 1995 on, unchanged through the PSone and POPS)", _ => "unrecognised" }, id.kernel_banner.lines().next().unwrap_or("")));
        ui.small(format!("Shell: {} — {} bytes at 0x80030000", match id.shell_storage { ps2kit::ps1bios::ShellStorage::Raw => "stored raw", ps2kit::ps1bios::ShellStorage::Packed => "OSD-LZ packed (the PS2's scheme)", ps2kit::ps1bios::ShellStorage::Pops => "POPS loader (PSP)", _ => "?" }, id.shell_length));
        if let Some(shell) = &m.ps1_shell {
            ui.small(format!("Licence check: {:?} — {}; {} licence string(s){}", shell.licence_policy,
                match shell.licence_policy { ps2kit::ps1::LicencePolicy::Unconditional => "always compares text and logo", ps2kit::ps1::LicencePolicy::Always => "compares (flag stored as 1)", ps2kit::ps1::LicencePolicy::Never => "never compares (flag stored as 0)", ps2kit::ps1::LicencePolicy::ByLetter => "compares unless the ROM letter is A", _ => "?" },
                shell.licence_strings.len(), if shell.reference_logo().is_some() { ", carries a reference logo" } else { ", no reference logo" }));
        }
        let audit = ps2kit::ps1bios::audit(b.data());
        for i in &audit.issues { ui.small(format!("Audit: {i:?}")); }
    });
}

fn ps2_bios_card(ui: &mut egui::Ui, m: &mut Model) {
    section(ui, "ROM", |ui| {
        let Some(a) = &m.assets else { ui.small("No BIOS loaded."); return };
        let v = a.rom_version();
        ui.monospace(format!("ROMVER {v}"));
        let (ver, region, kind, date) = (v.get(0..4).unwrap_or(""), v.get(4..5).unwrap_or(""), v.get(5..6).unwrap_or(""), v.get(6..14).unwrap_or(""));
        ui.small(format!("version {}.{}  region {region} ({})  type {kind} ({})  built {}-{}-{}", ver.get(0..2).unwrap_or(""), ver.get(2..4).unwrap_or(""),
            match region { "J" => "Japan", "A" => "America", "E" => "Europe", "H" => "Asia", "C" => "China", _ => "?" },
            match kind { "C" => "consumer", "D" => "development", _ => "?" },
            date.get(0..4).unwrap_or(""), date.get(4..6).unwrap_or(""), date.get(6..8).unwrap_or("")));
        ui.small(format!("Default video mode: {}.  Opening assets located by content: {} textures, tower grid, growth table, orb colours.", video_mode_name(if region == "E" { VideoMode::Pal } else { VideoMode::Ntsc }), a.textures().count()));
    });
    if !m.bios_set_lines.is_empty() {
        section(ui, "Companion files (NVM · MEC · ROM1/2 · EROM)", |ui| {
            ui.small("What the files next to this BIOS say, and what the console would do with them.");
            for (text, cite) in &m.bios_set_lines { ui.small(format!("• {text}")).on_hover_text(*cite); }
        });
    }
    section(ui, "Modules (ROMDIR)", |ui| {
        let Some(rom) = &m.rom else { return };
        let entries = rom.entries();
        ui.small(format!("{} entries, {} bytes.", entries.len(), rom.data().len()));
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (name, offset, size) in &entries {
                ui.monospace(format!("{:<10} {:>8X} {:>8}", name, offset, size));
            }
        });
    });
}

/// Cumulative share of all launches after the 1st, 2nd, … title ranked by launch count;
/// `None` with fewer than two titles or no launches.
fn pareto(counts: impl Iterator<Item = u8>) -> Option<Vec<f32>> {
    let mut counts: Vec<u32> = counts.map(u32::from).collect();
    let total: u32 = counts.iter().sum();
    if counts.len() < 2 || total == 0 { return None }
    counts.sort_unstable_by(|a, b| b.cmp(a));
    Some(counts.iter().scan(0, |acc, &c| { *acc += c; Some(*acc as f32 / total as f32) }).collect())
}

/// The save-data graphs as a standalone SVG: launches per title (gold = maxed out) above the
/// Pareto curve. Plain strings only; title IDs are escaped.
fn history_svg(records: &[ps2kit::history::Record]) -> String {
    let mut used: Vec<_> = records.iter().filter(|r| !r.is_empty()).collect();
    used.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let (w, row, label_w) = (640.0f32, 18.0f32, 110.0f32);
    let bars_h = row * used.len() as f32;
    let (curve_top, curve_h) = (bars_h + 70.0, 160.0f32);
    let h = curve_top + curve_h + 40.0;
    let max = used.iter().map(|r| r.count).max().unwrap_or(1).max(1) as f32;
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"monospace\" font-size=\"11\">\n<rect width=\"100%\" height=\"100%\" fill=\"#14161c\"/>\n<text x=\"16\" y=\"22\" fill=\"#fff\" font-size=\"14\">Launches per title</text>\n");
    for (i, r) in used.iter().enumerate() {
        let y = 34.0 + i as f32 * row;
        let bw = (w - label_w - 60.0) * r.count as f32 / max;
        let fill = if r.index == 7 { "#e6be50" } else { "#7399ff" };
        s += &format!("<text x=\"16\" y=\"{:.1}\" fill=\"#ddd\">{}</text><rect x=\"{label_w}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"2\" fill=\"{fill}\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"#fff\">{}</text>\n", y + 12.0, esc(&r.name), y + 3.0, bw.max(2.0), row - 6.0, label_w + bw + 6.0, y + 12.0, r.count);
    }
    if let Some(curve) = pareto(used.iter().map(|r| r.count)) {
        let (x0, x1, y0) = (48.0f32, w - 24.0, curve_top + curve_h);
        let at = |i: usize, share: f32| (x0 + (x1 - x0) * i as f32 / curve.len() as f32, y0 - curve_h * share);
        s += &format!("<text x=\"16\" y=\"{:.1}\" fill=\"#fff\" font-size=\"14\">Share of play (Pareto)</text>\n", curve_top - 14.0);
        for level in [0.5f32, 0.8, 1.0] {
            let (_, y) = at(0, level);
            s += &format!("<line x1=\"{x0}\" y1=\"{y:.1}\" x2=\"{x1}\" y2=\"{y:.1}\" stroke=\"#fff\" stroke-opacity=\"{}\"/><text x=\"12\" y=\"{:.1}\" fill=\"#aaa\" font-size=\"9\">{:.0}%</text>\n", if level == 0.8 { 0.3 } else { 0.12 }, y + 3.0, level * 100.0);
        }
        let (ex, ey) = at(curve.len(), 1.0);
        s += &format!("<line x1=\"{x0}\" y1=\"{y0}\" x2=\"{ex:.1}\" y2=\"{ey:.1}\" stroke=\"#fff\" stroke-opacity=\"0.15\"/>\n");
        let pts: Vec<String> = std::iter::once(at(0, 0.0)).chain(curve.iter().enumerate().map(|(i, &v)| at(i + 1, v))).map(|(x, y)| format!("{x:.1},{y:.1}")).collect();
        s += &format!("<polyline points=\"{}\" fill=\"none\" stroke=\"#e6be50\" stroke-width=\"2\"/>\n", pts.join(" "));
        let eighty = curve.iter().position(|&v| v >= 0.8).map_or(curve.len(), |i| i + 1);
        s += &format!("<text x=\"{x0}\" y=\"{:.1}\" fill=\"#aaa\">{eighty} of {} titles account for 80% of the launches</text>\n", y0 + 24.0, curve.len());
    }
    s + "</svg>\n"
}

#[cfg(test)]
mod tests {
    use super::{history_svg, pareto};
    use ps2kit::history::PlayHistory;

    #[test]
    fn every_tab_lives_in_exactly_one_workspace() {
        use crate::model::{Tab, Workspace};
        for t in Tab::ALL {
            let homes = Workspace::ALL.iter().filter(|w| w.tabs().contains(&t)).count();
            assert_eq!(homes, 1, "{t:?} must belong to exactly one workspace");
        }
    }

    #[test]
    fn wav_header_and_clipping() {
        let w = super::wav_bytes(&[0.0, 1.0, -2.0, 0.5]);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(&w[36..40], b"data");
        let samples: Vec<i16> = w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(samples, [0, 32767, -32767, 16383]);
        assert_eq!(super::note_name(60), "C4");
        assert_eq!(super::note_name(69), "A4");
    }

    #[test]
    fn pareto_ranks_and_accumulates() {
        assert_eq!(pareto([1u8].into_iter()), None);
        assert_eq!(pareto([0u8, 0].into_iter()), None);
        assert_eq!(pareto([10u8, 30, 60].into_iter()), Some(vec![0.6, 0.9, 1.0]));
    }

    #[test]
    fn svg_is_well_formed_and_escaped() {
        let mut h = PlayHistory::synthetic(&[60, 40, 20, 14, 5, 1], &[], 7);
        h.records[0].name = "A<&>".into();
        let svg = history_svg(&h.records);
        assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains("A&lt;&amp;&gt;") && !svg.contains("A<&>"));
        assert_eq!(svg.matches("<polyline").count(), 1);
        if let Ok(p) = std::env::var("PS2_SVG_OUT") { std::fs::write(p, svg).unwrap() }
    }
}

/// The research instruments, each derived from the user's files: the hook map, the disc probe,
/// the PS1 boot narration, PS1 memory cards and PS3 update snapshots.
fn lab_tab(ui: &mut egui::Ui, m: &mut Model) {
    use ps2kit::hooks::Verdict;
    section(ui, "Hidden hand-overs (PS2)", |ui| {
        let Some(map) = &m.hook_map else { ui.small("Load a PS2 BIOS to see which hidden hand-overs it looks for."); return };
        ui.small("Everything this ROM looks for besides the disc, checked against your card, NVM and disc. Hover a line for its source address.");
        for verdict in [Verdict::Fires, Verdict::Unknown, Verdict::DoesNotFire] {
            let found: Vec<_> = map.findings.iter().filter(|f| f.verdict == verdict).collect();
            if found.is_empty() { continue }
            ui.label(egui::RichText::new(format!("{verdict} ({})", found.len())).strong());
            for f in found {
                ui.small(format!("• {}: {}", f.hook, f.because)).on_hover_text(f.source);
            }
        }
    });
    section(ui, "What the game asks of the console", |ui| {
        let Some(p) = &m.disc_probe else { ui.small("Load a game disc image."); return };
        ui.small(format!("{} files on the disc.", p.files.len()));
        for a in &p.cnf_audit {
            let readers: Vec<String> = a.readers.iter().map(|r| r.to_string()).collect();
            ui.small(format!("SYSTEM.CNF {} = {} — read by {}", a.key, a.value, readers.join(", "))).on_hover_text(a.explanation);
        }
        if let Some(e) = &p.elf_probe {
            let mut libs: Vec<&str> = e.sdk_stamps.iter().map(|s| s.stamp.as_str()).collect();
            libs.dedup();
            ui.small(format!("SDK libraries: {}", libs.join(", ")));
            ui.small(format!("IOP modules it brings: {}", e.referenced_modules.iter().map(|r| r.path.as_str()).collect::<Vec<_>>().join(", ")));
            for g in &e.version_gates { ui.small(format!("Refuses {} older than 0x{:04X}", g.module, g.min_version)); }
            if let Some((mode, off)) = &e.video_mode { ui.small(format!("Video mode hard-coded: {mode:?} (offset 0x{off:X})")); }
            let asks = ["GetOsdConfigParam", "GetMemorySize", "MachineType", "GetOsdConfigParam2"];
            let called: Vec<&str> = e.called_syscalls.iter().filter_map(|s| s.name).collect();
            let asked: Vec<&str> = asks.iter().copied().filter(|a| called.contains(a)).collect();
            ui.small(format!("{} EE syscalls called; asks the console about itself: {}", called.len(), if asked.is_empty() { "nothing (no OSD config, memory size or machine type)".to_string() } else { asked.join(", ") }));
        }
        if let Some(x) = &p.ps1_probe {
            ui.small(format!("PS-X EXE: {}", x.check));
            ui.small(format!("{} BIOS calls; PsyQ stamps: {}", x.bios_calls.len(), x.stamps.iter().map(|s| s.stamp.as_str()).collect::<Vec<_>>().join(", ")));
        }
    });
    if m.ps1_bios.is_some() {
        section(ui, "PS1 boot narration", |ui| {
            ui.small("The POST byte the kernel writes to 0x1F802041 at each stage, and what it would have printed to its (silent) TTY — from this dump's own code and format strings.");
            let post: Vec<String> = m.ps1_narration.0.iter().map(|p| format!("{:X}", p.value)).collect();
            ui.monospace(post.join(" "));
            for p in &m.ps1_narration.0 { ui.small(format!("{:X} @ 0x{:08X}: {}", p.value, p.rom_address, p.meaning)); }
            ui.add_space(4.0);
            for l in &m.ps1_narration.1 { ui.monospace(egui::RichText::new(l).size(10.0)); }
        });
    }
    section(ui, "PS1 memory card", |ui| {
        if ui.button("Open a PS1 card…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("PS1 card", &["mcr", "mcd", "mc", "mem", "gme", "srm", "bin"]).pick_file() {
                m.ps1_card = std::fs::read(&p).ok().and_then(|d| ps2kit::ps1card::Ps1Card::parse(&d).ok()).map(|c| (p, c));
            }
        }
        let Some((path, card)) = &m.ps1_card else { ui.small("Read-only: saves with titles and icons, what the four-shoulder undelete could restore, SoundScope performances."); return };
        ui.small(format!("{} — {} saves{}", path.file_name().unwrap_or_default().to_string_lossy(), card.saves.len(), if card.header_checksum_valid && card.directory_checksums_valid { "" } else { ", checksum problems" }));
        for s in &card.saves { ui.small(format!("• {} — {} ({} block{})", s.title, s.product, s.blocks.len(), if s.blocks.len() == 1 { "" } else { "s" })); }
        for d in card.deleted() { ui.small(format!("↺ deleted: {} — {}", d.save.title, if d.recoverable { "L1+L2+R1+R2 in the card manager would restore it, if it was the last deletion".to_string() } else { d.failure_reason.clone().unwrap_or_default() })); }
        for p in card.soundscope_performances() { ui.small(format!("♪ SoundScope performance: {} events, {} fields, checksum {}", p.recorded_count, p.duration_fields(), if p.checksum_valid { "valid" } else { "invalid" })); }
    });
    section(ui, "PS3 disc or save (plaintext metadata)", |ui| {
        ui.small("A disc folder (PS3_DISC.SFB, PS3_GAME/PARAM.SFO…) or image, or a save folder. Encrypted regions are reported, never read.");
        // The console's firmware is the PS3 update next to the BIOS files, when there is one.
        let firmware = m.ps3_firmware.clone();
        ui.horizontal(|ui| {
            if ui.button("Open a disc folder…").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() { m.ps3_lines = ps3_disc_lines(&p, firmware.as_deref()) }
            }
            if ui.button("Disc image…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("disc image", &["iso", "ISO"]).pick_file() { m.ps3_lines = ps3_disc_lines(&p, firmware.as_deref()) }
            }
            if ui.button("Save folder…").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    m.ps3_lines = match ps2kit::ps3::Ps3Save::open(&p) {
                        Ok(s) => {
                            let mut v: Vec<(String, String)> = s.sfo.entries.iter().map(|e| (e.key.clone(), format!("{:?}", e.value))).collect();
                            v.push(("ICON0.PNG".into(), s.icon_size.map_or("absent".into(), |n| format!("{n} bytes"))));
                            v.push(("data files".into(), s.data_files.iter().map(|f| format!("{} ({} bytes)", f.name, f.size)).collect::<Vec<_>>().join(", ")));
                            v
                        }
                        Err(e) => vec![("error".into(), e.to_string())],
                    };
                }
            }
        });
        ui.small(format!("Console firmware: {}", firmware.as_deref().unwrap_or("unknown (no PS3UPDAT.PUP next to the BIOS files)")));
        for (who, what) in &m.ps3_lines {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(egui::RichText::new(who).color(Color32::from_rgb(115, 153, 255)).size(11.0));
                ui.small(what);
            });
        }
    });
    section(ui, "PS3 update snapshots", |ui| {
        ui.small("A few KB of plaintext structure per update: snapshot one, delete the 200 MB file, compare later.");
        ui.horizontal(|ui| {
            // Snapshots read the update's tables, so they are built on a click, never per frame.
            let snapshot = |m: &Model| m.pup_path.as_ref().and_then(|p| ps2kit::pup::Pup::open(p).ok()).and_then(|pup| ps2kit::pup::PupSnapshot::from_pup(&pup).ok());
            if ui.add_enabled(m.pup_path.is_some(), egui::Button::new("Save snapshot…")).clicked() {
                if let (Some(s), Some(p)) = (snapshot(m), rfd::FileDialog::new().set_file_name("PS3UPDAT.pupsnap").save_file()) { let _ = std::fs::write(p, s.to_text()); }
            }
            if ui.add_enabled(m.pup_path.is_some(), egui::Button::new("Compare with…")).clicked() {
                if let (Some(a), Some(p)) = (snapshot(m), rfd::FileDialog::new().add_filter("PS3 update or snapshot", &["pup", "PUP", "pupsnap"]).pick_file()) {
                    let other = if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pup")) {
                        ps2kit::pup::Pup::open(&p).ok().and_then(|x| ps2kit::pup::PupSnapshot::from_pup(&x).ok())
                    } else { std::fs::read_to_string(&p).ok().and_then(|t| ps2kit::pup::PupSnapshot::parse(&t).ok()) };
                    if let Some(b) = other {
                        let d = a.diff(&b);
                        let mut lines = vec![format!("Compared with {}", p.file_name().unwrap_or_default().to_string_lossy())];
                        if let Some((x, y)) = &d.version_change { lines.push(format!("Version {} → {}", x.as_deref().unwrap_or("?"), y.as_deref().unwrap_or("?"))) }
                        lines.push(format!("Members: {} added, {} removed, {} changed", d.added_members.len(), d.removed_members.len(), d.changed_members.len()));
                        lines.push(format!("Packages: +{:?} −{:?} changed {:?}", d.added_packages, d.removed_packages, d.changed_packages));
                        lines.push(format!("Licence locales: +{:?} −{:?} changed {:?}", d.added_locales, d.removed_locales, d.changed_locales));
                        m.pup_diff = lines;
                    }
                }
            }
        });
        if m.pup_path.is_none() { ui.small("No PS3UPDAT.PUP next to the BIOS files."); }
        for l in &m.pup_diff { ui.small(l); }
    });
}

/// Director mode: pick, shuffle, import or export a keyframed camera script (`director.rs`).
/// Scripts play in the Orbit dials' scene frame and override the path while chosen.
fn director_controls(ui: &mut egui::Ui, m: &mut Model) {
    ui.horizontal(|ui| {
        ui.label("Director");
        let current = m.director.as_ref().map_or("off".to_string(), |s| s.name.clone());
        egui::ComboBox::from_id_salt("director").selected_text(current).show_ui(ui, |ui| {
            if ui.selectable_label(m.director.is_none(), "off").clicked() { m.director = None }
            for s in crate::director::built_in_scripts() {
                let on = m.director.as_ref().is_some_and(|d| d.name == s.name);
                if ui.selectable_label(on, &s.name).clicked() { m.director = Some(s); m.free_camera_enabled = false; m.options.solid_towers = true }
            }
        });
        if ui.small_button("Shuffle").on_hover_text("A seeded montage across the built-in scripts; each click a new seed").clicked() {
            m.director_seed = m.director_seed.wrapping_add(1);
            m.director = Some(crate::director::shuffle(m.director_seed, 150));
            m.free_camera_enabled = false;
            m.options.solid_towers = true;
        }
    });
    ui.horizontal(|ui| {
        if ui.small_button("Import JSON…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("camera script", &["json"]).pick_file() {
                match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| crate::director::CameraScript::from_json(&t)) {
                    Ok(s) => { m.director = Some(s); m.free_camera_enabled = false; m.options.solid_towers = true }
                    Err(e) => eprintln!("camera script {}: {e}", p.display()),
                }
            }
        }
        if ui.add_enabled(m.director.is_some(), egui::Button::new("Export JSON…").small()).clicked() {
            if let (Some(s), Some(p)) = (&m.director, rfd::FileDialog::new().set_file_name("camera-script.json").save_file()) { let _ = std::fs::write(p, s.to_json()); }
        }
    });
}

/// The PS3 hand-off for a disc folder or image, against the console firmware (`ps2kit::ps3`).
fn ps3_disc_lines(path: &std::path::Path, firmware: Option<&str>) -> Vec<(String, String)> {
    match ps2kit::ps3::Ps3Disc::open(path) {
        Ok(d) => d.handoff(firmware).iter().map(|s| (s.source().to_string(), s.to_string())).collect(),
        Err(e) => vec![("error".into(), e.to_string())],
    }
}

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
fn note_name(n: i32) -> String { format!("{}{}", NOTE_NAMES[n.rem_euclid(12) as usize], n.div_euclid(12) - 1) }

/// Stereo 48 kHz float PCM as a 16-bit WAV file.
fn wav_bytes(pcm: &[f32]) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    for v in [16u32] { out.extend_from_slice(&v.to_le_bytes()) }
    for v in [1u16, 2] { out.extend_from_slice(&v.to_le_bytes()) }
    for v in [48_000u32, 48_000 * 4] { out.extend_from_slice(&v.to_le_bytes()) }
    for v in [4u16, 16] { out.extend_from_slice(&v.to_le_bytes()) }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for x in pcm { out.extend_from_slice(&((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()) }
    out
}

/// Every instrument, sample and sound effect in the chosen console's BIOS (`ps2kit::bench`),
/// auditioned through the app's audio. Nothing is bundled: the bench is read from the dump.
fn sound_tab(ui: &mut egui::Ui, m: &mut Model) {
    section(ui, "Instrument bank", |ui| {
        let a = &mut m.asset_inspection;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut a.sound_console, Console::Ps2, "PS2 BIOS banks");
            ui.selectable_value(&mut a.sound_console, Console::Ps1, "PS1 shell VAB");
        });
        if a.sound.as_ref().map(|(c, _)| *c) != Some(a.sound_console) {
            let bench = match a.sound_console {
                Console::Ps2 => m.rom.as_ref().map(ps2kit::bench::SoundBench::ps2).transpose().map_err(|e| e.to_string()),
                Console::Ps1 => Ok(m.ps1_shell.as_ref().map(|s| ps2kit::bench::SoundBench::ps1(&s.bank))),
            };
            match bench {
                Ok(Some(b)) => { a.program = b.programs.first().map_or(0, |p| p.id); a.styles = b.by_style(); a.sound = Some((a.sound_console, b)); a.status.clear() }
                Ok(None) => { a.sound = None; a.status = "Load a BIOS for this console first.".into() }
                Err(e) => { a.sound = None; a.status = e }
            }
        }
        if !a.status.is_empty() { ui.small(&a.status); }
        let Some((_, bench)) = &a.sound else { return };
        ui.small(format!("{} programs, {} samples, {} sound effects — read from your dump.", bench.programs.len(), bench.samples.len(), bench.effects.len()));
    });
    let Some((_, bench)) = &m.asset_inspection.sound else { return };
    let bench_programs: Vec<ps2kit::bench::BenchProgram> = bench.programs.clone();
    section(ui, "Audition", |ui| {
        let a = &mut m.asset_inspection;
        ui.horizontal(|ui| {
            ui.label("Program");
            egui::ComboBox::from_id_salt("program").selected_text(format!("{}", a.program)).show_ui(ui, |ui| {
                for p in &bench_programs { ui.selectable_value(&mut a.program, p.id, format!("{} — {:?}, {} tone(s)", p.id, p.kind, p.tones.len())); }
            });
        });
        ui.add(egui::Slider::new(&mut a.note, 0..=127).text("note").custom_formatter(|v, _| format!("{} ({})", note_name(v as i32), v as i32)));
        ui.add(egui::Slider::new(&mut a.velocity, 1..=127).text("velocity"));
        ui.add(egui::Slider::new(&mut a.seconds, 0.1..=6.0).text("seconds"));
        // Two octaves of keys around the chosen note: click to play.
        let base = (a.note.div_euclid(12) * 12).clamp(0, 104);
        let mut play = None;
        ui.horizontal_wrapped(|ui| {
            for k in base..base + 24 {
                let black = matches!(k.rem_euclid(12), 1 | 3 | 6 | 8 | 10);
                let b = egui::Button::new(egui::RichText::new(note_name(k)).size(9.0).color(if black { Color32::WHITE } else { Color32::BLACK }))
                    .fill(if black { Color32::from_gray(30) } else { Color32::from_gray(225) }).min_size(Vec2::new(26.0, 30.0));
                if ui.add(b).clicked() { play = Some(k) }
            }
        });
        if ui.button("▶ Play").clicked() { play = Some(a.note) }
        if let (Some(k), Some((_, bench))) = (play, &a.sound) {
            a.note = k;
            let pcm = bench.render(a.program, k, a.velocity, a.seconds as f64);
            a.last_label = format!("program {} {} vel {}", a.program, note_name(k), a.velocity);
            m.audio.preview(pcm.clone());
            a.last = pcm;
        }
        if let Some(p) = bench_programs.iter().find(|p| p.id == a.program) {
            for t in &p.tones { ui.small(format!("tone: keys {}–{}, root {}{:+}, sample #{}, volume {}, pan {}", note_name(t.low), note_name(t.high), note_name(t.root), t.fine, t.sample_index, t.volume, t.pan)); }
        }
    });
    section(ui, "By style", |ui| {
        ui.small("Each program measured from one note of your bank: attack, ring, brightness, noise, loop. Click to audition at its root.");
        let a = &mut m.asset_inspection;
        let mut play = None;
        for (style, programs) in &a.styles {
            ui.label(egui::RichText::new(style.name()).strong());
            ui.horizontal_wrapped(|ui| {
                for (id, c) in programs {
                    let tip = format!("attack {:.2} s · rings {:.1} s · {:.0} Hz · noise {:.2}{}", c.attack_ms / 1000.0, c.ring_ms / 1000.0, c.brightness_hz, c.noisiness, if c.looped { " · loops" } else { "" });
                    if ui.small_button(format!("▶ {id}")).on_hover_text(tip).clicked() { play = Some(*id) }
                }
            });
        }
        if let (Some(id), Some((_, bench))) = (play, &a.sound) {
            let note = bench.program(id).and_then(|p| p.tones.first()).map_or(60, |t| t.root.clamp(t.low, t.high));
            a.program = id;
            a.note = note;
            let pcm = bench.render(id, note, a.velocity, a.seconds as f64);
            a.last_label = format!("program {id} {} vel {}", note_name(note), a.velocity);
            m.audio.preview(pcm.clone());
            a.last = pcm;
        }
    });
    section(ui, "Sound effects", |ui| {
        let a = &mut m.asset_inspection;
        let Some((_, bench)) = &a.sound else { return };
        if bench.effects.is_empty() { ui.small("This bank has no one-shot effects."); return }
        let mut play = None;
        ui.horizontal_wrapped(|ui| { for e in &bench.effects { if ui.small_button(format!("▶ {}", e.id)).on_hover_text(format!("sample #{}, pitch 0x{:04X}, volume {:?}", e.sample_index, e.pitch, e.volume)).clicked() { play = Some(e.id) } } });
        if let Some(id) = play {
            let pcm = bench.render_effect(id, 2.0);
            a.last_label = format!("effect {id}");
            m.audio.preview(pcm.clone());
            a.last = pcm;
        }
    });
    section(ui, "Samples", |ui| {
        let Some((_, bench)) = &m.asset_inspection.sound else { return };
        egui::ScrollArea::vertical().id_salt("samples").max_height(140.0).show(ui, |ui| {
            for s in &bench.samples { ui.monospace(format!("#{:<3} {:>7} frames ({:.2} s){}", s.index, s.length, s.length as f32 / 48_000.0, s.loop_start.map(|l| format!(", loops from {l}")).unwrap_or_default())); }
        });
    });
    section(ui, "Last sound", |ui| {
        let a = &m.asset_inspection;
        if a.last.is_empty() { ui.small("Play a note or an effect."); return }
        ui.small(&a.last_label);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 60.0), egui::Sense::hover());
        let painter = ui.painter();
        let frames = a.last.len() / 2;
        let cols = rect.width().max(1.0) as usize;
        let step = (frames / cols).max(1);
        for c in 0..cols.min(frames) {
            let chunk = &a.last[c * step * 2..((c + 1) * step * 2).min(a.last.len())];
            let peak = chunk.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let x = rect.min.x + c as f32;
            painter.line_segment([Pos2::new(x, rect.center().y - peak * 28.0), Pos2::new(x, rect.center().y + peak * 28.0)], Stroke::new(1.0, Color32::from_rgb(115, 153, 255)));
        }
        if ui.button("Export WAV…").clicked() {
            if let Some(p) = rfd::FileDialog::new().set_file_name("bios-instrument.wav").save_file() { let _ = std::fs::write(p, wav_bytes(&a.last)); }
        }
    });
}

/// Every font in the chosen console's BIOS (`ps2kit::bench::FontBench`): pages of 128 glyphs,
/// hover to inspect a glyph, export a page as PNG.
fn fonts_tab(ui: &mut egui::Ui, m: &mut Model) {
    let ctx = ui.ctx().clone();
    let a = &mut m.asset_inspection;
    section(ui, "Fonts", |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut a.font_console, Console::Ps2, "PS2 BIOS");
            ui.selectable_value(&mut a.font_console, Console::Ps1, "PS1 BIOS");
        });
        if a.fonts.as_ref().map(|(c, _)| *c) != Some(a.font_console) {
            let bench = match a.font_console {
                Console::Ps2 => m.rom.as_ref().map(ps2kit::bench::FontBench::ps2),
                Console::Ps1 => m.ps1_bios.as_ref().map(|b| ps2kit::bench::FontBench::ps1(b.data())),
            };
            a.fonts = match bench { Some(Ok(b)) => Some((a.font_console, b)), _ => None };
            a.font = 0;
            a.page = 0;
            a.texture = None;
        }
    });
    let Some((console, bench)) = &a.fonts else { ui.small("Load a BIOS for this console first."); return };
    let fonts = bench.fonts();
    if fonts.is_empty() { ui.small("No fonts found."); return }
    let font_name = fonts[a.font.min(fonts.len() - 1)].name().to_string();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("font").selected_text(&font_name).show_ui(ui, |ui| {
            for (i, f) in fonts.iter().enumerate() { if ui.selectable_value(&mut a.font, i, format!("{} — {} glyphs {}×{}", f.name(), f.glyph_count(), f.dimensions().0, f.dimensions().1)).changed() { a.page = 0 } }
        });
    });
    let font = &fonts[a.font.min(fonts.len() - 1)];
    ui.small(font.summary());
    const PAGE: usize = 128;
    let pages = font.glyph_count().div_ceil(PAGE).max(1);
    if pages > 1 { ui.add(egui::Slider::new(&mut a.page, 0..=pages - 1).text(format!("page (of {pages})"))); }
    let start = a.page.min(pages - 1) * PAGE;
    let end = (start + PAGE).min(font.glyph_count());
    let key = format!("{console:?}/{}/{start}", font.name());
    if a.texture.as_ref().map(|(k, _)| k) != Some(&key) {
        let (w, h, rgba) = font.atlas(start..end);
        a.texture = (w > 0).then(|| (key.clone(), ctx.load_texture(&key, egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba), egui::TextureOptions::NEAREST)));
    }
    let Some((_, tex)) = &a.texture else { return };
    let (gw, gh) = font.dimensions();
    let cols = 16usize.min(end - start).max(1);
    let scale = ((ui.available_width() - 8.0) / tex.size()[0] as f32).clamp(0.5, 4.0);
    let size = Vec2::new(tex.size()[0] as f32, tex.size()[1] as f32) * scale;
    let resp = ui.add(egui::Image::new((tex.id(), size)).sense(egui::Sense::hover()));
    if let Some(pos) = resp.hover_pos() {
        let local = (pos - resp.rect.min) / scale;
        let (col, row) = ((local.x / gw as f32) as usize, (local.y / gh as f32) as usize);
        let index = start + row * cols + col.min(cols - 1);
        if index < end {
            if let Some(g) = font.glyph(index) {
                let big = ctx.load_texture("glyph", egui::ColorImage::from_rgba_unmultiplied([g.width, g.height], &g.rgba), egui::TextureOptions::NEAREST);
                resp.on_hover_ui_at_pointer(|ui| {
                    ui.label(format!("glyph {index} (0x{index:X}) — {}×{}", g.width, g.height));
                    ui.add(egui::Image::new((big.id(), Vec2::new(g.width as f32, g.height as f32) * 6.0)));
                });
            }
        }
    }
    if ui.button("Export this page as PNG…").clicked() {
        let (w, h, rgba) = font.atlas(start..end);
        if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}-page{}.png", font.name(), a.page)).save_file() { let _ = image::save_buffer(p, &rgba, w as u32, h as u32, image::ColorType::Rgba8); }
    }
}

/// The bench a song's program numbers address, read from the loaded BIOS.
fn music_bench(m: &Model, bank: ps2kit::song::BankRef) -> Option<ps2kit::bench::SoundBench> {
    match bank {
        ps2kit::song::BankRef::Ps2 => m.rom.as_ref().and_then(|r| ps2kit::bench::SoundBench::ps2(r).ok()),
        _ => m.ps1_shell.as_ref().map(|s| ps2kit::bench::SoundBench::ps1(&s.bank)),
    }
}

fn set_song(m: &mut Model, song: ps2kit::Result<ps2kit::song::Song>) {
    match song {
        Ok(s) => {
            m.music.bench = music_bench(m, s.bank).map(|b| (s.bank, b));
            m.music.styles = m.music.bench.as_ref().map(|(_, b)| b.by_style()).unwrap_or_default();
            m.music.status = if m.music.bench.is_none() { "No BIOS loaded for this song's bank.".into() } else { s.import_notes.join(" ") };
            m.music.step = (s.ticks_per_beat as u32 / 4).max(1);
            m.music.channel = 0;
            m.music.pattern = 0;
            m.music.song = Some(s);
            name_tracks_by_style(m);
        }
        Err(e) => m.music.status = e.to_string(),
    }
}

/// The short style word for a program ("keys", "drone", "air"…), from the measured styles.
fn style_word(m: &Model, program: u8) -> &'static str {
    m.music.styles.iter().find(|(_, v)| v.iter().any(|(id, _)| *id == program as i32)).map_or("prog", |(s, _)| match s {
        ps2kit::bench::Style::BellsAndChimes => "bell", ps2kit::bench::Style::KeysAndBellPads => "keys", ps2kit::bench::Style::MalletsAndPlucks => "mallet",
        ps2kit::bench::Style::SoftLeadsAndStrings => "lead", ps2kit::bench::Style::BrightPads => "bright", ps2kit::bench::Style::WarmPads => "pad",
        ps2kit::bench::Style::DarkPadsAndDrones => "drone", ps2kit::bench::Style::Swells => "swell", ps2kit::bench::Style::AirAndNoise => "air",
        ps2kit::bench::Style::PercussiveNoise => "perc", _ => "prog",
    })
}

/// Imported songs name their tracks after the MIDI channel; name them after the instrument instead.
fn name_tracks_by_style(m: &mut Model) {
    let names: Vec<String> = m.music.song.as_ref().map(|s| s.channels.iter().map(|c| format!("{} {}", style_word(m, c.program), c.program)).collect()).unwrap_or_default();
    if let Some(song) = &mut m.music.song {
        for (c, n) in song.channels.iter_mut().zip(names) { if c.name.starts_with("MIDI channel") || c.name.starts_with("channel") { c.name = n } }
    }
}

/// Music maker, sidebar: new/import/open/save/export, song settings, channels. The editor itself
/// (arrangement and piano roll) fills the picture area while this tab is open.
fn music_tab(ui: &mut egui::Ui, m: &mut Model) {
    use ps2kit::song::{BankRef, Channel, Pattern, Song};
    section(ui, "Song", |ui| {
        ui.small("Patterns on a bar grid (after Bosca Ceoil and BeepBox), played on your BIOS's own instruments. Song files hold notes and program numbers only.");
        ui.horizontal_wrapped(|ui| {
            for (label, bank) in [("New on PS2 bank", BankRef::Ps2), ("New on PS1 bank", BankRef::Ps1)] {
                if ui.button(label).clicked() {
                    let mut s = Song::new("untitled", bank);
                    let mut c = Channel::new("lead", 0);
                    c.patterns.push(Pattern::new(s.ticks_per_bar()));
                    c.arrangement = vec![Some(0), None, None, None];
                    s.channels.push(c);
                    set_song(m, Ok(s));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(m.rom.is_some(), egui::Button::new("Import PS2 boot music")).clicked() { set_song(m, ps2kit::song::Song::from_ps2_boot(m.rom.as_ref().unwrap())) }
            if ui.add_enabled(m.ps1_shell.is_some(), egui::Button::new("PS1 licence chime")).clicked() { set_song(m, ps2kit::song::Song::from_ps1_licence(m.ps1_shell.as_ref().unwrap(), m.video)) }
            if ui.add_enabled(m.sce_intro.is_some(), egui::Button::new("PS1 SCE intro")).clicked() { set_song(m, ps2kit::song::Song::from_ps1_intro(m.sce_intro.as_ref().unwrap(), m.video)) }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("song", &["psxsong"]).pick_file() {
                    set_song(m, std::fs::read_to_string(&p).map_err(ps2kit::Error::from).and_then(|t| Song::parse(&t)));
                }
            }
            let has = m.music.song.is_some();
            if ui.add_enabled(has, egui::Button::new("Save…")).clicked() {
                if let Some(s) = &m.music.song {
                    match s.to_text() {
                        Ok(t) => if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.psxsong", s.name)).save_file() { let _ = std::fs::write(p, t); },
                        Err(e) => m.music.status = e.to_string(),
                    }
                }
            }
            if ui.add_enabled(has && m.music.bench.is_some(), egui::Button::new("Export WAV…")).clicked() {
                if let (Some(s), Some((_, b))) = (&m.music.song, &m.music.bench) {
                    if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.wav", s.name)).save_file() { if let Err(e) = s.write_wav(b, p) { m.music.status = e.to_string() } }
                }
            }
            if ui.add_enabled(has, egui::Button::new("Export SSsq…")).on_hover_text("The PS2 BIOS's own sequence format").clicked() {
                if let Some(s) = &m.music.song {
                    match s.to_sssq() {
                        Ok(bytes) => if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.sq", s.name)).save_file() { let _ = std::fs::write(p, bytes); },
                        Err(e) => m.music.status = e.to_string(),
                    }
                }
            }
        });
        if !m.music.status.is_empty() { ui.small(&m.music.status); }
    });
}

/// Music maker, picture area — a DAW's usual layout (as in Bitwig and most others): transport
/// across the top, an instrument browser on the left, the arranger in the middle with a playhead,
/// the detail editor (piano roll) below it and an inspector on the right. Space plays/pauses.
fn music_editor(ui: &mut egui::Ui, m: &mut Model) {
    let avail = ui.available_rect_before_wrap();
    ui.painter().rect_filled(avail, 0.0, Color32::from_rgb(18, 20, 28));
    if m.music.song.is_none() {
        ui.painter().text(avail.center(), egui::Align2::CENTER_CENTER, "Start a song in the Music tab: new, or import your BIOS's own boot music.", egui::FontId::proportional(16.0), Color32::GRAY);
        return;
    }
    music_sync_audio(ui, m);
    let transport = Rect::from_min_max(avail.min, Pos2::new(avail.max.x, avail.min.y + 40.0));
    let browser = Rect::from_min_max(Pos2::new(avail.min.x, transport.max.y), Pos2::new(avail.min.x + 190.0, avail.max.y));
    let inspector = Rect::from_min_max(Pos2::new(avail.max.x - 236.0, transport.max.y), avail.max);
    let centre = Rect::from_min_max(Pos2::new(browser.max.x, transport.max.y), Pos2::new(inspector.min.x, avail.max.y));
    let rows = m.music.song.as_ref().map_or(1, |s| s.channels.len().max(1)) as f32;
    let arranger = Rect::from_min_max(centre.min + Vec2::new(6.0, 6.0), Pos2::new(centre.max.x - 6.0, centre.min.y + 30.0 + rows * 26.0));
    let detail = Rect::from_min_max(Pos2::new(centre.min.x + 6.0, arranger.max.y + 8.0), centre.max - Vec2::new(6.0, 6.0));
    for r in [browser, inspector] { ui.painter().rect_filled(r, 0.0, Color32::from_rgb(24, 27, 37)); }
    ui.painter().rect_filled(transport, 0.0, Color32::from_rgb(30, 34, 46));
    music_transport(ui, m, transport);
    music_browser(ui, m, browser.shrink(8.0));
    music_inspector(ui, m, inspector.shrink(8.0));
    music_arranger(ui, m, arranger);
    music_piano_roll(ui, m, detail);
}

/// Keeps the transport's audio in step with the song: re-renders (at most every quarter second)
/// after an edit, applies the loop, and answers Space.
fn music_sync_audio(ui: &mut egui::Ui, m: &mut Model) {
    let Some(song) = &m.music.song else { return };
    let text = song.to_text().unwrap_or_default();
    let hash = { use std::hash::{Hash, Hasher}; let mut h = std::collections::hash_map::DefaultHasher::new(); text.hash(&mut h); h.finish() };
    let now = Instant::now();
    if hash != m.music.rendered_hash && m.music.last_render.is_none_or(|t| now - t > std::time::Duration::from_millis(250)) {
        if let Some((_, bench)) = &m.music.bench { m.audio.music_set(song.render(bench)) }
        m.music.rendered_hash = hash;
        m.music.last_render = Some(now);
    }
    let fpt = music_frames_per_tick(song);
    m.audio.music_loop(m.music.loop_bars.map(|(a, b)| (a as f64 * song.ticks_per_bar() as f64 * fpt, b as f64 * song.ticks_per_bar() as f64 * fpt)));
    let typing = ui.ctx().egui_wants_keyboard_input();
    if !typing && ui.input(|i| i.key_pressed(egui::Key::Space)) { let (_, playing) = m.audio.music_state(); m.audio.music_play(!playing) }
}

/// 48 kHz frames per song tick at the song's tempo.
fn music_frames_per_tick(song: &ps2kit::song::Song) -> f64 { 48_000.0 * 60.0 / (song.bpm.max(1) as f64 * song.ticks_per_beat.max(1) as f64) }

fn music_transport(ui: &mut egui::Ui, m: &mut Model, rect: Rect) {
    let (pos, playing) = m.audio.music_state();
    let Some(song) = &mut m.music.song else { return };
    let fpt = music_frames_per_tick(song);
    let tick = (pos / fpt) as u32;
    let (tpb, bpb) = (song.ticks_per_beat.max(1) as u32, song.beats_per_bar.max(1) as u32);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(10.0, 6.0))), |ui| {
        ui.horizontal_centered(|ui| {
            if ui.button("⏮").on_hover_text("To the start").clicked() { m.audio.music_seek(0.0) }
            if ui.add(egui::Button::new(if playing { "⏸" } else { "▶" }).min_size(Vec2::new(36.0, 24.0))).on_hover_text("Play / pause (Space)").clicked() { m.audio.music_play(!playing) }
            if ui.button("⏹").on_hover_text("Stop and return to the start").clicked() { m.audio.music_play(false); m.audio.music_seek(0.0) }
            ui.add_space(10.0);
            ui.label(egui::RichText::new(format!("{:>3}.{}.{:02}", tick / (tpb * bpb) + 1, tick / tpb % bpb + 1, tick % tpb * 16 / tpb)).monospace().size(18.0).color(Color32::WHITE));
            ui.add_space(10.0);
            ui.add(egui::DragValue::new(&mut song.bpm).range(30..=300).suffix(" BPM"));
            ui.add(egui::DragValue::new(&mut song.beats_per_bar).range(1..=12).suffix("/4"));
            let mut looping = m.music.loop_bars.is_some();
            if ui.toggle_value(&mut looping, "⟲ loop").on_hover_text("Loop the selected bars (set by dragging over the bar ruler)").changed() {
                m.music.loop_bars = looping.then(|| { let b = m.music.selected_bar; (b, b + 1) });
            }
            if let Some((a, b)) = m.music.loop_bars { ui.small(format!("bars {}–{}", a + 1, b)); }
            ui.add_space(10.0);
            ui.small(format!("{} — {:?} bank", song.name, song.bank));
        });
    });
}

/// The instrument browser: every program grouped by measured style, then the menu effects.
/// Click to audition at the instrument's root; + adds it as a new track.
fn music_browser(ui: &mut egui::Ui, m: &mut Model, rect: Rect) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.label(egui::RichText::new("Instruments").strong().color(Color32::WHITE));
        egui::ScrollArea::vertical().id_salt("browser").show(ui, |ui| {
            let mut audition = None;
            let mut add = None;
            for (style, programs) in &m.music.styles {
                ui.label(egui::RichText::new(style.name()).small().color(Color32::from_rgb(140, 160, 210)));
                for (id, c) in programs {
                    ui.horizontal(|ui| {
                        if ui.small_button("+").on_hover_text("Add as a track").clicked() { add = Some((*id, style.name())) }
                        if ui.selectable_label(false, format!("{} {id}", style.name().split([' ', '&']).next().unwrap_or("prog").to_lowercase())).on_hover_text(format!("attack {:.2} s · rings {:.1} s · {:.0} Hz{}", c.attack_ms / 1000.0, c.ring_ms / 1000.0, c.brightness_hz, if c.looped { " · loops" } else { "" })).clicked() { audition = Some(*id) }
                    });
                }
            }
            ui.add_space(6.0);
            ui.small("Drums: a kit derived from your BIOS's samples is being built.");
            if let (Some(id), Some((_, bench))) = (audition, &m.music.bench) {
                let note = bench.program(id).and_then(|p| p.tones.first()).map_or(60, |t| t.root.clamp(t.low, t.high));
                m.audio.preview(bench.render(id, note, 100, 1.5));
            }
            if let (Some((id, style)), Some(song)) = (add, &mut m.music.song) {
                let bars = song.bars().max(4);
                let mut c = ps2kit::song::Channel::new(format!("{} {id}", style.split_whitespace().next().unwrap_or("track").to_lowercase()), id as u8);
                c.patterns.push(ps2kit::song::Pattern::new(song.ticks_per_bar()));
                c.arrangement = vec![None; bars];
                song.channels.push(c);
                m.music.channel = song.channels.len() - 1;
                m.music.pattern = 0;
            }
        });
    });
}

/// The inspector: the selected track and pattern.
fn music_inspector(ui: &mut egui::Ui, m: &mut Model, rect: Rect) {
    let programs: Vec<(i32, String)> = m.music.styles.iter().flat_map(|(s, v)| v.iter().map(move |(id, _)| (*id, format!("{id} — {}", s.name())))).collect();
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        let Some(song) = &mut m.music.song else { return };
        let ticks_per_bar = song.ticks_per_bar();
        let Some(c) = song.channels.get_mut(m.music.channel) else { ui.small("Select a track."); return };
        ui.label(egui::RichText::new("Track").strong().color(Color32::WHITE));
        ui.text_edit_singleline(&mut c.name);
        egui::ComboBox::from_id_salt("insp-prog").width(rect.width() - 8.0).selected_text(format!("program {}", c.program)).show_ui(ui, |ui| {
            for (id, label) in &programs { ui.selectable_value(&mut c.program, *id as u8, label); }
        });
        ui.add(egui::Slider::new(&mut c.volume, 0..=127).text("vol"));
        ui.add(egui::Slider::new(&mut c.pan, 0..=127).text("pan"));
        ui.add_space(8.0);
        ui.label(egui::RichText::new(format!("Pattern {}", m.music.pattern + 1)).strong().color(Color32::WHITE));
        if let Some(p) = c.patterns.get_mut(m.music.pattern) {
            let mut bars = (p.length_ticks / ticks_per_bar.max(1)).max(1);
            if ui.add(egui::Slider::new(&mut bars, 1..=8).text("bars")).changed() { p.length_ticks = bars * ticks_per_bar; p.notes.retain(|n| n.start + n.length <= p.length_ticks) }
            ui.small(format!("{} notes", p.notes.len()));
            if ui.small_button("Clear notes").clicked() { p.notes.clear() }
        }
        if ui.small_button("New pattern").on_hover_text("An empty pattern for this track; place it from the arranger").clicked() {
            c.patterns.push(ps2kit::song::Pattern::new(ticks_per_bar));
            m.music.pattern = c.patterns.len() - 1;
        }
        ui.horizontal_wrapped(|ui| {
            for i in 0..c.patterns.len() { if ui.selectable_label(m.music.pattern == i, format!("{}", i + 1)).clicked() { m.music.pattern = i } }
        });
        ui.add_space(8.0);
        ui.small("Arranger: click a cell to place the selected pattern, right-click to clear; drag across the ruler to set the loop. Piano roll: click to add or remove a note; wheel scrolls the keys.");
    });
}

/// The arranger: a bar ruler (drag to set the loop), one lane per track, pattern cells, playhead.
fn music_arranger(ui: &mut egui::Ui, m: &mut Model, rect: Rect) {
    let (pos, _) = m.audio.music_state();
    let Some(song) = &mut m.music.song else { return };
    let painter = ui.painter_at(rect);
    let font = egui::FontId::monospace(11.0);
    let bars = song.bars().max(4) + 4;
    let label_w = 112.0;
    let cell_w = ((rect.width() - label_w) / bars as f32).clamp(14.0, 64.0);
    let ruler = Rect::from_min_size(Pos2::new(rect.min.x + label_w, rect.min.y), Vec2::new(cell_w * bars as f32, 20.0));
    painter.rect_filled(ruler, 0.0, Color32::from_rgb(36, 40, 54));
    if let Some((a, b)) = m.music.loop_bars { painter.rect_filled(Rect::from_min_max(Pos2::new(ruler.min.x + a as f32 * cell_w, ruler.min.y), Pos2::new(ruler.min.x + b as f32 * cell_w, ruler.max.y)), 0.0, Color32::from_rgba_unmultiplied(230, 190, 80, 90)); }
    for b in 0..bars { painter.text(Pos2::new(ruler.min.x + b as f32 * cell_w + 3.0, ruler.center().y), egui::Align2::LEFT_CENTER, format!("{}", b + 1), font.clone(), Color32::GRAY); }
    let rr = ui.interact(ruler, ui.id().with("ruler"), egui::Sense::click_and_drag());
    if let Some(p) = rr.interact_pointer_pos() {
        let bar = ((p.x - ruler.min.x) / cell_w).floor().max(0.0) as usize;
        if rr.drag_started() || rr.clicked() { m.music.loop_anchor = bar }
        if rr.dragged() || rr.drag_stopped() { let (a, b) = (m.music.loop_anchor.min(bar), m.music.loop_anchor.max(bar) + 1); m.music.loop_bars = Some((a, b)) }
        if rr.clicked() { let t = bar as f64 * song.ticks_per_bar() as f64 * music_frames_per_tick(song); m.audio.music_seek(t) }
    }
    let lane_h = 24.0;
    for (ci, c) in song.channels.iter().enumerate() {
        let y = ruler.max.y + 4.0 + ci as f32 * (lane_h + 2.0);
        let sel = ci == m.music.channel;
        painter.rect_filled(Rect::from_min_size(Pos2::new(rect.min.x, y), Vec2::new(label_w - 4.0, lane_h)), 2.0, if sel { Color32::from_rgb(55, 70, 110) } else { Color32::from_rgb(32, 36, 48) });
        let name: String = if c.name.chars().count() > 14 { c.name.chars().take(13).chain(['…']).collect() } else { c.name.clone() };
        painter.text(Pos2::new(rect.min.x + 6.0, y + lane_h / 2.0), egui::Align2::LEFT_CENTER, name, font.clone(), Color32::WHITE);
        for b in 0..bars {
            let r = Rect::from_min_size(Pos2::new(ruler.min.x + b as f32 * cell_w, y), Vec2::new(cell_w - 2.0, lane_h));
            let p = c.arrangement.get(b).copied().flatten();
            painter.rect_filled(r, 2.0, match p { Some(_) => Color32::from_rgb(70, 100, 190), None => Color32::from_white_alpha(10) });
            if let Some(p) = p { painter.text(r.center(), egui::Align2::CENTER_CENTER, format!("{}", p + 1), font.clone(), Color32::WHITE); }
        }
    }
    let lanes = Rect::from_min_size(Pos2::new(rect.min.x, ruler.max.y + 4.0), Vec2::new(label_w + cell_w * bars as f32, song.channels.len() as f32 * (lane_h + 2.0)));
    let lr = ui.interact(lanes, ui.id().with("lanes"), egui::Sense::click());
    if let Some(p) = lr.interact_pointer_pos() {
        let ci = ((p.y - lanes.min.y) / (lane_h + 2.0)) as usize;
        let tpb = song.ticks_per_bar();
        if let Some(c) = song.channels.get_mut(ci) {
            if lr.clicked() || lr.secondary_clicked() { if m.music.channel != ci { m.music.pattern = 0 } m.music.channel = ci }
            if p.x >= ruler.min.x {
                let b = ((p.x - ruler.min.x) / cell_w) as usize;
                if c.arrangement.len() <= b { c.arrangement.resize(b + 1, None) }
                if lr.secondary_clicked() { c.arrangement[b] = None }
                else if lr.clicked() {
                    if c.patterns.is_empty() { c.patterns.push(ps2kit::song::Pattern::new(tpb)) }
                    let pat = c.arrangement[b].unwrap_or(m.music.pattern.min(c.patterns.len() - 1));
                    c.arrangement[b] = Some(pat);
                    m.music.pattern = pat;
                    m.music.selected_bar = b;
                }
            }
        }
    }
    // Playhead.
    let x = ruler.min.x + (pos / music_frames_per_tick(song) / song.ticks_per_bar().max(1) as f64) as f32 * cell_w;
    if x <= rect.max.x { painter.line_segment([Pos2::new(x, ruler.min.y), Pos2::new(x, lanes.max.y)], Stroke::new(2.0, Color32::from_rgb(255, 120, 90))); }
}

/// The detail editor: a piano roll for the selected track's selected pattern.
fn music_piano_roll(ui: &mut egui::Ui, m: &mut Model, rect: Rect) {
    let Some(song) = &mut m.music.song else { return };
    let tpb = song.ticks_per_beat as u32;
    let Some(c) = song.channels.get_mut(m.music.channel) else { return };
    let program = c.program;
    let Some(pattern) = c.patterns.get_mut(m.music.pattern) else { return };
    let keys = 36u8;
    // A newly shown pattern scrolls the roll so its notes are in view.
    if m.music.roll_for != (m.music.channel, m.music.pattern) {
        m.music.roll_for = (m.music.channel, m.music.pattern);
        if let (Some(lo), Some(hi)) = (pattern.notes.iter().map(|n| n.key).min(), pattern.notes.iter().map(|n| n.key).max()) {
            let span = hi.saturating_sub(lo);
            m.music.low_key = if span < keys { lo.saturating_sub((keys - span) / 2) } else { lo }.min(127 - keys);
        }
    }
    let painter = ui.painter_at(rect);
    let font = egui::FontId::monospace(10.0);
    painter.text(rect.min, egui::Align2::LEFT_TOP, format!("{} · pattern {} · program {}", c.name, m.music.pattern + 1, program), egui::FontId::monospace(11.0), Color32::LIGHT_GRAY);
    let roll = Rect::from_min_max(Pos2::new(rect.min.x + 44.0, rect.min.y + 18.0), rect.max);
    let row_h = roll.height() / keys as f32;
    let step = m.music.step.max(1);
    let steps = pattern.length_ticks.div_ceil(step).max(1);
    let col_w = roll.width() / steps as f32;
    for k in 0..keys {
        let key = m.music.low_key.saturating_add(keys - 1 - k);
        let y = roll.min.y + k as f32 * row_h;
        let black = matches!(key % 12, 1 | 3 | 6 | 8 | 10);
        painter.rect_filled(Rect::from_min_size(Pos2::new(roll.min.x, y), Vec2::new(roll.width(), row_h - 1.0)), 0.0, if black { Color32::from_white_alpha(6) } else { Color32::from_white_alpha(12) });
        painter.rect_filled(Rect::from_min_size(Pos2::new(rect.min.x, y), Vec2::new(40.0, row_h - 1.0)), 0.0, if black { Color32::from_gray(40) } else { Color32::from_gray(200) });
        if key % 12 == 0 { painter.text(Pos2::new(rect.min.x + 2.0, y + row_h / 2.0), egui::Align2::LEFT_CENTER, note_name(key as i32), font.clone(), Color32::BLACK); }
    }
    let beat_steps = (tpb / step).max(1);
    for s in 0..=steps { let x = roll.min.x + s as f32 * col_w; painter.line_segment([Pos2::new(x, roll.min.y), Pos2::new(x, roll.max.y)], Stroke::new(1.0, Color32::from_white_alpha(if s % (beat_steps * 4) == 0 { 70 } else if s % beat_steps == 0 { 36 } else { 12 }))); }
    for n in &pattern.notes {
        if n.key < m.music.low_key || n.key >= m.music.low_key.saturating_add(keys) { continue }
        let row = (m.music.low_key + keys - 1 - n.key) as f32;
        let r = Rect::from_min_size(Pos2::new(roll.min.x + n.start as f32 / step as f32 * col_w, roll.min.y + row * row_h), Vec2::new((n.length as f32 / step as f32 * col_w - 1.0).max(3.0), row_h - 1.0));
        painter.rect_filled(r, 2.0, Color32::from_rgb(230, 190, 80));
    }
    let rr = ui.interact(roll, ui.id().with("roll"), egui::Sense::click());
    if rr.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 1.0 { m.music.low_key = (m.music.low_key as i32 + if scroll > 0.0 { 1 } else { -1 }).clamp(0, 127 - keys as i32) as u8 }
    }
    // The piano keys audition.
    let kr = ui.interact(Rect::from_min_max(Pos2::new(rect.min.x, roll.min.y), Pos2::new(roll.min.x, roll.max.y)), ui.id().with("keys"), egui::Sense::click());
    if let (true, Some(p), Some((_, b))) = (kr.clicked(), kr.interact_pointer_pos(), &m.music.bench) {
        let key = m.music.low_key + keys - 1 - ((p.y - roll.min.y) / row_h) as u8;
        m.audio.preview(b.render(program as i32, key as i32, 100, 0.8));
    }
    if let (true, Some(pos)) = (rr.clicked(), rr.interact_pointer_pos()) {
        let s = ((pos.x - roll.min.x) / col_w) as u32;
        let key = m.music.low_key + keys - 1 - ((pos.y - roll.min.y) / row_h) as u8;
        let start = s * step;
        if let Some(i) = pattern.notes.iter().position(|n| n.key == key && start >= n.start && start < n.start + n.length) { pattern.notes.remove(i); }
        else if start < pattern.length_ticks {
            pattern.notes.push(ps2kit::song::Note::new(start, step.min(pattern.length_ticks - start), key, 100));
            if let Some((_, b)) = &m.music.bench { m.audio.preview(b.render(program as i32, key as i32, 100, 0.4)); }
        }
    }
}

