# Q-Chem orbital check

Four Q-Chem single points of water, to check that APOST3Dview reads
Q-Chem `.fchk` files correctly (orbital order, pure/Cartesian functions,
open shell). Each run writes its `.fchk` (`GUI 2`) and cube files of a few
orbitals (`$plots`, `MAKE_CUBE_FILES`); APOST3Dview then evaluates the same
orbitals from the `.fchk` at the cube's own grid points and compares them
(`compare_cube` example), as was done for Gaussian and PySCF.

| Input | What it tests | Cubes |
|---|---|---|
| `H2O-QC-ccpVTZ.in` | pure d and f | MO 5 (HOMO), 6 (LUMO) |
| `H2O-QC-631Gstar-cart.in` | Cartesian d (`PURECART 2222`) | MO 5, 6 |
| `H2O-QC-ccpVQZ.in` | pure g | MO 5, 6 |
| `H2O-QC-T-ccpVTZ.in` | unrestricted triplet | alpha 6, 7; beta 4, 5 |

The `$plots` sections use Q-Chem's current format (`PLOTS TRUE` with
`grid_range`, `grid_points`, `alpha_molecular_orbital`,
`beta_molecular_orbital`; a first version of these inputs used the old
layout, which recent Q-Chem refuses with "Invalid entry under the new
format for the $plots section"). `SYM_IGNORE TRUE` keeps the molecule
where the input puts it, so the `.fchk` and the cubes share one frame.

## Results (2026-10-21)

Run with Q-Chem 6.3.0 (the `.qchem` files are the outputs, the `.plots/`
folders hold the cubes). Every orbital matches Q-Chem's own cube to
numerical noise, unchanged code: RMS 1.3–2.1e-7 of the peak value, for
pure d and f, Cartesian d (19 basis functions), pure g, and both spin
channels of the triplet. Checked by `crates/core/tests/reference_cubes.rs`
(`qchem_fchk_orbitals_match_qchem_cubes`).

## Files

Per job: the input (`.in`), Q-Chem's output (`.qchem`), the `.fchk`, and
the cubes in `<job>.plots/` (`mo.N.cube` alpha, `mo_b.N.cube` beta).
