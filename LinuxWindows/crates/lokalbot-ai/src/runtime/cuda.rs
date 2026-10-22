//! Optional CUDA 13.4 runtime. The NVIDIA driver stays system-managed; the
//! math libraries are reused from the system when the actual backend loads.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use super::{LlamaBackend, RuntimeAsset};

#[cfg(windows)]
pub(super) const ASSET: RuntimeAsset = RuntimeAsset {
    backend: LlamaBackend::Cuda,
    tag: "win-cuda-13.4-x64",
    name: "llama-b11669-bin-win-cuda-13.4-x64.zip",
    size: 154_582_227,
    sha256: "3b92d5c064f5032e603e531d3ec8961f3bd6613e5dfad047399a92dec8e95dbc",
};

#[cfg(not(windows))]
pub(super) const ASSET: RuntimeAsset = RuntimeAsset {
    backend: LlamaBackend::Cuda,
    tag: "ubuntu-cuda-13.4-x64",
    name: "llama-b11669-bin-ubuntu-cuda-13.4-x64.tar.gz",
    size: 153_679_828,
    sha256: "80878473d27278dbe9c38fb7e704f292a51e889d3c28a1e863137cdee1e2ba4e",
};

// RuntimeAsset also describes this companion archive; it is never selected
// independently as an executable runtime.
#[cfg(windows)]
pub(super) const LIBRARIES: RuntimeAsset = RuntimeAsset {
    backend: LlamaBackend::Cuda,
    tag: "win-cuda-13.4-x64",
    name: "cudart-llama-bin-win-cuda-13.4-x64.zip",
    size: 423_535_356,
    sha256: "738f8c251ac22b70c3ae6f83a10cf222725df0395246a2cf58f32bdb85fbe668",
};

#[cfg(not(windows))]
pub(super) const LIBRARIES: RuntimeAsset = RuntimeAsset {
    backend: LlamaBackend::Cuda,
    tag: "ubuntu-cuda-13.4-x64",
    name: "cudart-llama-b11669-bin-ubuntu-cuda-13.4-x64.tar.gz",
    size: 440_236_547,
    sha256: "1108bd9300a281e8eabb18d3fa028d4240ecd983f9f90dbbbc3297671016d697",
};

#[cfg(windows)]
const LIBRARY_NAMES: [&str; 3] = ["cudart64_13.dll", "cublas64_13.dll", "cublasLt64_13.dll"];
#[cfg(not(windows))]
const LIBRARY_NAMES: [&str; 3] = ["libcudart.so.13", "libcublas.so.13", "libcublasLt.so.13"];

pub(super) fn is_runtime_dir(dir: &Path) -> bool {
    dir.join(if cfg!(windows) {
        "ggml-cuda.dll"
    } else {
        "libggml-cuda.so"
    })
    .is_file()
}

pub(super) fn complete(dir: &Path) -> bool {
    LIBRARY_NAMES.iter().all(|name| {
        dir.join(name)
            .metadata()
            .is_ok_and(|m| m.is_file() && m.len() > 0)
    })
}

/// Both upstream archives use their own layout. Move only the three pinned
/// runtime libraries next to llama-server, never a driver or a system file.
pub(super) fn place_libraries(extracted: &Path, server_dir: &Path) -> Result<()> {
    let files: Vec<_> = LIBRARY_NAMES
        .iter()
        .map(|name| {
            super::find_file(extracted, name, 4)
                .with_context(|| format!("the CUDA runtime archive has no {name}"))
        })
        .collect::<Result<_>>()?;
    for (source, name) in files.iter().zip(LIBRARY_NAMES) {
        std::fs::rename(source, server_dir.join(name))?;
    }
    Ok(())
}

/// Check the driver before fetching hundreds of MB. No CUDA toolkit is
/// loaded here. The bounded llama-server device probe checks GPU capability.
pub(super) fn require_driver() -> Result<()> {
    #[cfg(windows)]
    let driver =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32/nvcuda.dll");
    #[cfg(not(windows))]
    let driver = PathBuf::from("libcuda.so.1");
    // SAFETY: load the installed CUDA driver and call its documented version
    // query, retaining the library until the function pointer is no longer used.
    let version = unsafe {
        let library = libloading::Library::new(&driver)
            .context("CUDA needs an NVIDIA driver. Install a current NVIDIA driver or choose Auto, Vulkan or CPU in Settings → Models.")?;
        let query: libloading::Symbol<'_, unsafe extern "system" fn(*mut i32) -> i32> =
            library.get(b"cuDriverGetVersion\0")?;
        let mut version = 0;
        if query(&mut version) != 0 {
            bail!(
                "The NVIDIA driver could not report its CUDA version. Choose Vulkan or CPU, or update the driver."
            );
        }
        version
    };
    validate_driver_version(version)
}

fn validate_driver_version(version: i32) -> Result<()> {
    // NVIDIA supports minor compatibility within CUDA 13.x (R580+). New PTX
    // or driver features can still fail, so device and model execution must pass.
    // https://docs.nvidia.com/deploy/cuda-compatibility/minor-version-compatibility.html
    if version < 13_000 {
        bail!(
            "This runtime needs an NVIDIA driver supporting CUDA 13.0 or newer (driver reports {}.{}). Update the driver or choose Auto, Vulkan or CPU.",
            version / 1000,
            (version % 1000) / 10
        );
    }
    Ok(())
}

/// Use normal system loader paths plus explicit toolkit locations. Never
/// search another application's private runtime. A real device probe, not
/// file presence, decides whether these libraries are usable.
pub(super) fn library_dirs() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["CUDA_PATH_V13_4", "CUDA_PATH", "CUDA_HOME"]
        .iter()
        .filter_map(|key| {
            std::env::var_os(key)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .collect();
    #[cfg(target_os = "linux")]
    roots.extend([
        PathBuf::from("/usr/local/cuda-13.4"),
        PathBuf::from("/usr/local/cuda"),
    ]);
    let mut dirs = Vec::new();
    for root in roots.drain(..) {
        #[cfg(windows)]
        let suffixes = ["bin/x64", "bin"];
        #[cfg(not(windows))]
        let suffixes = ["lib64", "targets/x86_64-linux/lib"];
        for suffix in suffixes {
            let path = root.join(suffix);
            if path.is_dir() && !dirs.contains(&path) {
                dirs.push(path);
            }
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_accepts_cuda_13_minor_compatibility() {
        assert!(validate_driver_version(13_000).is_ok());
        assert!(validate_driver_version(13_020).is_ok());
        assert!(validate_driver_version(12_080).is_err());
        assert!(validate_driver_version(13_040).is_ok());
        assert!(validate_driver_version(14_000).is_ok());
    }

    #[test]
    fn companion_layout_is_flattened_beside_the_server() {
        let root = tempfile::tempdir().unwrap();
        let extracted = root.path().join("extracted");
        let nested = extracted.join("cudart-upstream-root");
        let server_dir = root.path().join("llama-upstream-root");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&server_dir).unwrap();
        for name in LIBRARY_NAMES {
            std::fs::write(nested.join(name), b"test-library").unwrap();
        }
        place_libraries(&extracted, &server_dir).unwrap();
        assert!(complete(&server_dir));
        assert!(LIBRARY_NAMES.iter().all(|name| !nested.join(name).exists()));
        std::fs::remove_file(server_dir.join(LIBRARY_NAMES[0])).unwrap();
        assert!(!complete(&server_dir));
    }

    #[test]
    fn incomplete_companion_does_not_install_any_libraries() {
        let root = tempfile::tempdir().unwrap();
        let extracted = root.path().join("extracted");
        let server_dir = root.path().join("server");
        std::fs::create_dir_all(&extracted).unwrap();
        std::fs::create_dir_all(&server_dir).unwrap();
        std::fs::write(extracted.join(LIBRARY_NAMES[0]), b"partial").unwrap();
        assert!(place_libraries(&extracted, &server_dir).is_err());
        assert_eq!(std::fs::read_dir(server_dir).unwrap().count(), 0);
    }
}
