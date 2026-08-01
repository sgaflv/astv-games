#!/usr/bin/env bash
set -euo pipefail

# Build a signed Android APK from the Rust cdylib, without cargo-apk.
#
# Pipeline:
#   cargo ndk  -> libsnake.so
#   aapt2      -> base apk (binary AndroidManifest.xml)
#   zip        -> add native lib under lib/<abi>/
#   zipalign   -> align the apk
#   apksigner  -> sign with a debug keystore
#
# Configuration via environment:
#   ANDROID_ABI  default: armeabi-v7a   (also: arm64-v8a, x86, x86_64)
#   PROFILE      default: debug         (also: release)
#   PLATFORM     default: 26            (Android API level)
#   ANDROID_HOME / ANDROID_SDK_ROOT     SDK location (default: $HOME/Android/Sdk)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE="${PROFILE:-debug}"
ANDROID_ABI="${ANDROID_ABI:-armeabi-v7a}"
PLATFORM="${PLATFORM:-26}"
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"

case "$ANDROID_ABI" in
    armeabi-v7a) RUST_TARGET="armv7-linux-androideabi" ;;
    arm64-v8a)   RUST_TARGET="aarch64-linux-android" ;;
    x86)         RUST_TARGET="i686-linux-android" ;;
    x86_64)      RUST_TARGET="x86_64-linux-android" ;;
    *) echo "error: unknown ABI: $ANDROID_ABI" >&2; exit 1 ;;
esac

BUILD_TOOLS="$(ls -d "$SDK"/build-tools/* 2>/dev/null | sort -V | tail -1 || true)"
PLATFORM_JAR="$(ls "$SDK"/platforms/android-*/android.jar 2>/dev/null | sort -V | tail -1 || true)"

if [ -z "$BUILD_TOOLS" ] || [ -z "$PLATFORM_JAR" ]; then
    echo "error: Android SDK build-tools/platform not found under $SDK" >&2
    exit 1
fi

AAPT2="$BUILD_TOOLS/aapt2"
ZIPALIGN="$BUILD_TOOLS/zipalign"
APKSIGNER="$BUILD_TOOLS/apksigner"

# 1. Build the native library
echo ">> cargo ndk ($ANDROID_ABI, platform $PLATFORM, profile $PROFILE)"
NDK_ARGS=(-t "$ANDROID_ABI" --platform "$PLATFORM")
if [ "$PROFILE" = "release" ]; then
    cargo ndk "${NDK_ARGS[@]}" build --release
else
    cargo ndk "${NDK_ARGS[@]}" build
fi

SO="$ROOT/target/$RUST_TARGET/$PROFILE/libsnake.so"
if [ ! -f "$SO" ]; then
    echo "error: native library not found: $SO" >&2
    exit 1
fi

# 2. Package into an apk
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
OUT_DIR="$ROOT/target/apk"
mkdir -p "$OUT_DIR"

echo ">> aapt2 link"
"$AAPT2" link \
    -o "$STAGE/base.apk" \
    --manifest "$ROOT/android/AndroidManifest.xml" \
    -I "$PLATFORM_JAR"

mkdir -p "$STAGE/lib/$ANDROID_ABI"
cp "$SO" "$STAGE/lib/$ANDROID_ABI/"
( cd "$STAGE" && zip -qr0 base.apk lib )

echo ">> zipalign"
"$ZIPALIGN" -f -p 4 "$STAGE/base.apk" "$STAGE/aligned.apk"

# 3. Sign with the debug keystore
KEYSTORE="${KEYSTORE:-$ROOT/android/debug.keystore}"
if [ ! -f "$KEYSTORE" ]; then
    echo ">> generating debug keystore: $KEYSTORE"
    keytool -genkeypair -v \
        -keystore "$KEYSTORE" \
        -alias androiddebugkey \
        -keyalg RSA -keysize 2048 -validity 10000 \
        -storepass android -keypass android \
        -dname "CN=Android Debug,O=Android,C=US" >/dev/null 2>&1
fi

OUT_APK="$OUT_DIR/snake.apk"
echo ">> apksigner sign"
"$APKSIGNER" sign \
    --ks "$KEYSTORE" \
    --ks-key-alias androiddebugkey \
    --ks-pass pass:android \
    --key-pass pass:android \
    --out "$OUT_APK" \
    "$STAGE/aligned.apk"

"$APKSIGNER" verify --verbose "$OUT_APK" 2>/dev/null | grep -E "Verified using v2|Verified using v3" || true

echo ">> APK: $OUT_APK"
