# Working on GBADN

GBADN is a GBA emulator that only reads FASTA genomic files, plus a desktop UI that converts ROMs into those files. Read [ROADMAP.md](ROADMAP.md) for current status and [docs/FORMAT.md](docs/FORMAT.md) for the format.

## Git flow

- One short-lived branch and one pull request per roadmap item. Never push directly to `main`.
- Merge only after CI passes and the project owner has reviewed the PR.
- Commit, push and open PRs as part of this flow, on a task branch. Leave merging to the project owner.
- Use imperative commit subjects that say what changes, such as `Add gamepad support`.
- Commit with the repository-local noreply address, never a personal or work email: the repository is meant to be public.
- Merge stacked PRs in order with merge commits, retargeting each to `main` before merging it. `gh pr edit --base` can fail on a GitHub API error, leaving a PR on its old base, so use `gh api -X PATCH repos/jsarriaa/GBADN/pulls/<number> -f base=main` and confirm the base changed.

## Releases

- Pushing a `v*` tag from `main` builds Linux, Windows and macOS, runs all tests on each, and publishes a GitHub release. Tag only when the project owner asks.
- Keep the version in `Cargo.toml` matching the tag, and update `ROADMAP.md` and the README's current release in the PR before tagging.
- Don't change the repository's visibility or license without the project owner's decision.
- Pull requests only run the release builds when they touch the workflow, `build.rs`, the shim or `Cargo.lock`, because Windows and macOS minutes cost more on a private repository.
- Windows and macOS can't be run from the main development machine. Passing CI there isn't the same as trying them, so say which one you've done.

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

## Android

- APKs build in GitHub Actions. Don't install an SDK, NDK or JDK on a machine unless its owner agrees.
- To test: `gh run download <run-id>`, `adb install -r <apk>`, `adb logcat`, and `adb exec-out screencap -p`. Builds with access to the repository's secrets share a lasting signature, so updates install in place.
- The signing key lives in the secrets `ANDROID_KEYSTORE_BASE64` and `ANDROID_KEYSTORE_PASSWORD`, and its owner keeps a backup. Never commit it. Losing it means users must uninstall to update, losing their saves. Without the secrets, builds get a temporary key and a warning.
- Rust panics go to `adb logcat` under the tag `gbadn`.
- Keep Android-only code in `crates/gbadn-emu/src/android.rs` and `android/`. The console takes touch controls and the file picker as options, so desktop tests can cover them.

## Browser

- `web/build.sh` builds with Emscripten in CI, and `web/test/site.mjs` drives the result in headless Chromium. CI saves its screenshots as an artifact: look at them, because a test reading the canvas can pass while something covers the game.
- It deploys to GitHub Pages from `main`. The site must never include a game, and visitors' files must never leave their device.

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p gbadn-emu -- samples/demo.fasta
cargo build && target/debug/gbadn-studio   # Play needs the console built beside Studio
cargo run -p gbadn-codec --example make_demo -- samples/demo.fasta   # regenerate the sample
```

`GBADN_SKIP_MGBA_BUILD=1` skips compiling mGBA. It's only for `cargo check` and `cargo clippy`: nothing that links `gbadn-emu` will work with it set.

## Code conventions

- Rust 2024 edition, formatted by rustfmt, with clippy warnings treated as errors.
- All mGBA access goes through `crates/gbadn-emu/shim/shim.c`. Rust never sees `struct mCore`, and the shim follows mGBA's C style (tabs).
- Every `unsafe` block has a `// SAFETY:` comment.
- Don't modify `vendor/mgba`. To upgrade, move the submodule to a new tag in its own PR.
- Documents and comments are in English. Keep comments for what the code can't say itself.
- New dependencies must support the Rust version in use, which is pinned to 1.94.1 in CI so new lints can't fail unrelated PRs. `eframe` stays on 0.35 because newer releases require Rust 1.95.
- Keep the programs portable: Android is next, so avoid new system libraries, and keep desktop-only pieces such as the file dialog easy to swap out.
- Press-and-hold tests must step in real time, because the emulator paces itself by the clock.
