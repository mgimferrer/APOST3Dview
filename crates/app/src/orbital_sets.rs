//! Analysis orbital sets (M6): the EFOs of EOS and GEOS and the OSLOs of
//! OSLO, which APOST-3D writes to their own `.fchk` file next to the main
//! one. Each orbital is labeled from its row in the results tables
//! (fragment or fragments, occupation or FOLI, electrons assigned), and
//! its surface is colored by fragment.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use apost3dview_core::{check_orbital_file, hill_formula, parse_fchk_summary, parse_fchk_wavefunction, ApostResults, Molecule, OrbitalDetail, OrbitalLabel, OrbitalSet, Wavefunction};
use egui::{Color32, RichText};
use glam::Vec3;

use crate::app::App;
use crate::results::fragment_color;

/// An orbital set found next to a structure's results.
pub(crate) struct LoadedOrbitalSet {
    pub(crate) set: OrbitalSet,
    pub(crate) path: PathBuf,
    /// Read on first use, after checking the file belongs to the open
    /// `.fchk` (same atoms and basis): `Err` says why it can't be used.
    pub(crate) wavefunction: Option<Result<Wavefunction, String>>,
    pub(crate) selected_alpha: HashSet<usize>,
    pub(crate) selected_beta: HashSet<usize>,
}

/// The sets of `results` whose file exists next to the results file.
pub(crate) fn find_orbital_sets(results: &ApostResults, results_path: &Path) -> Vec<LoadedOrbitalSet> {
    let dir = results_path.parent().unwrap_or(Path::new("."));
    results
        .orbital_sets()
        .into_iter()
        .filter_map(|set| {
            let path = dir.join(Path::new(&set.file).file_name()?);
            path.is_file().then(|| LoadedOrbitalSet { set, path, wavefunction: None, selected_alpha: HashSet::new(), selected_beta: HashSet::new() })
        })
        .collect()
}

impl LoadedOrbitalSet {
    /// Reads the orbitals, once, checking them against `main_fchk`.
    pub(crate) fn load(&mut self, main_fchk: &Path) {
        if self.wavefunction.is_some() {
            return;
        }
        let name = self.set.file.clone();
        let result = (|| {
            let main = parse_fchk_summary(main_fchk).map_err(|e| e.to_string())?;
            let own = parse_fchk_summary(&self.path).map_err(|e| e.to_string())?;
            check_orbital_file(&main, &own).map_err(|reason| format!("{name} does not belong to the open .fchk: {reason}"))?;
            parse_fchk_wavefunction(&self.path).map_err(|e| format!("{name} could not be read: {e}"))
        })();
        self.wavefunction = Some(result);
    }

    /// The channel names of this set: alpha/beta, paired/unpaired (GEOS),
    /// or none (a restricted OSLO set, all in one channel).
    pub(crate) fn channel_name(&self, beta: bool) -> &'static str {
        match (self.set.analysis.as_str(), beta) {
            ("geos", false) => "paired",
            ("geos", true) => "unpaired",
            ("oslo", false) if self.set.beta.is_empty() => "",
            (_, false) => "\u{3b1}",
            (_, true) => "\u{3b2}",
        }
    }

    pub(crate) fn labels(&self, beta: bool) -> &[Option<OrbitalLabel>] {
        if beta { &self.set.beta } else { &self.set.alpha }
    }

    /// "EFO 3 α", "OSLO 9", "EFO 2 unpaired".
    pub(crate) fn orbital_name(&self, beta: bool, index: usize) -> String {
        let kind = if self.set.analysis == "oslo" { "OSLO" } else { "EFO" };
        let channel = self.channel_name(beta);
        if channel.is_empty() { format!("{kind} {}", index + 1) } else { format!("{kind} {} {channel}", index + 1) }
    }
}

/// "2 CN", from a fragment id.
pub(crate) fn fragment_name(results: &ApostResults, molecule: &Molecule, id: u32) -> String {
    let Some(fragment) = results.fragments().iter().find(|f| f.id == id) else { return id.to_string() };
    format!("{id} {}", hill_formula(fragment.atoms.iter().filter_map(|&a| molecule.atomic_numbers.get(a - 1).copied())))
}

/// "1 CH3 + 2 CH3".
fn fragments_text(results: &ApostResults, molecule: &Molecule, label: &OrbitalLabel) -> String {
    label.fragments.iter().map(|&f| fragment_name(results, molecule, f)).collect::<Vec<_>>().join(" + ")
}

/// The numbers that describe an orbital, for its row in the list.
fn detail_text(label: &OrbitalLabel) -> String {
    let shares = if label.shares.len() > 1 { format!("shares {}", label.shares.iter().map(|s| format!("{s:.2}")).collect::<Vec<_>>().join("/")) } else { String::new() };
    let text = match &label.detail {
        OrbitalDetail::Efo { negative: true, net, .. } => format!("negative, net {:.3}", net.unwrap_or(0.0)),
        OrbitalDetail::Efo { gross, net, electrons, .. } => {
            let occupation = gross.or(*net).map(|o| format!("occupation {o:.3}")).unwrap_or_default();
            let electrons = electrons.map(|e| format!("{} electron{}", trim(e), if (e - 1.0).abs() < 1e-9 { "" } else { "s" })).unwrap_or_default();
            [occupation, electrons].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
        }
        OrbitalDetail::Oslo { foli, .. } => foli.map(|f| format!("FOLI {f:.3}")).unwrap_or_default(),
    };
    [text, shares].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
}

fn trim(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 { format!("{}", value.round() as i64) } else { format!("{value:.2}") }
}

/// Atom positions and colors of an orbital's fragments, for coloring its
/// surface by fragment (blended across space for a shared orbital).
pub(crate) fn fragment_tint(results: &ApostResults, molecule: &Molecule, label: &OrbitalLabel) -> Vec<(Vec3, [f32; 3])> {
    let fragments = results.fragments();
    label
        .fragments
        .iter()
        .filter_map(|&id| fragments.iter().position(|f| f.id == id).map(|position| (position, &fragments[position])))
        .flat_map(|(position, fragment)| {
            let color = fragment_color(position).map(|c| c as f32 / 255.0);
            fragment.atoms.iter().filter_map(move |&a| molecule.positions.get(a - 1).map(|p| (*p, color)))
        })
        .collect()
}

/// The tick-box list of one channel of a set: only the orbitals the tables
/// describe, each with its fragment colors, name and numbers.
pub(crate) fn show_set_checklist(ui: &mut egui::Ui, set: &mut LoadedOrbitalSet, beta: bool, results: &ApostResults, molecule: &Molecule, height: f32) {
    let count = set.labels(beta).iter().flatten().count();
    if count == 0 {
        return;
    }
    let channel = set.channel_name(beta);
    let heading = if channel.is_empty() { format!("{} ({count})", set.set.title()) } else { format!("{} {channel} ({count})", set.set.title()) };
    ui.label(RichText::new(heading).small().weak());
    let rows: Vec<(usize, OrbitalLabel)> = set.labels(beta).iter().enumerate().filter_map(|(i, l)| l.clone().map(|l| (i, l))).collect();
    let names: Vec<String> = rows.iter().map(|(i, _)| set.orbital_name(beta, *i)).collect();
    let selected = if beta { &mut set.selected_beta } else { &mut set.selected_alpha };
    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    egui::ScrollArea::vertical().id_salt(("set_checklist", beta)).max_height(height).show_rows(ui, row_height, rows.len(), |ui, range| {
        for k in range {
            let (index, label) = &rows[k];
            ui.horizontal(|ui| {
                let mut checked = selected.contains(index);
                if ui.checkbox(&mut checked, "").changed() {
                    if checked {
                        selected.insert(*index);
                    } else {
                        selected.remove(index);
                    }
                }
                for &id in &label.fragments {
                    if let Some(position) = results.fragments().iter().position(|f| f.id == id) {
                        let [r, g, b] = fragment_color(position);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
                    }
                }
                let negative = matches!(label.detail, OrbitalDetail::Efo { negative: true, .. });
                let text = format!("{}  {}", names[k], fragments_text(results, molecule, label));
                ui.label(if negative { RichText::new(text).italics() } else { RichText::new(text) });
                ui.label(RichText::new(detail_text(label)).small().weak());
            });
        }
    });
}

impl App {
    /// The set's surface label: "EFO 3 α · 2 CN".
    pub(crate) fn set_orbital_label(set: &LoadedOrbitalSet, beta: bool, index: usize, results: &ApostResults, molecule: &Molecule) -> String {
        match set.labels(beta).get(index).and_then(Option::as_ref) {
            Some(label) => format!("{} \u{b7} {}", set.orbital_name(beta, index), fragments_text(results, molecule, label)),
            None => set.orbital_name(beta, index),
        }
    }
}
