# APOST3Dview — Project Roadmap

Working design document for a cross-platform (macOS + Windows) molecular
visualizer, aimed at reading `.fchk` / `.cube` files now, and APOST-3D's own
`.apost` output later. Captures the architecture decisions made during
brainstorming (Aug 2026) so we don't relitigate them every session.

Reference bar for visual quality: CYLview20, IBOview. Reference bar for
functional flexibility/scriptability: Chemcraft, VMD — but easier to use than
either.

---

## Decisions made so far

- **Two target platforms**: macOS and Windows. No Linux requirement (may
  revisit later — doesn't cost us much either way given the tech choice below). If necessary Windows can be done later.
- **Real native desktop app, not a browser/webview wrapper.** No Electron,
  no Tauri-as-webview. The whole point is GPU-native rendering quality on
  both platforms from one codebase.
- **Rendering/graphics core: Rust + [wgpu](https://wgpu.rs/).** wgpu is a
  native Rust implementation of the WebGPU standard — it compiles to real
  Metal calls on macOS and real DirectX12/Vulkan calls on Windows from the
  same Rust + WGSL shader code. It's the same tech that powers WebGPU in
  Firefox, so it's a mature dependency, not an experiment.
- **UI chrome: [egui](https://github.com/emgu/egui)/`eframe`**, paired with
  wgpu via `egui-wgpu`. Immediate-mode, fast to iterate, and every rendering
  parameter (material sliders, style toggles) becomes a UI control almost for
  free — this is where we get CYLview-style flexibility without fighting a
  black-box settings system.
- **Scientific computation: Python, called from Rust.** Start simple —
  Rust shells out to a local Python script/process for anything that needs
  the scientific Python ecosystem (grid-based orbital/density evaluation,
  eventually maybe analytic integrals). Only reach for tighter FFI (PyO3) if
  the subprocess approach becomes a real performance bottleneck. Candidate
  libraries when we get there: **ORBKIT** (evaluates MOs/densities on a grid
  directly from `.fchk`, molden, etc. — this is what saves us from writing
  our own basis-function evaluator), **IOData** / **cclib** (fchk parsing
  references, cube I/O).
  **Phase 1 needs none of this** — atomic numbers and Cartesian coordinates
  in a `.fchk` are a plain text block; Rust parses that directly.
- **Rendering technique for visual quality**: raymarched screen-space
  impostors (signed-distance-field spheres/cylinders in the fragment shader)
  rather than polygon meshes — this is what makes
  [Speck](https://github.com/wwwtyro/speck) (a small open-source WebGL
  molecule renderer) look as good as it does: pixel-perfect smooth atoms at
  any zoom, ambient occlusion, soft depth. Speck's code is public and usable
  as a reference for the shader math even though we're porting the technique
  to WGSL, not reusing its code directly (it's a browser/WebGL library).
- **Dev tool for the actual build: Claude Code + VS Code**, not Cowork. The
  reason is specific to this project: visual quality is the top priority,
  and judging that requires a real GPU and a real display to actually look
  at renders — Cowork's sandbox is headless. Cowork stays useful for
  research, docs, and non-visual planning (like this file).

## Open questions (revisit as they come up)

- egui vs. Slint for the long-term UI polish — starting with egui for
  iteration speed, may reconsider once the app has real shape.
- Subprocess vs. PyO3 for the Rust↔Python bridge — deferred until Phase 3
  actually needs Python.
- Whether to eventually expose a local HTTP server mode (view remotely from
  the cluster) — not designed against yet, but wgpu + a Rust web crate
  (e.g. `axum`) wouldn't be a large detour if we want it later.

---

## Phased plan

### Phase 1 — Geometry, styled beautifully (current focus)

Goal: open a real `.fchk`, see atoms + bonds rendered at a quality that holds
up next to CYLview/IBOview, before any orbital/analysis features exist.

- Parse `.fchk`: atomic numbers + Cartesian coordinates section (plain Rust
  text parsing, no external library needed).
- Bond perception from covalent radii + distance cutoff (standard approach,
  same as every other viewer).
- Render atoms as raymarched sphere impostors, bonds as cylinder impostors
  (Speck-derived technique, ported to WGSL).
- Camera: orbit / pan / zoom.
- Material/lighting panel in egui, live-bound to shader uniforms — ambient,
  diffuse, specular, shininess (mirrors CYLview's Styles panel), plus
  lighting angle and background.
- **"View coordinates" window**: a button that opens a secondary panel
  showing the plain-text XYZ block for the current structure (matches
  Chemcraft's equivalent feature) — selectable/copyable text.
- Distance/angle/dihedral measurement by clicking atoms, with on-screen
  labels (matches the "2.34" / "2.46" style annotations in CYLview).
- Export to high-resolution PNG (transparent background option).

**Explicitly not in scope for Phase 1**: cube files, orbitals, `.apost`,
Python bridge, toon/quadrant shading effects, batch file browsing.

### Phase 2 — `.cube` files and orbital/density isosurfaces

- Parse Gaussian `.cube` format (grid + scalar field).
- Isosurface extraction/rendering (marching cubes or raymarched volume,
  TBD) with alpha blending for the translucent lobe look, positive/negative
  phase coloring.
- Style controls: isovalue, opacity, two-color phase scheme.

### Phase 3 — Generate cubes directly from `.fchk` (skip external cubegen)

- Rust↔Python bridge comes in here. Use ORBKIT (or a custom grid evaluator
  if ORBKIT's license/maintenance state doesn't work out) to evaluate MOs/
  densities on a grid straight from the `.fchk`'s basis set + MO
  coefficients, no external `cubegen`/Multiwfn/APOST-3D step required.

### Phase 4 — `.apost` integration (the specialized, no-one-else-has-this part)

- Parse APOST-3D output. Likely means adding a structured sidecar output
  format to APOST-3D itself (JSON alongside the human-readable `.apost`
  report) rather than regex-scraping the text report — revisit with the
  Fortran side once we're here.
- Fragment definitions, EOS/IQA/QTAIM values, bond orders as visual overlays:
  per-atom/fragment coloring and numeric annotations positioned in 3D space
  (billboarded text, like the orbital-population figure you shared), bond
  rendering weighted by bond order, QTAIM basin surfaces.

---

## Getting started in Claude Code

Once Claude Code is set up (see chat for install steps), a reasonable first
prompt in this repo:

> Scaffold a Rust cargo workspace for a desktop app using eframe + wgpu.
> I want a window that opens, an empty 3D viewport with an orbit camera,
> and an egui side panel with placeholder sliders for ambient/diffuse/
> specular/shininess. Once that compiles and runs, we'll add `.fchk`
> parsing next.

Keep phase 1's scope narrow (see above) until the rendering quality itself
feels right — that's the whole point of sequencing it first.
