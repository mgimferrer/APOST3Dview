# Building APOST3Dview from source

For development, or if a [downloaded build](README.md#installation) doesn't work on your system. Most users don't need this.

## 1. Install Rust and a C toolchain

- **macOS**: install [Rust](https://rustup.rs/) with `rustup`, and the Xcode Command Line Tools: `xcode-select --install`.
- **Windows**: run [`rustup-init.exe`](https://rustup.rs/) and pick the default option (1). If Visual Studio's C++ Build Tools are missing, rustup installs them as part of the same step (about 1.5 GB, 5 to 15 minutes).
- **Linux**: install [Rust](https://rustup.rs/) with `rustup`, plus a C compiler and the window-system headers. On Ubuntu/Debian:
  ```bash
  sudo apt-get install -y build-essential libxkbcommon-dev libwayland-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
  ```
  Running the app also needs a Vulkan driver (`mesa-vulkan-drivers`, or your GPU vendor's driver).

<details>
<summary>Windows without Visual Studio (GNU toolchain, WSL)</summary>

[WinLibs](https://winlibs.com/) is a standalone GCC/MinGW-w64 for Windows. Extract it (for example to `C:\mingw64`) and add its `bin` folder to `PATH`. In `rustup-init.exe`, choose option 3 ("Don't install the Visual C++ prerequisites"), then option 2 ("Customize installation") and set the default host triple to `x86_64-pc-windows-gnu`. This path is less used for Rust GUI apps than the default one, and the release builds use the default (MSVC) one.

WSL (Windows Subsystem for Linux) can run the Linux build through WSLg, but it isn't simpler: a whole Linux environment to set up, and its GPU passthrough is less proven than running natively.
</details>

If `cargo` isn't found right after installing Rust, open a new terminal: an already-open one hasn't picked up `~/.cargo/bin` yet.

## 2. Build and run

```bash
git clone https://github.com/mgimferrer/APOST3Dview.git
cd APOST3Dview
cargo run --release -p apost3dview
```

The executable is then `target/release/apost3dview` (`.exe` on Windows). Files named on the command line open at startup.

## Tests

```bash
cargo test --workspace
```

- The tests that check APOST-3D's reference results need them copied from a local APOST-3D checkout first: `scripts/sync-apost3d-fixtures.sh path/to/APOST3D`. Without them, those tests skip themselves.
- The app tests (`-p apost3dview`) render with the real GPU pipelines, so they need a GPU (or a software one). The core and render tests don't.
- `crates/app/tests/input_runs.rs` runs inputs made with the input generator through APOST-3D itself and opens their results; it needs an APOST-3D checkout with the program built, `APOST3D_DIR=path/to/APOST3D cargo test -p apost3dview --test input_runs`, and skips itself otherwise (so it doesn't run in CI).
- Reference scenes (pixel-by-pixel comparisons before and after a renderer change): see the comments at the top of `crates/app/tests/render_reference.rs`.

## Test data

- `validation/`: committed. Calculations from Gaussian, PySCF, Q-Chem and ORCA with the programs' own orbital cubes, one folder per program with a README; `crates/core/tests/reference_cubes.rs` checks APOST3Dview's orbitals against them.
- `examples/`: committed. Five complete APOST-3D runs for new users (see its README), also opened by the app's tests (`crates/app/tests/examples.rs`, `orbital_sets.rs`). Refreshed from `fixtures/` with `scripts/update-examples.sh` when APOST-3D's results change.
- `fixtures/`: not committed. `fixtures/apost3d/` and `fixtures/apost3d-orbitals/` hold APOST-3D's reference results, copied by `scripts/sync-apost3d-fixtures.sh`. Unpublished test systems go in their own subfolder here (for example `fixtures/bi-dianion-osd/`); tests and reference scenes that need one skip themselves without it.

## Developer tools

Small programs kept in the crates' `examples/` folders, run with `cargo run --release -p <crate> --example <name> -- <arguments>`. They aren't part of `cargo test`.

In `apost3dview-core`:

- `parse_fchk`, `parse_wavefunction`, `parse_molden`, `parse_cube`: print what APOST3Dview reads from a file.
- `compare_cube`, `compare_cube_molden`: one orbital of a `.fchk` or `.molden` against a cube file of it (RMS and largest difference); see `validation/README.md`.
- `time_grids`: timings of orbitals, densities, isosurfaces and envelopes on a large molecule (needs `fixtures/`).

In `apost3dview-render`, checks of the rendering, most of them on a real GPU device, each ending in "ALL CHECKS PASSED" (pictures are written to the current folder, or to the folder in `AO_TEST_OUT_DIR`, `DOF_TEST_OUT_DIR` or `TURNTABLE_TEST_OUT_DIR`):

- `test_export`, `test_glyph_atlas`, `test_label_layout`, `test_isosurface`: offscreen PNG export, the label font atlas and layout, the isosurface pipeline.
- `test_ao`, `test_ao_live`, `test_dof`, `test_dof_live`: ambient occlusion and depth of field, export and live paths (the depth-of-field ones on a large molecule, from `fixtures/`, or a `.fchk` given as argument).
- `test_turntable_gif`: GIF export, decoded back and checked for a seamless loop.
- `test_real_cube_render <file.cube>`: a cube file's isosurface, rendered.
- `test_picking <file.fchk>`, `test_occlusion <file.fchk>`: atom and bond picking, and which atoms are visible; meant for a large molecule, to compare the counts with earlier runs.

## Packages and releases

`.github/workflows/release.yml` builds the downloadable packages:

- `scripts/package-macos.sh`: universal `APOST3Dview.app` (Apple Silicon + Intel), ad-hoc signed, in a `.dmg`. Runs the same on a Mac by hand.
- `scripts/package-linux.sh`: a `.tar.gz` with the executable, its menu entry and `install.sh`.
- Windows: the `.exe` in a `.zip`, built in the workflow itself.

Starting the workflow by hand (Actions, Release, Run workflow) only builds; the packages are under the run's Artifacts. To publish a release:

1. Set the version in the root `Cargo.toml` (`[workspace.package] version`) and `CITATION.cff`, commit.
2. Tag and push: `git tag v0.9.0 && git push origin v0.9.0`. The workflow refuses a tag that doesn't match `Cargo.toml`.
3. When the workflow is done, a **draft** release with the three packages and their checksums is waiting under Releases: check the notes, then Publish.
