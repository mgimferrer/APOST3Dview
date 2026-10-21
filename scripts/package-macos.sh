#!/usr/bin/env bash
# Builds dist/APOST3Dview-macos.dmg (no version in the file name, so the
# README can link to the latest release's file directly): one APOST3Dview.app
# holding both an Apple Silicon (arm64) and an Intel (x86_64) build,
# ad-hoc signed (free; Apple Silicon refuses to run unsigned code at all),
# next to the examples/ folder.
# Used by .github/workflows/release.yml; runs the same on a Mac by hand.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(cargo pkgid -p apost3dview | sed 's/.*@//')
export MACOSX_DEPLOYMENT_TARGET=11.0

for target in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target add "$target" >/dev/null
    cargo build --release --locked -p apost3dview --target "$target"
done

app=dist/APOST3Dview.app
rm -rf dist && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/APOST3Dview" \
    target/aarch64-apple-darwin/release/apost3dview target/x86_64-apple-darwin/release/apost3dview
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"

# The icon: every size macOS asks for, from the 512 px PNG.
iconset=dist/APOST3Dview.iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size crates/app/assets/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    if [ $double -le 512 ]; then
        sips -z $double $double crates/app/assets/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    fi
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/APOST3Dview.icns"
rm -rf "$iconset"

codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"

# The disk image: the app plus a shortcut to drag it onto, and the
# examples to try it with.
staging=dist/dmg
mkdir -p "$staging"
cp -R "$app" "$staging/"
cp -R examples "$staging/"
ln -s /Applications "$staging/Applications"
hdiutil create -quiet -volname "APOST3Dview $version" -srcfolder "$staging" -ov -format UDZO \
    dist/APOST3Dview-macos.dmg
rm -rf "$staging"
echo dist/APOST3Dview-macos.dmg
