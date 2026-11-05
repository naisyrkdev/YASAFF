#!/usr/bin/env bash
# Builds target/arch/yasaff-<version>-1-<arch>.pkg.tar.zst for Arch Linux and its derivatives
# (Manjaro, EndeavourOS, CachyOS...). Like the .deb, it doesn't bundle ffmpeg and xorriso: they
# are optional dependencies, and YASAFF finds them on PATH. Run it on Arch as a normal user
# (makepkg refuses root). Needs: makepkg (pacman) and cargo (package rust or rustup).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

for tool in makepkg cargo; do
    command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }
done

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
[[ -n "$version" ]] || { echo "No version in Cargo.toml" >&2; exit 1; }

out_dir="$root/target/arch"
work="$out_dir/work"
rm -rf "$work"
mkdir -p "$work"

# The PKGBUILD builds this checkout instead of downloading a source archive.
cat > "$work/PKGBUILD" <<EOF
pkgname=yasaff
pkgver=$version
pkgrel=1
pkgdesc='Console-style launcher for games on physical media'
arch=('x86_64' 'aarch64')
url='https://github.com/naisyrkdev/yasaff'
license=('GPL-3.0-only')
depends=('alsa-lib' 'systemd-libs' 'gcc-libs' 'glibc' 'libxkbcommon' 'libglvnd' 'hicolor-icon-theme')
optdepends=('ffmpeg: preview and idle videos'
            'libisoburn: xorriso, to burn games to CD and DVD')
makedepends=('cargo')
options=('!debug' '!lto')

_src='$root'

build() {
    cd "\$_src"
    cargo build --release --locked
}

package() {
    cd "\$_src"
    install -Dm 755 target/release/yasaff "\$pkgdir/usr/bin/yasaff"
    install -Dm 644 assets/icon-256.png "\$pkgdir/usr/share/icons/hicolor/256x256/apps/yasaff.png"
    install -Dm 644 LICENSE "\$pkgdir/usr/share/licenses/yasaff/LICENSE"
    install -Dm 644 README.md "\$pkgdir/usr/share/doc/yasaff/README.md"
    install -m 644 docs/*.md "\$pkgdir/usr/share/doc/yasaff/"
    install -Dm 644 /dev/stdin "\$pkgdir/usr/share/applications/yasaff.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=YASAFF
Comment=Console-style launcher for games on physical media
Exec=yasaff
Icon=yasaff
Terminal=false
Categories=Game;
DESKTOP
}
EOF

echo "Building yasaff $version..."
(cd "$work" && PKGDEST="$out_dir" makepkg --force --cleanbuild --nodeps)
rm -rf "$work"
echo
echo "Package: $(ls "$out_dir"/yasaff-"$version"-1-*.pkg.tar.zst)"
