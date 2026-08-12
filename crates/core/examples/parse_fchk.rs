use std::env;
use std::path::PathBuf;
use std::time::Instant;

use apost3dview_core::Molecule;

fn main() {
    let path = env::args().nth(1).map(PathBuf::from).expect("usage: parse_fchk <path.fchk>");
    let start = Instant::now();
    let molecule = Molecule::from_fchk(&path).expect("failed to parse fchk");
    let elapsed = start.elapsed();

    println!("Parsed {} atoms, {} bonds in {:?}", molecule.atomic_numbers.len(), molecule.bonds.len(), elapsed);
    let (center, radius) = molecule.bounding_sphere();
    println!("Bounding sphere: center={center:?} radius={radius:.2}");

    let mut counts = std::collections::BTreeMap::new();
    for &z in &molecule.atomic_numbers {
        *counts.entry(z).or_insert(0) += 1;
    }
    for (z, count) in counts {
        let symbol = apost3dview_core::element_data(z).symbol;
        println!("  Z={z:>3} {symbol:<2}  x{count}");
    }

    let filename = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    print_formats(&molecule, &filename);
}

#[allow(dead_code)]
fn print_formats(molecule: &apost3dview_core::Molecule, filename: &str) {
    use apost3dview_core::{format_coordinates, CoordinateFormat, LengthUnit};
    println!("\n--- Z x y z, Angstrom ---");
    println!("{}", format_coordinates(molecule, LengthUnit::Angstrom, CoordinateFormat::AtomicNumberTable, filename));
    println!("--- xyz, Bohr ---");
    println!("{}", format_coordinates(molecule, LengthUnit::Bohr, CoordinateFormat::XyzFile, filename));
}
