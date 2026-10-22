# APOST-3D atomic weights check

Since its results format 0.16, APOST-3D writes the weight functions of its
real-space atoms (BECKE, BECKE-RHO, TFVC) to the results file
(`atomic_weights`: the profile, the stiffness and the radius ratio of every
atom pair). APOST3Dview rebuilds w_A(r) of every atom from them, and the
weight of a fragment, w_F = Σ w_A over its atoms, to draw the EFOs of the
EOS/GEOS orbital files times their fragment's weight (the fit's tails on
other fragments go).

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
