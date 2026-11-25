# Point groups: APOST-3Dview against pymatgen

The point groups APOST-3Dview finds (Build window, Molecule window), checked
against pymatgen's `PointGroupAnalyzer` (pymatgen 2024.8.9) on 29 structures,
at distance tolerances of 0.01 and 0.1 angstrom.

- `molecules/`: 18 molecules built with their ideal symmetry (written by the
  `write_known_molecules` test of `crates/build/src/symmetry/tests.rs`):
  water, ammonia, methane, staggered and eclipsed ethane, benzene, eclipsed
  and staggered ferrocene, SF6, C60, allene, hydrogen peroxide, CHFClBr,
  boric acid, S8, cyclohexane (chair), N2, HCN. And 11 real geometries, the
  structures of the `.fchk` files in `examples/` and `validation/gaussian/`,
  written by `crates/build/examples/symmetry_report.rs --write-xyz`.
- `pymatgen_groups.py` (with `requirements.txt`): pymatgen's groups, written
  to `pymatgen.tsv`.
- `crates/build/tests/symmetry_pymatgen.rs` checks that APOST-3Dview finds the
  same group for every structure at both tolerances.

## Results

All 29 agree at both tolerances: C1, C2, C2v, C3v, C3h, Cs, D2d, D3d, D3h,
D4d, D5d, D5h, D6h, Td, Oh, Ih, C∞v and D∞h among them. The real
geometries: ethane D3d, CH3F C3v, [Fe(CN)5NO]3- Cs, CO C∞v, [Fe(CO)6]2+ Oh,
ferrocene D5d, water C2v (four calculations), the water dimer Cs, BiCl3 C3v.

The tolerances don't mean exactly the same in the two programs (pymatgen: how
far an operation may send an atom from a like atom; APOST-3Dview: how far
symmetrizing moves an atom), so structures close to a tolerance could differ.
The tests of the build crate also shake every ideal molecule by up to 0.02
angstrom and check that symmetrizing gives the group back exactly.
