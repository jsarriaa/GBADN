#!/usr/bin/env bash
# Builds the console APK. Needs ANDROID_HOME, ANDROID_NDK_HOME, a JDK, and the
# aarch64-linux-android Rust target. Set ANDROID_KEYSTORE and ANDROID_KEYSTORE_PASSWORD
# to sign with a lasting key; otherwise a throwaway key is made.
set -euo pipefail

output="${1:-target/gbadn-console.apk}"
api=26
triple=aarch64-linux-android
abi=arm64-v8a

build_tools="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -1)"
android_jar="$ANDROID_HOME/platforms/$(ls "$ANDROID_HOME/platforms" | sort -V | tail -1)/android.jar"
toolchain="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"

export "CC_aarch64_linux_android=$toolchain/$triple$api-clang"
export "CXX_aarch64_linux_android=$toolchain/$triple$api-clang++"
export "AR_aarch64_linux_android=$toolchain/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$toolchain/$triple$api-clang"
# Android 15 and later phones may use 16 KB memory pages.
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"

cargo build --release --locked --lib -p gbadn-emu --target "$triple"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/classes" "$work/dex" "$work/apk/lib/$abi"
javac --release 8 -nowarn -Xlint:-options -classpath "$android_jar" -d "$work/classes" \
    android/java/org/gbadn/console/*.java
"$build_tools/d8" --release --lib "$android_jar" --min-api "$api" --output "$work/dex" \
    "$work"/classes/org/gbadn/console/*.class

"$build_tools/aapt2" link -o "$work/base.apk" --manifest android/AndroidManifest.xml -I "$android_jar"
cp "$work/dex/classes.dex" "$work/apk/"
cp "target/$triple/release/libgbadn_emu.so" "$work/apk/lib/$abi/"
(
    cd "$work/apk"
    zip -q -r "$work/base.apk" .
)
"$build_tools/zipalign" -f 4 "$work/base.apk" "$work/aligned.apk"

keystore="${ANDROID_KEYSTORE:-$work/throwaway.jks}"
password="${ANDROID_KEYSTORE_PASSWORD:-throwaway}"
if [ ! -f "$keystore" ]; then
    keytool -genkeypair -keystore "$keystore" -storepass "$password" -alias gbadn \
        -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=GBADN" 2>/dev/null
fi
mkdir -p "$(dirname "$output")"
"$build_tools/apksigner" sign --ks "$keystore" --ks-pass "pass:$password" --out "$output" "$work/aligned.apk"
"$build_tools/apksigner" verify "$output"
echo "Built $output"
