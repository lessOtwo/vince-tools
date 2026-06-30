use std::{
    io::{self, BufRead},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use eframe::{
    App, CreationContext,
    egui::{
        self, Align2, Color32, FontId, Rect, Stroke, Vec2, ViewportBuilder, ViewportCommand, pos2,
        vec2,
    },
};

use crate::piano_overlay_protocol::{
    OverlayCommand, OverlayFrame, OverlayKeyFrame, OverlayKeyKind,
};

const OVERLAY_TITLE: &str = "Vince Tools - Piano Overlay";
const HIDDEN_SIZE: f32 = 1.0;
const REPAINT_INTERVAL: Duration = Duration::from_millis(16);

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title(OVERLAY_TITLE)
            .with_inner_size(vec2(HIDDEN_SIZE, HIDDEN_SIZE))
            .with_min_inner_size(vec2(HIDDEN_SIZE, HIDDEN_SIZE))
            .with_max_inner_size(vec2(HIDDEN_SIZE, HIDDEN_SIZE))
            .with_position(pos2(0.0, 0.0))
            .with_decorations(false)
            .with_transparent(true)
            .with_visible(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_taskbar(false),
        renderer: eframe::Renderer::Wgpu,
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        OVERLAY_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(PianoOverlayApp::new(cc)))),
    )
}

struct PianoOverlayApp {
    rx: Receiver<OverlayCommand>,
    frame: Option<OverlayFrame>,
    visible: bool,
    close_requested: bool,
    last_geometry: Option<OverlayGeometry>,
}

impl PianoOverlayApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        Self {
            rx: spawn_command_reader(cc.egui_ctx.clone()),
            frame: None,
            visible: false,
            close_requested: false,
            last_geometry: None,
        }
    }

    fn poll_commands(&mut self, ctx: &egui::Context) {
        while let Ok(command) = self.rx.try_recv() {
            match command {
                OverlayCommand::Frame(frame) => {
                    self.frame = Some(frame);
                    ctx.request_repaint();
                }
                OverlayCommand::Hide => {
                    self.frame = None;
                    hide_window(ctx, &mut self.visible, &mut self.last_geometry);
                }
                OverlayCommand::Shutdown => {
                    self.close_requested = true;
                    ctx.request_repaint();
                }
            }
        }
    }

    fn sync_window(&mut self, ctx: &egui::Context, frame: &OverlayFrame) {
        let geometry = OverlayGeometry {
            x: frame.x,
            y: frame.y,
            width: frame.width.max(1),
            height: frame.height.max(1),
        };

        if self.last_geometry != Some(geometry) {
            let size = vec2(geometry.width as f32, geometry.height as f32);
            ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(size));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(
                geometry.x as f32,
                geometry.y as f32,
            )));
            self.last_geometry = Some(geometry);
        }

        if !self.visible {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            self.visible = true;
        }
    }
}

impl App for PianoOverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_commands(ctx);

        if self.close_requested {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        let Some(frame) = self.frame.clone() else {
            hide_window(ctx, &mut self.visible, &mut self.last_geometry);
            ctx.request_repaint_after(Duration::from_millis(80));
            return;
        };

        if frame.width <= 0 || frame.height <= 0 || frame.opacity <= 0.001 {
            hide_window(ctx, &mut self.visible, &mut self.last_geometry);
            ctx.request_repaint_after(Duration::from_millis(80));
            return;
        }

        self.sync_window(ctx, &frame);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let painter = ui.painter_at(ui.max_rect());
                paint_overlay(&painter, &frame);
            });

        if frame.keys.iter().any(|key| key.active > 0.0) {
            ctx.request_repaint_after(REPAINT_INTERVAL);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::TRANSPARENT.to_normalized_gamma_f32()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct OverlayGeometry {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn spawn_command_reader(ctx: egui::Context) -> Receiver<OverlayCommand> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<OverlayCommand>(&line) {
                Ok(command) => {
                    let should_stop = matches!(command, OverlayCommand::Shutdown);
                    if tx.send(command).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                    if should_stop {
                        return;
                    }
                }
                Err(err) => {
                    eprintln!("piano overlay command ignored: {err}");
                }
            }
        }

        let _ = tx.send(OverlayCommand::Shutdown);
        ctx.request_repaint();
    });
    rx
}

fn hide_window(
    ctx: &egui::Context,
    visible: &mut bool,
    last_geometry: &mut Option<OverlayGeometry>,
) {
    if *visible {
        let size = vec2(HIDDEN_SIZE, HIDDEN_SIZE);
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(0.0, 0.0)));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(size));
        *visible = false;
    }
    *last_geometry = None;
}

fn paint_overlay(painter: &egui::Painter, frame: &OverlayFrame) {
    let opacity = frame.opacity.clamp(0.0, 1.0);

    for key in &frame.keys {
        draw_key(painter, key, opacity);
    }

    for key in &frame.keys {
        draw_particles(painter, key, opacity, frame.time);
    }
}

fn draw_key(painter: &egui::Painter, key: &OverlayKeyFrame, opacity: f32) {
    let rect = Rect::from_min_size(
        pos2(key.x, key.y + key.active * 7.0),
        vec2(key.width, key.height),
    );
    let accent = accent_color(key.kind);

    if key.active > 0.0 {
        painter.rect_filled(
            rect.expand(10.0 + key.active * 12.0),
            12.0,
            with_alpha(accent, 120.0 * key.active * opacity),
        );
        painter.rect_stroke(
            rect.expand(4.0 + key.active * 4.0),
            10.0,
            Stroke::new(2.0, with_alpha(accent, 180.0 * key.active * opacity)),
            egui::StrokeKind::Inside,
        );
    }

    painter.rect_filled(
        rect.translate(vec2(2.0, 5.0)).expand2(vec2(0.0, 3.0)),
        8.0,
        with_alpha(Color32::BLACK, 90.0 * opacity),
    );

    let (fill, text_color, stroke_color) = key_palette(key.kind, key.active, opacity);
    painter.rect(
        rect,
        8.0,
        fill,
        Stroke::new(1.0, stroke_color),
        egui::StrokeKind::Inside,
    );

    let label = display_label(&key.label);
    let font_size = if label.len() > 1 { 12.0 } else { 18.0 };
    let label_y = match key.kind {
        OverlayKeyKind::White => rect.bottom() - 19.0,
        OverlayKeyKind::Black => rect.bottom() - 15.0,
    };
    painter.text(
        pos2(rect.center().x, label_y),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(font_size),
        text_color,
    );
}

fn draw_particles(painter: &egui::Painter, key: &OverlayKeyFrame, opacity: f32, time: f64) {
    if key.active <= 0.0 {
        return;
    }

    let rect = Rect::from_min_size(
        pos2(key.x, key.y + key.active * 7.0),
        vec2(key.width, key.height),
    );
    let center = match key.kind {
        OverlayKeyKind::White => pos2(rect.center().x, rect.bottom() - 34.0),
        OverlayKeyKind::Black => rect.center(),
    };
    let accent = accent_color(key.kind);
    let seed = label_seed(&key.label);
    let life = key.active.clamp(0.0, 1.0);
    let expansion = 1.0 - life;

    for index in 0..16 {
        let base = index as f32 + seed;
        let angle = base * 2.399_963 + time as f32 * 0.9;
        let radius = 6.0 + expansion * (18.0 + base.sin().abs() * 16.0);
        let offset = Vec2::angled(angle) * radius;
        let particle_center = center + offset;
        let particle_radius = (1.8 + (base * 1.7).sin().abs() * 2.8) * life;
        let alpha = 235.0 * life * opacity.sqrt() * (0.55 + 0.45 * base.cos().abs());

        painter.circle_filled(particle_center, particle_radius, with_alpha(accent, alpha));
        painter.circle_filled(
            particle_center,
            particle_radius * 0.36,
            with_alpha(Color32::WHITE, alpha * 0.55),
        );
    }
}

fn key_palette(kind: OverlayKeyKind, active: f32, opacity: f32) -> (Color32, Color32, Color32) {
    let accent = accent_color(kind);
    match kind {
        OverlayKeyKind::White => (
            with_opacity(mix_color(Color32::WHITE, accent, active * 0.18), opacity),
            with_opacity(Color32::from_rgb(15, 23, 42), opacity),
            with_alpha(Color32::WHITE, 180.0 * opacity),
        ),
        OverlayKeyKind::Black => (
            with_opacity(
                mix_color(Color32::from_rgb(17, 24, 39), accent, active * 0.55),
                opacity,
            ),
            with_opacity(Color32::from_rgb(226, 232, 240), opacity),
            with_alpha(Color32::WHITE, 44.0 * opacity),
        ),
    }
}

fn accent_color(kind: OverlayKeyKind) -> Color32 {
    match kind {
        OverlayKeyKind::White => Color32::from_rgb(252, 211, 77),
        OverlayKeyKind::Black => Color32::from_rgb(129, 140, 248),
    }
}

fn display_label(label: &str) -> &str {
    match label {
        "Backspace" => "Bksp",
        "Enter" => "Ent",
        other => other,
    }
}

fn label_seed(label: &str) -> f32 {
    label.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    }) as f32
        * 0.013
}

fn with_opacity(color: Color32, opacity: f32) -> Color32 {
    with_alpha(color, color.a() as f32 * opacity)
}

fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        alpha.round().clamp(0.0, 255.0) as u8,
    )
}

fn mix_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |left: u8, right: u8| -> u8 {
        (left as f32 + (right as f32 - left as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_unmultiplied(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
        lerp(a.a(), b.a()),
    )
}
