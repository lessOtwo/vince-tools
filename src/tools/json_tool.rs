use eframe::egui::{self, Color32, RichText, Stroke, vec2};
use serde_json::Value;

use crate::{danger_button, primary_button, scrollable_code_editor, secondary_button};

struct FieldMatch {
    path: String,
    preview: String,
}

pub struct JsonTool {
    input: String,
    output: String,
    search_query: String,
    matches: Vec<FieldMatch>,
    status: String,
    search_status: String,
}

impl Default for JsonTool {
    fn default() -> Self {
        Self {
            input: String::new(),
            output: String::new(),
            search_query: String::new(),
            matches: Vec::new(),
            status: "粘贴 JSON 后点击格式化或压缩。".to_owned(),
            search_status: "输入字段名后搜索。".to_owned(),
        }
    }
}

impl JsonTool {
    pub fn with_input(input: String) -> Self {
        let mut tool = Self::default();
        tool.input = input;
        if !tool.input.trim().is_empty() {
            tool.auto_format_json();
        }
        tool
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if primary_button(ui, "格式化").clicked() {
                self.format_json();
            }
            if secondary_button(ui, "压缩").clicked() {
                self.compact_json();
            }
            if secondary_button(ui, "复制结果").clicked() {
                if self.output.is_empty() {
                    self.status = "没有可复制的结果。".to_owned();
                } else {
                    ctx.copy_text(self.output.clone());
                    self.status = "已复制格式化结果。".to_owned();
                }
            }
            if danger_button(ui, "清空").clicked() {
                *self = Self::default();
            }
            ui.separator();
            ui.label(RichText::new(&self.status).color(Color32::from_rgb(79, 88, 105)));
        });

        ui.add_space(10.0);
        self.search_bar(ui);
        ui.add_space(10.0);

        let editor_height = (ui.available_height() - 116.0).max(300.0);
        ui.columns(2, |columns| {
            columns[0].label(RichText::new("输入 JSON").strong());
            let input_width = columns[0].available_width();
            let input_response = scrollable_code_editor(
                &mut columns[0],
                "json_input_editor",
                &mut self.input,
                vec2(input_width, editor_height),
                true,
                "{\"name\":\"vince\"}",
            );
            if input_response.changed() {
                self.auto_format_json();
            }

            columns[1].label(RichText::new("格式化结果").strong());
            let output_width = columns[1].available_width();
            scrollable_code_editor(
                &mut columns[1],
                "json_output_editor",
                &mut self.output,
                vec2(output_width, editor_height),
                false,
                "",
            );
        });

        ui.add_space(8.0);
        self.search_results(ui);
    }

    fn search_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("字段搜索");
            let response = ui.add_sized(
                [260.0, 28.0],
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("输入字段名，例如 user_id"),
            );
            let enter_pressed = ui.input(|input| input.key_pressed(egui::Key::Enter));
            if primary_button(ui, "搜索").clicked() || (response.lost_focus() && enter_pressed) {
                self.run_field_search();
            }
            if secondary_button(ui, "清除搜索").clicked() {
                self.search_query.clear();
                self.matches.clear();
                self.search_status = "输入字段名后搜索。".to_owned();
            }
            ui.label(RichText::new(&self.search_status).color(Color32::from_rgb(79, 88, 105)));
        });
    }

    fn search_results(&mut self, ui: &mut egui::Ui) {
        if self.matches.is_empty() {
            return;
        }

        egui::Frame::new()
            .fill(Color32::from_rgb(244, 247, 250))
            .stroke(Stroke::new(1.0, Color32::from_rgb(219, 226, 235)))
            .corner_radius(8)
            .inner_margin(8)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(96.0)
                    .show(ui, |ui| {
                        egui::Grid::new("field_search_results")
                            .num_columns(2)
                            .striped(true)
                            .spacing(vec2(18.0, 6.0))
                            .show(ui, |ui| {
                                ui.strong("路径");
                                ui.strong("值预览");
                                ui.end_row();

                                for matched in &self.matches {
                                    ui.monospace(&matched.path);
                                    ui.label(&matched.preview);
                                    ui.end_row();
                                }
                            });
                    });
            });
    }

    fn format_json(&mut self) {
        match parse_json(&self.input) {
            Ok(value) => match serde_json::to_string_pretty(&value) {
                Ok(output) => {
                    self.output = output;
                    self.status = "格式化完成。".to_owned();
                    self.run_field_search();
                }
                Err(err) => self.status = format!("格式化失败：{err}"),
            },
            Err(err) => self.status = err,
        }
    }

    fn compact_json(&mut self) {
        match parse_json(&self.input) {
            Ok(value) => match serde_json::to_string(&value) {
                Ok(output) => {
                    self.output = output;
                    self.status = "压缩完成。".to_owned();
                    self.run_field_search();
                }
                Err(err) => self.status = format!("压缩失败：{err}"),
            },
            Err(err) => self.status = err,
        }
    }

    fn auto_format_json(&mut self) {
        if self.input.trim().is_empty() {
            self.output.clear();
            self.matches.clear();
            self.status = "粘贴 JSON 后会自动格式化。".to_owned();
            return;
        }

        match parse_json(&self.input) {
            Ok(value) => match serde_json::to_string_pretty(&value) {
                Ok(output) => {
                    self.output = output;
                    self.status = "已自动格式化。".to_owned();
                    self.run_field_search();
                }
                Err(err) => self.status = format!("格式化失败：{err}"),
            },
            Err(err) => self.status = err,
        }
    }

    fn run_field_search(&mut self) {
        let query = self.search_query.trim();
        self.matches.clear();

        if query.is_empty() {
            self.search_status = "输入字段名后搜索。".to_owned();
            return;
        }

        match parse_json(&self.input) {
            Ok(value) => {
                search_fields(&value, &query.to_lowercase(), "$", &mut self.matches);
                self.search_status = if self.matches.is_empty() {
                    "未找到匹配字段。".to_owned()
                } else {
                    format!("找到 {} 个匹配字段。", self.matches.len())
                };
            }
            Err(err) => {
                self.search_status = err;
            }
        }
    }
}

fn parse_json(input: &str) -> Result<Value, String> {
    if input.trim().is_empty() {
        return Err("请输入 JSON。".to_owned());
    }
    serde_json::from_str(input).map_err(|err| format!("JSON 解析失败：{err}"))
}

fn search_fields(value: &Value, needle: &str, path: &str, matches: &mut Vec<FieldMatch>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = join_field_path(path, key);
                if key.to_lowercase().contains(needle) {
                    matches.push(FieldMatch {
                        path: child_path.clone(),
                        preview: value_preview(child),
                    });
                }
                search_fields(child, needle, &child_path, matches);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                search_fields(child, needle, &format!("{path}[{index}]"), matches);
            }
        }
        _ => {}
    }
}

fn join_field_path(parent: &str, key: &str) -> String {
    if is_plain_field_name(key) {
        format!("{parent}.{key}")
    } else {
        let encoded = serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_owned());
        format!("{parent}[{encoded}]")
    }
}

fn is_plain_field_name(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn value_preview(value: &Value) -> String {
    let raw = match value {
        Value::String(text) => text.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    };
    truncate_chars(&raw, 120)
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
