//! The groups, rings, ligands and molecules the Build window offers, and
//! the user's own groups.
//!
//! The built-in library is `assets/library.toml`, written by the example
//! `make_library` from its sources (Avogadro's fragment collection, BSD-3;
//! PubChem 3D conformers, public domain; and groups composed here with
//! `place` and `clean`), each entry naming its source. A user's own group
//! is an `.xyz` file whose comment line says how it bonds.

use apost3dview_core::{atomic_number_from_symbol, element_data};
use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::place::{Attachment, Fragment, FragmentKind};

/// Where an entry is listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tab {
    Groups,
    Rings,
    Ligands,
    Molecules,
    Mine,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Groups, Tab::Rings, Tab::Ligands, Tab::Molecules, Tab::Mine];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Groups => "Groups",
            Tab::Rings => "Rings",
            Tab::Ligands => "Ligands",
            Tab::Molecules => "Molecules",
            Tab::Mine => "Mine",
        }
    }

    /// How an entry of this tab bonds (own groups say it themselves).
    fn kind(self) -> FragmentKind {
        match self {
            Tab::Groups | Tab::Rings | Tab::Mine => FragmentKind::Group,
            Tab::Ligands => FragmentKind::Ligand,
            Tab::Molecules => FragmentKind::Molecule,
        }
    }
}

/// One entry: what it is called, where it is listed, and its atoms.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    /// Stable name for sessions and tests ("tert-butyl").
    pub key: String,
    /// The short text on its button ("tBu").
    pub label: String,
    /// The full name ("tert-Butyl").
    pub name: String,
    pub tab: Tab,
    /// More words Find matches (abbreviations, other names).
    pub find: String,
    /// Where the coordinates come from.
    pub source: String,
    pub fragment: Fragment,
}

impl LibraryEntry {
    /// Whether `query` (lower case words) matches this entry.
    pub fn matches(&self, query: &str) -> bool {
        let text = format!("{} {} {} {}", self.key, self.label, self.name, self.find).to_lowercase();
        query.split_whitespace().all(|word| text.contains(word))
    }
}

#[derive(Serialize, Deserialize)]
struct FileEntry {
    key: String,
    label: String,
    name: String,
    tab: Tab,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    find: String,
    source: String,
    /// 1-based attaching atom (groups and ligands).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attach: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    toward: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<f64>,
    atoms: String,
}

#[derive(Serialize, Deserialize)]
struct LibraryFile {
    entry: Vec<FileEntry>,
}

/// Atom lines "C x y z" (angstrom).
fn read_atoms(text: &str) -> Result<(Vec<u32>, Vec<Vec3>), String> {
    let mut z = Vec::new();
    let mut p = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(format!("not an atom line: {line}"));
        }
        let number = atomic_number_from_symbol(parts[0]).ok_or_else(|| format!("unknown element {}", parts[0]))?;
        let mut xyz = [0.0f32; 3];
        for (k, value) in parts[1..4].iter().enumerate() {
            xyz[k] = value.parse().map_err(|_| format!("not a number: {value}"))?;
        }
        z.push(number);
        p.push(Vec3::from_array(xyz));
    }
    Ok((z, p))
}

fn write_atoms(z: &[u32], p: &[Vec3]) -> String {
    z.iter().zip(p).map(|(z, v)| format!("{:<2} {:>10.5} {:>10.5} {:>10.5}\n", element_data(*z).symbol, v.x, v.y, v.z)).collect()
}

fn entry_from_file(e: FileEntry) -> Result<LibraryEntry, String> {
    let (atomic_numbers, positions) = read_atoms(&e.atoms).map_err(|why| format!("{}: {why}", e.key))?;
    let kind = e.tab.kind();
    let attach = match (kind, e.attach, e.toward) {
        (FragmentKind::Molecule, _, _) => None,
        (_, Some(atom), Some(toward)) if (1..=atomic_numbers.len()).contains(&atom) => {
            Some(Attachment { atom: atom - 1, toward: Vec3::from_array(toward).normalize(), order: e.order.unwrap_or(1.0) })
        }
        _ => return Err(format!("{}: a group or ligand needs its attaching atom and direction", e.key)),
    };
    Ok(LibraryEntry { key: e.key, label: e.label, name: e.name, tab: e.tab, find: e.find, source: e.source, fragment: Fragment { kind, atomic_numbers, positions, attach } })
}

/// Reads a library file.
pub fn parse_library(text: &str) -> Result<Vec<LibraryEntry>, String> {
    let file: LibraryFile = toml::from_str(text).map_err(|e| e.to_string())?;
    file.entry.into_iter().map(entry_from_file).collect()
}

/// Writes a library file (the generator's output).
pub fn write_library(header: &str, entries: &[LibraryEntry]) -> String {
    let file = LibraryFile {
        entry: entries
            .iter()
            .map(|e| FileEntry {
                key: e.key.clone(),
                label: e.label.clone(),
                name: e.name.clone(),
                tab: e.tab,
                find: e.find.clone(),
                source: e.source.clone(),
                attach: e.fragment.attach.map(|a| a.atom + 1),
                toward: e.fragment.attach.map(|a| {
                    let r = |x: f32| (x * 1e5).round() / 1e5;
                    [r(a.toward.x), r(a.toward.y), r(a.toward.z)]
                }),
                order: e.fragment.attach.map(|a| a.order),
                atoms: write_atoms(&e.fragment.atomic_numbers, &e.fragment.positions),
            })
            .collect(),
    };
    format!("{header}{}", toml::to_string(&file).expect("a library serializes"))
}

/// The built-in library, read once.
pub fn library() -> &'static [LibraryEntry] {
    static LIBRARY: std::sync::OnceLock<Vec<LibraryEntry>> = std::sync::OnceLock::new();
    LIBRARY.get_or_init(|| parse_library(include_str!("../assets/library.toml")).expect("the built-in library reads"))
}

/// The comment line of an own group's .xyz: what the file is and how the
/// group bonds.
const GROUP_MARK: &str = "APOST-3Dview group";

/// An own group as .xyz text.
pub fn group_to_xyz(entry: &LibraryEntry) -> String {
    let f = &entry.fragment;
    let mut comment = format!("{GROUP_MARK}; tab {}", entry.tab.title().to_lowercase());
    if let Some(a) = f.attach {
        comment += &format!("; attach {}; toward {:.5} {:.5} {:.5}; order {}", a.atom + 1, a.toward.x, a.toward.y, a.toward.z, a.order);
    }
    format!("{}\n{comment}\n{}", f.atomic_numbers.len(), write_atoms(&f.atomic_numbers, &f.positions))
}

/// An own group read back from its .xyz (`name`: its file name).
pub fn group_from_xyz(name: &str, text: &str) -> Result<LibraryEntry, String> {
    let mut lines = text.lines();
    let count: usize = lines.next().and_then(|l| l.trim().parse().ok()).ok_or("the first line isn't an atom count")?;
    let comment = lines.next().unwrap_or("");
    if !comment.starts_with(GROUP_MARK) {
        return Err("not a group saved by APOST-3Dview (its comment line doesn't say how it bonds)".into());
    }
    let mut tab = Tab::Mine;
    let mut attach = None;
    let mut toward = None;
    let mut order = 1.0;
    for part in comment.split(';').skip(1).map(str::trim) {
        let (key, value) = part.split_once(' ').unwrap_or((part, ""));
        match key {
            "tab" => tab = if value == "ligands" { Tab::Ligands } else if value == "molecules" { Tab::Molecules } else { Tab::Mine },
            "attach" => attach = value.trim().parse::<usize>().ok(),
            "toward" => {
                let v: Vec<f32> = value.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                toward = (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]));
            }
            "order" => order = value.trim().parse().unwrap_or(1.0),
            _ => {}
        }
    }
    let atoms: String = lines.take(count).map(|l| format!("{l}\n")).collect();
    let (atomic_numbers, positions) = read_atoms(&atoms)?;
    if atomic_numbers.len() != count {
        return Err(format!("{count} atoms announced, {} found", atomic_numbers.len()));
    }
    let kind = if tab == Tab::Mine { FragmentKind::Group } else { tab.kind() };
    let attach = match (kind, attach, toward) {
        (FragmentKind::Molecule, _, _) => None,
        (_, Some(a), Some(t)) if (1..=count).contains(&a) && t.length() > 0.0 => Some(Attachment { atom: a - 1, toward: t.normalize(), order }),
        _ => return Err("the comment line doesn't say which atom bonds".into()),
    };
    Ok(LibraryEntry {
        key: format!("mine:{name}"),
        label: name.to_string(),
        name: name.to_string(),
        tab: Tab::Mine,
        find: String::new(),
        source: "your own group".into(),
        fragment: Fragment { kind, atomic_numbers, positions, attach },
    })
}

#[cfg(test)]
mod tests;
