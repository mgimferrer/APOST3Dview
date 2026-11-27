//! The Build window's Add section: what is picked (an element, a group, a
//! ligand, a molecule, or Select), what a click would do (worked out while
//! the pointer moves, shown as a preview in the view and as a line in the
//! status bar), and doing it. Also hydrogens, mirror images, new molecules
//! and the user's own groups.
//!
//! The geometry is `apost3dview_build::place` and friends; one click is one
//! edit (copy at the first edit, undo), with the new atoms left selected.
//!
//! Area: Build (see `areas.rs`).

use std::path::PathBuf;

use apost3dview_build::{
    add_hydrogens, change_element_adjusting, element_fragment, group_from_selection, group_from_xyz, group_to_xyz, library, place, Fragment, FragmentKind, LibraryEntry,
    Placement, Site, Tab, View,
};
use apost3dview_core::{element_data, Molecule};
use glam::Vec3;

use crate::app::App;
use crate::build::{Edited, StructureBuild};
use crate::names::atom_name;
use crate::state::LoadedStructure;

/// What a click in the view adds while the Build window is open.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub(crate) enum Tool {
    /// Clicks select, as everywhere else.
    #[default]
    Select,
    /// An element (on a hydrogen: the element with its hydrogens).
    Element(u32),
    /// A library entry or an own group, by key.
    Entry(String),
}

/// The Add section's state.
pub(crate) struct AddState {
    pub(crate) tool: Tool,
    pub(crate) tab: Tab,
    pub(crate) find: String,
    /// Changing an element leaves its hydrogens as they are (the
    /// "Adjust hydrogens" box unticked).
    pub(crate) keep_hydrogens: bool,
    /// The user's own groups, read from their folder when first needed.
    pub(crate) own: Option<Vec<LibraryEntry>>,
    /// "Save selection as a group": its name and whether it is a ligand.
    pub(crate) group_name: String,
    pub(crate) group_is_ligand: bool,
    /// Put the keyboard in the Find field at the next frame.
    pub(crate) focus_find: bool,
    /// The periodic table is shown.
    pub(crate) table_open: bool,
    /// What a click does now, for the pointer's place.
    pub(crate) plan: Option<Plan>,
    /// Where own groups are kept, when not the settings folder (tests).
    pub(crate) folder: Option<PathBuf>,
}

impl Default for AddState {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            tab: Tab::Groups,
            find: String::new(),
            keep_hydrogens: false,
            own: None,
            group_name: String::new(),
            group_is_ligand: false,
            focus_find: false,
            table_open: false,
            plan: None,
            folder: None,
        }
    }
}

/// Where the pointer is: over an atom, or in empty space (the point on
/// the plane through the view's centre).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Target {
    Atom(usize),
    Space(Vec3),
}

/// An addition, worked out without the app: what is added, where, and how
/// to say it.
#[derive(Clone)]
pub(crate) enum Action {
    Place { fragment: Fragment, site: Site, label: String },
    Change { atom: usize, element: u32, adjust: bool },
}

/// What a click would do: the action (done again on the click, so the
/// edit is computed from the structure as it is then), its result for the
/// preview, and the status line.
#[derive(Clone)]
pub(crate) struct Plan {
    key: (usize, u64, Option<usize>, Tool, bool),
    action: Option<(Action, View)>,
    pub(crate) text: String,
    pub(crate) placement: Option<Placement>,
    /// The atom the pointer is over (ringed in the preview).
    pub(crate) atom: Option<usize>,
}

/// The bonds drawn as pairs (hidden bonds cut).
fn drawn_pairs(s: &LoadedStructure) -> Vec<(usize, usize)> {
    let hidden = s.drawn_hidden_bonds();
    s.molecule.bonds.iter().enumerate().filter(|(k, _)| !hidden.contains(k)).map(|(_, b)| (b.atom_a, b.atom_b)).collect()
}

/// A formula in the usual order (C, H, then alphabetical), "CH3", "Fe".
fn formula(atomic_numbers: &[u32]) -> String {
    apost3dview_core::hill_formula(atomic_numbers.iter().copied())
}

/// An element fragment's name: the element, then its hydrogens ("NH2").
pub(crate) fn element_with_hydrogens(atomic_numbers: &[u32]) -> String {
    let hydrogens = atomic_numbers.iter().skip(1).filter(|&&z| z == 1).count();
    let symbol = atomic_numbers.first().map_or("", |&z| element_data(z).symbol);
    match hydrogens {
        0 => symbol.to_string(),
        1 => format!("{symbol}H"),
        n => format!("{symbol}H{n}"),
    }
}

/// The atoms of a structure after an action, or why it can't be done.
pub(crate) fn compute(s: &LoadedStructure, action: &Action, view: View) -> Result<Placement, String> {
    let bonds = drawn_pairs(s);
    match action {
        Action::Place { fragment, site, .. } => place(&s.molecule, &bonds, fragment, *site, view),
        Action::Change { atom, element, adjust } => Ok(change_element_adjusting(&s.molecule, &bonds, *atom, *element, *adjust, view)),
    }
}

impl App {
    /// Every entry offered: the library and the user's own groups.
    pub(crate) fn add_entries(&mut self) -> Vec<LibraryEntry> {
        if self.build.add.own.is_none() {
            self.build.add.own = Some(self.groups_folder().map(|dir| load_own_groups(&dir)).unwrap_or_default());
        }
        library().iter().cloned().chain(self.build.add.own.clone().unwrap_or_default()).collect()
    }

    fn entry(&mut self, key: &str) -> Option<LibraryEntry> {
        self.add_entries().into_iter().find(|e| e.key == key)
    }

    /// The short name of what is picked ("CH3", "tBu", "Fe").
    pub(crate) fn tool_label(&mut self) -> Option<String> {
        match self.build.add.tool.clone() {
            Tool::Select => None,
            Tool::Element(z) => Some(element_data(z).symbol.to_string()),
            Tool::Entry(key) => self.entry(&key).map(|e| e.label),
        }
    }

    /// The viewer's directions in the active structure's coordinates.
    fn view_directions(&self) -> View {
        let (right, _) = self.camera.screen_basis();
        View { right, toward_viewer: -self.camera.forward() }
    }

    /// The action a click at `target` would make, with the picked tool.
    fn action_for(&mut self, active: usize, target: Target) -> Result<Action, String> {
        let s = &self.structures[active];
        let z = s.molecule.atomic_numbers.clone();
        let z = &z;
        let bonds = drawn_pairs(s);
        let neighbours = |a: usize| bonds.iter().filter(|&&(x, y)| x == a || y == a).count();
        let space = |p: Vec3| Site::Free(p);
        match self.build.add.tool.clone() {
            Tool::Select => Err(String::new()),
            Tool::Element(element) => {
                let fragment = element_fragment(element);
                let label = element_with_hydrogens(&fragment.atomic_numbers);
                match target {
                    Target::Space(p) => Ok(Action::Place { fragment, site: space(p), label }),
                    Target::Atom(a) if z[a] == 1 && element == 1 => Err(format!("{} is already a hydrogen", atom_name(z, a))),
                    Target::Atom(a) if z[a] == 1 && neighbours(a) == 1 => Ok(Action::Place { fragment, site: Site::Replace(a), label }),
                    Target::Atom(a) if element == 1 => Ok(Action::Place { fragment, site: Site::On(a), label }),
                    Target::Atom(a) if z[a] == element => Err(format!("{} is already {}", atom_name(z, a), element_data(element).symbol)),
                    Target::Atom(a) => Ok(Action::Change { atom: a, element, adjust: !self.build.add.keep_hydrogens }),
                }
            }
            Tool::Entry(key) => {
                let entry = self.entry(&key).ok_or("That group is no longer there.")?;
                let fragment = entry.fragment.clone();
                let label = entry.label.clone();
                match target {
                    Target::Space(p) => Ok(Action::Place { fragment, site: space(p), label }),
                    Target::Atom(_) if fragment.kind == FragmentKind::Molecule => Err(format!("{label} is a whole molecule: click in empty space to add it")),
                    Target::Atom(a) if z[a] == 1 && neighbours(a) == 1 => Ok(Action::Place { fragment, site: Site::Replace(a), label }),
                    Target::Atom(a) => Ok(Action::Place { fragment, site: Site::On(a), label }),
                }
            }
        }
    }

    /// How a click would read in the status bar.
    fn describe(&self, active: usize, action: &Action, placement: &Placement) -> String {
        let z = &self.structures[active].molecule.atomic_numbers;
        match action {
            Action::Place { site: Site::Replace(h), label, .. } => format!("Click to replace {} by {label}", atom_name(z, *h)),
            Action::Place { site: Site::On(a), label, .. } if label == "H" => format!("Click to add an H to {}", atom_name(z, *a)),
            Action::Place { site: Site::On(a), label, .. } => format!("Click to attach {label} to {}", atom_name(z, *a)),
            Action::Place { label, fragment, .. } => {
                let added: Vec<u32> = placement.added.iter().map(|&i| placement.molecule.atomic_numbers[i]).collect();
                let whole = formula(&added);
                if fragment.kind == FragmentKind::Group && whole != *label { format!("Click to add {label} here (as {whole})") } else { format!("Click to add {label} here") }
            }
            Action::Change { atom, element, adjust } => {
                let symbol = element_data(*element).symbol;
                let mut text = format!("Click to change {} to {symbol}", atom_name(z, *atom));
                if *adjust {
                    let m = &placement.molecule;
                    let new_atom = placement.atom_map[*atom].unwrap_or(*atom);
                    let hydrogens = m.bonds.iter().filter_map(|b| if b.atom_a == new_atom { Some(b.atom_b) } else if b.atom_b == new_atom { Some(b.atom_a) } else { None }).filter(|&j| m.atomic_numbers[j] == 1).count();
                    text += &match hydrogens {
                        0 => String::new(),
                        1 => format!(", now {symbol}H"),
                        n => format!(", now {symbol}H{n}"),
                    };
                }
                text
            }
        }
    }

    /// Works out what a click at `target` would do (kept while nothing
    /// changes, so the preview costs nothing while the pointer rests).
    pub(crate) fn update_add_plan(&mut self, active: usize, target: Option<Target>) {
        let Some(target) = target else {
            self.build.add.plan = None;
            return;
        };
        let atom = match target {
            Target::Atom(a) => Some(a),
            Target::Space(_) => None,
        };
        let geometry = crate::symmetry::geometry_key(&self.structures[active].molecule);
        let key = (active, geometry, atom, self.build.add.tool.clone(), self.build.add.keep_hydrogens);
        // Empty space follows the pointer: placed anew whenever it moves.
        if let Some(plan) = &self.build.add.plan
            && plan.key == key
            && atom.is_some()
        {
            return;
        }
        let view = self.view_directions();
        let plan = match self.action_for(active, target) {
            Err(text) => Plan { key, action: None, text, placement: None, atom },
            Ok(action) => match compute(&self.structures[active], &action, view) {
                Ok(placement) => {
                    let text = self.describe(active, &action, &placement);
                    Plan { key, action: Some((action, view)), text, placement: Some(placement), atom }
                }
                Err(text) => Plan { key, action: None, text, placement: None, atom },
            },
        };
        self.build.add.plan = Some(plan);
    }

    /// Does what the plan says (one edit), leaving the new atoms selected.
    pub(crate) fn apply_add_plan(&mut self) {
        let Some(plan) = self.build.add.plan.take() else { return };
        let Some((action, view)) = plan.action else {
            if !plan.text.is_empty() {
                self.show_warning(plan.text);
            }
            return;
        };
        let done = self.edit(|s| compute(s, &action, view).map(Edited::Changed));
        if done && let Action::Change { atom, .. } = action {
            // The changed atom selected (no new atoms when only hydrogens
            // went).
            if let Some(s) = self.active_structure.and_then(|i| self.structures.get_mut(i))
                && s.selected_atoms.is_empty()
                && atom < s.molecule.atomic_numbers.len()
            {
                s.selected_atoms.push(atom);
                self.rebuild_highlights();
            }
        }
    }

    /// Picks what clicks add (Select: clicks select again).
    pub(crate) fn pick_tool(&mut self, tool: Tool) {
        self.build.add.tool = tool;
        self.build.add.plan = None;
    }

    /// Hydrogens for the selected atoms (all atoms when none is selected),
    /// to their usual valence.
    pub(crate) fn add_hydrogens_to_selection(&mut self) {
        let view = self.view_directions();
        let mut added = 0;
        self.edit(|s| {
            let placement = add_hydrogens(&s.molecule, &drawn_pairs(s), &s.selected_atoms, view);
            added = placement.added.len();
            if added == 0 {
                return Err("Every atom already has the hydrogens its usual valence asks for.".into());
            }
            Ok(Edited::Changed(placement))
        });
        if added > 0 {
            self.show_status(format!("{added} hydrogen{} added", if added == 1 { "" } else { "s" }));
        }
    }

    /// Removes the hydrogens bonded to the selected atoms (all hydrogens
    /// bonded to one atom when none is selected).
    pub(crate) fn remove_hydrogens_of_selection(&mut self) {
        self.edit(|s| {
            let bonds = drawn_pairs(s);
            let z = &s.molecule.atomic_numbers;
            let terminal = |h: usize| z[h] == 1 && bonds.iter().filter(|&&(a, b)| a == h || b == h).count() == 1;
            let gone: Vec<usize> = if s.selected_atoms.is_empty() {
                (0..z.len()).filter(|&h| terminal(h)).collect()
            } else {
                bonds
                    .iter()
                    .flat_map(|&(a, b)| [(a, b), (b, a)])
                    .filter(|(a, h)| s.selected_atoms.contains(a) && terminal(*h))
                    .map(|(_, h)| h)
                    .chain(s.selected_atoms.iter().copied().filter(|&h| terminal(h)))
                    .collect()
            };
            if gone.is_empty() {
                return Err("There are no hydrogens to remove.".into());
            }
            Ok(Edited::Deleted(apost3dview_build::delete_atoms(&s.molecule, &gone)))
        });
    }

    /// The mirror image (enantiomer): reflected left to right through the
    /// molecule's centre, as seen.
    pub(crate) fn mirror_structure(&mut self) {
        let (right, _) = self.camera.screen_basis();
        if self.edit(|s| {
            let centre = s.molecule.positions.iter().copied().sum::<Vec3>() / s.molecule.positions.len().max(1) as f32;
            Ok(Edited::Positions(s.molecule.positions.iter().map(|p| *p - right * 2.0 * (*p - centre).dot(right)).collect()))
        }) {
            self.show_status("Mirrored: the other enantiomer");
        }
    }

    /// A new structure holding what is picked (methane when Select is),
    /// made the active one: built from nothing, without a file.
    pub(crate) fn new_molecule(&mut self) {
        let fragment = match self.build.add.tool.clone() {
            Tool::Select => element_fragment(6),
            Tool::Element(z) => element_fragment(z),
            Tool::Entry(key) => match self.entry(&key) {
                Some(e) => e.fragment,
                None => element_fragment(6),
            },
        };
        let empty = Molecule { atomic_numbers: Vec::new(), positions: Vec::new(), bonds: Vec::new() };
        let Ok(start) = place(&empty, &[], &fragment, Site::Free(Vec3::ZERO), View::default()) else { return };
        let name = self.unused_structure_name("new molecule");
        let mut structure = LoadedStructure::new(name.clone(), start.molecule, None);
        structure.build = Some(StructureBuild { from: None, saved: false, undo: Vec::new(), redo: Vec::new() });
        self.structures.push(structure);
        self.set_active(self.structures.len() - 1);
        self.show_status(format!("{name}: add to it with the Add section"));
    }

    /// Saves the selected atoms as an own group (exactly one bond must
    /// leave the selection: it says where the group bonds).
    pub(crate) fn save_selection_as_group(&mut self) {
        let Some(active) = self.active_structure else { return };
        let name = self.build.add.group_name.trim().to_string();
        if name.is_empty() {
            self.show_warning("Give the group a name first.");
            return;
        }
        let s = &self.structures[active];
        let fragment = match group_from_selection(&s.molecule, &drawn_pairs(s), &s.selected_atoms) {
            Ok(f) => f,
            Err(why) => {
                self.show_warning(why);
                return;
            }
        };
        let Some(dir) = self.groups_folder() else {
            self.show_warning("No folder for settings on this system");
            return;
        };
        let tab = if self.build.add.group_is_ligand { Tab::Ligands } else { Tab::Mine };
        let kind = if self.build.add.group_is_ligand { FragmentKind::Ligand } else { FragmentKind::Group };
        let entry = LibraryEntry {
            key: format!("mine:{name}"),
            label: name.clone(),
            name: name.clone(),
            tab,
            find: String::new(),
            source: "your own group".into(),
            fragment: Fragment { kind, ..fragment },
        };
        let path = dir.join(format!("{}.xyz", crate::build::file_name_from(&name)));
        match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, group_to_xyz(&entry))) {
            Ok(()) => {
                self.build.add.own = None;
                self.build.add.group_name.clear();
                self.build.add.tab = Tab::Mine;
                self.show_status(format!("{name} saved: it is under Mine"));
            }
            Err(err) => self.show_warning(format!("Could not save {}: {err}", path.display())),
        }
    }

    /// Deletes an own group's file.
    pub(crate) fn delete_own_group(&mut self, key: &str) {
        let Some(name) = key.strip_prefix("mine:") else { return };
        if let Some(dir) = self.groups_folder() {
            let _ = std::fs::remove_file(dir.join(format!("{}.xyz", crate::build::file_name_from(name))));
        }
        if self.build.add.tool == Tool::Entry(key.to_string()) {
            self.pick_tool(Tool::Select);
        }
        self.build.add.own = None;
    }

    /// Where own groups are kept.
    pub(crate) fn groups_folder(&self) -> Option<PathBuf> {
        self.build.add.folder.clone().or_else(groups_dir)
    }

    pub(crate) fn open_groups_folder(&mut self) {
        match self.groups_folder() {
            Some(dir) => self.open_folder(&dir),
            None => self.show_warning("No folder for settings on this system"),
        }
    }

    /// A name not used by any open structure.
    pub(crate) fn unused_structure_name(&self, base: &str) -> String {
        if !self.structures.iter().any(|s| s.label == base) {
            return base.to_string();
        }
        (2..).map(|k| format!("{base} {k}")).find(|name| !self.structures.iter().any(|s| &s.label == name)).unwrap()
    }

    /// The point under the pointer on the plane through the view's centre,
    /// facing the viewer: where empty-space clicks put things.
    pub(crate) fn point_in_space(&self, rect: egui::Rect, at: egui::Pos2) -> Vec3 {
        let aspect = if rect.height() > 0.0 { rect.width() / rect.height() } else { 1.0 };
        let ndc_x = ((at.x - rect.left()) / rect.width()) * 2.0 - 1.0;
        let ndc_y = 1.0 - ((at.y - rect.top()) / rect.height()) * 2.0;
        let (origin, direction) = apost3dview_render::ray_from_ndc(&self.camera, aspect, ndc_x, ndc_y);
        let forward = self.camera.forward();
        let along = direction.dot(forward);
        if along.abs() < 1e-6 {
            return self.camera.target;
        }
        origin + direction * ((self.camera.target - origin).dot(forward) / along)
    }

    /// The preview: the atoms a click would add, outlined in the Build
    /// blue, bonds dashed, the atom under the pointer ringed.
    pub(crate) fn paint_add_preview(&self, painter: &egui::Painter, rect: egui::Rect) {
        let Some(plan) = &self.build.add.plan else { return };
        let Some(active) = self.active_structure else { return };
        let aspect = if rect.height() > 0.0 { rect.width() / rect.height() } else { 1.0 };
        let screen = |p: Vec3| crate::util::project_to_screen(&self.camera, aspect, rect, p);
        let blue = crate::windows::build::BUILD_BLUE;
        let (right, _) = self.camera.screen_basis();
        let radius_px = |p: Vec3, r: f32| -> Option<(egui::Pos2, f32)> {
            let c = screen(p)?;
            let e = screen(p + right * r)?;
            Some((c, (e.x - c.x).abs().max(3.0)))
        };
        // As big as ball-and-stick atoms are drawn.
        let drawn = |z: u32| element_data(z).vdw_radius * self.material.atom_scale;
        let molecule = &self.structures[active].molecule;
        if let Some(a) = plan.atom
            && let Some((c, r)) = radius_px(molecule.positions[a], drawn(molecule.atomic_numbers[a]))
        {
            let color = if plan.action.is_some() { blue } else { egui::Color32::from_rgb(230, 160, 40) };
            painter.circle_stroke(c, r + 3.0, egui::Stroke::new(2.5, color));
        }
        let Some(placement) = &plan.placement else { return };
        let m = &placement.molecule;
        let new: std::collections::HashSet<usize> = placement.added.iter().copied().collect();
        for b in &m.bonds {
            if (new.contains(&b.atom_a) || new.contains(&b.atom_b))
                && let (Some(a), Some(c)) = (screen(m.positions[b.atom_a]), screen(m.positions[b.atom_b]))
            {
                painter.extend(egui::Shape::dashed_line(&[a, c], egui::Stroke::new(2.5, blue.gamma_multiply(0.85)), 6.0, 4.0));
            }
        }
        for &i in &placement.added {
            if let Some((c, r)) = radius_px(m.positions[i], drawn(m.atomic_numbers[i])) {
                painter.circle(c, r, egui::Color32::from_rgba_unmultiplied(40, 130, 200, 40), egui::Stroke::new(2.0, blue));
            }
        }
    }
}

/// The folder of the user's own groups (beside the style presets).
pub(crate) fn groups_dir() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|dir| dir.join("groups"))
}

/// The user's own groups, by name.
fn load_own_groups(dir: &std::path::Path) -> Vec<LibraryEntry> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<LibraryEntry> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xyz")))
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            group_from_xyz(&name, &std::fs::read_to_string(&p).ok()?).ok()
        })
        .collect();
    entries.sort_by_key(|e| e.label.to_lowercase());
    entries
}

/// Test hooks of the Add section.
impl App {
    /// Picks what clicks add, by name: "select", "element:C", or a library
    /// key ("tert-butyl").
    pub fn build_pick(&mut self, what: &str) {
        let tool = if what == "select" {
            Tool::Select
        } else if let Some(symbol) = what.strip_prefix("element:") {
            Tool::Element(apost3dview_core::atomic_number_from_symbol(symbol).expect("a known element"))
        } else {
            Tool::Entry(what.to_string())
        };
        self.pick_tool(tool);
    }

    /// The status-bar line of what a click would do, if any.
    pub fn build_add_hint(&self) -> Option<String> {
        self.build.add.plan.as_ref().map(|p| p.text.clone())
    }

    /// Keeps own groups in `dir` instead of the settings folder.
    pub fn set_groups_folder(&mut self, dir: PathBuf) {
        self.build.add.folder = Some(dir);
        self.build.add.own = None;
    }

    /// Types the name for "Save selection as a group".
    pub fn build_set_group_name(&mut self, name: &str) {
        self.build.add.group_name = name.to_string();
    }

    /// The atom the pointer would find at a window point, if any.
    pub fn atom_at_window_point(&self, x: f32, y: f32) -> Option<usize> {
        self.atom_under(self.last_viewport_rect, egui::pos2(x, y))
    }

    /// The library's keys, in order.
    pub fn library_keys() -> Vec<String> {
        library().iter().map(|e| e.key.clone()).collect()
    }
}
