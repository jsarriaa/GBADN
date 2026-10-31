//! Genome-study measurements of a decoded ROM.
//!
//! Everything here reads the ROM's raw bytes at 4 bases per byte, not the
//! zstd-compressed text in the FASTA file, which looks like random sequence.
//! Results are numbers and coordinates only, so they can be published without
//! publishing any of the game.

use std::collections::{BTreeMap, HashMap};

use gbadn_codec::chromosomes::{Kind, classify};

pub const BASES_PER_BYTE: usize = 4;
pub const WINDOW_BYTES: usize = 25_000;
pub const KMER: usize = 21;

const REPEAT_BLOCK: usize = 512;
const REPEAT_STEP: usize = 4;
/// Blocks with fewer distinct bytes are padding, not duplicated content.
const REPEAT_MIN_DISTINCT: usize = 16;

/// A ROM split into chromosomes, each holding its runs joined in ROM order.
pub struct Genome {
    pub chromosomes: Vec<(Kind, Vec<u8>)>,
    pub contigs: Vec<Contig>,
}

/// One consecutive run of the ROM with a single kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contig {
    pub kind: Kind,
    pub rom_offset: usize,
    pub bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub kind: Kind,
    pub start_base: usize,
    pub end_base: usize,
    pub gc: f64,
    /// Shannon entropy of the window's bytes, from 0 to 8 bits.
    pub entropy: f64,
}

/// A stretch of a chromosome repeated exactly at a later place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeat {
    pub first_kind: Kind,
    pub first_base: usize,
    pub second_kind: Kind,
    pub second_base: usize,
    pub bases: usize,
}

impl Genome {
    pub fn new(rom: &[u8]) -> Genome {
        let mut parts: BTreeMap<Kind, Vec<u8>> = BTreeMap::new();
        let mut contigs = Vec::new();
        let mut offset = 0;
        for (kind, bytes) in classify(rom) {
            parts
                .entry(kind)
                .or_default()
                .extend_from_slice(&rom[offset..offset + bytes]);
            contigs.push(Contig {
                kind,
                rom_offset: offset,
                bytes,
            });
            offset += bytes;
        }
        Genome {
            chromosomes: parts.into_iter().collect(),
            contigs,
        }
    }

    pub fn windows(&self) -> Vec<Window> {
        let mut windows = Vec::new();
        for (kind, bytes) in &self.chromosomes {
            for (index, window) in bytes.chunks(WINDOW_BYTES).enumerate() {
                let start = index * WINDOW_BYTES * BASES_PER_BYTE;
                windows.push(Window {
                    kind: *kind,
                    start_base: start,
                    end_base: start + window.len() * BASES_PER_BYTE,
                    gc: gc_ratio(window),
                    entropy: entropy(window),
                });
            }
        }
        windows
    }

    /// Finds repeats between and within chromosomes, leaving out empty space.
    pub fn repeats(&self) -> Vec<Repeat> {
        let mut genome = Vec::new();
        let mut spans = Vec::new();
        for (kind, bytes) in &self.chromosomes {
            if *kind != Kind::Empty {
                spans.push((*kind, genome.len(), genome.len() + bytes.len()));
                genome.extend_from_slice(bytes);
            }
        }
        let span_of = |position: usize| {
            spans
                .iter()
                .find(|(_, start, end)| (*start..*end).contains(&position))
                .copied()
                .expect("every position belongs to a chromosome")
        };

        let mut repeats = Vec::new();
        let mut seen: HashMap<u64, usize> = HashMap::new();
        let mut next_indexed = 0;
        let mut position = 0;
        while position + REPEAT_BLOCK <= genome.len() {
            while next_indexed + REPEAT_BLOCK <= position {
                let block = &genome[next_indexed..next_indexed + REPEAT_BLOCK];
                seen.entry(block_hash(block)).or_insert(next_indexed);
                next_indexed += REPEAT_BLOCK;
            }
            let block = &genome[position..position + REPEAT_BLOCK];
            let earlier = seen
                .get(&block_hash(block))
                .copied()
                .filter(|&earlier| {
                    genome[earlier..earlier + REPEAT_BLOCK] == *block && is_complex(block)
                })
                .filter(|&earlier| {
                    span_of(earlier).2 >= earlier + REPEAT_BLOCK
                        && span_of(position).2 >= position + REPEAT_BLOCK
                });
            let Some(earlier) = earlier else {
                position += REPEAT_STEP;
                continue;
            };

            let (first_kind, first_start, first_end) = span_of(earlier);
            let (second_kind, second_start, second_end) = span_of(position);
            let limit = (first_end - earlier).min(second_end - position);
            let length = (0..limit)
                .take_while(|&offset| genome[earlier + offset] == genome[position + offset])
                .count();
            repeats.push(Repeat {
                first_kind,
                first_base: (earlier - first_start) * BASES_PER_BYTE,
                second_kind,
                second_base: (position - second_start) * BASES_PER_BYTE,
                bases: length * BASES_PER_BYTE,
            });
            position += length.next_multiple_of(REPEAT_STEP);
        }
        repeats
    }

    /// Counts how many distinct canonical k-mers occur each number of times,
    /// leaving out empty space, whose few k-mers would swamp everything else.
    pub fn kmer_spectrum(&self) -> BTreeMap<u32, u64> {
        let mask = (1u64 << (2 * KMER)) - 1;
        let mut kmers = Vec::new();
        for (kind, bytes) in &self.chromosomes {
            if *kind == Kind::Empty {
                continue;
            }
            kmers.reserve(bytes.len() * BASES_PER_BYTE);
            let (mut forward, mut reverse, mut length) = (0u64, 0u64, 0);
            for byte in bytes {
                for shift in [6, 4, 2, 0] {
                    let base = u64::from(byte >> shift & 3);
                    forward = (forward << 2 | base) & mask;
                    reverse = reverse >> 2 | (3 - base) << (2 * (KMER - 1));
                    length += 1;
                    if length >= KMER {
                        kmers.push(forward.min(reverse));
                    }
                }
            }
        }
        kmers.sort_unstable();

        let mut spectrum = BTreeMap::new();
        for run in kmers.chunk_by(|a, b| a == b) {
            *spectrum.entry(run.len() as u32).or_insert(0) += 1;
        }
        spectrum
    }
}

/// How many times each of A, C, G and T occurs, in that order.
pub fn base_counts(bytes: &[u8]) -> [u64; 4] {
    let mut counts = [0; 4];
    for byte in bytes {
        for shift in [6, 4, 2, 0] {
            counts[usize::from(byte >> shift & 3)] += 1;
        }
    }
    counts
}

pub fn gc_ratio(bytes: &[u8]) -> f64 {
    let [a, c, g, t] = base_counts(bytes);
    let total = a + c + g + t;
    if total == 0 {
        0.0
    } else {
        (c + g) as f64 / total as f64
    }
}

pub fn entropy(bytes: &[u8]) -> f64 {
    let mut counts = [0u32; 256];
    for byte in bytes {
        counts[usize::from(*byte)] += 1;
    }
    let total = bytes.len() as f64;
    counts
        .iter()
        .filter(|count| **count > 0)
        .map(|count| {
            let share = f64::from(*count) / total;
            -share * share.log2()
        })
        .sum()
}

/// The contig length at which contigs this long or longer hold half the assembly,
/// and how many contigs that takes.
pub fn n50(contigs: &[Contig]) -> (usize, usize) {
    let mut lengths: Vec<usize> = contigs
        .iter()
        .map(|contig| contig.bytes * BASES_PER_BYTE)
        .collect();
    lengths.sort_unstable_by(|a, b| b.cmp(a));
    let total: usize = lengths.iter().sum();
    let mut covered = 0;
    for (index, length) in lengths.iter().enumerate() {
        covered += length;
        if covered * 2 >= total {
            return (*length, index + 1);
        }
    }
    (0, 0)
}

fn block_hash(block: &[u8]) -> u64 {
    block.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn is_complex(block: &[u8]) -> bool {
    let mut present = [false; 256];
    for byte in block {
        present[usize::from(*byte)] = true;
    }
    present.iter().filter(|seen| **seen).count() >= REPEAT_MIN_DISTINCT
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes that never repeat a 512-byte block and have many distinct values.
    fn varied(seed: u32, length: usize) -> Vec<u8> {
        let mut state = seed.wrapping_mul(2_654_435_761) | 1;
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect()
    }

    fn genome_of(chromosomes: Vec<(Kind, Vec<u8>)>) -> Genome {
        Genome {
            chromosomes,
            contigs: Vec::new(),
        }
    }

    #[test]
    fn counts_each_base_of_a_byte() {
        // 0x1B is 00 01 10 11: one of each.
        assert_eq!(base_counts(&[0x1B]), [1, 1, 1, 1]);
        assert_eq!(base_counts(&[0x00]), [4, 0, 0, 0]);
        assert_eq!(gc_ratio(&[0x1B]), 0.5);
        assert_eq!(gc_ratio(&[0xFF]), 0.0);
        assert_eq!(gc_ratio(&[0x55]), 1.0);
        assert_eq!(gc_ratio(&[]), 0.0);
    }

    #[test]
    fn entropy_spans_from_uniform_to_every_value() {
        assert_eq!(entropy(&[7; 100]), 0.0);
        let every_value: Vec<u8> = (0..=255).collect();
        assert!((entropy(&every_value) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn n50_needs_half_the_assembly() {
        let contig = |bytes| Contig {
            kind: Kind::Data,
            rom_offset: 0,
            bytes,
        };
        let contigs = [contig(10), contig(5), contig(3), contig(2)];
        assert_eq!(n50(&contigs), (40, 1));
        assert_eq!(n50(&contigs[1..]), (20, 1));
        assert_eq!(n50(&[]), (0, 0));
    }

    #[test]
    fn the_demo_cartridge_splits_into_its_whole_rom() {
        let rom = gbadn_codec::demo::rom();
        let genome = Genome::new(&rom);
        let total: usize = genome.chromosomes.iter().map(|(_, b)| b.len()).sum();
        assert_eq!(total, rom.len());
        assert_eq!(genome.contigs.iter().map(|c| c.bytes).sum::<usize>(), total);

        let windows = genome.windows();
        let covered: usize = windows.iter().map(|w| w.end_base - w.start_base).sum();
        assert_eq!(covered, rom.len() * BASES_PER_BYTE);
    }

    #[test]
    fn finds_a_repeat_across_chromosomes_at_its_real_coordinates() {
        let shared = varied(1, 2_048);
        let mut code = varied(2, 1_024);
        code.extend_from_slice(&shared);
        let mut data = varied(3, 400);
        data.extend_from_slice(&shared);
        data.extend_from_slice(&varied(4, 1_000));

        let repeats = genome_of(vec![(Kind::Code, code), (Kind::Data, data)]).repeats();
        assert_eq!(repeats.len(), 1, "{repeats:?}");
        let repeat = repeats[0];
        assert_eq!(repeat.first_kind, Kind::Code);
        assert_eq!(repeat.second_kind, Kind::Data);
        assert_eq!(repeat.second_base, 400 * BASES_PER_BYTE);
        // The search starts on a block boundary, so it may find less than the whole copy.
        assert!(repeat.bases >= (2_048 - 2 * REPEAT_BLOCK) * BASES_PER_BYTE);
    }

    #[test]
    fn padding_and_empty_space_are_not_repeats() {
        let genome = genome_of(vec![
            (Kind::Data, vec![0xAB; 4_096]),
            (Kind::Empty, varied(1, 4_096)),
        ]);
        assert!(genome.repeats().is_empty());
    }

    #[test]
    fn kmers_count_a_sequence_and_its_reverse_complement_together() {
        // 0x1B is ACGT, its own reverse complement, so a long run has few distinct k-mers.
        let spectrum = genome_of(vec![(Kind::Data, vec![0x1B; 100])]).kmer_spectrum();
        let total: u64 = spectrum.iter().map(|(n, k)| u64::from(*n) * k).sum();
        assert_eq!(total, (100 * BASES_PER_BYTE - KMER + 1) as u64);
        assert_eq!(spectrum.values().sum::<u64>(), 2);
    }

    #[test]
    fn the_kmer_spectrum_leaves_out_empty_space() {
        let spectrum = genome_of(vec![(Kind::Empty, vec![0xFF; 100])]).kmer_spectrum();
        assert!(spectrum.is_empty());
    }
}
