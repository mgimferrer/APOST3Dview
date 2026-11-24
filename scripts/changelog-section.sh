#!/usr/bin/env sh
# Prints the section of CHANGELOG.md for one version (the lines under its
# "## X.Y.Z (date)" heading, up to the next "## "), for the release notes.
# Fails when the version has no section, so a release can't go out without
# saying what changed. Usage: scripts/changelog-section.sh 1.1.0
set -eu
cd "$(dirname "$0")/.."
version=$1
section=$(awk -v v="$version" '
    /^## / { inside = ($2 == v); if (inside) found = 1; next }
    inside { print }
    END { if (!found) exit 1 }
' CHANGELOG.md) || { echo "CHANGELOG.md has no section for $version: move Unreleased under \"## $version (date)\"" >&2; exit 1; }
# Without the blank lines at its start and end.
printf '%s\n' "$section" | sed -e '/./,$!d'
