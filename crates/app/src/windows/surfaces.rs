//! Surfaces window: orbital generation from the active structure's
//! wavefunction, and its list of isosurfaces (opened `.cube` files and
//! generated orbitals), each shown or hidden on its own, with the settings
//! of the selected one.

use egui::{RichText, Slider};

use crate::app::App;
use crate::windows::ToolWindow;
use crate::orbital_sets::show_set_checklist;
use crate::isosurfaces::{SurfaceSettings, SurfaceSource};
use crate::orbitals::{fitted_only, fragment_weighting, show_mo_checklist, OrbitalAccuracyPreset};
use crate::state::LoadedStructure;

/// What the surface list asks for after drawing.
enum ListAction {
    ShowAlone(usize),
    Toggle(usize),
    Remove(usize),
    ShowAll(bool),
}

impl App {
    pub(crate) fn show_surfaces_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_surfaces;
        let response = self.column_window(ctx, ToolWindow::Surfaces, &mut open).show(ctx, |ui| self.show_surfaces_content(ui));
        self.record_column_window(ctx, ToolWindow::Surfaces, response.map(|r| r.response));
        self.show_surfaces = open;
    }

    fn show_surfaces_content(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.active_structure else {
            ui.label(RichText::new("Open a structure first.").weak());
            return;
        };
        let structure = &self.structures[active];
        if structure.wavefunction.is_some() || !structure.orbital_sets.is_empty() {
            egui::CollapsingHeader::new(RichText::new("Generate orbitals and densities").strong())
                .id_salt("generate_orbitals")
                .default_open(true)
                .show(ui, |ui| {
                    self.show_orbital_set_selector(ui, active);
                    let natural_listed = self.structures[active].densities.as_ref().is_some_and(|d| d.listed.is_some());
                    if self.structures[active].active_orbital_set == 0 && natural_listed {
                        self.show_generate_natural_orbitals(ui, active);
                    } else if self.structures[active].active_orbital_set == 0 {
                        self.show_generate_orbitals(ui, active);
                    } else {
                        self.show_generate_set_orbitals(ui, active);
                    }
                });
            ui.add_space(6.0);
        }
        self.show_surface_list(ui, active);
    }

    /// Which orbitals to generate from: the SCF ones of the open file, the
    /// natural orbitals of a correlated density of it, or one of the
    /// analysis sets APOST-3D wrote next to its results.
    fn show_orbital_set_selector(&mut self, ui: &mut egui::Ui, active: usize) {
        let structure = &mut self.structures[active];
        let natural_names: Vec<String> = structure.densities.as_ref().map(|d| d.natural.iter().map(|n| format!("Natural orbitals ({})", n.name)).collect()).unwrap_or_default();
        if structure.orbital_sets.is_empty() && natural_names.is_empty() {
            return;
        }
        // A correlated 1-RDM stored as "SCF": the file's MOs aren't SCF ones.
        let file_orbitals = if structure.densities.as_ref().is_some_and(|d| d.info.correlated_scf) { "Orbitals of the file" } else { "SCF orbitals" };
        // (set, natural): (0, None) the file's orbitals, (0, Some(i)) natural set i, (i, None) analysis set i - 1.
        let mut options: Vec<((usize, Option<usize>), String)> = Vec::new();
        if structure.wavefunction.is_some() {
            options.push(((0, None), file_orbitals.to_string()));
            options.extend(natural_names.into_iter().enumerate().map(|(i, name)| ((0, Some(i)), name)));
        }
        options.extend(structure.orbital_sets.iter().enumerate().map(|(i, s)| ((i + 1, None), s.set.title().to_string())));
        let listed = structure.densities.as_ref().and_then(|d| d.listed);
        let mut current = (structure.active_orbital_set, if structure.active_orbital_set == 0 { listed } else { None });
        if !options.iter().any(|(o, _)| *o == current) {
            current = options[0].0;
        }
        let current_name = options.iter().find(|(o, _)| *o == current).map(|(_, n)| n.clone()).unwrap_or_default();
        let mut chosen = current;
        ui.horizontal(|ui| {
            ui.label("Orbitals:");
            egui::ComboBox::from_id_salt("orbital_set")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    for (option, name) in &options {
                        ui.selectable_value(&mut chosen, *option, name);
                    }
                })
                .response
                .on_hover_text(
                    "The orbitals of the open file; natural orbitals diagonalize a correlated density matrix stored in it (occupations 2 to 0); \
                     the others are APOST-3D's analysis orbitals, labeled with their fragments.",
                );
        });
        structure.active_orbital_set = chosen.0;
        if let Some(densities) = structure.densities.as_mut().filter(|d| d.listed != chosen.1) {
            if let Some(set) = chosen.1.and_then(|i| densities.natural.get(i)) {
                densities.selected = crate::natural::frontier(set);
            }
            densities.listed = chosen.1;
        }
        ui.add_space(2.0);
    }

    /// The natural orbitals of a correlated density, by occupation.
    fn show_generate_natural_orbitals(&mut self, ui: &mut egui::Ui, active: usize) {
        let Some(densities) = &mut self.structures[active].densities else { return };
        let Some(set) = densities.listed.and_then(|i| densities.natural.get(i)) else { return };
        let orbitals = &set.orbitals.alpha;
        ui.label(
            RichText::new(format!("The orbitals that diagonalize the {} density matrix of the file, largest occupation first.", set.name))
                .small()
                .weak(),
        );
        ui.horizontal(|ui| {
            if ui.button("Frontier").on_hover_text("The last natural orbital with occupation above 1 and the first below").clicked() {
                densities.selected = crate::natural::frontier(set);
            }
            if ui.button("Fractional").on_hover_text("Every natural orbital with occupation between 0.02 and 1.98").clicked() {
                densities.selected = orbitals.occupations.iter().enumerate().filter(|(_, n)| **n > 0.02 && **n < 1.98).map(|(i, _)| i).collect();
            }
            if ui.button("Clear").clicked() {
                densities.selected.clear();
            }
        });
        egui::ScrollArea::vertical().id_salt("natural_checklist").max_height(160.0).show(ui, |ui| {
            for (index, &occupation) in orbitals.occupations.iter().enumerate() {
                let mut checked = densities.selected.contains(&index);
                if ui.checkbox(&mut checked, format!("NO {}  occupation {occupation:.4}", index + 1)).changed() {
                    if checked {
                        densities.selected.insert(index);
                    } else {
                        densities.selected.remove(&index);
                    }
                }
            }
        });
        let selected_count = densities.selected.len();
        self.show_grid_controls(ui);
        ui.add_space(4.0);
        if ui.add_sized([ui.available_width(), 26.0], egui::Button::new(format!("Generate {selected_count} orbital(s)"))).clicked() {
            self.generate_natural_orbitals();
        }
    }

    /// The orbitals of an analysis set: only those the tables describe,
    /// each labeled with its fragment(s) and numbers.
    fn show_generate_set_orbitals(&mut self, ui: &mut egui::Ui, active: usize) {
        let set_index = self.structures[active].active_orbital_set - 1;
        let LoadedStructure { orbital_sets, apost, molecule, .. } = &mut self.structures[active];
        let Some(Ok(results)) = apost.as_ref().map(|a| &a.results) else { return };
        let set = &mut orbital_sets[set_index];
        if let Some(Err(reason)) = &set.wavefunction {
            ui.label(RichText::new(reason).small().color(ui.visuals().warn_fg_color));
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("Select all").clicked() {
                for beta in [false, true] {
                    let indices: Vec<usize> = set.labels(beta).iter().enumerate().filter(|(_, l)| l.is_some()).map(|(i, _)| i).collect();
                    let selected = if beta { &mut set.selected_beta } else { &mut set.selected_alpha };
                    selected.extend(indices);
                }
            }
            if ui.button("Clear").clicked() {
                set.selected_alpha.clear();
                set.selected_beta.clear();
            }
            // The frontier EFOs (APOST-3D 0.15 on): the ones that set R.
            if let Some(frontier) = set.set.frontier.clone().filter(|f| !f.is_empty())
                && ui
                    .button("Frontier")
                    .on_hover_text("The last occupied (LO) and first unoccupied (FU) EFOs: the ones that set the reliability index R (all of them, when several are degenerate)")
                    .clicked()
            {
                set.selected_alpha = frontier.iter().filter(|f| !f.beta).map(|f| f.index).collect();
                set.selected_beta = frontier.iter().filter(|f| f.beta).map(|f| f.index).collect();
            }
        });
        let has_beta = set.labels(true).iter().any(Option::is_some);
        let list_height = if has_beta { 110.0 } else { 160.0 };
        // Drawn from the unweighted file, EFOs are exact (no "approximate").
        let exact = self.orbital_generation.fragment_weighted && matches!(fragment_weighting(set, results), Some(Ok(_)));
        show_set_checklist(ui, set, false, results, molecule, list_height, exact);
        if has_beta {
            show_set_checklist(ui, set, true, results, molecule, list_height, exact);
        } else if let Some(note) = &set.set.beta_note {
            ui.label(RichText::new(note).small().weak());
        }
        // EFOs are drawn from APOST-3D's unweighted file times their
        // fragment's weight whenever the results have both (APOST-3D 0.17
        // on, real-space atoms): exactly the function of its cube files.
        // Drawing the other file as stored is kept out of the way, under
        // Advanced (Martí, 2026-10-22).
        let mut reweigh = None;
        match fragment_weighting(set, results).map(|cells| cells.err()) {
            Some(None) => {
                egui::CollapsingHeader::new(RichText::new("Advanced").small()).id_salt("set_orbitals_advanced").default_open(false).show(ui, |ui| {
                    let scheme = results.input.as_ref().and_then(|i| i.atomic_definition.as_ref()).and_then(|d| d.short_name.clone()).unwrap_or_else(|| "atomic".to_string());
                    let hint = format!(
                        "EFOs are drawn from APOST-3D's unweighted orbitals times their fragment's {scheme} weight w_F: exactly the function of \
                         APOST-3D's cube files, and the way to judge an EFO's shape. Ticked: the orbitals of the first .fchk exactly as stored, \
                         projections of the EFOs onto the basis set, approximate, with small tails on the other fragments (what other programs show \
                         from that file). Changing this redraws the EFOs already generated."
                    );
                    let mut as_stored = !self.orbital_generation.fragment_weighted;
                    if ui.checkbox(&mut as_stored, "Draw as stored in the .fchk").on_hover_text(hint).changed() {
                        self.orbital_generation.fragment_weighted = !as_stored;
                        reweigh = Some(set.set.key());
                    }
                });
            }
            Some(Some(reason)) => {
                ui.label(RichText::new(format!("The atomic weights of the results can't be used: {reason}")).small().color(ui.visuals().warn_fg_color));
            }
            None if fitted_only(set, results) => {
                ui.label(
                    RichText::new(
                        "Drawn as stored in the .fchk: projections of the EFOs onto the basis set, approximate, with small tails on other \
                         fragments. For an EFO's exact shape use its cube file, or results of APOST-3D 0.17 and later, whose unweighted EFOs \
                         are drawn exactly.",
                    )
                    .small()
                    .weak(),
                );
            }
            None => {}
        }
        let selected_count = set.selected_alpha.len() + set.selected_beta.len();
        self.show_grid_controls(ui);
        ui.add_space(4.0);
        if ui.add_sized([ui.available_width(), 26.0], egui::Button::new(format!("Generate {selected_count} orbital(s)"))).clicked() {
            self.generate_set_orbitals();
        }
        if let Some(key) = reweigh {
            self.reweigh_set_surfaces(active, &key);
        }
    }

    fn show_generate_orbitals(&mut self, ui: &mut egui::Ui, active: usize) {
        let num_orbitals = self.structures[active].wavefunction.as_ref().unwrap().alpha.num_orbitals();
        let is_unrestricted = self.structures[active].wavefunction.as_ref().unwrap().beta.is_some();
        self.orbital_generation.range_start = self.orbital_generation.range_start.clamp(1, num_orbitals.max(1));
        self.orbital_generation.range_end = self.orbital_generation.range_end.clamp(1, num_orbitals.max(1));

        ui.horizontal(|ui| {
            if ui.button("HOMO/LUMO").clicked() {
                let wfn = self.structures[active].wavefunction.as_ref().unwrap();
                let (alpha_homo, alpha_lumo, alpha_num) = (wfn.alpha.homo_index(), wfn.alpha.lumo_index(), wfn.alpha.num_orbitals());
                let beta_homo_lumo_num = wfn.beta.as_ref().map(|b| (b.homo_index(), b.lumo_index(), b.num_orbitals()));
                let structure = &mut self.structures[active];
                structure.selected_alpha_mos.clear();
                structure.selected_alpha_mos.insert(alpha_homo - 1);
                if alpha_lumo <= alpha_num {
                    structure.selected_alpha_mos.insert(alpha_lumo - 1);
                }
                structure.selected_beta_mos.clear();
                if let Some((beta_homo, beta_lumo, beta_num)) = beta_homo_lumo_num {
                    structure.selected_beta_mos.insert(beta_homo - 1);
                    if beta_lumo <= beta_num {
                        structure.selected_beta_mos.insert(beta_lumo - 1);
                    }
                }
            }
            if ui.button("Clear").clicked() {
                let structure = &mut self.structures[active];
                structure.selected_alpha_mos.clear();
                structure.selected_beta_mos.clear();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Range:");
            ui.add(egui::DragValue::new(&mut self.orbital_generation.range_start).range(1..=num_orbitals.max(1)));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut self.orbital_generation.range_end).range(1..=num_orbitals.max(1)));
            if ui.button("Select range").clicked() {
                let (lo, hi) = (
                    self.orbital_generation.range_start.min(self.orbital_generation.range_end),
                    self.orbital_generation.range_start.max(self.orbital_generation.range_end),
                );
                let structure = &mut self.structures[active];
                for mo_number in lo..=hi {
                    structure.selected_alpha_mos.insert(mo_number - 1);
                    if is_unrestricted {
                        structure.selected_beta_mos.insert(mo_number - 1);
                    }
                }
            }
        });

        // The densities, generated with the orbitals.
        let has_spin = crate::orbitals::has_spin_density(&self.structures[active]);
        let correlated = self.structures[active]
            .apost
            .as_ref()
            .and_then(|a| a.results.as_ref().ok())
            .and_then(|r| r.enpart())
            .and_then(|e| e.wavefunction)
            .filter(|w| w == "CASSCF" || w == "CISD");
        ui.horizontal(|ui| {
            let structure = &mut self.structures[active];
            ui.checkbox(&mut structure.selected_total_density, "Total density")
                .on_hover_text("The electron density, from the occupied orbitals");
            if has_spin {
                ui.checkbox(&mut structure.selected_spin_density, "Spin density")
                    .on_hover_text("Alpha minus beta density: positive where alpha electrons are in excess");
            }
        });
        // Which density matrix: the occupied orbitals' (mean field) or a
        // correlated one of the file, through its natural orbitals.
        if let Some(densities) = &mut self.structures[active].densities {
            let mut options: Vec<(Option<usize>, String)> = Vec::new();
            if densities.offers_mean_field() {
                options.push((None, "SCF".to_string()));
            }
            options.extend(densities.natural.iter().enumerate().map(|(i, n)| (Some(i), n.name.clone())));
            let name = |chosen: Option<usize>| options.iter().find(|(o, _)| *o == chosen).map(|(_, n)| n.clone()).unwrap_or_default();
            if options.len() > 1 {
                ui.horizontal(|ui| {
                    ui.label("Density:");
                    egui::ComboBox::from_id_salt("density_matrix")
                        .selected_text(name(densities.chosen))
                        .show_ui(ui, |ui| {
                            for (option, label) in &options {
                                ui.selectable_value(&mut densities.chosen, *option, label);
                            }
                        })
                        .response
                        .on_hover_text("SCF: from the occupied orbitals. A correlated density: from the density matrix in the .fchk, through its natural orbitals.");
                });
            } else if let Some(set) = densities.chosen() {
                ui.label(RichText::new(format!("The {} density of the .fchk, through its natural orbitals.", set.name)).small().weak());
            }
        }
        let natural_density = self.structures[active].densities.as_ref().is_some_and(|d| d.chosen.is_some());
        if let Some(method) = correlated.filter(|_| !natural_density) {
            ui.label(
                RichText::new(format!(
                    "For this {method} wavefunction, the density is that of the occupied orbitals in the .fchk, not the correlated density."
                ))
                .small()
                .weak(),
            );
        }

        let list_height = if is_unrestricted { 90.0 } else { 120.0 };
        // Destructuring through the `&mut LoadedStructure`
        // borrows `wavefunction`/`selected_alpha_mos`/
        // `selected_beta_mos` as separate, disjoint fields —
        // lets `wfn` (immutable) and the selection sets
        // (mutable) coexist without needing two separate
        // indexing expressions into `self.structures`, which
        // the borrow checker can't prove disjoint on its own.
        let LoadedStructure {
            wavefunction,
            densities,
            selected_alpha_mos,
            selected_beta_mos,
            ..
        } = &mut self.structures[active];
        let wfn = wavefunction.as_ref().unwrap();
        if densities.as_ref().is_some_and(|d| d.info.correlated_scf) && wfn.alpha.orbital_energies.iter().all(|&e| e == 0.0) {
            ui.label(RichText::new("This file stores no orbital energies (0 below); its natural orbitals carry the occupations.").small().weak());
        }
        show_mo_checklist(ui, &wfn.alpha, selected_alpha_mos, "alpha", list_height);
        if let Some(beta) = &wfn.beta {
            show_mo_checklist(ui, beta, selected_beta_mos, "beta", list_height);
        }

        self.show_grid_controls(ui);

        ui.add_space(4.0);
        let structure = &self.structures[active];
        let selected_count = structure.selected_alpha_mos.len() + structure.selected_beta_mos.len();
        let density_count = usize::from(structure.selected_total_density) + usize::from(structure.selected_spin_density && has_spin);
        let label = match density_count {
            0 => format!("Generate {selected_count} orbital(s)"),
            n => format!("Generate {} surface(s)", selected_count + n),
        };
        if ui.add_sized([ui.available_width(), 26.0], egui::Button::new(label)).clicked() {
            self.generate_selected_orbitals();
        }
    }

    /// Grid accuracy and box padding, shared by SCF and analysis orbitals.
    fn show_grid_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label("Accuracy:");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Low, "Low");
            ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Medium, "Medium");
            ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::High, "High");
            ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Custom, "Custom");
        });
        if self.orbital_generation.preset == OrbitalAccuracyPreset::Custom {
            ui.horizontal(|ui| {
                ui.label("Spacing (Bohr):");
                ui.add(egui::DragValue::new(&mut self.orbital_generation.custom_spacing_bohr).speed(0.01).range(0.02..=2.0));
            });
        } else {
            ui.label(egui::RichText::new(format!("Grid spacing: {:.2} Bohr", self.orbital_generation.resolve_spacing_bohr())).small().weak());
        }
        ui.horizontal(|ui| {
            ui.label("Box padding (Bohr):");
            ui.add(egui::DragValue::new(&mut self.orbital_generation.padding_bohr).speed(0.1).range(1.0..=20.0));
        });
        ui.label(egui::RichText::new("If a generated orbital's isosurface looks flat-cut at one side, raise this.").small().weak());
    }

    fn show_surface_list(&mut self, ui: &mut egui::Ui, active: usize) {
        let structure = &self.structures[active];
        if structure.surfaces.is_empty() {
            let hint = if structure.wavefunction.is_some() {
                "No surfaces yet: generate orbitals above, or open .cube files of this molecule."
            } else {
                "No surfaces. Open .cube files of this molecule, or a .fchk/.molden to generate orbitals."
            };
            ui.label(RichText::new(hint).small().weak());
            return;
        }

        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Surfaces ({})", structure.surfaces.len())).strong());
            if ui.small_button("Show all").clicked() {
                action = Some(ListAction::ShowAll(true));
            }
            if ui.small_button("Hide all").clicked() {
                action = Some(ListAction::ShowAll(false));
            }
        });
        egui::ScrollArea::vertical().id_salt("surface_list").max_height(200.0).show(ui, |ui| {
            for (index, surface) in structure.surfaces.iter().enumerate() {
                ui.horizontal(|ui| {
                    let mut visible = surface.visible;
                    if ui.checkbox(&mut visible, "").on_hover_text("Show or hide, keeping the others as they are").changed() {
                        action = Some(ListAction::Toggle(index));
                    }
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    // Colored by fragment: the fragment colors, side by side
                    // for a shared orbital.
                    let mut colors: Vec<egui::Color32> = Vec::new();
                    if let (true, Some(tint)) = (surface.settings.by_fragment, &surface.fragment_tint) {
                        for (_, c) in tint {
                            let color = crate::util::rgb_to_color32(*c);
                            if !colors.contains(&color) {
                                colors.push(color);
                            }
                        }
                    }
                    if colors.is_empty() {
                        colors.push(surface.settings.positive_color);
                    }
                    let width = rect.width() / colors.len() as f32;
                    for (k, color) in colors.iter().enumerate() {
                        let part = egui::Rect::from_min_size(rect.min + egui::vec2(k as f32 * width, 0.0), egui::vec2(width, rect.height()));
                        ui.painter().rect_filled(part, 0.0, *color);
                    }
                    let selected = structure.selected_surface == Some(index);
                    if ui.selectable_label(selected, &surface.label).on_hover_text("Show this surface alone, and edit it below").clicked() {
                        action = Some(ListAction::ShowAlone(index));
                    }
                    ui.label(RichText::new(surface.isovalue_text()).small().weak()).on_hover_text("Isovalue");
                    if ui.small_button("\u{d7}").on_hover_text("Remove this surface").clicked() {
                        action = Some(ListAction::Remove(index));
                    }
                });
            }
        });
        ui.label(RichText::new("Click a name to show it alone; tick boxes to show several together.").small().weak());

        if let Some(action) = action {
            let structure = &mut self.structures[active];
            match action {
                ListAction::ShowAlone(index) => structure.show_surface_alone(index),
                ListAction::Toggle(index) => {
                    structure.surfaces[index].visible = !structure.surfaces[index].visible;
                    structure.selected_surface = Some(index);
                }
                ListAction::Remove(index) => {
                    structure.surfaces.remove(index);
                    structure.selected_surface = match structure.selected_surface {
                        Some(s) if s == index => None,
                        Some(s) if s > index => Some(s - 1),
                        other => other,
                    };
                }
                ListAction::ShowAll(visible) => structure.surfaces.iter_mut().for_each(|s| s.visible = visible),
            }
            self.rebuild_isosurface();
        }

        self.show_surface_settings(ui, active);
    }

    /// Settings of the selected surface. Changes to the isovalue,
    /// refinement or signs re-extract it by themselves (`update_surfaces`);
    /// colors and opacity apply at once. New orbitals and cubes start from
    /// the default settings, or from a surface's with "Use for new
    /// surfaces" (never silently: an isovalue lowered for one surface made
    /// every later one look spread out, found by the APOST-3D session
    /// 2026-10-19).
    fn show_surface_settings(&mut self, ui: &mut egui::Ui, active: usize) {
        let structure = &mut self.structures[active];
        let Some(index) = structure.selected_surface.filter(|&i| i < structure.surfaces.len()) else {
            return;
        };
        let surface_count = structure.surfaces.len();
        let surface = &mut structure.surfaces[index];
        let own_defaults = surface.default_settings();
        let translucent_opacity = surface.translucent_opacity();
        let is_density = surface.density().is_some();

        ui.add_space(6.0);
        ui.separator();
        ui.label(RichText::new(&surface.label).strong());
        if let Some(note) = &surface.note {
            ui.label(RichText::new(note).small().color(ui.visuals().warn_fg_color));
        }
        if surface.reaches_edge && surface.visible {
            let note = match surface.source {
                SurfaceSource::Cube(_) => "This surface reaches the edge of the cube file's box and is cut flat there: write the cube with a larger box, or raise the isovalue.",
                _ => "This surface reaches the edge of its grid's box and is cut flat there: raise Box padding (Bohr) and generate it again, or raise the isovalue.",
            };
            ui.label(RichText::new(note).small().color(ui.visuals().warn_fg_color));
        }
        let before = surface.settings;
        let s = &mut surface.settings;
        ui.horizontal(|ui| {
            ui.label("Isovalue:");
            ui.add(egui::DragValue::new(&mut s.isovalue).speed(0.001).range(0.0001..=f32::MAX).max_decimals(4).update_while_editing(false));
            ui.label("Refinement:");
            for factor in 1..=3 {
                ui.selectable_value(&mut s.refinement, factor, format!("{factor}x"));
            }
        });
        ui.checkbox(&mut s.both_signs, "Both signs (+/-)");
        ui.horizontal(|ui| {
            let mut translucent = s.opacity < 1.0;
            if ui.checkbox(&mut translucent, "Translucent").on_hover_text("See the atoms and the other surfaces through this one").changed() {
                s.opacity = if translucent { translucent_opacity } else { 1.0 };
            }
            ui.add(Slider::new(&mut s.opacity, 0.05..=1.0).text("opacity"));
        });
        if surface.fragment_tint.is_some() {
            ui.checkbox(&mut s.by_fragment, "Color by fragment")
                .on_hover_text("Color the orbital with its fragment's color from the APOST-3D window (blended for an orbital shared between fragments) instead of the positive/negative colors below");
        }
        ui.horizontal(|ui| {
            ui.label("Positive:");
            ui.color_edit_button_srgba(&mut s.positive_color);
            ui.label("Negative:");
            ui.color_edit_button_srgba(&mut s.negative_color);
            if ui.button("Swap").clicked() {
                std::mem::swap(&mut s.positive_color, &mut s.negative_color);
            }
        });
        let mut apply_to_all = false;
        let mut reset = false;
        let mut use_for_new = false;
        ui.horizontal(|ui| {
            if surface_count > 1 && ui.button("Apply to all surfaces").on_hover_text("Use these settings for every surface of this structure").clicked() {
                apply_to_all = true;
            }
            if !is_density && ui.button("Use for new surfaces").on_hover_text("Orbitals and cubes opened or generated from now on start with these settings").clicked() {
                use_for_new = true;
            }
            if ui.button("Default").on_hover_text("Back to the default isovalue, colors and opacity").clicked() {
                reset = true;
            }
        });
        if reset {
            *s = own_defaults;
        }
        let settings = *s;
        if apply_to_all {
            structure.surfaces.iter_mut().for_each(|other| other.settings = settings);
        }
        if use_for_new {
            self.surface_defaults = settings;
        }
        if self.surface_defaults != SurfaceSettings::default() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("New orbitals and cubes start at isovalue {}.", self.surface_defaults.isovalue)).small().weak());
                if ui.small_button("Reset").on_hover_text("Back to the default settings for new surfaces").clicked() {
                    self.surface_defaults = SurfaceSettings::default();
                }
            });
        }
        if settings != before || apply_to_all {
            self.rebuild_isosurface();
        }
    }
}
