//! Splits a ROM into chromosomes by what its bytes appear to be.
//!
//! A GBA ROM has no table of contents, so apart from the header and empty space
//! these are best guesses from the content. A wrong guess only changes which
//! chromosome holds a byte: the layout table always rebuilds the exact ROM.

/// The first bytes of every cartridge, at a fixed place.
const HEADER_SIZE: usize = 0xC0;
const BLOCK_SIZE: usize = 256;
/// Shorter runs are absorbed by their neighbours, which keeps the layout table small.
const MIN_RUN_BLOCKS: usize = 4;

const LZ77_TAG: u8 = 0x10;
const LZ77_MIN_SIZE: usize = 64;
const LZ77_MAX_SIZE: usize = 1024 * 1024;
const TILE_BYTES: usize = 32;

const WAVE_HEADER_SIZE: usize = 16;
const WAVE_LOOPING: u32 = 0x4000_0000;
const WAVE_MIN_SAMPLES: usize = 256;
const WAVE_MAX_SAMPLES: usize = 4 * 1024 * 1024;
/// Sound moves gradually: the average step between 8-bit samples stays small.
const WAVE_MAX_AVERAGE_STEP: u64 = 24;

/// Calls, stack frames and returns per block. Random data averages about one.
const THUMB_MIN_MARKERS: usize = 6;
const ARM_MIN_ALWAYS_WORDS: usize = 40;

/// What part of a game a chromosome holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Kind {
    Header = 0,
    Code = 1,
    Graphics = 2,
    Audio = 3,
    Data = 4,
    Empty = 5,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Header,
        Kind::Code,
        Kind::Graphics,
        Kind::Audio,
        Kind::Data,
        Kind::Empty,
    ];

    pub fn from_id(id: u8) -> Option<Kind> {
        Kind::ALL.get(id as usize).copied()
    }

    /// The FASTA record name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Header => "chrHeader",
            Kind::Code => "chrCode",
            Kind::Graphics => "chrGraphics",
            Kind::Audio => "chrAudio",
            Kind::Data => "chrData",
            Kind::Empty => "chrEmpty",
        }
    }

    /// What the chromosome holds, for people.
    pub fn description(self) -> &'static str {
        match self {
            Kind::Header => "Cartridge header",
            Kind::Code => "Code (game mechanics)",
            Kind::Graphics => "Compressed graphics",
            Kind::Audio => "Sampled audio",
            Kind::Data => "Other data (text, maps, music, raw art)",
            Kind::Empty => "Empty space",
        }
    }
}

/// Divides a ROM into consecutive `(kind, length)` runs covering every byte.
pub fn classify(rom: &[u8]) -> Vec<(Kind, usize)> {
    let header = rom.len().min(HEADER_SIZE);
    let mut runs = vec![(Kind::Header, header)];
    if rom.len() == header {
        return runs;
    }

    let blocks = rom.len().div_ceil(BLOCK_SIZE);
    let mut graphics = vec![0usize; blocks];
    let mut audio = vec![0usize; blocks];
    scan_structures(rom, &mut graphics, &mut audio);

    let mut kinds: Vec<Kind> = (0..blocks)
        .map(|index| {
            let start = (index * BLOCK_SIZE).max(header);
            let end = ((index + 1) * BLOCK_SIZE).min(rom.len());
            let block = &rom[start.min(end)..end];
            if audio[index] * 2 >= BLOCK_SIZE {
                Kind::Audio
            } else if graphics[index] * 2 >= BLOCK_SIZE {
                Kind::Graphics
            } else if is_empty(block) {
                Kind::Empty
            } else if is_code(block) {
                Kind::Code
            } else {
                Kind::Data
            }
        })
        .collect();
    absorb_short_runs(&mut kinds);

    let mut position = header;
    for (index, kind) in kinds.into_iter().enumerate() {
        let end = ((index + 1) * BLOCK_SIZE).min(rom.len());
        if end <= position {
            continue;
        }
        match runs.last_mut() {
            Some((last, length)) if *last == kind => *length += end - position,
            _ => runs.push((kind, end - position)),
        }
        position = end;
    }
    runs
}

/// Finds data whose own structure proves what it is, counting its bytes per block.
fn scan_structures(rom: &[u8], graphics: &mut [usize], audio: &mut [usize]) {
    let mut offset = HEADER_SIZE.next_multiple_of(4);
    while offset + WAVE_HEADER_SIZE <= rom.len() {
        if let Some(length) = wave_length(rom, offset) {
            add_coverage(audio, offset, length);
            offset += length.next_multiple_of(4);
        } else if let Some((length, size)) = lz77_stream(rom, offset) {
            if size.is_multiple_of(TILE_BYTES) {
                add_coverage(graphics, offset, length);
            }
            offset += length.next_multiple_of(4);
        } else {
            offset += 4;
        }
    }
}

fn add_coverage(coverage: &mut [usize], start: usize, length: usize) {
    let end = start + length;
    let mut position = start;
    while position < end {
        let block = position / BLOCK_SIZE;
        let block_end = ((block + 1) * BLOCK_SIZE).min(end);
        coverage[block] += block_end - position;
        position = block_end;
    }
}

/// The total length of a sound sample in the format most GBA games share, if one starts here.
fn wave_length(rom: &[u8], offset: usize) -> Option<usize> {
    let flags = read_u32(rom, offset);
    if flags != 0 && flags != WAVE_LOOPING {
        return None;
    }
    let pitch = read_u32(rom, offset + 4);
    let loop_start = read_u32(rom, offset + 8) as usize;
    let samples = read_u32(rom, offset + 12) as usize;
    if !(0x1000..=0x400_0000).contains(&pitch)
        || !(WAVE_MIN_SAMPLES..=WAVE_MAX_SAMPLES).contains(&samples)
        || loop_start > samples
    {
        return None;
    }
    let data = rom.get(offset + WAVE_HEADER_SIZE..offset + WAVE_HEADER_SIZE + samples)?;
    let total_step: u64 = data
        .windows(2)
        .map(|pair| (pair[0] as i8 as i64 - pair[1] as i8 as i64).unsigned_abs())
        .sum();
    (total_step / (samples as u64 - 1) <= WAVE_MAX_AVERAGE_STEP)
        .then_some(WAVE_HEADER_SIZE + samples)
}

/// Walks a BIOS LZ77 stream without unpacking it, returning its packed and unpacked lengths.
/// Random bytes almost never reach the declared size with every back-reference in range.
fn lz77_stream(rom: &[u8], offset: usize) -> Option<(usize, usize)> {
    if rom[offset] != LZ77_TAG {
        return None;
    }
    let size = rom[offset + 1] as usize
        | (rom[offset + 2] as usize) << 8
        | (rom[offset + 3] as usize) << 16;
    if !(LZ77_MIN_SIZE..=LZ77_MAX_SIZE).contains(&size) {
        return None;
    }
    let mut position = offset + 4;
    let mut produced = 0;
    while produced < size {
        let flags = *rom.get(position)?;
        position += 1;
        for bit in (0..8).rev() {
            if produced == size {
                break;
            }
            if flags >> bit & 1 == 0 {
                rom.get(position)?;
                position += 1;
                produced += 1;
            } else {
                let first = *rom.get(position)? as usize;
                let second = *rom.get(position + 1)? as usize;
                position += 2;
                let distance = ((first & 0xF) << 8 | second) + 1;
                if distance > produced {
                    return None;
                }
                produced += (first >> 4) + 3;
            }
        }
    }
    (produced == size).then_some((position - offset, size))
}

fn is_empty(block: &[u8]) -> bool {
    block
        .first()
        .is_none_or(|&b| (b == 0x00 || b == 0xFF) && block.iter().all(|&other| other == b))
}

fn is_code(block: &[u8]) -> bool {
    let halfwords: Vec<u16> = block
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let mut markers = 0;
    for (index, &word) in halfwords.iter().enumerate() {
        let is_call = (0xF000..0xF800).contains(&word)
            && halfwords.get(index + 1).is_some_and(|next| *next >= 0xF800);
        let is_frame = word >> 8 == 0xB5 || word >> 8 == 0xBD;
        if is_call || is_frame || word == 0x4770 {
            markers += 1;
        }
    }
    if markers >= THUMB_MIN_MARKERS {
        return true;
    }
    let always_words = block
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|word| word[3] >> 4 == 0xE)
        .count();
    always_words >= ARM_MIN_ALWAYS_WORDS
}

fn absorb_short_runs(kinds: &mut [Kind]) {
    let mut start = 0;
    while start < kinds.len() {
        let kind = kinds[start];
        let end = start + kinds[start..].iter().take_while(|k| **k == kind).count();
        if start > 0 && end - start < MIN_RUN_BLOCKS {
            let previous = kinds[start - 1];
            kinds[start..end].fill(previous);
        }
        start = end;
    }
}

fn read_u32(rom: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        rom[offset],
        rom[offset + 1],
        rom[offset + 2],
        rom[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROM_SIZE: usize = 64 * 1024;
    const REGION: usize = 8 * 1024;

    fn noise(size: usize, seed: u32) -> Vec<u8> {
        let mut state = seed | 1;
        (0..size)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect()
    }

    fn thumb_code(size: usize) -> Vec<u8> {
        // push, then calls and data moves, then pop: the shape of a small function.
        let function: [u16; 12] = [
            0xB510, 0x2001, 0xF000, 0xF810, 0x1C04, 0xF000, 0xF820, 0x4288, 0xD001, 0x2000, 0xBD10,
            0x4770,
        ];
        function
            .iter()
            .cycle()
            .take(size / 2)
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    /// A real LZ77 stream: literals to start, then long back-references.
    fn lz77_graphics(size: usize) -> Vec<u8> {
        let mut stream = vec![LZ77_TAG, size as u8, (size >> 8) as u8, (size >> 16) as u8];
        stream.push(0x00);
        stream.extend(noise(8, 3));
        let mut produced = 8;
        while produced < size {
            stream.push(0xFF);
            for _ in 0..8 {
                let length = (size - produced).clamp(3, 18);
                if produced < size {
                    stream.push(((length - 3) as u8) << 4);
                    stream.push(0x03);
                    produced += length;
                } else {
                    stream.extend([0, 0]);
                }
            }
        }
        stream
    }

    fn wave(samples: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(0x0020_0000u32.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.extend((samples as u32).to_le_bytes());
        bytes.extend((0..samples).map(|i| (((i as f64 / 20.0).sin() * 60.0) as i8) as u8));
        bytes
    }

    fn place(rom: &mut [u8], region: usize, bytes: &[u8]) {
        let start = region * REGION;
        rom[start..start + bytes.len()].copy_from_slice(bytes);
    }

    fn kinds_at(runs: &[(Kind, usize)], offset: usize) -> Kind {
        let mut position = 0;
        for &(kind, length) in runs {
            position += length;
            if offset < position {
                return kind;
            }
        }
        panic!("offset {offset} is outside the runs");
    }

    #[test]
    fn runs_cover_every_byte_exactly() {
        for size in [0, 100, 0xC0, 0xC1, 300, 4096, 4097, ROM_SIZE] {
            let rom = noise(size, 7);
            let total: usize = classify(&rom).iter().map(|(_, length)| length).sum();
            assert_eq!(total, size);
        }
    }

    #[test]
    fn recognises_each_kind_of_content() {
        let mut rom = noise(ROM_SIZE, 11);
        place(&mut rom, 1, &thumb_code(REGION));
        place(&mut rom, 2, &wave(REGION - 64));
        place(&mut rom, 4, &vec![0xFF; REGION]);
        let graphics = lz77_graphics(64 * 1024);
        assert!(graphics.len() < 2 * REGION);
        place(&mut rom, 5, &graphics);

        let runs = classify(&rom);
        assert_eq!(runs[0], (Kind::Header, HEADER_SIZE));
        assert_eq!(kinds_at(&runs, REGION + REGION / 2), Kind::Code);
        assert_eq!(kinds_at(&runs, 2 * REGION + REGION / 2), Kind::Audio);
        assert_eq!(kinds_at(&runs, 3 * REGION + REGION / 2), Kind::Data);
        assert_eq!(kinds_at(&runs, 4 * REGION + REGION / 2), Kind::Empty);
        assert_eq!(kinds_at(&runs, 5 * REGION + 512), Kind::Graphics);
    }

    #[test]
    fn noise_is_only_data() {
        let runs = classify(&noise(ROM_SIZE, 5));
        assert_eq!(runs[0].0, Kind::Header);
        assert_eq!(runs[1], (Kind::Data, ROM_SIZE - HEADER_SIZE));
    }

    #[test]
    fn lz77_rejects_out_of_range_references() {
        // Declares 64 bytes, then immediately refers to bytes that don't exist yet.
        let bytes = [LZ77_TAG, 64, 0, 0, 0xFF, 0x00, 0x00, 0x00, 0x00];
        let mut rom = bytes.to_vec();
        rom.resize(256, 0);
        assert_eq!(lz77_stream(&rom, 0), None);
    }

    #[test]
    fn wave_requires_gradual_samples() {
        let mut rom = wave(1024);
        assert_eq!(wave_length(&rom, 0), Some(1024 + WAVE_HEADER_SIZE));
        let harsh = noise(1024, 9);
        rom[WAVE_HEADER_SIZE..].copy_from_slice(&harsh);
        assert_eq!(wave_length(&rom, 0), None);
    }

    #[test]
    fn short_runs_join_their_neighbours() {
        let mut kinds = vec![
            Kind::Code,
            Kind::Code,
            Kind::Code,
            Kind::Code,
            Kind::Data,
            Kind::Code,
            Kind::Code,
            Kind::Code,
            Kind::Code,
        ];
        absorb_short_runs(&mut kinds);
        assert!(kinds.iter().all(|k| *k == Kind::Code));
    }
}
