//! Our point groups of .xyz or .fchk files, at two tolerances, as a table
//! (for the comparison with pymatgen in validation/symmetry/). With
//! `--write-xyz DIR`, each structure is also written there as .xyz.
//!
//! With `--list`, every group listed with its largest atom move.
//!
//!     cargo run --release -p apost3dview-build --example symmetry_report -- [--list] [--write-xyz DIR] FILES...

use std::path::{Path, PathBuf};

use apost3dview_core::{element_data, Molecule};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut out_dir: Option<PathBuf> = None;
    let list = args.iter().any(|a| a == "--list");
    args.retain(|a| a != "--list");
    if let Some(k) = args.iter().position(|a| a == "--write-xyz") {
        out_dir = Some(PathBuf::from(args.remove(k + 1)));
        args.remove(k);
    }
    println!("file\tours_0.01\tours_0.1");
    for file in args {
        let path = Path::new(&file);
        let molecule = match path.extension().and_then(|e| e.to_str()) {
            Some("fchk") => Molecule::from_fchk(path),
            _ => apost3dview_core::xyz::parse_xyz(path),
        }
        .unwrap_or_else(|e| panic!("{file}: {e}"));
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if let Some(dir) = &out_dir {
            std::fs::create_dir_all(dir).unwrap();
            let mut text = format!("{}\n{name}, from {}\n", molecule.positions.len(), path.file_name().unwrap().to_string_lossy());
            for (z, p) in molecule.atomic_numbers.iter().zip(&molecule.positions) {
                text.push_str(&format!("{:<2} {:16.10} {:16.10} {:16.10}\n", element_data(*z).symbol, p.x, p.y, p.z));
            }
            std::fs::write(dir.join(format!("{name}.xyz")), text).unwrap();
        }
        let analysis = apost3dview_build::analyze(&molecule.atomic_numbers, &molecule.positions);
        println!("{name}\t{}\t{}", analysis.group_within(0.01).label, analysis.group_within(0.1).label);
        if list {
            for c in &analysis.candidates {
                println!("    {:<5} {:.4} A  order {}  {}", c.label, c.displacement, c.order, c.describe());
                if std::env::var_os("SYMMETRY_ELEMENTS").is_some() {
                    for e in &c.elements {
                        println!("        {e:?}");
                    }
                }
            }
        }
    }
}
