use serde::{Deserialize, Serialize};

#[allow(dead_code)]
pub const PIANO_OVERLAY_CHILD_ARG: &str = "--piano-overlay-child";

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayKeyKind {
    White,
    Black,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct OverlayKeyFrame {
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub active: f32,
    pub kind: OverlayKeyKind,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct OverlayFrame {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub opacity: f32,
    pub time: f64,
    pub keys: Vec<OverlayKeyFrame>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum OverlayCommand {
    Frame(OverlayFrame),
    Hide,
    Shutdown,
}
