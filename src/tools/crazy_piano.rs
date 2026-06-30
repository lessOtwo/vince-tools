use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::Duration,
};

use anyhow::{Context as _, Result};
use device_query::{DeviceQuery, DeviceState, Keycode};
use eframe::egui::{self, Color32, RichText, Stroke, vec2};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, source::Source};

use super::native_piano_overlay::{
    NativePianoOverlay, OverlayFrame, OverlayKeyFrame, OverlayKeyKind,
};

const DEFAULT_OVERLAY_HEIGHT: f32 = 400.0;
const MIN_OVERLAY_HEIGHT: f32 = 180.0;
const MAX_OVERLAY_HEIGHT: f32 = 720.0;
const KEY_ACTIVE_SECONDS: f64 = 0.24;
const KEY_THROTTLE_SECONDS: f64 = 0.045;
const KEY_POLL_INTERVAL: Duration = Duration::from_millis(12);
const MAX_OVERLAY_OPACITY: f32 = 0.5;
const DEFAULT_OVERLAY_OPACITY_BOOST: f32 = 0.07;
const DEFAULT_MIN_OVERLAY_OPACITY: f32 = 0.0;
const DEFAULT_OVERLAY_FADE_SECONDS: f32 = 1.1;
const OVERLAY_IDLE_DELAY: f64 = 0.25;
const WHITE_KEY_WIDTH: f32 = 82.0;
const SPACE_KEY_WIDTH: f32 = 154.0;
const BLACK_KEY_WIDTH: f32 = 34.0;
const WHITE_KEY_GAP: f32 = 2.0;

pub struct CrazyPianoTool {
    audio: PianoAudio,
    active_keys: HashMap<PianoKey, f64>,
    last_trigger_at: HashMap<PianoKey, f64>,
    recent_key: Option<PianoKey>,
    last_error: Option<String>,
    keyboard_rx: Option<Receiver<PianoKey>>,
    keyboard_stop: Option<Arc<AtomicBool>>,
    listener_status: String,
    enabled: bool,
    sound_enabled: bool,
    animation_enabled: bool,
    volume: f32,
    overlay_height: f32,
    overlay_opacity_boost: f32,
    min_overlay_opacity: f32,
    overlay_fade_seconds: f32,
    overlay_opacity: f32,
    last_key_at: f64,
    last_overlay_tick: f64,
    overlay: NativePianoOverlay,
}

impl CrazyPianoTool {
    pub fn new() -> Self {
        Self {
            audio: PianoAudio::new(),
            active_keys: HashMap::new(),
            last_trigger_at: HashMap::new(),
            recent_key: None,
            last_error: None,
            keyboard_rx: None,
            keyboard_stop: None,
            listener_status: "全局钢琴未开启。".to_owned(),
            enabled: false,
            sound_enabled: true,
            animation_enabled: true,
            volume: 0.7,
            overlay_height: DEFAULT_OVERLAY_HEIGHT,
            overlay_opacity_boost: DEFAULT_OVERLAY_OPACITY_BOOST,
            min_overlay_opacity: DEFAULT_MIN_OVERLAY_OPACITY,
            overlay_fade_seconds: DEFAULT_OVERLAY_FADE_SECONDS,
            overlay_opacity: 0.0,
            last_key_at: -1.0,
            last_overlay_tick: 0.0,
            overlay: NativePianoOverlay::new(),
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        self.sync_keyboard_listener();
        self.poll_global_keys(ctx);

        let now = ctx.input(|input| input.time);
        self.update_overlay_opacity(now);
        self.prune_finished_animations(now);
        self.update_native_overlay(now);
        if self.overlay_opacity > 0.0 || !self.active_keys.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if self.enabled {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    pub fn ui(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        self.draw_config_page(ui);
    }

    pub fn overlay_visible(&self) -> bool {
        self.enabled && self.overlay_opacity > 0.001
    }

    fn draw_config_page(&mut self, ui: &mut egui::Ui) {
        let recent = self
            .recent_key
            .map(|key| key.display_name())
            .unwrap_or("等待输入");
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("全局键盘演奏已准备好")
                    .strong()
                    .color(Color32::from_rgb(42, 57, 82)),
            );
            ui.separator();
            ui.label(
                RichText::new(format!("最近：{recent}")).color(Color32::from_rgb(79, 88, 105)),
            );
        });

        ui.add_space(12.0);
        let frame_width = ui.available_width();
        egui::Frame::new()
            .fill(Color32::from_rgb(248, 250, 253))
            .stroke(Stroke::new(1.0, Color32::from_rgb(222, 230, 242)))
            .corner_radius(8)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width((frame_width - 28.0).max(0.0));
                ui.horizontal(|ui| {
                    toggle_chip(ui, "工具开启", &mut self.enabled);
                    toggle_chip(ui, "启用音效", &mut self.sound_enabled);
                    toggle_chip(ui, "显示按键动画", &mut self.animation_enabled);
                });

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("音量")
                            .size(13.0)
                            .color(Color32::from_rgb(42, 57, 82)),
                    );
                    styled_slider(ui, &mut self.volume, 0.0..=1.0, 180.0);
                    ui.add_space(18.0);
                    ui.label(
                        RichText::new("显示高度")
                            .size(13.0)
                            .color(Color32::from_rgb(42, 57, 82)),
                    );
                    styled_slider(
                        ui,
                        &mut self.overlay_height,
                        MIN_OVERLAY_HEIGHT..=MAX_OVERLAY_HEIGHT,
                        220.0,
                    );
                    ui.label(
                        RichText::new(format!("{:.0}px", self.overlay_height))
                            .size(12.0)
                            .color(Color32::from_rgb(79, 88, 105)),
                    );
                });

                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    parameter_slider(
                        ui,
                        "连续按键透明度系数",
                        &mut self.overlay_opacity_boost,
                        0.0..=0.25,
                        150.0,
                    );
                    parameter_slider(
                        ui,
                        "最小透明度",
                        &mut self.min_overlay_opacity,
                        0.0..=MAX_OVERLAY_OPACITY,
                        150.0,
                    );
                    parameter_slider(
                        ui,
                        "渐隐时间",
                        &mut self.overlay_fade_seconds,
                        0.1..=5.0,
                        150.0,
                    );
                    ui.label(
                        RichText::new("秒")
                            .size(12.0)
                            .color(Color32::from_rgb(79, 88, 105)),
                    );
                });

                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!("最近：{recent}"))
                        .size(16.0)
                        .strong()
                        .color(Color32::from_rgb(37, 99, 235)),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(self.status_text())
                        .size(13.0)
                        .color(Color32::from_rgb(79, 88, 105)),
                );
                ui.label(
                    RichText::new(&self.listener_status)
                        .size(12.0)
                        .color(Color32::from_rgb(102, 116, 139)),
                );
            });
    }

    fn sync_keyboard_listener(&mut self) {
        if self.enabled && self.keyboard_rx.is_none() {
            let (rx, stop) = spawn_keyboard_listener();
            self.keyboard_rx = Some(rx);
            self.keyboard_stop = Some(stop);
            self.listener_status =
                "全局监听已开启，仅响应 A-Z / Space / Enter / Backspace。".to_owned();
        } else if !self.enabled && self.keyboard_rx.is_some() {
            self.stop_keyboard_listener();
        }
    }

    fn stop_keyboard_listener(&mut self) {
        if let Some(stop) = self.keyboard_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.keyboard_rx = None;
        self.listener_status = "全局钢琴未开启。".to_owned();
    }

    fn poll_global_keys(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.keyboard_rx.take() else {
            return;
        };

        let time = ctx.input(|input| input.time);
        while let Ok(key) = rx.try_recv() {
            let last_at = self.last_trigger_at.get(&key).copied().unwrap_or(-1.0);
            if time - last_at < KEY_THROTTLE_SECONDS {
                continue;
            }

            self.last_trigger_at.insert(key, time);
            self.trigger(PianoKeyEvent { key, at: time });
        }

        self.keyboard_rx = Some(rx);
    }

    fn trigger(&mut self, event: PianoKeyEvent) {
        self.recent_key = Some(event.key);
        self.last_error = None;
        self.last_key_at = event.at;
        let min_opacity = self.min_overlay_opacity.clamp(0.0, MAX_OVERLAY_OPACITY);
        self.overlay_opacity = (self.overlay_opacity.max(min_opacity)
            + self.overlay_opacity_boost.clamp(0.0, MAX_OVERLAY_OPACITY))
        .min(MAX_OVERLAY_OPACITY);

        if self.animation_enabled {
            self.active_keys.insert(event.key, event.at);
        }

        if self.sound_enabled {
            if let Err(err) = self.audio.play(event.key, self.volume) {
                self.last_error = Some(err);
            }
        }
    }

    fn update_overlay_opacity(&mut self, now: f64) {
        if !self.enabled {
            self.overlay_opacity = 0.0;
            self.last_overlay_tick = now;
            return;
        }

        if self.last_overlay_tick <= 0.0 {
            self.last_overlay_tick = now;
            return;
        }

        let dt = (now - self.last_overlay_tick).clamp(0.0, 0.1) as f32;
        self.last_overlay_tick = now;

        if self.last_key_at >= 0.0 && now - self.last_key_at > OVERLAY_IDLE_DELAY {
            let min_opacity = self.min_overlay_opacity.clamp(0.0, MAX_OVERLAY_OPACITY);
            let fade_seconds = self.overlay_fade_seconds.max(0.1);
            let fade_per_second = (MAX_OVERLAY_OPACITY - min_opacity).max(0.01) / fade_seconds;
            self.overlay_opacity = (self.overlay_opacity - dt * fade_per_second).max(min_opacity);
            if min_opacity <= 0.001 && self.overlay_opacity <= 0.001 {
                self.overlay_opacity = 0.0;
            }
        }
    }

    fn update_native_overlay(&mut self, now: f64) {
        if !self.overlay_visible() {
            self.overlay.hide();
            return;
        }

        let frame = self.overlay_frame(now);
        self.overlay.update(&frame);
    }

    fn overlay_frame(&self, now: f64) -> OverlayFrame {
        let (screen_width, screen_height) = NativePianoOverlay::primary_screen_size();
        let natural_width = white_keys().iter().map(|key| key.width).sum::<f32>()
            + WHITE_KEY_GAP * white_keys().len().saturating_sub(1) as f32;
        let max_width = (screen_width as f32 - 48.0).max(120.0);
        let scale = (max_width / natural_width).min(1.0).max(0.55);
        let width = (natural_width * scale).round().max(120.0) as i32;
        let height = self
            .overlay_height
            .clamp(MIN_OVERLAY_HEIGHT, screen_height as f32)
            .round() as i32;
        let x = ((screen_width - width) / 2).max(0);
        let y = (screen_height - height).max(0);

        OverlayFrame {
            x,
            y,
            width,
            height,
            opacity: self.overlay_opacity,
            keys: self.overlay_keys(width as f32, height as f32, now),
        }
    }

    fn overlay_keys(&self, width: f32, height: f32, now: f64) -> Vec<OverlayKeyFrame> {
        let white_rects = white_key_layout(width, height);
        let mut keys = white_keys()
            .into_iter()
            .zip(white_rects.iter().copied())
            .map(|(key, rect)| OverlayKeyFrame {
                label: key.label,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                active: self.active_progress(key.key, now),
                kind: OverlayKeyKind::White,
            })
            .collect::<Vec<_>>();

        let scale = keyboard_width_scale(width);
        for key in black_keys() {
            if key.after_white_index + 1 >= white_rects.len() {
                continue;
            }

            let left = white_rects[key.after_white_index];
            let right = white_rects[key.after_white_index + 1];
            let black_width = BLACK_KEY_WIDTH * scale;
            let black_height = height * 0.58;
            let center_x = (left.right() + right.x) * 0.5;
            keys.push(OverlayKeyFrame {
                label: key.label,
                x: center_x - black_width / 2.0,
                y: 0.0,
                width: black_width,
                height: black_height,
                active: self.active_progress(key.key, now),
                kind: OverlayKeyKind::Black,
            });
        }

        keys
    }

    fn prune_finished_animations(&mut self, now: f64) {
        self.active_keys
            .retain(|_, started_at| now - *started_at <= KEY_ACTIVE_SECONDS);
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

impl Drop for CrazyPianoTool {
    fn drop(&mut self) {
        self.stop_keyboard_listener();
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

fn spawn_keyboard_listener() -> (Receiver<PianoKey>, Arc<AtomicBool>) {
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);

    thread::spawn(move || {
        let device_state = DeviceState::new();
        let mut previous = HashSet::new();

        while !thread_stop.load(Ordering::Relaxed) {
            let current = device_state.get_keys().into_iter().collect::<HashSet<_>>();
            let modifier_down = has_modifier(&current);

            if !modifier_down {
                for keycode in current.difference(&previous) {
                    if let Some(key) = PianoKey::from_device_key(*keycode) {
                        if tx.send(key).is_err() {
                            return;
                        }
                    }
                }
            }

            previous = current;
            thread::sleep(KEY_POLL_INTERVAL);
        }
    });

    (rx, stop)
}

fn has_modifier(keys: &HashSet<Keycode>) -> bool {
    [
        Keycode::LControl,
        Keycode::RControl,
        Keycode::LAlt,
        Keycode::RAlt,
        Keycode::Command,
        Keycode::RCommand,
        Keycode::LOption,
        Keycode::ROption,
        Keycode::LMeta,
        Keycode::RMeta,
    ]
    .iter()
    .any(|key| keys.contains(key))
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
    let exe_asset_dir = std::env::current_exe().ok().and_then(|path| {
        path.parent()
            .map(|parent| parent.join("asset").join("piano"))
    });
    if let Some(path) = exe_asset_dir {
        if path.exists() {
            return path;
        }
    }

    let cwd_asset_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("asset")
        .join("piano");
    if cwd_asset_dir.exists() {
        return cwd_asset_dir;
    }

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

    fn from_device_key(key: Keycode) -> Option<Self> {
        match key {
            Keycode::A => Some(Self::Letter('a')),
            Keycode::B => Some(Self::Letter('b')),
            Keycode::C => Some(Self::Letter('c')),
            Keycode::D => Some(Self::Letter('d')),
            Keycode::E => Some(Self::Letter('e')),
            Keycode::F => Some(Self::Letter('f')),
            Keycode::G => Some(Self::Letter('g')),
            Keycode::H => Some(Self::Letter('h')),
            Keycode::I => Some(Self::Letter('i')),
            Keycode::J => Some(Self::Letter('j')),
            Keycode::K => Some(Self::Letter('k')),
            Keycode::L => Some(Self::Letter('l')),
            Keycode::M => Some(Self::Letter('m')),
            Keycode::N => Some(Self::Letter('n')),
            Keycode::O => Some(Self::Letter('o')),
            Keycode::P => Some(Self::Letter('p')),
            Keycode::Q => Some(Self::Letter('q')),
            Keycode::R => Some(Self::Letter('r')),
            Keycode::S => Some(Self::Letter('s')),
            Keycode::T => Some(Self::Letter('t')),
            Keycode::U => Some(Self::Letter('u')),
            Keycode::V => Some(Self::Letter('v')),
            Keycode::W => Some(Self::Letter('w')),
            Keycode::X => Some(Self::Letter('x')),
            Keycode::Y => Some(Self::Letter('y')),
            Keycode::Z => Some(Self::Letter('z')),
            Keycode::Space => Some(Self::Space),
            Keycode::Enter | Keycode::NumpadEnter => Some(Self::Enter),
            Keycode::Backspace => Some(Self::Backspace),
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
}

#[derive(Clone, Copy)]
struct BlackPianoKeyVisual {
    key: PianoKey,
    label: &'static str,
    after_white_index: usize,
}

#[derive(Clone, Copy)]
struct KeyLayoutRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl KeyLayoutRect {
    fn right(self) -> f32 {
        self.x + self.width
    }
}

fn white_key_layout(width: f32, height: f32) -> Vec<KeyLayoutRect> {
    let keys = white_keys();
    let scale = keyboard_width_scale(width);
    let gap = WHITE_KEY_GAP * scale;
    let mut x = 0.0;

    keys.iter()
        .map(|key| {
            let key_width = key.width * scale;
            let rect = KeyLayoutRect {
                x,
                y: 0.0,
                width: key_width,
                height,
            };
            x += key_width + gap;
            rect
        })
        .collect()
}

fn keyboard_width_scale(width: f32) -> f32 {
    let keys = white_keys();
    let natural_width = keys.iter().map(|key| key.width).sum::<f32>()
        + WHITE_KEY_GAP * keys.len().saturating_sub(1) as f32;
    (width / natural_width).clamp(0.55, 1.0)
}

fn white_keys() -> Vec<PianoKeyVisual> {
    let mut keys = "ASDFGHJKLZXCVBNM"
        .chars()
        .map(|letter| PianoKeyVisual {
            key: PianoKey::Letter(letter.to_ascii_lowercase()),
            label: letter_label(letter),
            width: WHITE_KEY_WIDTH,
        })
        .collect::<Vec<_>>();
    keys.push(PianoKeyVisual {
        key: PianoKey::Space,
        label: "Space",
        width: SPACE_KEY_WIDTH,
    });
    keys
}

fn black_keys() -> Vec<BlackPianoKeyVisual> {
    let mut keys = [
        ('Q', 0),
        ('W', 1),
        ('E', 3),
        ('R', 4),
        ('T', 5),
        ('Y', 7),
        ('U', 8),
        ('I', 10),
        ('O', 11),
        ('P', 12),
    ]
    .into_iter()
    .map(|(letter, after_white_index)| BlackPianoKeyVisual {
        key: PianoKey::Letter(letter.to_ascii_lowercase()),
        label: letter_label(letter),
        after_white_index,
    })
    .collect::<Vec<_>>();
    keys.push(BlackPianoKeyVisual {
        key: PianoKey::Enter,
        label: "Enter",
        after_white_index: 14,
    });
    keys.push(BlackPianoKeyVisual {
        key: PianoKey::Backspace,
        label: "Backspace",
        after_white_index: 15,
    });
    keys
}

fn letter_label(letter: char) -> &'static str {
    match letter {
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
    }
}

fn toggle_chip(ui: &mut egui::Ui, text: &str, value: &mut bool) {
    let (fill, stroke, text_color) = if *value {
        (
            Color32::from_rgb(91, 107, 238),
            Color32::from_rgb(91, 107, 238),
            Color32::WHITE,
        )
    } else {
        (
            Color32::from_rgb(248, 250, 253),
            Color32::from_rgb(205, 213, 225),
            Color32::from_rgb(42, 57, 82),
        )
    };
    let label = if *value {
        format!("{text}：开")
    } else {
        format!("{text}：关")
    };

    if ui
        .add(
            egui::Button::new(RichText::new(label).size(12.0).color(text_color))
                .fill(fill)
                .stroke(Stroke::new(1.0, stroke))
                .corner_radius(13)
                .min_size(vec2(126.0, 26.0)),
        )
        .clicked()
    {
        *value = !*value;
    }
}

fn styled_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    width: f32,
) {
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(226, 232, 240);
        ui.visuals_mut().widgets.hovered.bg_fill = Color32::from_rgb(214, 223, 238);
        ui.visuals_mut().widgets.active.bg_fill = Color32::from_rgb(91, 107, 238);
        ui.add_sized(
            [width, 22.0],
            egui::Slider::new(value, range).show_value(true),
        );
    });
}

fn parameter_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    width: f32,
) {
    ui.label(
        RichText::new(label)
            .size(13.0)
            .color(Color32::from_rgb(42, 57, 82)),
    );
    styled_slider(ui, value, range, width);
}
