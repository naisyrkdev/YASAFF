#!/usr/bin/env bash
# Builds target/rpm/yasaff-<version>-1.<arch>.rpm for Fedora, RHEL and openSUSE. Like the .deb,
# it doesn't bundle ffmpeg and xorriso: it recommends the distribution's packages, which dnf
# installs by default, and YASAFF finds them on PATH. Needs: cargo, rpmbuild (package rpm on
# Debian and Ubuntu, rpm-build on Fedora) and objdump (binutils).
#
# The library requirements are sonames, not package names, so the package installs on any
# RPM distribution with a glibc at least as new as the one it was built on. Build it on the
# oldest distribution you want to support.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

for tool in cargo rpmbuild objdump; do
    command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }
done

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
arch="$(uname -m)"
[[ -n "$version" ]] || { echo "No version in Cargo.toml" >&2; exit 1; }

echo "Building yasaff $version..."
cargo build --release --locked

work="$root/target/rpm/work"
rm -rf "$work"
mkdir -p "$work"/{BUILD,RPMS,SOURCES,SPECS,SRPMS,stage}

stage="$work/stage"
install -Dm 755 target/release/yasaff "$stage/yasaff"
strip "$stage/yasaff"

# Shared libraries the binary links, in RPM's soname form, e.g. libasound.so.2()(64bit).
suffix=""
[[ "$arch" == x86_64 || "$arch" == aarch64 ]] && suffix="(64bit)"
requires="$(objdump -p "$stage/yasaff" | awk '/NEEDED/ {print $2}' | grep -v '^ld-linux' \
    | sed "s/.*/Requires: &()$suffix/")"
[[ -n "$requires" ]] || { echo "objdump found no libraries" >&2; exit 1; }
# The newest glibc symbol version the binary uses, so dnf refuses distributions that are too old.
glibc="$(objdump -T "$stage/yasaff" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)"
[[ -n "$glibc" ]] && requires+=$'\n'"Requires: libc.so.6($glibc)$suffix"

cat > "$work/SPECS/yasaff.spec" <<EOF
Name:           yasaff
Version:        $version
Release:        1
Summary:        Console-style launcher for games on physical media
License:        GPL-3.0-only
URL:            https://github.com/naisyrkdev/yasaff
Packager:       Krystian Grzelak <krystian.grzelak@protonmail.com>
AutoReqProv:    no
$requires
# Loaded at run time by the window and graphics code, so objdump doesn't see them.
Requires:       libxkbcommon.so.0()$suffix
Requires:       libEGL.so.1()$suffix
Recommends:     (ffmpeg or ffmpeg-free)
Recommends:     xorriso

%global debug_package %{nil}
%global __os_install_post %{nil}

%description
YASAFF is a fullscreen, controller-driven menu that boots games from USB
sticks and discs, manages their saves, and installs apps and themes.
ffmpeg plays preview and idle videos; xorriso burns games to CD and DVD.

%install
install -Dm 755 $stage/yasaff %{buildroot}%{_bindir}/yasaff
install -Dm 644 $root/assets/icon-256.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/yasaff.png
install -Dm 644 $root/LICENSE %{buildroot}%{_datadir}/licenses/yasaff/LICENSE
install -Dm 644 $root/README.md %{buildroot}%{_docdir}/yasaff/README.md
install -m 644 $root/docs/*.md %{buildroot}%{_docdir}/yasaff/
mkdir -p %{buildroot}%{_datadir}/applications
cat > %{buildroot}%{_datadir}/applications/yasaff.desktop <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=YASAFF
Comment=Console-style launcher for games on physical media
Exec=yasaff
Icon=yasaff
Terminal=false
Categories=Game;
DESKTOP

%files
%{_bindir}/yasaff
%{_datadir}/applications/yasaff.desktop
%{_datadir}/icons/hicolor/256x256/apps/yasaff.png
%license %{_datadir}/licenses/yasaff/LICENSE
%doc %{_docdir}/yasaff
EOF

rpmbuild --define "_topdir $work" --target "$arch" -bb "$work/SPECS/yasaff.spec" >/dev/null

out_dir="$root/target/rpm"
mv "$work/RPMS/$arch/"*.rpm "$out_dir/"
rm -rf "$work"
echo
echo "Package: $(ls "$out_dir"/yasaff-"$version"-1*.rpm)"
