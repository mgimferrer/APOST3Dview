//! Gaussian formatted checkpoint (.fchk) parsing. Phase 1 only needs atomic
//! numbers and Cartesian coordinates, so this streams the file line-by-line
//! and stops as soon as both are found — both sections sit near the top of
//! the file, well before the (potentially huge) basis-set/MO-coefficient
//! blocks, so this stays fast regardless of overall file size.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Lines};
use std::path::Path;

use crate::units::ANGSTROM_PER_BOHR;

pub struct FchkGeometry {
    pub atomic_numbers: Vec<u32>,
    /// Angstrom.
    pub positions: Vec<[f32; 3]>,
}

pub(crate) enum HeaderKind {
    Scalar,
    Array(usize),
}

pub(crate) struct Header {
    pub(crate) label: String,
    pub(crate) kind: HeaderKind,
}

pub(crate) fn parse_header(line: &str) -> Option<Header> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let type_idx = tokens.iter().position(|t| t.len() == 1 && matches!(*t, "I" | "R" | "C"))?;
    if type_idx == 0 {
        return None;
    }
    let label = tokens[..type_idx].join(" ");
    if type_idx + 1 < tokens.len() && tokens[type_idx + 1] == "N=" {
        let count: usize = tokens.get(type_idx + 2)?.parse().ok()?;
        Some(Header { label, kind: HeaderKind::Array(count) })
    } else {
        Some(Header { label, kind: HeaderKind::Scalar })
    }
}

/// Reads forward, collecting whitespace-separated tokens across as many
/// lines as needed, until `count` tokens have been gathered. Relies on fchk
/// arrays always ending exactly on a line boundary (true of the format).
pub(crate) fn read_array_tokens(lines: &mut Lines<BufReader<File>>, count: usize) -> io::Result<Vec<String>> {
    let mut tokens = Vec::with_capacity(count);
    while tokens.len() < count {
        let line = lines
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "fchk array section truncated"))??;
        tokens.extend(line.split_whitespace().map(str::to_string));
    }
    tokens.truncate(count);
    Ok(tokens)
}

pub fn parse_fchk_geometry(path: &Path) -> io::Result<FchkGeometry> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();

    let mut atomic_numbers: Option<Vec<u32>> = None;
    let mut coordinates: Option<Vec<f64>> = None;

    while let Some(line) = lines.next() {
        let line = line?;
        let Some(header) = parse_header(&line) else { continue };
        let HeaderKind::Array(count) = header.kind else { continue };

        match header.label.as_str() {
            "Atomic numbers" => {
                let tokens = read_array_tokens(&mut lines, count)?;
                atomic_numbers = Some(tokens.iter().map(|t| t.parse().unwrap_or(0)).collect());
            }
            "Current cartesian coordinates" => {
                let tokens = read_array_tokens(&mut lines, count)?;
                coordinates = Some(tokens.iter().map(|t| t.parse().unwrap_or(0.0)).collect());
            }
            _ => {
                // Not a section we need — still have to consume its data so
                // the next header line is read at the right position.
                read_array_tokens(&mut lines, count)?;
            }
        }

        if atomic_numbers.is_some() && coordinates.is_some() {
            break;
        }
    }

    let atomic_numbers = atomic_numbers
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "fchk missing 'Atomic numbers' section"))?;
    let coordinates = coordinates
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "fchk missing 'Current cartesian coordinates' section"))?;

    if coordinates.len() != atomic_numbers.len() * 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "fchk coordinate count doesn't match 3x atom count",
        ));
    }

    let positions = coordinates
        .chunks_exact(3)
        .map(|c| {
            [
                (c[0] * ANGSTROM_PER_BOHR) as f32,
                (c[1] * ANGSTROM_PER_BOHR) as f32,
                (c[2] * ANGSTROM_PER_BOHR) as f32,
            ]
        })
        .collect();

    Ok(FchkGeometry { atomic_numbers, positions })
}
