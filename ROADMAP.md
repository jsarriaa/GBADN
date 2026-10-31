# GBADN Roadmap

Goal: a GBA emulator that only reads FASTA genomic files, and a desktop UI that turns GBA ROMs into those files.

Status: `[x]` meets the definition of done in [CLAUDE.md](CLAUDE.md). `[~]` written and partly verified, with what's missing noted. `[ ]` not started.

## Decisions

| Topic | Decision |
|---|---|
| Emulator | Wrap mGBA now; a from-scratch core is a long-term goal (M8) |
| Encoding | zstd, then 2 bits per base, with an MD5 check |
| Platform | Desktop first: Linux, then Windows and macOS; web later (M7) |
| Repository | Private on GitHub; license chosen before going public |

## M0: Foundations

- [x] Repository layout and Rust workspace, with mGBA pinned at 0.10.5
- [x] README, ROADMAP, CLAUDE.md, format specification
- [x] CI workflow, passing on GitHub
- [x] Private GitHub repository

## M1: Format v1

- [x] zstd and 2-bit encoding, with raw storage when zstd doesn't help
- [x] MD5 check; point mutations, truncation and non-cartridge FASTA are detected
- [x] Lowercase and CRLF sequences accepted
- [x] Original demo cartridge in `samples/demo.fasta`
- [x] Gzip-compressed cartridges (`.fasta.gz`), including `bgzip` files
- [ ] Validate output with an external FASTA tool (samtools)

## M2: FASTA-only console

- [x] mGBA builds as a static library through Cargo
- [x] Headless tests: boots a FASTA cartridge, draws the expected colours, reads buttons, produces audio
- [x] Refuses `.gba` files, renamed ROMs and real genomes with clear messages
- [~] Window: stays open with the demo; picture not yet confirmed by eye
- [~] Keyboard and sound: core input and audio tested headlessly; not yet tried in the window
- [~] Battery saves: `.sav` file is created; save and reload not yet tested
- [ ] Play a real game from start to save and reload

## M3: Studio

- [~] Window launches; drag and drop, file picker, report and save written but not exercised
- [ ] Convert a ROM in Studio and play the saved FASTA in the console
- [ ] Progress and responsiveness with a 32 MiB ROM

## M4: Everyday use

- [ ] "Play" button in Studio
- [ ] Command-line tool
- [ ] Gamepad support and remappable keys
- [ ] Save states

## M5: Releases

- [ ] Windows and macOS builds in CI
- [ ] Packaged downloads
- [ ] Pick a license and decide on going public

## M6: Genome features (ideas)

- [ ] Saves stored as FASTA too
- [ ] ROM sections as separate chromosomes
- [ ] Mutation mode that corrupts games on purpose
- [ ] Synthesis-ready encoding: no long single-base runs, balanced GC, error correction

## M7: Web version

- [ ] Both programs in the browser through WebAssembly

## M8: Our own emulator core

- [ ] Define a `Core` interface around the current API (`new`, `run_frame`, `video`, `read_audio`)
- [ ] Build a from-scratch Rust core, using the mGBA tests as its benchmark
