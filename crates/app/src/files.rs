//! Opening files: one entry point (`App::open_paths`) shared by the Open
//! dialog, drag-and-drop and the command line, which recognizes each file
//! by its name.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use apost3dview_core::{
    find_results_for, is_results_file, load_apost_results, parse_cube, parse_fchk_wavefunction, parse_molden_wavefunction, parse_xyz,
    wavefunction_path_for, Molecule, Wavefunction,
};

use crate::app::App;
use crate::isosurfaces::{Surface, SurfaceSource};
use crate::results::ApostAttachment;
use crate::session::SESSION_EXTENSION;
use crate::state::LoadedStructure;
use crate::util::molecules_share_geometry;

/// Every kind of file APOST3Dview opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    Fchk,
    Molden,
    Xyz,
    Cube,
    ApostResults,
    Session,
}

impl FileKind {
    pub(crate) const ALL: [FileKind; 6] = [FileKind::Fchk, FileKind::Molden, FileKind::Xyz, FileKind::Cube, FileKind::ApostResults, FileKind::Session];

    /// For the Open menu and the file dialog's filter.
    pub(crate) fn label(self) -> &'static str {
        match self {
            FileKind::Fchk => "Gaussian checkpoint (.fchk)",
            FileKind::Molden => "Molden (.molden)",
            FileKind::Xyz => "XYZ coordinates (.xyz)",
            FileKind::Cube => "Gaussian cube (.cube)",
            FileKind::ApostResults => "APOST-3D results (.apost.json)",
            FileKind::Session => "APOST3Dview session (.a3dview)",
        }
    }

    /// Extensions for the file dialog. The dialog filters on the last
    /// extension only, so APOST-3D results are offered as `.json`.
    fn extensions(self) -> &'static [&'static str] {
        match self {
            FileKind::Fchk => &["fchk"],
            FileKind::Molden => &["molden", "inp"],
            FileKind::Xyz => &["xyz"],
            FileKind::Cube => &["cube"],
            FileKind::ApostResults => &["json"],
            FileKind::Session => &[SESSION_EXTENSION],
        }
    }

    pub(crate) fn of_path(path: &Path) -> Option<FileKind> {
        if is_results_file(path) {
            return Some(FileKind::ApostResults);
        }
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        FileKind::ALL.into_iter().find(|kind| kind.extensions().contains(&extension.as_str()))
    }
}

fn file_label(path: &Path) -> String {
    match crate::util::file_display_name(path) {
        name if name.is_empty() => "untitled".into(),
        name => name,
    }
}

/// Some programs write Molden files as `.inp`, which is also the extension
/// of APOST-3D inputs. A Molden file opens with a `[Molden Format]` or
/// `[Atoms]` section tag; anything else is not one.
fn looks_like_molden(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else { return false };
    BufReader::new(file)
        .lines()
        .take(20)
        .map_while(Result::ok)
        .any(|line| {
            let line = line.trim().to_ascii_lowercase();
            line.starts_with("[molden format]") || line.starts_with("[atoms]")
        })
}

impl App {
    /// Shows the native file dialog, filtered to one kind of file, or to
    /// every supported kind when `kind` is `None`, and opens the choice.
    pub(crate) fn open_dialog(&mut self, kind: Option<FileKind>) {
        let mut dialog = rfd::FileDialog::new();
        dialog = match kind {
            Some(kind) => dialog.add_filter(kind.label(), kind.extensions()),
            None => {
                let all: Vec<&str> = FileKind::ALL.iter().flat_map(|k| k.extensions().iter().copied()).collect();
                dialog.add_filter("All supported files", &all)
            }
        };
        if let Some(paths) = dialog.pick_files() {
            self.open_paths(paths);
        }
    }

    /// Opens files of any supported kind, each recognized by its name. The
    /// first one opened becomes the active structure.
    pub fn open_paths(&mut self, paths: Vec<PathBuf>) {
        let mut first_new_index = None;
        // Structures that already got a surface from this batch: further
        // cubes for them are added without changing what is shown.
        let mut given_surface: Vec<usize> = Vec::new();
        for path in paths {
            let label = file_label(&path);
            let opened = match FileKind::of_path(&path) {
                Some(FileKind::Fchk) => self.open_wavefunction_file(&path, Molecule::from_fchk, parse_fchk_wavefunction, true),
                Some(FileKind::Molden) => {
                    if looks_like_molden(&path) {
                        self.open_wavefunction_file(&path, Molecule::from_molden, parse_molden_wavefunction, false)
                    } else {
                        self.show_warning(format!(
                            "{label} is not a Molden file. APOST-3D input files cannot be opened: open the job's .fchk or .apost.json instead."
                        ));
                        None
                    }
                }
                Some(FileKind::Xyz) => self.open_xyz(&path),
                Some(FileKind::Cube) => {
                    let opened = self.open_cube(&path, &given_surface);
                    given_surface.extend(opened);
                    opened
                }
                Some(FileKind::ApostResults) => self.open_results(&path),
                // A session replaces everything and sets its own active
                // structure and camera.
                Some(FileKind::Session) => {
                    self.open_session(&path);
                    first_new_index = None;
                    None
                }
                None => {
                    self.show_warning(format!("{label}: not a supported file type"));
                    None
                }
            };
            if let Some(index) = opened {
                first_new_index.get_or_insert(index);
            }
        }
        if let Some(index) = first_new_index {
            self.set_active(index);
        }
    }

    /// Opens one file as one structure, the way a session recorded it: a
    /// `.cube` becomes a structure of its own without its surface (the
    /// session lists the surfaces), and nothing is merged into structures
    /// already open.
    pub(crate) fn open_structure_file(&mut self, path: &Path) -> Option<usize> {
        match FileKind::of_path(path)? {
            FileKind::Fchk => self.open_wavefunction_file(path, Molecule::from_fchk, parse_fchk_wavefunction, true),
            FileKind::Molden => self.open_wavefunction_file(path, Molecule::from_molden, parse_molden_wavefunction, false),
            FileKind::Xyz => self.open_xyz(path),
            FileKind::Cube => {
                let cube = parse_cube(path).ok()?;
                Some(self.push_structure(LoadedStructure::new(file_label(path), cube.molecule, Some(path.to_path_buf()))))
            }
            FileKind::ApostResults => self.open_results(path),
            FileKind::Session => None,
        }
    }

    fn push_structure(&mut self, structure: LoadedStructure) -> usize {
        self.structures.push(structure);
        self.structures.len() - 1
    }

    /// A file that carries both a geometry and a wavefunction (`.fchk`,
    /// `.molden`). Parsing the wavefunction (basis set + MO coefficients) is
    /// a separate, slower full-file pass from the geometry-only one; failure
    /// there just means no orbital generation for that structure, not a
    /// failure to open it. A `.fchk` is the calculation alone: APOST-3D
    /// results written next to it are noted (`results_nearby`, offered in
    /// the APOST-3D window) but only loaded on request, or by opening the
    /// results file itself (Martí, 2026-10-19).
    fn open_wavefunction_file(
        &mut self,
        path: &Path,
        load_molecule: fn(&Path) -> std::io::Result<Molecule>,
        load_wavefunction: fn(&Path) -> std::io::Result<Wavefunction>,
        look_for_results: bool,
    ) -> Option<usize> {
        let label = file_label(path);
        let molecule = match load_molecule(path) {
            Ok(molecule) => molecule,
            Err(err) => {
                self.show_warning(format!("Could not load {label}: {err}"));
                return None;
            }
        };
        let mut structure = LoadedStructure::new(label.clone(), molecule, Some(path.to_path_buf()));
        match load_wavefunction(path) {
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
        if look_for_results {
            structure.results_nearby = find_results_for(path);
        }
        Some(self.push_structure(structure))
    }

    fn open_xyz(&mut self, path: &Path) -> Option<usize> {
        match parse_xyz(path) {
            Ok(molecule) => Some(self.push_structure(LoadedStructure::new(file_label(path), molecule, Some(path.to_path_buf())))),
            Err(err) => {
                self.show_warning(format!("Could not load {}: {err}", file_label(path)));
                None
            }
        }
    }

    /// A `.cube` file becomes a surface of the open structure with the
    /// same geometry (the active one first), so several orbitals of one
    /// molecule, or a cube and the `.fchk` it came from, end up together;
    /// otherwise it opens as a new structure with the cube's own molecule.
    fn open_cube(&mut self, path: &Path, already_showing: &[usize]) -> Option<usize> {
        let cube_file = match parse_cube(path) {
            Ok(cube_file) => cube_file,
            Err(err) => {
                self.show_warning(format!("Could not load {}: {err}", file_label(path)));
                return None;
            }
        };
        let candidates = self.active_structure.into_iter().chain(0..self.structures.len());
        let index = match candidates.into_iter().find(|&i| molecules_share_geometry(&self.structures[i].molecule, &cube_file.molecule)) {
            Some(index) => index,
            None => self.push_structure(LoadedStructure::new(file_label(path), cube_file.molecule, Some(path.to_path_buf()))),
        };
        let surface = Surface::new(file_label(path), cube_file.grid, self.surface_defaults, SurfaceSource::Cube(path.to_path_buf()));
        self.add_surfaces(index, vec![surface], !already_showing.contains(&index));
        Some(index)
    }

    /// An APOST-3D results file opened directly. Its `.fchk` (named in the
    /// file, next to it) is opened too and the results attached to it; if
    /// that `.fchk` is already open, the results attach to the open one.
    /// Without a `.fchk`, the geometry stored in the results file is shown
    /// instead, with a warning, since orbitals are then unavailable.
    fn open_results(&mut self, path: &Path) -> Option<usize> {
        let label = file_label(path);
        let results = match load_apost_results(path) {
            Ok(results) => results,
            Err(err) => {
                self.show_warning(format!("Could not load {label}: {err}"));
                return None;
            }
        };

        if let Some(fchk) = wavefunction_path_for(path, &results) {
            let already_open = self.structures.iter().position(|s| s.source_path.as_deref().is_some_and(|p| same_file(p, &fchk)));
            let index = match already_open {
                Some(index) => index,
                None => self.open_wavefunction_file(&fchk, Molecule::from_fchk, parse_fchk_wavefunction, false)?,
            };
            self.attach_results(index, path);
            return Some(index);
        }

        self.show_warning(format!(
            "No .fchk found for {label}: showing the geometry stored in the results file. Orbitals are not available."
        ));
        let molecule = results.molecule().unwrap_or_else(|| Molecule::from_atoms(Vec::new(), Vec::new()));
        let mut structure = LoadedStructure::new(label, molecule, Some(path.to_path_buf()));
        structure.apost = Some(ApostAttachment::without_fchk(path, results));
        self.open_window(crate::windows::ToolWindow::Apost);
        Some(self.push_structure(structure))
    }
}

pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
