# Changelog

What changed in each version of APOST-3Dview, for its users. The newest version is at the top. Changes not yet in a release are under "Unreleased".

## 1.0.1 (2026-11-26)

The version described in the paper.

### New
- **Reaction paths**: a multi-frame `.xyz` (IRC, NEB, an optimization) opens as one structure whose frames play in the window. Frames are lined up onto each other (or on chosen atoms, or kept as in the file), bonds grow and fade as they form and break, geometries in between are smooth, and the energy profile from the comment lines can be clicked to jump along the path. Views can be kept at chosen frames, so the molecule turns as the path plays. The GIF export records what plays. Example in `examples/reaction-path/`, guide chapter "Reaction paths and other frames".
- **Several surfaces in one panel**: a panel of a figure can hold any set of surfaces of one structure, drawn together in their own colors (a metal d EFO with a ligand EFO, as in bonding papers), with their captions joined by " + ".

### Changed
- The EOS legend reads "R(%) = 79.4", as APOST-3D's report and the paper write it.
- "Energy decomposition" is now "Energy partitioning" (IQA is not an energy decomposition analysis).
- **Linux**: the download now runs on any 64-bit Linux with glibc 2.17 or later (RHEL, CentOS, Rocky and AlmaLinux 7 and later, Ubuntu 14.04 and later). Version 1.0.0 needed glibc 2.35 (Ubuntu 22.04 or newer) and stopped with "GLIBC_2.xx not found" on older systems.

### Fixed
- Without a graphics driver it can use (Vulkan on Linux), the program now stops with a message saying what is missing and how to install it, instead of crashing. It no longer falls back to OpenGL, which can't draw its pictures yet.
- Ambient occlusion is recomputed when the geometry changes (it kept the old shading until the view moved).

## 1.0.0 (2026-11-23)

### New
- **Panel figures**: several orbitals (or structures) in one PNG, each panel with a caption written from the APOST-3D tables, optional letters, one legend, sized for a half or a full page at a chosen DPI, with a live preview.
- **Aligning related molecules** on matched atoms, so molecules from separate calculations are shown in the same orientation and scale (RMSD shown). Only the view moves, never the data.
- **Frontier EFOs**: a Frontier button picks the last occupied and first unoccupied EFOs, the ones that set the reliability index R.
- **Exact EFOs**: drawn from APOST-3D's unweighted orbital file, times their fragment's weight, as APOST-3D's cube files show them. The density APOST-3D analysed is the one drawn.
- **Molecule styles**: ball and stick, sticks, thin sticks or space-filling.
- **Orbital colors**: a choice of three pairs, checked against the atom colors and for color-vision deficiencies.
- **Orthographic projection**, now the default (figures without perspective, as in papers). Perspective is still available.
- **Open > Recent**: the last ten files and sessions.
- **`apost3dview --render`**: a session's figure rendered without a window.
- **User guide**, online and in every download, with eight complete example calculations, three of them the paper's.
- **ORCA files** converted by `orca2fchk.py` are recognized, and the input generator names their functionals by libxc id.

### Changed
- Energies are blue when attractive and red when repulsive; Reverse colors flips any signed quantity.
- Energies shown with four decimals in au and one in kcal/mol.
- Oxidation states and other whole numbers without decimals (+2, not +2.00). Oxidation-state legends give the reliability index R.
- Labels in exported images keep the screen's proportions. A legend size for figures printed small.
- Orbital lobes in deep azure and crimson, apart from the N and O atom colors.
- Nothing is ticked when a file opens.
- Reads APOST-3D 5.0's results files (format 1.0).

### Fixed
- Scrolling over a window no longer zooms the molecule.
- Sessions record each file's size and checksum, report a file that changed, and open without their absolute paths when shared with their files.
- A surface cut flat by the edge of its grid says so.
- `.fchk` numbers written without the exponent's E (`1.27052287-118`) are read.

## 0.9.0 (2026-10-21)

First public release: APOST-3D results on the molecule (fragments, charges, bond orders, local spin, effective oxidation states, EFOs and OSLOs, IQA energies), orbitals and densities from `.fchk` and `.molden` files (Gaussian, Q-Chem, PySCF, ORCA), `.cube` files, the APOST-3D input generator with automatic fragments, sessions, PNG and GIF export. Downloads for Windows, macOS and Linux.
