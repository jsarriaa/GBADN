#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use eframe::egui;
use gbadn_emu::app::Console;
use gbadn_emu::{AUDIO_RATE, HEIGHT, WIDTH};
use sdl2::audio::{AudioQueue, AudioSpecDesired};

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
            let mut console = Console::new(open_audio());
            if let Some(path) = cartridge {
                console.open(path, &cc.egui_ctx);
            }
            Ok(Box::new(console))
        }),
    )
}

/// A machine without sound can still play, so audio failures are not fatal.
fn open_audio() -> Option<AudioQueue<i16>> {
    let desired = AudioSpecDesired {
        freq: Some(AUDIO_RATE),
        channels: Some(2),
        samples: Some(1024),
    };
    // SDL otherwise turns Ctrl+C into an event, and only sound uses SDL here, so nobody reads it.
    sdl2::hint::set("SDL_NO_SIGNAL_HANDLERS", "1");
    let queue = sdl2::init()
        .ok()?
        .audio()
        .ok()?
        .open_queue::<i16, _>(None, &desired)
        .ok()?;
    queue.resume();
    Some(queue)
}
