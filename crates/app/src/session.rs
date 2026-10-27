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
use crate::isosurfaces::{OrbitalColors, Surface, SurfaceSettings, SurfaceSource};
use crate::orbitals::{density_surface, orbital_surfaces, OrbitalChoice, OrbitalNames, SetContext};
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
    /// The Export window's figure: panels, captions, layout (sessions
    /// before panels have none, and get the defaults).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    figure: Option<FigureState>,
}

#[derive(Serialize, Deserialize)]
struct FigureState {
    settings: crate::panels::FigureSettings,
    #[serde(default)]
    panels: Vec<PanelState>,
}

/// A panel: a structure (1-based position in `structures`), the surface
/// shown alone in it (absent: the structure as shown), its own caption
/// (absent: automatic).
#[derive(Serialize, Deserialize)]
struct PanelState {
    structure: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    surface: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    caption: String,
}

/// A camera: what it looks at, from how far, turned how.
#[derive(Serialize, Deserialize, Clone, Copy)]
struct CameraState {
    target: [f32; 3],
    distance: f32,
    /// Quaternion, x y z w.
    orientation: [f32; 4],
}

impl CameraState {
    fn of(camera: &apost3dview_render::OrbitCamera) -> Self {
        Self { target: camera.target.to_array(), distance: camera.distance, orientation: camera.orientation.to_array() }
    }

    /// `base` (field of view, clipping) turned and placed as saved.
    fn apply(&self, base: &apost3dview_render::OrbitCamera) -> apost3dview_render::OrbitCamera {
        let mut camera = *base;
        camera.target = Vec3::from_array(self.target);
        camera.distance = self.distance;
        // Kept exactly as saved (re-normalizing would shift it by a
        // rounding error and the picture by a pixel); only a damaged value
        // is fixed.
        let orientation = Quat::from_array(self.orientation);
        camera.orientation = if orientation.is_normalized() { orientation } else { orientation.normalize() };
        camera
    }
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
    /// "Keep the view when switching structures".
    #[serde(default)]
    keep_view: bool,
    /// Sessions saved before the choice existed were seen in perspective,
    /// and reopen that way.
    #[serde(default)]
    projection: Projection,
}

/// Appearance > Projection.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
enum Projection {
    Orthographic,
    #[default]
    Perspective,
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
    /// Appearance > Orbital colors, for surfaces drawn after reopening
    /// (each saved surface keeps its own colors). Absent in sessions saved
    /// before 2026-10-27: the default pair.
    #[serde(default)]
    orbital_colors: OrbitalColors,
}

#[derive(Serialize, Deserialize)]
struct StructureState {
    /// Relative to the session file's folder.
    file: String,
    /// Where the file was when the session was saved; tried when the
    /// relative path doesn't lead to it. Optional: a session shared with
    /// its files (a data deposit) needs only the relative paths.
    #[serde(default)]
    file_absolute: String,
    /// The atoms the settings below refer to, to notice a changed file.
    atomic_numbers: Vec<u32>,
    /// Size and SHA-256 of `file` when the session was saved (sessions
    /// saved before 2026-10-26 have none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_fingerprint: Option<Fingerprint>,
    /// The APOST-3D results file opened for this structure (relative, then
    /// absolute), when it is not `file` itself: a `.fchk` is opened
    /// without its results, which are loaded on request (2026-10-19).
    /// Sessions saved before have none, and attach the results found next
    /// to the `.fchk` when they recorded a results view, as they did then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    results: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    results_absolute: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    results_fingerprint: Option<Fingerprint>,
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
    /// The view this structure was left with, when it isn't the active one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    view: Option<CameraState>,
    /// Aligned onto another structure on selected atoms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alignment: Option<AlignmentState>,
}

/// Where an aligned structure's coordinates sit in the shared frame
/// (`shared = rotation * own + translation`), and what it was aligned onto.
#[derive(Serialize, Deserialize)]
struct AlignmentState {
    /// Quaternion, x y z w.
    rotation: [f32; 4],
    translation: [f32; 3],
    onto: String,
    atoms: usize,
    rmsd: f64,
}

#[derive(Serialize, Deserialize)]
struct MeasurementState {
    atoms: Vec<usize>,
    label_offset: [f32; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum SurfaceOrigin {
    Cube {
        file: String,
        #[serde(default)]
        file_absolute: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fingerprint: Option<Fingerprint>,
    },
    Orbital {
        spin: Spin,
        orbital: usize,
        grid_spacing_bohr: f64,
        box_padding_bohr: f64,
        /// The analysis set ("eos", "geos", "oslo") or the natural orbitals
        /// of a density matrix ("natural-MP2"); absent for SCF orbitals.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        orbital_set: Option<String>,
        /// An EFO times its fragment's atomic weight (APOST-3D 0.16 on);
        /// absent = as stored in the orbital file.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fragment_weighted: bool,
    },
    /// The total or spin density, recomputed from the wavefunction.
    Density {
        spin_density: bool,
        grid_spacing_bohr: f64,
        box_padding_bohr: f64,
        /// The `.fchk` density matrix it comes from ("MP2"; "SCF" for a
        /// correlated 1-RDM), through its natural orbitals; absent for the
        /// density of the occupied orbitals.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        density_matrix: Option<String>,
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
    /// Analysis orbitals colored by fragment (default off).
    #[serde(default)]
    by_fragment: bool,
    #[serde(flatten)]
    origin: SurfaceOrigin,
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

/// What a file held when a session was saved: its size and SHA-256, so
/// that reopening the session notices a file that changed (rerun, edited,
/// replaced) even when its atoms are the same.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct Fingerprint {
    size: u64,
    sha256: String,
}

/// The fingerprint of `path`, cached by size and modification time: a large
/// `.fchk` is read once, not at every export that saves its session.
fn fingerprint(path: &Path) -> Option<Fingerprint> {
    use sha2::Digest;
    use std::collections::HashMap;
    use std::io::Read;
    use std::sync::{Mutex, OnceLock};
    use std::time::SystemTime;
    /// Path -> (size, modification time, SHA-256).
    type Cache = Mutex<HashMap<PathBuf, (u64, SystemTime, String)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let meta = std::fs::metadata(path).ok()?;
    let (size, modified) = (meta.len(), meta.modified().ok()?);
    let cache = CACHE.get_or_init(Default::default);
    if let Some((s, m, hash)) = cache.lock().ok()?.get(path)
        && *s == size
        && *m == modified
    {
        return Some(Fingerprint { size, sha256: hash.clone() });
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let sha256: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    cache.lock().ok()?.insert(path.to_path_buf(), (size, modified, sha256.clone()));
    Some(Fingerprint { size, sha256 })
}

/// Notes a file that differs from the one the session was saved with.
fn check_fingerprint(path: &Path, saved: Option<&Fingerprint>, problems: &mut Vec<String>) {
    if let Some(saved) = saved
        && fingerprint(path).as_ref() != Some(saved)
    {
        problems.push(format!("{} is not the file this session was saved with (its contents changed)", crate::util::file_display_name(path)));
    }
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
                            SurfaceSource::Cube(file) => SurfaceOrigin::Cube { file: relative_path(base, file), file_absolute: file.to_string_lossy().into_owned(), fingerprint: fingerprint(file) },
                            SurfaceSource::Density { spin, spacing_bohr, padding_bohr, natural } => SurfaceOrigin::Density {
                                spin_density: *spin,
                                grid_spacing_bohr: *spacing_bohr,
                                box_padding_bohr: *padding_bohr,
                                density_matrix: natural.clone(),
                            },
                            SurfaceSource::Orbital { choice, spacing_bohr, padding_bohr, set, weighted } => SurfaceOrigin::Orbital {
                                spin: if choice.beta { Spin::Beta } else { Spin::Alpha },
                                orbital: choice.mo_index + 1,
                                grid_spacing_bohr: *spacing_bohr,
                                box_padding_bohr: *padding_bohr,
                                orbital_set: set.clone(),
                                fragment_weighted: *weighted,
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
                let results = structure.apost.as_ref().map(|a| &a.results_path).filter(|r| !crate::files::same_file(r, path));
                Some(StructureState {
                    file: relative_path(base, path),
                    file_absolute: path.to_string_lossy().into_owned(),
                    atomic_numbers: molecule.atomic_numbers.clone(),
                    file_fingerprint: fingerprint(path),
                    results: results.map(|r| relative_path(base, r)),
                    results_absolute: results.map(|r| r.to_string_lossy().into_owned()),
                    results_fingerprint: results.and_then(|r| fingerprint(r)),
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
                    view: structure.view.as_ref().map(CameraState::of),
                    alignment: structure.placement.as_ref().map(|p| AlignmentState {
                        rotation: p.rotation.to_array(),
                        translation: p.translation.to_array(),
                        onto: p.onto.clone(),
                        atoms: p.atoms,
                        rmsd: p.rmsd,
                    }),
                })
            })
            .collect();
        // Structures are saved only when they come from a file: their
        // positions in the session, for the panels.
        let positions: Vec<Option<usize>> = self
            .structures
            .iter()
            .scan(0, |next, s| {
                Some(s.source_path.as_ref().map(|_| {
                    *next += 1;
                    *next
                }))
            })
            .collect();
        let figure = &self.render_export.figure;
        let figure = FigureState {
            settings: figure.settings.clone(),
            panels: figure
                .panels
                .iter()
                .filter_map(|p| {
                    Some(PanelState { structure: (*positions.get(p.structure)?)?, surface: p.surface.clone(), caption: p.caption.clone() })
                })
                .collect(),
        };

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
                keep_view: self.keep_view,
                projection: if self.camera.orthographic { Projection::Orthographic } else { Projection::Perspective },
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
                orbital_colors: self.orbital_colors,
            },
            color_ranges: self.color_ranges.clone(),
            structures,
            figure: Some(figure),
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
                self.show_status(format!("Session saved as {}", crate::util::file_display_name(&path)));
                self.session_path = Some(path);
            }
            Err(err) => self.show_warning(format!("Could not save the session: {err}")),
        }
    }

    /// Replaces everything open with the session in `path`.
    pub(crate) fn open_session(&mut self, path: &Path) {
        let name = crate::util::file_display_name(path);
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
            check_fingerprint(&file, state.file_fingerprint.as_ref(), &mut problems);
            let Some(index) = self.open_structure_file(&file) else {
                problems.push(format!("{} could not be opened", file_name(&state.file)));
                continue;
            };
            opened.insert(position, index);
            // Its APOST-3D results, as the session had them open.
            let results = match (&state.results, &state.results_absolute) {
                (Some(relative), absolute) => {
                    let found = resolve(&base, relative, absolute.as_deref().unwrap_or(""));
                    if found.is_none() {
                        problems.push(format!("{} was not found", file_name(relative)));
                    }
                    found
                }
                (None, _) if state.results_view.is_some() && self.structures[index].apost.is_none() => self.structures[index].results_nearby.clone(),
                _ => None,
            };
            if let Some(results) = results {
                check_fingerprint(&results, state.results_fingerprint.as_ref(), &mut problems);
                self.attach_results(index, &results);
            }
            self.restore_structure(index, state, &base, &mut problems);
        }

        let active = session.active_structure.and_then(|p| opened.get(&p).copied()).or_else(|| opened.values().next().copied());
        if let Some(active) = active {
            self.set_active(active);
        }
        let view = &session.view;
        let saved = CameraState { target: view.camera_target, distance: view.camera_distance, orientation: view.camera_orientation };
        self.camera = saved.apply(&self.camera);
        self.camera.orthographic = view.projection == Projection::Orthographic;
        self.keep_view = view.keep_view;
        for (position, state) in session.structures.iter().enumerate() {
            let Some(&index) = opened.get(&position) else { continue };
            if let Some(camera) = &state.view {
                self.structures[index].view = Some(camera.apply(&self.camera));
            }
            self.structures[index].placement = state.alignment.as_ref().map(|a| crate::alignment::Placement {
                // Kept exactly as saved, like the camera's orientation.
                rotation: Some(Quat::from_array(a.rotation)).filter(|q| q.is_normalized()).unwrap_or_else(|| Quat::from_array(a.rotation).normalize()),
                translation: Vec3::from_array(a.translation),
                onto: a.onto.clone(),
                atoms: a.atoms,
                rmsd: a.rmsd,
            });
        }
        if let Some(active) = self.active_structure {
            self.camera.world_up = crate::alignment::up_in(crate::alignment::motion(self.structures[active].placement.as_ref()));
        }
        let figure = &mut self.render_export.figure;
        figure.preview = Default::default();
        match &session.figure {
            Some(state) => {
                figure.settings = state.settings.clone();
                figure.panels = state
                    .panels
                    .iter()
                    .filter_map(|p| {
                        let index = *opened.get(&p.structure.checked_sub(1)?)?;
                        Some(crate::panels::Panel { structure: index, surface: p.surface.clone(), caption: p.caption.clone() })
                    })
                    .collect();
            }
            None => {
                figure.settings = Default::default();
                figure.panels.clear();
            }
        }
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
        self.set_orbital_colors(a.orbital_colors);
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
        if let (Some(view), Some(attachment)) = (&state.results_view, structure.apost.as_mut())
            && attachment.results.is_ok()
        {
            attachment.view = Some(view.clone());
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
        // (set, spacing bits, padding bits) -> (slot, orbital).
        type Group = (Option<String>, u64, u64, bool);
        let mut groups: BTreeMap<Group, Vec<(usize, OrbitalChoice)>> = BTreeMap::new();
        for (slot, state) in states.iter().enumerate() {
            match &state.origin {
                SurfaceOrigin::Cube { file, file_absolute, fingerprint: saved } => match resolve(base, file, file_absolute).map(|p| (parse_cube(&p), p)) {
                    Some((Ok(cube), path)) => {
                        check_fingerprint(&path, saved.as_ref(), problems);
                        slots[slot] = Some(Surface::new(state.label.clone(), cube.grid, settings_of(state), SurfaceSource::Cube(path)));
                    }
                    Some((Err(_), _)) => problems.push(format!("{} could not be read", file_name(file))),
                    None => problems.push(format!("{} was not found", file_name(file))),
                },
                SurfaceOrigin::Density { spin_density, grid_spacing_bohr, box_padding_bohr, density_matrix } => {
                    let structure = &self.structures[index];
                    let natural = density_matrix.as_ref().map(|kind| structure.densities.as_ref().and_then(|d| d.natural.iter().find(|n| &n.kind == kind)).ok_or(kind));
                    let generated = match (&structure.wavefunction, natural) {
                        (None, _) => Err("no wavefunction".to_string()),
                        (Some(_), Some(Err(kind))) => Err(format!("the file has no {kind} density matrix")),
                        (Some(wfn), natural) => density_surface(wfn, *spin_density, *grid_spacing_bohr, *box_padding_bohr, natural.and_then(Result::ok), self.orbital_colors),
                    };
                    match generated {
                        Ok(mut surface) => {
                            surface.settings = settings_of(state);
                            slots[slot] = Some(surface);
                        }
                        Err(err) => problems.push(format!("the density of {name} could not be generated ({err})")),
                    }
                }
                SurfaceOrigin::Orbital { spin, orbital, grid_spacing_bohr, box_padding_bohr, orbital_set, fragment_weighted } => {
                    let choice = OrbitalChoice { beta: *spin == Spin::Beta, mo_index: orbital.saturating_sub(1) };
                    groups.entry((orbital_set.clone(), grid_spacing_bohr.to_bits(), box_padding_bohr.to_bits(), *fragment_weighted)).or_default().push((slot, choice));
                }
            }
        }
        for ((set_name, spacing, padding, weighted), members) in groups {
            let choices: Vec<OrbitalChoice> = members.iter().map(|(_, c)| *c).collect();
            let generated = match &set_name {
                None => match &self.structures[index].wavefunction {
                    None => Err("no wavefunction".to_string()),
                    Some(wfn) => orbital_surfaces(wfn, &choices, f64::from_bits(spacing), f64::from_bits(padding), SurfaceSettings::default(), OrbitalNames::Scf),
                },
                Some(key) if key.starts_with("natural-") => match self.structures[index].densities.as_ref().and_then(|d| d.by_key(key)) {
                    None => Err(format!("the file has no {} density matrix", &key["natural-".len()..])),
                    Some(set) => orbital_surfaces(&set.orbitals, &choices, f64::from_bits(spacing), f64::from_bits(padding), SurfaceSettings::default(), OrbitalNames::Natural(set)),
                },
                Some(analysis) => match self.structures[index].orbital_sets.iter().position(|s| &s.set.key() == analysis) {
                    None => Err(format!("its {analysis} orbital file was not found")),
                    Some(position) if !self.load_orbital_set(index, position) => Err(format!("its {analysis} orbital file could not be used")),
                    Some(position) => {
                        let structure = &self.structures[index];
                        let set = &structure.orbital_sets[position];
                        match (&set.wavefunction, structure.apost.as_ref().map(|a| &a.results)) {
                            (Some(Ok(wfn)), Some(Ok(results))) => {
                                let context = SetContext { set, results, molecule: &structure.molecule, weighted };
                                orbital_surfaces(wfn, &choices, f64::from_bits(spacing), f64::from_bits(padding), SurfaceSettings::default(), OrbitalNames::Set(&context))
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
