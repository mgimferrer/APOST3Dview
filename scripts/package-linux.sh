#!/usr/bin/env bash
# Builds dist/APOST3Dview-linux.tar.gz (a folder APOST3Dview-<version>,
# without the version in the file name, so the README can link to the
# latest release's file directly): the executable,
# its icon and menu entry, install.sh to put them in place, and the
# examples/ folder.
# Used by .github/workflows/release.yml.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(cargo pkgid -p apost3dview | sed 's/.*@//')
cargo build --release --locked -p apost3dview

name=APOST3Dview-$version
dir=dist/$name
rm -rf dist && mkdir -p "$dir"
cp target/release/apost3dview "$dir/"
cp crates/app/assets/icon.png "$dir/apost3dview.png"
cp packaging/linux/APOST3Dview.desktop packaging/linux/install.sh LICENSE "$dir/"
cp -R examples "$dir/"
chmod +x "$dir/install.sh"
tar -C dist -czf dist/APOST3Dview-linux.tar.gz "$name"
rm -rf "$dir"
echo dist/APOST3Dview-linux.tar.gz
