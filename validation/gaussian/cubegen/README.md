# Gaussian's own cube files (cubegen)

Cases the Chemcraft set of `../` lacks, with cube files written by Gaussian
16 (A.03) itself: four single points of water and CH2 (`run_new.sh`, each
followed by `formchk`) and `cubegen` cubes of 80 points per side
(`run_cubes.sh`, the lines of `cubes.txt`). Run on the cluster for the
manuscript's revision, 2026-10-25.

| Job | What it covers | Cubes |
|---|---|---|
| `H2O-631G2df-cart` | B3LYP/6-31G(2df) with `6D 10F`: Cartesian d and f | HOMO, LUMO, MO 1 (O 1s) |
| `H2O-augccpVTZ` | B3LYP/aug-cc-pVTZ: diffuse functions | HOMO, LUMO, MO 1 |
| `CH2-T-ROHF-ccpVTZ` | ROHF/cc-pVTZ triplet: restricted open shell | HOMO (singly occupied), LUMO, MO 3 |
| `H2O-MP2-ccpVTZ` | MP2/cc-pVTZ, `density=current`: a correlated density | the MP2 density |

`crates/core/tests/reference_cubes.rs`
(`gaussian_fchk_orbitals_and_mp2_density_match_cubegen`) compares them at
every point. All orbitals agree to an RMS of at most 1.6e-6 and a largest
difference of at most 2.2e-4 of the orbital's largest value; the MP2
density, evaluated through the natural orbitals of the file's MP2 density
matrix, to 1.1e-6 and 4.0e-4.

Note: `cubegen` leaves out the frozen-core orbital (the O 1s) of the
densities of an MP2 frozen-core file, the SCF density as well as the MP2
one; APOST3Dview's density includes it, as the density matrix does. The
test subtracts it before comparing. Without that, the two differ only
within about half a bohr of the oxygen nucleus.

The same comparison was made, but is not stored here (about 230 MB of
cubes), for the Chemcraft set of `../` (all 18 orbitals, with MO 1 of the
water files) and for ferrocene (B3LYP/def2-TZVPP, 514 basis functions) and
nitroprusside (B3LYP/def2-TZVP, 417), HOMO and LUMO: RMS at most 1.6e-6,
largest difference at most 2.3e-4. Those cubes are in the manuscript's data
deposit.
