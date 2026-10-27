# Validation data

Calculations from other programs that APOST3Dview's own results are checked
against, one folder per program. Each folder holds the inputs, the
wavefunction files APOST3Dview reads, and cube files of a few orbitals
written by the program itself (or by Chemcraft, for Gaussian). APOST3Dview
evaluates the same orbitals at every grid point of the cube, and every one
must match to the precision of the cube file (RMS below 1e-5 and largest
difference below 1e-3 of the orbital's largest value; in practice RMS
1e-7 to 4e-6). Each folder's README says what it covers and what
was found.

| Folder | Program | Read from | What it covers |
|---|---|---|---|
| [`gaussian/`](gaussian/) | Gaussian 16 (cubes from Chemcraft) | `.fchk` | s-p, Cartesian d, pure d/f/g, an ECP, an unrestricted triplet |
| [`gaussian/cubegen/`](gaussian/cubegen/) | Gaussian 16 (its own cubegen) | `.fchk` | Cartesian d and f, diffuse functions, core orbitals, a restricted open-shell triplet, an MP2 density |
| [`pyscf/`](pyscf/) | PySCF 2.14 | `.molden` | s-p, Cartesian d, pure through g, an unrestricted triplet |
| [`pyscf/apost3d-py/`](pyscf/apost3d-py/) | PySCF 2.14, written by APOST-3D's apost3d.py | `.fchk` | CASSCF natural orbitals and density from the 1-RDM, diffuse functions (near linear dependence) |
| [`qchem/`](qchem/) | Q-Chem 6.3 | `.fchk` | pure d/f/g, Cartesian d, an unrestricted triplet |
| [`orca/`](orca/) | ORCA 6.1.1 | `.molden` | s-p-d, f, g, an unrestricted triplet, an ECP, CASSCF natural orbitals and density |
| [`orca/orca2fchk/`](orca/orca2fchk/) | ORCA 6.1.1, converted by APOST-3D's orca2fchk.py | `.fchk` | the same calculations as `orca/`, by the route ORCA users take to APOST-3D |
| [`apost3d/`](apost3d/) | APOST-3D 5 | `.apost.json` | the atomic weights (TFVC, Becke-rho) rebuilt from the results file, at reference points; EFOs from the unweighted file against APOST-3D's cube files |

The orbital comparisons run as `cargo test -p apost3dview-core --test reference_cubes`, the
atomic weights as `cargo test -p apost3dview-core --test atomic_weights`.
The Gaussian files also serve as ordinary inputs for the app's tests and
reference scenes. The densities from the Gaussian files are checked against
Gaussian's own density matrices in `crates/core/tests/densities.rs`.

To compare one orbital of another file by hand:

    cargo run --release -p apost3dview-core --example compare_cube -- file.fchk orbital.cube HOMO|LUMO|#N [ALPHA|BETA]
    cargo run --release -p apost3dview-core --example compare_cube_molden -- file.molden orbital.cube HOMO|LUMO|#N [ALPHA|BETA]

(`#N` numbers orbitals from 1.)
