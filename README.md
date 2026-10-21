<p align="center"><img width=22% src="crates/app/assets/logo.png"></p>

<h3 align="center">A molecular, orbital and density viewer for APOST-3D</h3>

<p align="center">APOST3Dview shows the chemical bonding analyses of <a href="https://github.com/mgimferrer/APOST3D">APOST-3D</a> on the molecule (fragments, charges, bond orders, local spin, effective oxidation states, effective fragment orbitals and OSLOs, the IQA energy decomposition), draws orbitals and densities straight from <code>.fchk</code> and <code>.molden</code> files, and makes publication figures and videos of them. Free, for Windows, macOS and Linux.</p>

<p align="center"><b><a href="https://mgimferrer.github.io/APOST3Dview/">User guide</a></b> · <a href="https://mgimferrer.github.io/APOST3Dview/tour.html">Five-minute tour</a> · <a href="https://github.com/mgimferrer/APOST3Dview/issues">Report a problem</a></p>

## Download

APOST3Dview is free, and nothing else needs to be installed:

| Your computer | Download |
|---|---|
| Windows 10 or 11 | [APOST3Dview-windows.zip](https://github.com/mgimferrer/APOST3Dview/releases/latest/download/APOST3Dview-windows.zip) |
| macOS 11 or later (Apple Silicon and Intel) | [APOST3Dview-macos.dmg](https://github.com/mgimferrer/APOST3Dview/releases/latest/download/APOST3Dview-macos.dmg) |
| Linux (x86_64) | [APOST3Dview-linux.tar.gz](https://github.com/mgimferrer/APOST3Dview/releases/latest/download/APOST3Dview-linux.tar.gz) |

The first time you open it, Windows and macOS ask once whether to trust it, because APOST3Dview isn't signed with a paid certificate: [what to click](https://mgimferrer.github.io/APOST3Dview/install.html). Each download also holds an `examples` folder with five APOST-3D calculations to try, and the user guide to read offline. Earlier versions are on the [releases page](https://github.com/mgimferrer/APOST3Dview/releases).

## Documentation

The [user guide](https://mgimferrer.github.io/APOST3Dview/) explains everything: installing, a five-minute tour with the examples, one chapter per task (looking at a molecule, orbitals and densities, APOST-3D results on the molecule, making an APOST-3D input, figures and videos, sessions), the file formats and what is validated, and troubleshooting. Its source is in [`docs/`](docs/).

To build APOST3Dview from source, see [BUILDING.md](BUILDING.md).

## Citation

APOST3Dview doesn't have its own publication yet. Until it does, if you use it in your work, please cite the APOST-3D paper:

* P. Salvador, E. Ramos-Cordoba, M. Montilla, L. Pujal and M. Gimferrer, *APOST-3D: Chemical concepts from wave function analysis*, **J. Chem. Phys.**, **2024**, 160, 172502
  DOI: [10.1063/5.0206187](https://doi.org/10.1063/5.0206187)

The `CITATION.cff` file in this repository keeps GitHub's "Cite this repository" button up to date.

## License

APOST3Dview is licensed under the [GNU General Public License v3.0](LICENSE).

Copyright © 2026 Martí Gimferrer.

## Bug reports and feature requests

Please open an [issue](https://github.com/mgimferrer/APOST3Dview/issues), or email [mgimferrer18@gmail.com](mailto:mgimferrer18@gmail.com).

## Author

**Martí Gimferrer**, University of Göttingen
[mgimferrer18@gmail.com](mailto:mgimferrer18@gmail.com)
