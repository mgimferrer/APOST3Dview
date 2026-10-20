//! Formatted checkpoint (.fchk) parsing: the header/array reading shared
//! with `wavefunction.rs`, and the fast summary (geometry, basis size, SCF
//! energy) that opening a file and checking APOST-3D results both use. It
//! streams the file and stops before the (potentially huge) MO blocks, so
//! it stays fast regardless of the file's size.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Lines};
use std::path::Path;

use crate::invalid_data;

pub(crate) enum HeaderKind {
    Scalar,
    Array(usize),
}

pub(crate) struct Header {
    pub(crate) label: String,
    pub(crate) kind: HeaderKind,
}

/// The value of a scalar header line ("Number of basis functions   I   58").
pub(crate) fn scalar_value<T: std::str::FromStr>(line: &str) -> Option<T> {
    line.split_whitespace().last()?.parse().ok()
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

/// What the APOST-3D results consistency check compares against (see
/// `apost::check_against_fchk`): the geometry in the file's own units, plus
/// two scalars that tell apart calculations sharing one geometry (the same
/// molecule in another basis set, or with another method).
pub struct FchkSummary {
    pub atomic_numbers: Vec<u32>,
    pub coordinates_bohr: Vec<[f64; 3]>,
    pub n_basis_functions: Option<usize>,
    /// The "SCF Energy" field, the energy APOST-3D reports as
    /// `wavefunction.energy_au` (not "Total Energy", which holds the MP2
    /// energy in an MP2 file). Absent from Q-Chem files.
    pub scf_energy: Option<f64>,
}

/// Reads `FchkSummary` from a `.fchk`. Gaussian writes all four fields
/// before the MO coefficients, so the scan stops there once the geometry is
/// known; Q-Chem writes the coordinates after the MOs, in which case the
/// scan continues until they are found.
pub fn parse_fchk_summary(path: &Path) -> io::Result<FchkSummary> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();

    let mut atomic_numbers: Option<Vec<u32>> = None;
    let mut coordinates: Option<Vec<f64>> = None;
    let mut n_basis_functions = None;
    let mut scf_energy = None;

    while let Some(line) = lines.next() {
        let line = line?;
        let Some(header) = parse_header(&line) else { continue };
        match header.kind {
            HeaderKind::Scalar => {
                match header.label.as_str() {
                    "Number of basis functions" => n_basis_functions = scalar_value(&line),
                    "SCF Energy" => scf_energy = scalar_value(&line),
                    _ => {}
                }
            }
            HeaderKind::Array(count) => {
                let geometry_known = atomic_numbers.is_some() && coordinates.is_some();
                if header.label == "Alpha MO coefficients" && geometry_known {
                    break;
                }
                let tokens = read_array_tokens(&mut lines, count)?;
                match header.label.as_str() {
                    "Atomic numbers" => atomic_numbers = Some(tokens.iter().map(|t| t.parse().unwrap_or(0)).collect()),
                    // The first block: Q-Chem repeats it (frequency jobs).
                    "Current cartesian coordinates" if coordinates.is_none() => coordinates = Some(tokens.iter().map(|t| t.parse().unwrap_or(0.0)).collect()),
                    _ => {}
                }
            }
        }
        if atomic_numbers.is_some() && coordinates.is_some() && n_basis_functions.is_some() && scf_energy.is_some() {
            break;
        }
    }

    let atomic_numbers = atomic_numbers.ok_or_else(|| invalid_data("fchk missing 'Atomic numbers' section"))?;
    let coordinates = coordinates.ok_or_else(|| invalid_data("fchk missing 'Current cartesian coordinates' section"))?;
    if coordinates.len() != atomic_numbers.len() * 3 {
        return Err(invalid_data("fchk coordinate count doesn't match 3x atom count"));
    }
    let coordinates_bohr = coordinates.as_chunks::<3>().0.to_vec();

    Ok(FchkSummary { atomic_numbers, coordinates_bohr, n_basis_functions, scf_energy })
}
