//! Length unit conversion. Single source of truth so the .fchk parser and
//! the coordinate-display code can't drift apart.

/// Bohr -> angstrom, CODATA 2018.
pub const ANGSTROM_PER_BOHR: f64 = 0.529177210903;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Angstrom,
    Bohr,
}

impl LengthUnit {
    pub fn from_angstrom(self, angstrom: f64) -> f64 {
        match self {
            LengthUnit::Angstrom => angstrom,
            LengthUnit::Bohr => angstrom / ANGSTROM_PER_BOHR,
        }
    }
}
