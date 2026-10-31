# GBADN Roadmap

Goal: a GBA emulator that only reads FASTA genomic files, and a desktop UI that turns GBA ROMs into those files.

Status: `[x]` meets the definition of done in [CLAUDE.md](CLAUDE.md). `[~]` written and partly verified, with what's missing noted. `[ ]` not started.

## Decisions

| Topic | Decision |
|---|---|
| Emulator | Wrap mGBA now; a from-scratch core is a long-term goal (M8) |
| Encoding | Chromosomes, zstd, then 2 bits per base, with an MD5 check; gzip over the FASTA |
| Platform | Desktop first: Linux, then Windows and macOS; web later (M7) |
| Repository | Private on GitHub; license chosen before going public |

## M0: Foundations

- [x] Repository layout and Rust workspace, with mGBA pinned at 0.10.5
- [x] README, ROADMAP, CLAUDE.md, format specification
- [x] CI workflow, passing on GitHub
- [x] Private GitHub repository

## M1: Format

- [x] zstd and 2-bit encoding, with raw storage when zstd doesn't help
- [x] MD5 check; point mutations, truncation and non-cartridge FASTA are detected
- [x] Lowercase and CRLF sequences accepted
- [x] Original demo cartridge in `samples/demo.fasta`
- [x] Gzip-compressed cartridges (`.fasta.gz`), including `bgzip` files
- [x] Format 2: multi-record FASTA, split by content into chromosomes, still reading format 1
- [ ] Validate output with an external FASTA tool (samtools)
- [ ] Check chromosome labels against a game with a known layout

## M2: FASTA-only console

- [x] mGBA builds as a static library through Cargo
- [x] Headless tests: boots a FASTA cartridge, draws the expected colours, reads buttons, produces audio
- [x] Refuses `.gba` files, renamed ROMs and real genomes with clear messages
- [~] Console window: opens empty, then takes a cartridge through a button or drag and drop, shows refusals, plays and closes. Tested by driving the UI; the file picker and on-screen picture not yet confirmed by eye
- [~] Keyboard and sound: keyboard tested through the window; sound not yet confirmed by ear
- [x] mGBA debug logs silenced
- [~] Battery saves: `.sav` file is created; save and reload not yet tested
- [ ] Play a real game from start to save and reload

## M3: Studio

- [~] Drag and drop, report and file writing tested by driving the UI; file picker and save dialog not yet tested automatically
- [ ] Convert a ROM in Studio and play the saved FASTA in the console
- [ ] Progress and responsiveness with a 32 MiB ROM

## M4: Everyday use

- [ ] "Play" button in Studio
- [ ] Command-line tool
- [ ] Gamepad support and remappable keys
- [x] Save states

## M5: Releases

- [ ] Windows and macOS builds in CI
- [ ] Packaged downloads
- [ ] Pick a license and decide on going public

## M6: Genome features (ideas)

- [ ] Saves stored as FASTA too
- [ ] Mutation mode that corrupts games on purpose
- [ ] Synthesis-ready encoding: no long single-base runs, balanced GC, error correction

## M7: Web version

- [ ] Both programs in the browser through WebAssembly

## M8: Our own emulator core

- [ ] Define a `Core` interface around the current API (`new`, `run_frame`, `video`, `read_audio`)
- [ ] Build a from-scratch Rust core, using the mGBA tests as its benchmark
