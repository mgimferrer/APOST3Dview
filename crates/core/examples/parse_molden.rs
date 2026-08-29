use std::env;
use std::path::PathBuf;

use apost3dview_core::{parse_molden_wavefunction, Molecule};

fn main() {
    let path = env::args().nth(1).map(PathBuf::from).expect("usage: parse_molden <path.molden>");

    let molecule = Molecule::from_molden(&path).expect("failed to parse molecule geometry");
    println!("Geometry: {} atoms, {} bonds", molecule.atomic_numbers.len(), molecule.bonds.len());
    for (i, (&z, &pos)) in molecule.atomic_numbers.iter().zip(&molecule.positions).enumerate() {
        println!("  atom {i}: Z={z}, pos={pos:?}");
    }

    let wfn = parse_molden_wavefunction(&path).expect("failed to parse wavefunction");
    println!("\nBasis: {} shells, {} basis functions", wfn.basis.shells.len(), wfn.alpha.num_basis_functions);
    let mut function_count = 0usize;
    for (i, shell) in wfn.basis.shells.iter().enumerate() {
        let kind = match (shell.angular_momentum, shell.is_pure) {
            (0, _) => "S".to_string(),
            (1, _) => "P".to_string(),
            (l, true) => format!("pure L={l} ({} functions)", 2 * l + 1),
            (l, false) => format!("cartesian L={l} ({} functions)", (l + 1) * (l + 2) / 2),
        };
        let n_funcs = if shell.is_pure { 2 * shell.angular_momentum + 1 } else { (shell.angular_momentum + 1) * (shell.angular_momentum + 2) / 2 };
        function_count += n_funcs as usize;
        println!("  shell {i}: {kind}, center={:?}, {} primitives", shell.center, shell.primitive_exponents.len());
    }
    println!("Total basis functions from shells: {function_count} (wavefunction says {})", wfn.alpha.num_basis_functions);
    assert_eq!(function_count, wfn.alpha.num_basis_functions, "shell-derived function count should match the parsed count");

    println!("\nAlpha orbitals: {} total, {} occupied", wfn.alpha.num_orbitals(), wfn.alpha.num_occupied);
    println!("HOMO index (1-based) = {}, LUMO index = {}", wfn.alpha.homo_index(), wfn.alpha.lumo_index());
    let homo_energy = wfn.alpha.orbital_energies[wfn.alpha.homo_index() - 1];
    let lumo_energy = wfn.alpha.orbital_energies[wfn.alpha.lumo_index() - 1];
    println!("HOMO energy = {homo_energy:.6}, LUMO energy = {lumo_energy:.6}");
    assert!(homo_energy < lumo_energy, "HOMO should be lower energy than LUMO");

    match &wfn.beta {
        Some(beta) => {
            println!("\nUnrestricted: beta orbitals present ({} total, {} occupied)", beta.num_orbitals(), beta.num_occupied);
            println!("Beta HOMO index (1-based) = {}, LUMO index = {}", beta.homo_index(), beta.lumo_index());
            let beta_homo_energy = beta.orbital_energies[beta.homo_index() - 1];
            let beta_lumo_energy = beta.orbital_energies[beta.lumo_index() - 1];
            println!("Beta HOMO energy = {beta_homo_energy:.6}, LUMO energy = {beta_lumo_energy:.6}");
            assert!(beta_homo_energy < beta_lumo_energy, "beta HOMO should be lower energy than beta LUMO");
        }
        None => println!("\nRestricted: no separate beta orbitals (alpha == beta)."),
    }

    println!("\nALL CHECKS PASSED");
}
