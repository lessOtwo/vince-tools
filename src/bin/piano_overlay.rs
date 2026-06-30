#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../piano_overlay_child.rs"]
mod piano_overlay_child;
#[path = "../piano_overlay_protocol.rs"]
mod piano_overlay_protocol;

fn main() -> eframe::Result<()> {
    piano_overlay_child::run()
}
