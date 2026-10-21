#!/usr/bin/env bash
# Builds dist/APOST3Dview-linux.tar.gz (a folder APOST3Dview-<version>,
# without the version in the file name, so the README can link to the
# latest release's file directly): the executable,
# its icon and menu entry, install.sh to put them in place, the examples/
# folder and the user guide (guide/; build it first with mdbook build docs).
# Used by .github/workflows/release.yml.
set -euo pipefail
cd "$(dirname "$0")/.."

test -f docs/book/index.html || { echo "The user guide isn't built: run mdbook build docs first" >&2; exit 1; }
version=$(cargo pkgid -p apost3dview | sed 's/.*@//')
cargo build --release --locked -p apost3dview

name=APOST3Dview-$version
dir=dist/$name
rm -rf dist && mkdir -p "$dir"
cp target/release/apost3dview "$dir/"
cp crates/app/assets/icon.png "$dir/apost3dview.png"
cp packaging/linux/APOST3Dview.desktop packaging/linux/install.sh LICENSE "$dir/"
cp crates/render/assets/fonts/OFL.txt "$dir/Arimo-OFL.txt"
cp -R examples "$dir/"
cp -R docs/book "$dir/guide"
chmod +x "$dir/install.sh"
tar -C dist -czf dist/APOST3Dview-linux.tar.gz "$name"
rm -rf "$dir"
echo dist/APOST3Dview-linux.tar.gz
