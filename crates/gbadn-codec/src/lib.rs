//! The GBADN format: a GBA ROM stored as a valid FASTA DNA sequence.
//! See `docs/FORMAT.md` for the specification.

pub mod chromosomes;
pub mod demo;

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use md5::{Digest, Md5};
use thiserror::Error;

use crate::chromosomes::Kind;

/// Largest cartridge the GBA address space can map.
pub const MAX_ROM_SIZE: usize = 32 * 1024 * 1024;
const MIN_ROM_SIZE: usize = 0xC0;
const MAGIC: &[u8; 4] = b"GBDN";
const VERSION: u8 = 2;
const VERSION_SINGLE_RECORD: u8 = 1;
const CONTAINER_HEADER_SIZE: usize = 32;
const LINE_WIDTH: usize = 70;
const BASES: [u8; 4] = *b"ACGT";
const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];
/// Upper bound on the bases of any cartridge; also bounds real genomes.
const MAX_BASES: usize = (MAX_ROM_SIZE + 1024 * 1024) * 4;
/// Upper bound on FASTA text, allowing for short lines, so a small gzip can't fill memory.
const MAX_FASTA_SIZE: usize = MAX_BASES * 2 + 4096;
/// Average mass of one single-stranded DNA nucleotide (about 330 Da).
const GRAMS_PER_BASE: f64 = 5.48e-22;

const SEGMENT_ENTRY_SIZE: usize = 5;
const CHROMOSOME_HEADER_SIZE: usize = 6;

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
    #[error("the gzip compression of this file is damaged")]
    DamagedGzip,
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

/// One chromosome of an encoded cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chromosome {
    pub kind: Kind,
    pub rom_bytes: usize,
    pub payload_size: usize,
}

/// The result of encoding a ROM, with the figures the UI reports.
#[derive(Debug, Clone)]
pub struct Encoded {
    pub fasta: String,
    /// The same FASTA, gzip-compressed: the form cartridges normally travel in.
    pub fasta_gz: Vec<u8>,
    pub info: RomInfo,
    pub rom_size: usize,
    pub payload_size: usize,
    pub chromosomes: Vec<Chromosome>,
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
    let md5 = hex(&digest);
    let segments = chromosomes::classify(rom);

    let mut layout =
        Vec::with_capacity(CONTAINER_HEADER_SIZE + segments.len() * SEGMENT_ENTRY_SIZE);
    layout.extend_from_slice(MAGIC);
    layout.extend_from_slice(&[VERSION, 0, 0, 0]);
    layout.extend_from_slice(&(rom.len() as u32).to_le_bytes());
    layout.extend_from_slice(&(segments.len() as u32).to_le_bytes());
    layout.extend_from_slice(&digest);
    for (kind, length) in &segments {
        layout.push(*kind as u8);
        layout.extend_from_slice(&(*length as u32).to_le_bytes());
    }

    let mut writer = FastaWriter::default();
    writer.record(
        &format!(
            "gbadn_layout format=GBADN2 title=\"{}\" code={} rom_bytes={} md5={}",
            info.title.replace('"', "'"),
            if info.game_code.is_empty() {
                "NONE"
            } else {
                &info.game_code
            },
            rom.len(),
            md5,
        ),
        &layout,
    );

    let mut chromosomes = Vec::new();
    for kind in Kind::ALL {
        let raw = gather(rom, &segments, kind);
        if raw.is_empty() {
            continue;
        }
        let compressed =
            zstd::bulk::compress(&raw, 19).map_err(|e| Error::Compression(e.to_string()))?;
        let (compression, payload) = if compressed.len() < raw.len() {
            (COMPRESSION_ZSTD, compressed.as_slice())
        } else {
            (COMPRESSION_NONE, raw.as_slice())
        };

        let mut record = Vec::with_capacity(CHROMOSOME_HEADER_SIZE + payload.len());
        record.extend_from_slice(&[kind as u8, compression]);
        record.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        record.extend_from_slice(payload);
        writer.record(&format!("{} rom_bytes={}", kind.name(), raw.len()), &record);
        chromosomes.push(Chromosome {
            kind,
            rom_bytes: raw.len(),
            payload_size: payload.len(),
        });
    }

    let fasta_gz = gzip(writer.fasta.as_bytes())?;
    if decode_fasta(&fasta_gz)? != rom {
        return Err(Error::Corrupted("round trip verification failed"));
    }

    Ok(Encoded {
        gc_ratio: writer.gc as f64 / writer.bases as f64,
        bases: writer.bases,
        fasta: writer.fasta,
        fasta_gz,
        info,
        rom_size: rom.len(),
        payload_size: chromosomes.iter().map(|c| c.payload_size).sum(),
        chromosomes,
        md5,
    })
}

fn gather(rom: &[u8], segments: &[(Kind, usize)], wanted: Kind) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut position = 0;
    for &(kind, length) in segments {
        if kind == wanted {
            bytes.extend_from_slice(&rom[position..position + length]);
        }
        position += length;
    }
    bytes
}

#[derive(Default)]
struct FastaWriter {
    fasta: String,
    bases: usize,
    gc: usize,
}

impl FastaWriter {
    fn record(&mut self, header: &str, bytes: &[u8]) {
        self.fasta
            .reserve(bytes.len() * 4 + bytes.len() * 4 / LINE_WIDTH + 128);
        self.fasta.push('>');
        self.fasta.push_str(header);
        self.fasta.push('\n');
        let mut column = 0;
        for byte in bytes {
            for shift in [6, 4, 2, 0] {
                let code = (byte >> shift) & 0b11;
                self.gc += usize::from(code == 1 || code == 2);
                self.fasta.push(BASES[code as usize] as char);
                column += 1;
                if column == LINE_WIDTH {
                    self.fasta.push('\n');
                    column = 0;
                }
            }
        }
        if column != 0 {
            self.fasta.push('\n');
        }
        self.bases += bytes.len() * 4;
    }
}

fn gzip(text: &[u8]) -> Result<Vec<u8>, Error> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder
        .write_all(text)
        .and_then(|()| encoder.finish())
        .map_err(|e| Error::Compression(e.to_string()))
}

/// Unpacks gzip data, including the multi-member files `bgzip` writes.
fn gunzip(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut text = Vec::new();
    MultiGzDecoder::new(data)
        .take(MAX_FASTA_SIZE as u64 + 1)
        .read_to_end(&mut text)
        .map_err(|_| Error::DamagedGzip)?;
    if text.len() > MAX_FASTA_SIZE {
        return Err(Error::NotGbadn);
    }
    Ok(text)
}

/// Decodes a GBADN cartridge back into the ROM bytes, in memory.
/// Accepts FASTA text, or the same text gzip-compressed.
pub fn decode_fasta(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.starts_with(&GZIP_MAGIC) {
        return decode_text(&gunzip(data)?);
    }
    decode_text(data)
}

fn decode_text(text: &[u8]) -> Result<Vec<u8>, Error> {
    let records = read_records(text)?;
    let first = &records[0];
    if first.len() < CONTAINER_HEADER_SIZE || &first[..4] != MAGIC {
        return Err(Error::NotGbadn);
    }
    match first[4] {
        VERSION => decode_chromosomes(&records),
        VERSION_SINGLE_RECORD if records.len() == 1 => decode_single_record(first),
        VERSION_SINGLE_RECORD => Err(Error::NotGbadn),
        other => Err(Error::UnsupportedVersion(other)),
    }
}

/// Format 2: a layout record, then one record per chromosome.
fn decode_chromosomes(records: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let layout = &records[0];
    let rom_size = read_size(layout, 8);
    let segment_count = read_size(layout, 12);
    let expected_digest = &layout[16..CONTAINER_HEADER_SIZE];
    if rom_size > MAX_ROM_SIZE {
        return Err(Error::Corrupted("declared ROM size is larger than 32 MiB"));
    }
    if (layout.len() - CONTAINER_HEADER_SIZE) / SEGMENT_ENTRY_SIZE != segment_count
        || !(layout.len() - CONTAINER_HEADER_SIZE).is_multiple_of(SEGMENT_ENTRY_SIZE)
    {
        return Err(Error::Corrupted("layout length does not match its header"));
    }

    let mut chromosomes: [Option<Vec<u8>>; Kind::ALL.len()] = Default::default();
    for record in &records[1..] {
        if record.len() < CHROMOSOME_HEADER_SIZE {
            return Err(Error::Corrupted("chromosome is too short"));
        }
        let kind = Kind::from_id(record[0]).ok_or(Error::Corrupted("unknown chromosome"))?;
        let raw_size = read_size(record, 2);
        if raw_size > rom_size {
            return Err(Error::Corrupted("chromosome is larger than its ROM"));
        }
        let raw = unpack(record[1], &record[CHROMOSOME_HEADER_SIZE..], raw_size)?;
        if chromosomes[kind as usize].replace(raw).is_some() {
            return Err(Error::Corrupted("chromosome appears twice"));
        }
    }

    let mut rom = Vec::with_capacity(rom_size);
    let mut positions = [0usize; Kind::ALL.len()];
    for entry in layout[CONTAINER_HEADER_SIZE..].chunks_exact(SEGMENT_ENTRY_SIZE) {
        let kind = Kind::from_id(entry[0]).ok_or(Error::Corrupted("unknown chromosome"))?;
        let length = read_size(entry, 1);
        let position = positions[kind as usize];
        let end = position
            .checked_add(length)
            .ok_or(Error::Corrupted("layout is out of range"))?;
        let segment = chromosomes[kind as usize]
            .as_deref()
            .and_then(|bytes| bytes.get(position..end))
            .ok_or(Error::Corrupted("layout refers to missing bases"))?;
        if rom.len() + length > rom_size {
            return Err(Error::Corrupted("layout is longer than its ROM"));
        }
        rom.extend_from_slice(segment);
        positions[kind as usize] = end;
    }
    if rom.len() != rom_size {
        return Err(Error::Corrupted("decoded size does not match its header"));
    }
    if Md5::digest(&rom).as_slice() != expected_digest {
        return Err(Error::ChecksumMismatch);
    }
    Ok(rom)
}

/// Format 1: the whole ROM in one record.
fn decode_single_record(container: &[u8]) -> Result<Vec<u8>, Error> {
    let compression = container[5];
    let rom_size = read_size(container, 8);
    let payload_size = read_size(container, 12);
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

    let rom = unpack(compression, payload, rom_size)?;
    if Md5::digest(&rom).as_slice() != expected_digest {
        return Err(Error::ChecksumMismatch);
    }
    Ok(rom)
}

fn unpack(compression: u8, payload: &[u8], size: usize) -> Result<Vec<u8>, Error> {
    let bytes = match compression {
        COMPRESSION_NONE => payload.to_vec(),
        COMPRESSION_ZSTD => {
            zstd::bulk::decompress(payload, size).map_err(|_| Error::ChecksumMismatch)?
        }
        _ => return Err(Error::Corrupted("unknown compression method")),
    };
    if bytes.len() != size {
        return Err(Error::Corrupted("decoded size does not match its header"));
    }
    Ok(bytes)
}

fn read_size(bytes: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

/// Reads every FASTA record into bytes, four bases per byte.
fn read_records(text: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    if text.first() != Some(&b'>') {
        return Err(Error::NotFasta);
    }

    let mut records = Vec::new();
    let mut bytes = Vec::new();
    let (mut current, mut count, mut total) = (0u8, 0usize, 0usize);
    for (index, line) in text.split(|&b| b == b'\n').enumerate() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.first() == Some(&b'>') {
            if index > 0 {
                finish_record(&mut records, &mut bytes, &mut count)?;
            }
            continue;
        }
        let in_first_header = records.is_empty() && count < CONTAINER_HEADER_SIZE * 4;
        for &base in line {
            let code = match base.to_ascii_uppercase() {
                b'A' => 0,
                b'C' => 1,
                b'G' => 2,
                b'T' => 3,
                // Real genomes commonly contain IUPAC codes, and they are not cartridges.
                _ if in_first_header => return Err(Error::NotGbadn),
                other => return Err(Error::InvalidBase(other as char, index + 1)),
            };
            current = (current << 2) | code;
            count += 1;
            total += 1;
            if total > MAX_BASES {
                return Err(Error::NotGbadn);
            }
            if count % 4 == 0 {
                bytes.push(current);
                if records.is_empty() && bytes.len() == 4 && &bytes[..4] != MAGIC {
                    return Err(Error::NotGbadn);
                }
            }
        }
    }
    finish_record(&mut records, &mut bytes, &mut count)?;
    Ok(records)
}

fn finish_record(
    records: &mut Vec<Vec<u8>>,
    bytes: &mut Vec<u8>,
    count: &mut usize,
) -> Result<(), Error> {
    if !count.is_multiple_of(4) {
        return Err(Error::Corrupted(
            "sequence length is not a whole number of bytes",
        ));
    }
    records.push(std::mem::take(bytes));
    *count = 0;
    Ok(())
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
    fn layout_is_stable() {
        let rom = demo::rom();
        let encoded = encode(&rom).unwrap();
        assert_eq!(encoded.md5, DEMO_ROM_MD5);
        assert!(encoded.fasta.starts_with(&format!(
            ">gbadn_layout format=GBADN2 title=\"GBADN DEMO\" code=GDNA rom_bytes=1024 md5={DEMO_ROM_MD5}\n"
        )));

        let records = read_records(encoded.fasta.as_bytes()).unwrap();
        let layout = &records[0];
        assert_eq!(&layout[..4], b"GBDN");
        assert_eq!(layout[4], 2);
        assert_eq!(&layout[8..12], &1024u32.to_le_bytes());
        assert_eq!(hex(&layout[16..32]), DEMO_ROM_MD5);
        let segments = read_size(layout, 12);
        assert_eq!(layout.len(), 32 + segments * 5);
        // The header is always the first segment, at its fixed size.
        assert_eq!(layout[32], Kind::Header as u8);
        assert_eq!(read_size(layout, 33), 0xC0);
        assert_eq!(records.len(), encoded.chromosomes.len() + 1);
    }

    #[test]
    fn splits_into_named_chromosomes() {
        let mut rom = fake_rom(64 * 1024, true);
        rom[32 * 1024..48 * 1024].fill(0xFF);
        let encoded = encode(&rom).unwrap();

        let names: Vec<&str> = encoded
            .fasta
            .lines()
            .filter_map(|line| line.strip_prefix('>'))
            .map(|line| line.split(' ').next().unwrap())
            .collect();
        assert_eq!(names, ["gbadn_layout", "chrHeader", "chrData", "chrEmpty"]);
        let total: usize = encoded.chromosomes.iter().map(|c| c.rom_bytes).sum();
        assert_eq!(total, rom.len());
        assert_eq!(decode_fasta(encoded.fasta.as_bytes()).unwrap(), rom);
    }

    #[test]
    fn still_reads_format_1_cartridges() {
        // Written in full here, because the encoder no longer produces this format.
        let rom = fake_rom(1024, true);
        let mut container = Vec::new();
        container.extend_from_slice(b"GBDN");
        container.extend_from_slice(&[VERSION_SINGLE_RECORD, COMPRESSION_NONE, 0, 0]);
        container.extend_from_slice(&(rom.len() as u32).to_le_bytes());
        container.extend_from_slice(&(rom.len() as u32).to_le_bytes());
        container.extend_from_slice(&Md5::digest(&rom));
        container.extend_from_slice(&rom);
        let mut writer = FastaWriter::default();
        writer.record("GBADN1", &container);
        assert_eq!(decode_fasta(writer.fasta.as_bytes()).unwrap(), rom);

        // Format 1 is one record, so a second one is not a cartridge.
        writer.record("chr2", &[0; 4]);
        assert_eq!(decode_fasta(writer.fasta.as_bytes()), Err(Error::NotGbadn));
    }

    #[test]
    fn rejects_damaged_layouts_and_chromosomes() {
        let rom = fake_rom(4096, true);
        let encoded = encode(&rom).unwrap();
        let records = read_records(encoded.fasta.as_bytes()).unwrap();
        let rewrite = |change: &dyn Fn(&mut Vec<Vec<u8>>)| {
            let mut records = records.clone();
            change(&mut records);
            let mut writer = FastaWriter::default();
            for record in &records {
                writer.record("x", record);
            }
            decode_fasta(writer.fasta.as_bytes())
        };

        assert!(rewrite(&|_| ()).is_ok());
        assert!(
            rewrite(&|r| {
                r.pop();
            })
            .is_err()
        );
        assert!(rewrite(&|r| r.push(r[1].clone())).is_err());
        assert!(rewrite(&|r| r[0].truncate(34)).is_err());
        assert!(rewrite(&|r| r[0][8..12].copy_from_slice(&u32::MAX.to_le_bytes())).is_err());
        assert!(rewrite(&|r| r[0][33..37].copy_from_slice(&u32::MAX.to_le_bytes())).is_err());
        assert!(rewrite(&|r| r[0][32] = 200).is_err());
        assert!(rewrite(&|r| r[1][0] = 200).is_err());
        assert!(rewrite(&|r| r[1][1] = 9).is_err());
        assert!(rewrite(&|r| r[1].truncate(2)).is_err());
    }

    #[test]
    fn output_is_strict_fasta() {
        let encoded = encode(&fake_rom(4096, true)).unwrap();
        let mut bases = 0;
        let mut record: Vec<&str> = Vec::new();
        let mut check = |lines: &[&str]| {
            if let Some((last, full)) = lines.split_last() {
                assert!(full.iter().all(|line| line.len() == LINE_WIDTH));
                assert!(last.len() <= LINE_WIDTH);
            }
            bases += lines.iter().map(|line| line.len()).sum::<usize>();
        };
        for line in encoded.fasta.lines() {
            if line.starts_with('>') {
                check(&record);
                record.clear();
            } else {
                assert!(line.bytes().all(|b| b"ACGT".contains(&b)));
                record.push(line);
            }
        }
        check(&record);
        assert_eq!(bases, encoded.bases);
    }

    #[test]
    fn accepts_lowercase_and_crlf() {
        let rom = fake_rom(1024, true);
        let fasta = encode(&rom).unwrap().fasta;
        let altered: String = fasta
            .lines()
            .map(|line| {
                if line.starts_with('>') {
                    format!("{line}\r\n")
                } else {
                    format!("{}\r\n", line.to_lowercase())
                }
            })
            .collect();
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
    fn decodes_gzip_cartridges() {
        let rom = fake_rom(64 * 1024, false);
        let encoded = encode(&rom).unwrap();
        assert_eq!(&encoded.fasta_gz[..2], &GZIP_MAGIC);
        assert!(encoded.fasta_gz.len() < encoded.fasta.len());
        assert_eq!(decode_fasta(&encoded.fasta_gz).unwrap(), rom);
    }

    #[test]
    fn decodes_multi_member_gzip_as_bgzip_writes_it() {
        let rom = fake_rom(4096, true);
        let text = encode(&rom).unwrap().fasta.into_bytes();
        let (first, second) = text.split_at(text.len() / 2);
        let mut members = gzip(first).unwrap();
        members.extend(gzip(second).unwrap());
        assert_eq!(decode_fasta(&members).unwrap(), rom);
    }

    #[test]
    fn rejects_damaged_gzip() {
        let mut data = encode(&fake_rom(4096, true)).unwrap().fasta_gz;
        data.truncate(data.len() / 2);
        assert_eq!(decode_fasta(&data), Err(Error::DamagedGzip));
    }

    #[test]
    fn rejects_gzip_that_expands_without_limit() {
        // Far past the limit, while staying cheap to compress.
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        let chunk = vec![b'A'; 1024 * 1024];
        encoder.write_all(b">big\n").unwrap();
        for _ in 0..(MAX_FASTA_SIZE / chunk.len() + 2) {
            encoder.write_all(&chunk).unwrap();
        }
        let bomb = encoder.finish().unwrap();
        assert_eq!(decode_fasta(&bomb), Err(Error::NotGbadn));
    }

    #[test]
    fn rejects_gzip_of_something_else() {
        let rom = fake_rom(1024, true);
        assert_eq!(decode_fasta(&gzip(&rom).unwrap()), Err(Error::NotFasta));
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
