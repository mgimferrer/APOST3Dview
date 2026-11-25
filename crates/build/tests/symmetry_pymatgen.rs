//! Our point groups against pymatgen's (validation/symmetry/): every
//! structure there, at the two tolerances pymatgen.tsv was made with.

use std::path::PathBuf;

#[test]
fn point_groups_match_pymatgen() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../validation/symmetry");
    let table = std::fs::read_to_string(dir.join("pymatgen.tsv")).expect("validation/symmetry/pymatgen.tsv");
    let mut checked = 0;
    for line in table.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        let (name, at_001, at_01) = (cells[0], cells[1], cells[2]);
        let molecule = apost3dview_core::parse_xyz(&dir.join("molecules").join(format!("{name}.xyz"))).unwrap();
        let analysis = apost3dview_build::analyze(&molecule.atomic_numbers, &molecule.positions);
        assert_eq!(analysis.group_within(0.01).label, at_001, "{name} at 0.01");
        assert_eq!(analysis.group_within(0.1).label, at_01, "{name} at 0.1");
        checked += 1;
    }
    assert_eq!(checked, 29);
}
