//! Parses `.molden` files — plain-text, section-tagged (`[Atoms]`/`[GTO]`/
//! `[MO]`), unlike `.fchk`'s fixed-width binary-ish dump. Targets the
//! PySCF writer specifically for now (`pyscf.tools.molden`) — other
//! programs (ORCA in particular) are known to deviate from the format for
//! f/g functions, deliberately not handled here yet.
//!
//! The one thing that made this straightforward rather than needing a new
//! reordering layer: PySCF's own `order_ao_index` (in its molden writer)
//! documents the exact per-shell component order it writes —
//! 5D: D0,D+1,D-1,D+2,D-2; 7F: F0,F+1,F-1,F+2,F-2,F+3,F-3; 9G likewise;
//! 6D: xx,yy,zz,xy,xz,yz; 10F: xxx,yyy,zzz,xyy,xxy,xxz,xzz,yzz,yyz,xyz —
//! which is *exactly* the order `gto.rs::evaluate_basis_functions` already
//! produces (that module's own doc comments spell out the same order,
//! confirmed against real Gaussian `.fchk` files). So a `.molden` shell's
//! coefficients can be read straight into the same `Shell`/`MolecularOrbitals`
//! structures `wavefunction.rs` builds for `.fchk`, with no permutation —
//! `gto.rs`'s evaluator doesn't know or care which file format a `Shell`
//! came from.
//!
//! Purity (pure/spherical vs Cartesian) is a *global* file flag here
//! (`[5D]`/`[7F]`/`[9G]` vs `[6D]`/`[10F]`/`[15G]`), unlike `.fchk` where
//! each shell carries its own signed type code — real `.fchk` files were
//! found to mix Cartesian and pure shells in one basis, but Molden's flag
//! is per-file, so that mixing isn't representable here (not a concern for
//! PySCF's own output, which is always uniformly one or the other).

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use glam::DVec3;

use crate::units::ANGSTROM_PER_BOHR;
use crate::wavefunction::{BasisSet, MolecularOrbitals, Shell, Wavefunction};

pub struct MoldenGeometry {
    pub atomic_numbers: Vec<u32>,
    /// Angstrom.
    pub positions: Vec<[f32; 3]>,
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A Fortran-style `D`/`d` exponent (`1.23D-05`) alongside PySCF's own
/// plain `e`-notation — cheap to normalize unconditionally, so a stray
/// non-PySCF-authored file doesn't fail on this alone even though full
/// cross-program support isn't a goal yet.
fn parse_molden_float(token: &str) -> io::Result<f64> {
    token.replace(['D', 'd'], "e").parse::<f64>().map_err(|_| invalid_data(format!("malformed floating-point value '{token}' in .molden")))
}

/// One `[TAG]`-delimited section: `tag` and `trailer` are both the
/// uppercased content of the tag line (split at the first `]`) — `trailer`
/// carries e.g. `[Atoms] Angs`'s unit flag, which sits on the tag line
/// itself rather than as a separate content line. Blank lines and `#`
/// comments are dropped from `lines` as they're read, mirroring how
/// PySCF's own molden reader treats them.
struct Section {
    tag: String,
    trailer: String,
    lines: Vec<String>,
}

/// Splits the file into sections. `stop_after_atoms` lets the geometry-only
/// fast path skip the (potentially large) `[GTO]`/`[MO]` blocks entirely —
/// `[Atoms]` always comes first in a Molden file, same reasoning as
/// `.fchk`'s geometry/wavefunction parse split in `fchk.rs`/`wavefunction.rs`.
fn read_sections(reader: impl BufRead, stop_after_atoms: bool) -> io::Result<Vec<Section>> {
    let mut sections = Vec::new();
    let mut current: Option<Section> = None;
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('[') {
            if let Some(sec) = current.take() {
                let was_atoms = sec.tag == "ATOMS";
                sections.push(sec);
                if stop_after_atoms && was_atoms {
                    return Ok(sections);
                }
            }
            let close = rest.find(']').ok_or_else(|| invalid_data(format!("malformed section header '{trimmed}'")))?;
            let tag = rest[..close].trim().to_uppercase();
            let trailer = rest[close + 1..].trim().to_uppercase();
            current = Some(Section { tag, trailer, lines: Vec::new() });
        } else if let Some(sec) = current.as_mut() {
            sec.lines.push(trimmed.to_string());
        }
    }
    if let Some(sec) = current.take() {
        sections.push(sec);
    }
    Ok(sections)
}

struct MoldenAtom {
    atomic_number: u32,
    /// Bohr — same native unit `Shell::center` uses.
    position_bohr: DVec3,
}

fn parse_atoms_section(sec: &Section) -> io::Result<Vec<MoldenAtom>> {
    // Per the Molden format spec, coordinates are Bohr unless the tag
    // line itself says otherwise (`[Atoms] Angs`) — not a separate flag
    // line, so this has to come from `trailer`, not `lines`.
    let angstrom_input = sec.trailer.contains("ANG");
    let mut atoms = Vec::with_capacity(sec.lines.len());
    for line in &sec.lines {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 6 {
            return Err(invalid_data(format!("malformed [Atoms] line '{line}'")));
        }
        let atomic_number: u32 = tokens[2].parse().map_err(|_| invalid_data(format!("malformed atomic number in '{line}'")))?;
        let x = parse_molden_float(tokens[3])?;
        let y = parse_molden_float(tokens[4])?;
        let z = parse_molden_float(tokens[5])?;
        let position_bohr = if angstrom_input { DVec3::new(x, y, z) / ANGSTROM_PER_BOHR } else { DVec3::new(x, y, z) };
        atoms.push(MoldenAtom { atomic_number, position_bohr });
    }
    Ok(atoms)
}

/// Number of AO components a shell of this angular momentum/purity
/// contributes — matches `gto.rs::evaluate_basis_functions`'s own output
/// count per shell exactly (that's what lets `.molden` MO coefficients
/// slot in with zero reordering).
fn shell_component_count(l: u32, is_pure: bool) -> usize {
    if is_pure {
        (2 * l + 1) as usize
    } else {
        ((l + 1) * (l + 2) / 2) as usize
    }
}

fn angular_momentum_from_letter(c: char) -> Option<u32> {
    match c.to_ascii_lowercase() {
        's' => Some(0),
        'p' => Some(1),
        'd' => Some(2),
        'f' => Some(3),
        'g' => Some(4),
        _ => None,
    }
}

/// Scans every section for the `[5D]`/`[7F]`/`[9G]` (pure) vs `[6D]`/
/// `[10F]`/`[15G]` (Cartesian) marker tags — a global file flag, not
/// per-shell (see the module doc). Absence of either for a given angular
/// momentum defaults to Cartesian, matching the format spec.
fn detect_purity(sections: &[Section]) -> (bool, bool, bool) {
    let mut pure_d = false;
    let mut pure_f = false;
    let mut pure_g = false;
    for sec in sections {
        let tag = sec.tag.as_str();
        if tag.contains("5D") {
            pure_d = true;
        }
        if tag.contains("6D") {
            pure_d = false;
        }
        if tag.contains("7F") {
            pure_f = true;
        }
        if tag.contains("10F") {
            pure_f = false;
        }
        if tag.contains("9G") {
            pure_g = true;
        }
        if tag.contains("15G") {
            pure_g = false;
        }
    }
    (pure_d, pure_f, pure_g)
}

/// Parses `[GTO]`'s per-atom shell blocks. Mirrors PySCF's own reader's
/// trick for telling an atom-index line (`"3 0"`) apart from a primitive
/// line whose exponent happens to have no decimal point and so is also
/// all-digits (e.g. `"61420  9.07e-05"`): primitive lines are consumed
/// directly out of the shared iterator *inside* the shell-header branch,
/// so the outer loop never re-examines them as a possible atom index —
/// same shape as PySCF's `_parse_gto`/`read_one_bas`, not a coincidence.
fn parse_gto_section(sec: &Section, atoms: &[MoldenAtom], pure_d: bool, pure_f: bool, pure_g: bool) -> io::Result<Vec<Shell>> {
    let mut shells = Vec::new();
    let mut current_center: Option<DVec3> = None;
    let mut lines_iter = sec.lines.iter();
    while let Some(line) = lines_iter.next() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some(&first) = tokens.first() else { continue };

        if first.chars().all(|c| c.is_ascii_digit()) {
            let atom_index: usize = first.parse().unwrap();
            let atom = atoms
                .get(atom_index.wrapping_sub(1))
                .ok_or_else(|| invalid_data(format!("[GTO] references atom {atom_index}, only {} atoms in [Atoms]", atoms.len())))?;
            current_center = Some(atom.position_bohr);
            continue;
        }

        let shell_letter = first.chars().next().unwrap();
        let Some(l) = angular_momentum_from_letter(shell_letter) else {
            return Err(invalid_data(format!("unrecognized or unsupported shell type '{first}' in [GTO] (combined SP/L shells aren't supported yet)")));
        };
        if l > 4 {
            return Err(invalid_data(format!("angular momentum {l} (h and above) not supported yet")));
        }
        let n_prim: usize = tokens.get(1).ok_or_else(|| invalid_data(format!("malformed shell header '{line}' (missing primitive count)")))?.parse().map_err(|_| {
            invalid_data(format!("malformed primitive count in shell header '{line}'"))
        })?;
        let mut scale: f64 = tokens.get(2).map(|t| parse_molden_float(t)).transpose()?.unwrap_or(1.0);
        if scale == 0.0 {
            scale = 1.0;
        }
        let center = current_center.ok_or_else(|| invalid_data("[GTO] shell block appears before any atom index"))?;

        let mut exponents = Vec::with_capacity(n_prim);
        let mut coefficients = Vec::with_capacity(n_prim);
        for _ in 0..n_prim {
            let prim_line = lines_iter.next().ok_or_else(|| invalid_data("[GTO] shell truncated (missing primitive line)"))?;
            let ptoks: Vec<&str> = prim_line.split_whitespace().collect();
            if ptoks.len() < 2 {
                return Err(invalid_data(format!("malformed primitive line '{prim_line}'")));
            }
            exponents.push(parse_molden_float(ptoks[0])?);
            coefficients.push(parse_molden_float(ptoks[1])? * scale);
        }

        let is_pure = match l {
            2 => pure_d,
            3 => pure_f,
            4 => pure_g,
            _ => false,
        };
        shells.push(Shell { angular_momentum: l, is_pure, center, primitive_exponents: exponents, contraction_coefficients: coefficients });
    }
    Ok(shells)
}

struct MoldenOrbital {
    energy: f64,
    spin_beta: bool,
    occupation: f64,
    /// Indexed by (AO index - 1), same file-order convention as `Shell`s
    /// are read in — length always `num_basis_functions`.
    coefficients: Vec<f64>,
}

fn parse_mo_section(sec: &Section, num_basis_functions: usize) -> io::Result<Vec<MoldenOrbital>> {
    let mut orbitals = Vec::new();
    let mut energy = 0.0f64;
    let mut spin_beta = false;
    let mut occupation = 0.0f64;
    let mut coefficients: Option<Vec<f64>> = None;

    for line in &sec.lines {
        let upper = line.to_uppercase();
        if upper.starts_with("SYM") {
            if let Some(c) = coefficients.take() {
                orbitals.push(MoldenOrbital { energy, spin_beta, occupation, coefficients: c });
            }
            coefficients = Some(vec![0.0; num_basis_functions]);
        } else if upper.starts_with("ENE") {
            let v = upper.split('=').nth(1).ok_or_else(|| invalid_data(format!("malformed 'Ene=' line '{line}'")))?;
            energy = parse_molden_float(v.trim())?;
        } else if upper.starts_with("SPIN") {
            let v = upper.split('=').nth(1).ok_or_else(|| invalid_data(format!("malformed 'Spin=' line '{line}'")))?;
            spin_beta = v.trim().starts_with('B');
        } else if upper.starts_with("OCC") {
            let v = upper.split('=').nth(1).ok_or_else(|| invalid_data(format!("malformed 'Occup=' line '{line}'")))?;
            occupation = parse_molden_float(v.trim())?;
        } else {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() < 2 {
                continue;
            }
            let ao_id: usize = tokens[0].parse().map_err(|_| invalid_data(format!("malformed AO index in '{line}'")))?;
            let value = parse_molden_float(tokens[1])?;
            let c = coefficients.as_mut().ok_or_else(|| invalid_data("[MO] coefficient line appears before any 'Sym=' header"))?;
            if ao_id == 0 || ao_id > c.len() {
                return Err(invalid_data(format!("[MO] AO index {ao_id} out of range (expected 1..={})", c.len())));
            }
            c[ao_id - 1] = value;
        }
    }
    if let Some(c) = coefficients.take() {
        orbitals.push(MoldenOrbital { energy, spin_beta, occupation, coefficients: c });
    }
    Ok(orbitals)
}

/// Fast geometry-only parse — stops right after `[Atoms]`, same reasoning
/// as `fchk::parse_fchk_geometry` staying separate from
/// `parse_fchk_wavefunction`.
pub fn parse_molden_geometry(path: &Path) -> io::Result<MoldenGeometry> {
    let file = File::open(path)?;
    let sections = read_sections(BufReader::new(file), true)?;
    let atoms_section = sections.iter().find(|s| s.tag == "ATOMS").ok_or_else(|| invalid_data(".molden file missing [Atoms] section"))?;
    let atoms = parse_atoms_section(atoms_section)?;
    let atomic_numbers = atoms.iter().map(|a| a.atomic_number).collect();
    let positions = atoms.iter().map(|a| (a.position_bohr * ANGSTROM_PER_BOHR).as_vec3().to_array()).collect();
    Ok(MoldenGeometry { atomic_numbers, positions })
}

/// Full parse — basis set + MO coefficients, ready for
/// `gto::evaluate_mo`/`generate_mo_grids`, same contract as
/// `parse_fchk_wavefunction`. Restricted vs unrestricted is detected the
/// same way as the `.fchk` path: by whether any `Spin= Beta` orbital was
/// actually present, not by electron count/multiplicity.
pub fn parse_molden_wavefunction(path: &Path) -> io::Result<Wavefunction> {
    let file = File::open(path)?;
    let sections = read_sections(BufReader::new(file), false)?;

    let atoms_section = sections.iter().find(|s| s.tag == "ATOMS").ok_or_else(|| invalid_data(".molden file missing [Atoms] section"))?;
    let atoms = parse_atoms_section(atoms_section)?;

    let gto_section = sections.iter().find(|s| s.tag == "GTO").ok_or_else(|| invalid_data(".molden file missing [GTO] section"))?;
    let (pure_d, pure_f, pure_g) = detect_purity(&sections);
    let shells = parse_gto_section(gto_section, &atoms, pure_d, pure_f, pure_g)?;
    let num_basis_functions: usize = shells.iter().map(|s| shell_component_count(s.angular_momentum, s.is_pure)).sum();

    let mo_section = sections.iter().find(|s| s.tag == "MO").ok_or_else(|| invalid_data(".molden file missing [MO] section"))?;
    let orbitals = parse_mo_section(mo_section, num_basis_functions)?;

    let (alpha_orbitals, beta_orbitals): (Vec<_>, Vec<_>) = orbitals.into_iter().partition(|o| !o.spin_beta);
    if alpha_orbitals.is_empty() {
        return Err(invalid_data(".molden [MO] section has no alpha orbitals"));
    }

    let build_channel = |orbitals: Vec<MoldenOrbital>| -> MolecularOrbitals {
        // An orbital's Occup is 2.0 (restricted) or 1.0 (unrestricted) when
        // occupied, 0.0 when virtual — either way ">0.5" cleanly separates
        // them, giving the occupied *orbital* count `num_occupied` expects
        // (not an electron count, though they coincide for the alpha
        // channel — see `MolecularOrbitals::num_occupied`'s own doc).
        let num_occupied = orbitals.iter().filter(|o| o.occupation > 0.5).count();
        let orbital_energies = orbitals.iter().map(|o| o.energy).collect();
        let mut coefficients = Vec::with_capacity(orbitals.len() * num_basis_functions);
        for o in &orbitals {
            coefficients.extend_from_slice(&o.coefficients);
        }
        MolecularOrbitals { num_basis_functions, orbital_energies, coefficients, num_occupied }
    };

    let alpha = build_channel(alpha_orbitals);
    let beta = if beta_orbitals.is_empty() { None } else { Some(build_channel(beta_orbitals)) };

    Ok(Wavefunction { basis: BasisSet { shells }, alpha, beta })
}
