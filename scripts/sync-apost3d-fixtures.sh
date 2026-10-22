#!/bin/sh
# Copies APOST-3D's reference results into fixtures/, for the viewer's own
# tests (crates/core/tests/apost_fixtures.rs, the app's tests):
#   fixtures/apost3d/: from tests/reference/ and tests/inputs/
#     <job>.apost.json   the results files
#     <job>.apost        their text reports
#     <job>.fchk         the wavefunctions they were computed from
#     <job>.inp          every reference input (the input generator's
#                        tests), with its .fchk and, for a correlated
#                        wavefunction, its RDM files (.dm1, .dm2)
#   fixtures/apost3d-orbitals/: the cases with EOS/GEOS/OSLO orbital files,
#     from tests/reference-orbitals/ (since APOST3D b9e8eee; from
#     VIEWER-HANDOFF/orbital-fixtures/ next to an older checkout), each
#     complete like a run's folder: the orbital .fchk files, the results and
#     report of the same run from tests/reference/, and the main .fchk from
#     tests/inputs/
# Rerun it whenever APOST-3D's results change (with each new brief). The
# APOST3D checkout is only read, never written. fixtures/ is not tracked
# by git.
#
# Usage: scripts/sync-apost3d-fixtures.sh [path/to/APOST3D]
#        scripts/sync-apost3d-fixtures.sh --pinned
# Default: the APOST3D checkout next to this repository
# (../APOST3D-MAJORCHANGE/APOST3D). With --pinned, the commit named in
# scripts/apost3d-commit is fetched from GitHub instead (tests/ only, about
# 25 MB, into fixtures/apost3d-source/): what CI does, and no APOST-3D
# checkout needed.
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
pinned=$(grep -v '^#' "$repo/scripts/apost3d-commit" | head -n 1 | tr -d '[:space:]')
if [ "${1:-}" = "--pinned" ]; then
    src="$repo/fixtures/apost3d-source"
    rm -rf "$src"
    git init -q "$src"
    git -C "$src" config core.autocrlf false
    git -C "$src" remote add origin https://github.com/mgimferrer/APOST3D.git
    git -C "$src" sparse-checkout set tests/reference tests/reference-orbitals tests/inputs
    git -C "$src" fetch -q --depth 1 --filter=blob:none origin "$pinned"
    git -C "$src" checkout -q FETCH_HEAD
else
    src=${1:-"$repo/../APOST3D-MAJORCHANGE/APOST3D"}
fi
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
inputs=0
for inp in "$src"/tests/inputs/*.inp; do
    job=$(basename "$inp" .inp)
    cp "$inp" "$dest/"
    [ -f "$dest/$job.fchk" ] || [ ! -f "$src/tests/inputs/$job.fchk" ] || cp "$src/tests/inputs/$job.fchk" "$dest/"
    for rdm in "$src/tests/inputs/$job.dm1" "$src/tests/inputs/$job.dm2"; do
        [ ! -f "$rdm" ] || cp "$rdm" "$dest/"
    done
    inputs=$((inputs + 1))
done
echo "Copied $inputs reference inputs."
commit=$(git -C "$src" rev-parse --short HEAD 2>/dev/null || echo unknown)
echo "Copied $count results files from $src (commit $commit) into $dest."
echo "$commit" > "$dest/APOST3D-COMMIT"
case "$pinned" in
    "$commit"*) ;;
    *) echo "Note: CI tests against APOST-3D $pinned (scripts/apost3d-commit), not $commit." ;;
esac

# The orbital-file cases: each orbital .fchk with the results of its run
# (same name before the suffix, same run id: APOST-3D regenerates them
# together) and the main .fchk.
orbitals="$repo/fixtures/apost3d-orbitals"
handoff="$src/../VIEWER-HANDOFF/orbital-fixtures"
if [ -d "$src/tests/reference-orbitals" ]; then
    rm -rf "$orbitals"
    mkdir -p "$orbitals"
    for file in "$src"/tests/reference-orbitals/*.fchk; do
        job=$(basename "$file" | sed -E 's/-(EOS-EFOs|GEOS-EFOs|OSLOs-preortho|OSLOs)\.fchk$//')
        cp "$file" "$orbitals/"
        for f in "$src/tests/reference/$job.apost.json" "$src/tests/reference/$job.apost" "$src/tests/inputs/$job.fchk"; do
            if [ -f "$f" ]; then
                cp "$f" "$orbitals/"
            else
                echo "warning: no $f for orbital file $(basename "$file")" >&2
            fi
        done
    done
    cases=$(ls "$orbitals"/*.apost.json | wc -l | tr -d ' ')
    echo "Copied $cases orbital-file cases from $src/tests/reference-orbitals into $orbitals."
elif [ -d "$handoff" ]; then
    rm -rf "$orbitals"
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
    echo "No tests/reference-orbitals or $handoff: fixtures/apost3d-orbitals left as it was." >&2
fi
