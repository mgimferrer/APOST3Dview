//! Text formatting for the XYZ coordinate viewer. Kept separate from any
//! UI toolkit so it's independently testable and reusable (e.g. by a
//! future "export .xyz" feature).

use std::fmt::Write as _;

use crate::element::element_data;
use crate::molecule::Molecule;
use crate::units::LengthUnit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateFormat {
    /// Four columns: atomic number, x, y, z.
    AtomicNumberTable,
    /// Standard .xyz file: atom count, comment line, then symbol + x y z.
    XyzFile,
}

pub fn format_coordinates(
    molecule: &Molecule,
    unit: LengthUnit,
    format: CoordinateFormat,
    comment: &str,
) -> String {
    let mut out = String::new();

    if format == CoordinateFormat::XyzFile {
        let _ = writeln!(out, "{}", molecule.atomic_numbers.len());
        let _ = writeln!(out, "{comment}");
    }

    for (&atomic_number, position) in molecule.atomic_numbers.iter().zip(&molecule.positions) {
        let x = unit.from_angstrom(position.x as f64);
        let y = unit.from_angstrom(position.y as f64);
        let z = unit.from_angstrom(position.z as f64);

        match format {
            CoordinateFormat::XyzFile => {
                let symbol = element_data(atomic_number).symbol;
                let _ = writeln!(out, "{symbol:<2}  {x:>14.8}  {y:>14.8}  {z:>14.8}");
            }
            CoordinateFormat::AtomicNumberTable => {
                let _ = writeln!(out, "{atomic_number:>3}  {x:>14.8}  {y:>14.8}  {z:>14.8}");
            }
        }
    }

    out
}
