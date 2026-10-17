//! Length unit conversion. Single source of truth so the .fchk parser and
//! the coordinate-display code can't drift apart.

/// Bohr -> angstrom, CODATA 2018.
pub const ANGSTROM_PER_BOHR: f64 = 0.529177210903;

/// A point in bohr, in angstrom (the viewer's unit).
pub fn angstrom_from_bohr(p: [f64; 3]) -> glam::Vec3 {
    glam::Vec3::new((p[0] * ANGSTROM_PER_BOHR) as f32, (p[1] * ANGSTROM_PER_BOHR) as f32, (p[2] * ANGSTROM_PER_BOHR) as f32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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
