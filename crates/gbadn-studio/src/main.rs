#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Color32, RichText};
use gbadn_codec::Encoded;

const PREVIEW_BASES: usize = 48;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 460.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "GBADN Studio",
        options,
        Box::new(|_| Ok(Box::<Studio>::default())),
    )
}

#[derive(Default)]
enum State {
    #[default]
    Empty,
    Encoding(PathBuf, Receiver<Result<Encoded, String>>),
    Ready(PathBuf, Box<Encoded>),
    Failed(String),
}

#[derive(Default)]
struct Studio {
    state: State,
    saved_to: Option<PathBuf>,
}

impl Studio {
    fn start(&mut self, path: PathBuf, ctx: &egui::Context) {
        let (sender, receiver) = channel();
        let worker_path = path.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::fs::read(&worker_path)
                .map_err(|e| e.to_string())
                .and_then(|rom| gbadn_codec::encode(&rom).map_err(|e| e.to_string()));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        self.saved_to = None;
        self.state = State::Encoding(path, receiver);
    }

    fn poll(&mut self) {
        if let State::Encoding(path, receiver) = &self.state
            && let Ok(result) = receiver.try_recv()
        {
            self.state = match result {
                Ok(encoded) => State::Ready(path.clone(), Box::new(encoded)),
                Err(message) => State::Failed(message),
            };
        }
    }

    fn save(&mut self) {
        let State::Ready(path, encoded) = &self.state else {
            return;
        };
        let name = Path::new(path.file_stem().unwrap_or_default()).with_extension("fasta.gz");
        let target = rfd::FileDialog::new()
            .set_file_name(name.to_string_lossy())
            .add_filter("Compressed FASTA", &["gz"])
            .add_filter("FASTA", &["fasta", "fa", "fna"])
            .save_file();
        let Some(target) = target else {
            return;
        };
        let is_gzip = target
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gz"));
        let contents = if is_gzip {
            encoded.fasta_gz.as_slice()
        } else {
            encoded.fasta.as_bytes()
        };
        match std::fs::write(&target, contents) {
            Ok(()) => self.saved_to = Some(target),
            Err(error) => self.state = State::Failed(format!("could not save: {error}")),
        }
    }
}

impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();

        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        let busy = matches!(self.state, State::Encoding(..));
        if let Some(path) = dropped
            && !busy
        {
            self.start(path, &ctx);
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("GBADN Studio");
            ui.label("Turn a Game Boy Advance ROM into a DNA sequence.");
            ui.add_space(12.0);

            let stroke = if hovering {
                Color32::LIGHT_GREEN
            } else {
                Color32::GRAY
            };
            egui::Frame::group(ui.style())
                .stroke(egui::Stroke::new(1.5, stroke))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.add_space(16.0);
                        ui.label("Drop a .gba file here");
                        let choose = ui.add_enabled(!busy, egui::Button::new("Choose a file"));
                        if choose.clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("GBA ROM", &["gba", "agb", "bin"])
                                .pick_file()
                        {
                            self.start(path, &ctx);
                        }
                        ui.add_space(16.0);
                    });
                });
            ui.add_space(12.0);

            let mut wants_save = false;
            match &self.state {
                State::Empty => {}
                State::Encoding(path, _) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Sequencing {}", display_name(path)));
                    });
                }
                State::Failed(message) => {
                    ui.colored_label(Color32::LIGHT_RED, message);
                }
                State::Ready(_, encoded) => {
                    report(ui, encoded);
                    ui.add_space(8.0);
                    wants_save = ui.button("Save cartridge").clicked();
                }
            }
            if wants_save {
                self.save();
            }
            if let Some(path) = &self.saved_to {
                ui.colored_label(Color32::LIGHT_GREEN, format!("Saved to {}", path.display()));
            }
        });
    }
}

fn report(ui: &mut egui::Ui, encoded: &Encoded) {
    let title = if encoded.info.title.is_empty() {
        "Untitled"
    } else {
        &encoded.info.title
    };
    ui.label(RichText::new(format!("{title}  ({})", encoded.info.game_code)).strong());
    egui::Grid::new("report").num_columns(2).show(ui, |ui| {
        let rows = [
            ("ROM", format_bytes(encoded.rom_size)),
            ("After zstd", format_bytes(encoded.payload_size)),
            ("FASTA", format_bytes(encoded.fasta.len())),
            ("FASTA + gzip", format_bytes(encoded.fasta_gz.len())),
            (
                "Sequence",
                format!(
                    "{} bases, {:.1}% GC",
                    encoded.bases,
                    encoded.gc_ratio * 100.0
                ),
            ),
            ("MD5", encoded.md5.clone()),
            ("As real DNA", format_mass(encoded.dna_mass_grams())),
        ];
        for (name, value) in rows {
            ui.label(name);
            ui.label(value);
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    let preview = encoded.fasta.lines().nth(1).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for base in preview.chars().take(PREVIEW_BASES) {
            ui.label(
                RichText::new(base.to_string())
                    .monospace()
                    .color(base_colour(base)),
            );
        }
    });
}

fn base_colour(base: char) -> Color32 {
    match base {
        'A' => Color32::from_rgb(90, 190, 110),
        'C' => Color32::from_rgb(90, 150, 230),
        'G' => Color32::from_rgb(230, 180, 70),
        _ => Color32::from_rgb(225, 95, 95),
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn format_bytes(bytes: usize) -> String {
    let bytes = bytes as f64;
    if bytes >= 1024.0 * 1024.0 {
        format!("{:.2} MiB", bytes / (1024.0 * 1024.0))
    } else {
        format!("{:.1} KiB", bytes / 1024.0)
    }
}

fn format_mass(grams: f64) -> String {
    if grams >= 1e-12 {
        format!("{:.1} picograms", grams * 1e12)
    } else if grams >= 1e-15 {
        format!("{:.1} femtograms", grams * 1e15)
    } else {
        format!("{:.1} attograms", grams * 1e18)
    }
}
