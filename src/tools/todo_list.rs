use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, TextEdit, pos2,
    vec2,
};
use serde::{Deserialize, Serialize};

use crate::{danger_button, primary_button, secondary_button};

#[derive(Clone, Deserialize, Serialize)]
pub struct TodoList {
    pub title: String,
    pub expanded: bool,
    pub items: Vec<TodoItem>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TodoItem {
    pub prefix: String,
    pub text: String,
    pub done: bool,
    pub deleted: bool,
}

pub struct TodoListTool {
    lists: Vec<TodoList>,
    title_input: String,
    batch_input: String,
    status: String,
    add_dialog_open: bool,
}

impl TodoListTool {
    pub fn new() -> Self {
        Self {
            lists: Vec::new(),
            title_input: String::new(),
            batch_input: String::new(),
            status: "点击新增清单后可批量录入待办。".to_owned(),
            add_dialog_open: false,
        }
    }

    pub fn with_lists(lists: Vec<TodoList>) -> Self {
        let mut tool = Self::new();
        tool.lists = lists;
        if !tool.lists.is_empty() {
            tool.status = format!("已恢复 {} 个 TODOlist。", tool.lists.len());
        }
        tool
    }

    pub fn lists(&self) -> &[TodoList] {
        &self.lists
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        self.add_dialog(ui.ctx());
        ui.add_space(10.0);
        self.list_panel(ui);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if primary_button(ui, "新增清单").clicked() {
                self.add_dialog_open = true;
            }
            ui.separator();
            ui.label(RichText::new(&self.status).color(Color32::from_rgb(79, 88, 105)));
        });
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        if !self.add_dialog_open {
            return;
        }

        let mut open = true;
        let mut close_after = false;
        egui::Window::new("新增 TODOlist")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(580.0, 360.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("标题")
                            .strong()
                            .color(Color32::from_rgb(42, 57, 82)),
                    );
                    ui.add_sized(
                        [360.0, 28.0],
                        TextEdit::singleline(&mut self.title_input).hint_text("可为空"),
                    );
                });

                ui.add_space(8.0);
                ui.label(
                    RichText::new("批量待办")
                        .strong()
                        .color(Color32::from_rgb(42, 57, 82)),
                );
                ui.add_sized(
                    [ui.available_width(), 190.0],
                    TextEdit::multiline(&mut self.batch_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("3、已有序号会保留\n没有序号的行会自动生成 1、2、3、"),
                );

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, "录入为新清单").clicked() && self.create_list_from_input()
                    {
                        close_after = true;
                    }
                    if secondary_button(ui, "新建空清单").clicked() {
                        self.create_empty_list();
                        close_after = true;
                    }
                    if secondary_button(ui, "取消").clicked() {
                        close_after = true;
                    }
                });
            });

        self.add_dialog_open = open && !close_after;
    }

    fn create_list_from_input(&mut self) -> bool {
        let items = parse_todo_items(&self.batch_input);
        if items.is_empty() {
            self.status = "没有可录入的待办内容。".to_owned();
            return false;
        }

        let title = self.title_input.trim().to_owned();
        self.lists.insert(
            0,
            TodoList {
                title,
                expanded: true,
                items,
            },
        );
        self.title_input.clear();
        self.batch_input.clear();
        self.status = "已录入新 TODOlist。".to_owned();
        true
    }

    fn create_empty_list(&mut self) {
        let title = self.title_input.trim().to_owned();
        self.lists.insert(
            0,
            TodoList {
                title,
                expanded: true,
                items: Vec::new(),
            },
        );
        self.title_input.clear();
        self.batch_input.clear();
        self.status = "已新建空 TODOlist。".to_owned();
    }

    fn list_panel(&mut self, ui: &mut egui::Ui) {
        if self.lists.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.label(
                    RichText::new("暂无 TODOlist")
                        .size(18.0)
                        .color(Color32::from_rgb(102, 116, 139)),
                );
            });
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("todo_list_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut index = 0;
                while index < self.lists.len() {
                    let remove = draw_list_card(ui, index, &mut self.lists[index]);
                    if remove {
                        self.lists.remove(index);
                    } else {
                        index += 1;
                    }
                    ui.add_space(10.0);
                }
            });
    }
}

impl Default for TodoListTool {
    fn default() -> Self {
        Self::new()
    }
}

fn draw_list_card(ui: &mut egui::Ui, index: usize, list: &mut TodoList) -> bool {
    let mut remove = false;
    let title = list_title(index, &list.title);
    let done_count = list
        .items
        .iter()
        .filter(|item| item.done && !item.deleted)
        .count();
    let active_count = list.items.iter().filter(|item| !item.deleted).count();
    let deleted_count = list.items.iter().filter(|item| item.deleted).count();

    egui::Frame::new()
        .fill(Color32::from_rgb(248, 250, 253))
        .stroke(Stroke::new(1.0, Color32::from_rgb(222, 230, 242)))
        .corner_radius(8)
        .inner_margin(10)
        .show(ui, |ui| {
            let header_text =
                format!("{title}    完成 {done_count}/{active_count}    已删除 {deleted_count}");
            let header = paint_list_header(ui, list.expanded, &header_text);
            if header.clicked() {
                list.expanded = !list.expanded;
            }

            if list.expanded {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("标题");
                    ui.add_sized(
                        [260.0, 26.0],
                        TextEdit::singleline(&mut list.title).hint_text("可为空"),
                    );
                    if danger_button(ui, "删除清单").clicked() {
                        remove = true;
                    }
                });

                ui.add_space(8.0);
                if list.items.is_empty() {
                    ui.label(
                        RichText::new("这个清单还没有待办项。")
                            .color(Color32::from_rgb(102, 116, 139)),
                    );
                } else {
                    for item in &mut list.items {
                        draw_item_row(ui, item);
                        ui.add_space(6.0);
                    }
                }
            }
        });

    remove
}

fn paint_list_header(ui: &mut egui::Ui, expanded: bool, text: &str) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    ui.painter().rect(
        rect,
        6.0,
        Color32::WHITE,
        Stroke::new(1.0, Color32::from_rgb(218, 227, 240)),
        egui::StrokeKind::Inside,
    );

    let icon_rect =
        Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(12.0, 12.0));
    paint_expand_triangle(ui, icon_rect, expanded, Color32::from_rgb(37, 51, 76));
    ui.painter().text(
        pos2(rect.left() + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(14.0),
        Color32::from_rgb(37, 51, 76),
    );

    response
}

fn paint_expand_triangle(ui: &egui::Ui, rect: Rect, expanded: bool, color: Color32) {
    let points = if expanded {
        vec![
            pos2(rect.left(), rect.top() + 2.0),
            pos2(rect.right(), rect.top() + 2.0),
            pos2(rect.center().x, rect.bottom() - 2.0),
        ]
    } else {
        vec![
            pos2(rect.left() + 2.0, rect.top()),
            pos2(rect.left() + 2.0, rect.bottom()),
            pos2(rect.right() - 2.0, rect.center().y),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        Stroke::new(0.0, Color32::TRANSPARENT),
    ));
}

fn draw_item_row(ui: &mut egui::Ui, item: &mut TodoItem) {
    let fill = if item.deleted {
        Color32::from_rgb(248, 250, 253)
    } else if item.done {
        Color32::from_rgb(226, 246, 232)
    } else {
        Color32::WHITE
    };
    let stroke = if item.done && !item.deleted {
        Color32::from_rgb(134, 197, 151)
    } else {
        Color32::from_rgb(222, 230, 242)
    };

    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(6)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let text_width = (ui.available_width() - 182.0).max(160.0);
                let mut text = RichText::new(format!("{} {}", item.prefix, item.text))
                    .size(13.0)
                    .color(if item.deleted {
                        Color32::from_rgb(110, 118, 132)
                    } else {
                        Color32::from_rgb(37, 51, 76)
                    });
                if item.deleted {
                    text = text.strikethrough();
                }

                ui.scope(|ui| {
                    ui.set_min_width(text_width);
                    ui.set_max_width(text_width);
                    ui.add(egui::Label::new(text).wrap());
                });

                let (icon_rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                if item.done && !item.deleted {
                    paint_check_icon(ui, icon_rect);
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if item.deleted {
                        if secondary_button(ui, "恢复").clicked() {
                            item.deleted = false;
                        }
                    } else if danger_button(ui, "删除").clicked() {
                        item.deleted = true;
                    }

                    if item.done {
                        if secondary_button(ui, "取消完成").clicked() {
                            item.done = false;
                        }
                    } else if primary_button(ui, "完成").clicked() {
                        item.done = true;
                    }
                });
            });
        });
}

fn paint_check_icon(ui: &egui::Ui, rect: Rect) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.42;
    ui.painter()
        .circle_filled(center, radius, Color32::from_rgb(220, 252, 231));
    ui.painter().line_segment(
        [
            pos2(center.x - radius * 0.55, center.y + radius * 0.02),
            pos2(center.x - radius * 0.18, center.y + radius * 0.38),
        ],
        Stroke::new(2.2, Color32::from_rgb(22, 163, 74)),
    );
    ui.painter().line_segment(
        [
            pos2(center.x - radius * 0.18, center.y + radius * 0.38),
            pos2(center.x + radius * 0.62, center.y - radius * 0.44),
        ],
        Stroke::new(2.2, Color32::from_rgb(22, 163, 74)),
    );
}

fn list_title(index: usize, title: &str) -> String {
    if title.trim().is_empty() {
        format!("TODOlist {}", index + 1)
    } else {
        title.trim().to_owned()
    }
}

fn parse_todo_items(input: &str) -> Vec<TodoItem> {
    let mut items = Vec::new();
    let mut auto_number = 1;

    for raw_line in input.lines() {
        let line = trim_wrapping_quotes(raw_line.trim());
        if line.is_empty() {
            continue;
        }

        let (prefix, text) = if let Some((prefix, text)) = split_numbered_prefix(line) {
            (prefix, text)
        } else {
            let prefix = format!("{auto_number}、");
            auto_number += 1;
            (prefix, line.to_owned())
        };

        if text.trim().is_empty() {
            continue;
        }

        items.push(TodoItem {
            prefix,
            text: text.trim().to_owned(),
            done: false,
            deleted: false,
        });
    }

    items
}

fn split_numbered_prefix(line: &str) -> Option<(String, String)> {
    let mut digit_end = 0;
    let mut has_digit = false;

    for (index, ch) in line.char_indices() {
        if is_number_char(ch) {
            has_digit = true;
            digit_end = index + ch.len_utf8();
        } else {
            break;
        }
    }

    if !has_digit {
        return None;
    }

    let delimiter = line[digit_end..].chars().next()?;
    if !is_number_delimiter(delimiter) {
        return None;
    }

    let prefix_end = digit_end + delimiter.len_utf8();
    Some((line[..prefix_end].to_owned(), line[prefix_end..].to_owned()))
}

fn is_number_char(ch: char) -> bool {
    ch.is_ascii_digit() || ('０'..='９').contains(&ch)
}

fn is_number_delimiter(ch: char) -> bool {
    matches!(ch, '、' | '.' | '．' | ')' | '）' | ',' | '，')
}

fn trim_wrapping_quotes(line: &str) -> &str {
    line.trim_matches(|ch| matches!(ch, '“' | '”' | '"' | '\'' | '`'))
        .trim()
}
