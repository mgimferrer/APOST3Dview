//! The Surfaces area's part of a session: each surface's look and where
//! it comes from (a cube file, or an orbital or density to compute again).

use std::collections::BTreeMap;
use std::path::Path;

use apost3dview_core::parse_cube;
use serde::{Deserialize, Serialize};

use super::{check_fingerprint, color_from_array, color_to_array, file_name, fingerprint, relative_path, resolve, Fingerprint};
use crate::app::App;
use crate::isosurfaces::{Surface, SurfaceSettings, SurfaceSource};
use crate::orbitals::{density_surface, orbital_surfaces, OrbitalChoice, OrbitalNames, SetContext};
use crate::state::LoadedStructure;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(super) enum SurfaceOrigin {
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
pub(super) enum Spin {
    Alpha,
    Beta,
}

#[derive(Serialize, Deserialize)]
pub(super) struct SurfaceState {
    pub(super) label: String,
    pub(super) visible: bool,
    pub(super) isovalue: f32,
    pub(super) refinement: usize,
    pub(super) both_signs: bool,
    pub(super) positive_color: [u8; 4],
    pub(super) negative_color: [u8; 4],
    pub(super) opacity: f32,
    /// Analysis orbitals colored by fragment (default off).
    #[serde(default)]
    pub(super) by_fragment: bool,
    #[serde(flatten)]
    pub(super) origin: SurfaceOrigin,
}

/// A structure's surfaces as saved, paths relative to `base`.
pub(super) fn saved_surfaces(structure: &LoadedStructure, base: &Path) -> Vec<SurfaceState> {
    structure
        .surfaces.list
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
        .collect()
}

impl App {
    /// The structure's surfaces, in the recorded order: cubes read again,
    /// generated orbitals computed again (all of one grid in one pass).
    pub(super) fn restore_surfaces(&mut self, index: usize, states: &[SurfaceState], base: &Path, name: &str, problems: &mut Vec<String>) -> Vec<Surface> {
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
                    let natural = density_matrix.as_ref().map(|kind| structure.surfaces.densities.as_ref().and_then(|d| d.natural.iter().find(|n| &n.kind == kind)).ok_or(kind));
                    let generated = match (&structure.surfaces.wavefunction, natural) {
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
                None => match &self.structures[index].surfaces.wavefunction {
                    None => Err("no wavefunction".to_string()),
                    Some(wfn) => orbital_surfaces(wfn, &choices, f64::from_bits(spacing), f64::from_bits(padding), SurfaceSettings::default(), OrbitalNames::Scf),
                },
                Some(key) if key.starts_with("natural-") => match self.structures[index].surfaces.densities.as_ref().and_then(|d| d.by_key(key)) {
                    None => Err(format!("the file has no {} density matrix", &key["natural-".len()..])),
                    Some(set) => orbital_surfaces(&set.orbitals, &choices, f64::from_bits(spacing), f64::from_bits(padding), SurfaceSettings::default(), OrbitalNames::Natural(set)),
                },
                Some(analysis) => match self.structures[index].apost.orbital_sets.iter().position(|s| &s.set.key() == analysis) {
                    None => Err(format!("its {analysis} orbital file was not found")),
                    Some(position) if !self.load_orbital_set(index, position) => Err(format!("its {analysis} orbital file could not be used")),
                    Some(position) => {
                        let structure = &self.structures[index];
                        let set = &structure.apost.orbital_sets[position];
                        match (&set.wavefunction, structure.apost.results.as_ref().map(|a| &a.results)) {
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
