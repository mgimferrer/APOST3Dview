# Gaussian orbital check

Seven Gaussian 16 (A.03) B3LYP calculations, to check APOST3Dview's
orbital evaluator on `.fchk` files: the first validation set of the
evaluator (August 2026). Cubes of a few orbitals were made from each
`.fchk` with Chemcraft; APOST3Dview evaluates the same orbitals at the
cube's own grid points and compares them.

| Job | What it tests | Cubes |
|---|---|---|
| `H2O-631G` | s and p, combined SP shells | HOMO, LUMO |
| `H2O-631Gstar` | Cartesian d (6-31G* in Gaussian) | HOMO, LUMO |
| `H2O-ccpVDZ` | pure d | HOMO, LUMO |
| `H2O-ccpVTZ` | pure d and f | HOMO, LUMO |
| `H2O-T-ccpVTZ` | unrestricted triplet (`guess=mix`) | HOMO and LUMO, alpha and beta |
| `BiCl3-def2TZVPP` | ECP on Bi, pure through f | HOMO, LUMO, MO 45, MO 51 |
| `BiCl3-def2QZVPP` | ECP on Bi, pure g | HOMO, LUMO |

## Results

Every orbital matches Chemcraft's cube to numerical noise (RMS 1.5–5.6e-7
of the peak value). Checked by `crates/core/tests/reference_cubes.rs`
(`gaussian_fchk_orbitals_match_chemcraft_cubes`). The total and spin
densities of these files are checked against the density matrices Gaussian
writes into the `.fchk` (`crates/core/tests/densities.rs`).

Chemcraft's cubes start with a UTF-8 byte-order mark and have no
`ORBITAL_INDEX` line, unlike APOST-3D's; the cube reader accepts both.

## Files

Per job: the input (`.com`), the `.fchk`, and the cubes
(`<job>-<orbital>.cube`). Gaussian's `.chk` and `.log` aren't kept. The
BiCl3 inputs ask for `opt freq`; the `.fchk` holds the final geometry.
