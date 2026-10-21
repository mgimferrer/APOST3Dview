# Examples

Five complete APOST-3D calculations, to try APOST3Dview without running
APOST-3D first. They are test systems of APOST-3D itself: each folder is one
run, as APOST-3D leaves it.

**What to look at in each** is in the user guide's
[five-minute tour](https://mgimferrer.github.io/APOST3Dview/tour.html)
(in a download, also `guide/tour.html` next to this folder). To open one,
open its `.apost.json` (Open, or drag it onto the window): the molecule
opens with its results.

| Folder | System | What it shows |
|---|---|---|
| `FeCN5NO3--UBLYP` | [Fe(CN)5NO]³⁻, UBLYP/def2-SVP, Löwdin atoms, seven fragments | Fragments, charges, bond orders, EOS oxidation states |
| `H2O-Dimer-B3LYP` | Water dimer, B3LYP/cc-pVDZ, TFVC atoms | Energy decomposition (IQA) |
| `C2H6-OSLO-2CH3` | Ethane, B3LYP/cc-pVTZ, TFVC atoms, two CH3 | OSLOs, the C-C bond shared in two colors |
| `CH3F-GEOS` | Fluoromethane, BP86/def2-SVP, TFVC atoms, F and CH3 | GEOS oxidation states and effective fragment orbitals |
| `H2O-CAS44` | Water, CASSCF(4,4)/STO-3G from PySCF, TFVC atoms | Correlated density, natural orbitals, local spin |

In each folder:

| File | What it is |
|---|---|
| `<job>.inp` | the APOST-3D input |
| `<job>.fchk` | the wavefunction (Gaussian or PySCF) |
| `<job>.apost` | APOST-3D's text report |
| `<job>.apost.json` | the results file APOST3Dview reads |
| `<job>-*.fchk` | orbital files written by EOS, GEOS or OSLO, where there are any |
| `<job>.dm1`, `<job>.dm2` | the density matrices of a correlated wavefunction (only needed to run APOST-3D again) |

## About these files

They are APOST-3D's own test cases. The C2H6 and CH3F folders are runs made
for APOST3Dview with the orbital files written; the others are APOST-3D's
reference results. The FeCN5NO3 folder doesn't include its EFO file, so its
EFOs are listed but can't be drawn. The examples are refreshed whenever
APOST-3D's results format changes (with `scripts/update-examples.sh` in the
source repository), so they always open in the version of APOST3Dview they
come with.
