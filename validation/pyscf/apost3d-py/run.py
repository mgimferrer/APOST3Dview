"""Natural orbitals and densities written by apost3d.py, with PySCF's own
cube files of the same quantities, for checking APOST3Dview's evaluation."""
import numpy as np
from pyscf import gto, scf, mcscf
from pyscf.tools import cubegen
import apost3d

def fchk_geometry(path):
    lines = open(path).read().splitlines()
    def block(label):
        i = next(k for k, l in enumerate(lines) if l.startswith(label))
        n = int(lines[i].split()[-1]); values = []
        for l in lines[i + 1:]:
            if len(values) == n: break
            values += l.split()
        return values
    z = [int(v) for v in block("Atomic numbers")]
    xyz = np.array([float(v) for v in block("Current cartesian coordinates")]).reshape(-1, 3)
    return z, xyz

def cubes(mol, mc, name):
    occ, no = mcscf.addons.make_natural_orbitals(mc)
    order = np.argsort(-occ); occ, no = occ[order], no[:, order]
    k = mc.ncore
    print(name, "natural occupations around the frontier:", occ[k - 1:k + 3])
    cubegen.orbital(mol, f"{name}-HONO.cube", no[:, k], nx=80, ny=80, nz=80)
    cubegen.orbital(mol, f"{name}-LUNO.cube", no[:, k + 1], nx=80, ny=80, nz=80)
    cubegen.density(mol, f"{name}-density.cube", mc.make_rdm1(), nx=80, ny=80, nz=80)
    np.savetxt(f"{name}-occupations.txt", occ)

# p-benzyne, as pB-cas.py (cc-pVDZ, CASSCF(2,2) from UHF natural orbitals)
z, xyz = fchk_geometry("pB.fchk")
mol = gto.M(atom=[(zi, tuple(r)) for zi, r in zip(z, xyz)], unit="Bohr", basis="cc-pvdz", verbose=3)
uhf = scf.UHF(mol); uhf.kernel()
for _ in range(6):
    mo, _, stable, _ = uhf.stability(return_status=True)
    if stable: break
    uhf.kernel(uhf.make_rdm1(mo, uhf.mo_occ))
noons, natorbs = mcscf.addons.make_natural_orbitals(uhf)
active = sorted(np.argsort(np.abs(noons - 1.0))[:2])
rhf = scf.RHF(mol).run()
mc = mcscf.CASSCF(rhf, 2, 2); mc.fix_spin_(ss=0)
mc.kernel(mc.sort_mo([i + 1 for i in active], natorbs))
apost3d.write_fchk(mol, mc, "pB-CAS22")
cubes(mol, mc, "pB-CAS22")

# water CASSCF(4,4), aug-cc-pVTZ (diffuse functions)
mol = gto.M(atom="O 0 0 0.1173; H 0 0.7572 -0.4692; H 0 -0.7572 -0.4692", basis="aug-cc-pvtz", verbose=3)
rhf = scf.RHF(mol).run()
mc = mcscf.CASSCF(rhf, 4, 4).run()
apost3d.write_fchk(mol, mc, "H2O-CAS44-aug")
cubes(mol, mc, "H2O-CAS44-aug")
print("smallest overlap eigenvalue (aug-cc-pVTZ):", np.linalg.eigvalsh(mol.intor("int1e_ovlp")).min())
