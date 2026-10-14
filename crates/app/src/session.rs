//! Sessions (`.a3dview` files): a figure setup saved to reopen later and
//! get the same picture — which files were open, what was hidden, created
//! or measured, which surfaces were shown and how, the camera and the
//! look of the scene.
//!
//! A session refers to its files rather than containing them, with paths
//! relative to the session file (so moving the whole folder keeps it
//! working; the absolute path is kept as a fallback). Generated orbitals
//! are stored as a recipe and computed again on opening. Atom numbers are
//! 1-based, as everywhere in the app, since the file is meant to be
//! readable.
//!
//! On opening, nothing is applied silently to something that changed: a
//! missing file is skipped, and a file whose atoms differ from those
//! recorded keeps its surfaces but not its per-atom settings (hidden
//! atoms, bonds, measurements); both are reported.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use apost3dview_core::{parse_cube, Bond, LengthUnit, MeasurementKind};
use apost3dview_render::{AoSettings, BondVisualStyle, ColorScale, DofSettings, IsosurfaceMaterial, Material};
use egui::Color32;
use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::isosurfaces::{Surface, SurfaceSettings, SurfaceSource};
use crate::orbitals::{orbital_surfaces, OrbitalChoice, SetContext};
use crate::results_view::ResultsView;
use crate::state::{AtomLabelMode, Measurement};
use crate::util::find_bond_between;

pub(crate) const SESSION_EXTENSION: &str = "a3dview";
const SESSION_FORMAT: &str = "apost3dview-session";
/// Raised when a change means an older APOST3Dview can't read the file.
const SESSION_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct SessionFile {
    format: String,
    version: u32,
    /// The APOST3Dview version that wrote the file, for information.
    #[serde(default)]
    written_by: String,
    /// Position in `structures` (0-based) of the structure on screen.
    active_structure: Option<usize>,
    view: ViewState,
    appearance: AppearanceState,
    /// Fixed color ranges, by quantity id (empty: all automatic).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    color_ranges: BTreeMap<String, ColorScale>,
    #[serde(default)]
    structures: Vec<StructureState>,
}

#[derive(Serialize, Deserialize)]
struct ViewState {
    camera_target: [f32; 3],
    camera_distance: f32,
    /// Quaternion, x y z w.
    camera_orientation: [f32; 4],
    /// Screen direction of auto-spin and of the GIF turn.
    spin_direction: [f32; 2],
    spin_speed: f32,
}

#[derive(Serialize, Deserialize)]
struct AppearanceState {
    ambient_occlusion: bool,
    depth_of_field: bool,
    atom_labels: AtomLabelMode,
    atom_label_size: f32,
    atom_label_color: [u8; 4],
    distance_unit: LengthUnit,
    measurement_font_size: f32,
    measurement_decimals: usize,
    measurement_bold: bool,
    measurement_text_color: [u8; 4],
    measurement_line_color: [f32; 3],
    /// Atom and bond size, light, background (see `Material`).
    material: Material,
    ao_settings: AoSettings,
    dof_settings: DofSettings,
    surface_material: IsosurfaceMaterial,
}

#[derive(Serialize, Deserialize)]
struct StructureState {
    /// Relative to the session file's folder.
    file: String,
    /// Where the file was when the session was saved; tried when the
    /// relative path doesn't lead to it.
    file_absolute: String,
    /// The atoms the settings below refer to, to notice a changed file.
    atomic_numbers: Vec<u32>,
    /// What the APOST-3D results show on the molecule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    results_view: Option<ResultsView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_surface: Option<usize>,
    #[serde(default)]
    hidden_atoms: Vec<usize>,
    #[serde(default)]
    added_bonds: Vec<[usize; 2]>,
    #[serde(default)]
    hidden_bonds: Vec<[usize; 2]>,
    #[serde(default)]
    dashed_bonds: Vec<[usize; 2]>,
    #[serde(default)]
    measurements: Vec<MeasurementState>,
    #[serde(default)]
    surfaces: Vec<SurfaceState>,
}

#[derive(Serialize, Deserialize)]
struct MeasurementState {
    atoms: Vec<usize>,
    label_offset: [f32; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum SurfaceOrigin {
    Cube { file: String, file_absolute: String },
    Orbital {
        spin: Spin,
        orbital: usize,
        grid_spacing_bohr: f64,
        box_padding_bohr: f64,
        /// The analysis set ("eos", "geos", "oslo"); absent for SCF orbitals.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        orbital_set: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Spin {
    Alpha,
    Beta,
}

#[derive(Serialize, Deserialize)]
struct SurfaceState {
    label: String,
    visible: bool,
    isovalue: f32,
    refinement: usize,
    both_signs: bool,
    positive_color: [u8; 4],
    negative_color: [u8; 4],
    opacity: f32,
    /// Analysis orbitals colored by fragment (default on).
    #[serde(default = "default_true")]
    by_fragment: bool,
    #[serde(flatten)]
    origin: SurfaceOrigin,
}

fn default_true() -> bool {
    true
}

fn color_to_array(color: Color32) -> [u8; 4] {
    color.to_array()
}

fn color_from_array(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3])
}

/// `path` relative to `base` when both are on the same root, else `path`
/// itself — written with `/`, so a session saved on one system reads on
/// another.
fn relative_path(base: &Path, path: &Path) -> String {
    let base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let base_parts: Vec<Component> = base.components().collect();
    let path_parts: Vec<Component> = path.components().collect();
    if base_parts.first() != path_parts.first() {
        return path.to_string_lossy().into_owned();
    }
    let common = base_parts.iter().zip(&path_parts).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_string(); base_parts.len() - common];
    parts.extend(path_parts[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    parts.join("/")
}

/// The file a session refers to: relative to the session first, then
/// where it was when saved.
fn resolve(base: &Path, relative: &str, absolute: &str) -> Option<PathBuf> {
    let from_session = base.join(relative);
    if from_session.is_file() {
        return Some(from_session);
    }
    let original = PathBuf::from(absolute);
    original.is_file().then_some(original)
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

fn atom_pair(molecule: &apost3dview_core::Molecule, bond: usize) -> [usize; 2] {
    let b = &molecule.bonds[bond];
    [b.atom_a + 1, b.atom_b + 1]
}

impl App {
    /// The current setup as a session whose paths are relative to `base`.
    fn session_state(&self, base: &Path) -> SessionFile {
        let structures = self
            .structures
            .iter()
            .filter_map(|structure| {
                let path = structure.source_path.as_ref()?;
                let molecule = &structure.molecule;
                let bonds_with = |style: BondVisualStyle| {
                    structure.bond_styles.iter().enumerate().filter(|(_, s)| **s == style).map(|(i, _)| atom_pair(molecule, i)).collect::<Vec<_>>()
                };
                let surfaces = structure
                    .surfaces
                    .iter()
                    .map(|surface| {
                        let origin = match &surface.source {
                            SurfaceSource::Cube(file) => SurfaceOrigin::Cube { file: relative_path(base, file), file_absolute: file.to_string_lossy().into_owned() },
                            SurfaceSource::Orbital { choice, spacing_bohr, padding_bohr, set } => SurfaceOrigin::Orbital {
                                spin: if choice.beta { Spin::Beta } else { Spin::Alpha },
                                orbital: choice.mo_index + 1,
                                grid_spacing_bohr: *spacing_bohr,
                                box_padding_bohr: *padding_bohr,
                                orbital_set: set.clone(),
                            },
                        };
                        let s = &surface.settings;
                        SurfaceState {
                            label: surface.label.clone(),
                            visible: surface.visible,
                            isovalue: s.isovalue,
                            refinement: s.refinement,
                            both_signs: s.both_signs,
                            positive_color: color_to_array(s.positive_color),
                            negative_color: color_to_array(s.negative_color),
                            opacity: s.opacity,
                            by_fragment: s.by_fragment,
                            origin,
                        }
                    })
                    .collect();
                let mut hidden_atoms: Vec<usize> = structure.hidden_atoms.iter().map(|&i| i + 1).collect();
                hidden_atoms.sort_unstable();
                let mut hidden_bonds: Vec<[usize; 2]> = structure.hidden_bonds.iter().map(|&i| atom_pair(molecule, i)).collect();
                hidden_bonds.sort_unstable();
                Some(StructureState {
                    file: relative_path(base, path),
                    file_absolute: path.to_string_lossy().into_owned(),
                    atomic_numbers: molecule.atomic_numbers.clone(),
                    results_view: structure.apost.as_ref().and_then(|a| a.view.clone()),
                    selected_surface: structure.selected_surface,
                    hidden_atoms,
                    added_bonds: (structure.perceived_bonds..molecule.bonds.len()).map(|i| atom_pair(molecule, i)).collect(),
                    hidden_bonds,
                    dashed_bonds: bonds_with(BondVisualStyle::TransitionState),
                    measurements: structure
                        .measurements
                        .iter()
                        .map(|m| MeasurementState { atoms: m.kind.atoms().iter().map(|a| a + 1).collect(), label_offset: [m.label_offset.x, m.label_offset.y] })
                        .collect(),
                    surfaces,
                })
            })
            .collect();

        let camera = &self.camera;
        SessionFile {
            format: SESSION_FORMAT.to_string(),
            version: SESSION_VERSION,
            written_by: format!("APOST3Dview {}", env!("CARGO_PKG_VERSION")),
            active_structure: self.active_structure,
            view: ViewState {
                camera_target: camera.target.to_array(),
                camera_distance: camera.distance,
                camera_orientation: camera.orientation.to_array(),
                spin_direction: self.spin_direction.to_array(),
                spin_speed: self.auto_spin_speed,
            },
            appearance: AppearanceState {
                ambient_occlusion: self.ao_enabled,
                depth_of_field: self.dof_enabled,
                atom_labels: self.atom_label_mode,
                atom_label_size: self.atom_label_style.relative_size,
                atom_label_color: color_to_array(self.atom_label_style.text_color),
                distance_unit: self.coordinate_unit,
                measurement_font_size: self.measurement_style.font_size,
                measurement_decimals: self.measurement_style.decimal_places,
                measurement_bold: self.measurement_style.bold,
                measurement_text_color: color_to_array(self.measurement_style.text_color),
                measurement_line_color: self.measurement_style.line_color,
                material: self.material,
                ao_settings: self.ao_settings,
                dof_settings: self.dof_settings,
                surface_material: self.isosurface_material,
            },
            color_ranges: self.color_ranges.clone(),
            structures,
        }
    }

    /// Writes the current setup to `path`. Doesn't change which session is
    /// "the" open one (see `save_session`), so it also serves to save a
    /// copy next to an exported image.
    pub fn save_session_to(&self, path: &Path) -> Result<(), String> {
        let base = path.parent().unwrap_or(Path::new("."));
        let text = toml::to_string_pretty(&self.session_state(base)).map_err(|err| err.to_string())?;
        let header = "# APOST3Dview session: open it with APOST3Dview (Open, drag and drop, or the command line).\n\n";
        std::fs::write(path, format!("{header}{text}")).map_err(|err| err.to_string())
    }

    /// Save session (to the open one) or Save session as (always asks).
    pub(crate) fn save_session(&mut self, ask_for_name: bool) {
        if self.structures.is_empty() {
            self.show_warning("Nothing to save: open a structure first.");
            return;
        }
        let path = match (&self.session_path, ask_for_name) {
            (Some(path), false) => path.clone(),
            _ => {
                let stem = self
                    .active_structure
                    .and_then(|i| self.structures[i].source_path.as_ref())
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "session".to_string());
                let mut dialog = rfd::FileDialog::new().add_filter("APOST3Dview session", &[SESSION_EXTENSION]).set_file_name(format!("{stem}.{SESSION_EXTENSION}"));
                if let Some(dir) = self.session_path.as_ref().and_then(|p| p.parent()) {
                    dialog = dialog.set_directory(dir);
                }
                let Some(path) = dialog.save_file() else { return };
                path
            }
        };
        match self.save_session_to(&path) {
            Ok(()) => {
                self.show_status(format!("Session saved as {}", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
                self.session_path = Some(path);
            }
            Err(err) => self.show_warning(format!("Could not save the session: {err}")),
        }
    }

    /// Replaces everything open with the session in `path`.
    pub(crate) fn open_session(&mut self, path: &Path) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let session: SessionFile = match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| toml::from_str(&t).map_err(|e| e.to_string())) {
            Ok(session) => session,
            Err(err) => {
                self.show_warning(format!("Could not open the session {name}: {err}"));
                return;
            }
        };
        if session.format != SESSION_FORMAT {
            self.show_warning(format!("{name} is not an APOST3Dview session."));
            return;
        }
        if session.version > SESSION_VERSION {
            self.show_warning(format!("{name} was saved by a newer APOST3Dview ({}). Please update APOST3Dview to open it.", session.written_by));
            return;
        }

        self.structures.clear();
        self.active_structure = None;
        self.apply_appearance(&session.appearance);
        self.color_ranges = session.color_ranges.clone();

        let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut problems: Vec<String> = Vec::new();
        // Position in the session -> index of the structure opened for it.
        let mut opened: BTreeMap<usize, usize> = BTreeMap::new();
        for (position, state) in session.structures.iter().enumerate() {
            let Some(file) = resolve(&base, &state.file, &state.file_absolute) else {
                problems.push(format!("{} was not found", file_name(&state.file)));
                continue;
            };
            let Some(index) = self.open_structure_file(&file) else {
                problems.push(format!("{} could not be opened", file_name(&state.file)));
                continue;
            };
            opened.insert(position, index);
            self.restore_structure(index, state, &base, &mut problems);
        }

        let active = session.active_structure.and_then(|p| opened.get(&p).copied()).or_else(|| opened.values().next().copied());
        if let Some(active) = active {
            self.set_active(active);
        }
        let view = &session.view;
        self.camera.target = Vec3::from_array(view.camera_target);
        self.camera.distance = view.camera_distance;
        // Kept exactly as saved (re-normalizing would shift it by a rounding
        // error and the picture by a pixel); only a damaged value is fixed.
        let orientation = Quat::from_array(view.camera_orientation);
        self.camera.orientation = if orientation.is_normalized() { orientation } else { orientation.normalize() };
        self.spin_direction = Vec2::from_array(view.spin_direction).normalize_or(Vec2::X);
        self.auto_spin_speed = view.spin_speed;
        self.rebuild_measurements();
        self.rebuild_isosurface();

        self.session_path = Some(path.to_path_buf());
        if problems.is_empty() {
            self.show_status(format!("Session {name} opened"));
        } else {
            self.show_warning(format!("Session {name} opened, but: {}.", problems.join("; ")));
        }
    }

    fn apply_appearance(&mut self, a: &AppearanceState) {
        self.ao_enabled = a.ambient_occlusion;
        self.dof_enabled = a.depth_of_field;
        self.atom_label_mode = a.atom_labels;
        self.atom_label_style.relative_size = a.atom_label_size;
        self.atom_label_style.text_color = color_from_array(a.atom_label_color);
        self.coordinate_unit = a.distance_unit;
        self.measurement_style.font_size = a.measurement_font_size;
        self.measurement_style.decimal_places = a.measurement_decimals;
        self.measurement_style.bold = a.measurement_bold;
        self.measurement_style.text_color = color_from_array(a.measurement_text_color);
        self.measurement_style.line_color = a.measurement_line_color;
        self.material = a.material;
        self.ao_settings = a.ao_settings;
        self.dof_settings = a.dof_settings;
        self.isosurface_material = a.surface_material;
    }

    /// Puts a reopened structure back as the session recorded it.
    fn restore_structure(&mut self, index: usize, state: &StructureState, base: &Path, problems: &mut Vec<String>) {
        let name = file_name(&state.file);
        let same_atoms = self.structures[index].molecule.atomic_numbers == state.atomic_numbers;
        if !same_atoms {
            problems.push(format!("{name} has changed since the session was saved, so its hidden atoms, bonds and measurements were not restored"));
        }

        let surfaces = self.restore_surfaces(index, &state.surfaces, base, &name, problems);
        let structure = &mut self.structures[index];
        structure.surfaces = surfaces;
        structure.selected_surface = state.selected_surface.filter(|&i| i < structure.surfaces.len());
        if let (Some(view), Some(attachment)) = (&state.results_view, structure.apost.as_mut()) {
            if attachment.results.is_ok() {
                attachment.view = Some(view.clone());
            }
        }
        if !same_atoms {
            return;
        }

        let n_atoms = structure.molecule.atomic_numbers.len();
        let valid = |atoms: &[usize]| atoms.iter().all(|&a| a >= 1 && a <= n_atoms);
        for &[a, b] in state.added_bonds.iter().filter(|p| valid(&p[..])) {
            if find_bond_between(&structure.molecule, a - 1, b - 1).is_none() {
                structure.molecule.bonds.push(Bond { atom_a: a - 1, atom_b: b - 1 });
                structure.bond_styles.push(BondVisualStyle::Single);
            }
        }
        for &[a, b] in state.dashed_bonds.iter().filter(|p| valid(&p[..])) {
            if let Some(bond) = find_bond_between(&structure.molecule, a - 1, b - 1) {
                structure.bond_styles[bond] = BondVisualStyle::TransitionState;
            }
        }
        for &[a, b] in state.hidden_bonds.iter().filter(|p| valid(&p[..])) {
            if let Some(bond) = find_bond_between(&structure.molecule, a - 1, b - 1) {
                structure.hidden_bonds.insert(bond);
            }
        }
        structure.hidden_atoms.extend(state.hidden_atoms.iter().filter(|&&a| a >= 1 && a <= n_atoms).map(|a| a - 1));
        for m in state.measurements.iter().filter(|m| valid(&m.atoms)) {
            let atoms: Vec<usize> = m.atoms.iter().map(|a| a - 1).collect();
            if let Some(kind) = MeasurementKind::from_picks(&atoms) {
                structure.measurements.push(Measurement { kind, label_offset: egui::vec2(m.label_offset[0], m.label_offset[1]) });
            }
        }
    }

    /// The structure's surfaces, in the recorded order: cubes read again,
    /// generated orbitals computed again (all of one grid in one pass).
    fn restore_surfaces(&mut self, index: usize, states: &[SurfaceState], base: &Path, name: &str, problems: &mut Vec<String>) -> Vec<Surface> {
        let mut slots: Vec<Option<Surface>> = states.iter().map(|_| None).collect();
        let settings_of = |s: &SurfaceState| SurfaceSettings {
            isovalue: s.isovalue,
            refinement: s.refinement.clamp(1, 3),
            both_signs: s.both_signs,
            positive_color: color_from_array(s.positive_color),
            negative_color: color_from_array(s.negative_color),
            opacity: s.opacity.clamp(0.05, 1.0),
            by_fragment: s.by_fragment,
        };

        // Orbitals, grouped by set and grid so each group is one batched
        // evaluation.
        let mut groups: BTreeMap<(Option<String>, u64, u64), Vec<(usize, OrbitalChoice)>> = BTreeMap::new();
        for (slot, state) in states.iter().enumerate() {
            match &state.origin {
                SurfaceOrigin::Cube { file, file_absolute } => match resolve(base, file, file_absolute).map(|p| (parse_cube(&p), p)) {
                    Some((Ok(cube), path)) => slots[slot] = Some(Surface::new(state.label.clone(), cube.grid, settings_of(state), SurfaceSource::Cube(path))),
                    Some((Err(_), _)) => problems.push(format!("{} could not be read", file_name(file))),
                    None => problems.push(format!("{} was not found", file_name(file))),
                },
                SurfaceOrigin::Orbital { spin, orbital, grid_spacing_bohr, box_padding_bohr, orbital_set } => {
                    let choice = OrbitalChoice { beta: *spin == Spin::Beta, mo_index: orbital.saturating_sub(1) };
                    groups.entry((orbital_set.clone(), grid_spacing_bohr.to_bits(), box_padding_bohr.to_bits())).or_default().push((slot, choice));
                }
            }
        }
        for ((set_name, spacing, padding), members) in groups {
            let choices: Vec<OrbitalChoice> = members.iter().map(|(_, c)| *c).collect();
            let generated = match &set_name {
                None => match &self.structures[index].wavefunction {
                    None => Err("no wavefunction".to_string()),
                    Some(wfn) => orbital_surfaces(wfn, &choices, f64::from_bits(spacing), f64::from_bits(padding), SurfaceSettings::default(), None),
                },
                Some(analysis) => match self.structures[index].orbital_sets.iter().position(|s| &s.set.analysis == analysis) {
                    None => Err(format!("its {analysis} orbital file was not found")),
                    Some(position) if !self.load_orbital_set(index, position) => Err(format!("its {analysis} orbital file could not be used")),
                    Some(position) => {
                        let structure = &self.structures[index];
                        let set = &structure.orbital_sets[position];
                        match (&set.wavefunction, structure.apost.as_ref().map(|a| &a.results)) {
                            (Some(Ok(wfn)), Some(Ok(results))) => {
                                let context = SetContext { set, results, molecule: &structure.molecule };
                                orbital_surfaces(wfn, &choices, f64::from_bits(spacing), f64::from_bits(padding), SurfaceSettings::default(), Some(&context))
                            }
                            _ => Err("no results".to_string()),
                        }
                    }
                },
            };
            match generated {
                Ok(surfaces) => {
                    for ((slot, _), mut surface) in members.into_iter().zip(surfaces) {
                        surface.settings = settings_of(&states[slot]);
                        slots[slot] = Some(surface);
                    }
                }
                Err(err) => problems.push(format!("orbitals of {name} could not be generated ({err})")),
            }
        }

        slots
            .into_iter()
            .zip(states)
            .filter_map(|(surface, state)| {
                surface.map(|mut s| {
                    s.visible = state.visible;
                    s
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::relative_path;
    use std::path::Path;

    #[test]
    fn relative_paths() {
        assert_eq!(relative_path(Path::new("/a/b"), Path::new("/a/b/c.fchk")), "c.fchk");
        assert_eq!(relative_path(Path::new("/a/b"), Path::new("/a/d/c.fchk")), "../d/c.fchk");
        assert_eq!(relative_path(Path::new("/a/b/e"), Path::new("/a/c.fchk")), "../../c.fchk");
    }
}
