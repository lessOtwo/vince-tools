#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use directories::ProjectDirs;
use eframe::{App, CreationContext, egui};
use egui::{
    Align, Color32, FontData, FontDefinitions, FontFamily, Layout, Rect, RichText, Sense, Stroke,
    TextureHandle, TextureOptions, UiBuilder, Vec2, ViewportBuilder, ViewportCommand, ViewportId,
    pos2, vec2,
};
use serde::{Deserialize, Serialize};
use tools::{
    clipboard_history::ClipboardHistoryTool, crazy_piano::CrazyPianoTool, json_tool::JsonTool,
};

mod tools;

const APP_TITLE: &str = "Vince Tools";
const FLOAT_MARGIN: f32 = 20.0;
const COMPACT_SIZE: Vec2 = Vec2::new(76.0, 76.0);
const MENU_SIZE: Vec2 = Vec2::new(248.0, 258.0);
const TOOL_SIZE: Vec2 = Vec2::new(1040.0, 660.0);
const CLIPBOARD_SIZE: Vec2 = Vec2::new(680.0, 560.0);
const CRAZY_PIANO_SIZE: Vec2 = Vec2::new(760.0, 560.0);
const SETTINGS_SIZE: Vec2 = Vec2::new(540.0, 360.0);
const TOOL_TITLE_HEIGHT: f32 = 48.0;
const WINDOW_CONTROL_WIDTH: f32 = 46.0;
const MENU_BUTTON_SLOT_HEIGHT: f32 = 34.0;
const MENU_BUTTON_HEIGHT: f32 = 30.0;
const MENU_BUTTON_HOVER_FONT_SIZE: f32 = 13.5;
const MENU_BUTTON_FONT_SIZE: f32 = 12.0;
const DEFAULT_ICON_BYTES: &[u8] = include_bytes!("asset/default.png");

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_inner_size(COMPACT_SIZE)
            .with_min_inner_size(COMPACT_SIZE)
            .with_position(pos2(1200.0, FLOAT_MARGIN))
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false),
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(VinceToolsApp::new(cc)))),
    )
}

#[derive(Default, Deserialize, Serialize)]
struct AppConfig {
    icon_path: Option<PathBuf>,
}

struct VinceToolsApp {
    config: AppConfig,
    config_path: PathBuf,
    config_dir: PathBuf,
    icon_texture: Option<TextureHandle>,
    icon_status: String,
    json: JsonTool,
    clipboard: ClipboardHistoryTool,
    crazy_piano: CrazyPianoTool,
    json_open: bool,
    json_center_pending: bool,
    json_last_position: Option<egui::Pos2>,
    json_start_position: Option<egui::Pos2>,
    clipboard_open: bool,
    clipboard_center_pending: bool,
    clipboard_last_position: Option<egui::Pos2>,
    clipboard_start_position: Option<egui::Pos2>,
    crazy_piano_open: bool,
    crazy_piano_center_pending: bool,
    crazy_piano_last_position: Option<egui::Pos2>,
    crazy_piano_start_position: Option<egui::Pos2>,
    settings_open: bool,
    settings_center_pending: bool,
    settings_last_position: Option<egui::Pos2>,
    settings_start_position: Option<egui::Pos2>,
    last_launcher_size: Option<Vec2>,
    launcher_user_moved: bool,
}

impl VinceToolsApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        install_cjk_font(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::light());

        let config_dir = config_dir();
        let config_path = config_dir.join("config.json");
        let config = load_config(&config_path);
        let (icon_texture, icon_status) = match config.icon_path.as_deref() {
            Some(path) => match load_icon_texture(&cc.egui_ctx, path) {
                Ok(texture) => (Some(texture), "已加载自定义图标。".to_owned()),
                Err(err) => match load_default_icon_texture(&cc.egui_ctx) {
                    Ok(texture) => (
                        Some(texture),
                        format!("自定义图标加载失败：{err}，已使用默认图标。"),
                    ),
                    Err(default_err) => (
                        None,
                        format!("自定义图标加载失败：{err}；默认图标加载失败：{default_err}"),
                    ),
                },
            },
            None => match load_default_icon_texture(&cc.egui_ctx) {
                Ok(texture) => (Some(texture), "当前使用默认图标。".to_owned()),
                Err(err) => (None, format!("默认图标加载失败：{err}")),
            },
        };

        Self {
            config,
            config_path,
            config_dir,
            icon_texture,
            icon_status,
            json: JsonTool::default(),
            clipboard: ClipboardHistoryTool::new(),
            crazy_piano: CrazyPianoTool::new(),
            json_open: false,
            json_center_pending: false,
            json_last_position: None,
            json_start_position: None,
            clipboard_open: false,
            clipboard_center_pending: false,
            clipboard_last_position: None,
            clipboard_start_position: None,
            crazy_piano_open: false,
            crazy_piano_center_pending: false,
            crazy_piano_last_position: None,
            crazy_piano_start_position: None,
            settings_open: false,
            settings_center_pending: false,
            settings_last_position: None,
            settings_start_position: None,
            last_launcher_size: None,
            launcher_user_moved: false,
        }
    }

    fn launcher_size(&self, launcher_hovered: bool) -> Vec2 {
        if launcher_hovered {
            MENU_SIZE
        } else {
            COMPACT_SIZE
        }
    }

    fn apply_launcher_window_shape(&mut self, ctx: &egui::Context, desired_size: Vec2) {
        if self.last_launcher_size == Some(desired_size) {
            return;
        }

        ctx.send_viewport_cmd(ViewportCommand::InnerSize(desired_size));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(desired_size));
        ctx.send_viewport_cmd(ViewportCommand::MaxInnerSize(desired_size));

        if !self.launcher_user_moved {
            if let Some(monitor_size) = ctx.input(|input| input.viewport().monitor_size) {
                let x = (monitor_size.x - desired_size.x - FLOAT_MARGIN).max(FLOAT_MARGIN);
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(x, FLOAT_MARGIN)));
            }
        }

        self.last_launcher_size = Some(desired_size);
    }

    fn show_launcher(&mut self, ctx: &egui::Context, launcher_hovered: bool) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                if launcher_hovered {
                    self.show_launcher_menu(ui, ctx);
                } else {
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            self.icon_tile(ui, ctx, 56.0, true);
                        },
                    );
                }
            });
    }

    fn show_launcher_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(250, 252, 255, 246))
            .stroke(Stroke::new(1.0, Color32::from_rgb(205, 213, 225)))
            .corner_radius(18)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.icon_tile(ui, ctx, 48.0, true);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(APP_TITLE).strong().size(18.0));
                        ui.label(RichText::new("本地工具集").color(Color32::from_rgb(83, 95, 116)));
                    });
                });

                ui.add_space(8.0);

                if full_width_primary_button(ui, "JSON 格式化工具").clicked() {
                    self.open_json_tool(ctx);
                }
                if full_width_button(ui, "剪贴板历史管理器").clicked() {
                    self.open_clipboard_history(ctx);
                }
                if full_width_button(ui, "疯狂钢琴").clicked() {
                    self.open_crazy_piano(ctx);
                }
                if full_width_button(ui, "设置").clicked() {
                    self.open_settings(ctx);
                }
                if full_width_danger_button(ui, "退出").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            });
    }

    fn open_json_tool(&mut self, ctx: &egui::Context) {
        if self.json_open {
            focus_viewport(ctx, json_viewport_id());
            return;
        }

        self.json_open = true;
        self.json_start_position = self.json_last_position;
        self.json_center_pending = true;
        ctx.request_repaint();
    }

    fn open_clipboard_history(&mut self, ctx: &egui::Context) {
        if self.clipboard_open {
            focus_viewport(ctx, clipboard_viewport_id());
            return;
        }

        self.clipboard_open = true;
        self.clipboard_start_position = self.clipboard_last_position;
        self.clipboard_center_pending = true;
        ctx.request_repaint();
    }

    fn open_crazy_piano(&mut self, ctx: &egui::Context) {
        if self.crazy_piano_open {
            focus_viewport(ctx, crazy_piano_viewport_id());
            return;
        }

        self.crazy_piano_open = true;
        self.crazy_piano_start_position = self.crazy_piano_last_position;
        self.crazy_piano_center_pending = true;
        ctx.request_repaint();
    }

    fn open_settings(&mut self, ctx: &egui::Context) {
        if self.settings_open {
            focus_viewport(ctx, settings_viewport_id());
            return;
        }

        self.settings_open = true;
        self.settings_start_position = self.settings_last_position;
        self.settings_center_pending = true;
        ctx.request_repaint();
    }

    fn icon_tile(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, size: f32, draggable: bool) {
        let sense = if draggable {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(size, size), sense);

        if let Some(texture) = &self.icon_texture {
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        if draggable && response.drag_started() {
            self.launcher_user_moved = true;
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
    }

    fn show_tool_viewports(&mut self, ctx: &egui::Context) {
        if self.json_open {
            let builder = centered_tool_builder(ctx, "JSON 格式化工具", TOOL_SIZE);
            ctx.show_viewport_immediate(json_viewport_id(), builder, |ctx, _class| {
                if ctx.input(|input| input.viewport().close_requested()) {
                    record_viewport_position(ctx, &mut self.json_last_position);
                    self.json_open = false;
                    return;
                }
                let placed_this_frame = place_viewport_once(
                    ctx,
                    &mut self.json_center_pending,
                    self.json_start_position,
                );
                if !placed_this_frame {
                    record_viewport_position(ctx, &mut self.json_last_position);
                }
                self.show_json_tool(ctx);
            });
        }

        if self.clipboard_open {
            let builder = centered_tool_builder(ctx, "剪贴板历史管理器", CLIPBOARD_SIZE);
            ctx.show_viewport_immediate(clipboard_viewport_id(), builder, |ctx, _class| {
                if ctx.input(|input| input.viewport().close_requested()) {
                    record_viewport_position(ctx, &mut self.clipboard_last_position);
                    self.clipboard_open = false;
                    return;
                }
                let placed_this_frame = place_viewport_once(
                    ctx,
                    &mut self.clipboard_center_pending,
                    self.clipboard_start_position,
                );
                if !placed_this_frame {
                    record_viewport_position(ctx, &mut self.clipboard_last_position);
                }
                self.show_clipboard_history(ctx);
            });
        }

        if self.crazy_piano_open {
            let builder = centered_tool_builder(ctx, "疯狂钢琴", CRAZY_PIANO_SIZE);
            ctx.show_viewport_immediate(crazy_piano_viewport_id(), builder, |ctx, _class| {
                if ctx.input(|input| input.viewport().close_requested()) {
                    record_viewport_position(ctx, &mut self.crazy_piano_last_position);
                    self.crazy_piano_open = false;
                    return;
                }
                let placed_this_frame = place_viewport_once(
                    ctx,
                    &mut self.crazy_piano_center_pending,
                    self.crazy_piano_start_position,
                );
                if !placed_this_frame {
                    record_viewport_position(ctx, &mut self.crazy_piano_last_position);
                }
                self.show_crazy_piano(ctx);
            });
        }

        if self.settings_open {
            let builder = centered_tool_builder(ctx, "设置", SETTINGS_SIZE);
            ctx.show_viewport_immediate(settings_viewport_id(), builder, |ctx, _class| {
                if ctx.input(|input| input.viewport().close_requested()) {
                    record_viewport_position(ctx, &mut self.settings_last_position);
                    self.settings_open = false;
                    return;
                }
                let placed_this_frame = place_viewport_once(
                    ctx,
                    &mut self.settings_center_pending,
                    self.settings_start_position,
                );
                if !placed_this_frame {
                    record_viewport_position(ctx, &mut self.settings_last_position);
                }
                self.show_settings(ctx);
            });
        }
    }

    fn show_json_tool(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(tool_window_background_frame())
            .show(ctx, |ui| {
                tool_panel(ui, |ui| {
                    if title_bar(ui, ctx, "JSON 格式化工具") {
                        self.json_open = false;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                        return;
                    }

                    tool_body(ui, |ui| {
                        self.json.ui(ctx, ui);
                    });
                });
            });
    }

    fn show_clipboard_history(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(tool_window_background_frame())
            .show(ctx, |ui| {
                tool_panel(ui, |ui| {
                    if title_bar(ui, ctx, "剪贴板历史管理器") {
                        self.clipboard_open = false;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                        return;
                    }

                    tool_body(ui, |ui| {
                        self.clipboard.ui(ui);
                    });
                });
            });
    }

    fn show_crazy_piano(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(tool_window_background_frame())
            .show(ctx, |ui| {
                tool_panel(ui, |ui| {
                    if title_bar(ui, ctx, "疯狂钢琴") {
                        self.crazy_piano_open = false;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                        return;
                    }

                    tool_body(ui, |ui| {
                        self.crazy_piano.ui(ctx, ui);
                    });
                });
            });
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(tool_window_background_frame())
            .show(ctx, |ui| {
                tool_panel(ui, |ui| {
                    if title_bar(ui, ctx, "设置") {
                        self.settings_open = false;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                        return;
                    }

                    tool_body(ui, |ui| {
                        ui.horizontal(|ui| {
                            self.icon_tile(ui, ctx, 72.0, false);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("悬浮图标").strong().size(18.0));
                                ui.label(
                                    "选择 PNG 后会复制到本地配置目录，并立即替换悬浮入口显示。",
                                );
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    if primary_button(ui, "选择 PNG").clicked() {
                                        self.pick_icon(ctx);
                                    }
                                    if secondary_button(ui, "恢复默认").clicked() {
                                        self.reset_icon(ctx);
                                    }
                                });
                            });
                        });

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(&self.icon_status).color(Color32::from_rgb(70, 83, 103)),
                        );
                        ui.label(format!("配置目录：{}", self.config_dir.display()));
                    });
                });
            });
    }

    fn pick_icon(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG 图片", &["png"])
            .pick_file()
        else {
            return;
        };

        if !has_png_extension(&path) {
            self.icon_status = "请选择 PNG 图片。".to_owned();
            return;
        }

        let target = self.config_dir.join("launcher-icon.png");
        if let Err(err) = fs::create_dir_all(&self.config_dir) {
            self.icon_status = format!("创建配置目录失败：{err}");
            return;
        }
        if let Err(err) = fs::copy(&path, &target) {
            self.icon_status = format!("保存图标失败：{err}");
            return;
        }

        match load_icon_texture(ctx, &target) {
            Ok(texture) => {
                self.icon_texture = Some(texture);
                self.config.icon_path = Some(target);
                self.save_config();
                self.icon_status = "图标已更新。".to_owned();
            }
            Err(err) => {
                self.icon_status = format!("PNG 加载失败：{err}");
            }
        }
    }

    fn reset_icon(&mut self, ctx: &egui::Context) {
        self.config.icon_path = None;
        self.save_config();
        match load_default_icon_texture(ctx) {
            Ok(texture) => {
                self.icon_texture = Some(texture);
                self.icon_status = "已恢复默认图标。".to_owned();
            }
            Err(err) => {
                self.icon_texture = None;
                self.icon_status = format!("默认图标加载失败：{err}");
            }
        }
        ctx.request_repaint();
    }

    fn save_config(&mut self) {
        if let Err(err) = save_config(&self.config_path, &self.config) {
            self.icon_status = format!("保存配置失败：{err}");
        }
    }
}

impl App for VinceToolsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.clipboard.poll();
        self.crazy_piano.update(ctx);
        let launcher_hovered = ctx.input(|input| input.pointer.hover_pos().is_some());
        let launcher_size = self.launcher_size(launcher_hovered);
        self.apply_launcher_window_shape(ctx, launcher_size);
        self.show_launcher(ctx, launcher_hovered);
        self.show_tool_viewports(ctx);

        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::TRANSPARENT.to_normalized_gamma_f32()
    }
}

fn title_bar(ui: &mut egui::Ui, ctx: &egui::Context, title: &str) -> bool {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, TOOL_TITLE_HEIGHT), Sense::click_and_drag());

    paint_horizontal_gradient_rect(
        ui,
        rect,
        Color32::from_rgb(53, 116, 255),
        Color32::from_rgb(139, 77, 255),
    );

    let mut close_clicked = false;
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add_space(16.0);
            ui.label(
                RichText::new(title)
                    .strong()
                    .size(18.0)
                    .color(Color32::WHITE),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if window_control_button(ui, WindowControlKind::Close).clicked() {
                    close_clicked = true;
                }
                if window_control_button(ui, WindowControlKind::Maximize).clicked() {
                    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                if window_control_button(ui, WindowControlKind::Minimize).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
            });
        },
    );

    if response.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }

    close_clicked || escape_pressed(ctx)
}

fn escape_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.key_pressed(egui::Key::Escape))
}

fn tool_window_background_frame() -> egui::Frame {
    egui::Frame::new().fill(Color32::WHITE).inner_margin(0)
}

fn tool_panel<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let available_size = ui.available_size();
    let (panel_rect, _) = ui.allocate_exact_size(available_size, Sense::hover());

    ui.painter().rect_filled(panel_rect, 0.0, Color32::WHITE);

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(panel_rect)
            .layout(Layout::top_down(Align::Min)),
        add_contents,
    )
    .inner
}

fn tool_body<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(14)
        .show(ui, add_contents)
        .inner
}

fn paint_horizontal_gradient_rect(
    ui: &egui::Ui,
    rect: Rect,
    left_color: Color32,
    right_color: Color32,
) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), left_color);
    mesh.colored_vertex(rect.right_top(), right_color);
    mesh.colored_vertex(rect.right_bottom(), right_color);
    mesh.colored_vertex(rect.left_bottom(), left_color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);

    ui.painter().add(mesh);
}

fn full_width_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    menu_button_response(
        ui,
        text,
        Color32::from_rgb(248, 250, 253),
        Color32::from_rgb(218, 227, 240),
        Color32::from_rgb(37, 51, 76),
    )
}

fn full_width_primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    menu_button_response(
        ui,
        text,
        Color32::from_rgb(61, 111, 246),
        Color32::from_rgb(61, 111, 246),
        Color32::WHITE,
    )
}

fn full_width_danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    menu_button_response(
        ui,
        text,
        Color32::from_rgb(255, 245, 245),
        Color32::from_rgb(241, 190, 190),
        Color32::from_rgb(154, 52, 52),
    )
}

pub(crate) fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(styled_button(
        text,
        Color32::from_rgb(61, 111, 246),
        Color32::from_rgb(61, 111, 246),
        Color32::WHITE,
        54.0,
    ))
}

pub(crate) fn secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(styled_button(
        text,
        Color32::from_rgb(244, 247, 252),
        Color32::from_rgb(198, 210, 230),
        Color32::from_rgb(42, 57, 82),
        54.0,
    ))
}

pub(crate) fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(styled_button(
        text,
        Color32::from_rgb(255, 245, 245),
        Color32::from_rgb(241, 190, 190),
        Color32::from_rgb(154, 52, 52),
        46.0,
    ))
}

#[derive(Clone, Copy)]
enum WindowControlKind {
    Minimize,
    Maximize,
    Close,
}

fn window_control_button(ui: &mut egui::Ui, kind: WindowControlKind) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(WINDOW_CONTROL_WIDTH, TOOL_TITLE_HEIGHT),
        Sense::click(),
    );
    paint_window_control_icon(ui, rect, kind);

    response.on_hover_text(match kind {
        WindowControlKind::Minimize => "最小化",
        WindowControlKind::Maximize => "最大化",
        WindowControlKind::Close => "关闭",
    })
}

fn paint_window_control_icon(ui: &egui::Ui, rect: Rect, kind: WindowControlKind) {
    let center = rect.center();
    let stroke = Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 255, 255, 210));

    match kind {
        WindowControlKind::Minimize => {
            ui.painter().line_segment(
                [
                    pos2(center.x - 5.0, center.y),
                    pos2(center.x + 5.0, center.y),
                ],
                stroke,
            );
        }
        WindowControlKind::Maximize => {
            let left = center.x - 4.0;
            let right = center.x + 4.0;
            let top = center.y - 4.0;
            let bottom = center.y + 4.0;
            let painter = ui.painter();
            painter.line_segment([pos2(left, top), pos2(right, top)], stroke);
            painter.line_segment([pos2(right, top), pos2(right, bottom)], stroke);
            painter.line_segment([pos2(right, bottom), pos2(left, bottom)], stroke);
            painter.line_segment([pos2(left, bottom), pos2(left, top)], stroke);
        }
        WindowControlKind::Close => {
            ui.painter().line_segment(
                [
                    pos2(center.x - 4.6, center.y - 4.6),
                    pos2(center.x + 4.6, center.y + 4.6),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    pos2(center.x + 4.6, center.y - 4.6),
                    pos2(center.x - 4.6, center.y + 4.6),
                ],
                stroke,
            );
        }
    }
}

fn menu_button_response(
    ui: &mut egui::Ui,
    text: &str,
    fill: Color32,
    stroke: Color32,
    text_color: Color32,
) -> egui::Response {
    let (slot_rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), MENU_BUTTON_SLOT_HEIGHT),
        Sense::click(),
    );
    let hovered = response.hovered();
    let rect = if hovered {
        slot_rect
    } else {
        Rect::from_center_size(
            slot_rect.center(),
            vec2(slot_rect.width() - 6.0, MENU_BUTTON_HEIGHT),
        )
    };
    let font_size = if hovered {
        MENU_BUTTON_HOVER_FONT_SIZE
    } else {
        MENU_BUTTON_FONT_SIZE
    };

    ui.painter().rect(
        rect,
        6.0,
        fill,
        Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(font_size),
        text_color,
    );

    response
}

fn styled_button<'a>(
    text: &'a str,
    fill: Color32,
    stroke: Color32,
    text_color: Color32,
    min_width: f32,
) -> egui::Button<'a> {
    egui::Button::new(RichText::new(text).color(text_color).size(12.0))
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(6)
        .min_size(vec2(min_width, 26.0))
}

pub(crate) fn scrollable_code_editor(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    text: &mut String,
    size: Vec2,
    interactive: bool,
    hint_text: &str,
) -> egui::Response {
    let stroke = Stroke::new(1.0, Color32::from_rgb(35, 48, 72));
    let fill = Color32::from_rgb(15, 23, 42);
    let content_size = editor_content_size(text, size);

    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .inner_margin(0)
        .show(ui, |ui| {
            ui.set_min_size(size);
            egui::ScrollArea::both()
                .id_salt(id_salt)
                .auto_shrink([false, false])
                .max_width(size.x)
                .max_height(size.y)
                .min_scrolled_width(size.x)
                .min_scrolled_height(size.y)
                .scroll_bar_visibility(
                    egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                )
                .show(ui, |ui| {
                    let mut editor = egui::TextEdit::multiline(text)
                        .code_editor()
                        .background_color(fill)
                        .text_color(Color32::from_rgb(224, 231, 255))
                        .margin(egui::Margin::symmetric(8, 6))
                        .frame(false)
                        .desired_width(content_size.x)
                        .interactive(interactive);
                    if !hint_text.is_empty() {
                        editor = editor.hint_text(hint_text);
                    }

                    ui.add_sized([content_size.x, content_size.y], editor)
                })
                .inner
        })
        .inner
}

fn editor_content_size(text: &str, viewport_size: Vec2) -> Vec2 {
    let line_count = text.lines().count().max(1) as f32;
    let max_line_chars = text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(1) as f32;
    vec2(
        viewport_size.x.max(max_line_chars * 8.5 + 32.0),
        viewport_size.y.max(line_count * 18.0 + 24.0),
    )
}

fn json_viewport_id() -> ViewportId {
    ViewportId::from_hash_of("vince-tools-json-window")
}

fn clipboard_viewport_id() -> ViewportId {
    ViewportId::from_hash_of("vince-tools-clipboard-window")
}

fn crazy_piano_viewport_id() -> ViewportId {
    ViewportId::from_hash_of("vince-tools-crazy-piano-window")
}

fn settings_viewport_id() -> ViewportId {
    ViewportId::from_hash_of("vince-tools-settings-window")
}

fn focus_viewport(ctx: &egui::Context, id: ViewportId) {
    ctx.send_viewport_cmd_to(id, ViewportCommand::Visible(true));
    ctx.send_viewport_cmd_to(id, ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
}

fn centered_tool_builder(ctx: &egui::Context, title: &str, size: Vec2) -> ViewportBuilder {
    let mut builder = ViewportBuilder::default()
        .with_title(format!("{APP_TITLE} - {title}"))
        .with_inner_size(size)
        .with_min_inner_size(size)
        .with_max_inner_size(size)
        .with_decorations(false)
        .with_transparent(false)
        .with_resizable(false)
        .with_taskbar(true);

    if let Some(position) = centered_position(ctx, size) {
        builder = builder.with_position(position);
    }

    builder
}

fn centered_position(ctx: &egui::Context, size: Vec2) -> Option<egui::Pos2> {
    ctx.input(|input| {
        input.viewport().monitor_size.map(|monitor_size| {
            pos2(
                ((monitor_size.x - size.x) / 2.0).max(0.0),
                ((monitor_size.y - size.y) / 2.0).max(0.0),
            )
        })
    })
}

fn place_viewport_once(
    ctx: &egui::Context,
    pending: &mut bool,
    position: Option<egui::Pos2>,
) -> bool {
    if !*pending {
        return false;
    }

    if let Some(position) = position {
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
        *pending = false;
        return true;
    }

    let Some(command) = ViewportCommand::center_on_screen(ctx) else {
        return false;
    };

    ctx.send_viewport_cmd(command);
    *pending = false;
    true
}

fn record_viewport_position(ctx: &egui::Context, position: &mut Option<egui::Pos2>) {
    if let Some(current_position) =
        ctx.input(|input| input.viewport().outer_rect.map(|rect| rect.min))
    {
        *position = Some(current_position);
    }
}

fn config_dir() -> PathBuf {
    ProjectDirs::from("dev", "Vince", "VinceTools")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".vince-tools")
        })
}

fn load_config(path: &Path) -> AppConfig {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_config(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    fs::write(path, text).map_err(|err| err.to_string())
}

fn has_png_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

fn load_default_icon_texture(ctx: &egui::Context) -> Result<TextureHandle, String> {
    load_icon_texture_from_bytes(ctx, "default-launcher-icon", DEFAULT_ICON_BYTES)
}

fn load_icon_texture(ctx: &egui::Context, path: &Path) -> Result<TextureHandle, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    load_icon_texture_from_bytes(ctx, "custom-launcher-icon", &bytes)
}

fn load_icon_texture_from_bytes(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Result<TextureHandle, String> {
    let image = image::load_from_memory(&bytes)
        .map_err(|err| err.to_string())?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Ok(ctx.load_texture(name, color_image, TextureOptions::LINEAR))
}

fn install_cjk_font(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
    ];

    let Some(bytes) = candidates.iter().find_map(|path| fs::read(path).ok()) else {
        return;
    };

    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "system-cjk".to_owned(),
        Arc::new(FontData::from_owned(bytes)),
    );

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "system-cjk".to_owned());
    }

    ctx.set_fonts(fonts);
}
