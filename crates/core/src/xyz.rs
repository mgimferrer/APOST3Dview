//! Standard .xyz file parsing: an atom-count line, a free-text comment
//! line, then one `Symbol x y z` line per atom, already in angstrom (no
//! unit conversion needed, unlike .fchk's Bohr coordinates).
//!
//! A file may hold several geometries one after the other (a reaction
//! path, an optimization, a trajectory): each frame starts again with its
//! atom count. Blank lines between frames are skipped, and so are the `>`
//! lines ORCA writes between the frames of an `.allxyz` file.

use std::fs;
use std::io;
use std::path::Path;

use glam::Vec3;

use crate::element::atomic_number_from_symbol;
use crate::invalid_data;
use crate::molecule::Molecule;

/// One geometry of an .xyz file.
#[derive(Debug, Clone, PartialEq)]
pub struct XyzFrame {
    pub atomic_numbers: Vec<u32>,
    /// Angstrom.
    pub positions: Vec<Vec3>,
    /// The frame's comment line, as written (programs often put the energy
    /// there).
    pub comment: String,
}

/// The geometries of an .xyz file.
#[derive(Debug, Clone, PartialEq)]
pub struct XyzFile {
    /// In the file's order, at least one.
    pub frames: Vec<XyzFrame>,
    /// Why reading stopped before the end of the file, when it did: the
    /// complete geometries before are kept (a job stopped while writing
    /// leaves its last geometry cut).
    pub stopped: Option<String>,
}

/// The first geometry of an .xyz file.
pub fn parse_xyz(path: &Path) -> io::Result<Molecule> {
    let frame = parse_xyz_frames(path)?.frames.swap_remove(0);
    Ok(Molecule::from_atoms(frame.atomic_numbers, frame.positions))
}

/// Every geometry of an .xyz file, in order. An error only when not even
/// the first one can be read.
pub fn parse_xyz_frames(path: &Path) -> io::Result<XyzFile> {
    parse_xyz_text(&fs::read_to_string(path)?)
}

/// `parse_xyz_frames` on text already read.
pub fn parse_xyz_text(contents: &str) -> io::Result<XyzFile> {
    let mut frames = Vec::new();
    let stopped = read_frames(contents, &mut frames).err();
    match (frames.is_empty(), stopped) {
        (true, Some(err)) => Err(err),
        (true, None) => Err(invalid_data("empty .xyz file")),
        (false, stopped) => {
            let n = frames.len();
            Ok(XyzFile { frames, stopped: stopped.map(|err| format!("reading stopped after geometry {n}: {err}")) })
        }
    }
}

/// Reads geometries into `frames` until the end, or the first problem.
fn read_frames(contents: &str, frames: &mut Vec<XyzFrame>) -> io::Result<()> {
    let mut lines = contents.lines().peekable();
    loop {
        while lines.peek().is_some_and(|line| is_separator(line)) {
            lines.next();
        }
        let Some(count_line) = lines.next() else { break };
        let number = frames.len() + 1;
        let count: usize = count_line.trim().parse().map_err(|_| {
            if frames.is_empty() {
                invalid_data("first line of .xyz must be the atom count")
            } else {
                invalid_data(format!("expected the atom count of geometry {number}, found \"{}\"", count_line.trim()))
            }
        })?;
        let comment = lines.next().unwrap_or_default().trim().to_string();
        let mut atomic_numbers = Vec::with_capacity(count);
        let mut positions = Vec::with_capacity(count);
        for line in lines.by_ref().take(count) {
            let (z, position) = parse_atom_line(line)?;
            atomic_numbers.push(z);
            positions.push(position);
        }
        if atomic_numbers.len() != count {
            return Err(invalid_data(format!("expected {count} atom lines, found {}", atomic_numbers.len())));
        }
        frames.push(XyzFrame { atomic_numbers, positions, comment });
    }
    Ok(())
}

/// A blank line, or ORCA's `>` between the frames of an `.allxyz` file.
fn is_separator(line: &str) -> bool {
    let line = line.trim();
    line.is_empty() || line == ">"
}

/// `Symbol x y z` (or the atomic number instead of the symbol); anything
/// after the coordinates (forces, charges of an extended .xyz) is ignored.
fn parse_atom_line(line: &str) -> io::Result<(u32, Vec3)> {
    let mut fields = line.split_whitespace();
    let symbol = fields.next().ok_or_else(|| invalid_data("expected an atom line in .xyz file"))?;
    let atomic_number = match symbol.parse::<u32>() {
        Ok(z) if (1..=118).contains(&z) => Some(z),
        _ => atomic_number_from_symbol(symbol),
    }
    .ok_or_else(|| invalid_data(format!("unknown element symbol '{symbol}' in .xyz file")))?;
    let mut next_f32 = || -> io::Result<f32> {
        fields
            .next()
            .ok_or_else(|| invalid_data("expected x/y/z coordinate in .xyz file"))?
            .parse::<f32>()
            .map_err(|_| invalid_data("malformed coordinate in .xyz file"))
    };
    let position = Vec3::new(next_f32()?, next_f32()?, next_f32()?);
    Ok((atomic_number, position))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATER: &str = "3\nwater\nO 0.0 0.0 0.0\nH 0.96 0.0 0.0\nH -0.24 0.93 0.0\n";

    #[test]
    fn one_geometry() {
        let file = parse_xyz_text(WATER).unwrap();
        assert_eq!(file.stopped, None);
        let frames = file.frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].atomic_numbers, vec![8, 1, 1]);
        assert_eq!(frames[0].comment, "water");
        assert_eq!(frames[0].positions[1], Vec3::new(0.96, 0.0, 0.0));
    }

    #[test]
    fn several_geometries_one_after_the_other() {
        let text = format!("{WATER}{}", WATER.replace("water", "E -76.4").replace("0.96", "0.98"));
        let frames = parse_xyz_text(&text).unwrap().frames;
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].comment, "E -76.4");
        assert_eq!(frames[1].positions[1].x, 0.98);
    }

    #[test]
    fn orca_allxyz_separators_and_blank_lines() {
        let text = format!("{WATER}>\n{WATER}\n\n{WATER}\n");
        assert_eq!(parse_xyz_text(&text).unwrap().frames.len(), 3);
    }

    #[test]
    fn atomic_numbers_and_extra_columns() {
        let frames = parse_xyz_text("2\n\n6 0 0 0 0.1 0.2 0.3\nO 1.2 0 0 extra\n").unwrap().frames;
        assert_eq!(frames[0].atomic_numbers, vec![6, 8]);
        assert_eq!(frames[0].positions[1], Vec3::new(1.2, 0.0, 0.0));
    }

    #[test]
    fn a_cut_last_geometry_keeps_the_ones_before() {
        let file = parse_xyz_text(&format!("{WATER}{WATER}3\ncut\nO 0 0 0\n")).unwrap();
        assert_eq!(file.frames.len(), 2);
        assert_eq!(file.stopped.as_deref(), Some("reading stopped after geometry 2: expected 3 atom lines, found 1"));
        let file = parse_xyz_text(&format!("{WATER}garbage\n")).unwrap();
        assert_eq!(file.frames.len(), 1);
        assert!(file.stopped.is_some());
        assert!(parse_xyz_text("").is_err());
        assert!(parse_xyz_text("x\n").is_err());
        assert!(parse_xyz_text("3\nw\nO 0 0 0\n").is_err());
    }
}
