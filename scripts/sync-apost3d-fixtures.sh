#!/bin/sh
# Copies APOST-3D's reference results into fixtures/, for the viewer's own
# tests (crates/core/tests/apost_fixtures.rs, the app's tests):
#   fixtures/apost3d/: from tests/reference/ and tests/inputs/
#     <job>.apost.json   the results files
#     <job>.apost        their text reports
#     <job>.fchk         the wavefunctions they were computed from
#   fixtures/apost3d-orbitals/: the cases with EOS/GEOS/OSLO orbital files,
#     from VIEWER-HANDOFF/orbital-fixtures/ (next to the APOST3D checkout),
#     each complete like a run's folder: results, report, orbital .fchk, and
#     the main .fchk from tests/inputs/
# Rerun it whenever APOST-3D's results change (with each new brief). The
# APOST3D checkout is only read, never written. fixtures/ is not tracked
# by git.
#
# Usage: scripts/sync-apost3d-fixtures.sh [path/to/APOST3D]
# Default: the APOST3D checkout next to this repository
# (../APOST3D-MAJORCHANGE/APOST3D).
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
src=${1:-"$repo/../APOST3D-MAJORCHANGE/APOST3D"}
if [ ! -d "$src/tests/reference" ] || [ ! -d "$src/tests/inputs" ]; then
    echo "No APOST3D checkout at $src (expected tests/reference and tests/inputs)." >&2
    echo "Usage: $0 [path/to/APOST3D]" >&2
    exit 1
fi
src=$(cd "$src" && pwd)
dest="$repo/fixtures/apost3d"

rm -rf "$dest"
mkdir -p "$dest"
count=0
for json in "$src"/tests/reference/*.apost.json; do
    job=$(basename "$json" .apost.json)
    cp "$json" "$dest/"
    [ -f "$src/tests/reference/$job.apost" ] && cp "$src/tests/reference/$job.apost" "$dest/"
    if [ -f "$src/tests/inputs/$job.fchk" ]; then
        cp "$src/tests/inputs/$job.fchk" "$dest/"
    else
        echo "warning: no tests/inputs/$job.fchk for $job" >&2
    fi
    count=$((count + 1))
done
commit=$(git -C "$src" rev-parse --short HEAD 2>/dev/null || echo unknown)
echo "Copied $count results files from $src (commit $commit) into $dest."
echo "$commit" > "$dest/APOST3D-COMMIT"

handoff="$src/../VIEWER-HANDOFF/orbital-fixtures"
orbitals="$repo/fixtures/apost3d-orbitals"
rm -rf "$orbitals"
if [ -d "$handoff" ]; then
    mkdir -p "$orbitals"
    cp "$handoff"/* "$orbitals/"
    cases=0
    for json in "$orbitals"/*.apost.json; do
        job=$(basename "$json" .apost.json)
        if [ -f "$src/tests/inputs/$job.fchk" ]; then
            cp "$src/tests/inputs/$job.fchk" "$orbitals/"
        else
            echo "warning: no tests/inputs/$job.fchk for orbital case $job" >&2
        fi
        cases=$((cases + 1))
    done
    echo "Copied $cases orbital-file cases from $handoff into $orbitals."
else
    echo "No $handoff: orbital-file cases not copied." >&2
fi
