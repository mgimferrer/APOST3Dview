//! What the renderer needs from the graphics driver, and what to tell the
//! user when it isn't there. Shared by the window (`main.rs`) and
//! `apost3dview --render`.
//!
//! The renderer needs Vulkan, Metal or DirectX 12. OpenGL isn't enough yet:
//! the ambient occlusion shaders read depth textures with `textureLoad`,
//! which wgpu can't translate to GLSL (found on Rocky Linux 8 without a
//! Vulkan driver, 2026-11-24: wgpu fell back to OpenGL and the app stopped
//! while building its pipelines). So OpenGL is left out of the default
//! choice, and an OpenGL adapter asked for by hand is refused with a
//! message, not a crash.

/// The graphics APIs wgpu may use: `WGPU_BACKEND` when set (`dx12`,
/// `vulkan`, `gl`, `metal`), otherwise DX12 alone on Windows and Vulkan or
/// Metal elsewhere. On Windows, eframe's default also tries Vulkan and
/// OpenGL, and on a laptop with AMD + NVIDIA graphics (2026-10-21) asking
/// those for their adapters never returned: the app hung before opening
/// its window. DX12 is always there on Windows 10/11 (with Microsoft's
/// software renderer as a last resort), and it is what CI tests.
pub fn backends() -> wgpu::Backends {
    wgpu::Backends::from_env().unwrap_or(if cfg!(windows) { wgpu::Backends::DX12 } else { wgpu::Backends::PRIMARY })
}

/// Why an adapter can't draw APOST-3Dview's pictures, if it can't.
pub fn unsupported_adapter(info: &wgpu::AdapterInfo) -> Option<String> {
    unsupported(info.backend, &info.name)
}

fn unsupported(backend: wgpu::Backend, name: &str) -> Option<String> {
    (backend == wgpu::Backend::Gl).then(|| {
        format!(
            "The only graphics found here is OpenGL ({name}), and APOST-3Dview needs {}.\n\n{}",
            needed_api(),
            how_to_get_it()
        )
    })
}

/// What to say when no graphics adapter is found at all.
pub fn no_adapter_message() -> String {
    format!("APOST-3Dview needs {}, and no graphics driver for it was found on this computer.\n\n{}", needed_api(), how_to_get_it())
}

fn needed_api() -> &'static str {
    if cfg!(windows) {
        "DirectX 12"
    } else if cfg!(target_os = "macos") {
        "Metal"
    } else {
        "Vulkan"
    }
}

fn how_to_get_it() -> &'static str {
    if cfg!(windows) {
        "Update the graphics driver from your graphics card maker (Intel, AMD or NVIDIA)."
    } else if cfg!(target_os = "macos") {
        "Every Mac since 2012 has Metal: please report this."
    } else {
        "Install the Vulkan driver for your graphics card: Mesa's for Intel and AMD cards \
         (package mesa-vulkan-drivers on Ubuntu, Debian, Fedora, RHEL and Rocky Linux), or NVIDIA's own driver. \
         `vulkaninfo --summary` (package vulkan-tools) shows whether Vulkan works. \
         A remote session (ssh -X, most remote desktops) usually has no Vulkan: run APOST-3Dview on the computer in front of you."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opengl_is_refused_with_a_reason() {
        let message = unsupported(wgpu::Backend::Gl, "llvmpipe").expect("OpenGL refused");
        assert!(message.contains("OpenGL (llvmpipe)"), "{message}");
        for backend in [wgpu::Backend::Vulkan, wgpu::Backend::Metal, wgpu::Backend::Dx12] {
            assert!(unsupported(backend, "a card").is_none());
        }
    }

    #[test]
    fn opengl_is_not_tried_by_default() {
        if std::env::var_os("WGPU_BACKEND").is_none() {
            assert!(!backends().contains(wgpu::Backends::GL));
        }
    }
}
