//! Color scales for values shown on the molecule, and their legend.
//!
//! Signed values (charges, spin) use a diverging scale: negative red,
//! positive blue, neutral gray at zero, as in electrostatic potential maps.
//! Magnitudes (bond orders, populations, valences) use a sequential scale
//! from light gray to dark blue. Both are the validated data-visualization
//! palette's ramps (sRGB). The legend is drawn the same way in the live
//! view (by the app, with egui) and into exported images (here, onto the
//! pixels), so a figure always says what its colors mean.

use std::sync::OnceLock;

fn hex(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn ramp(stops: &[[f32; 3]; 4], t: f32) -> [f32; 3] {
    let x = t.clamp(0.0, 1.0) * 3.0;
    let i = (x.floor() as usize).min(2);
    mix(stops[i], stops[i + 1], x - i as f32)
}

/// t in [-1, 1]: -1 red, 0 neutral gray, +1 blue.
pub fn diverging(t: f32) -> [f32; 3] {
    if t >= 0.0 {
        ramp(&[hex(0xf0efec), hex(0x86b6ef), hex(0x3987e5), hex(0x184f95)], t)
    } else {
        ramp(&[hex(0xf0efec), hex(0xf2a3a2), hex(0xe34948), hex(0x9c2523)], -t)
    }
}

/// t in [0, 1]: light gray to dark blue.
pub fn sequential(t: f32) -> [f32; 3] {
    ramp(&[hex(0xf0efec), hex(0x86b6ef), hex(0x3987e5), hex(0x184f95)], t)
}

/// The logarithmic diverging scale is linear below max_abs / this (two
/// decades of magnitude get distinct colors).
const LOG_DECADES_FACTOR: f64 = 100.0;

/// The part of a diverging scale its legend shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Half {
    #[default]
    Both,
    /// From zero to +max_abs (white to blue).
    Positive,
    /// From -max_abs to zero (red to white).
    Negative,
}

/// How values map to colors.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "scale", rename_all = "lowercase")]
pub enum ColorScale {
    /// Symmetric around zero, from -max_abs (red) to +max_abs (blue).
    /// `log`: colors follow the logarithm of the magnitude (two decades
    /// below max_abs, linear near zero), for values spanning orders of
    /// magnitude such as interaction energies.
    Diverging {
        max_abs: f64,
        #[serde(default)]
        log: bool,
        /// Which part of the scale the legend shows: both signs, or only
        /// the half the values use (all positive, or all negative). Colors
        /// are the same either way: blue positive, red negative.
        #[serde(default)]
        half: Half,
    },
    /// From min (light) to max (dark).
    Sequential { min: f64, max: f64 },
}

impl ColorScale {
    pub fn color(&self, value: f64) -> [f32; 3] {
        match *self {
            ColorScale::Diverging { max_abs, log, .. } => diverging(if max_abs <= 0.0 {
                0.0
            } else if log {
                let c = max_abs / LOG_DECADES_FACTOR;
                (value.signum() * (1.0 + value.abs() / c).ln() / (1.0 + max_abs / c).ln()) as f32
            } else {
                (value / max_abs) as f32
            }),
            ColorScale::Sequential { min, max } => sequential(if max > min { ((value - min) / (max - min)) as f32 } else { 1.0 }),
        }
    }

    /// The same scale for values multiplied by `factor` (a unit change).
    pub fn scaled(&self, factor: f64) -> Self {
        match *self {
            ColorScale::Diverging { max_abs, log, half } => ColorScale::Diverging { max_abs: max_abs * factor, log, half },
            ColorScale::Sequential { min, max } => ColorScale::Sequential { min: min * factor, max: max * factor },
        }
    }

    /// The colors along the legend bar, left to right, `u` in [0, 1].
    pub fn bar_color(&self, u: f32) -> [f32; 3] {
        match self {
            ColorScale::Diverging { half: Half::Both, .. } => diverging(u * 2.0 - 1.0),
            ColorScale::Diverging { half: Half::Positive, .. } => diverging(u),
            ColorScale::Diverging { half: Half::Negative, .. } => diverging(u - 1.0),
            ColorScale::Sequential { .. } => sequential(u),
        }
    }

    /// The values printed under the bar: left end, middle (diverging over
    /// both signs only: zero), right end.
    pub fn bar_labels(&self, decimals: usize) -> (String, Option<String>, String) {
        match *self {
            ColorScale::Diverging { max_abs, half: Half::Both, .. } => (format_signed(-max_abs, decimals), Some("0".to_string()), format_signed(max_abs, decimals)),
            ColorScale::Diverging { max_abs, half: Half::Positive, .. } => ("0".to_string(), None, format_signed(max_abs, decimals)),
            ColorScale::Diverging { max_abs, half: Half::Negative, .. } => (format_signed(-max_abs, decimals), None, "0".to_string()),
            ColorScale::Sequential { min, max } => (format!("{min:.decimals$}"), None, format!("{max:.decimals$}")),
        }
    }
}

/// A value with an explicit sign and a true minus sign ("+0.29", "\u{2212}0.52");
/// a value that rounds to zero has no sign.
pub fn format_signed(value: f64, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, value.abs());
    if text.chars().all(|c| c == '0' || c == '.') {
        text
    } else if value < 0.0 {
        format!("\u{2212}{text}")
    } else {
        format!("+{text}")
    }
}

/// What a legend shows: a title and its scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Legend {
    pub title: String,
    pub scale: ColorScale,
    pub decimals: usize,
    /// Size relative to the default (1: the title 2.3% of the image's
    /// height), for figures printed small. A panel figure's legend below the
    /// grid follows the captions' size instead.
    pub size: f32,
}

fn legend_font() -> &'static fontdue::Font {
    static FONT: OnceLock<fontdue::Font> = OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = egui::FontDefinitions::default().font_data.get("Ubuntu-Light").expect("egui's default fonts include Ubuntu-Light").font.clone();
        fontdue::Font::from_bytes(bytes.as_ref(), fontdue::FontSettings::default()).expect("valid font")
    })
}

fn blend(pixels: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: [u8; 3], alpha: f32) {
    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height || alpha <= 0.0 {
        return;
    }
    let i = ((y as u32 * width + x as u32) * 4) as usize;
    for c in 0..3 {
        pixels[i + c] = (pixels[i + c] as f32 * (1.0 - alpha) + color[c] as f32 * alpha).round() as u8;
    }
    pixels[i + 3] = pixels[i + 3].max((alpha * 255.0) as u8);
}

fn text_width(text: &str, px: f32) -> f32 {
    text.chars().map(|c| legend_font().metrics(c, px).advance_width).sum()
}

#[allow(clippy::too_many_arguments)]
fn draw_text(pixels: &mut [u8], width: u32, height: u32, text: &str, x: f32, baseline: f32, px: f32, color: [u8; 3]) {
    let font = legend_font();
    let mut pen = x;
    for ch in text.chars() {
        let (m, bitmap) = font.rasterize(ch, px);
        let ox = pen.round() as i32 + m.xmin;
        let oy = baseline.round() as i32 - m.height as i32 - m.ymin;
        for row in 0..m.height {
            for col in 0..m.width {
                let a = bitmap[row * m.width + col] as f32 / 255.0;
                blend(pixels, width, height, ox + col as i32, oy + row as i32, color, a);
            }
        }
        pen += m.advance_width;
    }
}

/// Draws `legend` into RGBA8 `pixels` (row-major, `width` x `height`),
/// bottom left, sized relative to the image so it reads the same at any
/// export resolution. `ink` is the text color (dark on a light background,
/// light on a dark one).
pub fn draw_legend(pixels: &mut [u8], width: u32, height: u32, legend: &Legend, ink: [u8; 3]) {
    let unit = (height as f32 / 860.0).max(0.25) * legend.size;
    let x0 = (40.0 * unit) as u32;
    let y0 = height.saturating_sub((76.0 * unit) as u32);
    draw_legend_at(pixels, width, height, legend, ink, x0, y0, unit);
}

/// Draws `legend` with its color bar's top left corner at (`x0`, `y0`),
/// sized by `unit` (the title is 20 units tall, the bar 300 units wide):
/// the title above the bar, the end values below it.
#[allow(clippy::too_many_arguments)]
pub fn draw_legend_at(pixels: &mut [u8], width: u32, height: u32, legend: &Legend, ink: [u8; 3], x0: u32, y0: u32, unit: f32) {
    let (bar_w, bar_h) = ((300.0 * unit) as u32, (14.0 * unit).max(3.0) as u32);
    for x in 0..bar_w {
        let c = legend.scale.bar_color(x as f32 / (bar_w - 1).max(1) as f32);
        let c = [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8];
        for y in 0..bar_h {
            blend(pixels, width, height, (x0 + x) as i32, (y0 + y) as i32, c, 1.0);
        }
    }
    let (title_px, value_px) = (20.0 * unit, 18.0 * unit);
    // The title wraps to the picture's width (margins as on the left), its
    // lines stacked upward from just above the bar.
    let lines = wrap_title(&legend.title, title_px, width.saturating_sub(2 * x0) as f32);
    for (k, line) in lines.iter().rev().enumerate() {
        draw_text(pixels, width, height, line, x0 as f32, y0 as f32 - 9.0 * unit - k as f32 * 1.2 * title_px, title_px, ink);
    }
    let (lo, mid, hi) = legend.scale.bar_labels(legend.decimals);
    let baseline = (y0 + bar_h) as f32 + 23.0 * unit;
    draw_text(pixels, width, height, &lo, x0 as f32, baseline, value_px, ink);
    if let Some(mid) = mid {
        draw_text(pixels, width, height, &mid, x0 as f32 + bar_w as f32 / 2.0 - text_width(&mid, value_px) / 2.0, baseline, value_px, ink);
    }
    draw_text(pixels, width, height, &hi, x0 as f32 + bar_w as f32 - text_width(&hi, value_px), baseline, value_px, ink);
}

/// `title` in lines no wider than `max` at size `px`, broken at spaces (a
/// word longer than `max` gets a line of its own).
fn wrap_title(title: &str, px: f32, max: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in title.split(' ') {
        match lines.last_mut() {
            Some(line) if text_width(&format!("{line} {word}"), px) <= max => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    /// A one-sided diverging scale colors values as the full one and shows
    /// only its half in the legend.
    #[test]
    fn half_scales() {
        let full = ColorScale::Diverging { max_abs: 0.6, log: false, half: Half::Both };
        let positive = ColorScale::Diverging { max_abs: 0.6, log: false, half: Half::Positive };
        let negative = ColorScale::Diverging { max_abs: 0.6, log: false, half: Half::Negative };
        assert!(close(positive.color(0.3), full.color(0.3)) && close(negative.color(-0.3), full.color(-0.3)));
        assert!(close(positive.bar_color(0.0), diverging(0.0)) && close(positive.bar_color(1.0), diverging(1.0)));
        assert!(close(negative.bar_color(0.0), diverging(-1.0)) && close(negative.bar_color(1.0), diverging(0.0)));
        assert_eq!(positive.bar_labels(2), ("0".to_string(), None, "+0.60".to_string()));
        assert_eq!(negative.bar_labels(2), ("\u{2212}0.60".to_string(), None, "0".to_string()));
    }

    #[test]
    fn long_titles_wrap() {
        let title = "Inter-atomic classical (electrostatic) interaction with Fe1 (TFVC), kcal/mol";
        assert_eq!(wrap_title(title, 20.0, 10_000.0), vec![title.to_string()]);
        let lines = wrap_title(title, 20.0, text_width(title, 20.0) * 0.6);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines.join(" "), title);
        assert!(lines.iter().all(|l| text_width(l, 20.0) <= text_width(title, 20.0) * 0.6));
    }

    #[test]
    fn scales() {
        let d = ColorScale::Diverging { max_abs: 0.5, log: false, half: Half::Both };
        assert!(close(d.color(0.0), hex(0xf0efec)));
        assert!(close(d.color(0.5), hex(0x184f95)));
        assert!(close(d.color(-0.5), hex(0x9c2523)));
        assert!(close(d.color(-2.0), hex(0x9c2523)), "clamped beyond the range");
        let s = ColorScale::Sequential { min: 0.0, max: 2.0 };
        assert!(close(s.color(0.0), hex(0xf0efec)));
        assert!(close(s.color(2.0), hex(0x184f95)));
        let l = ColorScale::Diverging { max_abs: 0.5, log: true, half: Half::Both };
        assert!(close(l.color(0.5), hex(0x184f95)) && close(l.color(-0.5), hex(0x9c2523)) && close(l.color(0.0), hex(0xf0efec)));
        // A value 100 times smaller still gets a third of the way out.
        let t = |c: [f32; 3]| (c[0] - hex(0xf0efec)[0]).abs();
        assert!(t(l.color(-0.005)) > 3.0 * t(d.color(-0.005)));
        assert_eq!(format_signed(-0.518, 2), "\u{2212}0.52");
        assert_eq!(format_signed(0.004, 2), "0.00");
        assert_eq!(format_signed(0.69, 2), "+0.69");
    }

    #[test]
    fn the_legend_draws_its_bar_and_text() {
        let (w, h) = (800u32, 600u32);
        let mut pixels = vec![255u8; (w * h * 4) as usize];
        draw_legend(&mut pixels, w, h, &Legend { title: "Fragment charge (TFVC)".into(), scale: ColorScale::Diverging { max_abs: 0.5, log: false, half: Half::Both }, decimals: 2, size: 1.0 }, [30, 30, 30]);
        let changed = pixels.chunks(4).filter(|p| p[..3] != [255, 255, 255]).count();
        assert!(changed > 2000, "{changed}");
    }
}
