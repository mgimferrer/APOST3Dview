//! APOST-3D results file (`<job>.apost.json`): data model, loading,
//! discovery next to a `.fchk`, and the consistency check against it.
//!
//! The schema is defined in the APOST-3D repository
//! (`docs/source/developer/json-output.md`), which is the source of truth.
//! Reading rules from it that shape this module:
//! - `schema_version` is "major.minor". Any minor version of a known major
//!   is accepted and unknown keys are ignored (serde's default); an unknown
//!   major is refused with a message, before deserializing anything else.
//! - A run that stopped on an error still writes a complete file, but may
//!   lack every block after the point where it stopped, so every block is
//!   optional here.
//! - `null` means "not available": every scalar is an `Option`.
//! - Atom and fragment numbers are 1-based in the file. They are kept that
//!   way in `Fragment::atoms`; helpers that return atom indices for the
//!   viewer (`fragment_of_atom`, `FragmentCoverage`) return 0-based ones.
//! - Values are reached through the `index` (since 0.3): one entry per
//!   quantity that can be shown on atoms, atom pairs, fragments or
//!   fragment pairs, with a JSON Pointer to it. The viewer offers what the
//!   index lists, without code per analysis; `analyses` itself stays raw
//!   JSON. Fragment values are read from the file, never summed from
//!   atoms (several later quantities are not sums).

use std::path::{Path, PathBuf};

use glam::Vec3;
use serde::Deserialize;
use serde_json::Value;

use crate::fchk::FchkSummary;
use crate::molecule::Molecule;
use crate::units::ANGSTROM_PER_BOHR;

pub const RESULTS_SCHEMA: &str = "apost3d-results";
/// The only schema major version this viewer understands.
pub const SUPPORTED_SCHEMA_MAJOR: u32 = 0;
/// File name suffix of an APOST-3D results file.
pub const RESULTS_SUFFIX: &str = ".apost.json";

/// Coordinates in the results file and the `.fchk` must agree within this
/// distance. Both hold the same double-precision numbers, so any real
/// difference means a different geometry.
const COORDINATE_TOLERANCE_BOHR: f64 = 1e-4;
/// `energy_au` against the `.fchk`'s "SCF Energy". Different methods on the
/// same geometry and basis set differ by far more than this.
const ENERGY_TOLERANCE_AU: f64 = 1e-6;

#[derive(Debug, Clone, Deserialize)]
pub struct ApostResults {
    pub schema_version: String,
    pub program: Option<ProgramInfo>,
    pub wavefunction: Option<WavefunctionSummary>,
    pub input: Option<InputInfo>,
    pub molecule: Option<MoleculeBlock>,
    pub fragments: Option<Vec<Fragment>>,
    pub analyses: Option<serde_json::Map<String, Value>>,
    /// Every quantity that can be shown (since 0.3), in file order. Only
    /// entries whose values were found with the expected shape are kept;
    /// the others are described in `index_problems`.
    #[serde(default)]
    pub index: Vec<IndexEntry>,
    pub run: Option<RunInfo>,
    #[serde(skip)]
    pub index_problems: Vec<String>,
    /// The whole file, for following the index's JSON Pointers.
    #[serde(skip)]
    raw: Value,
}

/// What an index entry holds, and over what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantityKind {
    /// A single number (0.4).
    Scalar,
    AtomVector,
    AtomMatrix,
    FragmentVector,
    FragmentMatrix,
    /// Several quantities per fragment or per orbital (0.5).
    Table,
    /// A kind this viewer doesn't know yet.
    #[serde(other)]
    Other,
}

impl QuantityKind {
    pub fn is_matrix(self) -> bool {
        matches!(self, QuantityKind::AtomMatrix | QuantityKind::FragmentMatrix)
    }

    pub fn on_fragments(self) -> bool {
        matches!(self, QuantityKind::FragmentVector | QuantityKind::FragmentMatrix)
    }

    /// Values drawn on the molecule: per atom or fragment, or pairs of them.
    pub fn on_molecule(self) -> bool {
        matches!(self, QuantityKind::AtomVector | QuantityKind::AtomMatrix | QuantityKind::FragmentVector | QuantityKind::FragmentMatrix)
    }
}

/// One displayable quantity (`index` entry).
#[derive(Debug, Clone, Deserialize)]
pub struct IndexEntry {
    /// Stable name ("populations.atoms.charges"), to remember a choice
    /// across files and sessions.
    pub id: String,
    pub analysis: Option<String>,
    /// "alpha", "beta", "total", or `None` when not given per spin.
    pub spin: Option<String>,
    /// JSON Pointer to the values (to the matrix object for a matrix).
    pub path: String,
    pub kind: QuantityKind,
    /// For a menu, with the atomic definition: "Atomic charges (TFVC)".
    pub title: String,
    pub unit: Option<String>,
    /// Decimals the text output prints: a default for display (`None` for
    /// a table, whose columns give their own).
    pub decimals: Option<usize>,
    /// "comparison" for values printed only to compare with the run's own
    /// atomic definition (0.4).
    #[serde(default)]
    pub role: Option<String>,
    /// True when the values can be negative by nature (charges, spin,
    /// oxidation states, ...): a diverging scale centered on zero (0.4).
    #[serde(default)]
    pub signed: Option<bool>,
}

impl IndexEntry {
    /// Values APOST-3D prints for comparison with another atomic
    /// definition than the run's own (the Mulliken populations of a TFVC
    /// run), marked `role: "comparison"`.
    pub fn is_comparison(&self) -> bool {
        self.role.as_deref() == Some("comparison")
    }
}

/// A table: several quantities per fragment or per orbital, one column
/// each (0.5).
#[derive(Debug, Clone, Deserialize)]
pub struct TableObject {
    /// "fragment" (row i is fragment i) or "orbital".
    pub rows: String,
    pub n_rows: usize,
    pub columns: Vec<TableColumn>,
    /// One list per column, under its key; a cell is a number, a list of
    /// numbers (OSLO's fragments and shares), or null.
    pub data: serde_json::Map<String, Value>,
    /// For a table of orbitals written to a `.fchk` file (since 0.8):
    /// "alpha" or "beta", the block its `fchk_column` values refer to.
    #[serde(default)]
    pub fchk_orbitals: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableColumn {
    pub key: String,
    pub title: String,
    pub unit: Option<String>,
    pub decimals: Option<usize>,
    #[serde(default)]
    pub signed: bool,
}

/// A table cell, see [`TableObject::cell_value`].
#[derive(Debug, Clone, PartialEq)]
pub enum TableCell {
    Number(f64),
    List(Vec<f64>),
    Empty,
}

impl TableObject {
    fn cell(&self, key: &str, row: usize) -> Option<&Value> {
        self.data.get(key)?.as_array()?.get(row)
    }

    /// The number in column `key`, row `row`.
    pub fn number(&self, key: &str, row: usize) -> Option<f64> {
        self.cell(key, row)?.as_f64()
    }

    /// The numbers of a list cell (or a one-number list from a plain cell).
    pub fn numbers(&self, key: &str, row: usize) -> Option<Vec<f64>> {
        match self.cell(key, row)? {
            Value::Array(items) => Some(items.iter().filter_map(Value::as_f64).collect()),
            Value::Number(n) => n.as_f64().map(|v| vec![v]),
            _ => None,
        }
    }

    /// One cell: a number, a list of numbers, or nothing (null or absent).
    pub fn cell_value(&self, key: &str, row: usize) -> TableCell {
        match self.cell(key, row) {
            Some(Value::Number(n)) => n.as_f64().map(TableCell::Number).unwrap_or(TableCell::Empty),
            Some(Value::Array(items)) => TableCell::List(items.iter().filter_map(Value::as_f64).collect()),
            _ => TableCell::Empty,
        }
    }

    pub fn has_column(&self, key: &str) -> bool {
        self.columns.iter().any(|c| c.key == key)
    }
}

/// An atom x atom or fragment x fragment matrix: full square values and
/// what they mean (the same layout means different things in different
/// analyses, so the descriptions are shown with it).
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixObject {
    pub values: Vec<Vec<Option<f64>>>,
    #[serde(default)]
    pub symmetric: bool,
    /// "localization index", "atomic term <S^2>_A", ...
    pub diagonal: Option<String>,
    /// "bond order", "delocalization index", ...
    pub off_diagonal: Option<String>,
    pub total: Option<f64>,
    /// "diagonal and one triangle" or "all elements".
    pub total_rule: Option<String>,
}

impl MatrixObject {
    pub fn get(&self, i: usize, j: usize) -> Option<f64> {
        self.values.get(i)?.get(j).copied().flatten()
    }

    /// The total recomputed from the values following `total_rule`, to
    /// check the reading against the file's own `total`.
    pub fn total_by_rule(&self) -> Option<f64> {
        let n = self.values.len();
        let all = |i: usize, j: usize| self.get(i, j).unwrap_or(0.0);
        match self.total_rule.as_deref()? {
            "all elements" => Some((0..n).flat_map(|i| (0..n).map(move |j| (i, j))).map(|(i, j)| all(i, j)).sum()),
            "diagonal and one triangle" => Some((0..n).flat_map(|i| (i..n).map(move |j| (i, j))).map(|(i, j)| all(i, j)).sum()),
            _ => None,
        }
    }
}

/// The values an index entry points to.
#[derive(Debug, Clone)]
pub enum Quantity {
    /// A single number (`None`: not available).
    Scalar(Option<f64>),
    Table(TableObject),
    /// One value per atom or per fragment (`None`: not available).
    Vector(Vec<Option<f64>>),
    Matrix(MatrixObject),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProgramInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    /// Release date of that version, ISO 8601 ("2026-09-22"). Since 0.2.
    pub version_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WavefunctionSummary {
    /// "restricted" or "unrestricted".
    pub formalism: Option<String>,
    /// As APOST-3D prints it: "Hartree-Fock", "KS-DFT", "CASSCF", ...
    pub method: Option<String>,
    pub n_basis_functions: Option<usize>,
    pub n_electrons: Option<u32>,
    pub n_alpha: Option<u32>,
    pub n_beta: Option<u32>,
    pub energy_au: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputInfo {
    /// The whole input file, as written by the user.
    pub text: Option<String>,
    pub atomic_definition: Option<AtomicDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AtomicDefinition {
    /// "TFVC", "MULLIKEN", ...
    pub keyword: Option<String>,
    /// Full name, e.g. "Topological fuzzy Voronoi cells".
    pub name: Option<String>,
    /// Display name, as in the index titles: "TFVC", "Lowdin", ... (0.4).
    pub short_name: Option<String>,
    /// "real" (real-space partition) or "hilbert" (Hilbert-space).
    pub space: Option<String>,
    /// True when the input gave no atomic definition and TFVC was used.
    pub default: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MoleculeBlock {
    pub n_atoms: usize,
    pub atomic_numbers: Vec<u32>,
    #[serde(default)]
    pub symbols: Vec<String>,
    /// Smaller than the atomic number for an atom with an ECP.
    #[serde(default)]
    pub nuclear_charges: Vec<Option<f64>>,
    pub coordinates_bohr: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Fragment {
    pub id: u32,
    /// 1-based atom numbers, as in the file.
    pub atoms: Vec<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunInfo {
    /// `job_name`, `input_file` and `wavefunction_file` are bare file
    /// names (since 0.2); the input, wavefunction and results files are
    /// always in the same folder.
    pub job_name: Option<String>,
    pub input_file: Option<String>,
    pub wavefunction_file: Option<String>,
    /// When the run started, ISO 8601 with time zone
    /// ("2026-10-13T14:05:31+02:00"). Since 0.2.
    pub date: Option<String>,
    /// The run id, the start time and 4 hex digits (since 0.8); also on
    /// the first line of every orbital `.fchk` file of the run.
    pub id: Option<String>,
    pub threads: Option<u32>,
    /// "normal" or "stopped".
    pub status: Option<String>,
    pub stop_message: Option<String>,
}

impl ApostResults {
    /// True when APOST-3D stopped on an error. The file is still complete,
    /// but holds only the blocks written before the stop.
    pub fn stopped(&self) -> bool {
        self.run.as_ref().and_then(|r| r.status.as_deref()) == Some("stopped")
    }

    pub fn fragments(&self) -> &[Fragment] {
        self.fragments.as_deref().unwrap_or(&[])
    }

    /// Names of the analyses that were performed, in file order.
    pub fn analysis_names(&self) -> Vec<&str> {
        self.analyses.as_ref().map(|a| a.keys().map(String::as_str).collect()).unwrap_or_default()
    }

    /// The values of an index entry, checked to have one entry per atom
    /// (or fragment), or to be a square matrix of that size.
    pub fn quantity(&self, entry: &IndexEntry) -> Result<Quantity, String> {
        let value = self.raw.pointer(&entry.path).ok_or_else(|| format!("{}: nothing at {}", entry.id, entry.path))?;
        match entry.kind {
            QuantityKind::Scalar => {
                return match value {
                    Value::Null => Ok(Quantity::Scalar(None)),
                    v => v.as_f64().map(|x| Quantity::Scalar(Some(x))).ok_or_else(|| format!("{}: not a number", entry.id)),
                };
            }
            QuantityKind::Table => {
                let table: TableObject = serde_json::from_value(value.clone()).map_err(|e| format!("{}: not a table ({e})", entry.id))?;
                if table.rows == "fragment" && table.n_rows != self.fragments().len() {
                    return Err(format!("{}: {} rows for {} fragments", entry.id, table.n_rows, self.fragments().len()));
                }
                for column in &table.columns {
                    let length = table.data.get(&column.key).and_then(Value::as_array).map(Vec::len);
                    if length != Some(table.n_rows) {
                        return Err(format!("{}: column {} does not hold {} rows", entry.id, column.key, table.n_rows));
                    }
                }
                return Ok(Quantity::Table(table));
            }
            _ => {}
        }
        let n = match entry.kind {
            QuantityKind::AtomVector | QuantityKind::AtomMatrix => self.molecule.as_ref().map(|m| m.n_atoms).ok_or("the file has no molecule")?,
            QuantityKind::FragmentVector | QuantityKind::FragmentMatrix => self.fragments().len(),
            _ => return Err(format!("{}: kind not supported by this APOST3Dview", entry.id)),
        };
        if entry.kind.is_matrix() {
            let matrix: MatrixObject = serde_json::from_value(value.clone()).map_err(|e| format!("{}: not a matrix object ({e})", entry.id))?;
            if matrix.values.len() != n || matrix.values.iter().any(|row| row.len() != n) {
                return Err(format!("{}: expected a {n} x {n} matrix", entry.id));
            }
            Ok(Quantity::Matrix(matrix))
        } else {
            let values: Vec<Option<f64>> = serde_json::from_value(value.clone()).map_err(|e| format!("{}: not a list of numbers ({e})", entry.id))?;
            if values.len() != n {
                return Err(format!("{}: expected {n} values, found {}", entry.id, values.len()));
            }
            Ok(Quantity::Vector(values))
        }
    }

    /// Index entries of one kind, in file order.
    pub fn entries(&self, kind: QuantityKind) -> impl Iterator<Item = &IndexEntry> {
        self.index.iter().filter(move |e| e.kind == kind)
    }

    /// The molecule as the viewer's own `Molecule` (angstrom, perceived
    /// bonds), for when the results file is opened without its `.fchk`.
    pub fn molecule(&self) -> Option<Molecule> {
        let block = self.molecule.as_ref()?;
        let positions = block
            .coordinates_bohr
            .iter()
            .map(|c| Vec3::new((c[0] * ANGSTROM_PER_BOHR) as f32, (c[1] * ANGSTROM_PER_BOHR) as f32, (c[2] * ANGSTROM_PER_BOHR) as f32))
            .collect();
        Some(Molecule::from_atoms(block.atomic_numbers.clone(), positions))
    }

    /// Checks the internal consistency the rest of the viewer relies on:
    /// per-atom lists of `n_atoms` entries, fragment atoms within range.
    fn validate(&self) -> Result<(), String> {
        let n_atoms = match &self.molecule {
            Some(m) => {
                let lengths_ok = m.atomic_numbers.len() == m.n_atoms
                    && m.coordinates_bohr.len() == m.n_atoms
                    && (m.symbols.is_empty() || m.symbols.len() == m.n_atoms)
                    && (m.nuclear_charges.is_empty() || m.nuclear_charges.len() == m.n_atoms);
                if !lengths_ok {
                    return Err(format!("the molecule block does not hold {} entries per atom", m.n_atoms));
                }
                Some(m.n_atoms)
            }
            None => None,
        };
        for fragment in self.fragments() {
            if fragment.atoms.is_empty() {
                return Err(format!("fragment {} has no atoms", fragment.id));
            }
            if let Some(n_atoms) = n_atoms {
                if let Some(&bad) = fragment.atoms.iter().find(|&&a| a == 0 || a > n_atoms) {
                    return Err(format!("fragment {} lists atom {bad}, but the molecule has {n_atoms} atoms", fragment.id));
                }
            }
        }
        Ok(())
    }
}

/// The orbitals one analysis wrote to its own `.fchk` file (EOS, GEOS,
/// OSLO): the file's name, and what each of its orbitals is.
#[derive(Debug, Clone)]
pub struct OrbitalSet {
    /// "eos", "geos" or "oslo".
    pub analysis: String,
    /// The `.fchk` file, a bare name in the results file's folder.
    pub file: String,
    /// The run id the file must carry on its first line (since 0.8).
    pub file_id: Option<String>,
    /// Per alpha orbital of the file (index = `.fchk` column - 1): its
    /// label, or `None` for an empty orbital (no row in the tables).
    pub alpha: Vec<Option<OrbitalLabel>>,
    pub beta: Vec<Option<OrbitalLabel>>,
    /// Why there are no beta orbitals, when the file says so: "same as
    /// alpha" (closed shell) or "no beta electrons".
    pub beta_note: Option<String>,
}

/// What an analysis orbital is.
#[derive(Debug, Clone, PartialEq)]
pub struct OrbitalLabel {
    /// The fragment ids it belongs to: one, or several for a shared OSLO.
    pub fragments: Vec<u32>,
    /// Each fragment's share (1 for a single fragment).
    pub shares: Vec<f64>,
    pub detail: OrbitalDetail,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrbitalDetail {
    /// An effective fragment orbital (EOS, GEOS).
    Efo {
        net: Option<f64>,
        gross: Option<f64>,
        fit_percent: Option<f64>,
        /// Electrons the assignment gave it: 1 or 0, 2 or 0 (paired GEOS),
        /// or a fraction when shared at the frontier.
        electrons: Option<f64>,
        /// A GEOS paired EFO with a clearly negative net occupation, left
        /// out of the assignment.
        negative: bool,
        recovered_percent: Option<f64>,
    },
    /// An OSLO.
    Oslo { iteration: Option<f64>, foli: Option<f64> },
}

impl OrbitalSet {
    /// A readable name: "EOS EFOs", "GEOS EFOs", "OSLOs".
    pub fn title(&self) -> &'static str {
        match self.analysis.as_str() {
            "eos" => "EOS EFOs",
            "geos" => "GEOS EFOs",
            "oslo" => "OSLOs",
            _ => "Analysis orbitals",
        }
    }
}

impl ApostResults {
    /// The analysis orbital files this run wrote, with the label of each
    /// of their orbitals (brief 04's mapping): EOS alpha/beta EFOs, GEOS
    /// paired EFOs (then the negative ones) as alpha and unpaired as beta,
    /// OSLO total or alpha OSLOs as alpha and beta OSLOs as beta.
    pub fn orbital_sets(&self) -> Vec<OrbitalSet> {
        let mut sets = Vec::new();
        for analysis in ["eos", "geos", "oslo"] {
            let base = format!("/analyses/{analysis}");
            let Some(block) = self.raw.pointer(&base) else { continue };
            let Some(file) = block.get("orbital_file").and_then(Value::as_str) else { continue };
            let file_id = block.get("orbital_file_id").and_then(Value::as_str).map(str::to_string);
            let mut set = OrbitalSet { analysis: analysis.to_string(), file: file.to_string(), file_id, alpha: Vec::new(), beta: Vec::new(), beta_note: None };
            let beta = block.get("beta");
            if beta.and_then(|b| b.get("same_as_alpha")).and_then(Value::as_bool) == Some(true) {
                set.beta_note = Some("The \u{3b2} orbitals are the same as the \u{3b1} ones.".to_string());
            } else if beta.and_then(|b| b.get("no_electrons")).and_then(Value::as_bool) == Some(true) {
                set.beta_note = Some("No \u{3b2} electrons.".to_string());
            }
            // Every table of orbitals in the block that says which block
            // of the orbital file it describes (`fchk_orbitals`).
            for (path, t) in orbital_tables(block, "") {
                let labels = match t.fchk_orbitals.as_deref() {
                    Some("alpha") => &mut set.alpha,
                    Some("beta") => &mut set.beta,
                    _ => continue,
                };
                let negative = path.ends_with("negative_efos");
                for row in 0..t.n_rows {
                    let Some(column) = t.number("fchk_column", row) else { continue };
                    let index = column as usize;
                    if index == 0 {
                        continue;
                    }
                    let fragments: Vec<u32> = t.numbers("fragments", row).unwrap_or_default().iter().map(|f| *f as u32).collect();
                    let shares = t.numbers("shares", row).unwrap_or_else(|| vec![1.0; fragments.len()]);
                    let detail = if analysis == "oslo" {
                        OrbitalDetail::Oslo { iteration: t.number("iteration", row), foli: t.number("foli", row) }
                    } else {
                        OrbitalDetail::Efo {
                            net: t.number("net", row),
                            gross: t.number("gross", row),
                            fit_percent: t.number("fit_percent", row),
                            electrons: t.number("electrons", row),
                            negative,
                            recovered_percent: t.number("recovered_percent", row),
                        }
                    };
                    if labels.len() < index {
                        labels.resize(index, None);
                    }
                    labels[index - 1] = Some(OrbitalLabel { fragments, shares, detail });
                }
            }
            sets.push(set);
        }
        sets
    }
}

/// Parses the text of a results file. Checks `schema` and the major
/// version first, so a file from an incompatible APOST-3D gets a clear
/// message instead of an error about some key it doesn't recognize.
pub fn parse_apost_results(text: &str) -> Result<ApostResults, String> {
    let value: Value = serde_json::from_str(text).map_err(|err| format!("not valid JSON ({err})"))?;
    if value.get("schema").and_then(Value::as_str) != Some(RESULTS_SCHEMA) {
        return Err("not an APOST-3D results file".to_string());
    }
    let version = value.get("schema_version").and_then(Value::as_str).unwrap_or("");
    let major = version.split('.').next().and_then(|m| m.parse::<u32>().ok());
    match major {
        Some(SUPPORTED_SCHEMA_MAJOR) => {}
        Some(_) => {
            return Err(format!(
                "written in results-file version {version}, which this APOST3Dview does not know \
                 (it reads version {SUPPORTED_SCHEMA_MAJOR}.x). Please update APOST3Dview."
            ));
        }
        None => return Err(format!("unreadable schema_version \"{version}\"")),
    }
    let mut results: ApostResults = serde_json::from_value(value.clone()).map_err(|err| format!("unexpected content ({err})"))?;
    results.raw = value;
    results.validate()?;
    // Keep only the quantities that can actually be shown; a broken entry
    // is reported, not fatal for the rest of the file.
    let (mut kept, mut problems) = (Vec::new(), Vec::new());
    for entry in std::mem::take(&mut results.index) {
        match results.quantity(&entry) {
            Ok(_) => kept.push(entry),
            Err(err) => problems.push(err),
        }
    }
    results.index = kept;
    results.index_problems = problems;
    Ok(results)
}

pub fn load_apost_results(path: &Path) -> Result<ApostResults, String> {
    let text = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    parse_apost_results(&text)
}

/// Whether a file name ends in `.apost.json` (any letter case).
pub fn is_results_file(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_ascii_lowercase().ends_with(RESULTS_SUFFIX))
}

/// `/dir/X.fchk` -> `/dir/X.apost.json`. An APOST-3D job `X` always reads
/// `X.fchk`, and writes its results under the same name.
pub fn results_path_for(wavefunction_path: &Path) -> PathBuf {
    let stem = wavefunction_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    wavefunction_path.with_file_name(format!("{stem}{RESULTS_SUFFIX}"))
}

/// The results file for `/dir/X.fchk`, if one exists. A leftover
/// `X.apost.json.tmp` (a run that crashed) is never matched.
pub fn find_results_for(wavefunction_path: &Path) -> Option<PathBuf> {
    let path = results_path_for(wavefunction_path);
    path.is_file().then_some(path)
}

/// The job name a results file was written under: its file name without
/// `.apost.json`.
fn job_stem(results_path: &Path) -> Option<String> {
    let name = results_path.file_name()?.to_str()?;
    let cut = name.len().checked_sub(RESULTS_SUFFIX.len())?;
    name.is_char_boundary(cut).then(|| name[..cut].to_string())
}

/// The existing file `dir/name`, trying `name` as written, then its bare
/// file name. Since schema 0.2 names are always bare; the second try is a
/// fallback for files written by 0.1.
fn existing_beside(dir: &Path, name: &str) -> Option<PathBuf> {
    let as_written = dir.join(name);
    if as_written.is_file() {
        return Some(as_written);
    }
    let bare = dir.join(Path::new(name).file_name()?);
    bare.is_file().then_some(bare)
}

/// The `.fchk` a results file was computed from, if it exists next to it
/// (APOST-3D keeps them in the same folder): `run.wavefunction_file`, else
/// `<job>.fchk` (for a renamed results file).
pub fn wavefunction_path_for(results_path: &Path, results: &ApostResults) -> Option<PathBuf> {
    let dir = results_path.parent()?;
    let named = results.run.as_ref().and_then(|r| r.wavefunction_file.as_deref());
    named
        .and_then(|name| existing_beside(dir, name))
        .or_else(|| job_stem(results_path).and_then(|stem| existing_beside(dir, &format!("{stem}.fchk"))))
}

/// The text report `<job>.apost` next to a results file, if it exists.
/// APOST-3D writes the report to standard output, so its name is a
/// convention, not something the results file records.
pub fn report_path_for(results_path: &Path, results: &ApostResults) -> Option<PathBuf> {
    let dir = results_path.parent()?;
    let job = results.run.as_ref().and_then(|r| r.job_name.clone());
    [job, job_stem(results_path)].into_iter().flatten().find_map(|name| existing_beside(dir, &format!("{name}.apost")))
}

/// Whether a results file belongs to a `.fchk`: same atoms in the same
/// order and positions, same number of basis functions, same SCF energy.
/// Geometry alone is not enough, since several calculations often share
/// one geometry. `Err` gives the first difference found, as a sentence.
/// A results file without a molecule block (a run that stopped while
/// reading its input) has nothing atom-related to contradict and passes.
pub fn check_against_fchk(results: &ApostResults, fchk: &FchkSummary) -> Result<(), String> {
    if let Some(molecule) = &results.molecule {
        if molecule.n_atoms != fchk.atomic_numbers.len() {
            return Err(format!("the results have {} atoms, the .fchk has {}", molecule.n_atoms, fchk.atomic_numbers.len()));
        }
        for (index, (&a, &b)) in molecule.atomic_numbers.iter().zip(&fchk.atomic_numbers).enumerate() {
            if a != b {
                return Err(format!("atom {} has atomic number {a} in the results but {b} in the .fchk", index + 1));
            }
        }
        for (index, (a, b)) in molecule.coordinates_bohr.iter().zip(&fchk.coordinates_bohr).enumerate() {
            let distance = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            if distance > COORDINATE_TOLERANCE_BOHR {
                return Err(format!("atom {} is {distance:.4} bohr away from its position in the .fchk", index + 1));
            }
        }
    }
    let summary = results.wavefunction.as_ref();
    if let (Some(a), Some(b)) = (summary.and_then(|w| w.n_basis_functions), fchk.n_basis_functions) {
        if a != b {
            return Err(format!("the results used {a} basis functions, the .fchk has {b}"));
        }
    }
    if let (Some(a), Some(b)) = (summary.and_then(|w| w.energy_au), fchk.scf_energy) {
        if (a - b).abs() > ENERGY_TOLERANCE_AU {
            return Err(format!("the results were computed from an energy of {a:.6} hartree, the .fchk has {b:.6}"));
        }
    }
    Ok(())
}

/// Whether an analysis orbital file belongs to the main `.fchk`: the
/// same atoms at the same positions and the same number of basis
/// functions (it is a copy of it with other orbitals). `Err` gives the
/// first difference.
pub fn check_orbital_file(main: &FchkSummary, orbitals: &FchkSummary) -> Result<(), String> {
    if main.atomic_numbers != orbitals.atomic_numbers {
        return Err("its atoms differ from the open .fchk".to_string());
    }
    for (index, (a, b)) in main.coordinates_bohr.iter().zip(&orbitals.coordinates_bohr).enumerate() {
        let distance = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        if distance > COORDINATE_TOLERANCE_BOHR {
            return Err(format!("atom {} is not where it is in the open .fchk", index + 1));
        }
    }
    if main.n_basis_functions != orbitals.n_basis_functions {
        return Err(format!("it has {:?} basis functions, the open .fchk {:?}", orbitals.n_basis_functions, main.n_basis_functions));
    }
    Ok(())
}

/// The tables of orbitals under `value` with their path, in file order.
fn orbital_tables(value: &Value, path: &str) -> Vec<(String, TableObject)> {
    let Some(object) = value.as_object() else { return Vec::new() };
    if object.get("rows").and_then(Value::as_str) == Some("orbital") {
        return serde_json::from_value(value.clone()).map(|t| vec![(path.to_string(), t)]).unwrap_or_default();
    }
    object.iter().flat_map(|(key, child)| orbital_tables(child, &format!("{path}/{key}"))).collect()
}

/// The run id on the first line of an orbital `.fchk` file
/// ("APOST-3D OSLOs, run <id>").
pub fn orbital_file_run_id(path: &Path) -> std::io::Result<Option<String>> {
    use std::io::BufRead;
    let mut line = String::new();
    std::io::BufReader::new(std::fs::File::open(path)?).read_line(&mut line)?;
    Ok(line.trim_end().rsplit_once(", run ").map(|(_, id)| id.trim().to_string()))
}

/// Refuses an orbital file left from another run of the same molecule:
/// its first line must carry the run id the results expect.
pub fn check_orbital_file_id(expected: Option<&str>, found: Option<&str>) -> Result<(), String> {
    match (expected, found) {
        (None, _) => Ok(()),
        (Some(e), Some(f)) if e == f => Ok(()),
        (Some(e), Some(f)) => Err(format!("it was written by another APOST-3D run (run {f}, the results are from run {e})")),
        (Some(_), None) => Err("it carries no APOST-3D run id on its first line, so it can't be matched with these results".to_string()),
    }
}

/// Atoms (0-based) that belong to no fragment, or to more than one.
#[derive(Debug, Default, PartialEq)]
pub struct FragmentCoverage {
    pub unassigned: Vec<usize>,
    pub shared: Vec<usize>,
}

pub fn fragment_coverage(n_atoms: usize, fragments: &[Fragment]) -> FragmentCoverage {
    let mut count = vec![0usize; n_atoms];
    for fragment in fragments {
        for &atom in &fragment.atoms {
            if let Some(c) = atom.checked_sub(1).and_then(|i| count.get_mut(i)) {
                *c += 1;
            }
        }
    }
    let unassigned = (0..n_atoms).filter(|&i| count[i] == 0).collect();
    let shared = (0..n_atoms).filter(|&i| count[i] > 1).collect();
    FragmentCoverage { unassigned, shared }
}

/// For each atom (0-based), the position in `fragments` of the first
/// fragment that contains it.
pub fn fragment_of_atom(n_atoms: usize, fragments: &[Fragment]) -> Vec<Option<usize>> {
    let mut owner = vec![None; n_atoms];
    for (position, fragment) in fragments.iter().enumerate() {
        for &atom in &fragment.atoms {
            if let Some(slot) = atom.checked_sub(1).and_then(|i| owner.get_mut(i)) {
                slot.get_or_insert(position);
            }
        }
    }
    owner
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real `H2O-Dimer-RHF.apost.json` written by APOST-3D (schema 0.1),
    /// trimmed of its input text.
    const DIMER: &str = r##"{
      "schema": "apost3d-results",
      "schema_version": "0.1",
      "program": {"name": "APOST-3D", "version": "5.0"},
      "wavefunction": {"formalism": "restricted", "method": "Hartree-Fock", "n_basis_functions": 48,
        "n_electrons": 20, "n_alpha": 10, "n_beta": 10, "energy_au": -1.5206236833536570E+002},
      "input": {"text": "# METHOD\nTFVC\n", "atomic_definition": {"keyword": "TFVC",
        "name": "Topological fuzzy Voronoi cells", "space": "real", "default": false}},
      "molecule": {"n_atoms": 6, "atomic_numbers": [8, 1, 1, 8, 1, 1], "symbols": ["O", "H", "H", "O", "H", "H"],
        "nuclear_charges": [8.0, 1.0, 1.0, 8.0, 1.0, 1.0],
        "coordinates_bohr": [
          [-2.6201562900000002E+000, -9.3919388400000000E-004, 2.1352771400000001E-001],
          [-3.2523263700000000E+000, 1.4433388000000000E+000, -6.8377094199999999E-001],
          [-3.2552856900000000E+000, -1.4363260300000000E+000, -6.9585763000000000E-001],
          [2.8519142000000000E+000, 1.0110034799999999E-003, -2.2826379799999999E-001],
          [3.6013795800000001E+000, -6.3532592299999999E-003, 1.4196473000000001E+000],
          [1.0521787199999999E+000, -1.2321014300000000E-003, 7.7868054699999995E-002]]},
      "fragments": [{"id": 1, "atoms": [1, 2, 3]}, {"id": 2, "atoms": [4, 5, 6]}],
      "analyses": {},
      "run": {"job_name": "H2O-Dimer-RHF", "input_file": "H2O-Dimer-RHF.inp",
        "wavefunction_file": "H2O-Dimer-RHF.fchk", "threads": 8, "status": "normal", "stop_message": null}
    }"##;

    fn dimer() -> ApostResults {
        parse_apost_results(DIMER).unwrap()
    }

    fn edited(f: impl FnOnce(&mut Value)) -> String {
        let mut value: Value = serde_json::from_str(DIMER).unwrap();
        f(&mut value);
        value.to_string()
    }

    fn matching_fchk() -> FchkSummary {
        let r = dimer();
        let m = r.molecule.unwrap();
        FchkSummary {
            atomic_numbers: m.atomic_numbers,
            coordinates_bohr: m.coordinates_bohr,
            n_basis_functions: Some(48),
            scf_energy: Some(-152.0623683353657),
        }
    }

    #[test]
    fn reads_every_block() {
        let r = dimer();
        assert!(!r.stopped());
        assert_eq!(r.program.as_ref().unwrap().version.as_deref(), Some("5.0"));
        assert_eq!(r.wavefunction.as_ref().unwrap().n_basis_functions, Some(48));
        assert_eq!(r.input.as_ref().unwrap().atomic_definition.as_ref().unwrap().space.as_deref(), Some("real"));
        assert_eq!(r.fragments().len(), 2);
        assert_eq!(r.fragments()[1].atoms, vec![4, 5, 6]);
        assert!(r.analysis_names().is_empty());
        let molecule = r.molecule().unwrap();
        assert_eq!(molecule.atomic_numbers.len(), 6);
        assert!((molecule.positions[0].x as f64 - -2.62015629 * ANGSTROM_PER_BOHR).abs() < 1e-5);
    }

    #[test]
    fn ignores_unknown_keys_and_accepts_newer_minor_versions() {
        let text = edited(|v| {
            v["schema_version"] = "0.7".into();
            v["something_new"] = serde_json::json!({"a": 1});
            v["run"]["host"] = "cluster".into();
            v["analyses"]["populations"] = serde_json::json!({"atoms": {}});
        });
        let r = parse_apost_results(&text).unwrap();
        assert_eq!(r.analysis_names(), vec!["populations"]);
    }

    #[test]
    fn refuses_an_unknown_major_version_with_a_clear_message() {
        let err = parse_apost_results(&edited(|v| v["schema_version"] = "1.0".into())).unwrap_err();
        assert!(err.contains("version 1.0") && err.contains("update APOST3Dview"), "{err}");
    }

    #[test]
    fn refuses_other_json() {
        assert_eq!(parse_apost_results(r#"{"a": 1}"#).unwrap_err(), "not an APOST-3D results file");
        assert!(parse_apost_results("{").unwrap_err().starts_with("not valid JSON"));
    }

    #[test]
    fn reads_a_run_that_stopped_before_the_molecule() {
        let text = r#"{"schema": "apost3d-results", "schema_version": "0.1",
            "program": {"name": "APOST-3D", "version": "5.0"},
            "run": {"job_name": "x", "status": "stopped", "stop_message": "UNKNOWN KEYWORD"}}"#;
        let r = parse_apost_results(text).unwrap();
        assert!(r.stopped());
        assert!(r.molecule().is_none() && r.fragments().is_empty());
        assert!(check_against_fchk(&r, &matching_fchk()).is_ok());
    }

    #[test]
    fn rejects_inconsistent_content() {
        let err = parse_apost_results(&edited(|v| v["molecule"]["atomic_numbers"] = serde_json::json!([8, 1]))).unwrap_err();
        assert!(err.contains("per atom"), "{err}");
        let err = parse_apost_results(&edited(|v| v["fragments"][1]["atoms"] = serde_json::json!([4, 7]))).unwrap_err();
        assert!(err.contains("atom 7"), "{err}");
    }

    #[test]
    fn consistency_check_against_the_fchk() {
        let r = dimer();
        assert!(check_against_fchk(&r, &matching_fchk()).is_ok());

        let mut fchk = matching_fchk();
        fchk.coordinates_bohr[4][2] += 5e-5;
        assert!(check_against_fchk(&r, &fchk).is_ok(), "below tolerance");
        fchk.coordinates_bohr[4][2] += 2e-4;
        assert!(check_against_fchk(&r, &fchk).unwrap_err().starts_with("atom 5 "));

        let mut fchk = matching_fchk();
        fchk.atomic_numbers.swap(0, 1);
        assert!(check_against_fchk(&r, &fchk).unwrap_err().contains("atom 1 has atomic number 8"));

        let mut fchk = matching_fchk();
        fchk.atomic_numbers.pop();
        fchk.coordinates_bohr.pop();
        assert!(check_against_fchk(&r, &fchk).unwrap_err().contains("6 atoms"));

        let mut fchk = matching_fchk();
        fchk.n_basis_functions = Some(58);
        assert!(check_against_fchk(&r, &fchk).unwrap_err().contains("basis functions"));

        let mut fchk = matching_fchk();
        fchk.scf_energy = Some(-152.8537536222573);
        assert!(check_against_fchk(&r, &fchk).unwrap_err().contains("energy"));
        fchk.scf_energy = None;
        assert!(check_against_fchk(&r, &fchk).is_ok(), "no energy to compare (Q-Chem)");
    }

    #[test]
    fn fragment_coverage_and_ownership() {
        let fragments = vec![Fragment { id: 1, atoms: vec![1, 2] }, Fragment { id: 2, atoms: vec![2, 4] }];
        assert_eq!(fragment_coverage(5, &fragments), FragmentCoverage { unassigned: vec![2, 4], shared: vec![1] });
        assert_eq!(fragment_of_atom(5, &fragments), vec![Some(0), Some(0), None, Some(1), None]);
        assert_eq!(fragment_coverage(6, dimer().fragments()), FragmentCoverage::default());
    }

    #[test]
    fn index_entries_with_the_wrong_shape_are_set_aside() {
        let text = edited(|v| {
            v["schema_version"] = "0.3".into();
            v["analyses"]["populations"] = serde_json::json!({"atoms": {"charges": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6], "short": [1.0]}});
            v["index"] = serde_json::json!([
                {"id": "populations.atoms.charges", "analysis": "populations", "spin": null, "path": "/analyses/populations/atoms/charges",
                 "kind": "atom_vector", "title": "Atomic charges (TFVC)", "unit": null, "decimals": 6},
                {"id": "populations.atoms.short", "analysis": "populations", "spin": null, "path": "/analyses/populations/atoms/short",
                 "kind": "atom_vector", "title": "Short", "unit": null, "decimals": 6},
                {"id": "x.y", "analysis": "x", "spin": null, "path": "/analyses/x/y", "kind": "table", "title": "A table", "unit": null, "decimals": 6}
            ]);
        });
        let r = parse_apost_results(&text).unwrap();
        assert_eq!(r.index.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["populations.atoms.charges"]);
        assert_eq!(r.index_problems.len(), 2);
        assert!(parse_apost_results(DIMER).unwrap().index.is_empty(), "0.1/0.2 files have no index");
    }

    #[test]
    fn comparison_values_are_marked_by_their_role() {
        let entry = |role: Option<&str>| IndexEntry {
            id: "populations.mulliken.atoms.charges".into(),
            analysis: None,
            spin: None,
            path: String::new(),
            kind: QuantityKind::AtomVector,
            title: String::new(),
            unit: None,
            decimals: None,
            role: role.map(Into::into),
            signed: Some(true),
        };
        assert!(entry(Some("comparison")).is_comparison());
        assert!(!entry(None).is_comparison(), "only the role decides (0.4)");
    }

    #[test]
    fn discovery_next_to_the_fchk() {
        let dir = std::env::temp_dir().join(format!("apost3dview-discovery-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fchk = dir.join("job.fchk");
        assert_eq!(results_path_for(&fchk), dir.join("job.apost.json"));
        assert!(is_results_file(Path::new("/a/Job.APOST.JSON")) && !is_results_file(Path::new("/a/job.json")));

        std::fs::write(dir.join("job.apost.json.tmp"), "{}").unwrap();
        assert_eq!(find_results_for(&fchk), None, "a crashed run's .tmp is never used");
        std::fs::write(dir.join("job.apost.json"), DIMER).unwrap();
        assert_eq!(find_results_for(&fchk), Some(dir.join("job.apost.json")));

        // The results name H2O-Dimer-RHF.fchk; only job.fchk exists, which is
        // found through the results file's own name.
        let json = dir.join("job.apost.json");
        let r = dimer();
        assert_eq!(wavefunction_path_for(&json, &r), None);
        std::fs::write(&fchk, "").unwrap();
        assert_eq!(wavefunction_path_for(&json, &r), Some(fchk.clone()));
        std::fs::write(dir.join("H2O-Dimer-RHF.fchk"), "").unwrap();
        assert_eq!(wavefunction_path_for(&json, &r), Some(dir.join("H2O-Dimer-RHF.fchk")));

        assert_eq!(report_path_for(&json, &r), None);
        std::fs::write(dir.join("H2O-Dimer-RHF.apost"), "").unwrap();
        assert_eq!(report_path_for(&json, &r), Some(dir.join("H2O-Dimer-RHF.apost")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
