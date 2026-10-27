# PySCF CASSCF files written by APOST-3D's apost3d.py

Water CASSCF(4,4)/aug-cc-pVTZ (92 basis functions, smallest overlap
eigenvalue 3.9e-4: near linear dependence), computed with PySCF 2.14 and
written to `.fchk` by APOST-3D's `utils/apost3d.py` (GitHub MAJOR-UPDATE,
2026-10-27), the CASSCF one-particle density matrix in "Total SCF Density",
with PySCF's own cube files (`cubegen`, 80 points per side) of the two most
occupied active natural orbitals (named `-HONO`, `-LUNO` by the script) and of
the density, and PySCF's natural occupations (`-occupations.txt`). `run.py`
made them (it also makes the p-benzyne case of the manuscript, kept with the
manuscript's data).

`crates/core/tests/reference_cubes.rs`
(`apost3d_py_natural_orbitals_match_pyscf`) builds the natural orbitals from
the `.fchk` (overlap-free, from the file's MOs and density block) and
compares at every point: occupations to 2.9e-9, the two orbitals to an RMS of
2.0e-6 and a largest difference of 1.5e-4 of the peak, the density to 8.2e-7
and 3.2e-4.
