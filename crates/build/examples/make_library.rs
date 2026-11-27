//! Writes `crates/build/assets/library.toml`, the Build window's groups,
//! rings, ligands and molecules, from their sources:
//!
//! - Avogadro's fragment collection (github.com/OpenChemistry/fragments,
//!   BSD-3-Clause, licence in `assets/library-avogadro-LICENSE.txt`): its
//!   `.cjson` files mark the attaching atom with a dummy atom;
//! - PubChem 3D conformers (public domain), listed in `library/pubchem.txt`
//!   and downloaded by `library/fetch_pubchem.sh`;
//! - the cAAC and its Dipp groups from the DFT structure of Be(cAAC)2
//!   (M. Gimferrer et al., Chem. Sci. 2022, 13, 6583);
//! - groups composed here from those with `place`, then tidied with
//!   `clean` (NHCs, NMe2, SiMe3, PPh2, ...).
//!
//! ```text
//! cargo run -p apost3dview-build --example make_library -- \
//!     <avogadro fragments folder> <pubchem folder> <Be-cAAC .fchk>
//! ```

use std::path::{Path, PathBuf};

use apost3dview_build::{
    clean, element_fragment, free_direction, place, write_library, Attachment, CleanOptions, Fragment, FragmentKind, Graph, LibraryEntry, Site, Tab, View,
};
use apost3dview_core::{atomic_number_from_symbol, perceive_bonds, Molecule};
use glam::Vec3;

struct Sources {
    avogadro: PathBuf,
    pubchem: PathBuf,
    caac: PathBuf,
}

fn pairs(z: &[u32], p: &[Vec3]) -> Vec<(usize, usize)> {
    perceive_bonds(z, p).iter().map(|b| (b.atom_a, b.atom_b)).collect()
}

/// An Avogadro fragment: its dummy atom (element 0) marks where it bonds.
fn avogadro(dir: &Path, name: &str, kind: FragmentKind) -> Fragment {
    let text = std::fs::read_to_string(dir.join(format!("{name}.cjson"))).unwrap_or_else(|e| panic!("{name}: {e}"));
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let numbers: Vec<u32> = json["atoms"]["elements"]["number"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    let coords: Vec<f32> = json["atoms"]["coords"]["3d"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
    let index: Vec<usize> = json["bonds"]["connections"]["index"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
    let orders: Vec<f64> = json["bonds"]["order"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap()).collect()).unwrap_or_else(|| vec![1.0; index.len() / 2]);
    let at = |k: usize| Vec3::new(coords[3 * k], coords[3 * k + 1], coords[3 * k + 2]);
    let dummy = numbers.iter().position(|&z| z == 0).unwrap_or_else(|| panic!("{name}: no dummy atom"));
    let (attach, order) = (0..index.len() / 2)
        .find_map(|k| match (index[2 * k], index[2 * k + 1]) {
            (a, b) if a == dummy => Some((b, orders[k])),
            (a, b) if b == dummy => Some((a, orders[k])),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name}: dummy atom not bonded"));
    let kept: Vec<usize> = (0..numbers.len()).filter(|&k| numbers[k] != 0).collect();
    let toward = (at(dummy) - at(attach)).normalize();
    Fragment {
        kind,
        atomic_numbers: kept.iter().map(|&k| numbers[k]).collect(),
        positions: kept.iter().map(|&k| at(k) - at(attach)).collect(),
        attach: Some(Attachment { atom: kept.iter().position(|&k| k == attach).unwrap(), toward, order }),
    }
}

/// A PubChem 3D conformer (SDF, V2000).
fn sdf(dir: &Path, key: &str) -> (Vec<u32>, Vec<Vec3>) {
    let text = std::fs::read_to_string(dir.join(format!("{key}.sdf"))).unwrap_or_else(|e| panic!("{key}: {e} (run library/fetch_pubchem.sh)"));
    let lines: Vec<&str> = text.lines().collect();
    let count: usize = lines[3][..3].trim().parse().unwrap();
    let mut z = Vec::new();
    let mut p = Vec::new();
    for line in &lines[4..4 + count] {
        let parts: Vec<&str> = line.split_whitespace().collect();
        p.push(Vec3::new(parts[0].parse().unwrap(), parts[1].parse().unwrap(), parts[2].parse().unwrap()));
        z.push(atomic_number_from_symbol(parts[3]).unwrap());
    }
    (z, p)
}

fn centred(p: &[Vec3]) -> Vec<Vec3> {
    let c = p.iter().copied().sum::<Vec3>() / p.len() as f32;
    p.iter().map(|v| *v - c).collect()
}

fn whole(z: Vec<u32>, p: Vec<Vec3>) -> Fragment {
    Fragment { kind: FragmentKind::Molecule, positions: centred(&p), atomic_numbers: z, attach: None }
}

/// A ligand bonding through `donor`, toward its lone pair.
fn ligand(z: Vec<u32>, p: Vec<Vec3>, donor: usize) -> Fragment {
    let toward = free_direction(&z, &p, &pairs(&z, &p), donor, 1.0, View::default());
    let p: Vec<Vec3> = p.iter().map(|v| *v - p[donor]).collect();
    Fragment { kind: FragmentKind::Ligand, atomic_numbers: z, positions: p, attach: Some(Attachment { atom: donor, toward, order: 1.0 }) }
}

/// The only atom of element `element` with `neighbours` neighbours.
fn find(z: &[u32], p: &[Vec3], element: u32, neighbours: usize) -> usize {
    let bonds = pairs(z, p);
    let count = |i: usize| bonds.iter().filter(|&&(a, b)| a == i || b == i).count();
    let found: Vec<usize> = (0..z.len()).filter(|&i| z[i] == element && count(i) == neighbours).collect();
    assert_eq!(found.len(), 1, "element {element} with {neighbours} neighbours: {found:?}");
    found[0]
}

/// `base` with the hydrogens bonded to `on` (atoms of `base`) replaced by
/// `group`, one per hydrogen, then tidied. The attaching atom and its
/// direction are kept (groups get a hydrogen on their open bond while
/// cleaning).
fn substituted(base: &Fragment, on: &[usize], group: &Fragment) -> Fragment {
    let mut z = base.atomic_numbers.clone();
    let mut p = base.positions.clone();
    // While composing, the open bond holds an atom: a hydrogen for a group,
    // a metal for a ligand (so nothing turns into the metal's place).
    let cap = base.attach.map(|a| {
        let element = if base.kind == FragmentKind::Ligand { 26 } else { 1 };
        let length = if element == 26 { 2.0 } else { apost3dview_build::bond_length(z[a.atom], 1, a.order) as f32 };
        z.push(element);
        p.push(p[a.atom] + a.toward * length);
        z.len() - 1
    });
    let mut molecule = Molecule::from_atoms(z, p);
    let mut track: Vec<usize> = (0..molecule.atomic_numbers.len()).collect();
    for &atom in on {
        let bonds: Vec<(usize, usize)> = molecule.bonds.iter().map(|b| (b.atom_a, b.atom_b)).collect();
        let host = track[atom];
        let hydrogen = bonds
            .iter()
            .filter_map(|&(a, b)| if a == host { Some(b) } else if b == host { Some(a) } else { None })
            .find(|&h| molecule.atomic_numbers[h] == 1 && Some(h) != cap.map(|c| track[c]))
            .unwrap_or_else(|| panic!("no hydrogen left on atom {atom}"));
        let placed = place(&molecule, &bonds, group, Site::Replace(hydrogen), View::default()).unwrap();
        track = track.iter().map(|&t| placed.atom_map.get(t).copied().flatten().unwrap_or(usize::MAX)).collect();
        molecule = placed.molecule;
    }
    let bonds: Vec<(usize, usize)> = molecule.bonds.iter().map(|b| (b.atom_a, b.atom_b)).collect();
    let held: Vec<usize> = cap.map(|c| track[c]).into_iter().collect();
    let tidy = clean(&molecule.atomic_numbers, &molecule.positions, &bonds, &CleanOptions { twist: true, held, ..Default::default() }).positions;
    let attach = base.attach.map(|a| track[a.atom]);
    let mut keep: Vec<usize> = (0..tidy.len()).collect();
    let toward = match (cap.map(|c| track[c]), attach) {
        (Some(c), Some(a)) => {
            keep.retain(|&k| k != c);
            Some((tidy[c] - tidy[a]).normalize())
        }
        _ => None,
    };
    let origin = attach.map_or(Vec3::ZERO, |a| tidy[a]);
    Fragment {
        kind: base.kind,
        atomic_numbers: keep.iter().map(|&k| molecule.atomic_numbers[k]).collect(),
        positions: keep.iter().map(|&k| tidy[k] - origin).collect(),
        attach: attach.zip(toward).map(|(a, t)| Attachment { atom: keep.iter().position(|&k| k == a).unwrap(), toward: t, order: base.attach.unwrap().order }),
    }
}

/// The cAAC of the Be(cAAC)2 structure (bonding through its carbene C,
/// toward Be) and one of its Dipp groups (bonding through the ipso C,
/// toward N).
fn caac_and_dipp(path: &Path) -> (Fragment, Fragment) {
    let m = Molecule::from_fchk(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let (z, p) = (&m.atomic_numbers, &m.positions);
    let bonds = pairs(z, p);
    let graph = Graph::new(z.len(), bonds.iter().copied());
    let be = z.iter().position(|&x| x == 4).expect("a Be atom");
    let neighbours = |i: usize| -> Vec<usize> { bonds.iter().filter_map(|&(a, b)| if a == i { Some(b) } else if b == i { Some(a) } else { None }).collect() };
    let carbene = *neighbours(be).iter().filter(|&&c| z[c] == 6).min().expect("a carbene C on Be");
    let caac = graph.side(be, carbene).expect("the cAAC is a side");
    let fragment_of = |atoms: &[usize], attach: usize, toward_atom: usize, kind: FragmentKind| Fragment {
        kind,
        atomic_numbers: atoms.iter().map(|&a| z[a]).collect(),
        positions: atoms.iter().map(|&a| p[a] - p[attach]).collect(),
        attach: Some(Attachment { atom: atoms.iter().position(|&a| a == attach).unwrap(), toward: (p[toward_atom] - p[attach]).normalize(), order: 1.0 }),
    };
    let n = *neighbours(carbene).iter().find(|&&a| z[a] == 7).expect("the cAAC N");
    let ipso = *neighbours(n).iter().find(|&&a| a != carbene && z[a] == 6 && neighbours(a).len() == 3).expect("the Dipp ipso C");
    let dipp = graph.side(n, ipso).expect("the Dipp is a side");
    (fragment_of(&caac, carbene, be, FragmentKind::Ligand), fragment_of(&dipp, ipso, n, FragmentKind::Group))
}

/// An N-heterocyclic carbene from a ring whose C between the two N lost
/// its hydrogens: `ring` (imidazole or imidazolidine) with R on both N.
fn nhc(ring: (Vec<u32>, Vec<Vec3>), r: Option<&Fragment>) -> Fragment {
    let (mut z, mut p) = ring;
    let bonds = pairs(&z, &p);
    let neighbours = |i: usize| -> Vec<usize> { bonds.iter().filter_map(|&(a, b)| if a == i { Some(b) } else if b == i { Some(a) } else { None }).collect() };
    let carbene = (0..z.len()).find(|&i| z[i] == 6 && neighbours(i).iter().filter(|&&j| z[j] == 7).count() == 2).expect("the C between the N");
    let nitrogens: Vec<usize> = neighbours(carbene).into_iter().filter(|&j| z[j] == 7).collect();
    // Imidazole has one N without H: give it one.
    for &nn in &nitrogens {
        if !neighbours(nn).iter().any(|&j| z[j] == 1) {
            let m = Molecule::from_atoms(z.clone(), p.clone());
            let b: Vec<(usize, usize)> = m.bonds.iter().map(|b| (b.atom_a, b.atom_b)).collect();
            let placed = place(&m, &b, &element_fragment(1), Site::On(nn), View::default()).unwrap();
            z = placed.molecule.atomic_numbers;
            p = placed.molecule.positions;
        }
    }
    let gone: Vec<usize> = neighbours(carbene).into_iter().filter(|&j| z[j] == 1).collect();
    let keep: Vec<usize> = (0..z.len()).filter(|k| !gone.contains(k)).collect();
    let index = |a: usize| keep.iter().position(|&k| k == a).unwrap();
    let (z, p): (Vec<u32>, Vec<Vec3>) = keep.iter().map(|&k| (z[k], p[k])).unzip();
    let base = ligand(z, p, index(carbene));
    match r {
        Some(group) => substituted(&base, &[index(nitrogens[0]), index(nitrogens[1])], group),
        None => base,
    }
}

fn entry(key: &str, label: &str, name: &str, tab: Tab, find: &str, source: &str, fragment: Fragment) -> LibraryEntry {
    LibraryEntry { key: key.into(), label: label.into(), name: name.into(), tab, find: find.into(), source: source.into(), fragment }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 3, "usage: make_library <avogadro fragments folder> <pubchem folder> <Be-cAAC .fchk>");
    let s = Sources { avogadro: PathBuf::from(&args[0]), pubchem: PathBuf::from(&args[1]), caac: PathBuf::from(&args[2]) };
    let av = |name: &str| avogadro(&s.avogadro, name, FragmentKind::Group);
    let avl = |name: &str| avogadro(&s.avogadro, name, FragmentKind::Ligand);
    let pc = |key: &str| sdf(&s.pubchem, key);
    let avo = |name: &str| format!("Avogadro fragments: {name}");
    let pub_ = |key: &str| format!("PubChem 3D conformer: {key}");
    let (caac, dipp) = caac_and_dipp(&s.caac);
    let dft = "DFT structure of Be(cAAC)2, Chem. Sci. 2022, 13, 6583";
    let composed = "composed with place and clean";
    let methyl = av("ligands/monodentate/methyl");
    let phenyl = av("groups/aromatics/phenyl");
    let mut e = Vec::new();
    use Tab::*;
    // Groups.
    for (key, label, name, find, path) in [
        ("methyl", "CH3", "Methyl", "Me", "ligands/monodentate/methyl"),
        ("ethyl", "Et", "Ethyl", "C2H5", "groups/alkanes/ethyl"),
        ("propyl", "nPr", "n-Propyl", "propyl C3H7", "groups/alkanes/propyl"),
        ("isopropyl", "iPr", "Isopropyl", "C3H7", "groups/alkanes/iso-propyl"),
        ("tert-butyl", "tBu", "tert-Butyl", "C4H9", "groups/alkanes/t-butyl"),
        ("vinyl", "vinyl", "Vinyl", "ethenyl C2H3", "groups/alkenes/ethylene"),
        ("ethynyl", "ethynyl", "Ethynyl", "alkyne C2H", "groups/alkynes/ethyne"),
        ("phenyl", "Ph", "Phenyl", "C6H5 aryl", "groups/aromatics/phenyl"),
        ("mesityl", "Mes", "Mesityl", "2,4,6-trimethylphenyl aryl", "groups/aromatics/mesityl"),
    ] {
        e.push(entry(key, label, name, Groups, find, &avo(path), av(path)));
    }
    e.push(entry("benzyl", "Bn", "Benzyl", Groups, "CH2Ph", composed, substituted(&element_fragment(6), &[0], &phenyl)));
    e.push(entry("dipp", "Dipp", "2,6-Diisopropylphenyl", Groups, "Dip aryl", dft, dipp.clone()));
    e.push(entry("hydroxy", "OH", "Hydroxy", Groups, "alcohol", "element with its hydrogens", element_fragment(8)));
    e.push(entry("methoxy", "OMe", "Methoxy", Groups, "OCH3 ether", &avo("groups/alkoxy/methoxy"), av("groups/alkoxy/methoxy")));
    e.push(entry("amino", "NH2", "Amino", Groups, "amine", "element with its hydrogens", element_fragment(7)));
    e.push(entry("dimethylamino", "NMe2", "Dimethylamino", Groups, "amine N(CH3)2", composed, substituted(&element_fragment(7), &[0, 0], &methyl)));
    for (key, label, name, find, path) in [
        ("nitro", "NO2", "Nitro", "", "groups/nitrogen/nitro"),
        ("cyano", "CN", "Cyano", "nitrile", "groups/nitrogen/nitrile"),
        ("isocyano", "NC", "Isocyano", "isonitrile", "groups/nitrogen/isocyano"),
        ("trifluoromethyl", "CF3", "Trifluoromethyl", "", "groups/halogens/trifluoromethyl"),
        ("formyl", "CHO", "Formyl", "aldehyde", "groups/oxygen/aldehyde"),
        ("acetyl", "COMe", "Acetyl", "Ac ketone", "groups/oxygen/ketone"),
        ("carboxy", "COOH", "Carboxy", "carboxylic acid", "groups/oxygen/carboxylate"),
        ("methoxycarbonyl", "COOMe", "Methoxycarbonyl", "ester CO2Me", "groups/oxygen/ester"),
        ("carbamoyl", "CONH2", "Carbamoyl", "amide", "groups/oxygen/amide"),
        ("methylthio", "SMe", "Methylthio", "thioether sulfide", "groups/sulfur/sulfide"),
        ("sulfo", "SO3H", "Sulfo", "sulfonic acid", "groups/sulfur/sulfonate"),
        ("triflate", "OTf", "Triflate", "trifluoromethanesulfonate", "groups/sulfur/triflate"),
        ("mesylate", "OMs", "Mesylate", "methanesulfonate", "groups/sulfur/mesyl"),
        ("tosyl", "Ts", "Tosyl", "p-toluenesulfonyl", "groups/sulfur/tosyl"),
        ("dimethoxyboryl", "B(OMe)2", "Dimethoxyboryl", "boronic ester", "groups/boron/boronic_ester"),
    ] {
        e.push(entry(key, label, name, Groups, find, &avo(path), av(path)));
    }
    e.push(entry("thiol", "SH", "Sulfanyl", Groups, "thiol mercapto", "element with its hydrogens", element_fragment(16)));
    e.push(entry("trimethylsilyl", "SiMe3", "Trimethylsilyl", Groups, "TMS", composed, substituted(&element_fragment(14), &[0, 0, 0], &methyl)));
    e.push(entry("dimethylphosphino", "PMe2", "Dimethylphosphino", Groups, "phosphine", composed, substituted(&element_fragment(15), &[0, 0], &methyl)));
    e.push(entry("diphenylphosphino", "PPh2", "Diphenylphosphino", Groups, "phosphine", composed, substituted(&element_fragment(15), &[0, 0], &phenyl)));
    // Rings.
    for (key, label, name, find, path) in [
        ("cyclopropyl", "cPr", "Cyclopropyl", "C3H5", "groups/cycloalkanes/cyclopropane"),
        ("cyclobutyl", "cBu", "Cyclobutyl", "C4H7", "groups/cycloalkanes/cyclobutane"),
        ("cyclopentyl", "cPent", "Cyclopentyl", "C5H9", "groups/cycloalkanes/cyclopentane"),
        ("cyclohexyl", "Cy", "Cyclohexyl", "C6H11 chair", "groups/cycloalkanes/cyclohexane"),
        ("ring-phenyl", "Ph", "Phenyl", "benzene C6H5", "groups/aromatics/phenyl"),
        ("naphthyl", "Naph", "Naphthyl", "naphthalene", "groups/aromatics/naphthalene"),
        ("pyridyl", "2-Py", "2-Pyridyl", "pyridine", "groups/heteroaromatics/pyridine"),
        ("furyl", "2-Fur", "2-Furyl", "furan", "groups/heteroaromatics/furan"),
        ("thienyl", "2-Th", "2-Thienyl", "thiophene", "groups/heteroaromatics/thiophene"),
        ("pyrrolyl", "2-Pyr", "2-Pyrrolyl", "pyrrole", "groups/heteroaromatics/pyrrole"),
        ("imidazolyl", "2-Im", "2-Imidazolyl", "imidazole", "groups/heteroaromatics/imidazole"),
        ("adamantyl", "Ad", "1-Adamantyl", "adamantane", "groups/bicyclics/adamantyl"),
        ("norbornyl", "Nb", "Norbornyl", "norbornane bicyclo", "groups/bicyclics/norbornyl"),
    ] {
        e.push(entry(key, label, name, Rings, find, &avo(path), av(path)));
    }
    // Ligands.
    for (key, label, name, find, path) in [
        ("carbonyl", "CO", "Carbonyl", "carbon monoxide", "ligands/monodentate/carbonyl"),
        ("cyanido", "CN-", "Cyanido", "cyanide", "ligands/monodentate/cyano"),
        ("nitrosyl", "NO", "Nitrosyl", "nitric oxide", "ligands/monodentate/nitrosyl"),
        ("ammine", "NH3", "Ammine", "ammonia", "ligands/monodentate/ammine"),
        ("aqua", "H2O", "Aqua", "water", "ligands/monodentate/aqua"),
        ("thf", "THF", "Tetrahydrofuran", "solvent ether", "ligands/monodentate/tetrahydrofurane"),
        ("diethyl-ether", "Et2O", "Diethyl ether", "solvent ether", "ligands/monodentate/diethylether"),
        ("dimethyl-sulfide", "SMe2", "Dimethyl sulfide", "thioether", "ligands/monodentate/dimethylsulfide"),
        ("pyridine", "py", "Pyridine", "", "ligands/monodentate/pyridyl"),
        ("phosphine", "PH3", "Phosphine", "", "ligands/monodentate/phosphine"),
        ("trimethylphosphine", "PMe3", "Trimethylphosphine", "phosphine", "ligands/monodentate/trimethylphosphine"),
        ("triphenylphosphine", "PPh3", "Triphenylphosphine", "phosphine", "ligands/monodentate/triphenylphosphine"),
        ("azido", "N3-", "Azido", "azide", "ligands/monodentate/azido"),
        ("thiocyanato-n", "NCS-", "Thiocyanato-N", "isothiocyanate", "ligands/monodentate/thiocyanato-N"),
    ] {
        e.push(entry(key, label, name, Ligands, find, &avo(path), avl(path)));
    }
    let n2 = Fragment {
        kind: FragmentKind::Ligand,
        atomic_numbers: vec![7, 7],
        positions: vec![Vec3::ZERO, Vec3::new(0.0, 0.0, 1.10)],
        attach: Some(Attachment { atom: 0, toward: Vec3::NEG_Z, order: 1.0 }),
    };
    e.push(entry("dinitrogen", "N2", "Dinitrogen", Ligands, "end-on", "N-N 1.10 A", n2));
    for (key, label, name, find, element, neighbours) in [
        ("acetonitrile", "MeCN", "Acetonitrile", "solvent nitrile", 7, 1),
        ("dmap", "DMAP", "4-Dimethylaminopyridine", "pyridine", 7, 2),
        ("triethylphosphine", "PEt3", "Triethylphosphine", "phosphine", 15, 3),
        ("tricyclohexylphosphine", "PCy3", "Tricyclohexylphosphine", "phosphine", 15, 3),
        ("tri-tert-butylphosphine", "PtBu3", "Tri-tert-butylphosphine", "phosphine", 15, 3),
        ("trimethyl-phosphite", "P(OMe)3", "Trimethyl phosphite", "phosphite", 15, 3),
        ("tert-butyl-isocyanide", "CNtBu", "tert-Butyl isocyanide", "isonitrile", 6, 1),
        ("xylyl-isocyanide", "CNXyl", "2,6-Dimethylphenyl isocyanide", "isonitrile xylyl", 6, 1),
    ] {
        let (z, p) = pc(key);
        let donor = find_donor(&z, &p, element, neighbours);
        e.push(entry(key, label, name, Ligands, find, &pub_(key), ligand(z, p, donor)));
    }
    let imidazole = || pc("imidazole");
    let carbene_find = "NHC carbene";
    e.push(entry("ime", "IMe", "IMe", Ligands, &format!("{carbene_find} 1,3-dimethylimidazol-2-ylidene"), composed, nhc(imidazole(), Some(&methyl))));
    e.push(entry("itbu", "ItBu", "ItBu", Ligands, &format!("{carbene_find} 1,3-di-tert-butylimidazol-2-ylidene"), composed, nhc(imidazole(), Some(&av("groups/alkanes/t-butyl")))));
    e.push(entry("imes", "IMes", "IMes", Ligands, &format!("{carbene_find} 1,3-dimesitylimidazol-2-ylidene"), composed, nhc(imidazole(), Some(&av("groups/aromatics/mesityl")))));
    e.push(entry("ipr", "IPr", "IPr", Ligands, &format!("{carbene_find} 1,3-bis(2,6-diisopropylphenyl)imidazol-2-ylidene"), composed, nhc(imidazole(), Some(&dipp))));
    e.push(entry("simes", "SIMes", "SIMes", Ligands, &format!("{carbene_find} 1,3-dimesitylimidazolidin-2-ylidene"), composed, nhc(pc("imidazolidine"), Some(&av("groups/aromatics/mesityl")))));
    e.push(entry("sipr", "SIPr", "SIPr", Ligands, &format!("{carbene_find} 1,3-bis(2,6-diisopropylphenyl)imidazolidin-2-ylidene"), composed, nhc(pc("imidazolidine"), Some(&dipp))));
    e.push(entry("caac", "cAAC", "cAAC (Me2, Dipp)", Ligands, "carbene cyclic alkyl amino carbene CAAC", dft, caac));
    // Molecules.
    for (key, label, name, find) in [
        ("water", "H2O", "Water", ""),
        ("ammonia", "NH3", "Ammonia", ""),
        ("methane", "CH4", "Methane", ""),
        ("carbon-dioxide", "CO2", "Carbon dioxide", ""),
        ("hydrogen-sulfide", "H2S", "Hydrogen sulfide", ""),
        ("formaldehyde", "H2CO", "Formaldehyde", "methanal"),
        ("methanol", "MeOH", "Methanol", "solvent"),
        ("ethanol", "EtOH", "Ethanol", "solvent"),
        ("ethylene", "C2H4", "Ethylene", "ethene"),
        ("acetylene", "C2H2", "Acetylene", "ethyne"),
        ("benzene", "C6H6", "Benzene", "solvent"),
        ("toluene", "PhMe", "Toluene", "solvent"),
        ("pyridine", "Py", "Pyridine", "solvent"),
        ("acetone", "acetone", "Acetone", "solvent"),
        ("acetonitrile", "MeCN", "Acetonitrile", "solvent"),
        ("thf", "THF", "Tetrahydrofuran", "solvent"),
        ("diethyl-ether", "Et2O", "Diethyl ether", "solvent"),
        ("dmso", "DMSO", "Dimethyl sulfoxide", "solvent"),
        ("dichloromethane", "CH2Cl2", "Dichloromethane", "DCM solvent"),
        ("chloroform", "CHCl3", "Chloroform", "solvent"),
    ] {
        let (z, p) = pc(key);
        e.push(entry(&format!("molecule-{key}"), label, name, Molecules, find, &pub_(key), whole(z, p)));
    }
    let header = "# The Build window's groups, rings, ligands and molecules.\n\
                  # Written by crates/build/examples/make_library.rs from its sources;\n\
                  # each entry names its source. Avogadro fragments: BSD-3-Clause\n\
                  # (library-avogadro-LICENSE.txt). PubChem 3D conformers: public domain.\n\
                  # Coordinates in angstrom; attach is the 1-based bonding atom, toward\n\
                  # the direction from it to the atom it bonds to.\n\n";
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/library.toml");
    std::fs::write(&out, write_library(header, &e)).unwrap();
    println!("{} entries written to {}", e.len(), out.display());
}

/// The donor atom of a PubChem ligand: the only atom of `element` with
/// `neighbours` neighbours.
fn find_donor(z: &[u32], p: &[Vec3], element: u32, neighbours: usize) -> usize {
    find(z, p, element, neighbours)
}
