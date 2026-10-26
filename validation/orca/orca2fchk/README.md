# ORCA files converted by orca2fchk.py

The four ORCA calculations of `../` (water RHF/cc-pVTZ, the O2 UHF triplet,
BiCl3 with the def2 ECP, and water CASSCF(4,4)), run again on the cluster
(2026-10-25; the CASSCF input also prints its determinants) and converted
to `.fchk` with APOST-3D's `utils/orca2fchk.py` (APOST-3D commit 142bedd),
the route by which ORCA users reach APOST-3D. `*.orca2fchk.log` is the
script's output; `H2O-ORCA-CAS44-def2SVP.dm1/.dm2` are the RDM files it
writes for a CASSCF. The orbitals are the same as those of the first run,
so they are compared with the cube files in `../`.

`crates/core/tests/reference_cubes.rs` (`orca2fchk_files_match_orca_cubes`)
compares every orbital at every point of ORCA's cube files, and the CASSCF
density through the natural orbitals of the file's density block: RMS at
most 1.1e-6, largest difference at most 1.9e-4 of the largest value, the
same as the Molden files of the same calculations.
