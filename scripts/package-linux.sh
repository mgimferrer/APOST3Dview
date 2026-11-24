#!/usr/bin/env bash
# Builds dist/APOST3Dview-linux.tar.gz (a folder APOST3Dview-<version>,
# without the version in the file name, so the README can link to the
# latest release's file directly): the executable,
# its icon and menu entry, install.sh to put them in place, the examples/
# folder and the user guide (guide/; build it first with mdbook build docs).
# Used by .github/workflows/release.yml.
#
# Built with cargo-zigbuild against glibc 2.17, so the program runs on any
# Linux from 2014 on (RHEL/CentOS/Rocky 7 and 8, older Ubuntu), not only on
# systems as new as the build machine: a program needs a glibc at least as
# new as the one it was linked against. Nothing else is linked: Vulkan,
# OpenGL, X11 and Wayland are loaded at run time. Needs zig and
# cargo-zigbuild (pip install ziglang cargo-zigbuild).
set -euo pipefail
cd "$(dirname "$0")/.."

test -f docs/book/index.html || { echo "The user guide isn't built: run mdbook build docs first" >&2; exit 1; }
version=$(cargo pkgid -p apost3dview | sed 's/.*@//')
glibc=2.17
target=x86_64-unknown-linux-gnu
cargo zigbuild --release --locked -p apost3dview --target "$target.$glibc"
binary=target/$target/release/apost3dview

# The newest glibc symbol version the executable asks for must not be above
# the one built against.
newest=$(objdump -T "$binary" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
if [ "$(printf '%s\n%s\n' "$newest" "$glibc" | sort -V | tail -1)" != "$glibc" ]; then
    echo "The executable needs glibc $newest, above $glibc" >&2
    exit 1
fi
echo "The executable needs glibc $newest or later"

name=APOST3Dview-$version
dir=dist/$name
rm -rf dist && mkdir -p "$dir"
cp "$binary" "$dir/"
cp crates/app/assets/icon.png "$dir/apost3dview.png"
cp packaging/linux/APOST3Dview.desktop packaging/linux/install.sh LICENSE "$dir/"
cp crates/render/assets/fonts/OFL.txt "$dir/Arimo-OFL.txt"
cp -R examples "$dir/"
cp -R docs/book "$dir/guide"
chmod +x "$dir/install.sh"
tar -C dist -czf dist/APOST3Dview-linux.tar.gz "$name"
rm -rf "$dir"
echo dist/APOST3Dview-linux.tar.gz
