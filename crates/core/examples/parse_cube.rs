use std::env;
use std::path::PathBuf;
use std::time::Instant;

use apost3dview_core::{extract_isosurface, refine_grid, ScalarGrid};

fn main() {
    let path = env::args().nth(1).map(PathBuf::from).expect("usage: parse_cube <path.cube> [isovalue] [refinement]");
    let isovalue_arg: Option<f32> = env::args().nth(2).and_then(|s| s.parse().ok());
    let refinement: usize = env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(1);

    let start = Instant::now();
    let cube = apost3dview_core::parse_cube(&path).expect("failed to parse .cube file");
    let parse_elapsed = start.elapsed();

    println!(
        "Parsed {} atoms, grid {}x{}x{} ({} points) in {:?}",
        cube.molecule.atomic_numbers.len(),
        cube.grid.dims[0],
        cube.grid.dims[1],
        cube.grid.dims[2],
        cube.grid.values.len(),
        parse_elapsed
    );
    println!("Origin: {:?}", cube.grid.origin);
    println!("Step vectors: {:?}, {:?}, {:?}", cube.grid.steps[0], cube.grid.steps[1], cube.grid.steps[2]);
    let (center, radius) = cube.molecule.bounding_sphere();
    println!("Molecule bounding sphere: center={center:?} radius={radius:.2}");
    let max_abs = cube.grid.max_abs_value();
    println!("Grid value range: max |value| = {max_abs:.6}");

    let grid: ScalarGrid = if refinement > 1 {
        let start = Instant::now();
        let refined = refine_grid(&cube.grid, refinement);
        println!(
            "Refined to {}x{}x{} ({} points) in {:?}",
            refined.dims[0],
            refined.dims[1],
            refined.dims[2],
            refined.values.len(),
            start.elapsed()
        );
        refined
    } else {
        cube.grid
    };

    let isovalue = isovalue_arg.unwrap_or(max_abs * 0.25);
    println!("\nExtracting isosurface at isovalue = {isovalue:.6} (positive lobe)...");
    let start = Instant::now();
    let positive = extract_isosurface(&grid, isovalue);
    println!("  {} triangles in {:?}", positive.positions.len() / 3, start.elapsed());

    println!("Extracting isosurface at isovalue = {isovalue:.6} (negative lobe)...");
    let negated = grid.negated();
    let start = Instant::now();
    let negative = extract_isosurface(&negated, isovalue);
    println!("  {} triangles in {:?}", negative.positions.len() / 3, start.elapsed());
}
