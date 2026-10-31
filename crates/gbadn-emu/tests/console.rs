//! Boots a FASTA cartridge on the real core, with no window.

use gbadn_codec::{decode_fasta, demo, encode};
use std::path::{Path, PathBuf};

use gbadn_emu::{Core, LoadError, WIDTH, keys, read_cartridge, save_path};

fn boot_demo() -> Core {
    let fasta = encode(&demo::rom()).unwrap().fasta;
    let rom = decode_fasta(fasta.as_bytes()).unwrap();
    Core::new(&rom, None).unwrap()
}

/// Returns the R, G and B bytes of the pixel in the middle of the screen.
fn centre_pixel(core: &Core) -> (u8, u8, u8) {
    let [r, g, b, _] = core.video()[80 * WIDTH + 120].to_le_bytes();
    (r, g, b)
}

#[test]
fn demo_cartridge_draws_green() {
    let mut core = boot_demo();
    for _ in 0..5 {
        core.run_frame(0);
    }
    let (r, g, b) = centre_pixel(&core);
    assert!(g > 200 && r < 16 && b < 16, "got {r} {g} {b}");
}

#[test]
fn demo_cartridge_reacts_to_buttons() {
    let mut core = boot_demo();
    for _ in 0..5 {
        core.run_frame(0);
    }
    for _ in 0..5 {
        core.run_frame(keys::A);
    }
    let (r, g, b) = centre_pixel(&core);
    assert!(r > 200 && g < 16 && b < 16, "got {r} {g} {b}");
}

#[test]
fn produces_audio_samples() {
    let mut core = boot_demo();
    let mut samples = vec![0i16; 4096];
    let mut total = 0;
    for _ in 0..10 {
        core.run_frame(0);
        total += core.read_audio(&mut samples);
    }
    assert!(total > 0);
}

#[test]
fn gzip_cartridges_load_from_disk() {
    let dir = std::env::temp_dir().join(format!("gbadn-gz-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("demo.fasta.gz");
    std::fs::write(&path, encode(&demo::rom()).unwrap().fasta_gz).unwrap();
    assert_eq!(read_cartridge(&path).unwrap(), demo::rom());

    // A gzip of a raw ROM is still not DNA, and neither is another extension.
    let renamed = dir.join("demo.gz");
    std::fs::write(&renamed, encode(&demo::rom()).unwrap().fasta_gz).unwrap();
    assert!(matches!(
        read_cartridge(&renamed),
        Err(LoadError::NotAFastaFile)
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn compressed_and_plain_cartridges_share_a_save() {
    let expected = PathBuf::from("/saves/game.sav");
    assert_eq!(save_path(Path::new("/saves/game.fasta")), expected);
    assert_eq!(save_path(Path::new("/saves/game.fasta.gz")), expected);
    assert_eq!(save_path(Path::new("/saves/game.FA.GZ")), expected);
}

#[test]
fn raw_rom_files_are_refused() {
    let dir = std::env::temp_dir().join(format!("gbadn-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("game.gba");
    std::fs::write(&path, demo::rom()).unwrap();
    assert!(matches!(
        read_cartridge(&path),
        Err(LoadError::NotAFastaFile)
    ));

    // Renaming a ROM does not make it DNA.
    let renamed = dir.join("game.fasta");
    std::fs::write(&renamed, demo::rom()).unwrap();
    assert!(matches!(
        read_cartridge(&renamed),
        Err(LoadError::Sequence(_))
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}
