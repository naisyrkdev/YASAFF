#!/usr/bin/env bash
# Builds target/appimage/yasaff.AppImage with ffmpeg and xorriso inside, so users don't have
# to install anything. Needs: cargo-appimage, appimagetool, curl, a C compiler and make.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

ffmpeg_name="ffmpeg-n8.1-latest-linux64-lgpl-8.1"
ffmpeg_release="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest"
xorriso_version="1.5.8.pl02"
xorriso_sha256="b1455ecafbf0692ddafe1d71002a96f2ce2d77f4deae602678261ce033f97bc8"

cache="$root/target/bundle/cache"
tools="$root/target/bundle/tools"
mkdir -p "$cache"
rm -rf "$tools"
mkdir -p "$tools"

for tool in cargo-appimage appimagetool curl cc make; do
    command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }
done

# ffmpeg: prebuilt LGPL build. It only links against glibc.
echo "Fetching ffmpeg..."
curl -fsSL -o "$cache/ffmpeg-checksums.sha256" "$ffmpeg_release/checksums.sha256"
expected="$(grep " $ffmpeg_name.tar.xz\$" "$cache/ffmpeg-checksums.sha256" | cut -d' ' -f1)"
[[ -n "$expected" ]] || { echo "No checksum for $ffmpeg_name.tar.xz" >&2; exit 1; }
archive="$cache/$ffmpeg_name.tar.xz"
if [[ ! -f "$archive" ]] || ! echo "$expected  $archive" | sha256sum -c --status; then
    curl -fL -o "$archive" "$ffmpeg_release/$ffmpeg_name.tar.xz"
    echo "$expected  $archive" | sha256sum -c --status || { rm -f "$archive"; echo "ffmpeg checksum mismatch" >&2; exit 1; }
fi
tar -xJf "$archive" -C "$cache" "$ffmpeg_name/bin/ffmpeg" "$ffmpeg_name/LICENSE.txt"
cp "$cache/$ffmpeg_name/bin/ffmpeg" "$tools/ffmpeg"
cp "$cache/$ffmpeg_name/LICENSE.txt" "$tools/ffmpeg-LICENSE.txt"

# xorriso: GNU xorriso bundles libburn, libisofs and libisoburn, so it builds into one binary
# that only needs libc. It handles CDs and DVDs, so wodim/growisofs aren't needed.
echo "Building xorriso..."
xorriso_tar="$cache/xorriso-$xorriso_version.tar.gz"
if [[ ! -f "$xorriso_tar" ]] || ! echo "$xorriso_sha256  $xorriso_tar" | sha256sum -c --status; then
    # ftp.gnu.org is often unreachable; the checksum below makes any mirror safe to use.
    for mirror in https://ftp.gnu.org/gnu https://mirrors.kernel.org/gnu https://ftpmirror.gnu.org; do
        curl -fL --connect-timeout 20 -o "$xorriso_tar" "$mirror/xorriso/xorriso-$xorriso_version.tar.gz" && break
    done
    echo "$xorriso_sha256  $xorriso_tar" | sha256sum -c --status || { rm -f "$xorriso_tar"; echo "xorriso checksum mismatch" >&2; exit 1; }
fi
xorriso_src="$cache/xorriso-src"
rm -rf "$xorriso_src"
mkdir -p "$xorriso_src"
tar -xzf "$xorriso_tar" -C "$xorriso_src" --strip-components=1
(
    cd "$xorriso_src"
    ./configure --disable-shared --enable-static --disable-libreadline --disable-libedit \
        --disable-libacl --disable-xattr --disable-zlib --disable-libbz2 --disable-jtethreads >/dev/null
    make -j"$(nproc)" >/dev/null
)
cp "$xorriso_src/xorriso/xorriso" "$tools/xorriso"
strip "$tools/xorriso"
cp "$xorriso_src/COPYING" "$tools/xorriso-COPYING"
cp "$xorriso_tar" "$tools/xorriso-source.tar.gz"

cp "$root/installer/THIRD-PARTY.txt" "$tools/"
chmod +x "$tools/ffmpeg" "$tools/xorriso"

# Cargo.toml lists target/bundle/tools as an AppImage asset; it lands at $APPDIR/tools.
echo "Building the AppImage..."
cargo appimage
echo
echo "AppImage: $root/target/appimage/yasaff.AppImage"
