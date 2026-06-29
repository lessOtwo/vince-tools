use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, Result};
use eframe::egui::{
    self, Align, Align2, Color32, Event, FontId, Key, Layout, RichText, Sense, Stroke, UiBuilder,
    vec2,
};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, source::Source};

const KEYBOARD_PANEL_HEIGHT: f32 = 128.0;
const KEY_HEIGHT: f32 = 30.0;
const KEY_GAP: f32 = 6.0;
const KEY_ACTIVE_SECONDS: f64 = 0.22;
const KEY_THROTTLE_SECONDS: f64 = 0.045;

pub struct CrazyPianoTool {
    audio: PianoAudio,
    active_keys: HashMap<PianoKey, f64>,
    last_trigger_at: HashMap<PianoKey, f64>,
    recent_key: Option<PianoKey>,
    last_error: Option<String>,
    volume: f32,
    sound_enabled: bool,
    animation_enabled: bool,
}

impl CrazyPianoTool {
    pub fn new() -> Self {
        Self {
            audio: PianoAudio::new(),
            active_keys: HashMap::new(),
            last_trigger_at: HashMap::new(),
            recent_key: None,
            last_error: None,
            volume: 0.7,
            sound_enabled: true,
            animation_enabled: true,
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> bool {
        self.handle_input(ctx);

        let now = ctx.input(|input| input.time);
        self.prune_finished_animations(now);
        if !self.active_keys.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        let available = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(available, Sense::hover());
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_rgb(17, 24, 39));

        let mut back_home = false;
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(18.0, 16.0)))
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                back_home = self.draw_stage(ctx, ui, now);
            },
        );

        back_home
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        let (events, time, focused) = ctx.input(|input| {
            (
                input.events.clone(),
                input.time,
                input.viewport().focused.unwrap_or(true),
            )
        });
        if !focused {
            return;
        }

        for event in events {
            let Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                continue;
            };

            if modifiers.command || modifiers.ctrl || modifiers.alt {
                continue;
            }

            let Some(key) = PianoKey::from_egui_key(key) else {
                continue;
            };
            let last_at = self.last_trigger_at.get(&key).copied().unwrap_or(-1.0);
            if time - last_at < KEY_THROTTLE_SECONDS {
                continue;
            }

            self.last_trigger_at.insert(key, time);
            self.trigger(PianoKeyEvent { key, at: time });
        }
    }

    fn trigger(&mut self, event: PianoKeyEvent) {
        self.recent_key = Some(event.key);
        self.last_error = None;

        if self.animation_enabled {
            self.active_keys.insert(event.key, event.at);
        }

        if self.sound_enabled {
            if let Err(err) = self.audio.play(event.key, self.volume) {
                self.last_error = Some(err);
            }
        }
    }

    fn prune_finished_animations(&mut self, now: f64) {
        self.active_keys
            .retain(|_, started_at| now - *started_at <= KEY_ACTIVE_SECONDS);
    }

    fn draw_stage(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, now: f64) -> bool {
        let mut back_home = false;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("疯狂钢琴")
                        .size(28.0)
                        .strong()
                        .color(Color32::from_rgb(248, 250, 252)),
                );
                ui.label(
                    RichText::new("敲键盘，让文字变成旋律")
                        .size(14.0)
                        .color(Color32::from_rgb(199, 210, 254)),
                );
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if stage_button(ui, "返回首页", 86.0).clicked() {
                    back_home = true;
                }
            });
        });

        ui.add_space(14.0);
        self.draw_control_panel(ui);

        ui.add_space(12.0);
        let top_height = (ui.available_height() - KEYBOARD_PANEL_HEIGHT - 12.0).max(130.0);
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 92))
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(148, 163, 184, 54),
            ))
            .inner_margin(18)
            .show(ui, |ui| {
                ui.set_min_height(top_height);
                ui.vertical_centered(|ui| {
                    ui.add_space((top_height * 0.26).min(54.0));
                    let recent = self
                        .recent_key
                        .map(|key| key.display_name())
                        .unwrap_or("等待输入");
                    ui.label(
                        RichText::new(format!("最近：{recent}"))
                            .size(22.0)
                            .strong()
                            .color(Color32::from_rgb(252, 211, 77)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(self.status_text())
                            .size(13.0)
                            .color(Color32::from_rgb(203, 213, 225)),
                    );
                });
            });

        ui.add_space(12.0);
        self.draw_keyboard(ui, now);

        if back_home {
            ctx.request_repaint();
        }
        back_home
    }

    fn draw_control_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 126))
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(148, 163, 184, 58),
            ))
            .corner_radius(8)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("音量")
                            .size(13.0)
                            .color(Color32::from_rgb(226, 232, 240)),
                    );
                    ui.scope(|ui| {
                        ui.visuals_mut().widgets.inactive.bg_fill =
                            Color32::from_rgba_unmultiplied(15, 23, 42, 210);
                        ui.visuals_mut().widgets.hovered.bg_fill =
                            Color32::from_rgba_unmultiplied(30, 41, 59, 230);
                        ui.visuals_mut().widgets.active.bg_fill = Color32::from_rgb(99, 102, 241);
                        ui.add_sized(
                            [180.0, 22.0],
                            egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(true),
                        );
                    });
                    ui.add_space(10.0);
                    toggle_chip(ui, "启用音效", &mut self.sound_enabled);
                    toggle_chip(ui, "显示键盘动画", &mut self.animation_enabled);
                });
            });
    }

    fn draw_keyboard(&self, ui: &mut egui::Ui, now: f64) {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 145))
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(148, 163, 184, 68),
            ))
            .corner_radius(8)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_min_height(KEYBOARD_PANEL_HEIGHT);
                self.draw_keyboard_row(ui, "高音区", &top_row(), now);
                ui.add_space(KEY_GAP);
                self.draw_keyboard_row(ui, "中音区", &middle_row(), now);
                ui.add_space(KEY_GAP);
                self.draw_keyboard_row(ui, "低音区", &bottom_row(), now);
            });
    }

    fn draw_keyboard_row(
        &self,
        ui: &mut egui::Ui,
        caption: &str,
        keys: &[PianoKeyVisual],
        now: f64,
    ) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = KEY_GAP;
            ui.add_sized(
                [48.0, KEY_HEIGHT],
                egui::Label::new(
                    RichText::new(caption)
                        .size(12.0)
                        .color(Color32::from_rgb(148, 163, 184)),
                ),
            );

            let total_width = keys.iter().map(|key| key.width).sum::<f32>()
                + KEY_GAP * keys.len().saturating_sub(1) as f32;
            let left_pad = ((ui.available_width() - total_width) / 2.0).max(0.0);
            ui.add_space(left_pad);

            for key in keys {
                let active = self.active_progress(key.key, now);
                draw_key(ui, key, active);
            }
        });
    }

    fn active_progress(&self, key: PianoKey, now: f64) -> f32 {
        let Some(started_at) = self.active_keys.get(&key) else {
            return 0.0;
        };
        (1.0 - ((now - *started_at) / KEY_ACTIVE_SECONDS) as f32).clamp(0.0, 1.0)
    }

    fn status_text(&self) -> String {
        if let Some(err) = &self.last_error {
            return err.clone();
        }
        self.audio.status().to_owned()
    }
}

impl Default for CrazyPianoTool {
    fn default() -> Self {
        Self::new()
    }
}

struct PianoAudio {
    sink: Option<MixerDeviceSink>,
    sounds: HashMap<PianoKey, Arc<[u8]>>,
    status: String,
}

impl PianoAudio {
    fn new() -> Self {
        let mut messages = Vec::new();
        let sink = match DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => Some(sink),
            Err(err) => {
                messages.push(format!("未找到可用音频输出设备：{err}"));
                None
            }
        };

        let asset_dir = piano_asset_dir();
        let mut sounds = HashMap::new();
        let mut failed = Vec::new();

        for key in PianoKey::all() {
            match load_sound(&asset_dir, key) {
                Ok(bytes) => {
                    sounds.insert(key, bytes);
                }
                Err(err) => failed.push(format!("{}：{err:#}", key.display_name())),
            }
        }

        if failed.is_empty() {
            messages.push(format!("音效就绪：已加载 {} 个 wav", sounds.len()));
        } else {
            messages.push(format!(
                "部分音频未加载：{}。已加载 {} 个 wav",
                failed.join("；"),
                sounds.len()
            ));
        }

        Self {
            sink,
            sounds,
            status: messages.join("；"),
        }
    }

    fn play(&self, key: PianoKey, volume: f32) -> Result<(), String> {
        let Some(sink) = &self.sink else {
            return Err("音频输出不可用，请检查系统默认播放设备。".to_owned());
        };
        let Some(bytes) = self.sounds.get(&key) else {
            return Err(format!("缺少音频文件：{}", key.file_name()));
        };

        let cursor = Cursor::new(Arc::clone(bytes));
        let source = Decoder::try_from(cursor)
            .map_err(|err| format!("音频解码失败：{}，{err}", key.file_name()))?
            .amplify(volume.clamp(0.0, 1.0));
        sink.mixer().add(source);
        Ok(())
    }

    fn status(&self) -> &str {
        &self.status
    }
}

fn load_sound(asset_dir: &Path, key: PianoKey) -> Result<Arc<[u8]>> {
    let path = asset_dir.join(key.file_name());
    let bytes = fs::read(&path).with_context(|| format!("读取失败 {}", path.display()))?;
    let bytes: Arc<[u8]> = bytes.into();
    Decoder::try_from(Cursor::new(Arc::clone(&bytes)))
        .with_context(|| format!("解码失败 {}", path.display()))?;
    Ok(bytes)
}

fn piano_asset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("asset")
        .join("piano")
}

#[derive(Clone, Copy, Debug)]
struct PianoKeyEvent {
    key: PianoKey,
    at: f64,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum PianoKey {
    Letter(char),
    Space,
    Enter,
    Backspace,
}

impl PianoKey {
    fn all() -> Vec<Self> {
        let mut keys = (b'a'..=b'z')
            .map(|letter| Self::Letter(letter as char))
            .collect::<Vec<_>>();
        keys.extend([Self::Space, Self::Enter, Self::Backspace]);
        keys
    }

    fn from_egui_key(key: Key) -> Option<Self> {
        match key {
            Key::A => Some(Self::Letter('a')),
            Key::B => Some(Self::Letter('b')),
            Key::C => Some(Self::Letter('c')),
            Key::D => Some(Self::Letter('d')),
            Key::E => Some(Self::Letter('e')),
            Key::F => Some(Self::Letter('f')),
            Key::G => Some(Self::Letter('g')),
            Key::H => Some(Self::Letter('h')),
            Key::I => Some(Self::Letter('i')),
            Key::J => Some(Self::Letter('j')),
            Key::K => Some(Self::Letter('k')),
            Key::L => Some(Self::Letter('l')),
            Key::M => Some(Self::Letter('m')),
            Key::N => Some(Self::Letter('n')),
            Key::O => Some(Self::Letter('o')),
            Key::P => Some(Self::Letter('p')),
            Key::Q => Some(Self::Letter('q')),
            Key::R => Some(Self::Letter('r')),
            Key::S => Some(Self::Letter('s')),
            Key::T => Some(Self::Letter('t')),
            Key::U => Some(Self::Letter('u')),
            Key::V => Some(Self::Letter('v')),
            Key::W => Some(Self::Letter('w')),
            Key::X => Some(Self::Letter('x')),
            Key::Y => Some(Self::Letter('y')),
            Key::Z => Some(Self::Letter('z')),
            Key::Space => Some(Self::Space),
            Key::Enter => Some(Self::Enter),
            Key::Backspace => Some(Self::Backspace),
            _ => None,
        }
    }

    fn file_name(self) -> String {
        match self {
            Self::Letter(letter) => format!("key_{}.wav", letter.to_ascii_lowercase()),
            Self::Space => "key_space.wav".to_owned(),
            Self::Enter => "key_enter_chord_c_major.wav".to_owned(),
            Self::Backspace => "key_backspace_muted_low.wav".to_owned(),
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Letter('a') => "A",
            Self::Letter('b') => "B",
            Self::Letter('c') => "C",
            Self::Letter('d') => "D",
            Self::Letter('e') => "E",
            Self::Letter('f') => "F",
            Self::Letter('g') => "G",
            Self::Letter('h') => "H",
            Self::Letter('i') => "I",
            Self::Letter('j') => "J",
            Self::Letter('k') => "K",
            Self::Letter('l') => "L",
            Self::Letter('m') => "M",
            Self::Letter('n') => "N",
            Self::Letter('o') => "O",
            Self::Letter('p') => "P",
            Self::Letter('q') => "Q",
            Self::Letter('r') => "R",
            Self::Letter('s') => "S",
            Self::Letter('t') => "T",
            Self::Letter('u') => "U",
            Self::Letter('v') => "V",
            Self::Letter('w') => "W",
            Self::Letter('x') => "X",
            Self::Letter('y') => "Y",
            Self::Letter('z') => "Z",
            Self::Letter(_) => "Key",
            Self::Space => "Space",
            Self::Enter => "Enter",
            Self::Backspace => "Backspace",
        }
    }
}

#[derive(Clone, Copy)]
struct PianoKeyVisual {
    key: PianoKey,
    label: &'static str,
    width: f32,
    tone: PianoKeyTone,
}

#[derive(Clone, Copy)]
enum PianoKeyTone {
    High,
    Mid,
    Low,
    Special,
}

fn top_row() -> Vec<PianoKeyVisual> {
    letter_row("QWERTYUIOP", PianoKeyTone::High, 42.0)
}

fn middle_row() -> Vec<PianoKeyVisual> {
    let mut row = letter_row("ASDFGHJKL", PianoKeyTone::Mid, 42.0);
    row.push(PianoKeyVisual {
        key: PianoKey::Enter,
        label: "Enter 和弦",
        width: 92.0,
        tone: PianoKeyTone::Special,
    });
    row
}

fn bottom_row() -> Vec<PianoKeyVisual> {
    let mut row = letter_row("ZXCVBNM", PianoKeyTone::Low, 42.0);
    row.push(PianoKeyVisual {
        key: PianoKey::Space,
        label: "Space",
        width: 138.0,
        tone: PianoKeyTone::Low,
    });
    row.push(PianoKeyVisual {
        key: PianoKey::Backspace,
        label: "Backspace 闷击",
        width: 118.0,
        tone: PianoKeyTone::Special,
    });
    row
}

fn letter_row(letters: &'static str, tone: PianoKeyTone, width: f32) -> Vec<PianoKeyVisual> {
    letters
        .chars()
        .map(|letter| PianoKeyVisual {
            key: PianoKey::Letter(letter.to_ascii_lowercase()),
            label: match letter {
                'A' => "A",
                'B' => "B",
                'C' => "C",
                'D' => "D",
                'E' => "E",
                'F' => "F",
                'G' => "G",
                'H' => "H",
                'I' => "I",
                'J' => "J",
                'K' => "K",
                'L' => "L",
                'M' => "M",
                'N' => "N",
                'O' => "O",
                'P' => "P",
                'Q' => "Q",
                'R' => "R",
                'S' => "S",
                'T' => "T",
                'U' => "U",
                'V' => "V",
                'W' => "W",
                'X' => "X",
                'Y' => "Y",
                'Z' => "Z",
                _ => "?",
            },
            width,
            tone,
        })
        .collect()
}

fn draw_key(ui: &mut egui::Ui, key: &PianoKeyVisual, active: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(key.width, KEY_HEIGHT), Sense::hover());
    let rect = rect.translate(vec2(0.0, active * 3.0));
    let (base, text_color, stroke) = key_colors(key.tone);
    let active_color = match key.tone {
        PianoKeyTone::Special => Color32::from_rgb(129, 140, 248),
        _ => Color32::from_rgb(252, 211, 77),
    };
    let fill = mix_color(base, active_color, active * 0.82);

    ui.painter().rect_filled(rect, 5.0, fill);
    ui.painter()
        .rect_stroke(rect, 5.0, stroke, egui::StrokeKind::Inside);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        key.label,
        FontId::proportional(if key.width > 70.0 { 12.0 } else { 13.0 }),
        if active > 0.1 {
            Color32::from_rgb(17, 24, 39)
        } else {
            text_color
        },
    );
}

fn key_colors(tone: PianoKeyTone) -> (Color32, Color32, Stroke) {
    match tone {
        PianoKeyTone::High => (
            Color32::from_rgba_unmultiplied(248, 250, 252, 226),
            Color32::from_rgb(30, 41, 59),
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 92)),
        ),
        PianoKeyTone::Mid => (
            Color32::from_rgba_unmultiplied(226, 232, 240, 224),
            Color32::from_rgb(30, 41, 59),
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 72)),
        ),
        PianoKeyTone::Low => (
            Color32::from_rgba_unmultiplied(203, 213, 225, 216),
            Color32::from_rgb(17, 24, 39),
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 64)),
        ),
        PianoKeyTone::Special => (
            Color32::from_rgba_unmultiplied(31, 41, 55, 230),
            Color32::from_rgb(226, 232, 240),
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(148, 163, 184, 96)),
        ),
    }
}

fn stage_button(ui: &mut egui::Ui, text: &str, width: f32) -> egui::Response {
    ui.add_sized(
        [width, 28.0],
        egui::Button::new(RichText::new(text).size(12.0).color(Color32::WHITE))
            .fill(Color32::from_rgba_unmultiplied(99, 102, 241, 160))
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(199, 210, 254, 90),
            ))
            .corner_radius(6),
    )
}

fn toggle_chip(ui: &mut egui::Ui, text: &str, value: &mut bool) {
    let fill = if *value {
        Color32::from_rgba_unmultiplied(99, 102, 241, 190)
    } else {
        Color32::from_rgba_unmultiplied(15, 23, 42, 210)
    };
    let stroke = if *value {
        Color32::from_rgba_unmultiplied(199, 210, 254, 110)
    } else {
        Color32::from_rgba_unmultiplied(148, 163, 184, 88)
    };
    let label = if *value {
        format!("{text}：开")
    } else {
        format!("{text}：关")
    };

    if ui
        .add(
            egui::Button::new(RichText::new(label).size(12.0).color(Color32::WHITE))
                .fill(fill)
                .stroke(Stroke::new(1.0, stroke))
                .corner_radius(13)
                .min_size(vec2(112.0, 26.0)),
        )
        .clicked()
    {
        *value = !*value;
    }
}

fn mix_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |left: u8, right: u8| -> u8 {
        (left as f32 + (right as f32 - left as f32) * t).round() as u8
    };
    Color32::from_rgba_unmultiplied(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
        lerp(a.a(), b.a()),
    )
}
