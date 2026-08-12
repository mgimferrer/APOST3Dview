//! Gaussian `.cube` file parsing: a molecule (same atom-line format as
//! `.fchk`, just inline) plus a scalar field sampled on a regular 3D grid
//! (an orbital, a density, ...). Format, confirmed against real
//! APOST-3D-generated cube files:
//!
//! ```text
//! <comment>
//! <comment>
//! NATOMS  ORIGIN_X  ORIGIN_Y  ORIGIN_Z          (NATOMS<0: multi-dataset, see below)
//! NX  STEP_X_X  STEP_X_Y  STEP_X_Z              (grid dims + step vectors, one line
//! NY  STEP_Y_X  STEP_Y_Y  STEP_Y_Z               per axis — general 3x3 basis is
//! NZ  STEP_Z_X  STEP_Z_Y  STEP_Z_Z               legal, though almost all real files
//!                                                are axis-aligned/diagonal)
//! <NATOMS lines>: ATOMIC_NUMBER  CHARGE  X  Y  Z
//! [if NATOMS<0: one line, M ORBITAL_INDEX_1 ... ORBITAL_INDEX_M]
//! <grid values>: NX*NY*NZ (*M if multi-dataset) whitespace-separated
//!                floats, any number per line, X slowest / Z fastest,
//!                and — for a multi-dataset file — the M dataset values
//!                for a given grid point written consecutively before
//!                moving to the next point.
//! ```
//!
//! Everything (origin, step vectors, atom positions) is in Bohr by
//! convention; converted to angstrom here so the rest of the app never has
//! to think about it, same as `.fchk` parsing already does.
//!
//! Multi-dataset cubes (several orbitals stacked in one file) parse
//! correctly but only the first dataset is kept — none of the test files
//! this was built against use more than one, and `ScalarGrid` staying
//! "always exactly one value per grid point" keeps every downstream
//! consumer (interpolation, marching cubes) simple. Worth revisiting if a
//! real multi-dataset file shows up.
//!
//! Two Fortran-output quirks confirmed against real APOST-3D cube files
//! (NATOMS was *positive* in every file this was tested against, so
//! detection below can't rely on its sign the way the textbook spec
//! describes):
//! - The `M ORBITAL_INDEX...` line before the grid data is present
//!   whenever the atom section is followed by a line of bare integers
//!   (real grid values are always written with a decimal point) —
//!   detected by content, not by NATOMS's sign.
//! - Extremely small values (e.g. deep in an orbital's decaying tail,
//!   ~1e-150) drop the exponent's `E` when the fixed-width field can't
//!   fit both — `0.78396-167` means `0.78396E-167`. `parse_fortran_f32`
//!   below repairs this (and the `D`-exponent variant some Fortran codes
//!   use instead of `E`) before falling back to a normal float parse.

use std::fs;
use std::io;
use std::path::Path;

use glam::Vec3;

use crate::molecule::Molecule;
use crate::units::ANGSTROM_PER_BOHR;

/// A scalar field sampled on a regular (possibly non-axis-aligned) 3D
/// grid — an orbital or density from a `.cube` file.
#[derive(Debug, Clone)]
pub struct ScalarGrid {
    /// Grid origin (the position of grid point (0,0,0)), angstrom.
    pub origin: Vec3,
    /// Number of grid points along each axis.
    pub dims: [usize; 3],
    /// Step vector for each axis, angstrom — the world-space displacement
    /// per unit grid index along that axis. Diagonal (axis-aligned) for
    /// essentially every real file, but stored as full vectors since nothing
    /// stops a `.cube` file from using a sheared/rotated grid basis.
    pub steps: [Vec3; 3],
    /// `dims[0] * dims[1] * dims[2]` values, x-major order (x slowest, z
    /// fastest) — the same order the cube file itself stores them in.
    pub values: Vec<f32>,
}

impl ScalarGrid {
    #[inline]
    pub fn index(&self, i: usize, j: usize, k: usize) -> usize {
        (i * self.dims[1] + j) * self.dims[2] + k
    }

    #[inline]
    pub fn value_at(&self, i: usize, j: usize, k: usize) -> f32 {
        self.values[self.index(i, j, k)]
    }

    /// World-space position of grid point (i, j, k) — fractional indices
    /// allowed, so callers can also ask for interpolated sub-grid points.
    pub fn world_position(&self, i: f32, j: f32, k: f32) -> Vec3 {
        self.origin + self.steps[0] * i + self.steps[1] * j + self.steps[2] * k
    }

    /// Largest absolute value anywhere in the grid — used to suggest a
    /// sensible default isovalue, since orbitals and densities have very
    /// different natural magnitudes and there's no one constant that
    /// works for both.
    pub fn max_abs_value(&self) -> f32 {
        self.values.iter().fold(0.0f32, |acc, &v| acc.max(v.abs()))
    }

    /// A copy of this grid with every value negated. Signed fields (an
    /// orbital, unlike a density) have two isosurfaces — a "positive
    /// lobe" and a "negative lobe" — and extracting the negative one
    /// isn't just `extract_isosurface(grid, -isovalue)`: with the
    /// standard "inside = value >= isovalue" convention, a negative
    /// isovalue would select almost the *entire* grid (everything above
    /// that low bar, including all the positive region), not just the
    /// deeply negative area. Negating the grid first and extracting at
    /// the same positive isovalue selects exactly the intended region.
    pub fn negated(&self) -> ScalarGrid {
        ScalarGrid { origin: self.origin, dims: self.dims, steps: self.steps, values: self.values.iter().map(|v| -v).collect() }
    }

    /// Trilinear sample at fractional grid-index coordinates, clamped to
    /// the grid's valid index range. Used for gradient-based normal
    /// estimation (see `isosurface::gradient_normal`), not for the
    /// isosurface extraction itself (which interpolates directly along
    /// tetrahedron edges from the real grid values).
    pub fn sample_trilinear(&self, fi: f32, fj: f32, fk: f32) -> f32 {
        let clamp = |x: f32, max: usize| x.clamp(0.0, (max - 1) as f32);
        let fi = clamp(fi, self.dims[0]);
        let fj = clamp(fj, self.dims[1]);
        let fk = clamp(fk, self.dims[2]);
        let i0 = fi.floor() as usize;
        let j0 = fj.floor() as usize;
        let k0 = fk.floor() as usize;
        let i1 = (i0 + 1).min(self.dims[0] - 1);
        let j1 = (j0 + 1).min(self.dims[1] - 1);
        let k1 = (k0 + 1).min(self.dims[2] - 1);
        let tx = fi - i0 as f32;
        let ty = fj - j0 as f32;
        let tz = fk - k0 as f32;

        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let c00 = lerp(self.value_at(i0, j0, k0), self.value_at(i1, j0, k0), tx);
        let c01 = lerp(self.value_at(i0, j0, k1), self.value_at(i1, j0, k1), tx);
        let c10 = lerp(self.value_at(i0, j1, k0), self.value_at(i1, j1, k0), tx);
        let c11 = lerp(self.value_at(i0, j1, k1), self.value_at(i1, j1, k1), tx);
        let c0 = lerp(c00, c10, ty);
        let c1 = lerp(c01, c11, ty);
        lerp(c0, c1, tz)
    }

    /// Tricubic (Catmull-Rom) sample at fractional grid-index coordinates
    /// — smooth (C1-continuous) unlike `sample_trilinear`, which is only
    /// C0: piecewise-linear, so its *derivative* jumps at every grid cell
    /// boundary. That jump doesn't matter for the isosurface's own
    /// position (marching tetrahedra interpolates directly along tet
    /// edges, never calls this), but it does matter for gradient-based
    /// normals (see `isosurface::gradient_normal`) — differentiating a
    /// kinked field produces small grid-periodic noise in the normals,
    /// invisible under flat lighting but strongly amplified by a specular
    /// highlight (`pow(dot(n,h), shininess)`), which is what was showing
    /// up as visible banding/lines on curved isosurfaces. Separable, same
    /// technique as `interpolation::refine_grid`, just evaluated at one
    /// arbitrary point instead of a whole new grid.
    pub fn sample_tricubic(&self, fi: f32, fj: f32, fk: f32) -> f32 {
        let clamp = |x: f32, max: usize| x.clamp(0.0, (max - 1) as f32);
        let fi = clamp(fi, self.dims[0]);
        let fj = clamp(fj, self.dims[1]);
        let fk = clamp(fk, self.dims[2]);

        let i0 = fi.floor() as isize;
        let j0 = fj.floor() as isize;
        let k0 = fk.floor() as isize;
        let tx = fi - i0 as f32;
        let ty = fj - j0 as f32;
        let tz = fk - k0 as f32;

        let clamp_index = |v: isize, max: usize| v.clamp(0, max as isize - 1) as usize;
        let sample = |di: isize, dj: isize, dk: isize| -> f32 {
            self.value_at(clamp_index(i0 + di, self.dims[0]), clamp_index(j0 + dj, self.dims[1]), clamp_index(k0 + dk, self.dims[2]))
        };

        // Separable: interpolate the 4x4 (j,k) neighborhood along X first,
        // then the resulting 4 (k) values along Y, then those along Z.
        let mut along_z = [0.0f32; 4];
        for (zi, dk) in (-1..=2).enumerate() {
            let mut along_y = [0.0f32; 4];
            for (yi, dj) in (-1..=2).enumerate() {
                let p0 = sample(-1, dj, dk);
                let p1 = sample(0, dj, dk);
                let p2 = sample(1, dj, dk);
                let p3 = sample(2, dj, dk);
                along_y[yi] = crate::interpolation::catmull_rom(p0, p1, p2, p3, tx);
            }
            along_z[zi] = crate::interpolation::catmull_rom(along_y[0], along_y[1], along_y[2], along_y[3], ty);
        }
        crate::interpolation::catmull_rom(along_z[0], along_z[1], along_z[2], along_z[3], tz)
    }
}

pub struct CubeFile {
    pub molecule: Molecule,
    pub grid: ScalarGrid,
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Parses a float the way Rust's own `str::parse` does, but also repairs
/// two Fortran fixed-width output quirks it can't handle on its own — see
/// this module's doc comment. Only falls back to the repair attempts when
/// a plain parse fails, so well-formed tokens (the overwhelming majority)
/// pay no extra cost.
fn parse_fortran_f32(token: &str) -> Option<f32> {
    if let Ok(v) = token.parse::<f32>() {
        return Some(v);
    }
    if token.contains(['D', 'd']) {
        if let Ok(v) = token.replace(['D', 'd'], "E").parse::<f32>() {
            return Some(v);
        }
    }
    // Missing-E exponent: scan for a `+`/`-` not already preceded by
    // `E`/`e` (skipping index 0, which may be the mantissa's own sign)
    // and insert the `E` that should have been there.
    let bytes = token.as_bytes();
    for i in 1..bytes.len() {
        let c = bytes[i];
        if (c == b'+' || c == b'-') && !matches!(bytes[i - 1], b'E' | b'e') {
            let repaired = format!("{}E{}", &token[..i], &token[i..]);
            if let Ok(v) = repaired.parse::<f32>() {
                return Some(v);
            }
        }
    }
    None
}

fn next_line<'a>(all_lines: &[&'a str], line_index: &mut usize) -> Option<&'a str> {
    let line = all_lines.get(*line_index).copied();
    *line_index += 1;
    line
}

pub fn parse_cube(path: &Path) -> io::Result<CubeFile> {
    let contents = fs::read_to_string(path)?;
    let all_lines: Vec<&str> = contents.lines().collect();
    let mut line_index = 0usize;

    // Two free-text comment lines.
    next_line(&all_lines, &mut line_index).ok_or_else(|| invalid_data("empty .cube file"))?;
    next_line(&all_lines, &mut line_index).ok_or_else(|| invalid_data("truncated .cube file (missing second comment line)"))?;

    let header_line =
        next_line(&all_lines, &mut line_index).ok_or_else(|| invalid_data("truncated .cube file (missing atom-count line)"))?;
    let mut header_fields = header_line.split_whitespace();
    let raw_natoms: i64 = header_fields
        .next()
        .ok_or_else(|| invalid_data("missing atom count in .cube file"))?
        .parse()
        .map_err(|_| invalid_data("malformed atom count in .cube file"))?;
    let origin = parse_vec3_bohr(&mut header_fields, "grid origin")?;

    let natoms = raw_natoms.unsigned_abs() as usize;

    let mut dims = [0usize; 3];
    let mut steps = [Vec3::ZERO; 3];
    for axis in 0..3 {
        let line = next_line(&all_lines, &mut line_index).ok_or_else(|| invalid_data("truncated .cube file (missing grid dimension line)"))?;
        let mut fields = line.split_whitespace();
        let count: i64 = fields
            .next()
            .ok_or_else(|| invalid_data("missing grid point count in .cube file"))?
            .parse()
            .map_err(|_| invalid_data("malformed grid point count in .cube file"))?;
        dims[axis] = count.unsigned_abs() as usize;
        steps[axis] = parse_vec3_bohr(&mut fields, "grid step vector")?;
    }
    if dims.iter().any(|&d| d == 0) {
        return Err(invalid_data("grid dimension of zero in .cube file"));
    }

    let mut atomic_numbers = Vec::with_capacity(natoms);
    let mut positions = Vec::with_capacity(natoms);
    for _ in 0..natoms {
        let line = next_line(&all_lines, &mut line_index).ok_or_else(|| invalid_data("truncated .cube file (missing atom line)"))?;
        let mut fields = line.split_whitespace();
        let atomic_number: u32 = fields
            .next()
            .ok_or_else(|| invalid_data("missing atomic number in .cube file"))?
            .parse()
            .map_err(|_| invalid_data("malformed atomic number in .cube file"))?;
        fields.next().ok_or_else(|| invalid_data("missing nuclear charge in .cube file"))?; // unused
        let position = parse_vec3_bohr(&mut fields, "atom position")?;
        atomic_numbers.push(atomic_number);
        positions.push(position);
    }

    // The `M ORBITAL_INDEX...` line, when present, is a line of bare
    // integers (no decimal point) — real grid data never is, so peek at
    // content rather than trusting NATOMS's sign (see module doc comment:
    // real APOST-3D files write this line even with NATOMS positive).
    let looks_like_dataset_line = all_lines.get(line_index).is_some_and(|line| {
        let mut tokens = line.split_whitespace().peekable();
        tokens.peek().is_some() && tokens.all(|t| !t.contains('.'))
    });
    let datasets = if looks_like_dataset_line {
        let line = next_line(&all_lines, &mut line_index).expect("just peeked this line, it exists");
        let count: usize = line
            .split_whitespace()
            .next()
            .ok_or_else(|| invalid_data("missing dataset count in .cube file"))?
            .parse()
            .map_err(|_| invalid_data("malformed dataset count in .cube file"))?;
        count.max(1)
    } else {
        1
    };

    let grid_point_count = dims[0] * dims[1] * dims[2];
    let mut values = Vec::with_capacity(grid_point_count);
    let mut tokens = all_lines[line_index..].iter().flat_map(|line| line.split_ascii_whitespace());
    for _ in 0..grid_point_count {
        let raw_token = tokens.next().ok_or_else(|| invalid_data("truncated .cube file (grid data ends early)"))?;
        let value = parse_fortran_f32(raw_token).ok_or_else(|| invalid_data(format!("malformed grid value '{raw_token}' in .cube file")))?;
        values.push(value);
        // Multi-dataset files interleave M values per grid point — keep
        // only the first dataset (see module docs) and skip the rest.
        for _ in 1..datasets {
            tokens.next().ok_or_else(|| invalid_data("truncated .cube file (grid data ends early)"))?;
        }
    }

    let molecule = Molecule::from_atoms(atomic_numbers, positions);
    let grid = ScalarGrid { origin, dims, steps, values };
    Ok(CubeFile { molecule, grid })
}

fn parse_vec3_bohr<'a>(fields: &mut impl Iterator<Item = &'a str>, what: &str) -> io::Result<Vec3> {
    let mut next = || -> io::Result<f32> {
        let token = fields.next().ok_or_else(|| invalid_data(format!("missing component of {what} in .cube file")))?;
        let raw = parse_fortran_f32(token).ok_or_else(|| invalid_data(format!("malformed {what} '{token}' in .cube file")))?;
        Ok(raw * ANGSTROM_PER_BOHR as f32)
    };
    Ok(Vec3::new(next()?, next()?, next()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_scientific_notation() {
        assert!((parse_fortran_f32("0.10135E-30").unwrap() - 0.10135e-30).abs() < 1e-36);
        assert!((parse_fortran_f32("-0.39801E-20").unwrap() - (-0.39801e-20)).abs() < 1e-26);
        assert!((parse_fortran_f32("23.000000").unwrap() - 23.0).abs() < 1e-6);
    }

    #[test]
    fn repairs_missing_exponent_e() {
        // Real values seen in an actual APOST-3D cube file's decaying tail,
        // but scaled to stay within f32's representable range — the actual
        // file's ~1e-167 magnitudes correctly underflow f32 to exactly
        // 0.0, which is fine (those are physically negligible orbital-tail
        // values no isovalue would ever threshold against), but that makes
        // a poor test of the *parsing* logic specifically, which is what's
        // under test here.
        let got = parse_fortran_f32("0.78396-30").unwrap();
        assert!(got > 0.0 && got < 1e-25, "expected a tiny positive value, got {got}");

        let got = parse_fortran_f32("-0.16674-25").unwrap();
        assert!(got < 0.0 && got > -1e-20, "expected a tiny negative value, got {got}");
    }

    #[test]
    fn repairs_d_exponent() {
        assert!((parse_fortran_f32("0.123D+04").unwrap() - 1230.0).abs() < 1e-3);
        assert!((parse_fortran_f32("-0.5d-02").unwrap() - (-0.005)).abs() < 1e-6);
    }

    #[test]
    fn rejects_genuine_garbage() {
        assert!(parse_fortran_f32("not-a-number").is_none());
        assert!(parse_fortran_f32("").is_none());
    }

    fn axis_aligned_grid(dims: [usize; 3], step: f32, values: Vec<f32>) -> ScalarGrid {
        ScalarGrid {
            origin: Vec3::ZERO,
            dims,
            steps: [Vec3::new(step, 0.0, 0.0), Vec3::new(0.0, step, 0.0), Vec3::new(0.0, 0.0, step)],
            values,
        }
    }

    #[test]
    fn tricubic_sample_reproduces_exact_values_at_grid_points() {
        let n = 6;
        let values: Vec<f32> = (0..n * n * n).map(|idx| (idx as f32) * 0.37).collect();
        let grid = axis_aligned_grid([n, n, n], 1.0, values);
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let expected = grid.value_at(i, j, k);
                    let actual = grid.sample_tricubic(i as f32, j as f32, k as f32);
                    assert!((actual - expected).abs() < 1e-3, "mismatch at ({i},{j},{k}): {actual} vs {expected}");
                }
            }
        }
    }

    #[test]
    fn tricubic_sample_reproduces_a_linear_field_exactly() {
        // Catmull-Rom is exact on linear data (it's a cubic that happens
        // to degenerate to the line through collinear points) — a good
        // sanity check that the 3-pass separable wiring (axis order,
        // neighbor indexing) isn't introducing any bias.
        let n = 6;
        let mut values = vec![0.0f32; n * n * n];
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    values[(i * n + j) * n + k] = 2.0 * i as f32 + 3.0 * j as f32 + 5.0 * k as f32;
                }
            }
        }
        let grid = axis_aligned_grid([n, n, n], 1.0, values);
        let sample = grid.sample_tricubic(2.5, 1.25, 3.75);
        let expected = 2.0 * 2.5 + 3.0 * 1.25 + 5.0 * 3.75;
        assert!((sample - expected).abs() < 1e-3, "expected {expected}, got {sample}");
    }
}
