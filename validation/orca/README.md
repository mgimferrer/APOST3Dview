# ORCA orbital check

Six ORCA (6.1 or later) single points, to check that APOST3Dview reads
ORCA's `.molden` files correctly. ORCA is known to deviate from the
Molden conventions for f and g functions (sign and normalization of some
components), so the cases cover f and g, an open shell, an ECP, and
natural orbitals with fractional occupations. Each job writes cube files
of a few orbitals (`%plots`); APOST3Dview then evaluates the same
orbitals from the `.molden` file at the cube's own grid points and
compares them (`compare_cube_molden` example), as was done for Gaussian
and PySCF.

| Input | What it tests | Cubes (ORCA numbering, from 0) |
|---|---|---|
| `H2O-ORCA-def2SVP.inp` | s, p, pure d | MO 4 (HOMO), 5 (LUMO) |
| `H2O-ORCA-ccpVTZ.inp` | pure f | MO 4, 5 |
| `H2O-ORCA-ccpVQZ.inp` | pure g | MO 4, 5 |
| `O2-ORCA-T-ccpVTZ.inp` | unrestricted triplet | alpha 8, 9; beta 6, 7 |
| `BiCl3-ORCA-def2TZVPP.inp` | ECP on Bi (def2-ECP, added automatically), f on Bi | MO 36 (HOMO), 37 (LUMO) |
| `H2O-ORCA-CAS44-def2SVP.inp` | CASSCF(4,4) natural orbitals, fractional occupations | MO 4, 5, and the total density |

The geometries are those of the Gaussian test files (`../gaussian/H2O-ccpVTZ.fchk`,
`../gaussian/BiCl3-def2TZVPP.fchk`), so the molecules match the cases already
validated; O2 is at 1.207 Å.

## Running

On the cluster, with ORCA's folder in `ORCA_DIR` (or `orca` on the PATH):

    ORCA_DIR=/path/to/orca_6_1 ./run_all.sh

This runs every input and converts each `.gbw` to Molden with
`orca_2mkl <job> -molden`, renamed to `<job>.molden`. Each job is small
(seconds to a few minutes serially; cc-pVQZ and BiCl3 the longest). If a
job fails, the others still give useful checks: send what worked.

If ORCA rejects the `%plots` block (its syntax has changed between
versions), the `.molden` files alone are still useful; the cubes can then
be made afterwards with `orca_plot <job>.gbw -i` (format 7, Gaussian cube).

## Results (2026-10-21)

Run with ORCA 6.1.1 (`submit_orca.sh`: 48 cores, 3 GB per core). Kept per
job: the input, ORCA's output (`.out`), the `.molden` from `orca_2mkl`,
and the cubes; ORCA's intermediate files (`.gbw`, `.densities`,
`.property.txt`, the Bi atom job `*_atom83.*`) are not kept.

At first every orbital was off by 7–31%: `orca_2mkl` departs from the
Molden conventions in two ways, both now undone by the parser for files
whose `[Title]` names `orca_2mkl` (`molden::from_orca_conventions`):

1. each contraction coefficient includes its primitive's normalization,
   (2a/pi)^(3/4) (4a)^(L/2) / sqrt((2L-1)!!), so it is divided out and
   each contracted function normalized again;
2. the F+3, F-3 and G+3, G-3, G+4, G-4 components have the opposite sign.

After that, all 14 orbitals match ORCA's cubes (RMS ≤ 1.6e-6 of the peak):
s-p-d, f, g, the UHF triplet, BiCl3 with def2-ECP (the `.molden` lists Bi
with Z = 83 and its 23 valence electrons in `[Pseudo]`; the density is
the valence density, as for Gaussian's ECP files), and the CASSCF natural
orbitals; ORCA's CASSCF density equals the density of the natural
orbitals with their fractional occupations (4e-7). Checked by
`crates/core/tests/reference_cubes.rs`; the app opens these files in
`crates/app/tests/molden_files.rs`.

Not covered by this data: other ORCA versions (detection relies on the
title line of 6.1.1), ROHF files, and `.molden` files converted from a
`.gbw` by other tools (Multiwfn, MOKIT), which are read as standard.
