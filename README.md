# GBADN

A Game Boy Advance console whose cartridges are DNA.

GBADN has two programs:

- **GBADN Console** (`gbadn-emu`) is a GBA emulator that only reads FASTA genomic files, plain or gzip-compressed. It refuses `.gba` files, and a ROM renamed to `.fasta` as well.
- **GBADN Studio** (`gbadn-studio`) is a desktop app: drop a GBA ROM in and get a `.fasta.gz` file out.

Current release: v0.2.0, for Linux, Windows, macOS and Android. See [ROADMAP.md](ROADMAP.md) for what's verified and what's next.

## How it works

```
game.gba -> chromosomes -> zstd -> 2 bits per base -> gzip -> game.fasta.gz -> Console decodes in memory -> mGBA
```

The output is a real multi-record FASTA file with only `A`, `C`, `G` and `T`, and every cartridge carries an MD5 checksum, so a single mutated base is detected. The full specification is in [docs/FORMAT.md](docs/FORMAT.md).

### Chromosomes

Each cartridge is split into `chrHeader`, `chrCode`, `chrGraphics`, `chrAudio`, `chrData` and `chrEmpty`, plus a `gbadn_layout` record that rebuilds the exact ROM. A GBA ROM has no table of contents, so apart from the header and empty space these labels are best guesses from the content; a wrong guess never affects the game. One 16 MiB game, for example, split into roughly 10% code, 10% compressed graphics, 26% sampled audio, 37% other data and 18% empty space.

### About "compress"

Each byte takes 4 bases, and each base takes one byte of text, so plain FASTA is about 4 times the size of its zstd-compressed payload, and usually larger than the ROM. Cartridges are therefore gzip-compressed, like most genomic data: one 16 MiB ROM became 24 MiB of FASTA, or 7.4 MiB as `.fasta.gz`. It stays real FASTA: `zcat game.fasta.gz` shows the sequence.

## Download

Linux, Windows, macOS (Apple Silicon) and Android (arm64) builds are on the [Releases](../../releases) page. Intel Macs aren't covered yet.

Unpack the archive and keep both programs in the same folder. On Linux, you need ALSA and udev, which most desktop systems already have.

The builds aren't signed, so systems warn the first time:

- **Windows:** choose **More info**, then **Run anyway**.
- **macOS:** run `xattr -dr com.apple.quarantine <folder>` once on the unpacked folder.
- **Android:** allow installing from your browser or file manager when asked. Android is console-only, so convert on a computer.

## Android

Tap **Open cartridge** and choose a `.fasta.gz`. It's copied to the app's private storage, where its saves live too, so uninstalling the app deletes them.

## Build

You need Rust 1.92 or newer, a C compiler and CMake. On Ubuntu:

```sh
sudo apt install build-essential cmake libasound2-dev libudev-dev
git clone --recurse-submodules <repository-url>
cd GBADN
cargo build --release
```

If you cloned without submodules, run `git submodule update --init`.

## Run

Open the console, then click **Open cartridge** or drop a `.fasta.gz` onto the window:

```sh
cargo run --release -p gbadn-emu
```

You can also pass a cartridge directly. The included demo is a tiny original program that shows a green screen, turning red while a button is held:

```sh
cargo run --release -p gbadn-emu -- samples/demo.fasta
```

Open Studio to convert a ROM you own, then click **Play** once it's saved:

```sh
cargo build --release
target/release/gbadn-studio
```

Battery saves are written next to the cartridge as `<name>.sav`, and save states as `<name>.state`. There is one save state per game, and saving replaces it.

Keys can be changed under **Controls**, and your choices are remembered. Gamepads work too: A and B are the right and bottom face buttons, as on a GBA.

| GBA button | Default key |
|---|---|
| D-pad | Arrow keys |
| A / B | Z / X |
| L / R | A / S |
| Start / Select | Enter / Backspace |
| Save state / Load state | F5 / F9 |

## Repository layout

| Path | Contents |
|---|---|
| `crates/gbadn-codec` | The ROM to FASTA format, shared by both programs |
| `crates/gbadn-emu` | The console: FASTA-only loader, egui window, cpal sound, gilrs gamepads, C shim over mGBA |
| `crates/gbadn-studio` | The desktop UI |
| `android/` | The Android manifest and the activity that provides the file picker |
| `scripts/build-android.sh` | Builds the APK; used by CI |
| `vendor/mgba` | [mGBA](https://mgba.io) 0.10.5 as a git submodule (MPL-2.0) |
| `samples/demo.fasta` | The original demo cartridge |
| `docs/FORMAT.md` | Format specification |

## Games and legal

GBADN includes no games. Only convert ROMs you have the right to use. A FASTA file made from a game is still that game, so don't commit or share it.

## License

GBADN's own code is under the [MIT License](LICENSE). [mGBA](https://mgba.io) keeps its own MPL-2.0 license, included with every download.
