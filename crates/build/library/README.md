# The Build window's library

`crates/build/assets/library.toml` holds the groups, rings, ligands and
molecules the Build window offers (97 entries). It is written by
`crates/build/examples/make_library.rs` from:

- **Avogadro's fragment collection**,
  [OpenChemistry/fragments](https://github.com/OpenChemistry/fragments) at
  commit `7d842f4`, BSD-3-Clause (licence in
  `crates/build/assets/library-avogadro-LICENSE.txt`, shipped with the
  program). Its `.cjson` files mark the bonding atom with a dummy atom.
- **PubChem 3D conformers** (public domain), listed in `pubchem.txt` with
  their CIDs and downloaded by `fetch_pubchem.sh`.
- **The cAAC and Dipp** of the DFT structure of Be(cAAC)₂ (M. Gimferrer et
  al., Chem. Sci. 2022, 13, 6583), `Be-cAAC-Dip-EOS.fchk` of the case
  studies.
- **Composed here** with the builder's own `place` and `clean`: the NHCs
  (IMe, ItBu, IMes, IPr, SIMes, SIPr: an imidazole or imidazolidine from
  PubChem, the C2 hydrogens removed, both N-H replaced by the wingtip group,
  tidied with a placeholder metal at the carbene's lone pair), NMe₂, SiMe₃,
  PMe₂, PPh₂ and Bn.

Each entry names its source. To write it again:

    git clone https://github.com/OpenChemistry/fragments && git -C fragments checkout 7d842f4
    crates/build/library/fetch_pubchem.sh /tmp/pubchem3d
    cargo run -p apost3dview-build --example make_library -- fragments /tmp/pubchem3d \
        fixtures/case-studies/Be-cAAC-Dip/Be-cAAC-Dip-EOS.fchk

The test `library::tests::every_entry_is_sound` checks every entry: one
piece, no atoms closer than 0.6 of their van der Waals sum, Clean changes its
bond lengths by less than 0.2 Å and its angles by less than 26°, and placed
(groups on methane's H, ligands on a bare Fe) it makes only the bond it
should.
