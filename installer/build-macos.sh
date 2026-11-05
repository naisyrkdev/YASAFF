#!/usr/bin/env bash
# Builds target/macos/YASAFF-<version>-macos-universal.dmg: YASAFF.app for Apple silicon and
# Intel Macs, with a link to Applications to drag it onto. It doesn't bundle ffmpeg; YASAFF uses
# Homebrew's or MacPorts' (brew install ffmpeg). The app is only ad-hoc signed, not notarized.
# Run it on macOS. Needs: rustup, and Xcode's command line tools (lipo, iconutil, codesign).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

for tool in cargo rustup lipo sips iconutil codesign hdiutil; do
    command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }
done

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
[[ -n "$version" ]] || { echo "No version in Cargo.toml" >&2; exit 1; }

export MACOSX_DEPLOYMENT_TARGET=11.0
targets=(aarch64-apple-darwin x86_64-apple-darwin)
rustup target add "${targets[@]}"
for target in "${targets[@]}"; do
    echo "Building yasaff $version for $target..."
    cargo build --release --locked --target "$target"
done

out_dir="$root/target/macos"
work="$out_dir/work"
rm -rf "$work"
app="$work/dmg/YASAFF.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

lipo -create -output "$app/Contents/MacOS/yasaff" \
    target/aarch64-apple-darwin/release/yasaff target/x86_64-apple-darwin/release/yasaff
strip -x "$app/Contents/MacOS/yasaff"

# The icon comes from the 256 px PNG, so the Retina sizes above it are upscaled.
iconset="$work/yasaff.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256; do
    sips -z "$size" "$size" assets/icon-256.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/icon-256.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/yasaff.icns"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>                 <string>YASAFF</string>
    <key>CFBundleDisplayName</key>          <string>YASAFF</string>
    <key>CFBundleIdentifier</key>           <string>io.github.naisyrkdev.yasaff</string>
    <key>CFBundleExecutable</key>           <string>yasaff</string>
    <key>CFBundleIconFile</key>             <string>yasaff</string>
    <key>CFBundlePackageType</key>          <string>APPL</string>
    <key>CFBundleShortVersionString</key>   <string>$version</string>
    <key>CFBundleVersion</key>              <string>$version</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key>       <string>$MACOSX_DEPLOYMENT_TARGET</string>
    <key>LSApplicationCategoryType</key>    <string>public.app-category.games</string>
    <key>NSHighResolutionCapable</key>      <true/>
</dict>
</plist>
EOF

cp LICENSE "$work/dmg/LICENSE.txt"
ln -s /Applications "$work/dmg/Applications"

# Ad-hoc signature: Apple silicon refuses to run unsigned code, and it keeps the bundle intact.
codesign --force --deep --sign - "$app"

dmg="$out_dir/YASAFF-$version-macos-universal.dmg"
rm -f "$dmg"
hdiutil create -volname "YASAFF $version" -srcfolder "$work/dmg" -fs HFS+ -format UDZO -ov "$dmg" >/dev/null
rm -rf "$work"
echo
echo "Disk image: $dmg"
