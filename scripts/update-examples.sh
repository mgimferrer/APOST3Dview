#!/bin/sh
# Refreshes examples/ from fixtures/, for when APOST-3D's reference results
# change (above all when they are regenerated for schema 1.0 on APOST-3D's
# release day). Run scripts/sync-apost3d-fixtures.sh first, then this, then
# check the examples still open as examples/README.md describes and commit.
#
# Each example is one complete APOST-3D run folder: the input, the .fchk,
# the text report, the results file and, where the analysis writes them,
# the orbital files (and the RDM files a correlated input names). A case
# with orbital files is taken from fixtures/apost3d-orbitals/, a run of its
# own (its run id differs from the reference run of the same name), with
# the input from fixtures/apost3d/, which is the same file.
#
# Usage: scripts/update-examples.sh
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
reference="$repo/fixtures/apost3d"
orbitals="$repo/fixtures/apost3d-orbitals"
if [ ! -d "$reference" ] || [ ! -d "$orbitals" ]; then
    echo "No fixtures/apost3d or fixtures/apost3d-orbitals: run scripts/sync-apost3d-fixtures.sh first." >&2
    exit 1
fi

for job in FeCN5NO3--UBLYP H2O-Dimer-B3LYP C2H6-OSLO-2CH3 CH3F-GEOS H2O-CAS44; do
    dest="$repo/examples/$job"
    if [ -f "$orbitals/$job.apost.json" ]; then
        src=$orbitals
    else
        src=$reference
    fi
    for file in "$src/$job.apost.json" "$src/$job.apost" "$src/$job.fchk" "$reference/$job.inp"; do
        if [ ! -f "$file" ]; then
            echo "Missing $file: examples/ left unchanged." >&2
            exit 1
        fi
    done
    rm -rf "$dest"
    mkdir -p "$dest"
    cp "$src/$job.apost.json" "$src/$job.apost" "$src/$job.fchk" "$reference/$job.inp" "$dest/"
    for file in "$reference/$job.dm1" "$reference/$job.dm2"; do
        [ ! -f "$file" ] || cp "$file" "$dest/"
    done
    if [ "$src" = "$orbitals" ]; then
        for file in "$orbitals/$job"-*.fchk; do
            [ ! -f "$file" ] || cp "$file" "$dest/"
        done
    fi
    echo "examples/$job: $(ls "$dest" | wc -l | tr -d ' ') files"
done
echo "From APOST-3D $(cat "$reference/APOST3D-COMMIT" 2>/dev/null || echo '(commit unknown)')."
