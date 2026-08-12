//! Per-element display data: symbol, CPK color, covalent radius, van der
//! Waals radius. Indexed by atomic number (1-based; index 0 is unused).
//!
//! Colors are the standard Jmol/CPK scheme, Z=1-109, taken directly from
//! https://sciencenotes.org/molecule-atom-colors-cpk-colors/ (per Martí's
//! request — this is also the table VMD, Jmol, Avogadro etc. all converged
//! on, so it should look familiar).
//!
//! Two separate radius tables, used for two different purposes:
//! - `covalent_radius` (Cordero et al. 2008 single-bond values) drives bond
//!   perception — needs to reflect real bonding distances.
//! - `vdw_radius` (Bondi 1964 / Mantina 2009 / Alvarez 2013, best-available
//!   per element) drives ball display size. Covalent radii vary too sharply
//!   between elements for that (H at 0.31 A vs Bi at 1.48 A is a ~5x
//!   spread) and made light/heavy atoms look mismatched size; van der Waals
//!   radii vary far more gently (H 1.20 vs Bi 2.07, ~1.7x) and is what
//!   CPK-style ball-and-stick models are conventionally built on.
//!
//! Values for Z=97-109 (past Cm) aren't well established experimentally —
//! those rows use a flat generic estimate rather than a real measurement.

pub struct ElementData {
    pub symbol: &'static str,
    /// Jmol/CPK color, linear 0..1 RGB.
    pub cpk_color: [f32; 3],
    /// Cordero (2008) single-bond covalent radius, angstrom.
    pub covalent_radius: f32,
    /// Bondi/Mantina/Alvarez van der Waals radius, angstrom.
    pub vdw_radius: f32,
}

const fn rgb(hex: u32) -> [f32; 3] {
    let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
    let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
    let b = (hex & 0xFF) as f32 / 255.0;
    [r, g, b]
}

macro_rules! element {
    ($symbol:literal, $hex:expr, $covalent:expr, $vdw:expr) => {
        ElementData { symbol: $symbol, cpk_color: rgb($hex), covalent_radius: $covalent, vdw_radius: $vdw }
    };
}

/// Index 0 is unused (placeholder), so `ELEMENTS[atomic_number]` is direct.
pub static ELEMENTS: [ElementData; 110] = [
    element!("", 0xFFC0CB, 1.5, 2.0), // 0: unused
    element!("H", 0xFFFFFF, 0.31, 1.20),
    element!("He", 0xD9FFFF, 0.28, 1.40),
    element!("Li", 0xCC80FF, 1.28, 1.82),
    element!("Be", 0xC2FF00, 0.96, 1.53),
    element!("B", 0xFFB5B5, 0.84, 1.92),
    element!("C", 0x909090, 0.76, 1.70),
    element!("N", 0x3050F8, 0.71, 1.55),
    element!("O", 0xFF0D0D, 0.66, 1.52),
    element!("F", 0x90E050, 0.57, 1.47),
    element!("Ne", 0xB3E3F5, 0.58, 1.54),
    element!("Na", 0xAB5CF2, 1.66, 2.27),
    element!("Mg", 0x8AFF00, 1.41, 1.73),
    element!("Al", 0xBFA6A6, 1.21, 1.84),
    element!("Si", 0xF0C8A0, 1.11, 2.10),
    element!("P", 0xFF8000, 1.07, 1.80),
    element!("S", 0xFFFF30, 1.05, 1.80),
    element!("Cl", 0x1FF01F, 1.02, 1.75),
    element!("Ar", 0x80D1E3, 1.06, 1.88),
    element!("K", 0x8F40D4, 2.03, 2.75),
    element!("Ca", 0x3DFF00, 1.76, 2.31),
    element!("Sc", 0xE6E6E6, 1.70, 2.15),
    element!("Ti", 0xBFC2C7, 1.60, 2.11),
    element!("V", 0xA6A6AB, 1.53, 2.07),
    element!("Cr", 0x8A99C7, 1.39, 2.06),
    element!("Mn", 0x9C7AC7, 1.50, 2.05),
    element!("Fe", 0xE06633, 1.42, 2.04),
    element!("Co", 0xF090A0, 1.38, 2.00),
    element!("Ni", 0x50D050, 1.24, 1.97),
    element!("Cu", 0xC88033, 1.32, 1.96),
    element!("Zn", 0x7D80B0, 1.22, 2.01),
    element!("Ga", 0xC28F8F, 1.22, 1.87),
    element!("Ge", 0x668F8F, 1.20, 2.11),
    element!("As", 0xBD80E3, 1.19, 1.85),
    element!("Se", 0xFFA100, 1.20, 1.90),
    element!("Br", 0xA62929, 1.20, 1.85),
    element!("Kr", 0x5CB8D1, 1.16, 2.02),
    element!("Rb", 0x702EB0, 2.20, 3.03),
    element!("Sr", 0x00FF00, 1.95, 2.49),
    element!("Y", 0x94FFFF, 1.90, 2.32),
    element!("Zr", 0x94E0E0, 1.75, 2.23),
    element!("Nb", 0x73C2C9, 1.64, 2.18),
    element!("Mo", 0x54B5B5, 1.54, 2.17),
    element!("Tc", 0x3B9E9E, 1.47, 2.16),
    element!("Ru", 0x248F8F, 1.46, 2.13),
    element!("Rh", 0x0A7D8C, 1.42, 2.10),
    element!("Pd", 0x006985, 1.39, 2.10),
    element!("Ag", 0xC0C0C0, 1.45, 2.11),
    element!("Cd", 0xFFD98F, 1.44, 2.18),
    element!("In", 0xA67573, 1.42, 1.93),
    element!("Sn", 0x668080, 1.39, 2.17),
    element!("Sb", 0x9E63B5, 1.39, 2.06),
    element!("Te", 0xD47A00, 1.38, 2.06),
    element!("I", 0x940094, 1.39, 1.98),
    element!("Xe", 0x429EB0, 1.40, 2.16),
    element!("Cs", 0x57178F, 2.44, 3.43),
    element!("Ba", 0x00C900, 2.15, 2.68),
    element!("La", 0x70D4FF, 2.07, 2.43),
    element!("Ce", 0xFFFFC7, 2.04, 2.42),
    element!("Pr", 0xD9FFC7, 2.03, 2.40),
    element!("Nd", 0xC7FFC7, 2.01, 2.39),
    element!("Pm", 0xA3FFC7, 1.99, 2.38),
    element!("Sm", 0x8FFFC7, 1.98, 2.36),
    element!("Eu", 0x61FFC7, 1.98, 2.35),
    element!("Gd", 0x45FFC7, 1.96, 2.34),
    element!("Tb", 0x30FFC7, 1.94, 2.33),
    element!("Dy", 0x1FFFC7, 1.92, 2.31),
    element!("Ho", 0x00FF9C, 1.92, 2.30),
    element!("Er", 0x00E675, 1.89, 2.29),
    element!("Tm", 0x00D452, 1.90, 2.27),
    element!("Yb", 0x00BF38, 1.87, 2.26),
    element!("Lu", 0x00AB24, 1.87, 2.24),
    element!("Hf", 0x4DC2FF, 1.75, 2.23),
    element!("Ta", 0x4DA6FF, 1.70, 2.22),
    element!("W", 0x2194D6, 1.62, 2.18),
    element!("Re", 0x267DAB, 1.51, 2.16),
    element!("Os", 0x266696, 1.44, 2.16),
    element!("Ir", 0x175487, 1.41, 2.13),
    element!("Pt", 0xD0D0E0, 1.36, 2.13),
    element!("Au", 0xFFD123, 1.36, 2.14),
    element!("Hg", 0xB8B8D0, 1.32, 2.23),
    element!("Tl", 0xA6544D, 1.45, 1.96),
    element!("Pb", 0x575961, 1.46, 2.02),
    element!("Bi", 0x9E4FB5, 1.48, 2.07),
    element!("Po", 0xAB5C00, 1.40, 1.97),
    element!("At", 0x754F45, 1.50, 2.02),
    element!("Rn", 0x428296, 1.50, 2.20),
    element!("Fr", 0x420066, 2.60, 3.48),
    element!("Ra", 0x007D00, 2.21, 2.83),
    element!("Ac", 0x70ABFA, 2.15, 2.60),
    element!("Th", 0x00BAFF, 2.06, 2.37),
    element!("Pa", 0x00A1FF, 2.00, 2.43),
    element!("U", 0x008FFF, 1.96, 2.40),
    element!("Np", 0x0080FF, 1.90, 2.21),
    element!("Pu", 0x006BFF, 1.87, 2.43),
    element!("Am", 0x545CF2, 1.80, 2.44),
    element!("Cm", 0x785CE3, 1.69, 2.45),
    element!("Bk", 0x8A4FE3, 1.68, 2.00),
    element!("Cf", 0xA136D4, 1.68, 2.00),
    element!("Es", 0xB31FD4, 1.65, 2.00),
    element!("Fm", 0xB31FBA, 1.67, 2.00),
    element!("Md", 0xB30DA6, 1.73, 2.00),
    element!("No", 0xBD0D87, 1.76, 2.00),
    element!("Lr", 0xC70066, 1.61, 2.00),
    element!("Rf", 0xCC0059, 1.57, 2.00),
    element!("Db", 0xD1004F, 1.49, 2.00),
    element!("Sg", 0xD90045, 1.43, 2.00),
    element!("Bh", 0xE00038, 1.41, 2.00),
    element!("Hs", 0xE6002E, 1.34, 2.00),
    element!("Mt", 0xEB0026, 1.29, 2.00),
];

const FALLBACK: ElementData = ElementData { symbol: "?", cpk_color: rgb(0xFFC0CB), covalent_radius: 1.5, vdw_radius: 2.0 };

pub fn element_data(atomic_number: u32) -> &'static ElementData {
    ELEMENTS.get(atomic_number as usize).unwrap_or(&FALLBACK)
}

/// Reverse lookup for .xyz parsing, which gives element symbols rather
/// than atomic numbers. Case-insensitive (symbols in the wild show up as
/// "H", "h", occasionally "H1" for isotope-labeled atoms — this matches
/// on the element part only, ignoring trailing digits).
pub fn atomic_number_from_symbol(symbol: &str) -> Option<u32> {
    let symbol = symbol.trim_end_matches(|c: char| c.is_ascii_digit());
    ELEMENTS
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, element)| element.symbol.eq_ignore_ascii_case(symbol))
        .map(|(z, _)| z as u32)
}
