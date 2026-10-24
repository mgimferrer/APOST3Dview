# File formats and what is validated

## Files APOST3Dview opens

| File | Written by | What APOST3Dview reads |
|---|---|---|
| `.fchk` | Gaussian, Q-Chem, PySCF (through APOST-3D's `apost3d.py`) | The geometry, the basis set and the molecular orbitals (restricted, restricted open shell, unrestricted), and the density matrices stored in it, mean-field and correlated |
| `.molden` | PySCF, ORCA (`orca_2mkl -molden`) | The geometry, the basis set and the orbitals with their occupations |
| `.cube` | Any program (Gaussian cube format) | The geometry and the grid of values (the first one, when a file holds several) |
| `.xyz` | Any program | The geometry |
| `.apost.json` | APOST-3D | APOST-3D's results (see [APOST-3D results on the molecule](results.md)) |
| `.a3dview` | APOST3Dview | A session (see [Sessions](sessions.md)) |

Basis functions up to g are supported, pure and Cartesian (Cartesian g excepted), with or without effective core potentials (ECPs). With an ECP, densities are valence densities, as in the files themselves. ORCA's own conventions in `.molden` files (the normalization in the coefficients and the sign of some f and g functions) are recognized and undone.

APOST-3D's input files also end in `.inp`, like some Molden files; APOST3Dview tells them apart and explains that an APOST-3D input can't be opened (open the job's `.fchk` or `.apost.json` instead).

## What is validated

The orbitals APOST3Dview computes were compared, at every point of the grid, with the cube files the programs write themselves. The calculations and the cubes are in the [`validation` folder](https://github.com/mgimferrer/APOST3Dview/tree/main/validation) of the source repository, one folder per program with a README, and the comparisons run as tests on every change to the code.

| Program | File | What it covers | Largest difference (RMS, of the peak value) |
|---|---|---|---|
| Gaussian 16 (cubes from Chemcraft) | `.fchk` | s, p, Cartesian d, pure d, f and g, an ECP, an unrestricted triplet | 1.5 to 5.6 × 10⁻⁷ |
| Q-Chem 6.3 | `.fchk` | pure d, f and g, Cartesian d, an unrestricted triplet | 1.3 to 2.1 × 10⁻⁷ |
| PySCF 2.14 | `.molden` | s, p, Cartesian d, pure up to g, an unrestricted triplet | 2 to 5 × 10⁻⁶ (the cubes' five digits) |
| ORCA 6.1.1 | `.molden` | s, p, d, f, g, an unrestricted triplet, an ECP, CASSCF natural orbitals and density | at most 1.6 × 10⁻⁶ |

Densities were checked against the density matrices Gaussian writes itself, evaluated independently at every grid point (water at several basis sets, BiCl3 with an ECP and g functions, a triplet's total and spin densities): they agree to about 2 × 10⁻⁷ of the value. Correlated densities, through their natural orbitals, agree with their density matrices to 2 to 7 × 10⁻⁷ (CASSCF, CISD and FCI from PySCF, and Gaussian's MP2 density).

On the APOST-3D side, APOST3Dview is tested on every change against APOST-3D's own reference results: every file loads, every value it lists can be read, and the input APOST3Dview writes is read back as APOST-3D reads it. The atomic weights APOST3Dview rebuilds from a results file agree with APOST-3D's own at 105 points per system to 10⁻¹⁵ (TFVC and Becke-rho atoms), and the EFOs it draws from them and APOST-3D's unweighted orbital file agree with APOST-3D's own cube files of the same EFOs at every point of 16 cubes, to the 5 digits the cube files are written with.

These files open like any other, and are a quick way to try orbitals and densities.
