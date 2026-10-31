//! The codec as plain C functions, which the browser page calls through WebAssembly.
//!
//! Each call leaves its bytes and message here for the page to read. Only one
//! call runs at a time, because a page instance has a single thread.

use std::sync::Mutex;

use gbadn_codec::{Encoded, decode_fasta, encode};

const OK: i32 = 0;
const FAILED: i32 = 1;

#[derive(Default)]
struct Outcome {
    bytes: Vec<u8>,
    message: String,
}

static OUTCOME: Mutex<Outcome> = Mutex::new(Outcome {
    bytes: Vec::new(),
    message: String::new(),
});

fn finish(result: Result<(Vec<u8>, String), gbadn_codec::Error>) -> i32 {
    let mut outcome = OUTCOME.lock().unwrap();
    match result {
        Ok((bytes, message)) => {
            *outcome = Outcome { bytes, message };
            OK
        }
        Err(error) => {
            *outcome = Outcome {
                bytes: Vec::new(),
                message: error.to_string(),
            };
            FAILED
        }
    }
}

/// Decodes a cartridge. On success the bytes are the ROM.
///
/// # Safety
/// `data` must point to `size` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gbadn_web_decode(data: *const u8, size: usize) -> i32 {
    // SAFETY: the caller promises the range is readable.
    let input = unsafe { std::slice::from_raw_parts(data, size) };
    finish(decode_fasta(input).map(|rom| {
        let title = gbadn_codec::inspect_rom(&rom)
            .map(|info| info.title)
            .unwrap_or_default();
        (rom, title)
    }))
}

/// Encodes a ROM. On success the bytes are the `.fasta.gz`, and the message is a JSON report.
///
/// # Safety
/// `data` must point to `size` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gbadn_web_encode(data: *const u8, size: usize) -> i32 {
    // SAFETY: the caller promises the range is readable.
    let input = unsafe { std::slice::from_raw_parts(data, size) };
    finish(encode(input).map(|encoded| {
        let report = report_json(&encoded);
        (encoded.fasta_gz, report)
    }))
}

fn report_json(encoded: &Encoded) -> String {
    let chromosomes: Vec<String> = encoded
        .chromosomes
        .iter()
        .map(|c| {
            format!(
                r#"{{"name":"{}","description":"{}","romBytes":{}}}"#,
                c.kind.name(),
                c.kind.description(),
                c.rom_bytes
            )
        })
        .collect();
    format!(
        r#"{{"title":"{}","code":"{}","romBytes":{},"fastaBytes":{},"gzipBytes":{},"bases":{},"gc":{},"md5":"{}","chromosomes":[{}]}}"#,
        json_text(&encoded.info.title),
        json_text(&encoded.info.game_code),
        encoded.rom_size,
        encoded.fasta.len(),
        encoded.fasta_gz.len(),
        encoded.bases,
        encoded.gc_ratio,
        encoded.md5,
        chromosomes.join(",")
    )
}

fn json_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[unsafe(no_mangle)]
pub extern "C" fn gbadn_web_bytes() -> *const u8 {
    OUTCOME.lock().unwrap().bytes.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn gbadn_web_bytes_size() -> usize {
    OUTCOME.lock().unwrap().bytes.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn gbadn_web_message() -> *const u8 {
    OUTCOME.lock().unwrap().message.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn gbadn_web_message_size() -> usize {
    OUTCOME.lock().unwrap().message.len()
}

#[cfg(test)]
mod tests {
    use gbadn_codec::demo;

    use super::*;

    /// The outcome is shared, so one test walks through every call in order.
    #[test]
    fn calls_leave_their_results_to_read() {
        let rom = demo::rom();
        assert_eq!(unsafe { gbadn_web_encode(rom.as_ptr(), rom.len()) }, OK);
        let cartridge = read_bytes();
        let report = read_message();
        assert_eq!(&cartridge[..2], &[0x1F, 0x8B]);
        assert!(report.contains(r#""title":"GBADN DEMO""#));
        assert!(report.contains(r#""name":"chrHeader""#));
        assert!(report.contains(&format!(r#""gzipBytes":{}"#, cartridge.len())));

        let decoded = unsafe { gbadn_web_decode(cartridge.as_ptr(), cartridge.len()) };
        assert_eq!(decoded, OK);
        assert_eq!(read_bytes(), rom);
        assert_eq!(read_message(), "GBADN DEMO");

        let not_fasta = b"not a cartridge";
        let failed = unsafe { gbadn_web_decode(not_fasta.as_ptr(), not_fasta.len()) };
        assert_eq!(failed, FAILED);
        assert!(read_bytes().is_empty());
        assert_eq!(read_message(), "not a FASTA file");

        let refused = unsafe { gbadn_web_encode(not_fasta.as_ptr(), not_fasta.len()) };
        assert_eq!(refused, FAILED);
        assert!(read_message().contains("not a GBA ROM"));
    }

    fn read_bytes() -> Vec<u8> {
        unsafe { std::slice::from_raw_parts(gbadn_web_bytes(), gbadn_web_bytes_size()) }.to_vec()
    }

    fn read_message() -> String {
        let bytes =
            unsafe { std::slice::from_raw_parts(gbadn_web_message(), gbadn_web_message_size()) };
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn report_text_is_escaped() {
        assert_eq!(json_text(r#"a"b\c"#), r#"a\"b\\c"#);
    }
}
