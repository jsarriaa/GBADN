//! The console window: open a cartridge, play it, close it.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Key, RichText, TextureOptions};

use crate::audio::Speaker;
use crate::controls::{BUTTONS, Gamepads, KeyBindings, RESERVED_KEYS};
use crate::{Core, HEIGHT, LoadError, WIDTH, read_cartridge, save_path, state_path};

/// The GBA refreshes at 16.777 MHz / 280896 cycles.
const FRAME_TIME: Duration = Duration::from_nanos(16_742_706);
/// After a longer stall the game skips ahead rather than racing to catch up.
const MAX_FRAMES_PER_PAINT: u32 = 3;
/// Sound is still produced without a speaker, so the core's buffers don't fill.
const SILENT_RATE: u32 = 48000;
const SCREEN_ASPECT: f32 = WIDTH as f32 / HEIGHT as f32;

const BINDINGS_SETTING: &str = "key_bindings";

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
    speaker: Option<Speaker>,
    samples: Vec<i16>,
    bindings: KeyBindings,
    gamepads: Option<Gamepads>,
    /// The Controls screen is open, which pauses the game.
    showing_controls: bool,
    /// The GBA button waiting for a new key.
    rebinding: Option<u32>,
}

impl Console {
    /// `speaker` is optional because a machine without sound can still play.
    pub fn new(speaker: Option<Speaker>) -> Self {
        Self {
            cartridge: None,
            loading: None,
            error: None,
            notice: None,
            speaker,
            samples: vec![0; 4096],
            bindings: KeyBindings::default(),
            gamepads: None,
            showing_controls: false,
            rebinding: None,
        }
    }

    /// Restores the player's keys from saved settings text.
    pub fn with_saved_bindings(mut self, text: &str) -> Self {
        self.bindings = KeyBindings::from_text(text);
        self
    }

    /// Reads connected gamepads, on machines that can.
    pub fn with_gamepads(mut self, gamepads: Option<Gamepads>) -> Self {
        self.gamepads = gamepads;
        self
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
        let mut wants_controls = false;
        let mut wants_save_state = ctx.input(|i| i.key_pressed(Key::F5));
        let mut wants_load_state = ctx.input(|i| i.key_pressed(Key::F9));
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                let idle = self.loading.is_none();
                wants_open = ui
                    .add_enabled(idle, egui::Button::new("Open cartridge"))
                    .clicked();
                wants_controls = ui.button("Controls").clicked();
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

        if wants_controls {
            self.showing_controls = !self.showing_controls;
            self.rebinding = None;
        }

        if self.showing_controls {
            self.capture_rebinding(&ctx);
            egui::CentralPanel::default().show(ui, |ui| self.controls_screen(ui));
        } else if self.cartridge.is_some() {
            self.run_game(&ctx);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(Color32::BLACK))
                .show(ui, |ui| self.draw_screen(ui));
        } else {
            egui::CentralPanel::default().show(ui, |ui| self.start_screen(ui));
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
        let keyboard = ctx.input(|input| self.bindings.pressed(|key| input.key_down(key)));
        let gamepad = self.gamepads.as_mut().map_or(0, Gamepads::pressed);
        let pressed = keyboard | gamepad;

        let now = Instant::now();
        let mut frames = 0;
        while cartridge.next_frame <= now && frames < MAX_FRAMES_PER_PAINT {
            cartridge.core.run_frame(pressed);
            let rate = self.speaker.as_ref().map_or(SILENT_RATE, Speaker::rate);
            let count = cartridge.core.read_audio(&mut self.samples, rate);
            if let Some(speaker) = &self.speaker {
                speaker.push(&self.samples[..count * 2]);
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

    /// Takes the next key press for the button waiting for one.
    fn capture_rebinding(&mut self, ctx: &egui::Context) {
        let Some(button) = self.rebinding else {
            return;
        };
        let pressed = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } => Some(*key),
                _ => None,
            })
        });
        match pressed {
            Some(Key::Escape) => self.rebinding = None,
            Some(key) if !RESERVED_KEYS.contains(&key) => {
                self.bindings.assign(button, key);
                self.rebinding = None;
            }
            _ => {}
        }
    }

    fn controls_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Controls");
        ui.label("Click a key to change it, then press the new one. Esc cancels.");
        ui.add_space(8.0);
        egui::Grid::new("controls").num_columns(2).show(ui, |ui| {
            for (button, name, _) in BUTTONS {
                ui.label(name);
                let text = if self.rebinding == Some(button) {
                    "Press a key".to_string()
                } else {
                    self.bindings.key(button).name().to_string()
                };
                if ui.button(text).clicked() {
                    self.rebinding = Some(button);
                }
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Reset to defaults").clicked() {
                self.bindings = KeyBindings::default();
                self.rebinding = None;
            }
            if ui.button("Done").clicked() {
                self.showing_controls = false;
                self.rebinding = None;
            }
        });
        ui.add_space(12.0);
        let gamepad = self.gamepads.as_mut().and_then(Gamepads::name);
        ui.label(match gamepad {
            Some(name) => format!("Gamepad: {name}"),
            None => "No gamepad connected".to_string(),
        });
        ui.label("Gamepads use their D-pad or left stick, shoulder buttons, Start and Select.");
        ui.label("A and B are the right and bottom face buttons, as on a GBA.");
    }

    fn start_screen(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(48.0);
            ui.heading("GBADN Console");
            ui.add_space(8.0);
            ui.label("Drop a .fasta.gz cartridge here, or open one above.");
            ui.add_space(24.0);
            ui.label(RichText::new("Keys").strong());
            for (button, name, _) in BUTTONS {
                ui.label(format!("{name}: {}", self.bindings.key(button).name()));
            }
            ui.label("Save state: F5    Load state: F9");
        });
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

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(BINDINGS_SETTING, self.bindings.to_text());
    }
}

/// Reads the player's saved keys from the settings store.
pub fn saved_bindings(storage: Option<&dyn eframe::Storage>) -> String {
    storage
        .and_then(|s| s.get_string(BINDINGS_SETTING))
        .unwrap_or_default()
}
