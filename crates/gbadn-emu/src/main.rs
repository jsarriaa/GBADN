#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use eframe::egui;
use gbadn_emu::app::{Console, saved_bindings};
use gbadn_emu::audio::Speaker;
use gbadn_emu::controls::Gamepads;
use gbadn_emu::{HEIGHT, WIDTH};

const TOOLBAR_HEIGHT: f32 = 36.0;
const SCALE: f32 = 3.0;

fn main() -> eframe::Result {
    let cartridge = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GBADN Console")
            .with_inner_size([WIDTH as f32 * SCALE, HEIGHT as f32 * SCALE + TOOLBAR_HEIGHT])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "GBADN Console",
        options,
        Box::new(move |cc| {
            let mut console = Console::new(Speaker::open())
                .with_saved_bindings(&saved_bindings(cc.storage))
                .with_gamepads(Gamepads::new());
            if let Some(path) = cartridge {
                console.open(path, &cc.egui_ctx);
            }
            Ok(Box::new(console))
        }),
    )
}
