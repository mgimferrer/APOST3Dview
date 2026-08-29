"""Generate .molden test files (+ matching reference cubes for a couple of
MOs each) from PySCF, for developing APOST3Dview's .molden parser against —
mirrors the role Chemcraft's .fchk/.cube pairs played for the GTO evaluator
in Phase 3. Four cases, chosen to hit the same shell-complexity/spin tiers
that mattered for the .fchk work:

  1. h2o_631g            - restricted, S/P only, sanity baseline
  2. h2o_631gstar_cart    - restricted, CARTESIAN d (mol.cart=True), like
                            Gaussian's own 6-31G* convention
  3. h2o_ccpvqz_pure      - restricted, pure/spherical through g (cc-pVQZ
                            reaches g on row-2 atoms, no need for a heavy
                            element/ECP the way BiCl3/def2QZVPP was needed
                            for the .fchk g-shell test)
  4. o2_triplet_ccpvtz    - UNRESTRICTED (UHF triplet O2), pure through f

Run inside the venv: <venv>/bin/python gen_molden.py <output_dir>
"""
import sys
from pathlib import Path

from pyscf import gto, scf
from pyscf.tools import molden, cubegen

out_dir = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
out_dir.mkdir(parents=True, exist_ok=True)

H2O_GEOM = """
O  0.000000  0.000000  0.117300
H  0.000000  0.757200 -0.469200
H  0.000000 -0.757200 -0.469200
"""

O2_GEOM = """
O 0.000000 0.000000  0.604000
O 0.000000 0.000000 -0.604000
"""


def save(mf, mol, name, homo_idx, lumo_idx, beta_homo_idx=None, beta_lumo_idx=None):
    molden_path = out_dir / f"{name}.molden"
    molden.from_scf(mf, str(molden_path))
    print(f"wrote {molden_path}")

    unrestricted = isinstance(mf.mo_coeff, (list, tuple)) or getattr(mf.mo_coeff, "ndim", 2) == 3
    if unrestricted:
        alpha, beta = mf.mo_coeff[0], mf.mo_coeff[1]
        cubegen.orbital(mol, str(out_dir / f"{name}_alpha_homo.cube"), alpha[:, homo_idx])
        cubegen.orbital(mol, str(out_dir / f"{name}_alpha_lumo.cube"), alpha[:, lumo_idx])
        cubegen.orbital(mol, str(out_dir / f"{name}_beta_homo.cube"), beta[:, beta_homo_idx])
        cubegen.orbital(mol, str(out_dir / f"{name}_beta_lumo.cube"), beta[:, beta_lumo_idx])
    else:
        cubegen.orbital(mol, str(out_dir / f"{name}_homo.cube"), mf.mo_coeff[:, homo_idx])
        cubegen.orbital(mol, str(out_dir / f"{name}_lumo.cube"), mf.mo_coeff[:, lumo_idx])
    print(f"  reference cubes written for {name}")


# --- 1. H2O/6-31G, restricted, S/P only ---
mol = gto.M(atom=H2O_GEOM, basis="6-31g", verbose=0)
mf = scf.RHF(mol).run()
n_occ = mol.nelectron // 2
save(mf, mol, "h2o_631g", n_occ - 1, n_occ)

# --- 2. H2O/6-31G*, restricted, CARTESIAN d ---
mol = gto.M(atom=H2O_GEOM, basis="6-31g*", cart=True, verbose=0)
mf = scf.RHF(mol).run()
n_occ = mol.nelectron // 2
save(mf, mol, "h2o_631gstar_cart", n_occ - 1, n_occ)

# --- 3. H2O/cc-pVQZ, restricted, PURE through g ---
mol = gto.M(atom=H2O_GEOM, basis="cc-pvqz", verbose=0)
mf = scf.RHF(mol).run()
n_occ = mol.nelectron // 2
save(mf, mol, "h2o_ccpvqz_pure", n_occ - 1, n_occ)

# --- 4. O2 triplet, UNRESTRICTED (UHF), PURE through f ---
mol = gto.M(atom=O2_GEOM, basis="cc-pvtz", spin=2, verbose=0)  # spin = n_alpha - n_beta = 2 -> triplet
mf = scf.UHF(mol).run()
n_alpha = mol.nelectron // 2 + 1
n_beta = mol.nelectron // 2 - 1
save(mf, mol, "o2_triplet_ccpvtz", n_alpha - 1, n_alpha, n_beta - 1, n_beta)

print("\nAll done.")
