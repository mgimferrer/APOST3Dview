# Examples

Eight complete APOST-3D calculations, to try APOST3Dview without running
APOST-3D first. Five are test systems of APOST-3D itself, three come from
published bonding analyses: each folder is one run, as APOST-3D leaves it.

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
| `FeCp2-EOS-OSLO` | Ferrocene, B3LYP/def2-TZVPP, TFVC atoms, Fe and two Cp | EOS and OSLO: the Fe d and Cp pi effective fragment orbitals, OSLOs with their FOLI |
| `FeCp2-IQA` | Ferrocene, as above | Energy decomposition (IQA) between Fe and the rings, and atom by atom |
| `FeCO6-EOS` | [Fe(CO)6]²⁺, B3LYP/def2-TZVP, TFVC atoms, Fe and six CO | The sigma, pi and pi* effective fragment orbitals of a carbonyl |

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

The first five are APOST-3D's own test cases. The C2H6 and CH3F folders are
runs made for APOST3Dview with the orbital files written; the other three of
them are APOST-3D's reference results. The ferrocene and [Fe(CO)6]²⁺ folders
are the systems of published figures (APOST-3D's paper, J. Chem. Phys. 2024,
160, 172502, Figs 3-4 and Tables I-III; Theor. Chem. Acc. 2026, 145, 74,
Fig 1), run again with the current APOST-3D: the effective fragment orbital
occupations, the FOLI values, the reliability indices and the Fe-Cp and Fe-C
interaction energies are those published. The FeCN5NO3 folder doesn't include its EFO file, so its
EFOs are listed but can't be drawn. The examples are refreshed whenever
APOST-3D's results format changes (with `scripts/update-examples.sh` in the
source repository), so they always open in the version of APOST3Dview they
come with.
