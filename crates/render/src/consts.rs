/// Shared between the app crate's `NativeOptions` (which must request a
/// matching depth buffer and MSAA sample count from eframe) and this
/// crate's pipeline creation — both sides render into the same egui-wgpu
/// pass, so their configs must match exactly or wgpu will reject the
/// pipeline at draw time.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const DEPTH_BUFFER_BITS: u8 = 32;
pub const MSAA_SAMPLES: u32 = 4;
