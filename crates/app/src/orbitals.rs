//! Orbital generation from a wavefunction (.fchk/.molden).

use std::collections::HashSet;

use apost3dview_core::{generate_mo_grids, ApostResults, MolecularOrbitals, Molecule, Wavefunction};

use crate::app::App;
use crate::isosurfaces::{Surface, SurfaceSettings, SurfaceSource};
use crate::orbital_sets::{fragment_tint, LoadedOrbitalSet};

/// One spin channel's tick-box list in the "Generate orbitals" section —
/// factored out since alpha and beta need identical rendering, just
/// against a different `MolecularOrbitals`/selection set. `spin_label`
/// ("alpha"/"beta") is folded into each row's occupancy tag rather than
/// shown as a separate heading, and into the `ScrollArea`'s id so two
/// instances (restricted files only ever show one) don't collide.
pub(crate) fn show_mo_checklist(ui: &mut egui::Ui, orbitals: &MolecularOrbitals, selected: &mut HashSet<usize>, spin_label: &str, height: f32) {
    let homo = orbitals.homo_index();
    let lumo = orbitals.lumo_index();
    egui::ScrollArea::vertical().id_salt(("mo_checklist", spin_label)).max_height(height).show(ui, |ui| {
        for mo_index in 0..orbitals.num_orbitals() {
            let mo_number = mo_index + 1;
            let energy = orbitals.orbital_energies[mo_index];
            let occ_tag = if mo_number <= orbitals.num_occupied { "occ" } else { "virt" };
            let special_tag = if mo_number == homo {
                "  HOMO"
            } else if mo_number == lumo {
                "  LUMO"
            } else {
                ""
            };
            let mut checked = selected.contains(&mo_index);
            if ui.checkbox(&mut checked, format!("MO {mo_number}  {energy:+.4} Hartree  ({spin_label}, {occ_tag}){special_tag}")).changed() {
                if checked {
                    selected.insert(mo_index);
                } else {
                    selected.remove(&mo_index);
                }
            }
        }
    });
}

/// Grid-density tier for generating an orbital's `.cube`-equivalent grid
/// straight from a `.fchk` wavefunction — the same one-click-preset-or-
/// exact-control shape as `RenderPreset`. Values are grid spacing in
/// Bohr (native to the GTO evaluator); Medium roughly matches Chemcraft's
/// own default spacing (~0.29 Bohr) for a familiar-looking result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrbitalAccuracyPreset {
    Low,
    Medium,
    High,
    Custom,
}


pub(crate) const ORBITAL_SPACING_LOW_BOHR: f64 = 0.40;
pub(crate) const ORBITAL_SPACING_MEDIUM_BOHR: f64 = 0.28;
pub(crate) const ORBITAL_SPACING_HIGH_BOHR: f64 = 0.15;
/// Default padding around the wavefunction's own shell centers on every
/// side — a starting point for an orbital's decaying tail to read as
/// fully closed rather than clipped at the grid boundary, matching the
/// kind of margin `cubegen`/Chemcraft/APOST-3D use by default. Not always
/// enough on its own (a sufficiently diffuse orbital, or a low isovalue
/// chosen to show a larger lobe, can still extend past it — visible as a
/// flat, clipped face on the isosurface) — `OrbitalGenerationState.
/// padding_bohr` is user-adjustable specifically so that case has a fix
/// without needing a single constant to be large enough for every case,
/// which isn't really possible in general.
pub(crate) const ORBITAL_GRID_PADDING_BOHR: f64 = 4.0;

pub(crate) struct OrbitalGenerationState {
    pub(crate) preset: OrbitalAccuracyPreset,
    pub(crate) custom_spacing_bohr: f64,
    pub(crate) padding_bohr: f64,
    /// 1-based, inclusive — state for the "Select range" convenience
    /// button in the MO picker.
    pub(crate) range_start: usize,
    pub(crate) range_end: usize,
}

impl Default for OrbitalGenerationState {
    fn default() -> Self {
        Self {
            preset: OrbitalAccuracyPreset::Medium,
            custom_spacing_bohr: ORBITAL_SPACING_MEDIUM_BOHR,
            padding_bohr: ORBITAL_GRID_PADDING_BOHR,
            range_start: 1,
            range_end: 1,
        }
    }
}

impl OrbitalGenerationState {
    pub(crate) fn resolve_spacing_bohr(&self) -> f64 {
        match self.preset {
            OrbitalAccuracyPreset::Low => ORBITAL_SPACING_LOW_BOHR,
            OrbitalAccuracyPreset::Medium => ORBITAL_SPACING_MEDIUM_BOHR,
            OrbitalAccuracyPreset::High => ORBITAL_SPACING_HIGH_BOHR,
            OrbitalAccuracyPreset::Custom => self.custom_spacing_bohr.max(0.01),
        }
    }
}

/// One orbital to put on a grid: its spin channel and 0-based index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OrbitalChoice {
    pub(crate) beta: bool,
    pub(crate) mo_index: usize,
}

/// "HOMO", "LUMO", "MO 12", with " (alpha)"/" (beta)" for an unrestricted
/// wavefunction only (a restricted one has nothing to tell apart).
fn orbital_label(wfn: &Wavefunction, choice: OrbitalChoice) -> String {
    let orbitals = if choice.beta { wfn.beta.as_ref().unwrap_or(&wfn.alpha) } else { &wfn.alpha };
    let mo_number = choice.mo_index + 1;
    let tag = if mo_number == orbitals.homo_index() {
        "HOMO".to_string()
    } else if mo_number == orbitals.lumo_index() {
        "LUMO".to_string()
    } else {
        format!("MO {mo_number}")
    };
    let spin = match (wfn.beta.is_some(), choice.beta) {
        (false, _) => "",
        (true, false) => " (alpha)",
        (true, true) => " (beta)",
    };
    format!("{tag}{spin}")
}

/// Evaluates the chosen orbitals on grids and returns them as surfaces
/// drawn with `settings`. All of them in one batched `generate_mo_grids`
/// call: the basis functions (the expensive part, hundreds of shells on a
/// real molecule) are computed once per grid point and shared, rather
/// than once per orbital.
/// For orbitals of an analysis set: what labels and colors them.
pub(crate) struct SetContext<'a> {
    pub(crate) set: &'a LoadedOrbitalSet,
    pub(crate) results: &'a ApostResults,
    pub(crate) molecule: &'a Molecule,
}

pub(crate) fn orbital_surfaces(
    wfn: &Wavefunction,
    choices: &[OrbitalChoice],
    spacing_bohr: f64,
    padding_bohr: f64,
    settings: SurfaceSettings,
    context: Option<&SetContext>,
) -> Result<Vec<Surface>, String> {
    let mut requests: Vec<(&MolecularOrbitals, usize)> = Vec::new();
    for choice in choices {
        let orbitals = if choice.beta { wfn.beta.as_ref().ok_or("this wavefunction has no beta orbitals")? } else { &wfn.alpha };
        if choice.mo_index >= orbitals.num_orbitals() {
            return Err(format!("there is no orbital {} in this wavefunction", choice.mo_index + 1));
        }
        requests.push((orbitals, choice.mo_index));
    }
    let grids = generate_mo_grids(&wfn.basis, &requests, spacing_bohr, padding_bohr)?;
    Ok(grids
        .into_iter()
        .zip(choices)
        .map(|(grid, &choice)| {
            let set = context.map(|c| c.set.set.key());
            let source = SurfaceSource::Orbital { choice, spacing_bohr, padding_bohr, set };
            let label = match context {
                Some(c) => App::set_orbital_label(c.set, choice.beta, choice.mo_index, c.results, c.molecule),
                None => orbital_label(wfn, choice),
            };
            let mut surface = Surface::new(label, grid, settings, source);
            if let Some(c) = context {
                surface.fragment_tint = c.set.labels(choice.beta).get(choice.mo_index).and_then(Option::as_ref).map(|l| fragment_tint(c.results, c.molecule, l));
            }
            surface
        })
        .collect())
}

impl App {
    /// Evaluates every ticked MO of the active structure's wavefunction on
    /// a grid and adds each as a surface of that structure ("HOMO",
    /// "MO 12 (beta)", ...) — the same result as opening one `.cube` file
    /// per orbital, without an external `cubegen`/Multiwfn/APOST-3D step.
    pub(crate) fn generate_selected_orbitals(&mut self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let Some(wfn) = &structure.wavefunction else { return };
        if structure.selected_alpha_mos.is_empty() && structure.selected_beta_mos.is_empty() {
            self.show_warning("No orbitals selected");
            return;
        }

        let spacing = self.orbital_generation.resolve_spacing_bohr();
        let mut choices: Vec<OrbitalChoice> = structure.selected_alpha_mos.iter().map(|&mo_index| OrbitalChoice { beta: false, mo_index }).collect();
        if wfn.beta.is_some() {
            choices.extend(structure.selected_beta_mos.iter().map(|&mo_index| OrbitalChoice { beta: true, mo_index }));
        }
        choices.sort_unstable_by_key(|c| (c.beta, c.mo_index));

        let surfaces = match orbital_surfaces(wfn, &choices, spacing, self.orbital_generation.padding_bohr, self.surface_defaults, None) {
            Ok(surfaces) => surfaces,
            Err(err) => {
                self.show_warning(format!("Orbitals could not be generated: {err}"));
                return;
            }
        };
        let count = choices.len();
        self.add_surfaces(active, surfaces, true);
        self.show_status(format!("{count} orbital{} generated", if count == 1 { "" } else { "s" }));
    }

    /// Loads the active structure's orbital set `index` (0-based in
    /// `orbital_sets`) if needed; reports why it can't be used.
    pub(crate) fn load_orbital_set(&mut self, structure_index: usize, index: usize) -> bool {
        let Some(structure) = self.structures.get_mut(structure_index) else { return false };
        let Some(main) = structure.source_path.clone() else { return false };
        let Some(set) = structure.orbital_sets.get_mut(index) else { return false };
        let first_time = set.wavefunction.is_none();
        set.load(&main);
        match &set.wavefunction {
            Some(Ok(_)) => true,
            Some(Err(reason)) => {
                if first_time {
                    let reason = reason.clone();
                    self.show_warning(reason);
                }
                false
            }
            None => false,
        }
    }

    /// Generates the ticked orbitals of the active structure's current
    /// analysis set, labeled from the tables and colored by fragment.
    pub(crate) fn generate_set_orbitals(&mut self) {
        let Some(active) = self.active_structure else { return };
        let Some(index) = self.structures[active].active_orbital_set.checked_sub(1) else { return };
        if !self.load_orbital_set(active, index) {
            return;
        }
        let spacing = self.orbital_generation.resolve_spacing_bohr();
        let padding = self.orbital_generation.padding_bohr;
        let structure = &self.structures[active];
        let set = &structure.orbital_sets[index];
        let mut choices: Vec<OrbitalChoice> = set.selected_alpha.iter().map(|&mo_index| OrbitalChoice { beta: false, mo_index }).collect();
        choices.extend(set.selected_beta.iter().map(|&mo_index| OrbitalChoice { beta: true, mo_index }));
        choices.sort_unstable_by_key(|c| (c.beta, c.mo_index));
        if choices.is_empty() {
            self.show_warning("No orbitals selected");
            return;
        }
        let (Some(Ok(wfn)), Some(Ok(results))) = (&set.wavefunction, structure.apost.as_ref().map(|a| &a.results)) else { return };
        let context = SetContext { set, results, molecule: &structure.molecule };
        let surfaces = match orbital_surfaces(wfn, &choices, spacing, padding, self.surface_defaults, Some(&context)) {
            Ok(surfaces) => surfaces,
            Err(err) => {
                self.show_warning(format!("Orbitals could not be generated: {err}"));
                return;
            }
        };
        let count = surfaces.len();
        // "1 OSLO generated", "9 OSLOs (before orthogonalization) generated".
        let title = set.set.title();
        let noun = if count == 1 { title.replacen("Os", "O", 1) } else { title.to_string() };
        self.add_surfaces(active, surfaces, true);
        self.show_status(format!("{count} {noun} generated"));
    }

    /// Draws one orbital of an analysis set (from a table row): the set
    /// becomes the current one, the orbital is generated and shown.
    pub(crate) fn draw_set_orbital(&mut self, analysis: &str, beta: bool, mo_index: usize) {
        let Some(active) = self.active_structure else { return };
        let Some(position) = self.structures[active].orbital_sets.iter().position(|s| s.set.analysis == analysis && !s.set.preortho) else {
            self.show_warning("The orbital file of this analysis was not found next to the results.");
            return;
        };
        let structure = &mut self.structures[active];
        structure.active_orbital_set = position + 1;
        let set = &mut structure.orbital_sets[position];
        set.selected_alpha.clear();
        set.selected_beta.clear();
        if beta { set.selected_beta.insert(mo_index) } else { set.selected_alpha.insert(mo_index) };
        self.generate_set_orbitals();
    }
}
