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

- **APOST-3D results on the molecule**: opening a `.fchk` also loads the `<job>.apost.json` next to it, if there is one (or open the `.apost.json` itself). Atoms always keep their element colors; fragments are shown as soft translucent envelopes, and any value APOST-3D computed (populations, charges, spin, bond orders, valences, local spin, and more as APOST-3D adds them) can be shown on the molecule: per fragment as the color of its envelope, per atom as a translucent halo, with the values printed beside them and a legend that is also drawn into exported images. Quantities between pairs (bond orders between fragments, between atoms) are shown from one fragment or atom at a time: click it, and the others take the color of their value with it. The APOST-3D window also lists every value, with a heat map and the strongest pairs for matrices, ready to copy into a spreadsheet; how the run went (a stopped run shows its message first), the input and the text report are there too. Results computed from another calculation than the opened `.fchk` (other geometry, basis set or energy) are refused, with the reason.
- **Energy decomposition (IQA)**: the interaction energies of APOST-3D's ENPART analysis, for Hartree-Fock, KS-DFT and correlated (CASSCF, CISD) wavefunctions, between atoms or between fragments. Choose the term (total, classical, exchange-correlation, its exchange and correlation parts when available, Coulomb, and more) and click an atom or fragment to see its interactions with the others, on a red/blue scale (optionally logarithmic, for values spanning orders of magnitude), in kcal/mol or au. Pairs whose exchange-correlation comes from the multipolar expansion are marked with "≈". Selecting two atoms lists their interaction term by term, and the APOST-3D window shows how well the decomposition adds up (integration errors, zero-error correction).
- **Effective fragment orbitals and OSLOs**: the orbitals of APOST-3D's EOS and GEOS (effective fragment orbitals) and OSLO analyses, which APOST-3D writes to their own `.fchk` next to the main one, are drawn like any other orbital, each named after its fragment (or fragments, for an OSLO shared between two). Optionally, an orbital can be drawn in its fragment's color instead of the usual positive and negative colors, blending from one fragment's color to the other's across a shared orbital. Occupations, FOLI values, electrons assigned and effective oxidation states are listed in the APOST-3D window, where a click on a row draws that orbital.
- **Geometry viewing**: `.fchk` and `.xyz`, raymarched sphere/cylinder impostors (not polygon meshes) with CPK coloring, true 3D depth-tested atom labels, and one-click PNG export sized for a journal figure (DPI and physical size embedded in the file).
- **`.cube` isosurfaces**: standard Gaussian cube files, rendered as a smoothed (Surface Nets + Taubin) mesh, with tunable isovalue, refinement, and color per lobe. Every cube of a molecule, and every orbital generated for it, is one surface in that molecule's list: show one alone, or several together in one picture.
- **Orbitals generated directly from a wave function**, no `.cube` file needed. A from-scratch Rust GTO evaluator (s/p/d/f/g shells, restricted and unrestricted, ECPs handled transparently) reads a `.fchk` or a PySCF-written `.molden` file and evaluates any molecular orbital on a grid on the spot. Validated against independently generated reference cubes down to numerical noise (RMS around 10⁻⁶ to 10⁻⁷ of peak magnitude) across every shell type, and both restricted and open-shell cases.
- **Physically-based rendering**: Cook-Torrance/GGX lighting, screen-space ambient occlusion, depth of field, and linear-light tone mapping, with Default and Publication (figure-ready, orientation-independent lighting) style presets, plus your own saved Custom looks.
- **Measurements**: distance, angle, and dihedral from a normal atom selection, with draggable, fully customizable labels.
- **Export**: publication PNGs at a chosen DPI and figure size, and looping animated GIFs of the rotating molecule for presentations.
- **Sessions**: save everything you set up (files, hidden atoms, bonds, measurements, surfaces and how they're drawn, the camera, the look) as a small `.a3dview` file, and reopen it later to get exactly the same picture. By default a session is also saved next to every exported image, so each figure keeps the setup that made it.

## Installation

APOST3Dview is developed and tested on macOS. Windows and Linux builds should work through the same Rust/wgpu toolchain, but haven't been verified end-to-end yet. Feedback from anyone who tries either is very welcome.

### macOS

1. Install [Rust](https://rustup.rs/) via `rustup`.
2. Install the Xcode Command Line Tools: `xcode-select --install`.
3. Build (see [Building from source](#building-from-source) below).

### Windows

The easiest path, for almost everyone: download and run [`rustup-init.exe`](https://rustup.rs/) and pick the default option (1). If Visual Studio's C++ Build Tools aren't already on your machine, rustup detects that and downloads and installs them for you automatically as part of the same install (about 1.5 GB, 5 to 15 minutes, no separate trip to the Visual Studio site needed). Then build (see [Building from source](#building-from-source) below).

If you'd rather not have Visual Studio installed at all, there's a lighter alternative: [WinLibs](https://winlibs.com/) is a standalone GCC and MinGW-w64 build for Windows that needs no MSYS2 or Visual Studio.

1. Download a WinLibs release, extract it somewhere like `C:\mingw64`, and add its `bin` folder to your `PATH`.
2. Run `rustup-init.exe`. At the "Rust Visual C++ prerequisites" step, choose option 3 ("Don't install the Visual C++ prerequisites"), then option 2 ("Customize installation") and set the default host triple to `x86_64-pc-windows-gnu`.
3. Build as usual.

This GNU-toolchain path is less commonly used than the MSVC one for Rust GUI apps, so treat it as the "don't want Visual Studio on my machine" option rather than the most battle-tested one. Either way, wgpu targets DirectX12/Vulkan on Windows.

**What about WSL?** Windows Subsystem for Linux can run GPU-accelerated Linux GUI apps through WSLg, so building and running APOST3Dview inside a WSL Ubuntu install (see [Linux](#linux) below, once inside it) is technically possible. In practice it isn't simpler for most people: it's a whole Linux environment to set up, and Vulkan passthrough through WSLg is newer and less proven than either native option above. Worth trying only if you're already comfortable living in a Linux shell and would rather avoid Visual Studio and MinGW alike.

A prebuilt Windows executable (no Rust toolchain needed at all) isn't available yet, but is the likely eventual path for non-developer users; this section will be updated once one exists.

### Linux

1. Install [Rust](https://rustup.rs/) via `rustup`.
2. Install the system packages `eframe` (this app's UI framework) needs to build its window and native file dialogs, along with a C compiler. On Ubuntu/Debian:
   ```bash
   sudo apt-get install -y build-essential libclang-dev libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
   ```
   On Fedora:
   ```bash
   sudo dnf install gcc clang clang-devel clang-tools-extra libxkbcommon-devel pkg-config openssl-devel libxcb-devel gtk3-devel atk fontconfig-devel
   ```
   (Straight from [egui's own README](https://github.com/emilk/egui), the UI library APOST3Dview is built on.) If you're on a Wayland-only compositor without XWayland, you may additionally need `libwayland-dev` (Ubuntu/Debian) or the equivalent for native Wayland windowing support.
3. Make sure a Vulkan driver is actually installed for your GPU, separate from the build itself: `mesa-vulkan-drivers` for open-source Mesa drivers, or your GPU vendor's proprietary driver package if you use one. Without this, the app can build fine but fail to find a GPU at runtime.
4. Build (see [Building from source](#building-from-source) below).

### Building from source

```bash
git clone https://github.com/mgimferrer/APOST3Dview.git
cd APOST3Dview
cargo build --release -p apost3dview
```

The binary is then at `target/release/apost3dview` (`.exe` on Windows). For active development, `cargo run -p apost3dview` builds and launches in one step.

**Tests**: `cargo test --workspace`. The tests that check APOST-3D's reference results need them copied from a local APOST-3D checkout first, with `scripts/sync-apost3d-fixtures.sh path/to/APOST3D`; without them, those tests are skipped.

**Troubleshooting**: if `cargo`/`rustc` aren't found right after installing Rust, open a new terminal window. `rustup` adds `~/.cargo/bin` to `PATH` in your shell's startup file, which an already-open terminal won't have picked up yet.

## How to use

1. Launch the app (`apost3dview`, optionally followed by files to open) and open files with **Open** in the toolbar, Cmd+O (Ctrl+O on Windows and Linux), or by dragging them onto the window. `.fchk`, `.molden`, `.xyz`, `.cube` and APOST-3D `.apost.json` files are recognized by their name; the Open menu can also show one kind of file at a time.
2. Drag with the mouse or trackpad to rotate (freely, in any direction, including over the top), shift+drag to pan, and scroll or pinch to zoom. The arrow keys rotate too, two at once for a diagonal, and the Space bar starts or stops a continuous spin in the direction of the last arrow pressed, handy for presentations. Click atoms or bonds to select them; Esc clears the selection. The bar along the bottom shows what is open and selected.
3. **Molecule** lists the open structures and sets how the active one is drawn: atom and bond size, labels, hidden hydrogens. While something is selected, it offers what to do with it: hide it, create or hide a bond between two atoms, make bonds dashed (transition states), or add a measurement.
4. **Surfaces** generates orbitals from a `.fchk` or `.molden` file: tick the orbitals you want (HOMO/LUMO tagged, with energy and occupation) and click Generate. Generated orbitals and opened `.cube` files of the same molecule are listed together: click a name to show that surface alone, or tick several to show them together. Changes to the isovalue, refinement or colors show up immediately. When APOST-3D wrote the orbitals of an EOS, GEOS or OSLO analysis, the Orbitals menu at the top switches from the SCF orbitals to those (for OSLO, also the OSLOs before orthogonalization when the run wrote them, the orbitals the FOLI values were computed for): each is listed with its fragment, occupation or FOLI, and drawn in the usual positive and negative colors. Tick "Color by fragment" to draw it in its fragment's color instead.
5. **Measure** lists distances, angles and dihedrals (select 2, 3 or 4 atoms, then Add).
6. **APOST-3D** shows the results of an APOST-3D run. Choose what to show on the molecule (the fragments, or any value in the results); for values between pairs, click a fragment or atom in the view to see the others from it. Colors are automatic, or fixed to a range you choose (kept for that quantity in every open structure, so figures compare). "Bond thickness from bond orders" draws bonds by their bond order; it is off by default, since equal bonds usually look better in figures. Key numbers (such as the reliability index R of EOS) and the tables of the analysis (effective fragment orbitals, OSLOs, frontier occupations) are listed below; Draw on an orbital's row draws that orbital. For an energy decomposition (IQA), a Term menu chooses which energy term is shown, and the "Energy decomposition" section shows the integration errors.
7. **Appearance** sets the look of the scene: Default or Publication, the background, ambient occlusion and depth of field; the fine-tuning sliders are under Advanced. Once you like a look, save it under a name: it's written as a plain TOML file under `presets/` at the root of this repository (not tracked by git), easy to find, back up, or hand to someone else, and appears as its own button next to Default and Publication.
8. **Export** saves the current view as a PNG at a given DPI and physical figure size (or a custom resolution), or a looping animated GIF of one full turn of the molecule, with your choice of width, duration and frame rate. The GIF works directly in PowerPoint, Keynote and Google Slides. With "Also save the session" ticked (the default), `figure.a3dview` is written next to `figure.png`.
9. **Save** (Cmd+S, or Ctrl+S) saves the whole setup as a session (`.a3dview`). Open it again like any other file. A session points to your files rather than copying them, with paths relative to the session, so keep it in the same folder as the files (moving the whole folder is fine). Orbitals generated in the app are recalculated when the session opens. If a file is missing, or has changed since the session was saved, APOST3Dview opens the rest and tells you what it couldn't restore.

`TESTS-VISUALIZER/` in this repository has real committed `.fchk`/`.cube`/`.molden` test files (H2O and BiCl3 at several basis sets, an open-shell triplet, and a real APOST-3D bismuth complex), good for opening something right away.

## Known limitations

- `.molden` support currently targets PySCF's own writer specifically. Other programs' `.molden` output (ORCA in particular is known to deviate from the format for f/g functions) isn't tested yet.
- Basis functions above g (h, i, and so on) aren't supported. This fails with an explicit error rather than a silently wrong orbital.
- Cartesian g shells aren't implemented (pure/spherical g is). In practice, every basis set that reaches g is spherical by convention, so this hasn't been a real limitation so far.
- APOST-3D's results file currently carries populations, bond orders, local spin, the EOS, GEOS and OSLO analyses with their orbitals, and the energy decomposition (IQA). The EFFAO orbitals have no orbital file yet, so their table is listed but not drawn.
- Orbitals generated from a Q-Chem `.fchk` haven't been checked against a Q-Chem reference cube yet (Gaussian and PySCF files have, down to numerical noise).
- Windows builds aren't verified end-to-end yet (see [Installation](#installation)).

## Connection to APOST-3D

APOST3Dview is a sister project to [APOST-3D](https://github.com/mgimferrer/APOST3D) (see its own README for the full scope of what it computes), but a fully independent codebase and language stack: Rust/wgpu, no Fortran, no shared code. It's built specifically to read the files APOST-3D works with, the `.fchk` it reads and the `.apost.json` results file it writes, rather than being a general-purpose molecular viewer that happens to also open them. The format of the results file is documented in the APOST-3D repository (`docs/source/developer/json-output.md`).

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
