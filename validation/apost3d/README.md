# APOST-3D atomic weights check

Since its results format 0.16, APOST-3D writes the weight functions of its
real-space atoms (BECKE, BECKE-RHO, TFVC) to the results file
(`atomic_weights`: the profile, the stiffness and the radius ratio of every
atom pair). APOST3Dview rebuilds w_A(r) of every atom from them, and the
weight of a fragment, w_F = Σ w_A over its atoms. Since 0.17 APOST-3D also
writes the unweighted EFOs (`orbital_file_unweighted`): exact basis-set
vectors φ whose product w_F·φ is the EFO, exactly what its cube files hold.
APOST3Dview draws w_F·φ.

Each `<job>.weights-check.json` holds 105 points (at and near the nuclei,
along the bonds, random) with every atom's and fragment's weight, from an
evaluator written by the APOST-3D side that reproduces APOST-3D to 1e-13;
`<job>.apost.json` is the results file of that run.

| Job | Atoms | Profile |
|---|---|---|
| `CH3F-GEOS` | TFVC | `tfvc` |
| `FeCO2-PBEPBE` | TFVC | `tfvc` |
| `CH3F-BECKE-RHO-EOS` | BECKE-RHO | `becke` |

## Results

All weights agree to 1e-15 (atoms and fragments), checked by
`crates/core/tests/atomic_weights.rs` (`atomic_weights_match_apost3d`).
The same file checks, on `examples/CH3F-GEOS`, that a weighted orbital grid
is the plain one times w_F at every point.

The formula is in APOST-3D's `docs/source/developer/json-output.md`
(section `atomic_weights`) and in `crates/core/src/atomic_weights.rs`.

## EFOs against APOST-3D's cube files (`efo-cubes/`)

Two complete runs from the APOST-3D side (schema 0.17), with the cube files
APOST-3D wrote of their EFOs (`CUBE` in `# METHOD`): `CH3F-EOS-CUBE` (EOS,
TFVC, fragments F and CH3, 15 EFOs) and `LiH-32-FCI` (GEOS of an FCI
wavefunction, a negative paired EFO). Each folder holds the main `.fchk`,
the input, the text and results files, both orbital files (fitted and
unweighted) and the cubes.

`crates/core/tests/efo_cubes.rs` evaluates w_F·φ at every point of every
cube (φ from the unweighted file, w_F rebuilt from the results file) and
matches each cube to the EFO of its fragment that agrees best (either sign).
All 16 agree to an RMS of at most 1.0e-6 and a largest deviation of 2.9e-5
of the cube's largest value, the precision of the cube's 5 printed digits.
For comparison, the fitted file drawn as stored deviates by up to 0.62 of
the cube's largest value (weakly occupied EFOs of CH3F, fits below 60%),
and times w_F once more (how APOST3Dview drew them with 0.16 files) by up
to 0.67.
