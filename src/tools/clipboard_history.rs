use std::{sync::mpsc, thread, time::Duration};

use eframe::egui::{self, Color32, RichText, Stroke};

use crate::secondary_button;

const CLIPBOARD_HISTORY_LIMIT: usize = 60;

pub struct ClipboardHistoryTool {
    items: Vec<String>,
    status: String,
    rx: mpsc::Receiver<String>,
}

impl ClipboardHistoryTool {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            status: "监听中，复制文本后会自动出现在这里。".to_owned(),
            rx: spawn_clipboard_watcher(),
        }
    }

    pub fn poll(&mut self) {
        while let Ok(text) = self.rx.try_recv() {
            self.add_text(text);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("已记录 {} 条", self.items.len()))
                    .strong()
                    .color(Color32::from_rgb(42, 57, 82)),
            );
            if secondary_button(ui, "清空历史").clicked() {
                self.items.clear();
                self.status = "历史已清空，继续监听新的复制文本。".to_owned();
            }
            ui.separator();
            ui.label(RichText::new(&self.status).color(Color32::from_rgb(79, 88, 105)));
        });

        ui.add_space(12.0);
        if self.items.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.label(
                    RichText::new("暂无剪贴板文本历史")
                        .size(18.0)
                        .color(Color32::from_rgb(102, 116, 139)),
                );
                ui.label(
                    RichText::new("复制一段文本后，它会自动出现在这里。")
                        .color(Color32::from_rgb(127, 139, 158)),
                );
            });
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("clipboard_history_list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for index in 0..self.items.len() {
                    let text = self.items[index].clone();
                    if clipboard_item_button(ui, &text).clicked() {
                        self.restore_text(text);
                    }
                    ui.add_space(8.0);
                }
            });
    }

    fn add_text(&mut self, text: String) {
        if text.is_empty() {
            return;
        }

        self.items.retain(|item| item != &text);
        self.items.insert(0, text.clone());
        self.items.truncate(CLIPBOARD_HISTORY_LIMIT);
        self.status = format!("已捕获：{}", compact_preview(&text, 64));
    }

    fn restore_text(&mut self, text: String) {
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text.clone())) {
            Ok(()) => {
                self.add_text(text);
                self.status = "已恢复到剪贴板。".to_owned();
            }
            Err(err) => {
                self.status = format!("写入剪贴板失败：{err}");
            }
        }
    }
}

impl Default for ClipboardHistoryTool {
    fn default() -> Self {
        Self::new()
    }
}

fn clipboard_item_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let preview = compact_preview(text, 160);
    ui.add_sized(
        [ui.available_width(), 46.0],
        egui::Button::new(
            RichText::new(preview)
                .color(Color32::from_rgb(37, 51, 76))
                .size(13.0),
        )
        .fill(Color32::from_rgb(248, 250, 253))
        .stroke(Stroke::new(1.0, Color32::from_rgb(222, 230, 242)))
        .corner_radius(8),
    )
}

fn spawn_clipboard_watcher() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut last_text = String::new();

        loop {
            match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                Ok(text) if !text.is_empty() && text != last_text => {
                    last_text = text.clone();
                    if tx.send(text).is_err() {
                        break;
                    }
                }
                _ => {}
            }

            thread::sleep(Duration::from_millis(700));
        }
    });

    rx
}

fn compact_preview(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return "[空白文本]".to_owned();
    }
    truncate_chars(&compact, max_chars)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}
