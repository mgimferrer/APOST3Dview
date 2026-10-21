<p align="center"><img width=22% src="media/logo.png"></p>

<h3 align="center">A molecular and orbital/density visualizer for APOST-3D</h3>

<p align="center">APOST3Dview is being built as the visual companion to <a href="https://github.com/mgimferrer/APOST3D">APOST-3D</a>: reading the results file APOST-3D writes next to every run and presenting the chemical bonding indicators it computes (fragment analysis, effective oxidation states, IQA energy decomposition, and more) on the molecule, in a clear, user-friendly way. Fragments, populations, bond orders, local spin, effective oxidation states, effective fragment orbitals and OSLOs, and the IQA energy decomposition are shown on the molecule. Today, it already renders real-time, GPU-rendered geometry, <code>.cube</code> isosurfaces, and orbitals generated directly from a <code>.fchk</code> or <code>.molden</code> file, no external <code>cubegen</code>/Multiwfn step required.</p>

<!-- Screenshots section: coming soon, once a representative set of images is chosen. -->

## Shortcuts

* [What it does](#what-it-does)
* [Installation](#installation)
* [How to use](#how-to-use)
* [Known limitations](#known-limitations)
* [Connection to APOST-3D](#connection-to-apost-3d)
* [Citations](#citations)
* [License](#license)
* [Bug reports and feature requests](#bug-reports-and-feature-requests)

## What it does

[APOST-3D](https://github.com/mgimferrer/APOST3D) computes chemical bonding concepts (fragment analysis, effective oxidation states, IQA energy decomposition, and more) from a wave function. Besides its text report (`<job>.apost`), it writes a machine-readable results file (`<job>.apost.json`) on every run. APOST3Dview's purpose is to read that file and display the indicators on top of the molecule, in a clear, user-friendly way, so results don't have to be read out of a text file by hand.

What's already here, usable today:

- **APOST-3D results on the molecule**: open the `<job>.apost.json` APOST-3D writes next to every run (its `.fchk` opens with it). A `.fchk` opened on its own shows the calculation alone; if its results are next to it, the APOST-3D window offers to open them with one click. Atoms always keep their element colors; fragments are shown as soft translucent envelopes, and any value APOST-3D computed (charges, spin populations, bond orders, local spin, oxidation states, and more as APOST-3D adds them) can be shown on the molecule: per fragment as the color of its envelope, per atom as a translucent halo, with the values printed beside them and a legend that is also drawn into exported images. Every value says whether it lies within one atom or fragment (intra, such as the local spin or the self-energy) or between two (inter, such as a bond order, a spin coupling or an interaction energy). Intra values are shown on every atom or fragment at once; inter values from one fragment or atom at a time: click it, and the others take the color of their value with it. The APOST-3D window also lists every value, with a heat map and the strongest pairs for matrices, ready to copy into a spreadsheet; how the run went (a stopped run shows its message first), the input and the text report are there too. Results computed from another calculation than the opened `.fchk` (other geometry, basis set or energy) are refused, with the reason.
- **Energy decomposition (IQA)**: the interaction energies of APOST-3D's ENPART analysis, for Hartree-Fock, KS-DFT and correlated (CASSCF, CISD) wavefunctions, between atoms or between fragments. Choose the term (total, classical, exchange-correlation, its exchange and correlation parts when available, Coulomb, and more) and click an atom or fragment to see its interactions with the others, on a red/blue scale (optionally logarithmic, for values spanning orders of magnitude), in kcal/mol or au. Pairs whose exchange-correlation comes from the multipolar expansion are marked with "≈". Selecting two atoms lists their interaction term by term, and the APOST-3D window shows how well the decomposition adds up (integration errors, zero-error correction).
- **APOST-3D inputs**: for a `.fchk`, the APOST-3D window prepares the input (`<job>.inp`) from a few choices: the atomic definition, the analyses (EOS or GEOS, OSLO, local spin, energy decomposition), and the fragments, made from atoms you select, around a metal you click (each ligand becomes a fragment), or one per molecule, drawn on the molecule while you make them. What the `.fchk` already says is filled in: the functional for the energy decomposition, the files of a correlated wavefunction, Q-Chem's keyword. Experienced users can keep writing inputs by hand; the full list of keywords is in APOST-3D's input reference.
- **Effective fragment orbitals and OSLOs**: the orbitals of APOST-3D's EOS and GEOS (effective fragment orbitals) and OSLO analyses, which APOST-3D writes to their own `.fchk` next to the main one, are drawn like any other orbital, each named after its fragment (or fragments, for an OSLO shared between two). Optionally, an orbital can be drawn in its fragment's color instead of the usual positive and negative colors, blending from one fragment's color to the other's across a shared orbital. Occupations, FOLI values, electrons assigned and effective oxidation states are listed in the APOST-3D window, where a click on a row draws that orbital.
- **Geometry viewing**: `.fchk` and `.xyz`, raymarched sphere/cylinder impostors (not polygon meshes) with CPK coloring, true 3D depth-tested atom labels, and one-click PNG export sized for a journal figure (DPI and physical size embedded in the file).
- **`.cube` isosurfaces**: standard Gaussian cube files, rendered as a smoothed (Surface Nets + Taubin) mesh, with tunable isovalue, refinement, and color per lobe. Every cube of a molecule, and every orbital generated for it, is one surface in that molecule's list: show one alone, or several together in one picture.
- **Orbitals generated directly from a wave function**, no `.cube` file needed. A from-scratch Rust GTO evaluator (s/p/d/f/g shells, restricted and unrestricted, ECPs handled transparently) reads a `.fchk` (Gaussian, Q-Chem, PySCF) or a `.molden` file (PySCF, ORCA) and evaluates any molecular orbital on a grid on the spot. Validated against the cube files Gaussian, Q-Chem, PySCF and ORCA write themselves, down to numerical noise (RMS around 10⁻⁶ to 10⁻⁷ of peak magnitude), across every shell type, both restricted and open-shell cases, and ECPs.
- **Physically-based rendering**: Cook-Torrance/GGX lighting, screen-space ambient occlusion, depth of field, and linear-light tone mapping, with Default and Publication (figure-ready, orientation-independent lighting) style presets, plus your own saved Custom looks.
- **Measurements**: distance, angle, and dihedral from a normal atom selection, with draggable, fully customizable labels.
- **Export**: publication PNGs at a chosen DPI and figure size, and looping animated GIFs of the rotating molecule for presentations.
- **Sessions**: save everything you set up (files, hidden atoms, bonds, measurements, surfaces and how they're drawn, the camera, the look) as a small `.a3dview` file, and reopen it later to get exactly the same picture. By default a session is also saved next to every exported image, so each figure keeps the setup that made it.

## Installation

Download the file for your computer from the [latest release](https://github.com/mgimferrer/APOST3Dview/releases/latest). Nothing else needs to be installed.

The downloads aren't signed with a paid certificate (APOST3Dview is free, and so is its distribution), so Windows and macOS ask once, the first time you open it. What to click is below.

### Windows (64-bit)

1. Download `APOST3Dview-<version>-windows-x86_64.zip`, right-click it and choose **Extract All**.
2. Double-click `APOST3Dview.exe` in the extracted folder.
3. If Windows shows "Windows protected your PC", click **More info**, then **Run anyway**. Windows remembers this for the next time.

### macOS (11 or later, Apple Silicon and Intel)

1. Download `APOST3Dview-<version>-macos-universal.dmg`, open it, and drag APOST3Dview onto Applications.
2. Open APOST3Dview from Applications. The first time, macOS says it could not verify the app. Click **Done** (not Move to Trash).
3. Open **System Settings**, then **Privacy & Security**, scroll down to the message about APOST3Dview, click **Open Anyway** and confirm. From then on it opens normally.

### Linux (x86_64)

1. Download `APOST3Dview-<version>-linux-x86_64.tar.gz` and unpack it: `tar xzf APOST3Dview-*-linux-x86_64.tar.gz`.
2. Run `./install.sh` in the unpacked folder to add APOST3Dview to your applications menu (for your user only, no root needed), or start `./apost3dview` directly.
3. A Vulkan driver for your GPU is needed (Mesa's `mesa-vulkan-drivers`, or your GPU vendor's driver). Most desktops have one already.

### Building from source

For development, or if a download doesn't work on your system: see [BUILDING.md](BUILDING.md).

## How to use

1. Launch the app (`apost3dview`, optionally followed by files to open) and open files with **Open** in the toolbar, Cmd+O (Ctrl+O on Windows and Linux), or by dragging them onto the window. `.fchk`, `.molden`, `.xyz`, `.cube` and APOST-3D `.apost.json` files are recognized by their name; the Open menu can also show one kind of file at a time. The windows (Molecule, Surfaces, Measure, APOST-3D, Appearance, Export, opened from the toolbar) stack in a column at the top right, all the same size, so they never cover the middle of the molecule: collapse one with its triangle and the ones below move up, resize one by its border, or drag one anywhere. **Arrange windows** in the toolbar puts them all back in the column at their default size.
2. Drag with the mouse or trackpad to rotate (freely, in any direction, including over the top), shift+drag to pan, and scroll or pinch to zoom. The arrow keys rotate too, two at once for a diagonal, and the Space bar starts or stops a continuous spin in the direction of the last arrow pressed, handy for presentations. Click atoms or bonds to select them, or Alt+drag (Option+drag on a Mac) a box to select every atom inside it; Esc clears the selection. The bar along the bottom shows what is open and selected.
3. **Molecule** lists the open structures and sets how the active one is drawn: atom and bond size, labels, hidden hydrogens. While something is selected, it offers what to do with it: hide it, create or hide a bond between two atoms, make bonds dashed (transition states), or add a measurement.
4. **Surfaces** generates orbitals and densities from a `.fchk` or `.molden` file: tick the orbitals you want (HOMO/LUMO tagged, with energy and occupation), and the total density or, for an open shell, the spin density, and click Generate. When the `.fchk` holds a correlated density (Gaussian's MP2, CI or CC density, or the CASSCF, FCI, CISD or CCSD density matrix APOST-3D's `apost3d.py` script writes for PySCF), the densities come from it by default (a Density menu switches back to SCF when the file has both), and its natural orbitals are a set of their own in the Orbitals menu, listed by occupation. Generated orbitals and opened `.cube` files of the same molecule are listed together: click a name to show that surface alone, or tick several to show them together. Changes to the isovalue, refinement or colors show up immediately and stay with that surface (its isovalue is shown next to its name); new surfaces start at the default settings unless you click "Use for new surfaces". When APOST-3D wrote the orbitals of an EOS, GEOS or OSLO analysis, the Orbitals menu at the top switches from the SCF orbitals to those (for OSLO, also the OSLOs before orthogonalization when the run wrote them, the orbitals the FOLI values were computed for): each is listed with its fragment, occupation or FOLI, and drawn in the usual positive and negative colors. Tick "Color by fragment" to draw it in its fragment's color instead. EFO rows also show how closely the orbital in the `.fchk` reproduces the EFO (its fit); below 60% they are marked approximate, since the fitted orbital can then show lobes the exact one doesn't have: open the EFO's cube file for its exact shape.
5. **Measure** lists distances, angles and dihedrals (select 2, 3 or 4 atoms, then Add). Their labels sit beside the bond, or inside the angle, always readable over the molecule; drag a label to move it.
6. **APOST-3D** (its Results tab) shows the results of an APOST-3D run. At first nothing is drawn on the molecule; choose what to show in the **Show** menu: the fragments, or any value. The menu is grouped by where a value lives. *Fragment values* and *Atom values* are values on each fragment or atom, including the intra parts of the matrices (for example "Intra-atomic S² (local spin)", "Intra-fragment localization index", "Intra-atomic (self) energy"). *Between fragments* and *Between atoms* are the inter parts (for example "Inter-fragment bond order", "Inter-atomic S² (spin coupling)", "Inter-atomic interaction energy"): click a fragment or atom in the view to see the others from it. When two matrices name a part the same way, the menu adds which matrix it comes from, as in "(exchange-only bond orders, TFVC)" and "(improved bond orders, TFVC)" for a correlated wavefunction. Values that follow from others (electron populations, valences, electron counts) and the Mulliken values printed for comparison are listed when "Also list other values", at the end of the menu, is ticked. Each value is printed with its atom's or fragment's name above it; untick "with names" for the values alone (atom names follow the Labels setting of the Molecule window). Labels never overlap: on a crowded view, a label of an atom hidden behind others is left out, and one that would cover another moves a line up or down, or is left out; every value is still in the window's tables, and turning or zooming the molecule brings labels back. Colors are automatic, or fixed to a range you choose (kept for that quantity in every open structure, so figures compare). "Bond thickness from bond orders" draws bonds by their bond order; it is off by default, since equal bonds usually look better in figures. Key numbers (such as the reliability index R of EOS) and the tables of the analysis (effective fragment orbitals, OSLOs, frontier occupations) are listed below; Draw on an orbital's row draws that orbital. For an energy decomposition (IQA), a Term menu chooses which energy term is shown, and the "Energy decomposition" section shows the integration errors.
7. **APOST-3D input** (the APOST-3D window's New input tab, opened by "APOST-3D input..." in the Molecule window for a `.fchk` without results) prepares an APOST-3D input for that `.fchk`. Choose the atomic definition (TFVC, recommended, works with every analysis) and the analyses; EOS, GEOS and OSLO need fragments, which are drawn on the molecule in their colors (atoms in no fragment in gray) as you make them: select atoms and click "New fragment from the selected", or click a metal and "Around the selected atom(s)" (the metal becomes fragment 1 and each ligand left around it a fragment), or "Separate molecules" for a dimer or cluster. The automatic fragments follow the bonds drawn, so fix a wrong bond (Create bond, Hide bond in the Molecule window) and click again. For the energy decomposition, the functional is taken from the `.fchk` when it names one (Gaussian files); otherwise choose it, and choose between APOST-3D's standard two-electron grid and a faster one for tests. Problems that would stop APOST-3D are listed at the top, and Save waits until there are none. **Save `<job>.inp`** writes the input next to the `.fchk`, with the name APOST-3D looks for (Copy input puts it on the clipboard instead; the Input file section shows it as it is built). If `<job>.inp` already exists, the form starts from it, naming any settings it can't edit (saving from the form leaves those out), and **Replace** overwrites it, while **Save as another job...** keeps it and saves the new input under another name, for another analysis of the same calculation. APOST-3D needs an input and a `.fchk` of the same name, so the `.fchk` is added under the new name too, as a link to the original that takes no extra disk space (a full copy where links aren't possible, such as a USB stick formatted FAT, and once the folder is copied to another computer or a cluster, so expect each job to carry its own `.fchk` there). Then run APOST-3D as usual (`apost3d <job> > <job>.apost 2>&1` in that folder) and open the `<job>.apost.json` it writes. The generator covers the common analyses with their defaults; for anything else (cube files, grids, thresholds, other analyses), edit the input by hand following APOST-3D's input reference. Running APOST-3D from the viewer is not planned for now: most runs go to a cluster. It may come later.
8. **Appearance** sets the look of the scene: Default or Publication, the background, ambient occlusion and depth of field; the fine-tuning sliders are under Advanced. Once you like a look, save it under a name: it appears as its own button next to Default and Publication, and is kept as a small plain-text file in your settings folder. **Open presets folder** shows that folder, to back a look up or hand it to someone else.
9. **Export** saves the current view as a PNG at a given DPI and physical figure size (or a custom resolution), or a looping animated GIF of one full turn of the molecule, with your choice of width, duration and frame rate. The GIF works directly in PowerPoint, Keynote and Google Slides. With "Also save the session" ticked (the default), `figure.a3dview` is written next to `figure.png`.
10. **Save** (Cmd+S, or Ctrl+S) saves the whole setup as a session (`.a3dview`). Open it again like any other file. A session points to your files rather than copying them, with paths relative to the session, so keep it in the same folder as the files (moving the whole folder is fine). Orbitals generated in the app are recalculated when the session opens. If a file is missing, or has changed since the session was saved, APOST3Dview opens the rest and tells you what it couldn't restore.

`TESTS-VISUALIZER/` in this repository has real committed `.fchk`/`.cube`/`.molden` test files (H2O and BiCl3 at several basis sets, an open-shell triplet, and a real APOST-3D bismuth complex), good for opening something right away.

## Known limitations

- `.molden` files are read as PySCF and ORCA write them (ORCA's own conventions, primitive normalization in the coefficients and the sign of some f and g components, are recognized and corrected; checked with ORCA 6.1.1). Other programs' `.molden` output isn't tested yet. With an ECP, densities are valence densities, as in the files themselves.
- Basis functions above g (h, i, and so on) aren't supported. This fails with an explicit error rather than a silently wrong orbital.
- Cartesian g shells aren't implemented (pure/spherical g is). In practice, every basis set that reaches g is spherical by convention, so this hasn't been a real limitation so far.
- APOST-3D's results file currently carries populations, bond orders, local spin, the EOS, GEOS and OSLO analyses with their orbitals, and the energy decomposition (IQA). The EFFAO orbitals have no orbital file yet, so their table is listed but not drawn.
- The Windows and Linux downloads are new and haven't been tried on many computers yet. If one doesn't work for you, please [report it](https://github.com/mgimferrer/APOST3Dview/issues).

## Connection to APOST-3D

APOST3Dview is a sister project to [APOST-3D](https://github.com/mgimferrer/APOST3D) (see its own README for the full scope of what it computes), but a fully independent codebase and language stack: Rust/wgpu, no Fortran, no shared code. It's built specifically around the files APOST-3D works with: it prepares the input (`.inp`) for a `.fchk`, and reads the `.fchk` APOST-3D reads and the `.apost.json` results file it writes, rather than being a general-purpose molecular viewer that happens to also open them. The format of the results file is documented in the APOST-3D repository (`docs/source/developer/json-output.md`).

If you use APOST-3D itself, or the chemical bonding methods it implements, see APOST-3D's own README for the specific paper to cite for each method.

## Citations

APOST3Dview doesn't have its own dedicated publication yet. Until it does, if you use it in a paper, please cite the APOST-3D paper below:

* P. Salvador, E. Ramos-Cordoba, M. Montilla, L. Pujal and M. Gimferrer, *APOST-3D: Chemical concepts from wave function analysis*, **J. Chem. Phys.**, **2024**, 160, 172502
  DOI: [10.1063/5.0206187](https://doi.org/10.1063/5.0206187)

A `CITATION.cff` file is included in this repository, so GitHub's "Cite this repository" button stays up to date automatically.

## License

APOST3Dview is licensed under the [GNU General Public License v3.0](LICENSE).

Copyright © 2026 Martí Gimferrer.

## Bug reports and feature requests

Please open an [issue](https://github.com/mgimferrer/APOST3Dview/issues), or email [mgimferrer18@gmail.com](mailto:mgimferrer18@gmail.com).

## Author

**Martí Gimferrer**, University of Göttingen
[mgimferrer18@gmail.com](mailto:mgimferrer18@gmail.com)
