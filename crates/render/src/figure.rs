//! Figures of several panels: rendered pictures laid out in a grid, each
//! with a caption below it and, optionally, a letter in its corner, put
//! together on the CPU after readback. The same panels and captions always
//! give the same bytes.
//!
//! Captions and letters use Arimo (SIL Open Font License, bundled in
//! `assets/fonts/` with its license), whose letter widths match Arial's,
//! the font journals usually ask for in figures. Digits inside a chemical
//! formula ("CH3", "H2O") are drawn as subscripts.

use std::sync::OnceLock;

use apost3dview_core::{atomic_number_from_symbol, element_data};

use crate::colormap::{draw_legend_at, Legend};

fn caption_font(bold: bool) -> &'static fontdue::Font {
    static REGULAR: OnceLock<fontdue::Font> = OnceLock::new();
    static BOLD: OnceLock<fontdue::Font> = OnceLock::new();
    let (cell, bytes): (&OnceLock<fontdue::Font>, &[u8]) = if bold {
        (&BOLD, include_bytes!("../assets/fonts/Arimo-Bold.ttf"))
    } else {
        (&REGULAR, include_bytes!("../assets/fonts/Arimo-Regular.ttf"))
    };
    cell.get_or_init(|| fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).expect("the bundled Arimo font is valid"))
}

/// Subscript digits: their size relative to the text, and how far below
/// the baseline they sit.
const SUBSCRIPT_SIZE: f32 = 0.72;
const SUBSCRIPT_DROP: f32 = 0.22;

/// Whether `word` is a chemical formula: element symbols (exact case),
/// each optionally followed by a count, with at least one count. "CH3",
/// "H2O", "BiCl3" are; "OSLO", "def2", "B3LYP", "6" are not.
fn is_formula(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    let element = |symbol: &str| atomic_number_from_symbol(symbol).is_some_and(|z| element_data(z).symbol == symbol);
    let mut i = 0;
    let mut has_count = false;
    while i < chars.len() {
        if !chars[i].is_ascii_uppercase() {
            return false;
        }
        let two = (i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase()).then(|| chars[i..i + 2].iter().collect::<String>());
        match two {
            Some(symbol) if element(&symbol) => i += 2,
            _ if element(&chars[i].to_string()) => i += 1,
            _ => return false,
        }
        while i < chars.len() && chars[i].is_ascii_digit() {
            has_count = true;
            i += 1;
        }
    }
    has_count
}

/// `text` as runs of plain text and subscript digits.
pub fn caption_runs(text: &str) -> Vec<(String, bool)> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    let mut push = |s: &str, sub: bool| match runs.last_mut() {
        Some((last, last_sub)) if *last_sub == sub => last.push_str(s),
        _ => runs.push((s.to_string(), sub)),
    };
    let mut rest = text;
    while !rest.is_empty() {
        let word_len = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
        if word_len == 0 {
            let c = rest.chars().next().unwrap();
            push(&rest[..c.len_utf8()], false);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        let word = &rest[..word_len];
        if is_formula(word) {
            let mut start = 0;
            for (i, c) in word.char_indices() {
                let digit = c.is_ascii_digit();
                let previous_digit = i > 0 && word.as_bytes()[i - 1].is_ascii_digit();
                if i > 0 && digit != previous_digit {
                    push(&word[start..i], previous_digit);
                    start = i;
                }
            }
            push(&word[start..], word.as_bytes()[word.len() - 1].is_ascii_digit());
        } else {
            push(word, false);
        }
        rest = &rest[word_len..];
    }
    runs
}

/// Width of `text` in pixels at `px`, subscripts included.
pub fn text_width(text: &str, px: f32, bold: bool) -> f32 {
    let font = caption_font(bold);
    caption_runs(text)
        .iter()
        .map(|(run, sub)| {
            let size = if *sub { px * SUBSCRIPT_SIZE } else { px };
            run.chars().map(|c| font.metrics(c, size).advance_width).sum::<f32>()
        })
        .sum()
}

/// Paints `color` with coverage `alpha` over a straight-alpha RGBA pixel.
fn blend_over(pixels: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: [u8; 3], alpha: f32) {
    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height || alpha <= 0.0 {
        return;
    }
    let i = ((y as u32 * width + x as u32) * 4) as usize;
    let dst_a = pixels[i + 3] as f32 / 255.0;
    let out_a = alpha + dst_a * (1.0 - alpha);
    for c in 0..3 {
        let mixed = (color[c] as f32 * alpha + pixels[i + c] as f32 * dst_a * (1.0 - alpha)) / out_a.max(1e-6);
        pixels[i + c] = mixed.round().clamp(0.0, 255.0) as u8;
    }
    pixels[i + 3] = (out_a * 255.0).round() as u8;
}

/// Draws one line of `text` with its left end at `x` and its baseline at
/// `baseline`.
#[allow(clippy::too_many_arguments)]
fn draw_line(pixels: &mut [u8], width: u32, height: u32, text: &str, x: f32, baseline: f32, px: f32, bold: bool, ink: [u8; 3]) {
    let font = caption_font(bold);
    let mut pen = x;
    for (run, sub) in caption_runs(text) {
        let (size, drop) = if sub { (px * SUBSCRIPT_SIZE, px * SUBSCRIPT_DROP) } else { (px, 0.0) };
        for c in run.chars() {
            let (m, bitmap) = font.rasterize(c, size);
            let ox = pen.round() as i32 + m.xmin;
            let oy = (baseline + drop).round() as i32 - m.height as i32 - m.ymin;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bitmap[row * m.width + col] as f32 / 255.0;
                    blend_over(pixels, width, height, ox + col as i32, oy + row as i32, ink, a);
                }
            }
            pen += m.advance_width;
        }
    }
}

/// `text` broken into lines no wider than `max_width`: at the " · "
/// separators first, then at spaces. A single word wider than that stays
/// whole.
pub fn wrap_caption(text: &str, px: f32, max_width: f32) -> Vec<String> {
    let fits = |s: &str| text_width(s, px, false) <= max_width;
    if text.is_empty() || fits(text) {
        return vec![text.to_string()];
    }
    let greedy = |parts: Vec<&str>, separator: &str| {
        let mut lines: Vec<String> = Vec::new();
        for part in parts {
            match lines.last_mut() {
                Some(line) if fits(&format!("{line}{separator}{part}")) => {
                    line.push_str(separator);
                    line.push_str(part);
                }
                _ => lines.push(part.to_string()),
            }
        }
        lines
    };
    greedy(text.split(" \u{b7} ").collect(), " \u{b7} ")
        .into_iter()
        .flat_map(|line| if fits(&line) { vec![line] } else { greedy(line.split(' ').collect(), " ") })
        .collect()
}

/// How a figure is laid out, all sizes in pixels of the final image.
#[derive(Debug, Clone, PartialEq)]
pub struct FigureLayout {
    pub columns: usize,
    pub rows: usize,
    pub panel_width: u32,
    pub panel_height: u32,
    /// Between panels, and around the whole figure.
    pub gap: u32,
    /// Text size of captions and letters (and of the legend below).
    pub text_px: f32,
    /// A caption under every panel.
    pub captions: bool,
    /// "(a)", "(b)", ... at the top left of every panel.
    pub letters: bool,
    /// `None`: transparent.
    pub background: Option<[u8; 3]>,
    /// Text color.
    pub ink: [u8; 3],
}

/// A composed figure, RGBA8 rows.
pub struct Figure {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// "(a)" for panel 0, ..., "(z)", then "(aa)".
pub fn panel_letter(index: usize) -> String {
    let letter = |i: usize| char::from(b'a' + (i % 26) as u8);
    if index < 26 { format!("({})", letter(index)) } else { format!("({}{})", letter(index / 26 - 1), letter(index)) }
}

struct Plan {
    width: u32,
    height: u32,
    line_height: f32,
    caption_height: u32,
    lines: Vec<Vec<String>>,
    legend_top: Option<u32>,
}

/// The legend strip below the grid: its height for a text size of `px`,
/// and the size unit the legend is drawn with (its title is 20 units).
fn legend_unit(px: f32) -> f32 {
    px / 20.0
}

fn legend_height(px: f32) -> u32 {
    (80.0 * legend_unit(px)).ceil() as u32
}

fn plan(layout: &FigureLayout, captions: &[String], legend: bool) -> Plan {
    let px = layout.text_px;
    let line_height = (px * 1.25).round();
    let lines: Vec<Vec<String>> = if layout.captions {
        captions.iter().map(|c| wrap_caption(c, px, layout.panel_width as f32 - px * 0.3)).collect()
    } else {
        Vec::new()
    };
    let max_lines = lines.iter().map(Vec::len).max().unwrap_or(0);
    let caption_height = if max_lines > 0 { (px * 0.3 + max_lines as f32 * line_height).ceil() as u32 } else { 0 };
    let columns = layout.columns.max(1) as u32;
    let rows = layout.rows.max(1) as u32;
    let width = 2 * layout.gap + columns * layout.panel_width + (columns - 1) * layout.gap;
    let grid_height = 2 * layout.gap + rows * (layout.panel_height + caption_height) + (rows - 1) * layout.gap;
    let (height, legend_top) = if legend { (grid_height + legend_height(px), Some(grid_height)) } else { (grid_height, None) };
    Plan { width, height, line_height, caption_height, lines, legend_top }
}

/// The size of the figure `compose_figure` would make, without making it.
pub fn figure_size(layout: &FigureLayout, captions: &[String], legend: bool) -> (u32, u32) {
    let plan = plan(layout, captions, legend);
    (plan.width, plan.height)
}

/// Lays `panels` (RGBA8, each `panel_width` x `panel_height`, in reading
/// order) out in the grid of `layout`, the last row centered, each with
/// its caption below (`captions[i]`, empty for none) and its letter, and
/// `legend` once below the grid.
pub fn compose_figure(layout: &FigureLayout, panels: &[&[u8]], captions: &[String], legend: Option<&Legend>) -> Figure {
    let plan = plan(layout, captions, legend.is_some());
    let (width, height) = (plan.width, plan.height);
    let mut pixels = match layout.background {
        Some([r, g, b]) => [r, g, b, 255].repeat((width * height) as usize),
        None => vec![0u8; (width * height * 4) as usize],
    };
    let (pw, ph) = (layout.panel_width, layout.panel_height);
    let columns = layout.columns.max(1);
    let px = layout.text_px;
    for (k, panel) in panels.iter().enumerate() {
        let (row, column) = (k / columns, k % columns);
        let in_row = columns.min(panels.len() - row * columns) as u32;
        let offset = (columns as u32 - in_row) * (pw + layout.gap) / 2;
        let x = layout.gap + offset + column as u32 * (pw + layout.gap);
        let y = layout.gap + row as u32 * (ph + plan.caption_height + layout.gap);
        for r in 0..ph.min(height.saturating_sub(y)) {
            let src = (r * pw * 4) as usize;
            let dst = (((y + r) * width + x) * 4) as usize;
            let n = (pw.min(width - x) * 4) as usize;
            if let Some(source) = panel.get(src..src + n) {
                pixels[dst..dst + n].copy_from_slice(source);
            }
        }
        if layout.letters {
            let letter_px = px * 1.05;
            let ascent = caption_font(true).horizontal_line_metrics(letter_px).map_or(letter_px * 0.9, |m| m.ascent);
            draw_line(&mut pixels, width, height, &panel_letter(k), (x as f32 + px * 0.3).round(), (y as f32 + px * 0.25 + ascent).round(), letter_px, true, layout.ink);
        }
        if let Some(lines) = plan.lines.get(k) {
            let ascent = caption_font(false).horizontal_line_metrics(px).map_or(px * 0.9, |m| m.ascent);
            for (j, line) in lines.iter().enumerate() {
                let w = text_width(line, px, false);
                let left = (x as f32 + (pw as f32 - w) / 2.0).round();
                let baseline = (y as f32 + ph as f32 + px * 0.3 + j as f32 * plan.line_height + ascent).round();
                draw_line(&mut pixels, width, height, line, left, baseline, px, false, layout.ink);
            }
        }
    }
    if let (Some(legend), Some(top)) = (legend, plan.legend_top) {
        let unit = legend_unit(px);
        let x0 = ((width as f32 - 300.0 * unit) / 2.0).max(0.0) as u32;
        draw_legend_at(&mut pixels, width, height, legend, layout.ink, x0, top + (30.0 * unit) as u32, unit);
    }
    Figure { width, height, pixels }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colormap::ColorScale;

    fn layout(columns: usize, rows: usize) -> FigureLayout {
        FigureLayout { columns, rows, panel_width: 120, panel_height: 100, gap: 4, text_px: 14.0, captions: true, letters: true, background: Some([255, 255, 255]), ink: [20, 20, 20] }
    }

    #[test]
    fn formulas_get_subscripts_and_nothing_else_does() {
        let subscripts = |text: &str| caption_runs(text).into_iter().filter(|(_, sub)| *sub).map(|(s, _)| s).collect::<Vec<_>>();
        assert_eq!(subscripts("OSLO 9 \u{b7} 1 CH3 + 2 CH3 \u{b7} FOLI 2.001"), ["3", "3"]);
        assert_eq!(subscripts("2.001 (CH3 + CH3)"), ["3", "3"]);
        assert_eq!(subscripts("H2O-HOMO"), ["2"]);
        assert_eq!(subscripts("BiCl3 def2-TZVPP"), ["3"]);
        assert_eq!(subscripts("Fe2(CO)9"), ["2"]);
        for plain in ["EFO 10 \u{3b1} \u{b7} 1 Fe \u{b7} occ. 0.890", "B3LYP", "CAS44", "MO 12 (beta)", "NO 5 (CASSCF)", "6-31G", "Cp2", "fit 48%", ""] {
            assert!(subscripts(plain).is_empty(), "{plain}");
        }
        // Nothing is lost or reordered.
        let text = "OSLO 9 \u{b7} 1 CH3 + 2 CH3";
        assert_eq!(caption_runs(text).into_iter().map(|(s, _)| s).collect::<String>(), text);
    }

    #[test]
    fn long_captions_wrap_at_separators_first() {
        let text = "OSLO 9 \u{b7} 1 CH3 + 2 CH3 \u{b7} FOLI 2.001";
        let width = text_width(text, 20.0, false);
        assert_eq!(wrap_caption(text, 20.0, width + 1.0), [text]);
        let lines = wrap_caption(text, 20.0, width * 0.7);
        assert_eq!(lines, ["OSLO 9 \u{b7} 1 CH3 + 2 CH3", "FOLI 2.001"]);
        let lines = wrap_caption(text, 20.0, text_width("1 CH3 + 2 CH3", 20.0, false) * 0.8);
        assert!(lines.len() > 3 && lines.iter().all(|l| !l.is_empty()), "{lines:?}");
    }

    #[test]
    fn panels_are_laid_out_with_the_last_row_centered() {
        let l = layout(3, 2);
        let red = [200u8, 0, 0, 255].repeat(120 * 100);
        let panels: Vec<&[u8]> = vec![&red; 5];
        let captions: Vec<String> = (0..5).map(|i| format!("OSLO {i}")).collect();
        let figure = compose_figure(&l, &panels, &captions, None);
        assert_eq!((figure.width, figure.height), figure_size(&l, &captions, false));
        assert_eq!(figure.width, 4 * 2 + 3 * 120 + 2 * 4);
        let at = |x: u32, y: u32| &figure.pixels[((y * figure.width + x) * 4) as usize..][..4];
        // First row starts at the gap; the second, two panels, is centered.
        assert_eq!(at(4 + 60, 4 + 60), [200, 0, 0, 255]);
        let caption_height = figure.height / 2 - 4 - 100 - 2;
        let second = 4 + 100 + caption_height + 4;
        assert_eq!(at(4 + 60, second + 60), [255, 255, 255, 255], "no panel at the left of the centered row");
        assert_eq!(at(4 + (124 / 2) + 60, second + 60), [200, 0, 0, 255]);
        // Captions and letters are drawn in ink.
        let inked = figure.pixels.chunks(4).filter(|p| p[0] < 128 && p[1] < 128).count();
        assert!(inked > 200, "{inked}");
        // The same input gives the same bytes.
        assert_eq!(compose_figure(&l, &panels, &captions, None).pixels, figure.pixels);
    }

    #[test]
    fn a_transparent_figure_keeps_its_text_ink_colored() {
        let mut l = layout(1, 1);
        l.background = None;
        l.letters = false;
        let clear = vec![0u8; 120 * 100 * 4];
        let figure = compose_figure(&l, &[&clear], &["CH3".to_string()], None);
        let solid: Vec<&[u8]> = figure.pixels.chunks(4).filter(|p| p[3] == 255).collect();
        assert!(!solid.is_empty() && solid.iter().all(|p| p[..3] == [20, 20, 20]), "straight alpha: full-coverage text is the ink color");
        assert!(figure.pixels.chunks(4).any(|p| p[3] > 0 && p[3] < 255), "edges antialiased");
    }

    #[test]
    fn the_legend_goes_once_below_the_grid() {
        let l = layout(2, 1);
        let gray = [128u8, 128, 128, 255].repeat(120 * 100);
        let legend = Legend { title: "Fragment charge (TFVC)".into(), scale: ColorScale::Diverging { max_abs: 0.5, log: false }, decimals: 2 };
        let captions = vec![String::new(), String::new()];
        let with = compose_figure(&l, &[&gray, &gray], &captions, Some(&legend));
        let without = compose_figure(&l, &[&gray, &gray], &captions, None);
        assert!(with.height > without.height);
        let below = &with.pixels[(without.height * with.width * 4) as usize..];
        assert!(below.chunks(4).filter(|p| p[..3] != [255, 255, 255]).count() > 500);
    }

    #[test]
    fn letters_run_past_z() {
        assert_eq!(panel_letter(0), "(a)");
        assert_eq!(panel_letter(8), "(i)");
        assert_eq!(panel_letter(26), "(aa)");
    }
}
