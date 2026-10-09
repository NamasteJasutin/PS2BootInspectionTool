//! The egui application: sidebar, the picture with camera input, visualiser, hand-off card.

use crate::audio::{VisualizerMode, BAND_COUNT};
use crate::model::{outcome_name, video_mode_name, CameraMode, DiscOverride, HistorySource, Model, Scene, Tab, LANGUAGES, REGIONS};
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
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.as_ref().expect("wgpu");
        let (device, queue) = arc_device(rs);
        let renderer = Renderer::new(device, queue);
        let view = SceneView::register(rs, &renderer.targets["scene"].1, renderer.size);
        let mut model = Model::new();
        model.load_defaults();
        Self { model, renderer, view, last: Instant::now(), loaded_assets: u32::MAX, loaded_logo: u32::MAX, ps1_layout: None }
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
            self.loaded_logo = m.logo_version;
        }
        let Some(assets) = &m.assets else { return };
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
                    Segment::Opening => self.renderer.render_opening(&mut enc, local as f32 + m.frame.fract(), &m.scene, assets, &m.sequence.opening, free, &m.options),
                    Segment::Logo => if let (Some(shell), Some(logo), Some(layout)) = (m.ps1_shell.as_ref().filter(|_| m.ps1_active()), m.ps1_logo_model(), &self.ps1_layout) {
                        self.renderer.render_ps1_licence(&mut enc, local as f32, shell, logo, layout, m.video, &m.options)
                    } else if let Some(anim) = m.logo_animation() { self.renderer.render_logo(&mut enc, local as f32, &anim, &m.options) }
                    Segment::Warning => self.renderer.render_warning(&mut enc, local as f32, assets, &m.sequence.warning, free, &m.options, &warning_tex),
                    _ => { self.renderer.logo_cached_field = None; self.renderer.pass(&mut enc, "scene", true, crate::renderer::DepthAction::None, |_, _| {}) }
                }
            }
            Scene::Boot => self.renderer.render_opening(&mut enc, m.frame, &m.scene, assets, &m.timeline, free, &m.options),
            Scene::Warning => self.renderer.render_warning(&mut enc, m.frame, assets, &m.timeline, free, &m.options, &warning_tex),
            Scene::Logo => if let (Some(shell), Some(logo), Some(layout)) = (m.ps1_shell.as_ref().filter(|_| m.ps1_active()), m.ps1_logo_model(), &self.ps1_layout) {
                self.renderer.render_ps1_licence(&mut enc, m.frame, shell, logo, layout, m.video, &m.options)
            } else if let Some(anim) = m.logo_animation() { if m.frame >= 0.0 { self.renderer.render_logo(&mut enc, m.frame, &anim, &m.options) } else { self.renderer.pass(&mut enc, "scene", true, crate::renderer::DepthAction::None, |_, _| {}) } }
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
        self.model.tick(dt);
        self.model.sync_audio();
        let t0 = Instant::now();
        if let Some(rs) = frame.wgpu_render_state() { self.render_frame(rs) }
        self.model.cpu_ms = t0.elapsed().as_secs_f64() * 1000.0;

        egui::Panel::right("sidebar").exact_size(330.0).show(root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| sidebar(ui, &mut self.model));
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(root, |ui| {
            picture(ui, &mut self.model, &self.view);
        });
        ctx.request_repaint();
    }
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
    if m.assets.is_none() {
        painter.text(avail.center(), egui::Align2::CENTER_CENTER, "Open your PS2 BIOS dump to begin.\nNothing from the BIOS is bundled with this app.", egui::FontId::proportional(16.0), Color32::GRAY);
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
        for s in m.handoff_steps() {
            painter.text(Pos2::new(card.min.x + 236.0, y), egui::Align2::RIGHT_TOP, s.who(), egui::FontId::monospace(11.0), Color32::from_white_alpha(230));
            let galley = painter.layout(s.to_string(), egui::FontId::monospace(11.0), Color32::WHITE, card.width() - 270.0);
            let h = galley.size().y;
            painter.galley(Pos2::new(card.min.x + 246.0, y), galley, Color32::WHITE);
            y += h + 6.0;
            if y > card.max.y - 20.0 { break }
        }
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
    ui.horizontal(|ui| {
        for t in Tab::ALL { ui.selectable_value(&mut m.tab, t, t.name()); }
    });
    ui.separator();
    match m.tab {
        Tab::Boot => boot_tab(ui, m),
        Tab::SaveData => save_data_tab(ui, m),
        Tab::Disc => disc_tab(ui, m),
        Tab::Bios => bios_tab(ui, m),
    }
    ui.add_space(12.0);
}

fn boot_tab(ui: &mut egui::Ui, m: &mut Model) {
    section(ui, "Your files", |ui| {
        if let Some(p) = file_row(ui, "BIOS", &m.bios_status.clone(), false, &["bin", "BIN", "rom"]) { m.load_bios(&p, false) }
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
        if m.camera_mode != CameraMode::Scripted { ui.add(egui::Slider::new(&mut m.camera_period, 2.0..=60.0).text("seconds per lap").logarithmic(true)); }
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
                        Some(ps2kit::disc::Ps1Verdict::NotChecked) => "This console (A) does not check the licence or the logo.",
                        Some(ps2kit::disc::Ps1Verdict::Accepted) => "This console's PS1 shell accepts the licence line and the logo.",
                        Some(ps2kit::disc::Ps1Verdict::TextMismatch) => "This console's PS1 shell would not accept the licence line (black screen, endless re-read). Shown anyway.",
                        Some(ps2kit::disc::Ps1Verdict::LogoMismatch) => "The logo differs from the shell's copy: the console would hang. Shown anyway.",
                        _ => "Licence check: unknown.",
                    });
                }
                _ => { ui.small("No SYSTEM.CNF and no licence sector."); }
            }
        } else {
            ui.small("Open a game disc image (.iso or .cue) to see what the console would load.");
        }
    });
    section(ui, "What the console would do next", |ui| {
        ui.small(format!("Under the scenario: {}.", outcome_name(m.outcome())));
        for s in m.handoff_steps() {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(egui::RichText::new(s.who()).color(Color32::from_rgb(115, 153, 255)).size(11.0));
                ui.small(s.to_string());
            });
        }
    });
}

fn bios_tab(ui: &mut egui::Ui, m: &mut Model) {
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
