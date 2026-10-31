//! The GBADN format: a GBA ROM stored as a valid FASTA DNA sequence.
//! See `docs/FORMAT.md` for the specification.

pub mod demo;

use md5::{Digest, Md5};
use thiserror::Error;

/// Largest cartridge the GBA address space can map.
pub const MAX_ROM_SIZE: usize = 32 * 1024 * 1024;
const MIN_ROM_SIZE: usize = 0xC0;
const MAGIC: &[u8; 4] = b"GBDN";
const VERSION: u8 = 1;
const CONTAINER_HEADER_SIZE: usize = 32;
const LINE_WIDTH: usize = 70;
const BASES: [u8; 4] = *b"ACGT";
/// Average mass of one single-stranded DNA nucleotide (about 330 Da).
const GRAMS_PER_BASE: f64 = 5.48e-22;

const COMPRESSION_NONE: u8 = 0;
const COMPRESSION_ZSTD: u8 = 1;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("not a GBA ROM: {0}")]
    NotARom(&'static str),
    #[error("not a FASTA file")]
    NotFasta,
    #[error("this FASTA does not contain a GBADN cartridge")]
    NotGbadn,
    #[error("unsupported GBADN version {0}")]
    UnsupportedVersion(u8),
    #[error("invalid base {0:?} on line {1}: only A, C, G and T can be decoded")]
    InvalidBase(char, usize),
    #[error("sequence is corrupted: {0}")]
    Corrupted(&'static str),
    #[error("sequence checksum mismatch: the DNA has mutated")]
    ChecksumMismatch,
    #[error("compression failed: {0}")]
    Compression(String),
}

/// Identity read from a ROM's cartridge header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomInfo {
    pub title: String,
    pub game_code: String,
}

/// Checks the cartridge header and extracts its identity.
pub fn inspect_rom(rom: &[u8]) -> Result<RomInfo, Error> {
    if rom.len() < MIN_ROM_SIZE {
        return Err(Error::NotARom("file is smaller than a cartridge header"));
    }
    if rom.len() > MAX_ROM_SIZE {
        return Err(Error::NotARom("file is larger than 32 MiB"));
    }
    if rom[0xB2] != 0x96 {
        return Err(Error::NotARom("missing the cartridge header signature"));
    }
    Ok(RomInfo {
        title: header_text(&rom[0xA0..0xAC]),
        game_code: header_text(&rom[0xAC..0xB0]),
    })
}

fn header_text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|&&b| b != 0)
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|&b| b as char)
        .collect::<String>()
        .trim()
        .to_string()
}

/// The result of encoding a ROM, with the figures the UI reports.
#[derive(Debug, Clone)]
pub struct Encoded {
    pub fasta: String,
    pub info: RomInfo,
    pub rom_size: usize,
    pub payload_size: usize,
    pub bases: usize,
    pub gc_ratio: f64,
    pub md5: String,
}

impl Encoded {
    /// Mass of the sequence if it were synthesized as single-stranded DNA.
    pub fn dna_mass_grams(&self) -> f64 {
        self.bases as f64 * GRAMS_PER_BASE
    }
}

/// Encodes a ROM as FASTA text, and proves the result decodes back to the same bytes.
pub fn encode(rom: &[u8]) -> Result<Encoded, Error> {
    let info = inspect_rom(rom)?;
    let digest: [u8; 16] = Md5::digest(rom).into();

    let compressed =
        zstd::bulk::compress(rom, 19).map_err(|e| Error::Compression(e.to_string()))?;
    let (compression, payload) = if compressed.len() < rom.len() {
        (COMPRESSION_ZSTD, compressed.as_slice())
    } else {
        (COMPRESSION_NONE, rom)
    };

    let mut container = Vec::with_capacity(CONTAINER_HEADER_SIZE + payload.len());
    container.extend_from_slice(MAGIC);
    container.push(VERSION);
    container.push(compression);
    container.extend_from_slice(&[0, 0]);
    container.extend_from_slice(&(rom.len() as u32).to_le_bytes());
    container.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    container.extend_from_slice(&digest);
    container.extend_from_slice(payload);

    let bases = container.len() * 4;
    let md5 = hex(&digest);
    let mut fasta = String::with_capacity(bases + bases / LINE_WIDTH + 256);
    fasta.push_str(&format!(
        ">GBADN1 title=\"{}\" code={} rom_bytes={} md5={}\n",
        info.title.replace('"', "'"),
        if info.game_code.is_empty() {
            "NONE"
        } else {
            &info.game_code
        },
        rom.len(),
        md5,
    ));

    let mut gc = 0usize;
    let mut column = 0;
    for byte in &container {
        for shift in [6, 4, 2, 0] {
            let code = (byte >> shift) & 0b11;
            gc += usize::from(code == 1 || code == 2);
            fasta.push(BASES[code as usize] as char);
            column += 1;
            if column == LINE_WIDTH {
                fasta.push('\n');
                column = 0;
            }
        }
    }
    if column != 0 {
        fasta.push('\n');
    }

    if decode_fasta(fasta.as_bytes())? != rom {
        return Err(Error::Corrupted("round trip verification failed"));
    }

    Ok(Encoded {
        fasta,
        info,
        rom_size: rom.len(),
        payload_size: payload.len(),
        bases,
        gc_ratio: gc as f64 / bases as f64,
        md5,
    })
}

/// Decodes GBADN FASTA text back into the ROM bytes, in memory.
pub fn decode_fasta(text: &[u8]) -> Result<Vec<u8>, Error> {
    let container = read_sequence(text)?;
    if container.len() < CONTAINER_HEADER_SIZE || &container[..4] != MAGIC {
        return Err(Error::NotGbadn);
    }
    if container[4] != VERSION {
        return Err(Error::UnsupportedVersion(container[4]));
    }
    let compression = container[5];
    let rom_size = u32::from_le_bytes(container[8..12].try_into().unwrap()) as usize;
    let payload_size = u32::from_le_bytes(container[12..16].try_into().unwrap()) as usize;
    let expected_digest = &container[16..CONTAINER_HEADER_SIZE];
    let payload = &container[CONTAINER_HEADER_SIZE..];

    if rom_size > MAX_ROM_SIZE {
        return Err(Error::Corrupted("declared ROM size is larger than 32 MiB"));
    }
    if payload.len() != payload_size {
        return Err(Error::Corrupted(
            "sequence length does not match its header",
        ));
    }

    let rom = match compression {
        COMPRESSION_NONE => payload.to_vec(),
        COMPRESSION_ZSTD => {
            zstd::bulk::decompress(payload, rom_size).map_err(|_| Error::ChecksumMismatch)?
        }
        _ => return Err(Error::Corrupted("unknown compression method")),
    };
    if rom.len() != rom_size {
        return Err(Error::Corrupted("decoded size does not match its header"));
    }
    if Md5::digest(&rom).as_slice() != expected_digest {
        return Err(Error::ChecksumMismatch);
    }
    Ok(rom)
}

/// Reads the single FASTA record into bytes, four bases per byte.
fn read_sequence(text: &[u8]) -> Result<Vec<u8>, Error> {
    if text.first() != Some(&b'>') {
        return Err(Error::NotFasta);
    }
    // A raw payload never exceeds the ROM limit, so this also bounds real genomes.
    let max_bases = (CONTAINER_HEADER_SIZE + MAX_ROM_SIZE) * 4;

    let mut bytes = Vec::with_capacity(text.len() / 4);
    let (mut current, mut count) = (0u8, 0usize);
    for (index, line) in text.split(|&b| b == b'\n').enumerate() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if index == 0 {
            continue;
        }
        if line.first() == Some(&b'>') {
            return Err(Error::NotGbadn);
        }
        for &base in line {
            let code = match base.to_ascii_uppercase() {
                b'A' => 0,
                b'C' => 1,
                b'G' => 2,
                b'T' => 3,
                // Real genomes commonly contain IUPAC codes, and they are not cartridges.
                _ if count < CONTAINER_HEADER_SIZE * 4 => return Err(Error::NotGbadn),
                other => return Err(Error::InvalidBase(other as char, index + 1)),
            };
            current = (current << 2) | code;
            count += 1;
            if count > max_bases {
                return Err(Error::NotGbadn);
            }
            if count % 4 == 0 {
                bytes.push(current);
                if bytes.len() == 4 && &bytes[..4] != MAGIC {
                    return Err(Error::NotGbadn);
                }
            }
        }
    }
    if count % 4 != 0 {
        return Err(Error::Corrupted(
            "sequence length is not a whole number of bytes",
        ));
    }
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MD5 of the demo ROM, computed outside this codebase.
    const DEMO_ROM_MD5: &str = "fed2821399a5d92a58313b784b1a3808";

    fn fake_rom(size: usize, noisy: bool) -> Vec<u8> {
        let mut state = 0x1234_5678u32;
        let mut rom: Vec<u8> = (0..size)
            .map(|i| {
                if noisy {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                } else {
                    (i % 7) as u8
                }
            })
            .collect();
        rom[0xA0..0xAC].copy_from_slice(b"GBADN TEST\0\0");
        rom[0xAC..0xB0].copy_from_slice(b"GDNA");
        rom[0xB2] = 0x96;
        rom
    }

    #[test]
    fn round_trips_compressible_and_incompressible_roms() {
        for noisy in [false, true] {
            let rom = fake_rom(64 * 1024, noisy);
            let encoded = encode(&rom).unwrap();
            assert_eq!(decode_fasta(encoded.fasta.as_bytes()).unwrap(), rom);
            assert_eq!(encoded.info.title, "GBADN TEST");
            assert_eq!(encoded.info.game_code, "GDNA");
        }
    }

    #[test]
    fn incompressible_rom_is_stored_raw() {
        let rom = fake_rom(64 * 1024, true);
        assert_eq!(encode(&rom).unwrap().payload_size, rom.len());
    }

    #[test]
    fn container_layout_is_stable() {
        let rom = demo::rom();
        let encoded = encode(&rom).unwrap();
        assert_eq!(encoded.md5, DEMO_ROM_MD5);
        assert!(encoded.fasta.starts_with(&format!(
            ">GBADN1 title=\"GBADN DEMO\" code=GDNA rom_bytes=1024 md5={DEMO_ROM_MD5}\n"
        )));

        let container = read_sequence(encoded.fasta.as_bytes()).unwrap();
        assert_eq!(container.len(), 32 + encoded.payload_size);
        assert_eq!(&container[..4], b"GBDN");
        assert_eq!(container[4], 1);
        assert_eq!(container[5], COMPRESSION_ZSTD);
        assert_eq!(&container[6..8], &[0, 0]);
        assert_eq!(&container[8..12], &1024u32.to_le_bytes());
        assert_eq!(
            &container[12..16],
            &(encoded.payload_size as u32).to_le_bytes()
        );
        assert_eq!(hex(&container[16..32]), DEMO_ROM_MD5);
    }

    #[test]
    fn output_is_strict_fasta() {
        let encoded = encode(&fake_rom(4096, true)).unwrap();
        let mut lines = encoded.fasta.lines();
        assert!(lines.next().unwrap().starts_with(">GBADN1 "));
        let body: Vec<&str> = lines.collect();
        for line in &body[..body.len() - 1] {
            assert_eq!(line.len(), LINE_WIDTH);
        }
        assert!(body.iter().all(|l| l.bytes().all(|b| b"ACGT".contains(&b))));
        assert_eq!(body.iter().map(|l| l.len()).sum::<usize>(), encoded.bases);
    }

    #[test]
    fn accepts_lowercase_and_crlf() {
        let rom = fake_rom(1024, true);
        let fasta = encode(&rom).unwrap().fasta;
        let (header, body) = fasta.split_once('\n').unwrap();
        let altered = format!("{header}\r\n{}", body.to_lowercase().replace('\n', "\r\n"));
        assert_eq!(decode_fasta(altered.as_bytes()).unwrap(), rom);
    }

    #[test]
    fn detects_a_point_mutation() {
        let fasta = encode(&fake_rom(4096, true)).unwrap().fasta;
        let mut bytes = fasta.into_bytes();
        let position = bytes.len() - 100;
        bytes[position] = if bytes[position] == b'A' { b'C' } else { b'A' };
        assert_eq!(decode_fasta(&bytes), Err(Error::ChecksumMismatch));
    }

    #[test]
    fn detects_a_mutation_in_compressed_payload() {
        let fasta = encode(&fake_rom(64 * 1024, false)).unwrap().fasta;
        let mut bytes = fasta.into_bytes();
        let position = bytes.len() - 100;
        bytes[position] = if bytes[position] == b'A' { b'C' } else { b'A' };
        assert!(decode_fasta(&bytes).is_err());
    }

    #[test]
    fn rejects_truncated_sequences() {
        let fasta = encode(&fake_rom(4096, true)).unwrap().fasta;
        let cut = &fasta.as_bytes()[..fasta.len() - 71];
        assert!(decode_fasta(cut).is_err());
    }

    #[test]
    fn rejects_everything_that_is_not_a_cartridge() {
        let rom = fake_rom(1024, true);
        assert_eq!(decode_fasta(&rom), Err(Error::NotFasta));
        assert_eq!(
            decode_fasta(b">chr1\nACGTACGTACGTACGT\n"),
            Err(Error::NotGbadn)
        );
        assert_eq!(decode_fasta(b">chr1\nNNNNNNNNACGT\n"), Err(Error::NotGbadn));
        assert_eq!(decode_fasta(b">empty\n"), Err(Error::NotGbadn));
    }

    #[test]
    fn rejects_files_that_are_not_roms() {
        assert!(matches!(encode(b"hello"), Err(Error::NotARom(_))));
        assert!(matches!(encode(&vec![0; 1024]), Err(Error::NotARom(_))));
    }

    #[test]
    fn demo_rom_round_trips() {
        let rom = demo::rom();
        assert_eq!(
            decode_fasta(encode(&rom).unwrap().fasta.as_bytes()).unwrap(),
            rom
        );
    }
}
