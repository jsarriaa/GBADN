//! Writes a cartridge's genome-study tables, for `scripts/plot-study.py` to draw.

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use gbadn_codec::{decode_fasta, inspect_rom};
use gbadn_study::{BASES_PER_BYTE, Genome, KMER, base_counts, n50};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [cartridge, output] = arguments.as_slice() else {
        eprintln!("usage: gbadn-study <cartridge.fasta[.gz]> <output-folder>");
        return ExitCode::from(2);
    };
    match run(Path::new(cartridge), Path::new(output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("gbadn-study: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cartridge: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let rom = decode_fasta(&std::fs::read(cartridge)?)?;
    let info = inspect_rom(&rom)?;
    let genome = Genome::new(&rom);
    std::fs::create_dir_all(output)?;

    let (n50_bases, l50) = n50(&genome.contigs);
    let spectrum = genome.kmer_spectrum();
    let mut summary = String::from("field\tvalue\n");
    writeln!(summary, "title\t{}", info.title)?;
    writeln!(summary, "code\t{}", info.game_code)?;
    writeln!(summary, "rom_bytes\t{}", rom.len())?;
    writeln!(summary, "bases\t{}", rom.len() * BASES_PER_BYTE)?;
    writeln!(summary, "contigs\t{}", genome.contigs.len())?;
    writeln!(summary, "n50\t{n50_bases}")?;
    writeln!(summary, "l50\t{l50}")?;
    writeln!(summary, "k\t{KMER}")?;
    std::fs::write(output.join("summary.tsv"), summary)?;

    let mut table = String::from("chromosome\tbases\tA\tC\tG\tT\n");
    for (kind, bytes) in &genome.chromosomes {
        let [a, c, g, t] = base_counts(bytes);
        writeln!(
            table,
            "{}\t{}\t{a}\t{c}\t{g}\t{t}",
            kind.name(),
            bytes.len() * BASES_PER_BYTE
        )?;
    }
    std::fs::write(output.join("chromosomes.tsv"), table)?;

    let mut table = String::from("chromosome\trom_offset\tbases\n");
    for contig in &genome.contigs {
        writeln!(
            table,
            "{}\t{}\t{}",
            contig.kind.name(),
            contig.rom_offset,
            contig.bytes * BASES_PER_BYTE
        )?;
    }
    std::fs::write(output.join("contigs.tsv"), table)?;

    let mut table = String::from("chromosome\tstart\tend\tgc\tentropy\n");
    for window in genome.windows() {
        writeln!(
            table,
            "{}\t{}\t{}\t{:.4}\t{:.4}",
            window.kind.name(),
            window.start_base,
            window.end_base,
            window.gc,
            window.entropy
        )?;
    }
    std::fs::write(output.join("windows.tsv"), table)?;

    let mut table = String::from("first\tfirst_start\tsecond\tsecond_start\tbases\n");
    for repeat in genome.repeats() {
        writeln!(
            table,
            "{}\t{}\t{}\t{}\t{}",
            repeat.first_kind.name(),
            repeat.first_base,
            repeat.second_kind.name(),
            repeat.second_base,
            repeat.bases
        )?;
    }
    std::fs::write(output.join("repeats.tsv"), table)?;

    let mut table = String::from("multiplicity\tdistinct_kmers\n");
    for (multiplicity, distinct) in spectrum {
        writeln!(table, "{multiplicity}\t{distinct}")?;
    }
    std::fs::write(output.join("kmers.tsv"), table)?;
    Ok(())
}
