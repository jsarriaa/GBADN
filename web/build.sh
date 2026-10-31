#!/usr/bin/env bash
# Builds the browser version into web/dist. Needs Emscripten and the
# wasm32-unknown-emscripten Rust target.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dist="$root/web/dist"
core="$root/target/web-mgba"
target=wasm32-unknown-emscripten

emcmake cmake -S "$root/vendor/mgba" -B "$core" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DCMAKE_C_FLAGS="-DDISABLE_THREADING -D_GNU_SOURCE" \
    -DLIBMGBA_ONLY=ON -DM_CORE_GBA=ON -DM_CORE_GB=OFF
cmake --build "$core" --target mgba -j4

export CC_wasm32_unknown_emscripten=emcc
export AR_wasm32_unknown_emscripten=emar
CARGO_PROFILE_RELEASE_PANIC=abort cargo build --release --locked -p gbadn-web --target "$target"

rm -rf "$dist"
mkdir -p "$dist"
emcc -O3 \
    -DBUILD_STATIC -DDISABLE_THREADING -D_GNU_SOURCE \
    -I"$root/vendor/mgba/include" -I"$core/include" \
    "$root/crates/gbadn-emu/shim/shim.c" \
    "$root/target/$target/release/libgbadn_web.a" \
    "$core/libmgba.a" \
    -sMODULARIZE -sEXPORT_NAME=createGbadn \
    -sENVIRONMENT=web,worker \
    -sALLOW_MEMORY_GROWTH -sINITIAL_MEMORY=64MB -sMAXIMUM_MEMORY=1GB \
    -sEXPORTED_RUNTIME_METHODS=HEAPU8,HEAP16,HEAPU32 \
    -sEXPORTED_FUNCTIONS=_malloc,_free,_gbadn_web_decode,_gbadn_web_encode,_gbadn_web_bytes,_gbadn_web_bytes_size,_gbadn_web_message,_gbadn_web_message_size,_gbadn_core_new,_gbadn_core_free,_gbadn_core_run_frame,_gbadn_core_video,_gbadn_core_read_audio,_gbadn_core_state_size,_gbadn_core_save_state,_gbadn_core_load_state,_gbadn_core_savedata_clone,_gbadn_core_savedata_restore \
    -o "$dist/gbadn.js"

cp "$root"/web/index.html "$root"/web/style.css "$root"/web/app.js "$root"/web/codec-worker.js "$root"/web/sound-worklet.js "$dist/"
cp "$root/samples/demo.fasta" "$dist/demo.fasta"
cp "$root/LICENSE" "$dist/LICENSE.txt"
cp "$root/vendor/mgba/LICENSE" "$dist/LICENSE.mGBA.txt"
touch "$dist/.nojekyll"
echo "Built $dist"
