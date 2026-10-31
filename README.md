# GBADN

A Game Boy Advance console whose cartridges are DNA.

GBADN has two programs:

- **GBADN Console** (`gbadn-emu`) is a GBA emulator that only reads FASTA genomic files, plain or gzip-compressed. It refuses `.gba` files, and a ROM renamed to `.fasta` as well.
- **GBADN Studio** (`gbadn-studio`) is a desktop app: drop a GBA ROM in and get a `.fasta.gz` file out.

Early development: see [ROADMAP.md](ROADMAP.md) for what works today.

## How it works

```
game.gba -> zstd -> 2 bits per base -> gzip -> game.fasta.gz -> Console decodes in memory -> mGBA
```

The output is a real single-record FASTA file with only `A`, `C`, `G` and `T`, and every cartridge carries an MD5 checksum, so a single mutated base is detected. The full specification is in [docs/FORMAT.md](docs/FORMAT.md).

### About "compress"

Each byte takes 4 bases, and each base takes one byte of text, so plain FASTA is about 4 times the size of its zstd-compressed payload, and usually larger than the ROM. Cartridges are therefore gzip-compressed, like most genomic data: a 16 MiB Pokemon Emerald ROM becomes 24 MiB of FASTA, or 7.4 MiB as `.fasta.gz`. It stays real FASTA: `zcat game.fasta.gz` shows the sequence.

## Build

You need Rust 1.92 or newer, a C compiler, CMake and the SDL2 development package. On Ubuntu:

```sh
sudo apt install build-essential cmake libsdl2-dev
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

Open Studio to convert a ROM you own:

```sh
cargo run --release -p gbadn-studio
```

Battery saves are written next to the cartridge, as `<name>.sav`.

| GBA button | Key |
|---|---|
| D-pad | Arrow keys |
| A / B | Z / X |
| L / R | A / S |
| Start / Select | Enter / Backspace |

## Repository layout

| Path | Contents |
|---|---|
| `crates/gbadn-codec` | The ROM to FASTA format, shared by both programs |
| `crates/gbadn-emu` | The console: FASTA-only loader, egui window, SDL2 sound, C shim over mGBA |
| `crates/gbadn-studio` | The desktop UI |
| `vendor/mgba` | [mGBA](https://mgba.io) 0.10.5 as a git submodule (MPL-2.0) |
| `samples/demo.fasta` | The original demo cartridge |
| `docs/FORMAT.md` | Format specification |

## Games and legal

GBADN includes no games. Only convert ROMs you have the right to use. A FASTA file made from a game is still that game, so don't commit or share it.

## License

Not chosen yet: the repository is private, and a license will be picked before it goes public. mGBA keeps its own MPL-2.0 license.
