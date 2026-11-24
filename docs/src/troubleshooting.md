# Troubleshooting

## It doesn't start

APOST-3Dview writes what happens at every start to a log file, replaced at each start:

| System | Log file |
|---|---|
| Windows | `%APPDATA%\APOST3Dview\data\apost3dview.log` |
| macOS | `~/Library/Application Support/APOST3Dview/apost3dview.log` |
| Linux | `~/.local/share/apost3dview/apost3dview.log` |

When it stops with an error, a message says so. Please attach the log file when you [report the problem](help.md).

APOST-3Dview draws with the graphics card, through DirectX 12 on Windows, Metal on macOS and Vulkan on Linux. To try another graphics interface, start it from a terminal with the variable `WGPU_BACKEND` set. On Windows, in a Command Prompt in its folder:

    set WGPU_BACKEND=vulkan
    APOST-3Dview.exe

OpenGL (`gl`) isn't enough yet: APOST-3Dview says so and stops.

## Linux: "APOST-3Dview needs Vulkan"

On Linux, APOST-3Dview needs a Vulkan driver for the graphics card. Without one it says so at the start and stops. Install the driver: Mesa's for Intel and AMD cards (the package `mesa-vulkan-drivers` on Ubuntu, Debian, Fedora, RHEL and Rocky Linux), or NVIDIA's own driver. `vulkaninfo --summary` (package `vulkan-tools`) shows whether Vulkan works.

A remote session (`ssh -X`, most remote desktops) usually has no Vulkan, even when the remote computer has a graphics card: run APOST-3Dview on the computer in front of you, and copy the files there.

## Linux: "version `GLIBC_2.xx' not found"

Downloads up to version 1.0.1 needed a recent Linux (glibc 2.35, Ubuntu 22.04 or newer). Download the latest version, which runs on any Linux with glibc 2.17 or later (`ldd --version` tells which glibc a system has).

## Windows or macOS won't open it

The downloads aren't signed with a paid certificate, so the first start needs one extra step: see [Install and first start](install.md).

## APOST-3D results are refused

APOST-3Dview checks that results belong to the `.fchk` they are opened with (the same atoms at the same positions, the same number of basis functions and the same SCF energy). The red **Results not loaded** box in the **APOST-3D** window says what differs. The usual causes: the `.fchk` was replaced after APOST-3D ran, or the results are from another calculation of the same molecule (another functional or basis set). Open the `.fchk` APOST-3D actually read, or run APOST-3D again.

An orbital file (EFOs, OSLOs) of another run is refused the same way, with the reason in the **Surfaces** window: run APOST-3D again to write it anew.

## An orbital looks cut flat on one side

The grid an orbital is computed on reaches 4 Bohr beyond the outermost atoms by default. A diffuse orbital (often a high virtual one, or at a small isovalue) can reach further and is then cut at the edge of the grid; the **Surfaces** window says so under the surface's name. Raise **Box padding (Bohr)** and generate it again. A cube file has its own box: write it with a larger one, or raise the isovalue.

## An orbital looks faceted or rough

Choose a finer grid (**High**) and generate it again, or raise **Refinement:** in its settings.

## A label is missing

Labels never overlap: in a crowded view, a label that would cover another, or that belongs to an atom hidden behind others, is left out. Turn or zoom the view to bring it back. Every value is also in the **APOST-3D** window's tables.

## The windows are in the way

Collapse a window with its triangle, or close it with its **×**; **Arrange windows** in the toolbar puts every window back in the column at its default size.
