#!/usr/bin/env bash
# Build JNI native Rust library for Android. Requires Rust + Android NDK + cargo-ndk.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="$ROOT/rust/football_match_engine"
OUT="$ROOT/app/src/main/jniLibs"

if ! command -v cargo >/dev/null 2>&1; then
  echo 'ERROR: Rust/Cargo is not installed.' >&2
  exit 2
fi
if ! cargo ndk --version >/dev/null 2>&1; then
  echo 'ERROR: cargo-ndk is missing. Install it with: cargo install cargo-ndk' >&2
  exit 2
fi
if [[ -z "${ANDROID_NDK_HOME:-}" && -z "${ANDROID_NDK_ROOT:-}" && -z "${ANDROID_HOME:-}" && -z "${ANDROID_SDK_ROOT:-}" ]]; then
  echo 'WARNING: ANDROID_NDK_HOME / ANDROID_HOME is unset. cargo-ndk may not locate the NDK.' >&2
fi

# Default target: modern Android ARM64. To build more, set e.g.
# RUST_ANDROID_ABIS=arm64-v8a,armeabi-v7a,x86_64
IFS=',' read -r -a ABIS <<< "${RUST_ANDROID_ABIS:-arm64-v8a}"
ARGS=()
for abi in "${ABIS[@]}"; do
  case "$abi" in
    arm64-v8a|armeabi-v7a|x86_64|x86) ARGS+=(-t "$abi") ;;
    *) echo "ERROR: unsupported Android ABI: $abi" >&2; exit 2 ;;
  esac
done

mkdir -p "$OUT"
(cd "$CRATE" && cargo ndk "${ARGS[@]}" -o "$OUT" build --release)

for abi in "${ABIS[@]}"; do
  so="$OUT/$abi/libfootball_match_engine.so"
  if [[ ! -s "$so" ]]; then
    echo "ERROR: JNI library not generated: $so" >&2
    exit 1
  fi
  echo "BUILT $so ($(wc -c < "$so") bytes)"
done
printf 'NEXT: ./gradlew :app:assembleDebug\n'
