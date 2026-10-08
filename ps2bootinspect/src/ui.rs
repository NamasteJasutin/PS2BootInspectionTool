//! The egui application: sidebar, the picture with camera input, visualiser, hand-off card.

use crate::audio::{VisualizerMode, BAND_COUNT};
use crate::model::{HistorySource, Model, LANGUAGES};
use crate::renderer::Renderer;
use crate::{arc_device, SceneView};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use glam::Vec3;
use ps2kit::sim::{SceneKind, Segment, VideoMode};
use std::time::Instant;

pub struct App {
    model: Model,
    renderer: Renderer,
    view: SceneView,
    last: Instant,
    loaded_assets: u32,
    loaded_logo: u32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.as_ref().expect("wgpu");
        let (device, queue) = arc_device(rs);
        let renderer = Renderer::new(device, queue);
        let view = SceneView::register(rs, &renderer.targets["scene"].1, renderer.size);
        let mut model = Model::new();
        model.load_defaults();
        Self { model, renderer, view, last: Instant::now(), loaded_assets: u32::MAX, loaded_logo: u32::MAX }
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
            self.loaded_logo = m.logo_version;
        }
        let Some(assets) = &m.assets else { return };
        let mut enc = self.renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("scene") });
        self.renderer.begin_frame();
        let free = m.free_camera_enabled.then(|| m.free_camera.view(m.video));
        let warning_tex = format!("TEXOPNG{}", m.language);
        match m.scene_kind {
            SceneKind::Full => {
                let (span, local) = m.sequence.span_at(m.frame.max(0.0) as usize);
                match span.segment {
                    Segment::Opening => self.renderer.render_opening(&mut enc, local as f32 + m.frame.fract(), &m.scene, assets, &m.sequence.opening, free, &m.options),
                    Segment::Logo => { if let Some(anim) = m.logo_animation() { self.renderer.render_logo(&mut enc, local as f32, &anim, &m.options) } }
                    _ => { self.renderer.logo_cached_field = None; self.renderer.pass(&mut enc, "scene", true, crate::renderer::DepthAction::None, |_, _| {}) }
                }
            }
            SceneKind::Boot => self.renderer.render_opening(&mut enc, m.frame, &m.scene, assets, &m.timeline, free, &m.options),
            SceneKind::Warning => self.renderer.render_warning(&mut enc, m.frame, assets, &m.timeline, free, &m.options, &warning_tex),
            SceneKind::Logo => { if let Some(anim) = m.logo_animation() { if m.frame >= 0.0 { self.renderer.render_logo(&mut enc, m.frame, &anim, &m.options) } else { self.renderer.pass(&mut enc, "scene", true, crate::renderer::DepthAction::None, |_, _| {}) } } }
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

    // Status line and boot-phase readout.
    let c = m.camera();
    let fps = m.timeline.fps();
    let status = if m.scene_kind == SceneKind::Full {
        let (span, local) = m.sequence.span_at(m.frame.max(0.0) as usize);
        let name = match span.segment { Segment::PowerOn => "power-on", Segment::Opening => "ONE: BIOS opening", Segment::Handoff => "hand-off", Segment::Logo => "TWO: disc logo", Segment::End => "end" };
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
        painter.text(avail.min + Vec2::new(10.0, 26.0), egui::Align2::LEFT_TOP, format!("booting: {}", phase.name), mono.clone(), Color32::from_rgb(230, 210, 80));
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
            painter.text(Pos2::new(card.min.x + 236.0, y), egui::Align2::RIGHT_TOP, &s.who, egui::FontId::monospace(11.0), Color32::from_white_alpha(230));
            let galley = painter.layout(s.what.clone(), egui::FontId::monospace(11.0), Color32::WHITE, card.width() - 270.0);
            let h = galley.size().y;
            painter.galley(Pos2::new(card.min.x + 246.0, y), galley, Color32::WHITE);
            y += h + 6.0;
            if y > card.max.y - 20.0 { break }
        }
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
    section(ui, "Your files", |ui| {
        if let Some(p) = file_row(ui, "BIOS", &m.bios_status.clone(), false, &["bin", "BIN", "rom"]) { m.load_bios(&p, false) }
        if let Some(p) = file_row(ui, "Memory card", &m.card_status.clone(), true, &["ps2"]) { m.load_card(&p, false) }
        if let Some(p) = file_row(ui, "Game disc image", &m.disc_status.clone(), false, &["iso", "bin", "cue", "img"]) { m.load_disc(&p, false) }
    });
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
        let used: Vec<_> = m.history.records.iter().filter(|r| !r.is_empty()).cloned().collect();
        if used.is_empty() { ui.small("No titles: the screen shows no towers."); }
        for r in used {
            let towers: String = (0..6).map(|k| if r.mask >> k & 1 == 1 { if k == r.index { '▫' } else { '▪' } } else { '·' }).collect();
            ui.monospace(format!("{:<12} {:>3}× {towers}", r.name, r.count));
        }
    });
    section(ui, "Scene", |ui| {
        let before = m.scene_kind;
        let mut kind = m.scene_kind;
        egui::ComboBox::from_id_salt("scene").selected_text(kind.name()).show_ui(ui, |ui| {
            for s in SceneKind::ALL { ui.selectable_value(&mut kind, s, s.name()); }
        });
        if kind != before { m.set_scene(kind) }
        let mut rebuild = false;
        match m.scene_kind {
            SceneKind::Full => {
                ui.small("Phase ONE (BIOS): power-on, the opening. Phase TWO (disc): hand-off to rom0:PS2LOGO, the logo, then the point where the game's ELF would start. The console is never asked to run the game.");
                rebuild |= ui.add(egui::Slider::new(&mut m.handoff_seconds, 0.2..=4.0).text("hand-off to PS2LOGO (s)")).changed();
            }
            SceneKind::Logo => { ui.small("What a licensed disc shows before its game starts: the lettering comes from the disc's first 12 sectors, the animation from rom0:PS2LOGO."); }
            SceneKind::Warning => {
                egui::ComboBox::from_id_salt("lang").selected_text(LANGUAGES.iter().find(|l| l.0 == m.language).map(|l| l.1).unwrap_or("")).show_ui(ui, |ui| {
                    for (code, name) in LANGUAGES { ui.selectable_value(&mut m.language, code, name); }
                });
                rebuild |= ui.add(egui::Slider::new(&mut m.warning_exit_seconds, 3.0..=60.0).text("drive reports a change after (s)")).changed();
            }
            SceneKind::Boot => {}
        }
        ui.horizontal(|ui| {
            for v in [VideoMode::Ntsc, VideoMode::Pal] {
                if ui.selectable_label(m.video == v, v.name()).clicked() && m.video != v { m.video = v; rebuild = true }
            }
        });
        if rebuild { m.rebuild_timeline() }
    });
    section(ui, "Time", |ui| {
        ui.horizontal(|ui| {
            if ui.button(if m.playing { "Pause" } else { "Play" }).clicked() { m.playing = !m.playing }
            if ui.button("Restart").clicked() { m.frame = m.start_frame(); m.playing = true }
            ui.checkbox(&mut m.looping, "Loop");
        });
        let (start, end) = (m.start_frame(), m.end_frame().max(1.0));
        ui.add(egui::Slider::new(&mut m.frame, start..=end).show_value(false));
        ui.add(egui::Slider::new(&mut m.speed, 0.05..=2.0).text("speed").logarithmic(true));
        let mut rebuild = false;
        rebuild |= ui.add(egui::Slider::new(&mut m.power_on_seconds, 0.0..=6.0).text("power-on black (s)")).changed();
        if matches!(m.scene_kind, SceneKind::Boot | SceneKind::Full) {
            rebuild |= ui.add(egui::Slider::new(&mut m.disc_seconds, 0.0..=10.5).text("disc identified after (s)")).changed();
        }
        if rebuild { m.rebuild_timeline() }
    });
    section(ui, "Camera", |ui| {
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
    section(ui, "Disc", |ui| {
        if let Some(d) = &m.disc {
            ui.small(format!("Volume: {} ({} sectors, {} MiB, {})", d.volume_id, d.sector_count, d.byte_size() >> 20, if d.is_dvd() { "DVD" } else { "CD" }));
            ui.small(format!("Title ID: {}   state 0x{:02X}", d.title_id().unwrap_or_else(|| "—".into()), d.disc_state_code()));
            ui.monospace(d.system_cnf_text.trim());
            if let Some(b) = &d.boot_elf {
                ui.small(format!("Boot ELF: {} — LBA {}, {} bytes, entry 0x{:08X}, {} segment(s)", b.file_name, b.lba, b.size, b.entry, b.segments.len()));
            }
            ui.small(d.logo_region.map(|r| format!("Logo sectors: {} master (checked by J/H and E consoles only)", if r == "J" { "J/A" } else { r })).unwrap_or_else(|| "Logo sectors: match neither the E nor the J/A master".into()));
            ui.collapsing("What the console would do next", |ui| {
                for s in m.handoff_steps() { ui.small(format!("{}  {}", s.who, s.what)); }
            });
        } else {
            ui.small("Open a game disc image (.iso) to see what the console would load.");
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
        if m.scene_kind == SceneKind::Logo {
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
        }
    });
    ui.add_space(12.0);
}
