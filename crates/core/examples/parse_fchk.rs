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
}
