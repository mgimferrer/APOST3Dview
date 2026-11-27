//! The Build window's Add section: Select, the elements, the groups,
//! rings, ligands and molecules (by tab, or found by name), the user's own
//! groups, and the hydrogen, mirror and new-molecule buttons.
//!
//! Area: Build (see `areas.rs`).

use apost3dview_build::{LibraryEntry, Tab};
use apost3dview_core::element_data;
use egui::RichText;

use crate::app::App;
use crate::build_add::Tool;

/// The elements offered as buttons; the rest in the periodic table.
const ADD_ELEMENTS: [u32; 12] = [1, 5, 6, 7, 8, 9, 14, 15, 16, 17, 35, 53];

/// The periodic table up to radon, as (row, column) per element, 1-based
/// (lanthanides and actinides left out: not offered).
fn table_position(z: u32) -> Option<(usize, usize)> {
    let (row, first, columns): (usize, u32, &[usize]) = match z {
        1..=2 => (1, 1, &[1, 18]),
        3..=10 => (2, 3, &[1, 2, 13, 14, 15, 16, 17, 18]),
        11..=18 => (3, 11, &[1, 2, 13, 14, 15, 16, 17, 18]),
        19..=36 => (4, 19, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]),
        37..=54 => (5, 37, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]),
        55..=56 => (6, 55, &[1, 2]),
        57 => (6, 57, &[3]),
        72..=86 => (6, 72, &[4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]),
        _ => return None,
    };
    columns.get((z - first) as usize).map(|&c| (row, c))
}

impl App {
    /// The Add section (shown even with no structure open: New molecule).
    pub(crate) fn show_add(&mut self, ui: &mut egui::Ui, active: Option<usize>) {
        egui::CollapsingHeader::new(RichText::new("Add").strong()).id_salt("build_add").default_open(true).show(ui, |ui| {
            let picked = self.build.add.tool.clone();
            ui.horizontal(|ui| {
                if ui.selectable_label(picked == Tool::Select, "Select").on_hover_text("Clicks select atoms (Esc)").clicked() {
                    self.pick_tool(Tool::Select);
                }
                let find = ui.add(egui::TextEdit::singleline(&mut self.build.add.find).hint_text("Find: methyl, PPh3, IPr…").desired_width(170.0));
                if std::mem::take(&mut self.build.add.focus_find) {
                    find.request_focus();
                }
                if !self.build.add.find.is_empty() && ui.small_button("×").on_hover_text("Clear the search").clicked() {
                    self.build.add.find.clear();
                }
            });
            ui.horizontal_wrapped(|ui| {
                for z in ADD_ELEMENTS {
                    let on = picked == Tool::Element(z);
                    if ui.selectable_label(on, element_data(z).symbol).on_hover_text(element_hint(z)).clicked() {
                        self.pick_tool(if on { Tool::Select } else { Tool::Element(z) });
                    }
                }
                let label = if self.build.add.table_open { "Table ⏶" } else { "Table…" };
                if ui.small_button(label).on_hover_text("Every element up to radon").clicked() {
                    self.build.add.table_open = !self.build.add.table_open;
                }
            });
            if self.build.add.table_open {
                self.show_periodic_table(ui, &picked);
            }
            let entries = self.add_entries();
            let query = self.build.add.find.trim().to_lowercase();
            if query.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for tab in Tab::ALL {
                        ui.selectable_value(&mut self.build.add.tab, tab, tab_title(tab));
                    }
                });
                let tab = self.build.add.tab;
                let shown: Vec<&LibraryEntry> = entries.iter().filter(|e| e.tab == tab || (tab == Tab::Mine && e.key.starts_with("mine:"))).collect();
                if tab == Tab::Ligands {
                    ui.label(RichText::new("On one metal site each. Chelating and η ligands come later.").weak().small());
                }
                self.show_entry_chips(ui, &shown, &picked, tab == Tab::Mine);
                if tab == Tab::Mine {
                    self.show_own_groups(ui, active, shown.is_empty());
                }
            } else {
                let found: Vec<&LibraryEntry> = entries.iter().filter(|e| e.matches(&query)).collect();
                if found.is_empty() {
                    ui.label(RichText::new("Nothing found.").weak());
                }
                self.show_entry_chips(ui, &found, &picked, false);
            }
            ui.horizontal(|ui| {
                let mut adjust = !self.build.add.keep_hydrogens;
                if ui
                    .checkbox(&mut adjust, "Adjust hydrogens")
                    .on_hover_text("Changing an element adds or removes its hydrogens to fit its usual valence (C to N on a CH3 gives NH2)")
                    .changed()
                {
                    self.build.add.keep_hydrogens = !adjust;
                    self.build.add.plan = None;
                }
            });
            ui.horizontal_wrapped(|ui| {
                let some = active.is_some();
                if ui.add_enabled(some, egui::Button::new("Add hydrogens")).on_hover_text("To the selected atoms (all atoms when none is selected), to their usual valence").clicked() {
                    self.add_hydrogens_to_selection();
                }
                if ui.add_enabled(some, egui::Button::new("Remove hydrogens")).on_hover_text("Those of the selected atoms (all when none is selected)").clicked() {
                    self.remove_hydrogens_of_selection();
                }
                if ui.add_enabled(some, egui::Button::new("Mirror")).on_hover_text("The mirror image (the other enantiomer), reflected left to right as seen").clicked() {
                    self.mirror_structure();
                }
                if ui.button("New molecule").on_hover_text("A new structure from what is picked (methane with Select), without a file").clicked() {
                    self.new_molecule();
                }
            });
            let hint = match self.tool_label() {
                Some(label) => format!("{label} picked: click an H to replace it, another atom to bond to it, or empty space to add it apart. Esc: Select."),
                None => "Pick an element or a group, then click in the view.".to_string(),
            };
            ui.label(RichText::new(hint).weak().small());
        });
    }

    fn show_entry_chips(&mut self, ui: &mut egui::Ui, entries: &[&LibraryEntry], picked: &Tool, deletable: bool) {
        let mut delete = None;
        ui.horizontal_wrapped(|ui| {
            for entry in entries {
                let tool = Tool::Entry(entry.key.clone());
                let on = *picked == tool;
                let hover = format!("{} ({}), {}", entry.name, tab_title(entry.tab).to_lowercase(), entry.source);
                if ui.selectable_label(on, &entry.label).on_hover_text(hover).clicked() {
                    self.pick_tool(if on { Tool::Select } else { tool });
                }
                if deletable && ui.small_button("×").on_hover_text(format!("Delete {}", entry.label)).clicked() {
                    delete = Some(entry.key.clone());
                }
            }
        });
        if let Some(key) = delete {
            self.delete_own_group(&key);
        }
    }

    fn show_own_groups(&mut self, ui: &mut egui::Ui, active: Option<usize>, none: bool) {
        if none {
            ui.label(RichText::new("No groups of your own yet: select a group bonded to the rest by one bond and save it.").weak().small());
        }
        let selection = active.is_some_and(|i| !self.structures[i].selected_atoms.is_empty());
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.build.add.group_name).hint_text("name").desired_width(90.0));
            ui.checkbox(&mut self.build.add.group_is_ligand, "Ligand").on_hover_text("Listed under Ligands, and added in empty space as a molecule of its own");
            if ui.add_enabled(selection, egui::Button::new("Save selection as a group")).on_hover_text("The selected atoms; the one bond leaving them says where the group bonds").clicked() {
                self.save_selection_as_group();
            }
            if ui.small_button("Open groups folder").on_hover_text("Where your groups are saved, one .xyz each").clicked() {
                self.open_groups_folder();
            }
        });
    }

    fn show_periodic_table(&mut self, ui: &mut egui::Ui, picked: &Tool) {
        let mut chosen = None;
        egui::Grid::new("build_periodic_table").spacing([1.0, 1.0]).show(ui, |ui| {
            for row in 1..=6 {
                for column in 1..=18 {
                    match (1..=86).find(|&z| table_position(z) == Some((row, column))) {
                        Some(z) => {
                            let on = *picked == Tool::Element(z);
                            let button = egui::Button::selectable(on, RichText::new(element_data(z).symbol).small()).min_size(egui::vec2(18.0, 16.0));
                            if ui.add(button).on_hover_text(element_hint(z)).clicked() {
                                chosen = Some(if on { Tool::Select } else { Tool::Element(z) });
                            }
                        }
                        None => {
                            ui.label("");
                        }
                    }
                }
                ui.end_row();
            }
        });
        if let Some(tool) = chosen {
            self.pick_tool(tool);
        }
    }
}

/// A tab's name on its button.
fn tab_title(tab: Tab) -> &'static str {
    match tab {
        Tab::Groups => "Groups",
        Tab::Rings => "Rings",
        Tab::Ligands => "Ligands",
        Tab::Molecules => "Molecules",
        Tab::Mine => "Mine",
    }
}

/// What an element button does.
fn element_hint(z: u32) -> String {
    let with = crate::build_add::element_with_hydrogens(&apost3dview_build::element_fragment(z).atomic_numbers);
    let symbol = element_data(z).symbol;
    if with == symbol { format!("{symbol}: on an H, replaces it; on an atom, changes it") } else { format!("{symbol}: on an H, replaces it by {with}; on an atom, changes it") }
}
