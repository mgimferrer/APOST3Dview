//! Standard .xyz file parsing: an atom-count line, a free-text comment
//! line, then one `Symbol x y z` line per atom, already in angstrom (no
//! unit conversion needed, unlike .fchk's Bohr coordinates).

use std::fs;
use std::io;
use std::path::Path;

use glam::Vec3;

use crate::element::atomic_number_from_symbol;
use crate::molecule::Molecule;

pub fn parse_xyz(path: &Path) -> io::Result<Molecule> {
    let contents = fs::read_to_string(path)?;
    let mut lines = contents.lines();

    let count: usize = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty .xyz file"))?
        .trim()
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "first line of .xyz must be the atom count"))?;

    // Comment line — present by convention but its content is free text
    // and unused for parsing.
    lines.next();

    let mut atomic_numbers = Vec::with_capacity(count);
    let mut positions = Vec::with_capacity(count);

    for line in lines.take(count) {
        let mut fields = line.split_whitespace();
        let symbol = fields
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "expected an atom line in .xyz file"))?;
        let atomic_number = atomic_number_from_symbol(symbol)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("unknown element symbol '{symbol}' in .xyz file")))?;

        let mut next_f32 = || -> io::Result<f32> {
            fields
                .next()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "expected x/y/z coordinate in .xyz file"))?
                .parse::<f32>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "malformed coordinate in .xyz file"))
        };
        let position = Vec3::new(next_f32()?, next_f32()?, next_f32()?);

        atomic_numbers.push(atomic_number);
        positions.push(position);
    }

    if atomic_numbers.len() != count {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected {count} atom lines, found {}", atomic_numbers.len()),
        ));
    }

    Ok(Molecule::from_atoms(atomic_numbers, positions))
}
