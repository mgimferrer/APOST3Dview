//! Opening files.

use apost3dview_core::{parse_cube, parse_fchk_wavefunction, parse_molden_wavefunction, parse_xyz, Molecule, Wavefunction};

use crate::app::App;
use crate::isosurfaces::IsosurfaceState;
use crate::state::LoadedStructure;

impl App {
    pub(crate) fn open_fchk(&mut self) {
        self.open_wavefunction_files("Gaussian checkpoint", &["fchk"], Molecule::from_fchk, parse_fchk_wavefunction);
    }

    /// Same as `open_fchk`, sourced from `.molden` files instead.
    /// `generate_mo_grid`/`evaluate_mo` already take `(&BasisSet,
    /// &MolecularOrbitals)` rather than a `.fchk`-specific `Wavefunction`,
    /// so nothing downstream of `parse_molden_wavefunction` needed to
    /// change for a second source to plug in here.
    pub(crate) fn open_molden(&mut self) {
        self.open_wavefunction_files("Molden format", &["molden", "inp"], Molecule::from_molden, parse_molden_wavefunction);
    }

    /// Opens one or more files that carry both a geometry and a
    /// wavefunction (`.fchk`, `.molden`) — same multi-select UX as `.xyz`/
    /// `.cube`, each file becoming its own Structures entry. Parsing the
    /// wavefunction (basis set + MO coefficients) is a separate, slower
    /// full-file pass from the geometry-only one; failure there just means
    /// no "Generate orbitals" section for that structure, not a failure to
    /// open it at all.
    pub(crate) fn open_wavefunction_files(
        &mut self,
        filter_name: &str,
        extensions: &[&str],
        load_molecule: fn(&std::path::Path) -> std::io::Result<Molecule>,
        load_wavefunction: fn(&std::path::Path) -> std::io::Result<Wavefunction>,
    ) {
        let Some(paths) = rfd::FileDialog::new().add_filter(filter_name, extensions).pick_files() else { return };
        let mut first_new_index = None;
        for path in paths {
            let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled".into());
            let molecule = match load_molecule(&path) {
                Ok(molecule) => molecule,
                Err(err) => {
                    self.show_warning(format!("Could not load {label}: {err}"));
                    continue;
                }
            };
            let mut structure = LoadedStructure::new(label.clone(), molecule, Some(path.clone()));
            match load_wavefunction(&path) {
                Ok(wfn) => {
                    // `evaluate_basis_functions` evaluates every shell in
                    // the basis regardless of which MO is being asked for,
                    // so a single h-or-higher shell anywhere in the file
                    // blocks *every* orbital, not just ones with real
                    // weight on it. Worth flagging right away rather than
                    // only on the first failed "Generate" click.
                    let max_angular_momentum = wfn.basis.shells.iter().map(|s| s.angular_momentum).max().unwrap_or(0);
                    if wfn.alpha.num_orbitals() > 0 {
                        structure.selected_alpha_mos.insert(wfn.alpha.homo_index() - 1);
                    }
                    if let Some(beta) = &wfn.beta {
                        if beta.num_orbitals() > 0 {
                            structure.selected_beta_mos.insert(beta.homo_index() - 1);
                        }
                    }
                    structure.wavefunction = Some(wfn);
                    if max_angular_momentum > 4 {
                        self.show_warning(format!("{label}: the basis set includes h (or higher) shells, orbital generation isn't supported yet for this file."));
                    }
                }
                Err(err) => self.show_warning(format!("{label}: geometry loaded, but orbitals unavailable: {err}")),
            }
            let index = self.structures.len();
            self.structures.push(structure);
            first_new_index.get_or_insert(index);
        }
        if let Some(index) = first_new_index {
            self.set_active(index);
        }
    }

    pub(crate) fn open_xyz(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("XYZ", &["xyz"]).pick_files() else { return };
        let mut first_new_index = None;
        for path in paths {
            match parse_xyz(&path) {
                Ok(molecule) => {
                    let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled.xyz".into());
                    let index = self.structures.len();
                    self.structures.push(LoadedStructure::new(label, molecule, Some(path)));
                    first_new_index.get_or_insert(index);
                }
                Err(err) => self.show_warning(format!("Could not load {}: {err}", path.display())),
            }
        }
        if let Some(index) = first_new_index {
            self.set_active(index);
        }
    }

    /// Opens one or more `.cube` files — same multi-select UX as `.xyz`.
    /// Each becomes its own Structures entry with both a molecule (parsed
    /// from the cube's own atom section) and an isosurface (see
    /// `IsosurfaceState`); `set_active` handles the shared-geometry
    /// camera-freeze and the one-time initial extraction.
    pub(crate) fn open_cube(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("Gaussian cube", &["cube"]).pick_files() else { return };
        let mut first_new_index = None;
        for path in paths {
            match parse_cube(&path) {
                Ok(cube_file) => {
                    let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled.cube".into());
                    let mut structure = LoadedStructure::new(label, cube_file.molecule, Some(path));
                    structure.isosurface = Some(IsosurfaceState::new(cube_file.grid));
                    let index = self.structures.len();
                    self.structures.push(structure);
                    first_new_index.get_or_insert(index);
                }
                Err(err) => self.show_warning(format!("Could not load {}: {err}", path.display())),
            }
        }
        if let Some(index) = first_new_index {
            self.set_active(index);
        }
    }
}
