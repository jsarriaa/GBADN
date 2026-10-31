# Working on GBADN

GBADN is a GBA emulator that only reads FASTA genomic files, plus a desktop UI that converts ROMs into those files. Read [ROADMAP.md](ROADMAP.md) for current status and [docs/FORMAT.md](docs/FORMAT.md) for the format.

## Git flow

- One short-lived branch and one pull request per roadmap item. Never push directly to `main`.
- Merge only after CI passes and the project owner has reviewed the PR.
- Commit, push and open PRs as part of this flow, on a task branch. Leave merging to the project owner.
- Use imperative commit subjects that say what changes, such as `Add gamepad support`.

## Definition of done

A roadmap item is ticked only when all of these are true:

1. Automated tests cover it and pass.
2. The real program has been launched and the feature exercised, using the demo cartridge.
3. `ROADMAP.md` and any affected docs are updated in the same change.

If a step could not be done, leave the item as `[~]` and note what's missing. Never tick an item on the strength of type-checking or code that only looks right.

## Rules that define the project

- **The console only accepts FASTA.** No code path may load a raw ROM, whatever the extension or a command-line flag says.
- **Decoded ROMs stay in memory.** The console never writes them to disk.
- **Never commit games, or FASTA made from games.** `samples/demo.fasta` is the only tracked cartridge; `samples/mGBA/` holds personal games and must never be pushed. Test with the demo cartridge (`crates/gbadn-codec/src/demo.rs`).
- **Format changes are deliberate.** Anything that alters the sequence needs a version bump and an update to `docs/FORMAT.md`, and `samples/demo.fasta` must be regenerated.
- **Chromosome labels are best guesses.** Say so wherever they're shown. The classifier may change freely as long as the ROM rebuilds exactly.
- **Be honest about size.** FASTA output is usually larger than the ROM; don't describe it as smaller on disk.

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p gbadn-emu -- samples/demo.fasta
cargo run -p gbadn-studio
cargo run -p gbadn-codec --example make_demo -- samples/demo.fasta   # regenerate the sample
```

`GBADN_SKIP_MGBA_BUILD=1` skips compiling mGBA. It's only for `cargo check` and `cargo clippy`: nothing that links `gbadn-emu` will work with it set.

## Code conventions

- Rust 2024 edition, formatted by rustfmt, with clippy warnings treated as errors.
- All mGBA access goes through `crates/gbadn-emu/shim/shim.c`. Rust never sees `struct mCore`, and the shim follows mGBA's C style (tabs).
- Every `unsafe` block has a `// SAFETY:` comment.
- Don't modify `vendor/mgba`. To upgrade, move the submodule to a new tag in its own PR.
- Documents and comments are in English. Keep comments for what the code can't say itself.
- New dependencies must support the Rust version in use. `eframe` stays on 0.35 because newer releases require Rust 1.95.
