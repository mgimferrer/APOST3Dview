//! A new APOST-3D input for a `.fchk` (the APOST-3D window's "New input"
//! part): the user's choices, what the `.fchk` decides by itself, and
//! saving (`<job>.inp`, or another job name with the `.fchk` linked under
//! it). The model and the reader/writer are `apost3dview_core::apost_input`.

use std::path::{Path, PathBuf};

use apost3dview_core::{read_apost_input, read_fchk_facts, AimScheme, ApostInput, FchkFacts, OxidationStates};

/// Which part of the APOST-3D window is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApostTab {
    Results,
    Input,
}

/// The input being prepared for one structure's `.fchk`.
pub(crate) struct InputDraft {
    pub(crate) fchk: PathBuf,
    /// The `.fchk`'s name without extension: APOST-3D's job name.
    pub(crate) job: String,
    pub(crate) facts: Result<FchkFacts, String>,
    /// The choices (its `fragments` unused: see `fragments`).
    pub(crate) input: ApostInput,
    /// 0-based atoms, in fragment order.
    pub(crate) fragments: Vec<Vec<usize>>,
    /// When the form was filled from an existing `<job>.inp`: what else that
    /// file sets, which saving from here leaves out.
    pub(crate) loaded: Option<Vec<String>>,
    /// "Save as another job": the name being typed, while open.
    pub(crate) another_job: Option<String>,
    pub(crate) another_job_error: Option<String>,
}

/// Characters a job name can't hold (it becomes two file names).
fn invalid_job_name(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return Some("Type a name.".into());
    }
    if name.contains(char::is_whitespace) {
        return Some("No spaces in a job name: APOST-3D takes it on the command line.".into());
    }
    if let Some(c) = name.chars().find(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()) {
        return Some(format!("A job name can't contain \"{c}\"."));
    }
    if name.ends_with(".inp") || name.ends_with(".fchk") {
        return Some("Give the name without extension.".into());
    }
    None
}

impl InputDraft {
    /// The form for `fchk`, filled from `<job>.inp` when it exists.
    pub(crate) fn new(fchk: &Path, n_atoms: usize) -> Self {
        let job = fchk.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut draft = Self {
            fchk: fchk.to_path_buf(),
            job,
            facts: read_fchk_facts(fchk).map_err(|e| e.to_string()),
            input: ApostInput::default(),
            fragments: Vec::new(),
            loaded: None,
            another_job: None,
            another_job_error: None,
        };
        if let Ok(text) = std::fs::read_to_string(draft.input_path()) {
            let read = read_apost_input(&text, n_atoms);
            draft.fragments = read.input.fragments.iter().flatten().map(|f| f.iter().filter(|&&a| a >= 1 && a <= n_atoms).map(|a| a - 1).collect()).collect();
            draft.input = read.input;
            draft.loaded = Some(read.other);
        }
        draft
    }

    fn folder(&self) -> &Path {
        self.fchk.parent().unwrap_or(Path::new(""))
    }

    pub(crate) fn input_path(&self) -> PathBuf {
        self.folder().join(format!("{}.inp", self.job))
    }

    pub(crate) fn file_name(&self) -> String {
        format!("{}.inp", self.job)
    }

    pub(crate) fn correlated(&self) -> bool {
        self.facts.as_ref().is_ok_and(|f| f.correlated)
    }

    /// The input the choices make, with what the `.fchk` decides.
    pub(crate) fn build(&self) -> ApostInput {
        let mut input = self.input.clone();
        input.fragments = (!self.fragments.is_empty()).then(|| self.fragments.iter().map(|f| f.iter().map(|a| a + 1).collect()).collect());
        if let Ok(facts) = &self.facts {
            input.apply_fchk(facts, &self.job);
        }
        input
    }

    /// The atomic definitions offered: TFVC, Mulliken and Löwdin; NAO-BASIS
    /// and the Hirshfeld schemes only when the file they read is there; any
    /// other only when a loaded input uses it.
    pub(crate) fn offered_schemes(&self) -> Vec<AimScheme> {
        let nao = self.folder().join(format!("{}.nao", self.job)).is_file();
        let densoutput = self.folder().join("densoutput").is_file();
        AimScheme::ALL
            .into_iter()
            .filter(|&s| {
                s == self.input.aim
                    || match s {
                        AimScheme::Tfvc | AimScheme::Mulliken | AimScheme::Lowdin => true,
                        AimScheme::NaoBasis => nao,
                        AimScheme::Hirsh | AimScheme::HirshIt => densoutput,
                        _ => false,
                    }
            })
            .collect()
    }

    /// Files the run reads that are not next to the `.fchk`, and other
    /// things worth knowing before running.
    pub(crate) fn notes(&self, input: &ApostInput) -> Vec<String> {
        let mut notes = Vec::new();
        let folder = self.folder();
        if input.aim == AimScheme::NaoBasis && !folder.join(format!("{}.nao", self.job)).is_file() {
            notes.push(format!("NAO-BASIS reads {}.nao, which is not next to the .fchk.", self.job));
        }
        if matches!(input.aim, AimScheme::Hirsh | AimScheme::HirshIt) && !folder.join("densoutput").is_file() {
            notes.push("The Hirshfeld schemes read the free-atom densities from a file densoutput, which is not next to the .fchk.".into());
        }
        if let Some(dm) = &input.dm {
            for file in [&dm.dm1, &dm.dm2] {
                if !folder.join(file).is_file() {
                    notes.push(format!("{file} (written by APOST-3D's pySCF script with the .fchk) is not next to the .fchk."));
                }
            }
        }
        if let Some(enpart) = &input.enpart {
            if self.facts.as_ref().is_ok_and(|f| !f.reference_energies) {
                notes.push("The .fchk has no reference energies: ENPART still runs, without the zero-error correction of the integration. For Gaussian, append them with utils/get_energy_g16.".into());
            }
            if enpart.fast_grid {
                notes.push("The faster two-electron grid is for tests and large systems: check the integration error in the output.".into());
            }
        }
        notes
    }

    /// A job name for "Save as another job": `<job>-<first analysis>`,
    /// numbered if taken.
    pub(crate) fn suggested_job(&self) -> String {
        let input = &self.input;
        let analysis = match input.oxidation {
            Some(OxidationStates::Eos) => "EOS",
            Some(OxidationStates::Geos) => "GEOS",
            None if input.oslo => "OSLO",
            None if input.enpart.is_some() => "ENPART",
            None if input.spin => "SPIN",
            None => "POP",
        };
        let base = format!("{}-{analysis}", self.job);
        let taken = |name: &str| self.folder().join(format!("{name}.inp")).exists() || self.folder().join(format!("{name}.fchk")).exists();
        if !taken(&base) {
            return base;
        }
        (2..).map(|n| format!("{base}-{n}")).find(|name| !taken(name)).unwrap()
    }

    /// Saves `text` as the input of another job `name`, next to the
    /// `.fchk`: `<name>.inp`, and `<name>.fchk` as a hard link to the
    /// `.fchk` (no extra disk space), or a copy where links aren't possible.
    /// The `.nao` file goes along the same way for NAO-BASIS. Returns what
    /// was done, in words.
    pub(crate) fn save_as_job(&self, name: &str, text: &str, input: &ApostInput) -> Result<String, String> {
        if let Some(problem) = invalid_job_name(name) {
            return Err(problem);
        }
        if name == self.job {
            return Err(format!("{name} is this job: use Replace instead."));
        }
        let folder = self.folder();
        let inp = folder.join(format!("{name}.inp"));
        let fchk = folder.join(format!("{name}.fchk"));
        for path in [&inp, &fchk] {
            if path.exists() {
                return Err(format!("{} already exists: choose another name.", crate::util::file_display_name(path)));
            }
        }
        let mut copied = Vec::new();
        let mut link = |from: &Path, to: &Path| -> Result<(), String> {
            if std::fs::hard_link(from, to).is_err() {
                std::fs::copy(from, to).map_err(|e| format!("Could not copy {}: {e}", crate::util::file_display_name(from)))?;
                copied.push(crate::util::file_display_name(to));
            }
            Ok(())
        };
        link(&self.fchk, &fchk)?;
        if input.aim == AimScheme::NaoBasis {
            let nao = folder.join(format!("{}.nao", self.job));
            if nao.is_file() {
                link(&nao, &folder.join(format!("{name}.nao")))?;
            }
        }
        std::fs::write(&inp, text).map_err(|e| format!("Could not write {name}.inp: {e}"))?;
        Ok(if copied.is_empty() {
            format!("{name}.inp saved, with {name}.fchk (a link to {}.fchk: no extra disk space)", self.job)
        } else {
            format!("{name}.inp saved, with a copy of {}.fchk as {name}.fchk (links aren't possible here)", self.job)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::invalid_job_name;

    #[test]
    fn job_names() {
        assert!(invalid_job_name("water-ENPART").is_none());
        assert!(invalid_job_name("water.v2").is_none());
        assert!(invalid_job_name("").is_some());
        assert!(invalid_job_name(" water").is_some());
        assert!(invalid_job_name("water EOS").is_some());
        assert!(invalid_job_name("a/b").is_some());
        assert!(invalid_job_name("water.inp").is_some());
    }
}
