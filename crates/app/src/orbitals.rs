//! Orbital generation from a wavefunction (.fchk/.molden).

use std::collections::HashSet;

use apost3dview_core::{generate_mo_grids, MolecularOrbitals};

use crate::app::App;
use crate::isosurfaces::Surface;

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
        let is_unrestricted = wfn.beta.is_some();

        // One entry per spin channel that has anything selected — for a
        // restricted file there's only ever the alpha one (and its
        // generated labels stay unsuffixed, since there's no beta
        // channel to disambiguate from).
        let mut channels: Vec<(&MolecularOrbitals, Vec<usize>, &str)> = Vec::new();
        let mut alpha_indices: Vec<usize> = structure.selected_alpha_mos.iter().copied().collect();
        alpha_indices.sort_unstable();
        channels.push((&wfn.alpha, alpha_indices, if is_unrestricted { " (alpha)" } else { "" }));
        if let Some(beta) = &wfn.beta {
            let mut beta_indices: Vec<usize> = structure.selected_beta_mos.iter().copied().collect();
            beta_indices.sort_unstable();
            channels.push((beta, beta_indices, " (beta)"));
        }

        // Every ticked orbital, across both spin channels, is evaluated in
        // one batched `generate_mo_grids` call — the basis functions
        // themselves (the expensive part, hundreds of shells on a real
        // molecule) get computed once per grid point and shared across all
        // of them, rather than once per orbital as a separate full grid pass.
        let mut requests: Vec<(&MolecularOrbitals, usize)> = Vec::new();
        let mut labels: Vec<String> = Vec::new();
        for (orbitals, mo_indices, spin_suffix) in &channels {
            let homo = orbitals.homo_index();
            let lumo = orbitals.lumo_index();
            for &mo_index in mo_indices {
                let mo_number = mo_index + 1;
                let tag = if mo_number == homo {
                    "HOMO".to_string()
                } else if mo_number == lumo {
                    "LUMO".to_string()
                } else {
                    format!("MO {mo_number}")
                };
                requests.push((*orbitals, mo_index));
                labels.push(format!("{tag}{spin_suffix}"));
            }
        }

        let surfaces = match generate_mo_grids(&wfn.basis, &requests, spacing, self.orbital_generation.padding_bohr) {
            Ok(grids) => grids.into_iter().zip(labels).map(|(grid, label)| Surface::new(label, grid, self.surface_defaults)).collect(),
            Err(err) => {
                self.show_warning(format!("Orbitals could not be generated: {err}"));
                return;
            }
        };
        let count = requests.len();
        self.add_surfaces(active, surfaces, true);
        self.show_status(format!("{count} orbital{} generated", if count == 1 { "" } else { "s" }));
    }
}
