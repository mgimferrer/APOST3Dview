//! Panel export: one PNG holding several pictures with the same view, one
//! per surface of a structure (or per structure), each with an editable
//! caption, laid out in a grid ready for a paper. The single image can
//! get a caption the same way (a figure of one panel).
//!
//! Every panel goes through the same offscreen render as a single PNG
//! (labels, ambient occlusion, depth of field, the results display), with
//! only its surface shown; the panels are then put together on the CPU
//! (`apost3dview_render::compose_figure`), so the figure is the same byte
//! for byte each time. The live view and the shown surfaces are put back
//! afterwards. A small preview of the figure is kept up to date in the
//! Export window.

use std::time::{Duration, Instant};

use apost3dview_core::{OrbitalDetail, OrbitalLabel};
use apost3dview_render::{compose_figure, figure_size, ExportSettings, Figure, FigureLayout, Legend, OrbitCamera};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::orbital_sets::APPROXIMATE_FIT_PERCENT;

/// Panel figures hold several pictures, so they start at a lower DPI than
/// single images (`DEFAULT_PUBLICATION_DPI`): nine panels at full page
/// width are already about 4000 pixels wide.
pub(crate) const DEFAULT_PANEL_DPI: u32 = 600;
pub(crate) const DEFAULT_FONT_PT: f32 = 8.0;
pub(crate) const HALF_PAGE_IN: f64 = 3.25;
pub(crate) const FULL_PAGE_IN: f64 = 6.75;
/// Text size in points is turned into pixels at this DPI when the figure
/// is sized in pixels (no DPI of its own).
const PIXEL_SIZING_DPI: f32 = 300.0;
/// Panels are rendered at this multiple and scaled down, like single
/// images.
const PANEL_SUPERSAMPLE: u32 = 2;
/// Width of the preview image, in pixels.
const PREVIEW_WIDTH: f32 = 760.0;
/// How long the view must stay still before the preview is rendered again.
const PREVIEW_SETTLE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ImageMode {
    Single,
    Panels,
}

/// The automatic caption: the surface's name with its number ("OSLO 9 ·
/// 1 CH3 + 2 CH3 · FOLI 2.001"), or the number with the fragments, as
/// figures in papers often have it ("2.001 (CH3 + CH3)").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CaptionStyle {
    Full,
    ValueAndFragment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PanelShape {
    Square,
    /// The shape of the live view.
    View,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FigureBackground {
    White,
    /// The Appearance window's background.
    View,
    Transparent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FigureSizing {
    /// A figure width in inches at a DPI.
    Dpi,
    /// A panel width in pixels.
    Pixels,
}

/// How the figure is made: everything but the panels themselves. Saved in
/// sessions as it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FigureSettings {
    pub(crate) mode: ImageMode,
    pub(crate) captions: bool,
    pub(crate) caption_style: CaptionStyle,
    pub(crate) font_size_pt: f32,
    pub(crate) letters: bool,
    /// Rows and columns as set (`rows` x `columns`); otherwise automatic.
    pub(crate) fixed_layout: bool,
    pub(crate) rows: usize,
    pub(crate) columns: usize,
    pub(crate) panel_shape: PanelShape,
    /// Gap between panels (and around them), in percent of a panel's width.
    pub(crate) gap_percent: f32,
    pub(crate) background: FigureBackground,
    pub(crate) sizing: FigureSizing,
    pub(crate) dpi: u32,
    pub(crate) figure_width_in: f64,
    pub(crate) panel_width_px: u32,
    /// A caption under a single image.
    pub(crate) single_caption: bool,
    /// Its text; empty: automatic.
    pub(crate) single_caption_text: String,
}

impl Default for FigureSettings {
    fn default() -> Self {
        Self {
            mode: ImageMode::Single,
            captions: true,
            caption_style: CaptionStyle::Full,
            font_size_pt: DEFAULT_FONT_PT,
            letters: false,
            fixed_layout: false,
            rows: 3,
            columns: 3,
            panel_shape: PanelShape::Square,
            gap_percent: 2.0,
            background: FigureBackground::White,
            sizing: FigureSizing::Dpi,
            dpi: DEFAULT_PANEL_DPI,
            figure_width_in: FULL_PAGE_IN,
            panel_width_px: 600,
            single_caption: false,
            single_caption_text: String::new(),
        }
    }
}

/// One panel: a surface of a structure shown alone, or the structure as
/// it is shown (`surface` `None`), with its caption (empty: automatic).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Panel {
    pub(crate) structure: usize,
    /// The surface's name in the structure's list.
    pub(crate) surface: Option<String>,
    pub(crate) caption: String,
}

/// What a surface's caption says beside its name: "FOLI 2.001",
/// "occ. 0.890", the number alone ("2.001") and its fragments' formulas
/// ("CH3 + CH3") for the short style.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CaptionFacts {
    pub(crate) detail: String,
    pub(crate) value: String,
    pub(crate) fragments: String,
}

/// "−0.028" with a real minus sign.
fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    match text.strip_prefix('-') {
        Some(rest) => format!("\u{2212}{rest}"),
        None => text,
    }
}

impl CaptionFacts {
    /// For an EFO or OSLO, from its row in the results tables.
    /// `exact`: drawn from APOST-3D's unweighted file, where the fit of the
    /// other file doesn't matter.
    pub(crate) fn of_set_orbital(label: &OrbitalLabel, fragments: String, exact: bool) -> Self {
        let (detail, value) = match &label.detail {
            OrbitalDetail::Oslo { foli: Some(foli), .. } => (format!("FOLI {}", number(*foli)), number(*foli)),
            OrbitalDetail::Efo { negative: true, net, .. } => {
                let net = number(net.unwrap_or(0.0));
                (format!("net {net}"), net)
            }
            OrbitalDetail::Efo { gross, net, .. } => match gross.or(*net) {
                Some(occupation) => (format!("occ. {}", number(occupation)), number(occupation)),
                None => (String::new(), String::new()),
            },
            OrbitalDetail::Oslo { foli: None, .. } => (String::new(), String::new()),
        };
        // The fit only when the orbital drawn is approximate (as the
        // Surfaces window calls it): a poor fit of the fitted file.
        let fit = match label.detail {
            OrbitalDetail::Efo { fit_percent: Some(p), .. } if p < APPROXIMATE_FIT_PERCENT && !exact => format!("fit {p:.0}%"),
            _ => String::new(),
        };
        let detail = [detail, fit].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" \u{b7} ");
        Self { detail, value, fragments }
    }

    /// For a natural orbital, from its occupation.
    pub(crate) fn of_natural_orbital(occupation: f64) -> Self {
        Self { detail: format!("occ. {}", number(occupation)), value: number(occupation), fragments: String::new() }
    }
}

/// The preview of the figure in the Export window, rendered again a
/// moment after anything it shows changes.
#[derive(Default)]
pub(crate) struct FigurePreview {
    pub(crate) texture: Option<egui::TextureHandle>,
    /// Shown larger, in a window of its own.
    pub(crate) large: bool,
    /// Why there is no preview, when there can't be one.
    pub(crate) problem: Option<String>,
    /// The panels last rendered, and what they were rendered for.
    panels: Vec<Vec<u8>>,
    rendered_for: String,
    composed_for: String,
    /// What the scene was last frame, and since when.
    seen: String,
    seen_since: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct PanelExportState {
    pub(crate) settings: FigureSettings,
    pub(crate) panels: Vec<Panel>,
    pub(crate) preview: FigurePreview,
}

/// A panel ready to render: its structure, its surface (index; `None`:
/// as shown), camera, caption, and whether the legend goes into it.
struct PanelJob {
    structure: usize,
    surface: Option<usize>,
    camera: OrbitCamera,
    caption: String,
    legend: bool,
}

/// A panel that can be drawn: its position in the list, its structure,
/// and its surface (index; `None`: the structure as shown).
type ResolvedPanel = (usize, usize, Option<usize>);

/// Everything a figure is made from.
struct FigureJob {
    panels: Vec<PanelJob>,
    layout: FigureLayout,
    supersample: u32,
    background: FigureBackground,
    /// Drawn once below the grid when every panel has the same.
    footer_legend: Option<Legend>,
}

/// Rows and columns for `n` panels: as set, or about square.
pub(crate) fn grid_for(settings: &FigureSettings, n: usize) -> (usize, usize) {
    let n = n.max(1);
    if settings.fixed_layout {
        let columns = settings.columns.max(1);
        (settings.rows.max(n.div_ceil(columns)), columns)
    } else {
        let columns = (n as f64).sqrt().ceil() as usize;
        (n.div_ceil(columns), columns)
    }
}

impl App {
    /// The camera a structure is seen with: the live one for the active
    /// structure (and for one with the same geometry); with "Keep the
    /// view", the live orientation and zoom centered on it; otherwise the
    /// view it was left with, or a view framing it.
    /// Structures aligned on selected atoms (`alignment.rs`) are carried
    /// through their placements instead of centered, so their matched atoms
    /// stay on the same spot.
    pub(crate) fn view_for(&self, index: usize) -> OrbitCamera {
        let Some(structure) = self.structures.get(index) else { return self.camera };
        // Views saved per structure take the projection chosen now (one
        // setting for the whole app).
        let projected = |mut camera: OrbitCamera| {
            camera.orthographic = self.camera.orthographic;
            camera
        };
        let own = crate::alignment::motion(structure.placement.as_ref());
        let active = self.active_structure.and_then(|a| self.structures.get(a).map(|s| (a, s)));
        if let Some((a, current)) = active {
            if a == index {
                return self.camera;
            }
            let same_geometry = crate::util::molecules_share_geometry(&current.molecule, &structure.molecule);
            if same_geometry || self.keep_view {
                if current.placement.is_some() || structure.placement.is_some() {
                    let from = crate::alignment::motion(current.placement.as_ref());
                    // Its own view, exactly, when that is still what is
                    // seen: going back and forth doesn't drift by rounding.
                    if let Some(saved) = structure.view.map(projected)
                        && crate::alignment::same_view(&crate::alignment::carry_camera(&saved, own, from), &self.camera)
                    {
                        return saved;
                    }
                    return crate::alignment::carry_camera(&self.camera, from, own);
                }
                if same_geometry {
                    return self.camera;
                }
                let mut camera = self.camera;
                camera.target = structure.molecule.bounding_sphere().0;
                return camera;
            }
        }
        if let Some(mut view) = structure.view.map(projected) {
            view.world_up = crate::alignment::up_in(own);
            return view;
        }
        let mut camera = self.camera;
        camera.world_up = crate::alignment::up_in(own);
        let (center, radius) = structure.molecule.bounding_sphere();
        // Room for fragment envelopes and the values beside them.
        let has_fragments = structure.apost.as_ref().and_then(|a| a.results.as_ref().ok()).is_some_and(|r| !r.fragments().is_empty());
        camera.frame_bounds(center, if has_fragments { radius + 2.2 } else { radius });
        camera
    }

    /// A structure's name in captions: its file name without extension.
    fn structure_caption(&self, index: usize) -> String {
        let structure = &self.structures[index];
        structure.source_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| structure.label.clone())
    }

    /// The automatic caption of a panel.
    pub(crate) fn default_caption(&self, structure: usize, surface: Option<&str>) -> String {
        let Some(found) = surface.and_then(|label| self.structures[structure].surfaces.iter().find(|s| s.label == label)) else {
            return self.structure_caption(structure);
        };
        let name = match &found.source {
            crate::isosurfaces::SurfaceSource::Cube(path) => path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| found.label.clone()),
            _ => found.label.clone(),
        };
        match (&found.caption_facts, self.render_export.figure.settings.caption_style) {
            (Some(facts), CaptionStyle::Full) if !facts.detail.is_empty() => format!("{name} \u{b7} {}", facts.detail),
            (Some(facts), CaptionStyle::ValueAndFragment) if !facts.value.is_empty() => {
                if facts.fragments.is_empty() { facts.value.clone() } else { format!("{} ({})", facts.value, facts.fragments) }
            }
            _ => name,
        }
    }

    /// The caption a panel gets: its own text, or the automatic one.
    pub(crate) fn panel_caption(&self, panel: &Panel) -> String {
        if panel.caption.trim().is_empty() { self.default_caption(panel.structure, panel.surface.as_deref()) } else { panel.caption.clone() }
    }

    /// The single image's automatic caption: the surface shown (when one
    /// is), else the structure.
    pub(crate) fn default_single_caption(&self) -> String {
        let Some(active) = self.active_structure else { return String::new() };
        let shown: Vec<&str> = self.structures[active].surfaces.iter().filter(|s| s.visible).map(|s| s.label.as_str()).collect();
        self.default_caption(active, if shown.len() == 1 { Some(shown[0]) } else { None })
    }

    /// Fills an empty panel list with the active structure's surfaces (or
    /// the structure itself, when it has none).
    pub(crate) fn fill_panels_if_empty(&mut self) {
        let figure = &self.render_export.figure;
        if !figure.panels.is_empty() {
            return;
        }
        let Some(active) = self.active_structure else { return };
        let surfaces: Vec<String> = self.structures[active].surfaces.iter().map(|s| s.label.clone()).collect();
        let panels = if surfaces.is_empty() {
            vec![Panel { structure: active, surface: None, caption: String::new() }]
        } else {
            surfaces.into_iter().map(|label| Panel { structure: active, surface: Some(label), caption: String::new() }).collect()
        };
        self.render_export.figure.panels = panels;
    }

    /// After structure `index` is closed: its panels go, later ones move up.
    pub(crate) fn forget_structure_panels(&mut self, index: usize) {
        let panels = &mut self.render_export.figure.panels;
        panels.retain(|p| p.structure != index);
        for panel in panels.iter_mut().filter(|p| p.structure > index) {
            panel.structure -= 1;
        }
    }

    /// The panels that can be drawn, and the names of those that can't.
    fn resolved_panels(&self) -> (Vec<ResolvedPanel>, Vec<String>) {
        let mut found = Vec::new();
        let mut missing = Vec::new();
        for (k, panel) in self.render_export.figure.panels.iter().enumerate() {
            let Some(structure) = self.structures.get(panel.structure) else { continue };
            match &panel.surface {
                None => found.push((k, panel.structure, None)),
                Some(label) => match structure.surfaces.iter().position(|s| &s.label == label) {
                    Some(i) => found.push((k, panel.structure, Some(i))),
                    None => missing.push(label.clone()),
                },
            }
        }
        (found, missing)
    }

    /// Text size in pixels, for `scale` (1 = the final image).
    fn text_px(&self, scale: f32) -> f32 {
        let settings = &self.render_export.figure.settings;
        let dpi = match (settings.mode, settings.sizing, self.render_export.preset) {
            (ImageMode::Panels, FigureSizing::Dpi, _) => settings.dpi as f32,
            (ImageMode::Single, _, crate::export::RenderPreset::Dpi) => self.render_export.dpi as f32,
            _ => PIXEL_SIZING_DPI,
        };
        settings.font_size_pt / 72.0 * dpi * scale
    }

    /// The figure width in pixels at full size, to scale the preview by.
    fn full_figure_width(&self) -> f32 {
        let settings = &self.render_export.figure.settings;
        match settings.mode {
            ImageMode::Panels => match settings.sizing {
                FigureSizing::Dpi => (settings.figure_width_in * settings.dpi as f64) as f32,
                FigureSizing::Pixels => {
                    let (_, columns) = grid_for(settings, self.render_export.figure.panels.len());
                    let gap = settings.panel_width_px as f32 * settings.gap_percent / 100.0;
                    columns as f32 * settings.panel_width_px as f32 + (columns + 1) as f32 * gap
                }
            },
            ImageMode::Single => self.resolve_export_settings().width as f32,
        }
    }

    /// What a figure would be made of at `scale` (1 = the final image).
    fn figure_job(&self, scale: f32) -> Result<FigureJob, String> {
        let figure = &self.render_export.figure;
        let settings = &figure.settings;
        let active = self.active_structure.ok_or("Open a structure first.")?;
        let aspect = if self.last_aspect_ratio > 0.0 { self.last_aspect_ratio } else { 1.0 };
        let gap_fraction = (settings.gap_percent / 100.0).max(0.0);

        let (mut panels, layout_grid, panel_width, panel_height, background, captions, letters) = match settings.mode {
            ImageMode::Single => {
                let caption = if settings.single_caption_text.trim().is_empty() { self.default_single_caption() } else { settings.single_caption_text.clone() };
                let job = PanelJob { structure: active, surface: None, camera: self.camera, caption, legend: false };
                let single = self.resolve_export_settings();
                let width = single.width as f32 * scale / (1.0 + 2.0 * gap_fraction);
                let height = width * single.height as f32 / single.width.max(1) as f32;
                let background = if self.render_export.transparent_background { FigureBackground::Transparent } else { FigureBackground::View };
                (vec![job], (1, 1), width, height, background, settings.single_caption, false)
            }
            ImageMode::Panels => {
                let (found, missing) = self.resolved_panels();
                if found.is_empty() {
                    return Err(if missing.is_empty() { "Add a panel first.".to_string() } else { format!("Not found: {}.", missing.join(", ")) });
                }
                let jobs: Vec<PanelJob> = found
                    .iter()
                    .map(|&(k, structure, surface)| PanelJob {
                        structure,
                        surface,
                        camera: self.view_for(structure),
                        caption: self.panel_caption(&figure.panels[k]),
                        legend: false,
                    })
                    .collect();
                let (rows, columns) = grid_for(settings, jobs.len());
                let width = match settings.sizing {
                    FigureSizing::Dpi => (settings.figure_width_in * settings.dpi as f64) as f32 * scale / (columns as f32 + gap_fraction * (columns + 1) as f32),
                    FigureSizing::Pixels => settings.panel_width_px as f32 * scale,
                };
                let height = match settings.panel_shape {
                    PanelShape::Square => width,
                    PanelShape::View => width / aspect,
                };
                (jobs, (rows, columns), width, height, settings.background, settings.captions, settings.letters)
            }
        };

        // One legend below the grid when every panel shows the same one;
        // otherwise each panel keeps its own.
        let legends: Vec<Option<Legend>> = panels.iter().map(|p| self.results_scene(&self.structures[p.structure]).and_then(|s| s.legend)).collect();
        let shared = legends.first().cloned().flatten().filter(|first| legends.iter().all(|l| l.as_ref() == Some(first)));
        let footer_legend = if settings.mode == ImageMode::Panels { shared } else { None };
        for (panel, legend) in panels.iter_mut().zip(&legends) {
            panel.legend = legend.is_some() && footer_legend.is_none();
        }

        let ink = match background {
            FigureBackground::View => self.label_colors().0,
            FigureBackground::White | FigureBackground::Transparent => [0.09, 0.1, 0.12],
        };
        let layout = FigureLayout {
            rows: layout_grid.0,
            columns: layout_grid.1,
            panel_width: (panel_width.floor() as u32).max(16),
            panel_height: (panel_height.floor() as u32).max(16),
            gap: (panel_width * gap_fraction).round() as u32,
            text_px: self.text_px(scale).max(4.0),
            captions,
            letters,
            background: match background {
                FigureBackground::White => Some([255, 255, 255]),
                FigureBackground::View => Some(self.material.background.map(|c| (c * 255.0).round() as u8)),
                FigureBackground::Transparent => None,
            },
            ink: ink.map(|c| (c * 255.0).round() as u8),
        };
        let supersample = if scale < 1.0 { 1 } else { PANEL_SUPERSAMPLE };
        Ok(FigureJob { panels, layout, supersample, background, footer_legend })
    }

    /// The figure's size in pixels, as it would be saved.
    pub(crate) fn figure_output_size(&self) -> Option<(u32, u32)> {
        let job = self.figure_job(1.0).ok()?;
        let captions: Vec<String> = job.panels.iter().map(|p| p.caption.clone()).collect();
        Some(figure_size(&job.layout, &captions, job.footer_legend.is_some()))
    }

    /// Puts structure `index` on the GPU (geometry, selection, measurements,
    /// results display) as the active one, without touching the camera.
    fn load_structure_on_gpu(&mut self, index: usize) {
        self.active_structure = Some(index);
        self.rebuild_geometry();
        self.rebuild_highlights();
        self.rebuild_measurements();
        self.rebuild_results_display();
    }

    /// Renders every panel of `job`, then puts the live view back as it was.
    fn render_panels(&mut self, job: &FigureJob) -> Result<Vec<Vec<u8>>, String> {
        let active = self.active_structure;
        let camera = self.camera;
        let background = self.material.background;
        let visibility: Vec<Vec<bool>> = self.structures.iter().map(|s| s.surfaces.iter().map(|x| x.visible).collect()).collect();

        if job.background == FigureBackground::White {
            // Labels take their outline from the background.
            self.material.background = [1.0, 1.0, 1.0];
        }
        let export_background = job.layout.background.map(|[r, g, b]| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]);
        let settings: ExportSettings = self.export_settings(job.layout.panel_width, job.layout.panel_height, job.supersample, export_background);
        let mut loaded = active;
        let mut result = Ok(Vec::with_capacity(job.panels.len()));
        for panel in &job.panels {
            if loaded != Some(panel.structure) {
                self.load_structure_on_gpu(panel.structure);
                loaded = Some(panel.structure);
            }
            self.camera = panel.camera;
            if let Some(surface) = panel.surface {
                let structure = &mut self.structures[panel.structure];
                for (i, s) in structure.surfaces.iter_mut().enumerate() {
                    s.visible = i == surface;
                }
            } else {
                for (s, &visible) in self.structures[panel.structure].surfaces.iter_mut().zip(&visibility[panel.structure]) {
                    s.visible = visible;
                }
            }
            self.extract_shown_surfaces(panel.structure);
            self.rebuild_isosurface();
            match self.render_rgba_with(panel.structure, &settings, panel.legend) {
                Ok(pixels) => {
                    if let Ok(list) = &mut result {
                        list.push(pixels);
                    }
                }
                Err(err) => {
                    result = Err(err);
                    break;
                }
            }
        }

        for (structure, visible) in self.structures.iter_mut().zip(&visibility) {
            for (s, &v) in structure.surfaces.iter_mut().zip(visible) {
                s.visible = v;
            }
        }
        self.material.background = background;
        self.camera = camera;
        if let Some(active) = active {
            self.load_structure_on_gpu(active);
        }
        self.active_structure = active;
        self.rebuild_isosurface();
        self.results_signature.clear();
        result
    }

    fn compose(job: &FigureJob, panels: &[Vec<u8>]) -> Figure {
        let slices: Vec<&[u8]> = panels.iter().map(Vec::as_slice).collect();
        let captions: Vec<String> = job.panels.iter().map(|p| p.caption.clone()).collect();
        compose_figure(&job.layout, &slices, &captions, job.footer_legend.as_ref())
    }

    /// The figure (panels, or the single image with its caption) at full
    /// size, as RGBA8 rows.
    fn render_figure(&mut self) -> Result<Figure, String> {
        let job = self.figure_job(1.0)?;
        let panels = self.render_panels(&job)?;
        Ok(Self::compose(&job, &panels))
    }

    /// The figure as Export would save it, without a file: width, height
    /// and RGBA8 rows — for tests.
    pub fn render_panel_figure(&mut self) -> Result<(u32, u32, Vec<u8>), String> {
        let figure = self.render_figure()?;
        Ok((figure.width, figure.height, figure.pixels))
    }

    /// Renders the figure and saves it as a PNG (with the DPI when sized
    /// in inches), and its session beside it when asked.
    pub(crate) fn export_figure_png(&mut self) {
        let Some(active) = self.active_structure else {
            self.show_warning("Open a structure first.");
            return;
        };
        let panels = self.render_export.figure.settings.mode == ImageMode::Panels;
        let stem = self.structures[active].source_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "figure".to_string());
        let default_name = if panels { format!("{stem}-panels.png") } else { format!("{stem}.png") };
        let Some(path) = rfd::FileDialog::new().add_filter("PNG image", &["png"]).set_file_name(default_name).save_file() else {
            return;
        };
        let dpi = if panels {
            let settings = &self.render_export.figure.settings;
            (settings.sizing == FigureSizing::Dpi).then_some(settings.dpi)
        } else {
            self.resolve_export_dpi()
        };
        match self.render_figure() {
            Ok(figure) => match Self::write_png(&path, &figure.pixels, figure.width, figure.height, dpi) {
                Ok(()) => self.report_saved_image(&path),
                Err(err) => self.show_warning(format!("Could not write {}: {err}", path.display())),
            },
            Err(err) => self.show_warning(format!("Render failed: {err}")),
        }
    }

    /// Whether the Export window shows a figure preview.
    pub(crate) fn figure_preview_wanted(&self) -> bool {
        let settings = &self.render_export.figure.settings;
        self.show_export && self.active_structure.is_some() && (settings.mode == ImageMode::Panels || settings.single_caption)
    }

    /// Everything the panels' pictures depend on, as text: the preview is
    /// rendered again when it changes.
    fn scene_key(&self, job: &FigureJob) -> String {
        let mut key = format!(
            "{:?}|{:?}|{}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}",
            job.layout.panel_width,
            job.layout.panel_height,
            job.supersample,
            job.background,
            self.ao_enabled,
            self.ao_settings,
            (self.dof_enabled, self.dof_settings),
            self.material,
            self.isosurface_material,
            (self.atom_label_mode, self.atom_label_style.relative_size, self.atom_label_style.text_color),
            (self.measurement_style.font_size, self.measurement_style.decimal_places, self.measurement_style.bold, self.measurement_style.text_color, self.measurement_style.line_color),
            (self.coordinate_unit, &self.color_ranges, self.editing_input()),
            self.last_viewport_height,
        );
        for panel in &job.panels {
            let s = &self.structures[panel.structure];
            let mut hidden: Vec<usize> = s.hidden_atoms.iter().copied().collect();
            hidden.sort_unstable();
            let mut hidden_bonds: Vec<usize> = s.hidden_bonds.iter().copied().collect();
            hidden_bonds.sort_unstable();
            let surfaces: Vec<(&str, bool, crate::isosurfaces::SurfaceSettings)> = s.surfaces.iter().map(|x| (x.label.as_str(), x.visible, x.settings)).collect();
            let measurements: Vec<(Vec<usize>, egui::Vec2)> = s.measurements.iter().map(|m| (m.kind.atoms().to_vec(), m.label_offset)).collect();
            key.push_str(&format!(
                "|{}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{}",
                panel.structure,
                panel.surface,
                panel.camera,
                panel.legend,
                hidden,
                hidden_bonds,
                (&s.bond_styles, s.molecule.bonds.len()),
                surfaces,
                measurements,
                (&s.selected_atoms, &s.selected_bonds, s.apost.as_ref().and_then(|a| a.view.as_ref())),
                s.input_draft.as_ref().map(|d| format!("{:?}", d.fragments)).unwrap_or_default(),
            ));
        }
        key
    }

    /// Keeps the Export window's preview up to date: the panels are
    /// rendered again (small) once the view has stayed still for a moment
    /// after a change, and put together again whenever a caption or the
    /// layout changes.
    pub(crate) fn update_figure_preview(&mut self, ctx: &egui::Context) {
        if !self.figure_preview_wanted() {
            return;
        }
        let full_width = self.full_figure_width().max(1.0);
        let scale = (PREVIEW_WIDTH / full_width).min(1.0);
        let job = match self.figure_job(scale) {
            Ok(job) => job,
            Err(problem) => {
                let preview = &mut self.render_export.figure.preview;
                preview.problem = Some(problem);
                preview.texture = None;
                preview.rendered_for.clear();
                return;
            }
        };
        let key = self.scene_key(&job);
        let busy = ctx.dragged_id().is_some() || ctx.input(|i| i.pointer.any_down());
        let preview = &mut self.render_export.figure.preview;
        preview.problem = None;
        if key != preview.rendered_for {
            if key != preview.seen {
                preview.seen = key.clone();
                preview.seen_since = Some(Instant::now());
            }
            let settled = preview.seen_since.is_some_and(|t| t.elapsed() >= PREVIEW_SETTLE);
            // The first preview doesn't wait.
            let first = preview.panels.is_empty();
            if busy || !(settled || first) {
                ctx.request_repaint_after(PREVIEW_SETTLE);
                return;
            }
            match self.render_panels(&job) {
                Ok(panels) => {
                    let preview = &mut self.render_export.figure.preview;
                    preview.panels = panels;
                    preview.rendered_for = key;
                    preview.composed_for.clear();
                }
                Err(problem) => {
                    self.render_export.figure.preview.problem = Some(problem);
                    return;
                }
            }
        }
        let preview = &mut self.render_export.figure.preview;
        let compose_key = format!("{:?}|{:?}|{:?}", job.layout, job.panels.iter().map(|p| &p.caption).collect::<Vec<_>>(), job.footer_legend);
        if compose_key != preview.composed_for && preview.panels.len() == job.panels.len() {
            let figure = Self::compose(&job, &preview.panels);
            let image = egui::ColorImage::from_rgba_unmultiplied([figure.width as usize, figure.height as usize], &figure.pixels);
            match &mut preview.texture {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => preview.texture = Some(ctx.load_texture("figure_preview", image, egui::TextureOptions::LINEAR)),
            }
            preview.composed_for = compose_key;
        }
    }

    /// While square panels are being prepared, a dashed frame in the live
    /// view around what a panel will hold (the middle of a wide view).
    pub(crate) fn paint_panel_frame(&self, ui: &egui::Ui, rect: egui::Rect) {
        let settings = &self.render_export.figure.settings;
        if !self.show_export || settings.mode != ImageMode::Panels || settings.panel_shape != PanelShape::Square || rect.width() <= rect.height() * 1.01 {
            return;
        }
        let frame = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.height(), rect.height())).shrink(1.0);
        let color = ui.visuals().weak_text_color().gamma_multiply(0.8);
        let corners = [frame.left_top(), frame.right_top(), frame.right_bottom(), frame.left_bottom(), frame.left_top()];
        for pair in corners.windows(2) {
            ui.painter().extend(egui::Shape::dashed_line(pair, egui::Stroke::new(1.0, color), 8.0, 6.0));
        }
    }

    /// The preview's size in pixels, if there is one — for tests.
    pub fn figure_preview_size(&self) -> Option<[usize; 2]> {
        self.render_export.figure.preview.texture.as_ref().map(|t| t.size())
    }

    /// The panels' captions, as they would be drawn — for tests.
    pub fn panel_captions(&self) -> Vec<String> {
        self.render_export.figure.panels.iter().map(|p| self.panel_caption(p)).collect()
    }

    /// Gives panel `index` its own caption (empty: automatic) — for tests.
    pub fn set_panel_caption(&mut self, index: usize, caption: &str) {
        if let Some(panel) = self.render_export.figure.panels.get_mut(index) {
            panel.caption = caption.to_string();
        }
    }

    /// Sizes panels in pixels, `width` wide, with captions and letters or
    /// not — for tests, which keep figures small.
    pub fn set_panel_figure(&mut self, width: u32, captions: bool, letters: bool) {
        let settings = &mut self.render_export.figure.settings;
        settings.sizing = FigureSizing::Pixels;
        settings.panel_width_px = width;
        settings.captions = captions;
        settings.letters = letters;
    }

    /// The live camera: target, distance, orientation (x y z w) — for tests.
    pub fn camera_state(&self) -> ([f32; 3], f32, [f32; 4]) {
        (self.camera.target.to_array(), self.camera.distance, self.camera.orientation.to_array())
    }

    /// The active structure's name — for tests.
    pub fn active_structure_label(&self) -> Option<String> {
        self.active_structure.and_then(|i| self.structures.get(i)).map(|s| s.label.clone())
    }

    /// Ticks or unticks "Keep the view when switching structures" — for
    /// tests.
    pub fn set_keep_view(&mut self, keep: bool) {
        self.keep_view = keep;
    }
}
