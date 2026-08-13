#!/usr/bin/env bash
set -euo pipefail

# Build a signed Android APK from the Rust cdylib + Java glue, without cargo-apk.
#
# Pipeline:
#   cargo ndk  -> libsnake.so
#   javac      -> compile the miniquad Java glue (MainActivity/QuadNative)
#   d8         -> dex the classes into classes.dex
#   aapt2      -> base apk (binary AndroidManifest.xml)
#   zip        -> add native lib under lib/<abi>/ and classes.dex
#   zipalign   -> align the apk
#   apksigner  -> sign with a debug keystore
#
# Requirements:
#   - Rust target installed (rustup target add <target>)
#   - cargo-ndk
#   - Android SDK with build-tools, platforms/android-30+ and NDK
#   - JDK (javac/d8) for the Java glue
#
# Configuration via environment:
#   ANDROID_ABI  default: armeabi-v7a   (also: arm64-v8a, x86, x86_64)
#   PROFILE      default: debug         (also: release)
#   PLATFORM     default: 26            (native min API level)
#   JAVA_SOURCE  default: 11            (javac -source/-target for the glue)
#   ANDROID_HOME / ANDROID_SDK_ROOT     SDK location (default: $HOME/Android/Sdk)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE="${PROFILE:-debug}"
ANDROID_ABI="${ANDROID_ABI:-armeabi-v7a}"
PLATFORM="${PLATFORM:-26}"
JAVA_SOURCE="${JAVA_SOURCE:-11}"
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
D8="$BUILD_TOOLS/d8"
JAVAC="$(command -v javac || true)"

if [ -z "$JAVAC" ]; then
    echo "error: javac not found on PATH (JDK required for the Java glue)" >&2
    exit 1
fi

# 1. Build the native library
echo ">> cargo ndk ($ANDROID_ABI, platform $PLATFORM, profile $PROFILE)"
NDK_ARGS=(-t "$ANDROID_ABI" --platform "$PLATFORM")
if [ "$PROFILE" = "release" ]; then
    cargo ndk "${NDK_ARGS[@]}" build -p app --release
else
    cargo ndk "${NDK_ARGS[@]}" build -p app
fi

SO="$ROOT/target/$RUST_TARGET/$PROFILE/libapp.so"
if [ ! -f "$SO" ]; then
    echo "error: native library not found: $SO" >&2
    exit 1
fi

# 2. Package into an apk
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
OUT_DIR="$ROOT/target/apk"
mkdir -p "$OUT_DIR"

# 3. Compile the Java glue (MainActivity/QuadNative) and dex it.
JAVA_SOURCES="$(find "$ROOT/android/java" -name '*.java')"
if [ -z "$JAVA_SOURCES" ]; then
    echo "error: no Java sources under $ROOT/android/java" >&2
    exit 1
fi

echo ">> javac"
mkdir -p "$STAGE/classes"
"$JAVAC" \
    -source "$JAVA_SOURCE" \
    -target "$JAVA_SOURCE" \
    -classpath "$PLATFORM_JAR" \
    -d "$STAGE/classes" \
    $JAVA_SOURCES

echo ">> d8"
CLASSES="$(find "$STAGE/classes" -name '*.class')"
"$D8" --release --min-api "$PLATFORM" --lib "$PLATFORM_JAR" \
    --output "$STAGE" $CLASSES

echo ">> aapt2 link"
"$AAPT2" link \
    -o "$STAGE/base.apk" \
    --manifest "$ROOT/android/AndroidManifest.xml" \
    -I "$PLATFORM_JAR"

mkdir -p "$STAGE/lib/$ANDROID_ABI"
cp "$SO" "$STAGE/lib/$ANDROID_ABI/"
( cd "$STAGE" && zip -qr0 base.apk lib classes.dex )

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

OUT_APK="$OUT_DIR/app.apk"
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
