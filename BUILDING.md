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
- Reference scenes (pixel-by-pixel comparisons before and after a renderer change): see the comments at the top of `crates/app/tests/render_reference.rs`.

## Packages and releases

`.github/workflows/release.yml` builds the downloadable packages:

- `scripts/package-macos.sh`: universal `APOST3Dview.app` (Apple Silicon + Intel), ad-hoc signed, in a `.dmg`. Runs the same on a Mac by hand.
- `scripts/package-linux.sh`: a `.tar.gz` with the executable, its menu entry and `install.sh`.
- Windows: the `.exe` in a `.zip`, built in the workflow itself.

Starting the workflow by hand (Actions, Release, Run workflow) only builds; the packages are under the run's Artifacts. To publish a release:

1. Set the version in the root `Cargo.toml` (`[workspace.package] version`) and `CITATION.cff`, commit.
2. Tag and push: `git tag v0.9.0 && git push origin v0.9.0`. The workflow refuses a tag that doesn't match `Cargo.toml`.
3. When the workflow is done, a **draft** release with the three packages and their checksums is waiting under Releases: check the notes, then Publish.
