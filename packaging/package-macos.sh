#!/usr/bin/env bash
# Builds a self-contained SenseStudio .dmg for one macOS architecture.
#   packaging/package-macos.sh <arm64|x86_64> <qt-prefix>
# <qt-prefix> is the Qt install root (contains bin/macdeployqt), e.g. $QT_ROOT_DIR in CI.
# Bundles static FFmpeg/ffprobe and standalone yt-dlp next to the app binary.
set -euo pipefail

ARCH="${1:?usage: package-macos.sh <arm64|x86_64> <qt-prefix>}"
QT_PREFIX="${2:?usage: package-macos.sh <arm64|x86_64> <qt-prefix>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/.*SENSE_VERSION_LABEL "\([^"]*\)".*/\1/p' "$ROOT/app/CMakeLists.txt")"

# Pinned FFmpeg 9.0.2 static builds (martin-riedl.de); each zip has a published .sha256.
case "$ARCH" in
  arm64)  RUST_TARGET=aarch64-apple-darwin; FF_BASE=https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2 ;;
  x86_64) RUST_TARGET=x86_64-apple-darwin;  FF_BASE=https://ffmpeg.martin-riedl.de/download/macos/amd64/1789931006_9.0.2 ;;
  *) echo "unknown arch: $ARCH" >&2; exit 2 ;;
esac
YTDLP_URL=https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos

BUILD="$ROOT/app/build-macos-$ARCH"
TOOLS="$ROOT/.cache/tools/macos-$ARCH"
DIST="$ROOT/dist"
mkdir -p "$TOOLS" "$DIST"

fetch_ffmpeg_tool() {
  local name="$1"
  [[ -x "$TOOLS/$name" ]] && return
  curl -fsSL -o "$TOOLS/$name.zip" "$FF_BASE/$name.zip"
  local expected actual
  expected="$(curl -fsSL "$FF_BASE/$name.zip.sha256" | awk '{print $1}')"
  actual="$(shasum -a 256 "$TOOLS/$name.zip" | awk '{print $1}')"
  if [[ "$expected" != "$actual" ]]; then
    echo "checksum mismatch for $name.zip: expected $expected, got $actual" >&2
    exit 1
  fi
  unzip -o -q "$TOOLS/$name.zip" -d "$TOOLS"
  rm "$TOOLS/$name.zip"
  chmod +x "$TOOLS/$name"
}

echo "==> Tools ($ARCH)"
fetch_ffmpeg_tool ffmpeg
fetch_ffmpeg_tool ffprobe
[[ -x "$TOOLS/yt-dlp" ]] || { curl -fsSL -o "$TOOLS/yt-dlp" "$YTDLP_URL"; chmod +x "$TOOLS/yt-dlp"; }

echo "==> Build $VERSION ($ARCH)"
if command -v rustup >/dev/null; then rustup target add "$RUST_TARGET" >/dev/null; fi
# Start from a fresh bundle so files from an earlier deploy or signature never leak in.
# This must happen before configuring: CMake writes Info.plist into the bundle at configure time.
rm -rf "${BUILD:?}/SenseStudio.app"
cmake -S "$ROOT/app" -B "$BUILD" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_PREFIX_PATH="$QT_PREFIX" \
  -DCMAKE_OSX_ARCHITECTURES="$ARCH" \
  -DCMAKE_OSX_DEPLOYMENT_TARGET=12.0 \
  -DRUST_TARGET="$RUST_TARGET"
cmake --build "$BUILD"

APP="$BUILD/SenseStudio.app"
MACOS_DIR="$APP/Contents/MacOS"
cp "$TOOLS/ffmpeg" "$TOOLS/ffprobe" "$TOOLS/yt-dlp" "$MACOS_DIR/"

echo "==> Deploy Qt"
"$QT_PREFIX/bin/macdeployqt" "$APP" -qmldir="$ROOT/app/qml" -verbose=1

# Every shipped binary must contain the target slice; a wrong-arch sidecar fails only at runtime.
for key in CFBundleIdentifier CFBundleShortVersionString CFBundleIconFile LSMinimumSystemVersion; do
  value="$(plutil -extract "$key" raw "$APP/Contents/Info.plist" 2>/dev/null || true)"
  if [[ -z "$value" ]]; then
    echo "Info.plist is missing $key" >&2
    exit 1
  fi
done
for bin in SenseStudio sense-cli ffmpeg ffprobe yt-dlp; do
  if ! lipo -archs "$MACOS_DIR/$bin" | tr ' ' '\n' | grep -qx "$ARCH"; then
    echo "$bin lacks $ARCH (has: $(lipo -archs "$MACOS_DIR/$bin"))" >&2
    exit 1
  fi
done

echo "==> Strip build-machine rpaths"
# Build rpaths (e.g. the Qt install dir) are searched before @executable_path/../Frameworks;
# on a machine that also has Qt installed they load a second QtCore and crash.
while IFS= read -r -d '' file; do
  file "$file" | grep -q 'Mach-O' || continue
  # `|| true`: files without absolute rpaths are the normal case, not a pipefail error.
  { otool -l "$file" | awk '/cmd LC_RPATH/{getline; getline; print $2}' | grep -v '^@' || true; } | while IFS= read -r rpath; do
    install_name_tool -delete_rpath "$rpath" "$file"
  done
done < <(find "$APP" -type f -print0)
leftover="$(find "$APP" -type f -exec sh -c 'file "$1" | grep -q Mach-O && otool -l "$1" | awk "/cmd LC_RPATH/{getline; getline; print \$2}" | grep -v "^@" | sed "s|^|$1: |"' _ {} \;)"
if [[ -n "$leftover" ]]; then
  echo "absolute rpaths remain:" >&2
  echo "$leftover" >&2
  exit 1
fi

echo "==> Sign (ad-hoc)"
# Apple Silicon refuses unsigned code, and macdeployqt invalidates existing signatures.
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

echo "==> DMG"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="$DIST/SenseStudio-$VERSION-macos-$ARCH.dmg"
rm -f "$DMG"
hdiutil create -volname "SenseStudio $VERSION" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null
echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
