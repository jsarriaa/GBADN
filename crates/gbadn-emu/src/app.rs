//! The console window: open a cartridge, play it, close it.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Key, RichText, TextureOptions};
use sdl2::audio::AudioQueue;

use crate::{Core, HEIGHT, LoadError, WIDTH, keys, read_cartridge, save_path, state_path};

/// The GBA refreshes at 16.777 MHz / 280896 cycles.
const FRAME_TIME: Duration = Duration::from_nanos(16_742_706);
/// After a longer stall the game skips ahead rather than racing to catch up.
const MAX_FRAMES_PER_PAINT: u32 = 3;
/// Audio queued beyond this is dropped so sound never lags behind the picture.
const MAX_QUEUED_AUDIO_BYTES: u32 = 48000 / 5 * 4;
const SCREEN_ASPECT: f32 = WIDTH as f32 / HEIGHT as f32;

const KEYMAP: [(Key, u32); 10] = [
    (Key::Z, keys::A),
    (Key::X, keys::B),
    (Key::A, keys::L),
    (Key::S, keys::R),
    (Key::Enter, keys::START),
    (Key::Backspace, keys::SELECT),
    (Key::ArrowUp, keys::UP),
    (Key::ArrowDown, keys::DOWN),
    (Key::ArrowLeft, keys::LEFT),
    (Key::ArrowRight, keys::RIGHT),
];

struct Cartridge {
    core: Core,
    path: PathBuf,
    title: String,
    texture: egui::TextureHandle,
    next_frame: Instant,
}

/// A file being decoded on a background thread.
type Loading = (PathBuf, Receiver<Result<Vec<u8>, LoadError>>);

/// The console's state. Show it each paint with [`Console::show`].
pub struct Console {
    cartridge: Option<Cartridge>,
    loading: Option<Loading>,
    error: Option<String>,
    notice: Option<String>,
    audio: Option<AudioQueue<i16>>,
    samples: Vec<i16>,
}

impl Console {
    /// `audio` is optional because a machine without sound can still play.
    pub fn new(audio: Option<AudioQueue<i16>>) -> Self {
        Self {
            cartridge: None,
            loading: None,
            error: None,
            notice: None,
            audio,
            samples: vec![0; 4096],
        }
    }

    /// Starts decoding a file. A game in progress keeps running until this succeeds.
    pub fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.loading.is_some() {
            return;
        }
        let (sender, receiver) = channel();
        let worker_path = path.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(read_cartridge(&worker_path));
            ctx.request_repaint();
        });
        self.error = None;
        self.notice = None;
        self.loading = Some((path, receiver));
    }

    /// The current frame, when a game is running.
    pub fn screen(&self) -> Option<&[u32]> {
        self.cartridge.as_ref().map(|c| c.core.video())
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.finish_loading(&ctx);

        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            self.open(path, &ctx);
        }

        let (mut wants_open, mut wants_close) = (false, false);
        let mut wants_save_state = ctx.input(|i| i.key_pressed(Key::F5));
        let mut wants_load_state = ctx.input(|i| i.key_pressed(Key::F9));
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                let idle = self.loading.is_none();
                wants_open = ui
                    .add_enabled(idle, egui::Button::new("Open cartridge"))
                    .clicked();
                if let Some(cartridge) = &self.cartridge {
                    wants_close = ui.button("Close").clicked();
                    wants_save_state |= ui.button("Save state").clicked();
                    wants_load_state |= ui.button("Load state").clicked();
                    ui.label(RichText::new(&cartridge.title).strong());
                }
                if self.loading.is_some() {
                    ui.spinner();
                    ui.label("Reading the sequence");
                }
                if let Some(notice) = &self.notice {
                    ui.label(notice);
                }
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
            });
        });

        if wants_open
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("FASTA cartridge", &["gz", "fasta", "fa", "fna"])
                .pick_file()
        {
            self.open(path, &ctx);
        }
        if wants_save_state {
            self.save_state();
        }
        if wants_load_state {
            self.load_state();
        }
        if wants_close {
            self.cartridge = None;
            self.notice = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("GBADN Console".into()));
        }

        if self.cartridge.is_some() {
            self.run_game(&ctx);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(Color32::BLACK))
                .show(ui, |ui| self.draw_screen(ui));
        } else {
            egui::CentralPanel::default().show(ui, start_screen);
        }
    }

    fn finish_loading(&mut self, ctx: &egui::Context) {
        let Some((path, receiver)) = &self.loading else {
            return;
        };
        let Ok(result) = receiver.try_recv() else {
            return;
        };
        let path = path.clone();
        self.loading = None;

        let rom = match result {
            Ok(rom) => rom,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        // Close any running game first, so its save is flushed before a new one may open it.
        self.cartridge = None;
        match Core::new(&rom, Some(&save_path(&path))) {
            Ok(core) => {
                let title = gbadn_codec::inspect_rom(&rom)
                    .map(|info| info.title)
                    .unwrap_or_default();
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "GBADN Console - {title}"
                )));
                let blank = egui::ColorImage::filled([WIDTH, HEIGHT], Color32::BLACK);
                self.cartridge = Some(Cartridge {
                    core,
                    path,
                    title,
                    texture: ctx.load_texture("screen", blank, TextureOptions::NEAREST),
                    next_frame: Instant::now(),
                });
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn save_state(&mut self) {
        let Some(cartridge) = &mut self.cartridge else {
            return;
        };
        let path = state_path(&cartridge.path);
        let written = cartridge
            .core
            .save_state()
            .ok_or_else(|| "the console could not capture a state".to_string())
            .and_then(|state| std::fs::write(&path, state).map_err(|e| e.to_string()));
        self.report_state_result(written.map(|()| "State saved"), "Could not save the state");
    }

    fn load_state(&mut self) {
        let Some(cartridge) = &mut self.cartridge else {
            return;
        };
        let loaded = match std::fs::read(state_path(&cartridge.path)) {
            Ok(state) if cartridge.core.load_state(&state) => Ok("State loaded"),
            Ok(_) => Err("it is damaged or belongs to another game".to_string()),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                Err("there is no saved state for this game yet".to_string())
            }
            Err(error) => Err(error.to_string()),
        };
        self.report_state_result(loaded, "Could not load the state");
    }

    fn report_state_result(&mut self, result: Result<&str, String>, failure: &str) {
        match result {
            Ok(message) => {
                self.notice = Some(message.to_string());
                self.error = None;
            }
            Err(reason) => {
                self.notice = None;
                self.error = Some(format!("{failure}: {reason}"));
            }
        }
    }

    fn run_game(&mut self, ctx: &egui::Context) {
        let Some(cartridge) = &mut self.cartridge else {
            return;
        };
        // Enter and the arrows are game buttons here, so they must not drive the toolbar.
        ctx.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
        let pressed = ctx.input(|input| {
            KEYMAP
                .iter()
                .filter(|(key, _)| input.key_down(*key))
                .fold(0, |mask, (_, button)| mask | button)
        });

        let now = Instant::now();
        let mut frames = 0;
        while cartridge.next_frame <= now && frames < MAX_FRAMES_PER_PAINT {
            cartridge.core.run_frame(pressed);
            let count = cartridge.core.read_audio(&mut self.samples);
            if let Some(queue) = &self.audio
                && queue.size() < MAX_QUEUED_AUDIO_BYTES
            {
                let _ = queue.queue_audio(&self.samples[..count * 2]);
            }
            cartridge.next_frame += FRAME_TIME;
            frames += 1;
        }
        if cartridge.next_frame <= now {
            cartridge.next_frame = now + FRAME_TIME;
        }

        if frames > 0 {
            let pixels = cartridge
                .core
                .video()
                .iter()
                .map(|pixel| {
                    let [r, g, b, _] = pixel.to_le_bytes();
                    Color32::from_rgb(r, g, b)
                })
                .collect();
            cartridge.texture.set(
                egui::ColorImage::new([WIDTH, HEIGHT], pixels),
                TextureOptions::NEAREST,
            );
        }
        ctx.request_repaint();
    }

    fn draw_screen(&self, ui: &mut egui::Ui) {
        let Some(cartridge) = &self.cartridge else {
            return;
        };
        let (area, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let width = area.width().min(area.height() * SCREEN_ASPECT);
        let size = egui::vec2(width, width / SCREEN_ASPECT);
        let screen = egui::Rect::from_center_size(area.center(), size);
        let whole_texture = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(
            cartridge.texture.id(),
            screen,
            whole_texture,
            Color32::WHITE,
        );
    }
}

impl eframe::App for Console {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

fn start_screen(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(48.0);
        ui.heading("GBADN Console");
        ui.add_space(8.0);
        ui.label("Drop a .fasta.gz cartridge here, or open one above.");
        ui.add_space(24.0);
        ui.label(RichText::new("Controls").strong());
        ui.label("D-pad: arrows    A / B: Z / X    L / R: A / S");
        ui.label("Start: Enter    Select: Backspace");
        ui.label("Save state: F5    Load state: F9");
    });
}
