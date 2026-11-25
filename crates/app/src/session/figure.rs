//! The Figures area's part of a session: the Export window's panels,
//! captions and layout.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::app::App;

#[derive(Serialize, Deserialize)]
pub(super) struct FigureState {
    pub(super) settings: crate::panels::FigureSettings,
    #[serde(default)]
    pub(super) panels: Vec<PanelState>,
}

/// A panel: a structure (1-based position in `structures`), the surface
/// shown alone in it, or the surfaces drawn together (`surfaces`, two or
/// more; written only then, so a one-surface panel is saved as before and
/// an older APOST3Dview reads an overlay as the structure as shown), or
/// neither (the structure as shown), its own caption (absent: automatic).
#[derive(Serialize, Deserialize)]
pub(super) struct PanelState {
    pub(super) structure: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) surface: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) surfaces: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) caption: String,
}

impl App {
    /// The figure as saved; `positions` maps each structure to its 1-based
    /// position in the session (`None` for structures not saved).
    pub(super) fn saved_figure(&self, positions: &[Option<usize>]) -> FigureState {
        let figure = &self.render_export.figure;
        FigureState {
            settings: figure.settings.clone(),
            panels: figure
                .panels
                .iter()
                .filter_map(|p| {
                    let (surface, surfaces) = match p.surfaces.as_slice() {
                        [one] => (Some(one.clone()), Vec::new()),
                        several => (None, several.to_vec()),
                    };
                    Some(PanelState { structure: (*positions.get(p.structure)?)?, surface, surfaces, caption: p.caption.clone() })
                })
                .collect(),
        }
    }

    /// Puts the figure back; `opened` maps session positions (0-based) to
    /// the structures opened for them.
    pub(super) fn restore_figure(&mut self, saved: Option<&FigureState>, opened: &BTreeMap<usize, usize>) {
        let figure = &mut self.render_export.figure;
        figure.preview = Default::default();
        match saved {
            Some(state) => {
                figure.settings = state.settings.clone();
                figure.panels = state
                    .panels
                    .iter()
                    .filter_map(|p| {
                        let index = *opened.get(&p.structure.checked_sub(1)?)?;
                        let surfaces = if p.surfaces.is_empty() { p.surface.iter().cloned().collect() } else { p.surfaces.clone() };
                        Some(crate::panels::Panel { structure: index, surfaces, caption: p.caption.clone() })
                    })
                    .collect();
            }
            None => {
                figure.settings = Default::default();
                figure.panels.clear();
            }
        }
    }
}
