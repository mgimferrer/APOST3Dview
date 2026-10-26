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

- The tests that check APOST-3D's reference results need them copied into `fixtures/` first: `scripts/sync-apost3d-fixtures.sh --pinned` fetches them from GitHub at the APOST-3D commit CI tests against (`scripts/apost3d-commit`), or `scripts/sync-apost3d-fixtures.sh path/to/APOST3D` copies them from a local APOST-3D checkout. Without them, those tests skip themselves. CI runs them on every push; to move to newer APOST-3D results, put the new commit in `scripts/apost3d-commit`.
- The app tests (`-p apost3dview`) render with the real GPU pipelines, so they need a GPU (or a software one). The core and render tests don't.
- `crates/app/tests/input_runs.rs` runs inputs made with the input generator through APOST-3D itself and opens their results; it needs an APOST-3D checkout with the program built, `APOST3D_DIR=path/to/APOST3D cargo test -p apost3dview --test input_runs`, and skips itself otherwise (so it doesn't run in CI).
- Reference scenes (pixel-by-pixel comparisons before and after a renderer change): see the comments at the top of `crates/app/tests/render_reference.rs`.

## Test data

- `validation/`: committed. Calculations from Gaussian, PySCF, Q-Chem and ORCA with the programs' own orbital cubes, one folder per program with a README; `crates/core/tests/reference_cubes.rs` checks APOST3Dview's orbitals against them. `validation/apost3d/` holds APOST-3D's atomic weights at reference points, checked by `crates/core/tests/atomic_weights.rs`.
- `examples/`: committed. Eight complete APOST-3D runs for new users (see its README), also opened by the app's tests (`crates/app/tests/examples.rs`, `orbital_sets.rs`). Refreshed from `fixtures/` with `scripts/update-examples.sh` when APOST-3D's results change.
- `fixtures/`: not committed. `fixtures/apost3d/` and `fixtures/apost3d-orbitals/` hold APOST-3D's reference results, copied by `scripts/sync-apost3d-fixtures.sh`. Unpublished test systems go in their own subfolder here (for example `fixtures/bi-dianion-osd/`); tests and reference scenes that need one skip themselves without it.

## The user guide

The guide's source is in `docs/` ([mdBook](https://rust-lang.github.io/mdBook/), `cargo install mdbook`): one Markdown page per chapter in `docs/src/`, listed in `docs/src/SUMMARY.md`. `mdbook serve docs` shows it in the browser while editing; `mdbook build docs` writes it to `docs/book/`. The Guide workflow builds it on every push and publishes it with GitHub Pages from `main`.

- Write the labels of the app's buttons, windows and menus in **bold**, exactly as the app shows them, and nothing else in bold; menu entries that come from a results file in *italics*; keys as `<kbd>Esc</kbd>`. No en or em dashes.
- `crates/app/tests/guide_text.rs` checks that every bold label exists in the app's code, and that every picture and link resolves (no GPU needed).
- `crates/app/tests/guide.rs` follows each chapter's steps in the real app and checks what the guide says, so a renamed button or a changed default fails the tests.
- The pictures in `docs/src/images/` are made by the same test: `scripts/guide-images.sh` takes them all again and builds the guide. Run it after changing a window or the look, check `git diff`, and commit the pictures. Pictures of a single window are included with `<img class="window" src="...">`, so they show at half their pixel size.

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

Every package also holds `examples/` and the guide (`guide/`, built by the workflow's guide job; to run the packaging scripts by hand, run `mdbook build docs` first). Starting the workflow by hand (Actions, Release, Run workflow) only builds; the packages are under the run's Artifacts. To publish a release:

1. Set the version in the root `Cargo.toml` (`[workspace.package] version`) and `CITATION.cff`, commit.
2. Tag and push: `git tag v0.9.0 && git push origin v0.9.0`. The workflow refuses a tag that doesn't match `Cargo.toml`.
3. When the workflow is done, a **draft** release with the three packages and their checksums is waiting under Releases: check the notes, then Publish.
