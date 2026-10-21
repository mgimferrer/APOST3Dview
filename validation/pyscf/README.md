# PySCF orbital check

Four PySCF Hartree–Fock calculations, to check that APOST3Dview reads
PySCF's `.molden` files correctly (component order, pure and Cartesian
functions, open shell). `gen_molden.py` writes each `.molden`
(`pyscf.tools.molden`) and cube files of its HOMO and LUMO
(`pyscf.tools.cubegen`); APOST3Dview evaluates the same orbitals from the
`.molden` at the cube's own grid points and compares them.

| File | What it tests | Cubes |
|---|---|---|
| `h2o_631g` | s and p | HOMO, LUMO |
| `h2o_631gstar_cart` | Cartesian d (`cart=True`) | HOMO, LUMO |
| `h2o_ccpvqz_pure` | pure d, f and g | HOMO, LUMO |
| `o2_triplet_ccpvtz` | unrestricted triplet, pure through f | HOMO and LUMO, alpha and beta |

## Results (2026-08-29)

Every orbital matches PySCF's cube to numerical noise (RMS 2–5e-6 of the
peak value; the cubes' five significant digits are the limit). Checked by
`crates/core/tests/reference_cubes.rs`
(`pyscf_molden_orbitals_match_pyscf_cubes`). PySCF writes the standard
Molden component order, which is also the order the `.fchk` path uses, so
no reordering was needed.

## Remaking the files

    python -m venv venv && venv/bin/pip install -r requirements.txt
    venv/bin/python gen_molden.py .

`requirements.txt` pins the PySCF version the files were made with.
