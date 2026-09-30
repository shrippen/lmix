//! GPU profile: which vision mixer backend fits the engine's GPU.
//!
//! ```text
//!   strom WS SystemStats.gl_renderer ─► Vendor ─► MixerBackend ─► flow property
//!        "NVIDIA Corporation"          Nvidia     Cuda            compositor_preference=cuda
//!        "AMD" / "Mesa (radeonsi)"     Amd        Gl  (unverified) =gpu
//!        "Intel" / "Mesa (iris)"       Intel      Gl  (unverified) =gpu
//!        llvmpipe / none               -          error, never a silent CPU fallback
//! ```
//!
//! NVIDIA gets `cudacompositor`: NVDEC frames stay in CUDA memory up to
//! NVENC. Measured on an RTX 3090 with 16 x 1080p30 H.264 inputs: 6 % CPU
//! vs. 108 % for NVDEC -> GL -> `glvideomixer` (see docs/DECISIONS.md).
//! AMD and Intel use the GL mixer (VA-API -> DMABuf -> GL); no hardware has
//! been available to verify that path yet.

use strom_types::GlRendererInfo;

/// GPU vendor, as far as the mixer backend choice cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
}

/// Vision mixer compositor backend (`compositor_preference` property).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixerBackend {
    Cuda,
    Gl,
    Cpu,
}

/// What the operator asked for on the command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum BackendChoice {
    /// Pick from the engine's GPU; fail if it has none.
    Auto,
    Cuda,
    Gl,
    /// Software compositor. Only on explicit request.
    Cpu,
}

/// Software GL renderers: present, but slower than the CPU compositor.
const SOFTWARE_RENDERERS: [&str; 3] = ["llvmpipe", "softpipe", "swrast"];

impl MixerBackend {
    /// Value of the vision mixer's `compositor_preference` property.
    pub fn property(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Gl => "gpu",
            Self::Cpu => "cpu",
        }
    }

    /// Fastest backend per vendor.
    pub fn for_vendor(vendor: Vendor) -> Self {
        match vendor {
            Vendor::Nvidia => Self::Cuda,
            // unverified: no AMD/Intel hardware tested yet (roadmap M5b).
            Vendor::Amd | Vendor::Intel => Self::Gl,
        }
    }

    /// Resolve the operator's choice against the engine's GPU.
    pub fn resolve(choice: BackendChoice, gl: Option<&GlRendererInfo>) -> Result<Self, String> {
        match choice {
            BackendChoice::Cuda => return Ok(Self::Cuda),
            BackendChoice::Gl => return Ok(Self::Gl),
            BackendChoice::Cpu => return Ok(Self::Cpu),
            BackendChoice::Auto => {}
        }

        let Some(info) = gl else {
            return Err("engine reports no hardware GL; pass --mixer-backend cpu to force the CPU compositor".into());
        };
        match detect_vendor(info) {
            Some(vendor) => Ok(Self::for_vendor(vendor)),
            None => Err(format!(
                "unsupported GPU '{}' ({}); pass --mixer-backend to choose explicitly",
                info.renderer, info.vendor
            )),
        }
    }
}

/// Vendor from the GL vendor/renderer strings; `None` for software GL.
pub fn detect_vendor(info: &GlRendererInfo) -> Option<Vendor> {
    let text = format!("{} {}", info.vendor, info.renderer).to_lowercase();
    if SOFTWARE_RENDERERS.iter().any(|s| text.contains(s)) {
        return None;
    }

    // Mesa reports vendor "Mesa" / "AMD" / "Intel" and names the driver in the renderer.
    if text.contains("nvidia") {
        return Some(Vendor::Nvidia);
    }
    if text.contains("amd") || text.contains("radeon") || text.contains("ati technologies") {
        return Some(Vendor::Amd);
    }
    if text.contains("intel") {
        return Some(Vendor::Intel);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gl(vendor: &str, renderer: &str) -> GlRendererInfo {
        GlRendererInfo {
            renderer: renderer.into(),
            version: String::new(),
            vendor: vendor.into(),
            glsl_version: String::new(),
        }
    }

    #[test]
    fn detects_vendors() {
        let nvidia = gl("NVIDIA Corporation", "NVIDIA GeForce RTX 3090/PCIe/SSE2");
        let amd = gl("AMD", "AMD Radeon RX 7900 XTX (radeonsi, navi31, LLVM 17.0.6, DRM 3.57)");
        let intel = gl("Intel", "Mesa Intel(R) Arc(tm) A770 Graphics (DG2)");
        let llvmpipe = gl("Mesa", "llvmpipe (LLVM 17.0.6, 256 bits)");

        assert_eq!(detect_vendor(&nvidia), Some(Vendor::Nvidia));
        assert_eq!(detect_vendor(&amd), Some(Vendor::Amd));
        assert_eq!(detect_vendor(&intel), Some(Vendor::Intel));
        assert_eq!(detect_vendor(&llvmpipe), None);
    }

    #[test]
    fn auto_picks_fastest_backend_per_vendor() {
        let nvidia = gl("NVIDIA Corporation", "NVIDIA GeForce RTX 3090/PCIe/SSE2");
        let intel = gl("Intel", "Mesa Intel(R) UHD Graphics 770 (ADL-S GT1)");

        assert_eq!(MixerBackend::resolve(BackendChoice::Auto, Some(&nvidia)), Ok(MixerBackend::Cuda));
        assert_eq!(MixerBackend::resolve(BackendChoice::Auto, Some(&intel)), Ok(MixerBackend::Gl));
    }

    #[test]
    fn auto_never_falls_back_to_cpu() {
        let llvmpipe = gl("Mesa", "llvmpipe (LLVM 17.0.6, 256 bits)");

        assert!(MixerBackend::resolve(BackendChoice::Auto, None).is_err());
        assert!(MixerBackend::resolve(BackendChoice::Auto, Some(&llvmpipe)).is_err());
        assert_eq!(MixerBackend::resolve(BackendChoice::Cpu, None), Ok(MixerBackend::Cpu));
    }
}
