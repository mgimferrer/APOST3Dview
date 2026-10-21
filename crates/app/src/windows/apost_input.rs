//! The APOST-3D window's "New input" part: an APOST-3D input for the active
//! `.fchk`, from a few choices with APOST-3D's defaults (see
//! `input_generator`). Kept small on purpose (Martí, 2026-10-21): it is for
//! users who don't write inputs by hand.

use apost3dview_core::{fragments_around, hill_formula, separate_molecules, AimScheme, EnpartMethod, EnpartOptions, FchkProgram, OxidationStates, ENPART_FUNCTIONALS};
use egui::{Color32, RichText};

use crate::app::App;
use crate::input_generator::InputDraft;
use crate::names::atom_name;
use crate::results::fragment_color;
use crate::util::swatch;

const PROBLEM: Color32 = Color32::from_rgb(196, 60, 40);
const NOTICE: Color32 = Color32::from_rgb(200, 130, 20);
const DONE: Color32 = Color32::from_rgb(45, 130, 80);

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

/// What the window asks for after drawing.
enum Action {
    FromSelection,
    AroundSelection,
    SeparateMolecules,
    Remove(usize),
    Up(usize),
    Select(usize),
    Clear,
    Save,
    SaveAsJob(String),
}

/// "Gaussian · RB3LYP · restricted", from the `.fchk`.
fn facts_line(draft: &InputDraft) -> String {
    match &draft.facts {
        Ok(f) => {
            let program = match f.program {
                FchkProgram::Gaussian => Some("Gaussian"),
                FchkProgram::QChem => Some("Q-Chem"),
                FchkProgram::PySCF => Some("pySCF"),
                FchkProgram::Unknown => None,
            };
            let shell = if f.correlated { "correlated" } else if f.unrestricted { "unrestricted" } else { "restricted" };
            [program, Some(f.method.as_str()).filter(|m| !m.is_empty()), Some(shell)].into_iter().flatten().collect::<Vec<_>>().join(" \u{b7} ")
        }
        Err(err) => format!("The .fchk could not be read: {err}"),
    }
}

fn show_analyses(ui: &mut egui::Ui, draft: &mut InputDraft) {
    let real = draft.input.aim.real_space();
    let correlated = draft.correlated();
    let input = &mut draft.input;
    ui.add_enabled(false, egui::Checkbox::new(&mut true, "Population analysis")).on_disabled_hover_text("Always done: charges, bond orders, valences");
    ui.horizontal(|ui| {
        let mut on = input.oxidation.is_some();
        if ui.checkbox(&mut on, "Effective oxidation states").on_hover_text("EOS: oxidation states of the fragments from their effective orbitals").changed() {
            input.oxidation = on.then_some(OxidationStates::Eos);
        }
        if let Some(kind) = &mut input.oxidation {
            ui.selectable_value(kind, OxidationStates::Eos, "EOS");
            ui.add_enabled_ui(real || *kind == OxidationStates::Geos, |ui| {
                ui.selectable_value(kind, OxidationStates::Geos, "GEOS").on_hover_text("Generalized EOS, from the paired and unpaired densities (for correlated and open-shell wavefunctions)")
            })
            .response
            .on_disabled_hover_text("GEOS needs a real-space atomic definition (TFVC)");
        }
    });
    let oslo_possible = (real && !correlated) || input.oslo;
    ui.add_enabled(oslo_possible, egui::Checkbox::new(&mut input.oslo, "Oxidation states from localized orbitals (OSLO)"))
        .on_disabled_hover_text(if correlated { "OSLO needs a single-determinant wavefunction (HF or DFT)" } else { "OSLO needs a real-space atomic definition (TFVC)" });
    if correlated {
        // DM 2 is always written for a correlated file, and does SPIN too.
        ui.add_enabled(false, egui::Checkbox::new(&mut true, "Local spin (SPIN)"))
            .on_disabled_hover_text("Always done for a correlated wavefunction: its density matrices (DM 2) include the local spin analysis");
    } else {
        ui.checkbox(&mut input.spin, "Local spin (SPIN)");
    }
    let mut enpart = input.enpart.is_some();
    if ui.add_enabled(real || enpart, egui::Checkbox::new(&mut enpart, "Energy decomposition, IQA (ENPART)")).on_disabled_hover_text("ENPART needs a real-space atomic definition (TFVC)").changed() {
        input.enpart = enpart.then_some(EnpartOptions { method: None, fast_grid: false });
    }
}

fn show_fragments(ui: &mut egui::Ui, draft: &InputDraft, atomic_numbers: &[u32], selected: &[usize], action: &mut Option<Action>) {
    let input = &draft.input;
    let needed = input.needs_fragments() || input.enpart.is_some();
    if draft.fragments.is_empty() {
        let (text, color) = if input.needs_fragments() {
            ("None yet: EOS, GEOS and OSLO need them.", NOTICE)
        } else {
            ("None: every atom counts on its own.", ui.visuals().weak_text_color())
        };
        ui.label(RichText::new(text).color(color));
    }
    for (i, atoms) in draft.fragments.iter().enumerate() {
        ui.horizontal(|ui| {
            let [r, g, b] = fragment_color(i);
            swatch(ui, Color32::from_rgb(r, g, b), 12.0);
            let names: Vec<String> = atoms.iter().map(|&a| atom_name(atomic_numbers, a)).collect();
            let shown = if names.len() > 6 { format!("{}, ... (+{})", names[..6].join(", "), names.len() - 6) } else { names.join(", ") };
            let formula = hill_formula(atoms.iter().map(|&a| atomic_numbers[a]));
            if ui.selectable_label(false, format!("{}  {formula}   {shown}", i + 1)).on_hover_text(format!("Fragment {}: {}\nClick to select its atoms", i + 1, names.join(", "))).clicked() {
                *action = Some(Action::Select(i));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("\u{d7}").on_hover_text("Remove this fragment").clicked() {
                    *action = Some(Action::Remove(i));
                }
                if i > 0 && ui.small_button("\u{23f6}").on_hover_text("Move up (fragments are numbered in this order)").clicked() {
                    *action = Some(Action::Up(i));
                }
            });
        });
    }
    let assigned: usize = draft.fragments.iter().map(Vec::len).sum();
    let rest = atomic_numbers.len().saturating_sub(assigned);
    if !draft.fragments.is_empty() && rest > 0 {
        let s = if rest == 1 { "" } else { "s" };
        let text = format!("{rest} atom{s} in no fragment (gray in the view)");
        ui.label(if needed { RichText::new(text).color(NOTICE) } else { RichText::new(text).weak() });
    }
    hint(ui, "Select atoms in the view (click them, or Alt/Option-drag a box around them), then:");
    ui.horizontal_wrapped(|ui| {
        let label = if selected.is_empty() { "New fragment from the selection".to_string() } else { format!("New fragment from the {} selected", selected.len()) };
        if ui.add_enabled(!selected.is_empty(), egui::Button::new(label)).on_disabled_hover_text("Select atoms in the view first").clicked() {
            *action = Some(Action::FromSelection);
        }
        if !draft.fragments.is_empty() && ui.button("Clear").on_hover_text("Remove all fragments").clicked() {
            *action = Some(Action::Clear);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Automatic:");
        if ui
            .add_enabled(!selected.is_empty(), egui::Button::new("Around the selected atom(s)"))
            .on_hover_text("Each selected atom (e.g. a metal) a fragment, and each ligand left when they are taken out another. Uses the bonds drawn: fix a wrong one with Create bond or Hide bond.")
            .on_disabled_hover_text("Select the central atom(s) first, e.g. the metal")
            .clicked()
        {
            *action = Some(Action::AroundSelection);
        }
        if ui.button("Separate molecules").on_hover_text("One fragment per molecule (atoms connected by bonds)").clicked() {
            *action = Some(Action::SeparateMolecules);
        }
    });
}

fn show_enpart_options(ui: &mut egui::Ui, draft: &mut InputDraft) {
    let from_file = draft.facts.as_ref().ok().and_then(|f| f.enpart_method());
    let method_line = draft.facts.as_ref().map(|f| f.method.clone()).unwrap_or_default();
    let correlated = draft.correlated();
    let Some(enpart) = &mut draft.input.enpart else { return };
    if correlated {
        ui.label("Energy of the CASSCF wavefunction (also FCI and CISD from pySCF), from its RDMs.");
    } else {
        // The functional the file names is used until another is chosen
        // (`build`).
        let current = enpart.method.clone().or(from_file.clone());
        let text = match &current {
            Some(EnpartMethod::Hf) => "HF".to_string(),
            Some(EnpartMethod::Functional(name)) => name.clone(),
            _ => "choose...".to_string(),
        };
        ui.horizontal(|ui| {
            ui.label("Functional");
            egui::ComboBox::from_id_salt("input_enpart_functional").selected_text(text).show_ui(ui, |ui| {
                ui.selectable_value(&mut enpart.method, Some(EnpartMethod::Hf), "HF");
                for (keyword, description) in ENPART_FUNCTIONALS {
                    ui.selectable_value(&mut enpart.method, Some(EnpartMethod::Functional(keyword.to_string())), keyword).on_hover_text(description);
                }
            });
        });
        match &from_file {
            Some(m) if current.as_ref() == Some(m) => hint(ui, &format!("From the .fchk ({method_line}). It must be the functional of the calculation.")),
            Some(_) => {
                ui.label(RichText::new(format!("The .fchk says {method_line}: the functional must be the one of the calculation.")).small().color(NOTICE));
            }
            None => {
                let text = if method_line.is_empty() {
                    "Choose the functional of the calculation.".to_string()
                } else {
                    format!("The .fchk says \"{method_line}\": choose the functional of the calculation (meta-GGA and range-separated ones are not supported).")
                };
                ui.label(RichText::new(text).small().color(NOTICE));
            }
        }
    }
    ui.horizontal(|ui| {
        ui.label("Two-electron grid");
        ui.selectable_value(&mut enpart.fast_grid, false, "Standard (150 \u{d7} 590)").on_hover_text("APOST-3D's default");
        ui.selectable_value(&mut enpart.fast_grid, true, "Faster (40 \u{d7} 146)").on_hover_text("Much cheaper and less accurate: for tests and large systems");
    });
}

impl App {
    /// The "New input" part of the APOST-3D window.
    pub(crate) fn show_input_generator(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.active_structure else {
            ui.label(RichText::new("Open the calculation's .fchk to prepare an APOST-3D input for it.").weak());
            return;
        };
        let structure = &mut self.structures[active];
        let Some(fchk) = structure.source_path.clone().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fchk"))) else {
            ui.label(RichText::new("An APOST-3D input is made for a .fchk: open the calculation's .fchk.").weak());
            return;
        };
        let atomic_numbers = structure.molecule.atomic_numbers.clone();
        let n_atoms = atomic_numbers.len();
        let selected = structure.selected_atoms.clone();
        // The first time the form is shown, make room for the fragment
        // envelopes it draws (as for results with fragments), zooming out
        // only if the view is tighter than that.
        if structure.input_draft.is_none() {
            let (center, radius) = structure.molecule.bounding_sphere();
            let mut roomy = self.camera;
            roomy.frame_bounds(center, radius + 2.2);
            if roomy.distance > self.camera.distance {
                self.camera.distance = roomy.distance;
            }
        }
        let draft = structure.input_draft.get_or_insert_with(|| InputDraft::new(&fchk, n_atoms));

        let input = draft.build();
        let text = input.to_text(n_atoms);
        let problems = input.problems(n_atoms);
        let notes = draft.notes(&input);
        let exists = draft.input_path().exists();
        let file_name = draft.file_name();
        let mut action = None;

        // Saving, always in sight at the top (the window scrolls).
        ui.horizontal_wrapped(|ui| {
            let ready = problems.is_empty();
            let save = if exists { format!("Replace {file_name}") } else { format!("Save {file_name}") };
            let hover = if exists { format!("Overwrite {file_name} next to the .fchk") } else { "Next to the .fchk, with the name APOST-3D looks for".to_string() };
            if ui.add_enabled(ready, egui::Button::new(save)).on_hover_text(hover).on_disabled_hover_text("Solve the problems listed below first").clicked() {
                action = Some(Action::Save);
            }
            if exists
                && draft.another_job.is_none()
                && ui
                    .add_enabled(ready, egui::Button::new("Save as another job..."))
                    .on_hover_text("Keep this input and save the new one under another name, for another analysis of the same calculation")
                    .clicked()
            {
                draft.another_job = Some(draft.suggested_job());
                draft.another_job_error = None;
            }
            if ui.button("Copy input").clicked() {
                ui.ctx().copy_text(text.clone());
            }
            if ready {
                ui.label(RichText::new("complete").color(DONE));
            }
        });
        if let Some(mut name) = draft.another_job.take() {
            let mut cancel = false;
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Job name");
                    ui.add(egui::TextEdit::singleline(&mut name).id(egui::Id::new("input_another_job")).desired_width(200.0));
                });
                hint(
                    ui,
                    &format!(
                        "Saves {name}.inp and {name}.fchk next to {job}.fchk: APOST-3D needs an input and a .fchk of the same name. {name}.fchk is a link to {job}.fchk, using no extra disk space (a full copy where links aren't possible, e.g. on a FAT-formatted USB stick, and once copied to another computer).",
                        job = draft.job
                    ),
                );
                if let Some(error) = &draft.another_job_error {
                    ui.label(RichText::new(error).color(PROBLEM));
                }
                ui.horizontal(|ui| {
                    if ui.button(format!("Save {name}.inp")).clicked() {
                        action = Some(Action::SaveAsJob(name.clone()));
                    }
                    cancel = ui.button("Cancel").clicked();
                });
            });
            if !cancel {
                draft.another_job = Some(name);
            }
        }
        for problem in &problems {
            ui.label(RichText::new(problem).small().color(PROBLEM));
        }
        for note in &notes {
            ui.label(RichText::new(note).small().color(NOTICE));
        }
        ui.separator();

        ui.label(RichText::new(format!("Input for {}.fchk", draft.job)).strong());
        hint(ui, &facts_line(draft));
        if let Some(other) = &draft.loaded {
            hint(ui, &format!("Filled in from the existing {file_name}."));
            if !other.is_empty() {
                ui.label(RichText::new(format!("{file_name} also sets what this form doesn't handle; saving from here leaves it out: {}.", other.join("; "))).small().color(NOTICE));
            }
        }
        ui.add_space(4.0);

        egui::CollapsingHeader::new(RichText::new("Atoms in molecules").strong()).id_salt("input_aim").default_open(true).show(ui, |ui| {
            let offered = draft.offered_schemes();
            let aim = &mut draft.input.aim;
            egui::ComboBox::from_id_salt("input_aim_combo").width(280.0).selected_text(format!("{}  {}", aim.keyword(), aim.name())).show_ui(ui, |ui| {
                for scheme in offered {
                    let recommended = if scheme == AimScheme::Tfvc { " (recommended)" } else { "" };
                    ui.selectable_value(aim, scheme, format!("{}  {}{recommended}", scheme.keyword(), scheme.name()));
                }
            });
            hint(ui, if aim.real_space() { "Real space: works with every analysis." } else { "Hilbert space: fast, but depends on the basis set; no GEOS, OSLO or ENPART." });
        });
        egui::CollapsingHeader::new(RichText::new("Analyses").strong()).id_salt("input_analyses").default_open(true).show(ui, |ui| show_analyses(ui, draft));
        egui::CollapsingHeader::new(RichText::new(format!("Fragments ({})", draft.fragments.len())).strong()).id_salt("input_fragments").default_open(true).show(ui, |ui| {
            show_fragments(ui, draft, &atomic_numbers, &selected, &mut action);
        });
        if draft.input.enpart.is_some() {
            egui::CollapsingHeader::new(RichText::new("Energy decomposition (ENPART)").strong()).id_salt("input_enpart").default_open(true).show(ui, |ui| show_enpart_options(ui, draft));
        }
        egui::CollapsingHeader::new(RichText::new("Input file").strong()).id_salt("input_text").default_open(false).show(ui, |ui| {
            ui.label(RichText::new(&file_name).strong());
            egui::Frame::new().fill(ui.visuals().extreme_bg_color).inner_margin(6.0).corner_radius(3.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(&text).monospace()).selectable(true));
            });
            hint(ui, &format!("To run it, in that folder:  apost3d {job} > {job}.apost 2>&1", job = draft.job));
        });

        self.apply_input_action(active, action, &text, &input);
    }

    fn apply_input_action(&mut self, active: usize, action: Option<Action>, text: &str, input: &apost3dview_core::ApostInput) {
        let Some(action) = action else { return };
        let structure = &mut self.structures[active];
        let n_atoms = structure.molecule.atomic_numbers.len();
        let bonds: Vec<(usize, usize)> =
            structure.molecule.bonds.iter().enumerate().filter(|(i, _)| !structure.hidden_bonds.contains(i)).map(|(_, b)| (b.atom_a, b.atom_b)).collect();
        let selected = structure.selected_atoms.clone();
        let Some(draft) = structure.input_draft.as_mut() else { return };
        match action {
            Action::FromSelection => {
                structure.selected_atoms.clear();
                self.add_fragment(selected);
            }
            Action::AroundSelection => {
                draft.fragments = fragments_around(n_atoms, &bonds, &selected);
                structure.selected_atoms.clear();
            }
            Action::SeparateMolecules => draft.fragments = separate_molecules(n_atoms, &bonds),
            Action::Remove(i) => {
                draft.fragments.remove(i);
            }
            Action::Up(i) => draft.fragments.swap(i, i - 1),
            Action::Clear => draft.fragments.clear(),
            Action::Select(i) => {
                structure.selected_atoms = draft.fragments[i].clone();
                structure.selected_bonds.clear();
            }
            Action::Save => {
                let path = draft.input_path();
                let name = draft.file_name();
                match std::fs::write(&path, text) {
                    Ok(()) => self.show_status(format!("{name} saved")),
                    Err(err) => self.show_warning(format!("Could not save {}: {err}", path.display())),
                }
            }
            Action::SaveAsJob(name) => match draft.save_as_job(&name, text, input) {
                Ok(message) => {
                    draft.another_job = None;
                    self.show_status(message);
                }
                Err(error) => draft.another_job_error = Some(error),
            },
        }
        self.rebuild_highlights();
    }

    /// Makes `atoms` (0-based) a new fragment of the active structure's
    /// input, taking them out of the fragments they were in.
    pub(crate) fn add_fragment(&mut self, mut atoms: Vec<usize>) {
        let Some(draft) = self.active_structure.and_then(|i| self.structures.get_mut(i)).and_then(|s| s.input_draft.as_mut()) else { return };
        atoms.sort_unstable();
        atoms.dedup();
        for fragment in &mut draft.fragments {
            fragment.retain(|a| !atoms.contains(a));
        }
        draft.fragments.retain(|f| !f.is_empty());
        if !atoms.is_empty() {
            draft.fragments.push(atoms);
        }
    }

    /// The fragments of the active structure's input, 0-based — for tests.
    pub fn input_fragments(&self) -> Vec<Vec<usize>> {
        self.active_structure.and_then(|i| self.structures.get(i)).and_then(|s| s.input_draft.as_ref()).map(|d| d.fragments.clone()).unwrap_or_default()
    }
}

impl App {
    /// The input the active structure's generator makes now — for tests.
    pub fn input_text(&self) -> Option<String> {
        let structure = self.active_structure.and_then(|i| self.structures.get(i))?;
        Some(structure.input_draft.as_ref()?.build().to_text(structure.molecule.atomic_numbers.len()))
    }
}
