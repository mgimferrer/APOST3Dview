# Examples

Five complete APOST-3D calculations, to try APOST3Dview without running
APOST-3D first. They are test systems of APOST-3D itself: each folder is one
run, as APOST-3D leaves it.

| File | What it is |
|---|---|
| `<job>.inp` | the APOST-3D input |
| `<job>.fchk` | the wavefunction (Gaussian or PySCF) |
| `<job>.apost` | APOST-3D's text report |
| `<job>.apost.json` | the results file APOST3Dview reads |
| `<job>-*.fchk` | orbital files written by EOS, GEOS or OSLO, where there are any |
| `<job>.dm1`, `<job>.dm2` | the density matrices of a correlated wavefunction (only needed to run APOST-3D again) |

**To open one**, open its `.apost.json` (Open, or drag it onto the window):
the molecule opens with its results, and the APOST-3D window shows them.
Opening the `.fchk` instead shows the calculation alone; "Open these
results" in the APOST-3D window then attaches the results.

## FeCN5NO3--UBLYP: fragments, charges, bond orders

Reduced nitroprusside, [Fe(CN)5NO]³⁻ (a doublet), UBLYP/def2-SVP, Löwdin
atoms, split into seven fragments (Fe, five CN, NO), with EOS oxidation
states.

- In the APOST-3D window, **Show** → "Fragments" draws the seven fragments
  as colored envelopes. The Fragments list below names the atoms in each
  (click one to select its atoms).
- **Show**, under *Fragment values*: "Fragment charges (Lowdin)" colors each
  fragment by its charge, with the value beside it and a legend in the
  corner; "EOS oxidation states (Lowdin)" is next to it.
- **Show**, under *Between fragments*: "Inter-fragment bond order (Lowdin)".
  The molecule is seen from Fe: every other fragment is colored by its bond
  order with Fe. Click another fragment in the view to look from there. The
  whole matrix is listed below, with a heat map and the strongest pairs.
- Under *Atom values*, "Atomic charges (Lowdin)" gives one value per atom.
  "Bond thickness from bond orders" draws the bonds by their bond order.
- Key numbers lists EOS's reliability index R, and Tables the EFOs. Their
  orbitals can't be drawn here: this folder doesn't include the EFO file
  (the C2H6 and CH3F examples have orbitals).

## H2O-Dimer-B3LYP: energy decomposition (IQA)

The water dimer, B3LYP/cc-pVDZ, TFVC atoms, each water a fragment, with
APOST-3D's energy decomposition (ENPART).

- **Show**, under *Energy decomposition (IQA)*: "Inter-atomic interaction
  energy (TFVC)". The molecule is seen from O1: every other atom is colored
  by its interaction energy with it, in kcal/mol. Click another atom to look
  from there. A value marked "≈" comes from the multipolar expansion.
- **Term** chooses the energy term (total, classical (electrostatic),
  exchange-correlation, ...), and the switch under it shows au instead of
  kcal/mol. Tick "Logarithmic colors" when values span orders of magnitude.
- "Inter-fragment interaction energy (TFVC)" does the same for the two
  waters: their hydrogen bond, term by term.
- Select two atoms (click one, Cmd/Ctrl-click the other): "Selected pair"
  decomposes their interaction term by term.
- The "Energy decomposition (IQA)" section reports the integration errors,
  to judge how far the numbers can be trusted.

## C2H6-OSLO-2CH3: OSLOs, the shared two-color orbital

Ethane, B3LYP/cc-pVTZ, TFVC atoms, two CH3 fragments, with OSLO (orbitals localized
on one fragment or shared between two).

- **Tables (1)** lists the OSLOs with their fragment(s) and FOLI. **Draw** on
  a row draws that orbital.
- Or in the **Surfaces** window: **Orbitals:** → "OSLOs", tick some, and
  Generate.
- OSLO 9 is shared by the two CH3: the C–C bond. Tick "Color by fragment" in
  the Surfaces window: it fades from one fragment's color to the other's
  across the bond, while each C–H orbital takes the color of its CH3.
- "OSLOs (before orthogonalization)" in the same menu are the orbitals the
  FOLI values were computed for.

## CH3F-GEOS: effective fragment orbitals (GEOS)

Fluoromethane, BP86/def2-SVP, TFVC atoms, F and CH3 as fragments, with
GEOS oxidation states.

- **Show**, under *Fragment values*: "GEOS oxidation states (TFVC)".
- **Surfaces** → **Orbitals:** → "GEOS EFOs": the effective fragment
  orbitals of each fragment, with their occupation and how well the file
  reproduces them (fit). Tick "Color by fragment" to see which fragment
  each belongs to.
- Key numbers lists the reliability index R; Tables, the EFOs with a Draw
  button per row.

## H2O-CAS44: correlated density and natural orbitals

Water, CASSCF(4,4)/STO-3G from PySCF, TFVC atoms, O and the two H as fragments,
with local spin and the energy decomposition of the correlated
wavefunction.

- **Surfaces**: tick "Total density" and Generate. It is the CASSCF
  density, evaluated through its natural orbitals, which APOST3Dview finds
  from the density matrix in the `.fchk`.
- **Orbitals:** → "Natural orbitals (CASSCF)": each listed with its
  occupation. **Frontier** ticks the two either side of the gap (the last
  with an occupation above 1, the first below), **Fractional** those
  between 0.02 and 1.98.
- In the APOST-3D window, a correlated wavefunction has two kinds of bond
  order, "exchange-only" and "improved" (from the natural orbitals), both in
  **Show**. Under *Atom values*, "Intra-atomic S² (local spin) (TFVC)"
  shows the local spin of each atom.

## About these files

They are APOST-3D's own test cases. The C2H6 and CH3F folders are runs made
for APOST3Dview with the orbital files written; the others are APOST-3D's
reference results. They're refreshed when APOST-3D's results format
changes (`scripts/update-examples.sh`), so they always open in the version
of APOST3Dview they come with.
