"""Point groups of every .xyz in molecules/ by pymatgen's PointGroupAnalyzer,
at distance tolerances 0.01 and 0.1 angstrom, written to pymatgen.tsv.

    python -m venv venv && venv/bin/pip install -r requirements.txt
    venv/bin/python pymatgen_groups.py
"""

from pathlib import Path

from pymatgen.core import Molecule
from pymatgen.symmetry.analyzer import PointGroupAnalyzer

here = Path(__file__).parent
rows = ["file\tpymatgen_0.01\tpymatgen_0.1"]
for xyz in sorted((here / "molecules").glob("*.xyz")):
    molecule = Molecule.from_file(xyz)
    groups = []
    for tolerance in (0.01, 0.1):
        label = PointGroupAnalyzer(molecule, tolerance=tolerance).sch_symbol
        groups.append(label.replace("*", "∞"))
    rows.append("\t".join([xyz.stem, *groups]))
(here / "pymatgen.tsv").write_text("\n".join(rows) + "\n")
print("\n".join(rows))
