//! Drives the console window as a user would, without opening one on screen.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{DroppedFile, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gbadn_codec::{demo, encode};
use gbadn_emu::WIDTH;
use gbadn_emu::app::Console;

const TIMEOUT: Duration = Duration::from_secs(20);

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gbadn-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str, contents: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn console() -> Harness<'static, Console> {
    Harness::new_ui_state(
        |ui, console: &mut Console| console.show(ui),
        Console::new(None),
    )
}

fn drop_file(harness: &mut Harness<'_, Console>, path: &Path) {
    harness.input_mut().dropped_files.push(DroppedFile {
        path: Some(path.to_path_buf()),
        ..Default::default()
    });
    harness.step();
}

/// Steps the window in real time, because the emulator paces itself by the clock.
fn step_until(
    harness: &mut Harness<'_, Console>,
    mut done: impl FnMut(&Harness<'_, Console>) -> bool,
) {
    let start = Instant::now();
    while !done(harness) {
        assert!(
            start.elapsed() < TIMEOUT,
            "timed out waiting for the window"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn shows_colour(harness: &Harness<'_, Console>, expected: fn(u8, u8, u8) -> bool) -> bool {
    harness.state().screen().is_some_and(|screen| {
        let [r, g, b, _] = screen[80 * WIDTH + 120].to_le_bytes();
        expected(r, g, b)
    })
}

fn green(r: u8, g: u8, b: u8) -> bool {
    g > 200 && r < 16 && b < 16
}

fn red(r: u8, g: u8, b: u8) -> bool {
    r > 200 && g < 16 && b < 16
}

fn demo_cartridge(dir: &TempDir) -> PathBuf {
    dir.file("demo.fasta.gz", &encode(&demo::rom()).unwrap().fasta_gz)
}

#[test]
fn starts_with_no_cartridge_and_asks_for_one() {
    let mut harness = console();
    harness.step();
    assert!(
        harness
            .query_by_label_contains("Drop a .fasta.gz")
            .is_some()
    );
    assert!(harness.query_by_label("Open cartridge").is_some());
    assert!(harness.query_by_label("Close").is_none());
    assert!(harness.state().screen().is_none());
}

#[test]
fn plays_a_dropped_cartridge_and_reacts_to_the_keyboard() {
    let dir = TempDir::new("window-play");
    let mut harness = console();
    drop_file(&mut harness, &demo_cartridge(&dir));
    step_until(&mut harness, |h| shows_colour(h, green));
    assert!(harness.query_by_label("GBADN DEMO").is_some());

    harness.key_down(Key::Z);
    step_until(&mut harness, |h| shows_colour(h, red));
    harness.key_up(Key::Z);
    step_until(&mut harness, |h| shows_colour(h, green));
}

#[test]
fn close_returns_to_the_start_screen() {
    let dir = TempDir::new("window-close");
    let mut harness = console();
    drop_file(&mut harness, &demo_cartridge(&dir));
    step_until(&mut harness, |h| h.state().screen().is_some());

    harness.get_by_label("Close").click();
    step_until(&mut harness, |h| h.state().screen().is_none());
    harness.step();
    assert!(
        harness
            .query_by_label_contains("Drop a .fasta.gz")
            .is_some()
    );
}

#[test]
fn refuses_a_raw_rom_and_explains_why() {
    let dir = TempDir::new("window-refuse");
    let mut harness = console();
    drop_file(&mut harness, &dir.file("game.gba", &demo::rom()));
    step_until(&mut harness, |h| {
        h.query_by_label_contains("only reads FASTA").is_some()
    });
    assert!(harness.state().screen().is_none());
}

#[test]
fn a_refused_file_does_not_stop_the_running_game() {
    let dir = TempDir::new("window-keep");
    let mut harness = console();
    drop_file(&mut harness, &demo_cartridge(&dir));
    step_until(&mut harness, |h| h.state().screen().is_some());

    drop_file(&mut harness, &dir.file("real.fasta", b">chr1\nACGTNNNN\n"));
    step_until(&mut harness, |h| {
        h.query_by_label_contains("does not contain a GBADN")
            .is_some()
    });
    assert!(harness.state().screen().is_some());
    assert!(harness.query_by_label("GBADN DEMO").is_some());
}

#[test]
fn save_state_writes_a_file_beside_the_cartridge_and_load_restores_it() {
    let dir = TempDir::new("window-state");
    let cartridge = demo_cartridge(&dir);
    let mut harness = console();
    drop_file(&mut harness, &cartridge);
    step_until(&mut harness, |h| h.state().screen().is_some());

    harness.get_by_label("Load state").click();
    step_until(&mut harness, |h| {
        h.query_by_label_contains("no saved state").is_some()
    });

    harness.get_by_label("Save state").click();
    step_until(&mut harness, |h| h.query_by_label("State saved").is_some());
    assert!(dir.0.join("demo.state").exists());

    harness.get_by_label("Load state").click();
    step_until(&mut harness, |h| h.query_by_label("State loaded").is_some());
    assert!(harness.query_by_label_contains("no saved state").is_none());
}

#[test]
fn function_keys_save_and_load_states() {
    let dir = TempDir::new("window-state-keys");
    let mut harness = console();
    drop_file(&mut harness, &demo_cartridge(&dir));
    step_until(&mut harness, |h| h.state().screen().is_some());

    harness.key_press(Key::F5);
    step_until(&mut harness, |h| h.query_by_label("State saved").is_some());
    harness.key_press(Key::F9);
    step_until(&mut harness, |h| h.query_by_label("State loaded").is_some());
}

#[test]
fn a_damaged_state_is_refused_and_the_game_keeps_running() {
    let dir = TempDir::new("window-state-bad");
    let mut harness = console();
    drop_file(&mut harness, &demo_cartridge(&dir));
    step_until(&mut harness, |h| h.state().screen().is_some());

    dir.file("demo.state", b"not a state");
    harness.get_by_label("Load state").click();
    step_until(&mut harness, |h| {
        h.query_by_label_contains("damaged").is_some()
    });
    assert!(harness.state().screen().is_some());
}

fn open_controls(harness: &mut Harness<'_, Console>) {
    harness.get_by_label("Controls").click();
    step_until(harness, |h| h.query_by_label("Done").is_some());
}

fn close_controls(harness: &mut Harness<'_, Console>) {
    harness.get_by_label("Done").click();
    step_until(harness, |h| h.query_by_label("Done").is_none());
}

#[test]
fn a_remapped_key_plays_and_the_old_one_stops() {
    let dir = TempDir::new("window-remap");
    let mut harness = console();
    drop_file(&mut harness, &demo_cartridge(&dir));
    step_until(&mut harness, |h| shows_colour(h, green));

    open_controls(&mut harness);
    harness.get_by_label("Enter").click();
    step_until(&mut harness, |h| h.query_by_label("Press a key").is_some());
    harness.key_press(Key::Q);
    step_until(&mut harness, |h| h.query_by_label("Q").is_some());
    close_controls(&mut harness);

    harness.key_down(Key::Q);
    step_until(&mut harness, |h| shows_colour(h, red));
    harness.key_up(Key::Q);
    step_until(&mut harness, |h| shows_colour(h, green));

    harness.key_down(Key::Enter);
    for _ in 0..30 {
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(shows_colour(&harness, green));
}

#[test]
fn escape_cancels_and_reserved_keys_are_ignored_while_remapping() {
    let mut harness = console();
    harness.step();
    open_controls(&mut harness);
    harness.get_by_label("Enter").click();
    step_until(&mut harness, |h| h.query_by_label("Press a key").is_some());

    harness.key_press(Key::F5);
    harness.step();
    harness.step();
    assert!(harness.query_by_label("Press a key").is_some());

    harness.key_press(Key::Escape);
    step_until(&mut harness, |h| h.query_by_label("Press a key").is_none());
    assert!(harness.query_by_label("Enter").is_some());
}

#[test]
fn the_game_pauses_while_controls_are_open() {
    let dir = TempDir::new("window-pause");
    let mut harness = console();
    drop_file(&mut harness, &demo_cartridge(&dir));
    step_until(&mut harness, |h| shows_colour(h, green));

    open_controls(&mut harness);
    harness.key_down(Key::Z);
    for _ in 0..30 {
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(shows_colour(&harness, green));

    harness.key_up(Key::Z);
    close_controls(&mut harness);
    harness.key_down(Key::Z);
    step_until(&mut harness, |h| shows_colour(h, red));
}

#[test]
fn reset_restores_the_default_keys() {
    let mut harness = console();
    harness.step();
    open_controls(&mut harness);
    harness.get_by_label("Enter").click();
    step_until(&mut harness, |h| h.query_by_label("Press a key").is_some());
    harness.key_press(Key::Q);
    step_until(&mut harness, |h| h.query_by_label("Q").is_some());

    harness.get_by_label("Reset to defaults").click();
    step_until(&mut harness, |h| h.query_by_label("Enter").is_some());
}
