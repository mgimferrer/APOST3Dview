# Limitations

- Basis functions above g (h, i, ...) aren't supported. Such a file opens, but its orbitals can't be generated, and APOST3Dview says so when it opens rather than drawing a wrong orbital.
- Cartesian g functions aren't supported (pure g functions are). Every basis set that reaches g is spherical by convention, so this has not been a limitation in practice.
- `.molden` files are read as PySCF and ORCA write them (ORCA checked with version 6.1.1). Other programs' `.molden` files are read as standard Molden files, but haven't been tested.
- Of a cube file with several grids, only the first grid is read.
- APOST3Dview shows what APOST-3D's results file holds: populations, bond orders, local spin, the EOS, GEOS and OSLO analyses with their orbitals, and the energy decomposition (IQA). The EFFAO orbitals have no orbital file, so their table is listed but can't be drawn.
- The input form covers the common analyses with APOST-3D's defaults. Anything else is written by hand, following [APOST-3D's input reference](https://github.com/mgimferrer/APOST3D).
- APOST-3D can't be run from APOST3Dview: run it as usual, and open its results.
- Videos are animated GIFs of one full turn. GIF files get large for long or big animations (about 3 MB for 6 s at 640 × 480).
- The downloads for Windows and Linux are new and haven't been tried on many computers yet. If one doesn't work for you, please [report it](help.md).
