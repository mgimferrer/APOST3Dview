#!/bin/sh
# Refreshes examples/ from fixtures/, for when APOST-3D's reference results
# change (above all when they are regenerated for schema 1.0 on APOST-3D's
# release day). Run scripts/sync-apost3d-fixtures.sh first, then this, then
# check the examples still open as the guide's tour describes (the tests
# examples.rs and guide.rs do), regenerate its pictures if they changed
# (scripts/guide-images.sh), and commit.
#
# Each example is one complete APOST-3D run folder: the input, the .fchk,
# the text report, the results file and, where the analysis writes them,
# the orbital files (and the RDM files a correlated input names). A case
# with orbital files is taken from fixtures/apost3d-orbitals/ (since APOST3D
# b9e8eee the same run as the reference results of that name, with its
# orbital files from tests/reference-orbitals/), with the input from
# fixtures/apost3d/.
#
# The three case-study examples (FeCp2-EOS-OSLO, FeCp2-IQA, FeCO6-EOS) come
# from fixtures/case-studies/ instead, see the end of this script.
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

# The paper's case studies that are examples too (Martí's calculations, run
# with APOST-3D on his cluster, in fixtures/case-studies/<system>/, local
# only): refreshed from there when it is here. After a results-format
# change, they are run again on the cluster first.
cases="$repo/fixtures/case-studies"
if [ ! -d "$cases" ]; then
    echo "No fixtures/case-studies: FeCp2-EOS-OSLO, FeCp2-IQA and FeCO6-EOS left as they are."
    exit 0
fi
for entry in FeCp2/FeCp2-EOS-OSLO FeCp2/FeCp2-IQA FeCO6/FeCO6-EOS; do
    job=$(basename "$entry")
    src="$cases/$(dirname "$entry")"
    dest="$repo/examples/$job"
    for file in "$src/$job.apost.json" "$src/$job.apost" "$src/$job.fchk" "$src/$job.inp"; do
        if [ ! -f "$file" ]; then
            echo "Missing $file: examples/$job left unchanged." >&2
            continue 2
        fi
    done
    rm -rf "$dest"
    mkdir -p "$dest"
    cp "$src/$job.apost.json" "$src/$job.apost" "$src/$job.fchk" "$src/$job.inp" "$dest/"
    for file in "$src/$job"-*.fchk; do
        [ ! -f "$file" ] || cp "$file" "$dest/"
    done
    # Free CO, for comparing the carbonyl's EFOs with its MOs.
    if [ "$job" = FeCO6-EOS ] && [ -f "$src/CO.fchk" ]; then
        cp "$src/CO.fchk" "$dest/"
    fi
    echo "examples/$job: $(ls "$dest" | wc -l | tr -d ' ') files"
done
