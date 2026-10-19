//! The tool windows' column (Martí, 2026-10-19): every tool window opens at
//! the top right, all the same width, one below the other in toolbar order,
//! so nothing lands in the middle of the molecule and nothing overlaps.
//! Collapsing a window leaves its title bar, and the ones below move up.
//! Each window keeps its height (its content scrolls inside) until the user
//! resizes it; when the expanded windows don't fit, they share the column's
//! height in proportion. A window dragged by its title bar leaves the
//! column and stays where it was put; "Arrange windows" (toolbar, shown
//! while some window is out) puts them all back.

use std::collections::{HashMap, HashSet};

use egui::{Pos2, Rect};

use super::ToolWindow;
use crate::app::App;

/// The width every window starts at.
pub(crate) const COLUMN_WIDTH: f32 = 400.0;
/// Around the column, and between windows.
const MARGIN: f32 = 8.0;
const GAP: f32 = 6.0;
/// The least height a window is squeezed to when the column is full.
const MIN_HEIGHT: f32 = 120.0;
/// A collapsed window's height before it has been drawn once.
const TITLE_BAR_HEIGHT: f32 = 34.0;
/// A window's frame and title bar around its content, before measured.
const DEFAULT_OVERHEAD: f32 = 48.0;
/// How near a window's border a press must start to be a resize.
const BORDER_GRAB: f32 = 10.0;

/// Where a window goes this frame: its top, and its whole height when
/// expanded (`None` when collapsed).
#[derive(Clone, Copy)]
struct Slot {
    top: f32,
    height: Option<f32>,
}

#[derive(Default)]
pub(crate) struct WindowColumn {
    /// Between the toolbar and the status bar, this frame.
    pub(crate) top: f32,
    pub(crate) bottom: f32,
    /// Where each open window was last drawn.
    rects: HashMap<ToolWindow, Rect>,
    /// Each window's frame and title bar around its content, as measured.
    overheads: HashMap<ToolWindow, f32>,
    /// The height each window had when the user last resized it.
    preferred: HashMap<ToolWindow, f32>,
    /// This frame's places of the windows in the column.
    plan: HashMap<ToolWindow, Slot>,
    /// The window whose border the user is dragging.
    resizing: Option<ToolWindow>,
    /// Windows the user dragged out of the column.
    detached: HashSet<ToolWindow>,
}

impl ToolWindow {
    /// How tall the window first opens (its content scrolls inside).
    fn default_height(self) -> f32 {
        match self {
            ToolWindow::Molecule => 280.0,
            ToolWindow::Surfaces => 520.0,
            ToolWindow::Measure => 300.0,
            ToolWindow::Apost => 560.0,
            ToolWindow::Appearance => 460.0,
            ToolWindow::Export => 420.0,
            ToolWindow::Coordinates => 360.0,
        }
    }

    fn expanded(self, ctx: &egui::Context) -> bool {
        egui::collapsing_header::CollapsingState::load(ctx, self.collapsing_id()).is_none_or(|s| s.is_open())
    }
}

impl App {
    /// Places the open windows of the column for this frame (after the
    /// toolbar and status bar, before the windows).
    pub(crate) fn plan_window_column(&mut self, ctx: &egui::Context) {
        let docked: Vec<ToolWindow> =
            ToolWindow::ALL.into_iter().filter(|w| self.window_open(*w) && !self.window_column.detached.contains(w)).collect();
        let column = &mut self.window_column;
        let expanded: Vec<bool> = docked.iter().map(|w| w.expanded(ctx)).collect();
        let collapsed_height = |w: &ToolWindow| column.rects.get(w).map_or(TITLE_BAR_HEIGHT, |r| r.height().min(TITLE_BAR_HEIGHT * 1.5));
        let preferred = |w: &ToolWindow| column.preferred.get(w).copied().unwrap_or_else(|| w.default_height());
        let gaps = GAP * docked.len().saturating_sub(1) as f32;
        let fixed: f32 = docked.iter().zip(&expanded).filter(|(_, e)| !**e).map(|(w, _)| collapsed_height(w)).sum();
        let wanted: f32 = docked.iter().zip(&expanded).filter(|(_, e)| **e).map(|(w, _)| preferred(w)).sum();
        let available = (column.bottom - column.top - 2.0 * MARGIN - gaps - fixed).max(0.0);

        // Never below MIN_HEIGHT, unless so many windows are open that even
        // that would run past the bottom: then they share what there is.
        let wishes: Vec<f32> = docked.iter().zip(&expanded).filter(|(_, e)| **e).map(|(w, _)| preferred(w)).collect();
        let least = MIN_HEIGHT.min(available / wishes.len().max(1) as f32);
        let scale = if wanted > available { fitting_scale(&wishes, available, least) } else { 1.0 };

        let mut plan = HashMap::new();
        let mut y = column.top + MARGIN;
        for (w, expanded) in docked.iter().zip(expanded) {
            let height = expanded.then(|| (preferred(w) * scale).max(least));
            plan.insert(*w, Slot { top: y, height });
            y += height.unwrap_or_else(|| collapsed_height(w)) + GAP;
        }
        column.plan = plan;
    }

    /// `window`'s egui window: in its place in the column unless the user
    /// dragged it out, its content scrolling inside.
    pub(crate) fn column_window<'a>(&self, ctx: &egui::Context, window: ToolWindow, open: &'a mut bool) -> egui::Window<'a> {
        // Movable from anywhere, in the column or not: in egui's title-bar
        // mode a window's stored position overrides `current_pos` (egui
        // 0.36), so the column could not place it, and a drag that takes a
        // window out of the column would stop following the pointer.
        let builder = egui::Window::new(window.title())
            .open(open)
            .default_width(COLUMN_WIDTH)
            .default_height(window.default_height())
            .resizable(true)
            .scroll([true, true])
            .drag_area(egui::containers::WindowDrag::Anywhere);
        let column = &self.window_column;
        let Some(slot) = column.plan.get(&window) else { return builder };
        let width = column.rects.get(&window).map_or(COLUMN_WIDTH, |r| r.width());
        // A drag takes it out of the column (`record_column_window`).
        let builder = builder.current_pos(Pos2::new(ctx.content_rect().right() - MARGIN - width, slot.top)).constrain(false);
        match slot.height {
            // The content gets the slot's height, except while the user drags
            // the window's border (then the window follows the pointer).
            Some(height) if column.resizing != Some(window) => {
                let overhead = column.overheads.get(&window).copied().unwrap_or(DEFAULT_OVERHEAD);
                let content = (height - overhead).max(0.0);
                builder.min_height(content).max_height(content)
            }
            _ => builder,
        }
    }

    /// After `window` was drawn (`response`, if open): measures it, notices
    /// the user resizing it (its new height is kept) or dragging it out of
    /// the column.
    pub(crate) fn record_column_window(&mut self, ctx: &egui::Context, window: ToolWindow, response: Option<egui::Response>) {
        let expanded = window.expanded(ctx);
        let column = &mut self.window_column;
        let Some(response) = response else {
            column.rects.remove(&window);
            return;
        };
        let rect = response.rect;
        let previous = column.rects.insert(window, rect);
        let Some(slot) = column.plan.get(&window).copied() else { return };
        // A real drag (not a click on the window's background) takes the
        // window out of the column.
        if response.dragged() && ctx.input(|i| i.pointer.is_decidedly_dragging()) {
            column.detached.insert(window);
            return;
        }
        // A press starting on this window's border is the user resizing it.
        let (pressed, down, origin) = ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.press_origin()));
        if pressed {
            let near_border = |r: Rect, p: Pos2| r.expand(BORDER_GRAB).contains(p) && !r.shrink(BORDER_GRAB).contains(p);
            if let (Some(previous), Some(origin)) = (previous, origin)
                && expanded
                && near_border(previous, origin)
            {
                column.resizing = Some(window);
            }
        }
        if column.resizing == Some(window) {
            column.preferred.insert(window, rect.height());
            if !down {
                column.resizing = None;
            }
        } else if let Some(height) = slot.height.filter(|_| expanded) {
            // How much frame and title bar the window adds to its content.
            let content = (height - column.overheads.get(&window).copied().unwrap_or(DEFAULT_OVERHEAD)).max(0.0);
            column.overheads.insert(window, rect.height() - content);
        }
        if previous.is_none_or(|p| p.size() != rect.size()) {
            ctx.request_repaint();
        }
    }

    /// Whether some window was dragged out of the column.
    pub(crate) fn windows_out_of_column(&self) -> bool {
        !self.window_column.detached.is_empty()
    }

    /// Puts every window back in the column.
    pub(crate) fn arrange_windows(&mut self) {
        self.window_column.detached.clear();
    }

    /// Where `title`'s window was last drawn — for tests.
    pub fn window_rect(&self, title: &str) -> Option<Rect> {
        let window = ToolWindow::ALL.into_iter().find(|w| w.title() == title)?;
        self.window_column.rects.get(&window).copied()
    }
}

/// The factor for the wished heights so that, each raised to at least
/// `least`, they add up to `available`: windows at the minimum take their
/// share first, the others divide the rest in proportion.
fn fitting_scale(wishes: &[f32], available: f32, least: f32) -> f32 {
    let mut at_least: Vec<bool> = vec![false; wishes.len()];
    loop {
        let fixed = least * at_least.iter().filter(|m| **m).count() as f32;
        let free: f32 = wishes.iter().zip(&at_least).filter(|(_, m)| !**m).map(|(w, _)| w).sum();
        if free <= 0.0 {
            return 0.0;
        }
        let scale = ((available - fixed) / free).max(0.0);
        let mut changed = false;
        for (wish, minimum) in wishes.iter().zip(at_least.iter_mut()) {
            if !*minimum && wish * scale < least {
                *minimum = true;
                changed = true;
            }
        }
        if !changed {
            return scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fitting_scale;

    #[test]
    fn wished_heights_fit_the_column() {
        let wishes = [280.0, 520.0, 300.0, 560.0, 460.0, 420.0];
        let (available, least) = (800.0, 120.0);
        let scale = fitting_scale(&wishes, available, least);
        let total: f32 = wishes.iter().map(|w| (w * scale).max(least)).sum();
        assert!((total - available).abs() < 0.01, "{total}");
        // Room for all: nothing changes.
        assert!(fitting_scale(&[200.0, 300.0], 600.0, least) >= 1.0);
    }
}
