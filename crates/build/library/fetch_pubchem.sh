#!/bin/sh
# Downloads the PubChem 3D conformers listed in pubchem.txt into $1 (one
# <key>.sdf each), for examples/make_library.rs.
set -e
out=${1:?usage: fetch_pubchem.sh <folder>}
mkdir -p "$out"
here=$(dirname "$0")
grep -v '^#' "$here/pubchem.txt" | while read -r key cid name; do
  [ -n "$key" ] || continue
  curl -sf -o "$out/$key.sdf" "https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound/cid/$cid/SDF?record_type=3d" || echo "no 3D conformer: $key ($cid, $name)"
  sleep 0.25
done
