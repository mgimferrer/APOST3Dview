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

use apost3dview_core::{envelope_mesh, ApostResults, IndexEntry, IsosurfaceMesh, MatrixObject, Molecule, Quantity, QuantityKind};
use apost3dview_render::{format_signed, AtomInstance, ColorScale, EnvelopeRange, Half, IsosurfaceVertex, Legend};
use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::names::{atom_name, fragment_name};
use crate::util::capitalize;
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
    /// Signed values colored by the logarithm of their magnitude (for
    /// values spanning orders of magnitude, such as IQA pair energies).
    #[serde(default)]
    pub(crate) log_scale: bool,
    /// The unit energies are shown in (APOST-3D writes them in au).
    #[serde(default)]
    pub(crate) energy_unit: EnergyUnit,
    /// Each value's atom or fragment named above it ("2 CN" over "-0.15");
    /// off: the values alone. For atoms, the name follows the Molecule
    /// window's Labels setting ("C12" when that is None).
    #[serde(default = "names_by_default")]
    pub(crate) names: bool,
    /// For a matrix: its diagonal on every atom or fragment (local spin,
    /// localization index, intra (self) energy) instead of the pairs seen
    /// from the focus (Martí, 2026-10-19). Ignored for a matrix whose
    /// diagonal means nothing (`diagonal_meaning`).
    #[serde(default)]
    pub(crate) diagonal: bool,
}

/// How energies are shown. kcal/mol by default: what chemists read
/// interaction energies in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EnergyUnit {
    Hartree,
    #[default]
    KcalPerMol,
}

/// Hartree to kcal/mol (CODATA 2018).
pub(crate) const KCAL_PER_HARTREE: f64 = 627.509_474_063_1;

impl EnergyUnit {
    pub(crate) fn name(self) -> &'static str {
        match self {
            EnergyUnit::Hartree => "au",
            EnergyUnit::KcalPerMol => "kcal/mol",
        }
    }

    /// What an energy in hartree is multiplied by to show it.
    pub(crate) fn factor(self) -> f64 {
        match self {
            EnergyUnit::Hartree => 1.0,
            EnergyUnit::KcalPerMol => KCAL_PER_HARTREE,
        }
    }

    /// Decimals every energy is shown with, whatever its size (Martí,
    /// 2026-10-22): four in au, one in kcal/mol.
    pub(crate) fn decimals(self) -> usize {
        match self {
            EnergyUnit::Hartree => 4,
            EnergyUnit::KcalPerMol => 1,
        }
    }
}

/// Whether an index entry is an energy (APOST-3D writes them in "au").
pub(crate) fn is_energy(entry: &IndexEntry) -> bool {
    entry.unit.as_deref() == Some("au")
}

/// The factor `entry`'s values are shown with in `view` (1 unless it is
/// an energy shown in kcal/mol), and the unit to name, if any.
pub(crate) fn display_unit(entry: &IndexEntry, view: &ResultsView) -> (f64, Option<&'static str>) {
    if is_energy(entry) { (view.energy_unit.factor(), Some(view.energy_unit.name())) } else { (1.0, None) }
}

/// A quantity's values multiplied by `factor` (energies shown in kcal/mol).
pub(crate) fn scaled_quantity(quantity: Quantity, factor: f64) -> Quantity {
    match quantity {
        Quantity::Matrix(m) if factor != 1.0 => Quantity::Matrix(scaled_matrix(&m, factor)),
        Quantity::Vector(v) if factor != 1.0 => Quantity::Vector(v.into_iter().map(|x| x.map(|x| x * factor)).collect()),
        other => other,
    }
}

/// A matrix with its values (and total) multiplied by `factor`.
pub(crate) fn scaled_matrix(m: &MatrixObject, factor: f64) -> MatrixObject {
    let mut m = m.clone();
    if factor != 1.0 {
        m.values.iter_mut().flatten().flatten().for_each(|v| *v *= factor);
        m.total = m.total.map(|t| t * factor);
    }
    m
}

fn names_by_default() -> bool {
    true
}

impl ResultsView {
    /// Results open showing nothing on the molecule (Martí, 2026-10-19):
    /// the molecule as it is, the APOST-3D window to choose from.
    pub(crate) fn new(_results: &ApostResults) -> Self {
        Self {
            shown: Shown::Nothing,
            visible: true,
            focus: 0,
            labels: true,
            decimals: None,
            label_size: 34.0,
            bond_thickness: false,
            log_scale: false,
            energy_unit: EnergyUnit::default(),
            diagonal: false,
            names: true,
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

/// A quantity whose sign means something (charges, spin, oxidation
/// states): always shown on the diverging scale centered on zero, whatever
/// the values happen to be. APOST-3D says so (`signed`, 0.4); for older
/// files, guessed from the id.
fn is_signed(entry: &IndexEntry, values: &[f64]) -> bool {
    if let Some(signed) = entry.signed {
        return signed;
    }
    let id = entry.id.to_ascii_lowercase();
    id.contains("charges") || id.ends_with(".spin") || id.contains(".s2") || values.iter().any(|v| *v < -1e-3)
}

/// The overall reliability index R (%) of an EOS or GEOS run, from its
/// key number (`<analysis>.reliability_percent`).
fn reliability(results: &ApostResults, analysis: &str) -> Option<f64> {
    let entry = results.entry(&format!("{analysis}.reliability_percent"))?;
    match results.quantity(entry).ok()? {
        Quantity::Scalar(value) => value,
        _ => None,
    }
}

/// The automatic scale for some values: diverging up to the largest
/// magnitude for signed quantities, sequential from zero otherwise. When
/// the values shown all have one sign (a local spin, the interactions of
/// one atom), the legend shows only that half: the colors stay those of
/// the full scale (blue positive, red negative), so figures still compare.
fn automatic_scale(entry: &IndexEntry, values: &[f64]) -> ColorScale {
    let max_abs = values.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if is_signed(entry, values) {
        let half = if values.iter().all(|&v| v >= 0.0) && max_abs > 0.0 {
            Half::Positive
        } else if values.iter().all(|&v| v <= 0.0) && max_abs > 0.0 {
            Half::Negative
        } else {
            Half::Both
        };
        ColorScale::Diverging { max_abs: if max_abs > 0.0 { max_abs } else { 1.0 }, log: false, half }
    } else {
        let max = values.iter().copied().fold(0.0f64, f64::max);
        ColorScale::Sequential { min: 0.0, max: if max > 0.0 { max } else { 1.0 } }
    }
}

/// Decimals enough to show two significant digits of the largest value;
/// energies always with their unit's (`EnergyUnit::decimals`); none when
/// every value is a whole number (oxidation states, electron counts: "+2",
/// not "+2.00").
fn automatic_decimals(values: &[f64], energy: Option<EnergyUnit>) -> usize {
    if let Some(unit) = energy {
        return unit.decimals();
    }
    if !values.is_empty() && values.iter().all(|v| (v - v.round()).abs() < 1e-9) {
        return 0;
    }
    let max_abs = values.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    match max_abs {
        m if m >= 0.1 || m == 0.0 => 2,
        m if m >= 0.01 => 3,
        _ => 4,
    }
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
    /// Notes for the window about what is shown (IQA: the focus's own
    /// self-energy, what "≈" marks).
    pub(crate) notes: Vec<String>,
}

/// An ENPART term matrix id, "enpart.<term>.atoms|fragments", as
/// (term, on fragments).
pub(crate) fn enpart_term(id: &str) -> Option<(&str, bool)> {
    let rest = id.strip_prefix("enpart.")?;
    let (term, level) = rest.rsplit_once('.')?;
    if !apost3dview_core::ENPART_TERMS.contains(&term) {
        return None;
    }
    match level {
        "atoms" => Some((term, false)),
        "fragments" => Some((term, true)),
        _ => None,
    }
}

/// A term's name for the term selector.
pub(crate) fn enpart_term_name(term: &str) -> &'static str {
    match term {
        "total" => "Total (IQA)",
        "classical_interaction" => "Classical (electrostatic)",
        "xc" => "Exchange-correlation",
        "exchange" => "Exchange (part of exchange-correlation)",
        "correlation" => "Correlation (part of exchange-correlation)",
        "coulomb" => "Coulomb",
        "exact_exchange_share" => "Exact-exchange share",
        "xc_dft" => "DFT exchange-correlation",
        "electron_nuclear" => "Electron-nuclear attraction",
        "nuclear_repulsion" => "Nuclear repulsion",
        "kinetic" => "Kinetic (intra only)",
        _ => "Other",
    }
}

/// How an IQA matrix value is named, so it always says whether it lies
/// between two fragments (atoms) or within one (Martí, 2026-10-19):
/// `diagonal` for the matrix diagonal (within: "intra-atomic (self)
/// energy"), otherwise a pair (between: "inter-fragment interaction
/// energy", "inter-atomic exchange-correlation term").
pub(crate) fn iqa_meaning(term: &str, on_fragments: bool, diagonal: bool) -> String {
    let level = if on_fragments { "fragment" } else { "atomic" };
    let prefix = if diagonal { format!("intra-{level}") } else { format!("inter-{level}") };
    let part = match term {
        "total" if diagonal => return format!("{prefix} (self) energy"),
        "total" => return format!("{prefix} interaction energy"),
        "classical_interaction" => return format!("{prefix} classical (electrostatic) interaction"),
        "xc" => "exchange-correlation",
        "exchange" => "exchange",
        "correlation" => "correlation",
        "coulomb" => "Coulomb",
        "exact_exchange_share" => "exact-exchange",
        "xc_dft" => "DFT exchange-correlation",
        "electron_nuclear" => "electron-nuclear",
        "nuclear_repulsion" => "nuclear repulsion",
        "kinetic" => "kinetic",
        _ => "energy",
    };
    format!("{prefix} {part} term")
}

/// What a matrix's diagonal and off-diagonal values are, always saying
/// intra (within one atom or fragment) or inter (between two), as Martí
/// asked for every matrix (2026-10-19): the IQA wording for energy terms
/// (`iqa_meaning`); "intra-atomic S² (local spin)" / "inter-atomic S²
/// (spin coupling)" for the S² decomposition; otherwise the file's own
/// noun with intra-/inter- ("inter-fragment bond order"). `None` where the
/// file says a part has no meaning ("none").
pub(crate) fn matrix_meanings(entry: &IndexEntry, matrix: &MatrixObject) -> (Option<String>, Option<String>) {
    let diagonal = matrix.diagonal.clone().filter(|d| d != "none");
    if let Some((term, on_fragments)) = enpart_term(&entry.id) {
        return (diagonal.map(|_| iqa_meaning(term, on_fragments, true)), Some(iqa_meaning(term, on_fragments, false)));
    }
    let level = if entry.kind.on_fragments() { "fragment" } else { "atomic" };
    let s2 = entry.id.ends_with(".s2");
    let name = |within: bool, noun: String| {
        let prefix = if within { "intra" } else { "inter" };
        if s2 {
            format!("{prefix}-{level} S\u{b2} ({noun})")
        } else {
            format!("{prefix}-{level} {noun}")
        }
    };
    (diagonal.map(|d| name(true, d)), matrix.off_diagonal.clone().map(|o| name(false, o)))
}

/// What a matrix entry's diagonal holds ("local spin", "intra-atomic
/// (self) energy"), if anything: `None` for a vector, or a matrix whose
/// diagonal means nothing (the classical interaction).
pub(crate) fn diagonal_meaning(results: &ApostResults, entry: &IndexEntry) -> Option<String> {
    match results.quantity(entry).ok()? {
        Quantity::Matrix(m) => matrix_meanings(entry, &m).0,
        _ => None,
    }
}

/// Whether `view` shows its matrix's diagonal (on every atom or fragment)
/// rather than the pairs seen from the focus.
pub(crate) fn shows_diagonal(results: &ApostResults, view: &ResultsView) -> bool {
    view.diagonal && shown_entry(results, view).is_some_and(|e| diagonal_meaning(results, e).is_some())
}

/// Under which name a fixed color range is kept: per quantity, and apart
/// for a matrix's diagonal, whose values have another scale.
pub(crate) fn range_key(entry: &IndexEntry, diagonal: bool) -> String {
    if diagonal {
        format!("{}/diagonal", entry.id)
    } else {
        entry.id.clone()
    }
}

/// The index entry a view shows, if it is a quantity this file has.
pub(crate) fn shown_entry<'a>(results: &'a ApostResults, view: &ResultsView) -> Option<&'a IndexEntry> {
    match &view.shown {
        Shown::Quantity(id) => results.entry(id),
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
            scene.envelopes = (0..n_fragments).map(|f| (f, fragment_rgb(f))).collect();
        }
        Shown::Quantity(_) => {
            let Some(entry) = shown_entry(results, view) else { return scene };
            let Ok(quantity) = results.quantity(entry) else { return scene };
            let on_fragments = entry.kind.on_fragments();
            let name = |i: usize| if on_fragments { fragment_name(&results.fragments()[i], &molecule.atomic_numbers) } else { atom_name(&molecule.atomic_numbers, i) };
            let anchor = |i: usize| if on_fragments { LabelAnchor::Fragment(i) } else { LabelAnchor::Atom(i) };
            // Energies in the chosen unit; fixed ranges are kept in au.
            let (factor, unit) = display_unit(entry, view);
            let quantity = scaled_quantity(quantity, factor);
            let diagonal = shows_diagonal(results, view);
            let (values, focus): (Vec<f64>, Option<usize>) = match &quantity {
                Quantity::Scalar(_) | Quantity::Table(_) => return scene,
                Quantity::Vector(v) => (v.iter().map(|x| x.unwrap_or(0.0)).collect(), None),
                Quantity::Matrix(m) if diagonal => ((0..m.values.len()).map(|i| m.get(i, i).unwrap_or(0.0)).collect(), None),
                Quantity::Matrix(m) => {
                    let focus = view.focus.min(m.values.len().saturating_sub(1));
                    ((0..m.values.len()).map(|j| m.get(focus, j).unwrap_or(0.0)).collect(), Some(focus))
                }
            };
            // The focused one's own (diagonal) value doesn't set the scale.
            let others: Vec<f64> = values.iter().enumerate().filter(|(i, _)| Some(*i) != focus).map(|(_, v)| *v).collect();
            let mut scale = fixed.get(&range_key(entry, diagonal)).map(|s| s.scaled(factor)).unwrap_or_else(|| automatic_scale(entry, &others));
            if let ColorScale::Diverging { log, .. } = &mut scale {
                *log = view.log_scale;
            }
            let energy = unit.map(|_| view.energy_unit);
            let decimals = view.decimals.unwrap_or_else(|| automatic_decimals(&others, energy));
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
            // IQA: the focus's own term (intra, its self-energy) in the
            // window, and the values of its pairs from the multipolar
            // expansion marked "≈" (their exchange-correlation is
            // approximate).
            let enpart = enpart_term(&entry.id).and_then(|_| results.enpart());
            if let (Quantity::Matrix(m), Some(f), Some(_)) = (&quantity, focus, &enpart)
                && let (Some(noun), Some(e)) = (matrix_meanings(entry, m).0, m.get(f, f))
            {
                let u = view.energy_unit;
                scene.notes.push(format!("{} of {}: {} {}", capitalize(&noun), name(f), format_signed(e, u.decimals()), u.name()));
            }
            let fragments = results.fragments();
            let approximate = |i: usize| match (&enpart, focus) {
                (Some(enpart), Some(f)) if !on_fragments => i != f && enpart.is_multipolar(f, i),
                (Some(enpart), Some(f)) => i != f && fragments.get(f).zip(fragments.get(i)).is_some_and(|(a, b)| enpart.has_multipolar_pair(a, b)),
                _ => false,
            };
            if (0..values.len()).any(approximate) {
                scene.notes.push(if on_fragments {
                    "\u{2248}: includes atom pairs from the multipolar expansion (approximate exchange-correlation).".to_string()
                } else {
                    "\u{2248}: pair from the multipolar expansion (approximate exchange-correlation).".to_string()
                });
            }
            if view.labels {
                for (i, &v) in values.iter().enumerate() {
                    let (text, color) = if Some(i) == focus {
                        (name(i), FOCUS_COLOR)
                    } else if approximate(i) {
                        (format!("\u{2248}{}", format(v)), [0.0; 3])
                    } else {
                        (format(v), [0.0; 3])
                    };
                    scene.labels.push(ValueLabel { text, anchor: anchor(i), color });
                }
            }
            // As in the Show menu, plus the focus for pairs: "Inter-atomic
            // bond order with O1 (new bond orders, TFVC)".
            let title = match (&quantity, focus) {
                (Quantity::Matrix(m), Some(f)) => {
                    let what = matrix_meanings(entry, m).1.unwrap_or_else(|| entry.title.clone());
                    format!("{} with {} {}", capitalize(&what), name(f), part_source(results, entry, false)).trim_end().to_string()
                }
                (Quantity::Matrix(_), None) => choice_label(results, entry, true),
                _ => entry.title.clone(),
            };
            let title = match unit {
                Some(unit) => format!("{title}, {unit}"),
                None => title,
            };
            // Oxidation states with the reliability index R of their
            // assignment, which says how clear-cut they are.
            let title = match entry.id.strip_suffix(".fragments.oxidation_states").and_then(|analysis| reliability(results, analysis)) {
                Some(r) => format!("{title}, R = {r:.1}%"),
                None => title,
            };
            let title = if matches!(scale, ColorScale::Diverging { log: true, .. }) { format!("{title}, log scale") } else { title };
            scene.caption = Some(title.clone());
            // Energies keep their decimals (four in au) at the bar's ends too.
            let legend_decimals = if energy.is_some() { decimals } else { decimals.min(3) };
            scene.legend = Some(Legend { title, scale, decimals: legend_decimals });
        }
    }
    scene
}

/// A fragment's palette color as sRGB in [0, 1].
pub(crate) fn fragment_rgb(position: usize) -> [f32; 3] {
    fragment_color(position).map(|v| v as f32 / 255.0)
}

/// The drawn radius of atom `i`, as the renderer draws it.
pub(crate) fn drawn_radius(molecule: &Molecule, i: usize, atom_scale: f32) -> f32 {
    crate::util::drawn_radius(molecule.atomic_numbers[i], atom_scale)
}

/// One envelope mesh per fragment, around its visible atoms. Kept until
/// the atom size or the hidden atoms change.
pub(crate) fn fragment_envelopes(results: &ApostResults, molecule: &Molecule, atom_scale: f32, hidden: &std::collections::HashSet<usize>) -> Vec<IsosurfaceMesh> {
    let groups: Vec<Vec<usize>> = results.fragments().iter().map(|f| f.atoms.iter().map(|a| a - 1).collect()).collect();
    atom_group_envelopes(&groups, molecule, atom_scale, hidden)
}

/// One envelope per group of atoms (0-based).
pub(crate) fn atom_group_envelopes(groups: &[Vec<usize>], molecule: &Molecule, atom_scale: f32, hidden: &std::collections::HashSet<usize>) -> Vec<IsosurfaceMesh> {
    groups
        .iter()
        .map(|group| {
            let atoms: Vec<usize> = group.iter().copied().filter(|a| *a < molecule.positions.len() && !hidden.contains(a)).collect();
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
    let entry = results.entry("bond_orders.atoms.matrix")?;
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

/// Quantities the Show menu leaves out unless "Also list other values" is
/// ticked (Martí, 2026-10-19): what follows from a quantity already shown
/// (electron populations and EOS/OSLO electron counts from the charges and
/// oxidation states, valences from the bond orders), overlap populations,
/// and GEOS paired electrons (the unpaired ones stay). They remain in the
/// text report and in the tables.
pub(crate) fn is_secondary(entry: &IndexEntry) -> bool {
    let parts: Vec<&str> = entry.id.split('.').collect();
    match parts.as_slice() {
        ["bond_orders", "overlap_populations", ..] => true,
        ["bond_orders", _, last] => last.starts_with("valence_"),
        ["populations", .., "electrons"] => true,
        ["eos" | "oslo", "fragments", last] => last.starts_with("electrons"),
        ["geos", "fragments", "electrons_paired"] => true,
        _ => false,
    }
}

/// One choice in the Show menu: a quantity, and for a matrix whether its
/// diagonal (a value on each atom or fragment) or its pairs.
pub(crate) struct Choice<'a> {
    pub(crate) entry: &'a IndexEntry,
    pub(crate) diagonal: bool,
    pub(crate) label: String,
}

/// How a quantity is named in the Show menu: a matrix by what its
/// diagonal or its pairs hold, always saying intra or inter
/// ("Intra-atomic S² (local spin) (TFVC)", "Inter-fragment bond order
/// (TFVC)"); anything else by its title.
pub(crate) fn choice_label(results: &ApostResults, entry: &IndexEntry, diagonal: bool) -> String {
    match part_meaning(results, entry, diagonal) {
        Some(meaning) => format!("{} {}", capitalize(&meaning), part_source(results, entry, diagonal)),
        None => entry.title.clone(),
    }
}

/// What a matrix's diagonal (or its pairs) holds, as `matrix_meanings`.
fn part_meaning(results: &ApostResults, entry: &IndexEntry, diagonal: bool) -> Option<String> {
    match results.quantity(entry).ok()? {
        Quantity::Matrix(m) => {
            let (d, o) = matrix_meanings(entry, &m);
            if diagonal { d } else { o }
        }
        _ => None,
    }
}

/// The parenthesis after a matrix part's name: the atomic definition
/// ("(TFVC)"), and the matrix it comes from when another matrix's part
/// has the same name ("(exchange-only bond orders, TFVC)" against
/// "(improved bond orders, TFVC)").
pub(crate) fn part_source(results: &ApostResults, entry: &IndexEntry, diagonal: bool) -> String {
    let title = entry.title.as_str();
    let (name, scheme) = match title.rfind(" (").filter(|_| title.ends_with(')')) {
        Some(i) => (&title[..i], &title[i + 2..title.len() - 1]),
        None => (title, ""),
    };
    let meaning = part_meaning(results, entry, diagonal);
    let shared = results.entries(entry.kind).any(|other| other.id != entry.id && part_meaning(results, other, diagonal) == meaning);
    let source = shared.then(|| {
        // "Fragment bond orders" -> "bond orders"; acronyms stay ("DI").
        let name = name.strip_prefix("Fragment ").unwrap_or(name);
        let mut chars = name.chars();
        match (chars.next(), chars.next()) {
            (Some(first), Some(second)) if second.is_lowercase() => format!("{}{}", first.to_lowercase(), &name[first.len_utf8()..]),
            _ => name.to_string(),
        }
    });
    match (source, scheme) {
        (Some(source), "") => format!("({source})"),
        (Some(source), scheme) => format!("({source}, {scheme})"),
        (None, "") => String::new(),
        (None, scheme) => format!("({scheme})"),
    }
}

/// The choices a file offers for the Show menu, grouped: fragment values,
/// fragment pairs, atom values, atom pairs (fragments first: they are what
/// fragment analyses are about), then the energy decomposition. A
/// matrix's diagonal is a value on each fragment or atom, so it is listed
/// with those values (intra); its pairs under "Between" (inter) — Martí,
/// 2026-10-19. Values in another atomic definition than the run's own,
/// printed only for comparison, and the secondary ones (`is_secondary`)
/// are left out unless `include_other`.
pub(crate) fn grouped_entries(results: &ApostResults, include_other: bool) -> Vec<(&'static str, Vec<Choice<'_>>)> {
    // The energy decomposition has a group of its own: the IQA matrices
    // (their other terms through the term selector).
    let is_enpart = |e: &IndexEntry| e.analysis.as_deref() == Some("enpart");
    let listed = |e: &&IndexEntry| (include_other || !(e.is_comparison() || is_secondary(e))) && !is_enpart(e);
    let values = |kind| results.entries(kind).filter(listed).map(|entry| Choice { entry, diagonal: false, label: entry.title.clone() });
    let diagonals = |kind| {
        results.entries(kind).filter(listed).filter(|e| diagonal_meaning(results, e).is_some()).map(|entry| Choice {
            entry,
            diagonal: true,
            label: choice_label(results, entry, true),
        })
    };
    let pairs = |kind| results.entries(kind).filter(listed).map(|entry| Choice { entry, diagonal: false, label: choice_label(results, entry, false) });
    let mut groups: Vec<(&'static str, Vec<Choice>)> = vec![
        ("Fragment values", values(QuantityKind::FragmentVector).chain(diagonals(QuantityKind::FragmentMatrix)).collect()),
        ("Between fragments", pairs(QuantityKind::FragmentMatrix).collect()),
        ("Atom values", values(QuantityKind::AtomVector).chain(diagonals(QuantityKind::AtomMatrix)).collect()),
        ("Between atoms", pairs(QuantityKind::AtomMatrix).collect()),
    ];
    // Atomic energies are listed as numbers in the window too (the IQA
    // section's tables); here the interactions and, on request, the
    // intra (self) energies.
    let iqa: Vec<Choice> = ["enpart.total.fragments", "enpart.total.atoms"]
        .iter()
        .filter_map(|id| results.entry(id))
        .flat_map(|entry| [false, true].map(|diagonal| Choice { entry, diagonal, label: choice_label(results, entry, diagonal) }))
        .collect();
    groups.push(("Energy decomposition (IQA)", iqa));
    groups.retain(|(_, choices)| !choices.is_empty());
    groups
}

/// Whether the shown quantity is a pair quantity on fragments (`Some(true)`)
/// or atoms (`Some(false)`): clicks then choose the focus.
pub(crate) fn focus_target(results: &ApostResults, view: &ResultsView) -> Option<bool> {
    let entry = shown_entry(results, view)?;
    if shows_diagonal(results, view) {
        return None;
    }
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
    fn iqa_values_say_inter_or_intra() {
        assert_eq!(iqa_meaning("total", true, false), "inter-fragment interaction energy");
        assert_eq!(iqa_meaning("total", true, true), "intra-fragment (self) energy");
        assert_eq!(iqa_meaning("total", false, false), "inter-atomic interaction energy");
        assert_eq!(iqa_meaning("total", false, true), "intra-atomic (self) energy");
        assert_eq!(iqa_meaning("classical_interaction", false, false), "inter-atomic classical (electrostatic) interaction");
        assert_eq!(iqa_meaning("xc", true, false), "inter-fragment exchange-correlation term");
        assert_eq!(iqa_meaning("kinetic", false, true), "intra-atomic kinetic term");
    }

    #[test]
    fn secondary_quantities() {
        let entry = |id: &str| IndexEntry {
            id: id.to_string(),
            analysis: None,
            spin: None,
            path: String::new(),
            kind: QuantityKind::FragmentVector,
            title: String::new(),
            unit: None,
            decimals: None,
            role: None,
            signed: None,
        };
        for id in [
            "bond_orders.atoms.valence_total",
            "bond_orders.fragments.valence_free",
            "bond_orders.overlap_populations.atoms.matrix",
            "populations.atoms.electrons",
            "populations.fragments.electrons",
            "eos.fragments.electrons_alpha",
            "oslo.fragments.electrons",
            "oslo.fragments.electrons_beta",
            "geos.fragments.electrons_paired",
        ] {
            assert!(is_secondary(&entry(id)), "{id} is secondary");
        }
        for id in [
            "populations.atoms.charges",
            "populations.fragments.spin",
            "bond_orders.atoms.matrix",
            "bond_orders.fragments.matrix",
            "eos.fragments.oxidation_states",
            "oslo.fragments.oxidation_states",
            "geos.fragments.electrons_unpaired",
            "local_spin.atoms.unpaired_electrons",
            "local_spin.fragments.s2",
            "enpart.total.atoms",
        ] {
            assert!(!is_secondary(&entry(id)), "{id} is listed");
        }
    }

    #[test]
    fn enpart_term_ids() {
        assert_eq!(enpart_term("enpart.total.atoms"), Some(("total", false)));
        assert_eq!(enpart_term("enpart.classical_interaction.fragments"), Some(("classical_interaction", true)));
        assert_eq!(enpart_term("enpart.atomic_energies.atoms.additive_energy"), None);
        assert_eq!(enpart_term("bond_orders.atoms.matrix"), None);
    }

    #[test]
    fn decimals_follow_the_size_of_the_values() {
        assert_eq!(automatic_decimals(&[0.52, -0.1], None), 2);
        assert_eq!(automatic_decimals(&[0.018, -0.018], None), 3);
        assert_eq!(automatic_decimals(&[0.004], None), 4);
        assert_eq!(automatic_decimals(&[26.1], None), 2);
        // Whole numbers (oxidation states) without decimals.
        assert_eq!(automatic_decimals(&[2.0, -1.0, 0.0], None), 0);
        assert_eq!(automatic_decimals(&[2.0, -1.5], None), 2);
        // Energies: their unit's decimals, whatever the size.
        assert_eq!(automatic_decimals(&[-338.8, 20.8], Some(EnergyUnit::KcalPerMol)), 1);
        assert_eq!(automatic_decimals(&[0.004], Some(EnergyUnit::KcalPerMol)), 1);
        assert_eq!(automatic_decimals(&[-0.6079, 0.0125], Some(EnergyUnit::Hartree)), 4);
    }
}
