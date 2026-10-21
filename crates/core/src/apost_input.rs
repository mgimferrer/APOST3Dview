//! APOST-3D input files (`<job>.inp`) for the input generator: the few
//! choices it offers (atomic definition, EOS/GEOS, OSLO, SPIN, ENPART with
//! its functional and a faster two-electron grid, fragments), written the
//! way APOST-3D's input reference describes (`docs/source/input/`), and
//! read back from an existing input, naming whatever else that input sets.
//!
//! Deliberately small (Martí, 2026-10-21): the generator is for users who
//! don't write inputs by hand; everything else stays in the input
//! reference. Options at APOST-3D's default are not written.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

/// The atomic definition (`# METHOD`), exactly one per run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AimScheme {
    Tfvc,
    Hirsh,
    HirshIt,
    BeckeRho,
    Becke,
    Mulliken,
    Lowdin,
    LowdinDavidson,
    NaoBasis,
}

impl AimScheme {
    pub const ALL: [AimScheme; 9] = [
        AimScheme::Tfvc,
        AimScheme::Mulliken,
        AimScheme::Lowdin,
        AimScheme::LowdinDavidson,
        AimScheme::NaoBasis,
        AimScheme::Hirsh,
        AimScheme::HirshIt,
        AimScheme::BeckeRho,
        AimScheme::Becke,
    ];

    pub fn keyword(self) -> &'static str {
        match self {
            AimScheme::Tfvc => "TFVC",
            AimScheme::Hirsh => "HIRSH",
            AimScheme::HirshIt => "HIRSH-IT",
            AimScheme::BeckeRho => "BECKE-RHO",
            AimScheme::Becke => "BECKE",
            AimScheme::Mulliken => "MULLIKEN",
            AimScheme::Lowdin => "LOWDIN",
            AimScheme::LowdinDavidson => "LOWDIN-DAVIDSON",
            AimScheme::NaoBasis => "NAO-BASIS",
        }
    }

    /// `MULLI` is accepted for `MULLIKEN`.
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        if keyword == "MULLI" {
            return Some(AimScheme::Mulliken);
        }
        Self::ALL.into_iter().find(|s| s.keyword() == keyword)
    }

    pub fn name(self) -> &'static str {
        match self {
            AimScheme::Tfvc => "Topological fuzzy Voronoi cells",
            AimScheme::Hirsh => "Hirshfeld",
            AimScheme::HirshIt => "Iterative Hirshfeld",
            AimScheme::BeckeRho => "Becke, radii from the density",
            AimScheme::Becke => "Becke, fixed radii",
            AimScheme::Mulliken => "Mulliken",
            AimScheme::Lowdin => "Löwdin",
            AimScheme::LowdinDavidson => "Löwdin-Davidson",
            AimScheme::NaoBasis => "Natural atomic orbitals",
        }
    }

    pub fn real_space(self) -> bool {
        matches!(self, AimScheme::Tfvc | AimScheme::Hirsh | AimScheme::HirshIt | AimScheme::BeckeRho | AimScheme::Becke)
    }
}

/// Oxidation states from effective orbitals: EOS or GEOS (APOST-3D does
/// GEOS when both are given, so they are one choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OxidationStates {
    Eos,
    Geos,
}

/// The energy expression of ENPART.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnpartMethod {
    Hf,
    /// One of `ENPART_FUNCTIONALS`.
    Functional(String),
    /// CASSCF, also FCI and CISD wavefunctions written by APOST-3D's pySCF
    /// script.
    Casscf,
}

/// APOST-3D's predefined functional keywords, each reproducing Gaussian
/// 16's functional of the same name, with a short description.
pub const ENPART_FUNCTIONALS: [(&str, &str); 10] = [
    ("SVWN", "Slater + VWN (RPA), Gaussian's SVWN"),
    ("SVWN5", "Slater + VWN5"),
    ("BLYP", "Becke 88 + LYP"),
    ("BP86", "Becke 88 + Perdew 86"),
    ("PBE", "PBE exchange and correlation (Gaussian's PBEPBE)"),
    ("B3LYP", "Gaussian's B3LYP (VWN-RPA)"),
    ("B3PW91", "B3PW91"),
    ("B3P86", "Gaussian's B3P86"),
    ("PBE0", "PBE0 (Gaussian's PBE1PBE)"),
    ("BHANDHLYP", "Gaussian's BHandHLYP"),
];

/// A functional or method name as APOST-3D's keyword: the predefined
/// keywords, case-insensitive, and Gaussian's other spellings of two of
/// them.
fn enpart_keyword(name: &str) -> Option<EnpartMethod> {
    let upper = name.to_ascii_uppercase();
    match upper.as_str() {
        "HF" => Some(EnpartMethod::Hf),
        "CASSCF" => Some(EnpartMethod::Casscf),
        "PBEPBE" => Some(EnpartMethod::Functional("PBE".into())),
        "PBE1PBE" => Some(EnpartMethod::Functional("PBE0".into())),
        other => ENPART_FUNCTIONALS.iter().find(|(k, _)| *k == other).map(|(k, _)| EnpartMethod::Functional(k.to_string())),
    }
}

/// The two-electron grid the "faster" ENPART choice sets (radial ×
/// angular), instead of APOST-3D's default 150 × 590.
pub const FAST_TWO_ELECTRON_GRID: (u32, u32) = (40, 146);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnpartOptions {
    /// `None` until known: the input is incomplete without it.
    pub method: Option<EnpartMethod>,
    pub fast_grid: bool,
}

/// The `# DM` block of a correlated wavefunction from pySCF: `DM 2`, the
/// 1- and 2-RDM files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmFiles {
    pub dm1: String,
    pub dm2: String,
}

/// An APOST-3D input, as far as the generator handles one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApostInput {
    pub aim: AimScheme,
    pub oxidation: Option<OxidationStates>,
    pub oslo: bool,
    pub spin: bool,
    pub enpart: Option<EnpartOptions>,
    /// `DOFRAGS` and `# FRAGMENTS`: atoms 1-based, in fragment order.
    pub fragments: Option<Vec<Vec<usize>>>,
    pub dm: Option<DmFiles>,
    pub qchem: bool,
}

impl Default for ApostInput {
    fn default() -> Self {
        Self { aim: AimScheme::Tfvc, oxidation: None, oslo: false, spin: false, enpart: None, fragments: None, dm: None, qchem: false }
    }
}

impl ApostInput {
    /// The input text, `# METHOD` first, then the blocks in the order of
    /// the input reference. The last fragment is written `-1` (all
    /// remaining atoms) when it holds exactly the atoms of no other one.
    pub fn to_text(&self, n_atoms: usize) -> String {
        let mut out = String::new();
        let mut block = |name: &str, lines: &[String]| {
            out.push_str(&format!("# {name}\n"));
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
            out.push_str("#\n");
        };

        let mut method = vec![self.aim.keyword().to_string()];
        match self.oxidation {
            Some(OxidationStates::Eos) => method.push("EOS".into()),
            Some(OxidationStates::Geos) => method.push("GEOS".into()),
            None => {}
        }
        if self.oslo {
            method.push("OSLO".into());
        }
        if self.spin {
            method.push("SPIN".into());
        }
        if self.enpart.is_some() {
            method.push("ENPART".into());
        }
        if self.fragments.is_some() {
            method.push("DOFRAGS".into());
        }
        if self.dm.is_some() {
            method.push("DM 2".into());
        }
        if self.qchem {
            method.push("QCHEM".into());
        }
        block("METHOD", &method);

        if let Some(fragments) = &self.fragments {
            let mut lines = vec![fragments.len().to_string()];
            let listed: usize = fragments.iter().map(Vec::len).sum();
            for (i, atoms) in fragments.iter().enumerate() {
                if i > 0 && i + 1 == fragments.len() && listed == n_atoms {
                    lines.push("-1".into());
                } else {
                    lines.push(atoms.len().to_string());
                    lines.push(atoms.iter().map(usize::to_string).collect::<Vec<_>>().join(" "));
                }
            }
            block("FRAGMENTS", &lines);
        }
        if let Some(enpart) = &self.enpart {
            let line = match &enpart.method {
                Some(EnpartMethod::Hf) => vec!["HF".to_string()],
                Some(EnpartMethod::Functional(name)) => vec![name.clone()],
                Some(EnpartMethod::Casscf) => vec!["CASSCF".to_string()],
                None => Vec::new(),
            };
            block("ENPART", &line);
        }
        if self.oslo {
            block("OSLO", &[]);
        }
        if let Some(dm) = &self.dm {
            block("DM", &[dm.dm1.clone(), dm.dm2.clone(), "pySCF".into()]);
        }
        if let Some(enpart) = &self.enpart
            && enpart.fast_grid
        {
            let (radial, angular) = FAST_TWO_ELECTRON_GRID;
            block("GRID", &[format!("RADIAL_2E {radial}"), format!("ANGULAR_2E {angular}")]);
        }
        out
    }

    /// The analyses that need every atom in a fragment.
    pub fn needs_fragments(&self) -> bool {
        self.oxidation.is_some() || self.oslo
    }

    /// The same input as APOST-3D runs it: `DM 2` switches on the local
    /// spin analysis. For comparing inputs by meaning.
    pub fn normalized(&self) -> Self {
        let mut input = self.clone();
        input.spin |= input.dm.is_some();
        input
    }

    /// What APOST-3D would stop on, or what the input still lacks, in
    /// words, for a molecule of `n_atoms` atoms.
    pub fn problems(&self, n_atoms: usize) -> Vec<String> {
        let mut problems = Vec::new();
        if !self.aim.real_space() {
            let keyword = self.aim.keyword();
            if self.oxidation == Some(OxidationStates::Geos) {
                problems.push(format!("GEOS needs a real-space atomic definition (TFVC), not {keyword}."));
            }
            if self.oslo {
                problems.push(format!("OSLO needs a real-space atomic definition (TFVC), not {keyword}."));
            }
            if self.enpart.is_some() {
                problems.push(format!("ENPART needs a real-space atomic definition (TFVC), not {keyword}."));
            }
        }
        if let Some(enpart) = &self.enpart
            && enpart.method.is_none()
        {
            problems.push("ENPART: choose the functional (or HF) of the calculation.".into());
        }
        let fragments = self.fragments.as_deref().unwrap_or(&[]);
        if self.needs_fragments() && fragments.is_empty() {
            problems.push("EOS, GEOS and OSLO need fragments.".into());
        }
        if !fragments.is_empty() && (self.needs_fragments() || self.enpart.is_some()) {
            let missing = (1..=n_atoms).filter(|a| !fragments.iter().any(|f| f.contains(a))).count();
            if missing > 0 {
                let s = if missing == 1 { " is" } else { "s are" };
                problems.push(format!("{missing} atom{s} in no fragment: EOS, GEOS, OSLO and ENPART need every atom in one."));
            }
        }
        problems
    }

    /// Fills in what the `.fchk` decides by itself, for job `job`: `QCHEM`
    /// for a Q-Chem file; for a correlated wavefunction the RDM files
    /// (`DM 2`, which also does the local spin analysis) when SPIN or
    /// ENPART need them, or always for an open shell, and CASSCF as the
    /// ENPART method; otherwise the ENPART functional named in the file,
    /// if it has not been chosen yet.
    pub fn apply_fchk(&mut self, facts: &FchkFacts, job: &str) {
        self.qchem = facts.program == FchkProgram::QChem;
        let needs_rdms = facts.correlated && (self.spin || self.enpart.is_some() || facts.open_shell);
        if needs_rdms {
            if self.dm.is_none() {
                self.dm = Some(DmFiles { dm1: format!("{job}.dm1"), dm2: format!("{job}.dm2") });
            }
        } else {
            self.dm = None;
        }
        if let Some(enpart) = &mut self.enpart {
            if facts.correlated {
                enpart.method = Some(EnpartMethod::Casscf);
            } else if enpart.method.is_none() || enpart.method == Some(EnpartMethod::Casscf) {
                enpart.method = facts.enpart_method();
            }
        }
    }
}

/// An existing input read back: what the generator handles, and the rest
/// in words ("CUBE", "THREBOD 10", "# GRID: RADIAL 70"), which saving from
/// the generator would leave out.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadInput {
    pub input: ApostInput,
    pub other: Vec<String>,
}

/// A block: its name (`METHOD` for `# METHOD`) and its lines.
struct Block<'a> {
    name: &'a str,
    lines: Vec<&'a str>,
}

/// Splits an input into blocks the way APOST-3D finds them: a block starts
/// at a header line (`# NAME`) and ends at the next line containing `#`;
/// lines outside blocks are ignored, and only a block's first occurrence
/// counts.
fn blocks(text: &str) -> Vec<Block<'_>> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut current: Option<Block> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            if let Some(block) = current.take()
                && !blocks.iter().any(|b| b.name == block.name)
            {
                blocks.push(block);
            }
            let name = rest.trim();
            if !name.is_empty() && !name.contains('#') {
                current = Some(Block { name, lines: Vec::new() });
            }
        } else if line.contains('#') {
            if let Some(block) = current.take()
                && !blocks.iter().any(|b| b.name == block.name)
            {
                blocks.push(block);
            }
        } else if let Some(block) = &mut current
            && !trimmed.is_empty()
        {
            block.lines.push(trimmed);
        }
    }
    if let Some(block) = current
        && !blocks.iter().any(|b| b.name == block.name)
    {
        blocks.push(block);
    }
    blocks
}

/// A line's keyword and value: `DM 2`, `DM=2` and `DM = 2` all give
/// ("DM", "2").
fn keyword_value(line: &str) -> (&str, &str) {
    let end = line.find(|c: char| c.is_whitespace() || c == '=').unwrap_or(line.len());
    let value = line[end..].trim_start().trim_start_matches('=').trim();
    (&line[..end], value)
}

/// The fragments of a `# FRAGMENTS` block (atoms 1-based), `-1` on the last
/// one expanded to the remaining atoms of `n_atoms`. `None` if the block
/// can't be read.
fn read_fragments(lines: &[&str], n_atoms: usize) -> Option<Vec<Vec<usize>>> {
    let numbers: Vec<i64> = lines.iter().flat_map(|l| l.split_whitespace()).map(|t| t.parse().ok()).collect::<Option<_>>()?;
    let mut numbers = numbers.into_iter();
    let count = usize::try_from(numbers.next()?).ok()?;
    let mut fragments: Vec<Vec<usize>> = Vec::with_capacity(count);
    for i in 0..count {
        let size = numbers.next()?;
        if size == -1 && i + 1 == count {
            let rest = (1..=n_atoms).filter(|a| !fragments.iter().any(|f| f.contains(a))).collect();
            fragments.push(rest);
        } else {
            let size = usize::try_from(size).ok()?;
            let atoms = (0..size).map(|_| numbers.next().and_then(|a| usize::try_from(a).ok())).collect::<Option<Vec<_>>>()?;
            fragments.push(atoms);
        }
    }
    Some(fragments)
}

/// Reads an APOST-3D input for a molecule of `n_atoms` atoms. Settings
/// equal to APOST-3D's defaults (`FOLI_TOLERANCE 3`, `THREBOD 50`, the
/// default grids, ...) count as handled.
pub fn read_apost_input(text: &str, n_atoms: usize) -> ReadInput {
    let blocks = blocks(text);
    let block = |name: &str| blocks.iter().find(|b| b.name == name).map(|b| b.lines.as_slice()).unwrap_or(&[]);
    let has_block = |name: &str| blocks.iter().any(|b| b.name == name);
    let mut input = ApostInput::default();
    let mut other = Vec::new();
    let mut aims = Vec::new();
    let (mut eos, mut geos, mut enpart, mut dofrags, mut dm) = (false, false, false, false, false);
    let mut uses_cube = false;

    for &line in block("METHOD") {
        let (keyword, value) = keyword_value(line);
        if let Some(scheme) = AimScheme::from_keyword(keyword) {
            aims.push(scheme);
            continue;
        }
        match (keyword, value) {
            ("EOS", "") => eos = true,
            ("GEOS", "") => geos = true,
            ("OSLO", "") => input.oslo = true,
            ("SPIN", "") => input.spin = true,
            ("ENPART", "") => enpart = true,
            ("DOFRAGS", "") => dofrags = true,
            ("QCHEM", "") => input.qchem = true,
            ("DM", "2") => dm = true,
            ("DENS", "1") | ("VERBOSE", "0") => {}
            ("CUBE", _) => {
                uses_cube = true;
                other.push("CUBE (cube files of the effective orbitals)".to_string());
            }
            _ => other.push(line.to_string()),
        }
    }
    if let Some((first, rest)) = aims.split_first() {
        input.aim = *first;
        other.extend(rest.iter().map(|s| format!("{} (a second atomic definition: APOST-3D stops)", s.keyword())));
    }
    input.oxidation = if geos { Some(OxidationStates::Geos) } else if eos { Some(OxidationStates::Eos) } else { None };

    if dofrags {
        match read_fragments(block("FRAGMENTS"), n_atoms) {
            Some(fragments) => input.fragments = Some(fragments),
            None => other.push("# FRAGMENTS (could not be read)".into()),
        }
    }

    let mut fast_grid = false;
    if has_block("GRID") {
        let mut radial_2e = None;
        let mut angular_2e = None;
        for &line in block("GRID") {
            match keyword_value(line) {
                ("RADIAL_2E", v) if enpart => radial_2e = v.parse::<u32>().ok(),
                ("ANGULAR_2E", v) if enpart => angular_2e = v.parse::<u32>().ok(),
                _ => other.push(format!("# GRID: {line}")),
            }
        }
        match (radial_2e, angular_2e) {
            (None, None) | (Some(150), Some(590)) | (Some(150), None) | (None, Some(590)) => {}
            (Some(r), Some(a)) if (r, a) == FAST_TWO_ELECTRON_GRID => fast_grid = true,
            (r, a) => other.push(format!("# GRID: two-electron grid {} \u{d7} {}", r.unwrap_or(150), a.unwrap_or(590))),
        }
    }

    if enpart {
        let mut method = None;
        for &line in block("ENPART") {
            let (keyword, value) = keyword_value(line);
            match (keyword, value) {
                ("THREBOD", "50") | ("TWOELTOLER", "0.25") => {}
                (k, "") if method.is_none() && enpart_keyword(k).is_some() => method = enpart_keyword(k),
                _ => other.push(format!("# ENPART: {line}")),
            }
        }
        input.enpart = Some(EnpartOptions { method, fast_grid });
    }

    if input.oslo {
        for &line in block("OSLO") {
            match keyword_value(line) {
                ("FOLI_TOLERANCE", "3") | ("BRANCH_ITERATION", "0") => {}
                _ => other.push(format!("# OSLO: {line}")),
            }
        }
    }

    if dm {
        match block("DM") {
            [dm1, dm2, program] if *program == "pySCF" => input.dm = Some(DmFiles { dm1: dm1.to_string(), dm2: dm2.to_string() }),
            lines => other.push(format!("DM 2 with # DM: {}", lines.join(" / "))),
        }
    }
    if uses_cube && !block("CUBE").is_empty() {
        other.push(format!("# CUBE: {}", block("CUBE").join(", ")));
    }
    ReadInput { input, other }
}

/// The program that wrote a `.fchk`, as far as the file tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FchkProgram {
    Gaussian,
    QChem,
    PySCF,
    Unknown,
}

/// What a `.fchk` says about how to fill in an input for it.
#[derive(Debug, Clone, PartialEq)]
pub struct FchkFacts {
    pub program: FchkProgram,
    /// The method on line 2 ("RB3LYP", "UHF", "CASSCF", ...).
    pub method: String,
    pub unrestricted: bool,
    /// Multiplicity above 1.
    pub open_shell: bool,
    /// A CASSCF/FCI/CISD (or CCSD) file from APOST-3D's pySCF script.
    pub correlated: bool,
    /// The kinetic, electron-nuclear and electron-electron energies that
    /// ENPART's error checks use.
    pub reference_energies: bool,
}

impl FchkFacts {
    /// The ENPART method the `.fchk`'s method line names, if APOST-3D has
    /// a keyword for it (Gaussian's names, without the R/U/RO prefix).
    /// pySCF writes "RKS"/"UKS" without the functional: `None`.
    pub fn enpart_method(&self) -> Option<EnpartMethod> {
        if self.correlated {
            return Some(EnpartMethod::Casscf);
        }
        let upper = self.method.to_ascii_uppercase();
        if upper == "RO" || upper == "R" || upper == "U" {
            return None;
        }
        let name = upper.strip_prefix("RO").or_else(|| upper.strip_prefix('R')).or_else(|| upper.strip_prefix('U')).unwrap_or(&upper);
        enpart_keyword(name).filter(|m| *m != EnpartMethod::Casscf)
    }
}

/// Reads `FchkFacts` from a `.fchk`, looking at the headers only (array
/// values are skipped, so a large file reads quickly).
pub fn read_fchk_facts(path: &Path) -> io::Result<FchkFacts> {
    let mut lines = BufReader::new(File::open(path)?).lines();
    let _title = lines.next().transpose()?;
    let line2 = lines.next().transpose()?.unwrap_or_default();
    let method = line2.split_whitespace().nth(1).unwrap_or_default().to_string();
    let mut facts = FchkFacts { program: FchkProgram::Unknown, method, unrestricted: false, open_shell: false, correlated: false, reference_energies: false };
    let mut seen_coefficients = false;
    let mut energies_after_coefficients = false;
    let mut reference = 0;
    let mut gaussian_version = false;
    while let Some(line) = lines.next() {
        let line = line?;
        let Some(header) = crate::fchk::parse_header(&line) else { continue };
        match header.label.as_str() {
            "Alpha MO coefficients" => seen_coefficients = true,
            "Alpha Orbital Energies" if seen_coefficients => energies_after_coefficients = true,
            "Beta MO coefficients" => facts.unrestricted = true,
            "Gaussian Version" => gaussian_version = true,
            "Multiplicity" => facts.open_shell = crate::fchk::scalar_value::<u32>(&line).is_some_and(|m| m > 1),
            "Kinetic Energy" | "Electron-Nuclei Energy" | "Electron-Electron Energy" => reference += 1,
            label if label.starts_with("Number of CAS") => facts.correlated = true,
            _ => {}
        }
        if let crate::fchk::HeaderKind::Array(count) = header.kind {
            let per_line = if line.split_whitespace().any(|t| t == "I") { 6 } else { 5 };
            for _ in 0..count.div_ceil(per_line) {
                lines.next().transpose()?;
            }
        }
    }
    facts.reference_energies = reference == 3;
    let upper = facts.method.to_ascii_uppercase();
    if upper.contains("CAS") || upper == "CCSD" {
        facts.correlated = true;
    }
    facts.program = if energies_after_coefficients {
        FchkProgram::QChem
    } else if gaussian_version {
        FchkProgram::Gaussian
    } else if facts.correlated || matches!(upper.as_str(), "RKS" | "UKS" | "ROKS" | "RESTRICTED") || upper.starts_with("UNRESTRICTED") {
        FchkProgram::PySCF
    } else {
        FchkProgram::Unknown
    };
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eos(fragments: Vec<Vec<usize>>) -> ApostInput {
        ApostInput { oxidation: Some(OxidationStates::Eos), fragments: Some(fragments), ..Default::default() }
    }

    #[test]
    fn writes_the_documented_layout() {
        assert_eq!(eos(vec![vec![1], vec![2, 3, 4]]).to_text(4), "# METHOD\nTFVC\nEOS\nDOFRAGS\n#\n# FRAGMENTS\n2\n1\n1\n-1\n#\n");
        // A last fragment that leaves atoms out is written out in full.
        assert_eq!(eos(vec![vec![1], vec![2, 3]]).to_text(4), "# METHOD\nTFVC\nEOS\nDOFRAGS\n#\n# FRAGMENTS\n2\n1\n1\n2\n2 3\n#\n");
        let enpart = ApostInput { enpart: Some(EnpartOptions { method: Some(EnpartMethod::Hf), fast_grid: true }), oslo: true, ..Default::default() };
        assert_eq!(enpart.to_text(3), "# METHOD\nTFVC\nOSLO\nENPART\n#\n# ENPART\nHF\n#\n# OSLO\n#\n# GRID\nRADIAL_2E 40\nANGULAR_2E 146\n#\n");
    }

    #[test]
    fn reads_what_it_writes() {
        let input = ApostInput {
            aim: AimScheme::Tfvc,
            oxidation: Some(OxidationStates::Geos),
            oslo: true,
            spin: true,
            enpart: Some(EnpartOptions { method: Some(EnpartMethod::Casscf), fast_grid: true }),
            fragments: Some(vec![vec![2], vec![1, 3]]),
            dm: Some(DmFiles { dm1: "a.dm1".into(), dm2: "a.dm2".into() }),
            qchem: true,
        };
        let read = read_apost_input(&input.to_text(3), 3);
        assert_eq!(read.input, input);
        assert!(read.other.is_empty(), "{:?}", read.other);
    }

    #[test]
    fn follows_apost3d_reading_rules() {
        // Values after a space or "="; a line containing "#" ends a block;
        // a long atom list continues on the next line; MULLI is MULLIKEN;
        // explicit defaults count as handled; EOS with GEOS is GEOS.
        let text = "junk before\n# METHOD\nMULLI\nEOS\nGEOS\nDOFRAGS\nDM=2\nVERBOSE 1\n#\nSPIN\n# FRAGMENTS\n2\n3\n1 2\n4\n-1\n#\n# DM\nx.dm1\nx.dm2\npySCF\n#\n";
        let read = read_apost_input(text, 5);
        assert_eq!(read.input.aim, AimScheme::Mulliken);
        assert_eq!(read.input.oxidation, Some(OxidationStates::Geos));
        assert!(!read.input.spin, "SPIN after the block's end is ignored");
        assert_eq!(read.input.fragments, Some(vec![vec![1, 2, 4], vec![3, 5]]));
        assert_eq!(read.input.dm, Some(DmFiles { dm1: "x.dm1".into(), dm2: "x.dm2".into() }));
        assert_eq!(read.other, vec!["VERBOSE 1"]);

        let text = "# METHOD\nTFVC\nENPART\nOSLO\nCUBE\n#\n# ENPART\nb3lyp\nTHREBOD 50\nTHREBOD 10\n# \n# OSLO\nFOLI_TOLERANCE 3\nPRINT_NONORTHO\n#\n# GRID\nRADIAL_2E 40\nANGULAR_2E 146\nRADIAL 70\n#\n# CUBE\nMAX_OCC 700\n#\n";
        let read = read_apost_input(text, 2);
        assert_eq!(read.input.enpart, Some(EnpartOptions { method: Some(EnpartMethod::Functional("B3LYP".into())), fast_grid: true }));
        assert_eq!(read.other, vec!["CUBE (cube files of the effective orbitals)", "# GRID: RADIAL 70", "# ENPART: THREBOD 10", "# OSLO: PRINT_NONORTHO", "# CUBE: MAX_OCC 700"]);
    }

    #[test]
    fn problems_name_what_stops_apost3d() {
        assert!(eos(vec![vec![1], vec![2, 3]]).problems(3).is_empty());
        assert_eq!(eos(vec![vec![1]]).problems(3), vec!["2 atoms are in no fragment: EOS, GEOS, OSLO and ENPART need every atom in one."]);
        let geos = ApostInput { aim: AimScheme::Lowdin, oxidation: Some(OxidationStates::Geos), ..Default::default() };
        assert_eq!(geos.problems(2).len(), 2);
        let enpart = ApostInput { enpart: Some(EnpartOptions { method: None, fast_grid: false }), ..Default::default() };
        assert_eq!(enpart.problems(2), vec!["ENPART: choose the functional (or HF) of the calculation."]);
    }

    #[test]
    fn functional_from_the_method_line() {
        let facts = |method: &str| FchkFacts { program: FchkProgram::Gaussian, method: method.into(), unrestricted: false, open_shell: false, correlated: false, reference_energies: true };
        assert_eq!(facts("RB3LYP").enpart_method(), Some(EnpartMethod::Functional("B3LYP".into())));
        assert_eq!(facts("UBLYP").enpart_method(), Some(EnpartMethod::Functional("BLYP".into())));
        assert_eq!(facts("RPBEPBE").enpart_method(), Some(EnpartMethod::Functional("PBE".into())));
        assert_eq!(facts("RPBE1PBE").enpart_method(), Some(EnpartMethod::Functional("PBE0".into())));
        assert_eq!(facts("ROHF").enpart_method(), Some(EnpartMethod::Hf));
        assert_eq!(facts("UHF").enpart_method(), Some(EnpartMethod::Hf));
        assert_eq!(facts("RTPSSTPSS").enpart_method(), None);
        assert_eq!(facts("RKS").enpart_method(), None);
        assert_eq!(facts("R").enpart_method(), None);
    }
}
