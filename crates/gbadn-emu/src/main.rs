use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use gbadn_emu::{AUDIO_RATE, Core, HEIGHT, WIDTH, keys, read_cartridge};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::pixels::PixelFormatEnum;

const SCALE: u32 = 3;
/// The GBA refreshes at 16.777 MHz / 280896 cycles.
const FRAME_TIME: Duration = Duration::from_nanos(16_742_706);
/// Audio queued beyond this is dropped so sound never lags behind the picture.
const MAX_QUEUED_AUDIO_BYTES: u32 = 48000 / 5 * 4;

const KEYMAP: [(Scancode, u32); 10] = [
    (Scancode::Z, keys::A),
    (Scancode::X, keys::B),
    (Scancode::A, keys::L),
    (Scancode::S, keys::R),
    (Scancode::Return, keys::START),
    (Scancode::Backspace, keys::SELECT),
    (Scancode::Up, keys::UP),
    (Scancode::Down, keys::DOWN),
    (Scancode::Left, keys::LEFT),
    (Scancode::Right, keys::RIGHT),
];

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: gbadn-emu <cartridge.fasta>");
        return ExitCode::from(2);
    };
    match run(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("gbadn-emu: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(path: &Path) -> Result<(), String> {
    let rom = read_cartridge(path).map_err(|e| e.to_string())?;
    let title = gbadn_codec::inspect_rom(&rom).map_or_else(|_| "GBADN".into(), |i| i.title);
    let mut core = Core::new(&rom, Some(&path.with_extension("sav"))).map_err(|e| e.to_string())?;
    drop(rom);

    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let window = video
        .window(
            &format!("GBADN - {title}"),
            WIDTH as u32 * SCALE,
            HEIGHT as u32 * SCALE,
        )
        .position_centered()
        .resizable()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    canvas
        .set_logical_size(WIDTH as u32, HEIGHT as u32)
        .map_err(|e| e.to_string())?;
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::ABGR8888, WIDTH as u32, HEIGHT as u32)
        .map_err(|e| e.to_string())?;

    let audio = open_audio(&sdl);
    let mut samples = vec![0i16; 4096];
    let mut events = sdl.event_pump()?;
    let mut next_frame = Instant::now();

    loop {
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => return Ok(()),
                _ => {}
            }
        }

        let keyboard = events.keyboard_state();
        let pressed = KEYMAP
            .iter()
            .filter(|(scancode, _)| keyboard.is_scancode_pressed(*scancode))
            .fold(0, |mask, (_, key)| mask | key);
        core.run_frame(pressed);

        let frames = core.read_audio(&mut samples);
        if let Some(queue) = &audio
            && queue.size() < MAX_QUEUED_AUDIO_BYTES
        {
            queue.queue_audio(&samples[..frames * 2])?;
        }

        // SAFETY: u32 pixels are always valid as bytes, and the length covers the same memory.
        let pixels = unsafe {
            std::slice::from_raw_parts(core.video().as_ptr().cast::<u8>(), WIDTH * HEIGHT * 4)
        };
        texture
            .update(None, pixels, WIDTH * 4)
            .map_err(|e| e.to_string())?;
        canvas.clear();
        canvas.copy(&texture, None, None)?;
        canvas.present();

        next_frame += FRAME_TIME;
        let now = Instant::now();
        if next_frame > now {
            std::thread::sleep(next_frame - now);
        } else {
            next_frame = now;
        }
    }
}

/// A machine without sound can still play, so audio failures are not fatal.
fn open_audio(sdl: &sdl2::Sdl) -> Option<AudioQueue<i16>> {
    let desired = AudioSpecDesired {
        freq: Some(AUDIO_RATE),
        channels: Some(2),
        samples: Some(1024),
    };
    let queue = sdl
        .audio()
        .ok()?
        .open_queue::<i16, _>(None, &desired)
        .ok()?;
    queue.resume();
    Some(queue)
}
