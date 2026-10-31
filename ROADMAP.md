# GBADN Roadmap

Goal: a GBA emulator that only reads FASTA genomic files, and a desktop UI that turns GBA ROMs into those files.

Current release: **v0.2.0**, for Linux, Windows, macOS and Android. Next step: browser version (M8).

Status: `[x]` meets the definition of done in [CLAUDE.md](CLAUDE.md). `[~]` written and partly verified, with what's missing noted. `[ ]` not started.

## Decisions

| Topic | Decision |
|---|---|
| Emulator | Wrap mGBA now; a from-scratch core is a long-term goal (M9) |
| Encoding | Chromosomes, zstd, then 2 bits per base, with an MD5 check; gzip over the FASTA |
| Platform | Linux, Windows, macOS and Android released; browser next (M8) |
| Repository | Going public on GitHub under the MIT license, hosting the browser version for free on Pages |

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

- [~] "Play" button in Studio: button and missing-console message tested; launching the real console not yet tried by hand
- [ ] Command-line tool
- [x] Remappable keys, remembered between sessions
- [~] Gamepad support: button mapping tested; not yet tried with a physical gamepad
- [x] Save states

## M5: Releases

- [x] Linux build: tests pass in CI, and the packaged console ran the demo
- [~] Windows and macOS (Apple Silicon) builds: tests pass in CI; not yet opened by hand on those systems
- [x] Version tags publish packaged downloads
- [x] MIT license

## M6: Android

- [x] Build the console as an arm64 APK in CI, including mGBA for Android
- [~] On-screen touch controls: holding one button works on a real phone; several fingers tested only in unit tests
- [x] Open cartridges through Android's file picker, with saves in private storage
- [x] Demo cartridge runs on a real phone (OnePlus 13R, Android 16), including save and load state
- [ ] Play a real game on the phone, with sound, by hand
- [ ] Check that a quick tap on the toolbar registers: a very short synthetic tap didn't, a 150 ms press did
- [x] Lasting signing key, so updates install over older versions
- [x] APK in releases, from v0.2.0

Conversion stays on desktop: Android only plays cartridges.

## M7: Genome features (ideas)

- [ ] Saves stored as FASTA too
- [ ] Mutation mode that corrupts games on purpose
- [ ] Synthesis-ready encoding: no long single-base runs, balanced GC, error correction

## M8: Browser version (next)

- [ ] Console and converter in the browser through WebAssembly
- [ ] Free hosting on GitHub Pages
- [ ] Make the repository public

## M9: Our own emulator core

- [ ] Define a `Core` interface around the current API (`new`, `run_frame`, `video`, `read_audio`)
- [ ] Build a from-scratch Rust core, using the mGBA tests as its benchmark
