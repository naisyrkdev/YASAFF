#!/usr/bin/env bash
# Builds target/deb/yasaff_<version>_<arch>.deb. Unlike the AppImage, it doesn't bundle ffmpeg
# and xorriso: it recommends the distribution's packages, which apt installs by default, and
# YASAFF finds them on PATH. Needs: cargo, dpkg-deb and dpkg-shlibdeps (package dpkg-dev).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

for tool in cargo dpkg-deb dpkg-shlibdeps; do
    command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }
done

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
arch="$(dpkg --print-architecture)"
[[ -n "$version" ]] || { echo "No version in Cargo.toml" >&2; exit 1; }

echo "Building yasaff $version..."
cargo build --release --locked

work="$root/target/deb/work"
pkg="$work/yasaff_${version}_$arch"
rm -rf "$work"
mkdir -p "$pkg/DEBIAN" "$pkg/usr/bin" "$pkg/usr/share/applications" \
    "$pkg/usr/share/icons/hicolor/256x256/apps" "$pkg/usr/share/doc/yasaff"

install -m 755 target/release/yasaff "$pkg/usr/bin/yasaff"
strip "$pkg/usr/bin/yasaff"
install -m 644 assets/icon-256.png "$pkg/usr/share/icons/hicolor/256x256/apps/yasaff.png"
install -m 644 LICENSE "$pkg/usr/share/doc/yasaff/copyright"
install -m 644 README.md "$pkg/usr/share/doc/yasaff/README.md"
install -m 644 docs/*.md "$pkg/usr/share/doc/yasaff/"

cat > "$pkg/usr/share/applications/yasaff.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=YASAFF
Comment=Console-style launcher for games on physical media
Exec=yasaff
Icon=yasaff
Terminal=false
Categories=Game;
EOF

# Library dependencies, with this distribution's package names. dpkg-shlibdeps wants a
# debian/control next to it, so give it a minimal one.
mkdir -p "$work/debian"
printf 'Source: yasaff\n\nPackage: yasaff\nArchitecture: any\n' > "$work/debian/control"
depends="$(cd "$work" && dpkg-shlibdeps -O "$pkg/usr/bin/yasaff" 2>/dev/null | sed -n 's/^shlibs:Depends=//p')"
[[ -n "$depends" ]] || { echo "dpkg-shlibdeps found no dependencies" >&2; exit 1; }

size="$(du -sk --exclude=DEBIAN "$pkg" | cut -f1)"
cat > "$pkg/DEBIAN/control" <<EOF
Package: yasaff
Version: $version
Section: games
Priority: optional
Architecture: $arch
Installed-Size: $size
Depends: $depends
Recommends: ffmpeg, xorriso
Maintainer: Krystian Grzelak <krystian.grzelak@protonmail.com>
Homepage: https://github.com/naisyrkdev/yasaff
Description: Console-style launcher for games on physical media
 YASAFF is a fullscreen, controller-driven menu that boots games from USB
 sticks and discs, manages their saves, and installs apps and themes.
 ffmpeg plays preview and idle videos; xorriso burns games to CD and DVD.
EOF

out="$root/target/deb/yasaff_${version}_$arch.deb"
dpkg-deb --root-owner-group --build "$pkg" "$out" >/dev/null
rm -rf "$work"
echo
echo "Package: $out"
