//! One module per floating tool window, toggled from the toolbar, and
//! what they share (`ToolWindow`).

mod about;
mod apost;
mod apost_enpart;
mod apost_tables;
mod apost_values;
mod appearance;
pub(crate) mod column;
mod coordinates;
mod export;
mod measure;
mod molecule;
mod surfaces;

use crate::app::App;

/// The tool windows, in toolbar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ToolWindow {
    Molecule,
    Surfaces,
    Measure,
    Apost,
    Appearance,
    Export,
    Coordinates,
}

impl ToolWindow {
    pub(crate) const ALL: [ToolWindow; 7] = [
        ToolWindow::Molecule,
        ToolWindow::Surfaces,
        ToolWindow::Measure,
        ToolWindow::Apost,
        ToolWindow::Appearance,
        ToolWindow::Export,
        ToolWindow::Coordinates,
    ];

    /// The windows with a toolbar button (Coordinates opens from Molecule).
    pub(crate) const TOOLBAR: [ToolWindow; 6] =
        [ToolWindow::Molecule, ToolWindow::Surfaces, ToolWindow::Measure, ToolWindow::Apost, ToolWindow::Appearance, ToolWindow::Export];

    /// The window's title.
    pub(crate) fn title(self) -> &'static str {
        match self {
            ToolWindow::Molecule => "Molecule",
            ToolWindow::Surfaces => "Surfaces",
            ToolWindow::Measure => "Measure",
            ToolWindow::Apost => "APOST-3D",
            ToolWindow::Appearance => "Appearance",
            ToolWindow::Export => "Export",
            ToolWindow::Coordinates => "Coordinates",
        }
    }

    /// The window's egui id: its title, and the layout generation, which
    /// "Arrange windows" changes to start every window afresh (egui keeps
    /// a window's size where the app cannot reset it).
    pub(crate) fn id(self, generation: u32) -> egui::Id {
        egui::Id::new(self.title()).with(generation)
    }

    /// The id under which egui keeps whether the window is collapsed.
    pub(crate) fn collapsing_id(self, generation: u32) -> egui::Id {
        self.id(generation).with("collapsing")
    }
}

impl App {
    pub(crate) fn window_open(&self, window: ToolWindow) -> bool {
        match window {
            ToolWindow::Molecule => self.show_molecule,
            ToolWindow::Surfaces => self.show_surfaces,
            ToolWindow::Measure => self.show_measure,
            ToolWindow::Apost => self.show_apost,
            ToolWindow::Appearance => self.show_appearance,
            ToolWindow::Export => self.show_export,
            ToolWindow::Coordinates => self.show_coordinates,
        }
    }

    pub(crate) fn window_flag(&mut self, window: ToolWindow) -> &mut bool {
        match window {
            ToolWindow::Molecule => &mut self.show_molecule,
            ToolWindow::Surfaces => &mut self.show_surfaces,
            ToolWindow::Measure => &mut self.show_measure,
            ToolWindow::Apost => &mut self.show_apost,
            ToolWindow::Appearance => &mut self.show_appearance,
            ToolWindow::Export => &mut self.show_export,
            ToolWindow::Coordinates => &mut self.show_coordinates,
        }
    }

    /// Opens `window`, expanded: asking for a window again (toolbar button,
    /// link, results loaded) brings it back whole even if it was collapsed
    /// (Martí, 2026-10-19). Takes effect at the start of the next frame
    /// (`expand_requested_windows`), so callers need no `egui::Context`.
    pub(crate) fn open_window(&mut self, window: ToolWindow) {
        *self.window_flag(window) = true;
        if !self.windows_to_expand.contains(&window) {
            self.windows_to_expand.push(window);
        }
    }

    pub(crate) fn expand_requested_windows(&mut self, ctx: &egui::Context) {
        for window in std::mem::take(&mut self.windows_to_expand) {
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ctx, window.collapsing_id(self.window_column.generation), true);
            state.set_open(true);
            state.store(ctx);
        }
    }

    /// Whether `title`'s window is open and not collapsed — for tests.
    pub fn window_expanded(&self, ctx: &egui::Context, title: &str) -> bool {
        let Some(window) = ToolWindow::ALL.into_iter().find(|w| w.title() == title) else { return false };
        self.window_open(window) && egui::collapsing_header::CollapsingState::load(ctx, window.collapsing_id(self.window_column.generation)).is_none_or(|s| s.is_open())
    }

    /// Collapses or expands `title`'s window — for tests.
    pub fn set_window_collapsed(&self, ctx: &egui::Context, title: &str, collapsed: bool) {
        let Some(window) = ToolWindow::ALL.into_iter().find(|w| w.title() == title) else { return };
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ctx, window.collapsing_id(self.window_column.generation), true);
        state.set_open(!collapsed);
        state.store(ctx);
    }
}
