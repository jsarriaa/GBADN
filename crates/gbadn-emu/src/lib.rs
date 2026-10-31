//! The GBADN console: mGBA behind a loader that only accepts FASTA cartridges.

use std::ffi::{CString, c_char, c_int};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use thiserror::Error;

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
pub const AUDIO_RATE: i32 = 48000;

const ACCEPTED_EXTENSIONS: [&str; 3] = ["fasta", "fa", "fna"];

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("this console only reads FASTA genomic files (.fasta, .fa, .fna)")]
    NotAFastaFile,
    #[error("could not read {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error(transparent)]
    Sequence(#[from] gbadn_codec::Error),
    #[error("the sequence decoded correctly, but it is not a GBA program")]
    NotAGbaRom,
    #[error("the emulation core failed to start")]
    CoreFailed,
}

/// Reads a cartridge from a FASTA file and returns the ROM bytes, in memory only.
pub fn read_cartridge(path: &Path) -> Result<Vec<u8>, LoadError> {
    let accepted = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ACCEPTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if !accepted {
        return Err(LoadError::NotAFastaFile);
    }
    let text = std::fs::read(path).map_err(|e| LoadError::Io(path.to_path_buf(), e))?;
    Ok(gbadn_codec::decode_fasta(&text)?)
}

/// GBA buttons, as bits of the mask passed to [`Core::run_frame`].
pub mod keys {
    pub const A: u32 = 1 << 0;
    pub const B: u32 = 1 << 1;
    pub const SELECT: u32 = 1 << 2;
    pub const START: u32 = 1 << 3;
    pub const RIGHT: u32 = 1 << 4;
    pub const LEFT: u32 = 1 << 5;
    pub const UP: u32 = 1 << 6;
    pub const DOWN: u32 = 1 << 7;
    pub const R: u32 = 1 << 8;
    pub const L: u32 = 1 << 9;
}

#[repr(C)]
struct RawCore {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn gbadn_core_new(
        rom: *const u8,
        size: usize,
        save_path: *const c_char,
        status: *mut c_int,
    ) -> *mut RawCore;
    fn gbadn_core_free(core: *mut RawCore);
    fn gbadn_core_run_frame(core: *mut RawCore, keys: u32);
    fn gbadn_core_video(core: *const RawCore) -> *const u32;
    fn gbadn_core_read_audio(core: *mut RawCore, stereo: *mut i16, max_frames: usize) -> usize;
}

/// A running console.
pub struct Core(NonNull<RawCore>);

impl Core {
    /// Boots a decoded ROM. `save_path` holds the battery save, if any.
    pub fn new(rom: &[u8], save_path: Option<&Path>) -> Result<Self, LoadError> {
        let save = save_path
            .and_then(|p| p.to_str())
            .and_then(|p| CString::new(p).ok());
        let save_ptr = save.as_ref().map_or(std::ptr::null(), |p| p.as_ptr());
        let mut status: c_int = 0;
        // SAFETY: the pointers are valid for this call and the shim copies what it keeps.
        let raw = unsafe { gbadn_core_new(rom.as_ptr(), rom.len(), save_ptr, &mut status) };
        NonNull::new(raw).map(Core).ok_or(match status {
            1 => LoadError::NotAGbaRom,
            _ => LoadError::CoreFailed,
        })
    }

    pub fn run_frame(&mut self, keys: u32) {
        // SAFETY: self.0 is a live core, exclusively borrowed.
        unsafe { gbadn_core_run_frame(self.0.as_ptr(), keys) }
    }

    /// The current frame, as bytes in R, G, B, X order.
    pub fn video(&self) -> &[u32] {
        // SAFETY: the shim owns a WIDTH * HEIGHT buffer that lives as long as the core.
        unsafe { std::slice::from_raw_parts(gbadn_core_video(self.0.as_ptr()), WIDTH * HEIGHT) }
    }

    /// Drains pending audio into interleaved stereo samples, returning the frame count.
    pub fn read_audio(&mut self, stereo: &mut [i16]) -> usize {
        // SAFETY: the shim writes at most stereo.len() / 2 frames.
        unsafe { gbadn_core_read_audio(self.0.as_ptr(), stereo.as_mut_ptr(), stereo.len() / 2) }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        // SAFETY: self.0 came from gbadn_core_new and is freed once.
        unsafe { gbadn_core_free(self.0.as_ptr()) }
    }
}
