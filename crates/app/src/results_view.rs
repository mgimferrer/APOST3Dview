//! What the APOST-3D results show on the molecule, and how (M3-M5).
//!
//! One choice at a time, from the results file's `index`: the fragments
//! themselves, a value per fragment or per atom, or a pair quantity seen
//! from one fragment or atom (the focus view: one row of the matrix, the
//! others shaded by their value with the focused one). Atoms always keep
//! their element colors: fragments are shown by translucent envelopes and
//! atom values by translucent halos, in the value's color, with the values
//! printed beside them and a legend. Fragment values are read from the
//! file, never summed from atoms.

use std::collections::BTreeMap;

use apost3dview_core::{element_data, envelope_mesh, ApostResults, IndexEntry, IsosurfaceMesh, Molecule, Quantity, QuantityKind};
use apost3dview_render::{format_signed, AtomInstance, ColorScale, EnvelopeRange, IsosurfaceVertex, Legend};
use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::results::fragment_color;

/// What is shown on the molecule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "show", content = "id", rename_all = "lowercase")]
pub(crate) enum Shown {
    Nothing,
    /// The fragments, each envelope in the fragment's own color.
    Fragments,
    /// An index entry, by its `id`.
    Quantity(String),
}

/// The per-structure choices about showing APOST-3D results. Saved in
/// sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ResultsView {
    pub(crate) shown: Shown,
    /// Off: nothing drawn, the choice kept (for a quick look without it).
    pub(crate) visible: bool,
    /// The focused fragment or atom (0-based) for a pair quantity.
    pub(crate) focus: usize,
    pub(crate) labels: bool,
    /// `None`: as many as the values need (2, or 3-4 for small values).
    pub(crate) decimals: Option<usize>,
    pub(crate) label_size: f32,
    /// Bonds drawn thicker for larger bond orders (off: all equal).
    pub(crate) bond_thickness: bool,
}

impl ResultsView {
    pub(crate) fn new(results: &ApostResults) -> Self {
        Self {
            shown: if results.fragments().is_empty() { Shown::Nothing } else { Shown::Fragments },
            visible: true,
            focus: 0,
            labels: true,
            decimals: None,
            label_size: 26.0,
            bond_thickness: false,
        }
    }
}

/// How far an envelope reaches beyond the atoms' drawn spheres, and a
/// halo beyond its atom's, in angstrom.
const ENVELOPE_MARGIN: f32 = 0.5;
const HALO_MARGIN: f32 = 0.24;
/// Overall opacity of envelopes (the shader shapes it: clearer in the
/// middle, stronger at the edge).
const ENVELOPE_STRENGTH: f32 = 0.5;
/// The color of the focused fragment or atom in the focus view.
pub(crate) const FOCUS_COLOR: [f32; 3] = [0.929, 0.631, 0.0];

/// A quantity whose sign means something (charges, spin): always shown on
/// the diverging scale centered on zero, whatever the values happen to be.
fn is_signed(entry: &IndexEntry, values: &[f64]) -> bool {
    let id = entry.id.to_ascii_lowercase();
    id.contains("charges") || id.ends_with(".spin") || id.contains(".s2") || values.iter().any(|v| *v < 0.0)
}

/// The automatic scale for some values: diverging up to the largest
/// magnitude for signed quantities, sequential from zero otherwise.
fn automatic_scale(entry: &IndexEntry, values: &[f64]) -> ColorScale {
    let max_abs = values.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if is_signed(entry, values) {
        ColorScale::Diverging { max_abs: if max_abs > 0.0 { max_abs } else { 1.0 } }
    } else {
        let max = values.iter().copied().fold(0.0f64, f64::max);
        ColorScale::Sequential { min: 0.0, max: if max > 0.0 { max } else { 1.0 } }
    }
}

/// Decimals enough to show two significant digits of the largest value.
fn automatic_decimals(values: &[f64]) -> usize {
    let max_abs = values.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    match max_abs {
        m if m >= 0.1 || m == 0.0 => 2,
        m if m >= 0.01 => 3,
        _ => 4,
    }
}

fn name_of_fragment(results: &ApostResults, molecule: &Molecule, f: usize) -> String {
    let atoms = &results.fragments()[f].atoms;
    let formula = apost3dview_core::hill_formula(atoms.iter().filter_map(|&a| molecule.atomic_numbers.get(a - 1).copied()));
    format!("{} {formula}", results.fragments()[f].id)
}

fn name_of_atom(molecule: &Molecule, a: usize) -> String {
    format!("{}{}", element_data(molecule.atomic_numbers[a]).symbol, a + 1)
}

/// Where a value label sits: beside a fragment or on an atom.
#[derive(Debug, Clone, Copy)]
pub(crate) enum LabelAnchor {
    Fragment(usize),
    Atom(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct ValueLabel {
    pub(crate) text: String,
    pub(crate) anchor: LabelAnchor,
    pub(crate) color: [f32; 3],
}

/// Everything drawn for the current choice.
#[derive(Default)]
pub(crate) struct ResultsScene {
    /// (fragment, color) for each envelope to draw.
    pub(crate) envelopes: Vec<(usize, [f32; 3])>,
    /// (atom, color) for each halo.
    pub(crate) halos: Vec<(usize, [f32; 3])>,
    pub(crate) labels: Vec<ValueLabel>,
    pub(crate) legend: Option<Legend>,
    /// Shown in the window: what the colors and values are.
    pub(crate) caption: Option<String>,
}

/// The index entry a view shows, if it is a quantity this file has.
pub(crate) fn shown_entry<'a>(results: &'a ApostResults, view: &ResultsView) -> Option<&'a IndexEntry> {
    match &view.shown {
        Shown::Quantity(id) => results.index.iter().find(|e| &e.id == id),
        _ => None,
    }
}

/// What to draw for `view`. `fixed` holds the user's fixed color ranges,
/// by quantity id (shared by every structure, so figures compare).
pub(crate) fn build_scene(results: &ApostResults, molecule: &Molecule, view: &ResultsView, fixed: &BTreeMap<String, ColorScale>) -> ResultsScene {
    let mut scene = ResultsScene::default();
    if !view.visible {
        return scene;
    }
    let n_fragments = results.fragments().len();
    match &view.shown {
        Shown::Nothing => {}
        Shown::Fragments => {
            scene.envelopes = (0..n_fragments).map(|f| (f, rgb(fragment_color(f)))).collect();
        }
        Shown::Quantity(_) => {
            let Some(entry) = shown_entry(results, view) else { return scene };
            let Ok(quantity) = results.quantity(entry) else { return scene };
            let on_fragments = entry.kind.on_fragments();
            let name = |i: usize| if on_fragments { name_of_fragment(results, molecule, i) } else { name_of_atom(molecule, i) };
            let anchor = |i: usize| if on_fragments { LabelAnchor::Fragment(i) } else { LabelAnchor::Atom(i) };
            let (values, focus): (Vec<f64>, Option<usize>) = match &quantity {
                Quantity::Vector(v) => (v.iter().map(|x| x.unwrap_or(0.0)).collect(), None),
                Quantity::Matrix(m) => {
                    let focus = view.focus.min(m.values.len().saturating_sub(1));
                    ((0..m.values.len()).map(|j| m.get(focus, j).unwrap_or(0.0)).collect(), Some(focus))
                }
            };
            // The focused one's own (diagonal) value doesn't set the scale.
            let others: Vec<f64> = values.iter().enumerate().filter(|(i, _)| Some(*i) != focus).map(|(_, v)| *v).collect();
            let scale = fixed.get(&entry.id).copied().unwrap_or_else(|| automatic_scale(entry, &others));
            let decimals = view.decimals.unwrap_or_else(|| automatic_decimals(&others));
            let signed = matches!(scale, ColorScale::Diverging { .. });
            let format = |v: f64| if signed { format_signed(v, decimals) } else { format!("{v:.decimals$}") };

            let items: Vec<(usize, [f32; 3])> = values
                .iter()
                .enumerate()
                .map(|(i, &v)| (i, if Some(i) == focus { FOCUS_COLOR } else { scale.color(v) }))
                .collect();
            if on_fragments {
                scene.envelopes = items;
            } else {
                scene.halos = items;
            }
            if view.labels {
                for (i, &v) in values.iter().enumerate() {
                    let (text, color) = if Some(i) == focus { (name(i), FOCUS_COLOR) } else { (format(v), [0.0; 3]) };
                    scene.labels.push(ValueLabel { text, anchor: anchor(i), color });
                }
            }
            let title = match (&quantity, focus) {
                (Quantity::Matrix(m), Some(f)) => {
                    let what = m.off_diagonal.clone().unwrap_or_else(|| entry.title.clone());
                    format!("{} with {} ({})", capitalize(&what), name(f), results.input.as_ref().and_then(|i| i.atomic_definition.as_ref()).and_then(|d| d.keyword.clone()).unwrap_or_default())
                }
                _ => entry.title.clone(),
            };
            scene.caption = Some(title.clone());
            scene.legend = Some(Legend { title, scale, decimals: decimals.min(3) });
        }
    }
    scene
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn rgb(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| v as f32 / 255.0)
}

/// The drawn radius of atom `i`, as the renderer draws it.
pub(crate) fn drawn_radius(molecule: &Molecule, i: usize, atom_scale: f32) -> f32 {
    element_data(molecule.atomic_numbers[i]).vdw_radius * atom_scale
}

/// One envelope mesh per fragment, around its visible atoms. Kept until
/// the atom size or the hidden atoms change.
pub(crate) fn fragment_envelopes(results: &ApostResults, molecule: &Molecule, atom_scale: f32, hidden: &std::collections::HashSet<usize>) -> Vec<IsosurfaceMesh> {
    results
        .fragments()
        .iter()
        .map(|fragment| {
            let atoms: Vec<usize> = fragment.atoms.iter().map(|a| a - 1).filter(|a| *a < molecule.positions.len() && !hidden.contains(a)).collect();
            let centers: Vec<Vec3> = atoms.iter().map(|&a| molecule.positions[a]).collect();
            let radii: Vec<f32> = atoms.iter().map(|&a| drawn_radius(molecule, a, atom_scale) + ENVELOPE_MARGIN).collect();
            envelope_mesh(&centers, &radii)
        })
        .collect()
}

/// Envelope vertices and ranges for `scene`, from the cached meshes.
pub(crate) fn envelope_vertices(scene: &ResultsScene, meshes: &[IsosurfaceMesh]) -> (Vec<IsosurfaceVertex>, Vec<EnvelopeRange>) {
    let mut vertices = Vec::new();
    let mut ranges = Vec::new();
    for &(f, color) in &scene.envelopes {
        let Some(mesh) = meshes.get(f) else { continue };
        if mesh.positions.is_empty() {
            continue;
        }
        let first = vertices.len() as u32;
        apost3dview_render::push_isosurface_vertices(&mut vertices, mesh, color, ENVELOPE_STRENGTH);
        let center = mesh.positions.iter().copied().sum::<Vec3>() / mesh.positions.len() as f32;
        ranges.push(EnvelopeRange { first_vertex: first, vertex_count: vertices.len() as u32 - first, center: center.to_array() });
    }
    (vertices, ranges)
}

/// Halo instances for `scene` (visible atoms only).
pub(crate) fn halo_instances(scene: &ResultsScene, molecule: &Molecule, atom_scale: f32, hidden: &std::collections::HashSet<usize>) -> Vec<AtomInstance> {
    scene
        .halos
        .iter()
        .filter(|(a, _)| *a < molecule.positions.len() && !hidden.contains(a))
        .map(|&(a, color)| {
            // The shader multiplies by the atom scale: undo it for the margin.
            let radius = drawn_radius(molecule, a, atom_scale) + HALO_MARGIN;
            AtomInstance { center: molecule.positions[a].to_array(), vdw_radius: radius / atom_scale.max(1e-3), color, _padding: 0.0 }
        })
        .collect()
}

/// Bond thickness from APOST-3D's atomic bond orders (one scale per
/// bond of `molecule`), when the run has them.
pub(crate) fn bond_scales(results: &ApostResults, molecule: &Molecule) -> Option<Vec<f32>> {
    let entry = results.index.iter().find(|e| e.id == "bond_orders.atoms.matrix")?;
    let Ok(Quantity::Matrix(m)) = results.quantity(entry) else { return None };
    Some(
        molecule
            .bonds
            .iter()
            .map(|b| {
                let order = m.get(b.atom_a, b.atom_b).unwrap_or(1.0) as f32;
                (0.25 + 0.75 * order).clamp(0.2, 2.2)
            })
            .collect(),
    )
}

/// Where a value label goes. A fragment's label sits outside it, pushed
/// away from the molecule's center (on the camera side for the central
/// fragment); an atom's just in front of its halo.
pub(crate) fn label_position(anchor: LabelAnchor, results: &ApostResults, molecule: &Molecule, atom_scale: f32, eye: Vec3) -> Option<Vec3> {
    match anchor {
        LabelAnchor::Atom(a) => {
            let p = *molecule.positions.get(a)?;
            Some(p + (eye - p).normalize_or_zero() * (drawn_radius(molecule, a, atom_scale) + HALO_MARGIN + 0.05))
        }
        LabelAnchor::Fragment(f) => {
            let atoms: Vec<Vec3> = results.fragments().get(f)?.atoms.iter().filter_map(|a| molecule.positions.get(a - 1).copied()).collect();
            if atoms.is_empty() {
                return None;
            }
            let (center, _) = molecule.bounding_sphere();
            let centroid = atoms.iter().copied().sum::<Vec3>() / atoms.len() as f32;
            let extent = atoms.iter().map(|p| p.distance(centroid)).fold(0.0f32, f32::max) + ENVELOPE_MARGIN + 0.45;
            let outward = centroid - center;
            if outward.length() < 0.5 {
                Some(centroid + (eye - centroid).normalize_or_zero() * extent)
            } else {
                Some(centroid + outward.normalize() * extent)
            }
        }
    }
}

/// The quantities a file offers for the selector, grouped: fragment values,
/// fragment pairs, atom values, atom pairs (fragments first: they are what
/// fragment analyses are about).
pub(crate) fn grouped_entries(results: &ApostResults) -> Vec<(&'static str, Vec<&IndexEntry>)> {
    [
        ("Fragment values", QuantityKind::FragmentVector),
        ("Between fragments", QuantityKind::FragmentMatrix),
        ("Atom values", QuantityKind::AtomVector),
        ("Between atoms", QuantityKind::AtomMatrix),
    ]
    .into_iter()
    .map(|(group, kind)| (group, results.entries(kind).collect::<Vec<_>>()))
    .filter(|(_, entries)| !entries.is_empty())
    .collect()
}

/// Whether the shown quantity is a pair quantity on fragments (`Some(true)`)
/// or atoms (`Some(false)`): clicks then choose the focus.
pub(crate) fn focus_target(results: &ApostResults, view: &ResultsView) -> Option<bool> {
    let entry = shown_entry(results, view)?;
    match entry.kind {
        QuantityKind::FragmentMatrix => Some(true),
        QuantityKind::AtomMatrix => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_follow_the_size_of_the_values() {
        assert_eq!(automatic_decimals(&[0.52, -0.1]), 2);
        assert_eq!(automatic_decimals(&[0.018, -0.018]), 3);
        assert_eq!(automatic_decimals(&[0.004]), 4);
    }
}
