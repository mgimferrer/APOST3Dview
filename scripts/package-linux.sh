#!/usr/bin/env bash
# Builds dist/APOST3Dview-<version>-linux-x86_64.tar.gz: the executable,
# its icon and menu entry, and install.sh to put them in place.
# Used by .github/workflows/release.yml.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(cargo pkgid -p apost3dview | sed 's/.*@//')
cargo build --release --locked -p apost3dview

name=APOST3Dview-$version-linux-x86_64
dir=dist/$name
rm -rf dist && mkdir -p "$dir"
cp target/release/apost3dview "$dir/"
cp crates/app/assets/icon.png "$dir/apost3dview.png"
cp packaging/linux/APOST3Dview.desktop packaging/linux/install.sh LICENSE "$dir/"
chmod +x "$dir/install.sh"
tar -C dist -czf "dist/$name.tar.gz" "$name"
rm -rf "$dir"
echo "dist/$name.tar.gz"
