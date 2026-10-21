#!/bin/sh
# Makes every picture of the user guide (docs/src/images/) again, from the
# app itself: crates/app/tests/guide.rs follows each chapter's steps in the
# real app and takes its pictures. Then builds the guide (docs/book/), if
# mdBook is installed. Run it after a change to the app's windows or look,
# check the pictures (git diff shows which changed), and commit them.
#
# Needs a GPU (or a software one), like the app's other tests.
#
# Usage: scripts/guide-images.sh
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
images="$repo/docs/src/images"

# Start from an empty folder (the logo stays), so a picture the guide no
# longer takes doesn't linger.
find "$images" -name '*.png' ! -name 'logo.png' -delete
APOST3DVIEW_GUIDE_IMAGES="$images" cargo test --manifest-path "$repo/Cargo.toml" -p apost3dview --test guide -- --test-threads=2
echo "Pictures written to docs/src/images/."

cargo test --manifest-path "$repo/Cargo.toml" -p apost3dview --test guide_text

if command -v mdbook >/dev/null 2>&1; then
    mdbook build "$repo/docs"
    echo "Guide built: docs/book/index.html"
else
    echo "mdBook isn't installed (cargo install mdbook), so the guide wasn't built."
fi
